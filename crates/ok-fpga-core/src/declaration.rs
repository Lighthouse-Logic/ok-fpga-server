// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Resource Declarations (ADR-034)
//!
//! This module turns a client's resource *declaration* (what a
//! `POST /resources` body or a gRPC `CreateResourceRequest` says) into a core
//! [`Resource`], and checks a new resource against the ones that already
//! exist on the device.
//!
//! REST and gRPC both call this module, so the two APIs apply the same rules
//! (roadmap Q44-5: "REST and gRPC behave the same").
//!
//! ## What is checked here
//!
//! Shape rules (all **422**, sub-phases 44-A and 44-B):
//! - `access_mode` is required when `endpoints` is present (D62).
//! - No `endpoints` field = an exclusive resource that claims every classic
//!   EP; `access_mode` may be omitted, `shared` is rejected (D68).
//! - Each endpoint entry takes its `access` from the entry, else from
//!   `default_access`, else the request is rejected (D61).
//! - Every address range has an `access` and `start <= end`.
//! - EP type matches its address; at least one known interface.
//! - **Rule A**: an exclusive resource may contain only exclusive items (no
//!   shared entry, no shared range, no `default_access: shared`).
//! - Each endpoint address appears at most once in one declaration.
//!
//! Registration rule **R1** ([`check_overlap`], **409**): first registration
//! wins. A new resource may overlap an existing one only where both items
//! are shared items in shared resources with the same endpoint type and the
//! same item-level `private` flag (ranges: both shared, same `private`).
//! A resource without `endpoints` claims every classic EP (today
//! 0x00–0xBF; D77 narrows this to 0x00–0x7F in 44-E).
//!
//! Helpers the server calls around these: [`parse_ep_address`] (strict REST
//! address syntax), [`check_interfaces_for_dataport`] (AXI interfaces need an
//! AXI data port) and [`self_overlapping_ranges`] (allowed, but logged).
//!
//! All errors are [`ResourceError`] values. A `Validation` code means the
//! request is wrong (REST 422, gRPC `INVALID_ARGUMENT`); `Overlap` means it
//! clashes with an existing resource (REST 409, gRPC `FAILED_PRECONDITION`).

use std::collections::{HashMap, HashSet};

use crate::device::DataportType;
use crate::resource::{
    format_ep_address, format_u32_address, AccessMode, AddressRange, ConflictCode,
    ConflictItemType, EndpointDeclaration, EndpointEntry, EndpointType, InterfaceDeclaration,
    Resource, ResourceConflict, ResourceError,
};

// ============================================================================
// Transport-neutral declaration types
// ============================================================================

/// A resource declaration after the transport layer has decoded it.
///
/// Optional fields are `Option` so that "the client did not send it" stays
/// different from "the client sent an empty value" — several rules depend on
/// that difference (D55, D61, D62, D68).
#[derive(Debug, Clone, Default)]
pub struct ResourceSpec {
    /// Resource name; the resource ID becomes `{device_serial}:{name}`.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Serial of the FPGA that provides the resource.
    pub device_serial: String,
    /// Declared mode (`Shared` / `Exclusive`), `None` if omitted.
    pub access_mode: Option<AccessMode>,
    /// Resource-level `private` flag (an access rule, not visibility; D57).
    pub private: bool,
    /// Interface names: `classic`, `axi_lite`, `axi_full`, `axi_stream`.
    pub interfaces: Vec<String>,
    /// `None` = the `endpoints` field was absent (D68).
    pub endpoints: Option<EndpointSpec>,
    /// Address ranges, in the order given.
    pub address_ranges: Vec<AddressRangeSpec>,
    /// Text-only metadata (D69).
    pub metadata: HashMap<String, String>,
}

/// The `endpoints` object of a declaration.
#[derive(Debug, Clone, Default)]
pub struct EndpointSpec {
    /// Fills in a missing `access` on listed entries (D61).
    pub default_access: Option<AccessMode>,
    /// Listed entries; empty = the resource claims no EP (D55).
    pub entries: Vec<EndpointEntrySpec>,
}

/// One endpoint entry of a declaration.
#[derive(Debug, Clone)]
pub struct EndpointEntrySpec {
    /// EP address. `u32` because gRPC sends `uint32`; values above 0xFF are
    /// rejected.
    pub address: u32,
    /// Spec type name, e.g. `wire_in`.
    pub ep_type: String,
    /// Declared mode of this entry, `None` if omitted.
    pub access: Option<AccessMode>,
    /// Item-level `private` flag.
    pub private: bool,
    /// Optional description.
    pub description: Option<String>,
}

/// One address range of a declaration.
#[derive(Debug, Clone)]
pub struct AddressRangeSpec {
    /// Optional range name.
    pub name: Option<String>,
    /// First address (inclusive).
    pub start: u32,
    /// Last address (inclusive).
    pub end: u32,
    /// Declared mode, `None` if omitted (rejected).
    pub access: Option<AccessMode>,
    /// Item-level `private` flag.
    pub private: bool,
    /// Optional description.
    pub description: Option<String>,
}

// ============================================================================
// Parsing helpers shared by REST and gRPC
// ============================================================================

/// Parse a hex address such as `"0x05"` or `"0x44A6_0000"` (the `0x` prefix
/// and `_` separators are optional).
///
/// # Errors
/// A validation [`ResourceError`] naming `field` if the text is not hex or
/// does not fit in 32 bits.
pub fn parse_hex_u32(text: &str, field: &str) -> Result<u32, ResourceError> {
    let stripped = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    let clean: String = stripped.chars().filter(|c| *c != '_').collect();
    u32::from_str_radix(&clean, 16).map_err(|_| {
        ResourceError::validation(format!(
            "{} '{}' is not a 32-bit hex address (expected e.g. 0x44A60000)",
            field, text
        ))
    })
}

/// Parse a Classic endpoint address with the strict OpenAPI pattern
/// `^0x[0-9A-Fa-f]{2}$` (REST only; gRPC sends a number).
///
/// Only lowercase `0x` followed by exactly two hex digits is accepted, e.g.
/// `0x05`. This is stricter than [`parse_hex_u32`] on purpose: `5`, `0x5`,
/// `0x005`, `0X05` and `0x0_5` are all rejected, so every client writes
/// addresses the same way the server reports them.
///
/// # Errors
/// A validation [`ResourceError`] naming `field` if `text` does not match.
pub fn parse_ep_address(text: &str, field: &str) -> Result<u32, ResourceError> {
    let invalid = || {
        ResourceError::validation(format!(
            "{} '{}' must be 0x followed by exactly two hex digits, e.g. 0x05",
            field, text
        ))
    };
    let digits = text.strip_prefix("0x").ok_or_else(invalid)?;
    if digits.len() != 2 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    u32::from_str_radix(digits, 16).map_err(|_| invalid())
}

/// Parse a declared mode string (`shared` / `exclusive`).
///
/// # Errors
/// A validation error naming `field` for any other value, including
/// `read_only` (a holder mode, not a declared mode).
pub fn parse_declared_mode(text: &str, field: &str) -> Result<AccessMode, ResourceError> {
    AccessMode::parse_declared(text).ok_or_else(|| {
        ResourceError::validation(format!(
            "{} must be 'shared' or 'exclusive' (got '{}')",
            field, text
        ))
    })
}

// ============================================================================
// build_resource
// ============================================================================

/// Turn a declaration into a core [`Resource`] in the `Available` state.
///
/// # Errors
/// A `Validation` [`ResourceError`] when the declaration breaks one of the
/// shape rules listed in the module documentation.
pub fn build_resource(spec: ResourceSpec) -> Result<Resource, ResourceError> {
    // Trim once and use the trimmed text everywhere, so the ID never ends up
    // as `SN1: dac` (which would also make the `Location` URL surprising).
    let name = spec.name.trim().to_string();
    let device_serial = spec.device_serial.trim().to_string();
    if name.is_empty() {
        return Err(ResourceError::validation("name is required"));
    }
    if device_serial.is_empty() {
        return Err(ResourceError::validation("device_serial is required"));
    }

    let interfaces = build_interfaces(&spec.interfaces)?;

    // D62 / D68: the presence of `endpoints` decides whether `access_mode`
    // may be omitted.
    let access_mode = match (&spec.endpoints, spec.access_mode) {
        (None, None) | (None, Some(AccessMode::Exclusive)) => AccessMode::Exclusive,
        (None, Some(_)) => {
            return Err(ResourceError::validation(
                "a resource without 'endpoints' claims every classic endpoint and must be \
                 exclusive; omit access_mode or set it to 'exclusive' (D68)",
            ))
        }
        (Some(_), None) => {
            return Err(ResourceError::validation(
                "access_mode is required when 'endpoints' is present (D62)",
            ))
        }
        (Some(_), Some(mode)) => mode,
    };
    if access_mode == AccessMode::ReadOnly {
        // Transport parsers never produce this, but the type allows it.
        return Err(ResourceError::validation(
            "access_mode must be 'shared' or 'exclusive'",
        ));
    }

    let endpoints = match spec.endpoints {
        None => None,
        Some(ep) => Some(build_endpoints(ep)?),
    };

    let mut address_ranges = Vec::with_capacity(spec.address_ranges.len());
    for range in spec.address_ranges {
        address_ranges.push(build_range(range)?);
    }

    // Rule A: an exclusive resource may contain only exclusive items. It is
    // checked here, before registration, so the answer is 422 "whatever
    // exists" and never a 409 that depends on other resources. It runs after
    // the items are built so the error can name the first bad item.
    if access_mode == AccessMode::Exclusive {
        check_rule_a(endpoints.as_ref(), &address_ranges)?;
    }

    let id = format!("{}:{}", device_serial, name);
    let mut resource = Resource::new(id, name, spec.description, device_serial, access_mode);
    resource.interfaces = interfaces;
    resource.endpoints = endpoints;
    resource.address_ranges = address_ranges;
    resource.metadata = spec.metadata;
    resource.private = spec.private;
    Ok(resource)
}

/// Rule A for an exclusive resource: no shared entry, no shared range and no
/// `default_access: shared` (even with no entries, because the spec forbids
/// the declaration itself, not only its effect).
fn check_rule_a(
    endpoints: Option<&EndpointDeclaration>,
    ranges: &[AddressRange],
) -> Result<(), ResourceError> {
    const PREFIX: &str = "an exclusive resource may contain only exclusive items";
    if let Some(ep) = endpoints {
        if ep.default_access == Some(AccessMode::Shared) {
            return Err(ResourceError::validation(format!(
                "{}; endpoints.default_access is 'shared'",
                PREFIX
            )));
        }
        if let Some(entry) = ep.entries.iter().find(|e| e.access == AccessMode::Shared) {
            return Err(ResourceError {
                detail: format!(
                    "{}; endpoint {} is declared shared",
                    PREFIX,
                    format_ep_address(entry.address)
                ),
                conflict: ResourceConflict::new(ConflictCode::Validation)
                    .with_endpoint(entry.address),
            });
        }
    }
    if let Some(range) = ranges.iter().find(|r| r.access == AccessMode::Shared) {
        return Err(ResourceError {
            detail: format!(
                "{}; address range {} is declared shared",
                PREFIX,
                format_range_item(range.start, range.end)
            ),
            conflict: ResourceConflict::new(ConflictCode::Validation)
                .with_range(range.start, range.end),
        });
    }
    Ok(())
}

/// Map interface names to the core flags; at least one is required.
fn build_interfaces(names: &[String]) -> Result<InterfaceDeclaration, ResourceError> {
    let mut decl = InterfaceDeclaration {
        classic: false,
        axi_lite: false,
        axi_full: false,
        axi_stream: false,
    };
    for name in names {
        match name.as_str() {
            "classic" => decl.classic = true,
            "axi_lite" => decl.axi_lite = true,
            "axi_full" => decl.axi_full = true,
            "axi_stream" => decl.axi_stream = true,
            other => {
                return Err(ResourceError::validation(format!(
                    "unknown interface '{}' (expected classic, axi_lite, axi_full or axi_stream)",
                    other
                )))
            }
        }
    }
    if !decl.has_any() {
        return Err(ResourceError::validation(
            "interfaces must list at least one of classic, axi_lite, axi_full, axi_stream",
        ));
    }
    Ok(decl)
}

/// Build the endpoint declaration, applying `default_access` (D61).
fn build_endpoints(spec: EndpointSpec) -> Result<EndpointDeclaration, ResourceError> {
    // `AccessMode` is shared with holder modes, so the type allows
    // `ReadOnly`; a *declared* mode may only be shared or exclusive
    // (ADR-034 §1). The transport parsers already refuse it, but this core
    // function must keep the rule even when called directly.
    if spec.default_access == Some(AccessMode::ReadOnly) {
        return Err(ResourceError::validation(
            "endpoints.default_access must be 'shared' or 'exclusive'",
        ));
    }
    let mut entries = Vec::with_capacity(spec.entries.len());
    // One physical endpoint may be listed only once: two entries for the same
    // address (even a Pipe and a BTPipe) would give it two types or modes.
    let mut seen: HashSet<u8> = HashSet::new();
    for entry in spec.entries {
        let address = u8::try_from(entry.address)
            .ok()
            .filter(|a| *a <= 0xBF)
            .ok_or_else(|| {
                ResourceError::validation(format!(
                    "endpoint address 0x{:X} is outside 0x00-0xBF",
                    entry.address
                ))
            })?;
        let invalid = |detail: String| ResourceError {
            detail,
            conflict: ResourceConflict::new(ConflictCode::Validation).with_endpoint(address),
        };
        if !seen.insert(address) {
            return Err(invalid(format!(
                "endpoint 0x{:02X} is listed more than once in this declaration",
                address
            )));
        }
        let ep_type = EndpointType::parse(&entry.ep_type).ok_or_else(|| {
            invalid(format!(
                "endpoint 0x{:02X}: unknown type '{}' (expected wire_in, wire_out, trigger_in, \
                 trigger_out, pipe_in, pipe_out, bt_pipe_in or bt_pipe_out)",
                address, entry.ep_type
            ))
        })?;
        if !ep_type.is_valid_address(address) {
            let (lo, hi) = ep_type.address_range();
            return Err(invalid(format!(
                "endpoint 0x{:02X} is not in the {} range (0x{:02X}-0x{:02X})",
                address, ep_type, lo, hi
            )));
        }
        let access = entry.access.or(spec.default_access).ok_or_else(|| {
            invalid(format!(
                "endpoint 0x{:02X} has no 'access' and the declaration has no \
                 'default_access' (D61)",
                address
            ))
        })?;
        if access == AccessMode::ReadOnly {
            return Err(invalid(format!(
                "endpoint 0x{:02X}: access must be 'shared' or 'exclusive'",
                address
            )));
        }
        entries.push(EndpointEntry {
            address,
            ep_type,
            access,
            description: entry.description,
            private: entry.private,
        });
    }
    Ok(EndpointDeclaration {
        default_access: spec.default_access,
        entries,
    })
}

/// Build one address range.
fn build_range(spec: AddressRangeSpec) -> Result<AddressRange, ResourceError> {
    let invalid = |detail: String| ResourceError {
        detail,
        conflict: ResourceConflict::new(ConflictCode::Validation).with_range(spec.start, spec.end),
    };
    if spec.start > spec.end {
        return Err(invalid(format!(
            "address range start 0x{:08X} must be <= end 0x{:08X}",
            spec.start, spec.end
        )));
    }
    let access = spec.access.ok_or_else(|| {
        invalid(format!(
            "address range 0x{:08X}-0x{:08X} needs 'access' ('shared' or 'exclusive')",
            spec.start, spec.end
        ))
    })?;
    if access == AccessMode::ReadOnly {
        return Err(invalid(format!(
            "address range 0x{:08X}-0x{:08X}: access must be 'shared' or 'exclusive'",
            spec.start, spec.end
        )));
    }
    Ok(AddressRange {
        name: spec.name,
        description: spec.description,
        start: spec.start,
        end: spec.end,
        access,
        private: spec.private,
    })
}

// ============================================================================
// Helpers the server calls around registration (44-B)
// ============================================================================

/// Return the index pairs `(i, j)` (`i < j`) of address ranges in the
/// **same** resource that overlap each other.
///
/// This is allowed (owner decision Q1, 44-B): the overlap is inside one
/// resource, so it cannot clash with anyone else. The server only logs a
/// warning when `warn_range_self_overlap` is on. Where two such ranges differ
/// in `private`, the more restrictive one (private) applies to the addresses
/// they share; that is enforced in 44-E.
pub fn self_overlapping_ranges(resource: &Resource) -> Vec<(usize, usize)> {
    let ranges = &resource.address_ranges;
    let mut pairs = Vec::new();
    for i in 0..ranges.len() {
        for j in (i + 1)..ranges.len() {
            if ranges[i].overlaps(&ranges[j]) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// Check the declared interfaces against the device's data port (SD-09).
///
/// `classic` is allowed on every device. `axi_lite`, `axi_full` and
/// `axi_stream` need [`DataportType::Axi`]; `Legacy` (and `Unknown`, which
/// public APIs never show) are rejected.
///
/// # Errors
/// A validation [`ResourceError`] naming the first AXI interface listed.
pub fn check_interfaces_for_dataport(
    interfaces: &InterfaceDeclaration,
    dataport: DataportType,
) -> Result<(), ResourceError> {
    if !interfaces.has_axi() || dataport == DataportType::Axi {
        return Ok(());
    }
    let name = if interfaces.axi_lite {
        "axi_lite"
    } else if interfaces.axi_full {
        "axi_full"
    } else {
        "axi_stream"
    };
    Err(ResourceError::validation(format!(
        "interface '{}' needs a device with an AXI data port; this device has a {} data port",
        name, dataport
    )))
}

// ============================================================================
// check_overlap (R1)
// ============================================================================

/// Last endpoint address claimed by a resource without `endpoints` (D77).
///
/// Omitting `endpoints` claims every Wire and Trigger endpoint (`0x00..=0x7F`,
/// 128 endpoints). Pipe and BTPipe endpoints (`0x80..=0xBF`) must be declared
/// explicitly, because the server cannot distinguish a Pipe from a BTPipe at
/// the same address (D77, 44-E).
pub const WILDCARD_EP_LAST: u8 = 0x7F;

/// One endpoint as one resource claims it.
///
/// `ep_type` is `None` for the "no `endpoints` field" wildcard: such a
/// resource claims the address without saying what type it is, so it can
/// never be compatible with anyone.
#[derive(Clone, Copy)]
struct EndpointClaim {
    address: u8,
    ep_type: Option<EndpointType>,
    access: AccessMode,
    private: bool,
}

/// Every endpoint `resource` claims, keyed by address.
///
/// A `BTreeMap` keeps the addresses sorted, so conflict lists come out in
/// ascending order without an extra sort.
fn endpoint_claims(resource: &Resource) -> std::collections::BTreeMap<u8, EndpointClaim> {
    let mut map = std::collections::BTreeMap::new();
    match &resource.endpoints {
        // No `endpoints` field: an exclusive resource that claims them all.
        None => {
            for address in 0x00..=WILDCARD_EP_LAST {
                map.insert(
                    address,
                    EndpointClaim { address, ep_type: None, access: AccessMode::Exclusive, private: false },
                );
            }
        }
        // `{"entries": []}` claims nothing (D55, row 19).
        Some(decl) => {
            for e in &decl.entries {
                map.insert(
                    e.address,
                    EndpointClaim {
                        address: e.address,
                        ep_type: Some(e.ep_type),
                        access: e.access,
                        private: e.private,
                    },
                );
            }
        }
    }
    map
}

/// Both resources are shared (the first half of "shared item in a shared
/// resource on both sides").
fn both_shared(new: &Resource, other: &Resource) -> bool {
    new.access_mode == Some(AccessMode::Shared) && other.access_mode == Some(AccessMode::Shared)
}

/// May two claims on the same endpoint coexist? Only shared items in shared
/// resources with the same type and the same item-level `private` flag.
/// Resource-level `private` is deliberately not compared (owner decision
/// Q3): it controls who may *acquire* a resource, not who owns an item.
fn endpoint_claims_compatible(
    new: &Resource,
    other: &Resource,
    n: &EndpointClaim,
    o: &EndpointClaim,
) -> bool {
    both_shared(new, other)
        && n.access == AccessMode::Shared
        && o.access == AccessMode::Shared
        && n.ep_type.is_some()
        && n.ep_type == o.ep_type
        && n.private == o.private
}

/// May two overlapping ranges coexist? Same rule as endpoints, minus the
/// type (ranges have none).
fn ranges_compatible(new: &Resource, other: &Resource, n: &AddressRange, o: &AddressRange) -> bool {
    both_shared(new, other)
        && n.access == AccessMode::Shared
        && o.access == AccessMode::Shared
        && n.private == o.private
}

/// Human text for the first clashing endpoint, most important reason first.
fn endpoint_detail(new: &Resource, other: &Resource, n: &EndpointClaim, o: &EndpointClaim) -> String {
    let item = format_ep_address(o.address);
    if other.access_mode == Some(AccessMode::Exclusive) {
        format!("Endpoint {} belongs to exclusive resource '{}'", item, other.name)
    } else if o.access == AccessMode::Exclusive {
        format!("Endpoint {} is already declared exclusive by resource '{}'", item, other.name)
    } else if new.access_mode == Some(AccessMode::Exclusive) || n.access == AccessMode::Exclusive {
        format!("Endpoint {} is already declared shared by resource '{}'", item, other.name)
    } else if n.ep_type != o.ep_type {
        match o.ep_type {
            Some(t) => format!(
                "Endpoint {} is declared as {} by resource '{}' (type differs)",
                item, t, other.name
            ),
            None => format!("Endpoint {} is claimed by resource '{}'", item, other.name),
        }
    } else {
        format!(
            "Endpoint {} is already declared shared by resource '{}' (private flag differs)",
            item, other.name
        )
    }
}

/// Human text for the first clashing range, most important reason first.
fn range_detail(new: &Resource, other: &Resource, n: &AddressRange, o: &AddressRange) -> String {
    let item = format_range_item(o.start, o.end);
    if other.access_mode == Some(AccessMode::Exclusive) {
        format!("Address range {} belongs to exclusive resource '{}'", item, other.name)
    } else if o.access == AccessMode::Exclusive {
        format!("Address range {} is already declared exclusive by resource '{}'", item, other.name)
    } else if new.access_mode == Some(AccessMode::Exclusive) || n.access == AccessMode::Exclusive {
        format!("Address range {} is already declared shared by resource '{}'", item, other.name)
    } else {
        format!(
            "Address range {} is already declared shared by resource '{}' (private flag differs)",
            item, other.name
        )
    }
}

/// Format a range the way conflict items show it: `0xSTART-0xEND`.
fn format_range_item(start: u32, end: u32) -> String {
    format!("{}-{}", format_u32_address(start), format_u32_address(end))
}

/// Check a new resource against the existing resources of its device (R1).
///
/// **First registration wins.** The new resource is accepted only if every
/// overlap with an existing resource is a shared item in a shared resource
/// on both sides, with the same endpoint type and the same item-level
/// `private` flag. A resource without `endpoints` claims every classic
/// endpoint (see [`WILDCARD_EP_LAST`]), so it clashes with any resource that
/// lists one, and with another resource without `endpoints`.
///
/// Existing resources are checked in `id` order. The registry is a
/// `HashMap`, whose order is random; sorting makes the reported resource the
/// same on every run.
///
/// # Errors
/// An `Overlap` [`ResourceError`] for the first existing resource with at
/// least one clash. `conflict.item` is the first clashing item and
/// `conflict.items` lists **every** clashing item with that resource:
/// endpoints ascending, then that resource's ranges in declaration order.
pub fn check_overlap(existing: &[Resource], new: &Resource) -> Result<(), ResourceError> {
    let mut sorted: Vec<&Resource> = existing.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let new_claims = endpoint_claims(new);

    for other in sorted {
        let other_claims = endpoint_claims(other);
        let mut items: Vec<String> = Vec::new();
        // (detail text, item type, item) of the first clash.
        let mut first: Option<(String, ConflictItemType, String)> = None;

        for (address, n) in &new_claims {
            let Some(o) = other_claims.get(address) else { continue };
            if endpoint_claims_compatible(new, other, n, o) {
                continue;
            }
            let item = format_ep_address(*address);
            if first.is_none() {
                first = Some((endpoint_detail(new, other, n, o), ConflictItemType::Endpoint, item.clone()));
            }
            items.push(item);
        }

        // Ranges are reported as the *existing* resource's range: that is
        // the item the client has to look up.
        for o in &other.address_ranges {
            let Some(n) = new
                .address_ranges
                .iter()
                .find(|n| n.overlaps(o) && !ranges_compatible(new, other, n, o))
            else {
                continue;
            };
            let item = format_range_item(o.start, o.end);
            if items.contains(&item) {
                continue;
            }
            if first.is_none() {
                first = Some((range_detail(new, other, n, o), ConflictItemType::AddressRange, item.clone()));
            }
            items.push(item);
        }

        if let Some((detail, item_type, item)) = first {
            let detail = if items.len() > 1 {
                format!("{} ({} conflicting items in total)", detail, items.len())
            } else {
                detail
            };
            let mut conflict =
                ResourceConflict::new(ConflictCode::Overlap).with_resource(&other.id, &other.name);
            conflict.item_type = Some(item_type);
            conflict.item = Some(item);
            return Err(ResourceError { detail, conflict: conflict.with_items(items) });
        }
    }
    Ok(())
}

// ============================================================================
// Tests (Tier 3: pure domain logic, no FFI)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::ConflictItemType;

    fn entry(address: u32, ep_type: &str, access: Option<AccessMode>) -> EndpointEntrySpec {
        EndpointEntrySpec {
            address,
            ep_type: ep_type.to_string(),
            access,
            private: false,
            description: None,
        }
    }

    fn range(start: u32, end: u32, access: Option<AccessMode>) -> AddressRangeSpec {
        AddressRangeSpec {
            name: Some("csr".to_string()),
            start,
            end,
            access,
            private: false,
            description: Some("control registers".to_string()),
        }
    }

    fn spec() -> ResourceSpec {
        ResourceSpec {
            name: "dac".to_string(),
            description: Some("DAC channel".to_string()),
            device_serial: "SN1".to_string(),
            access_mode: Some(AccessMode::Shared),
            private: false,
            interfaces: vec!["classic".to_string()],
            endpoints: Some(EndpointSpec {
                default_access: None,
                entries: vec![entry(0x00, "wire_in", Some(AccessMode::Shared))],
            }),
            address_ranges: vec![],
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_build_resource_full_declaration_roundtrips() {
        let mut s = spec();
        s.private = true;
        s.interfaces = vec!["classic".into(), "axi_lite".into()];
        s.endpoints = Some(EndpointSpec {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntrySpec {
                    private: true,
                    description: Some("ctrl".into()),
                    ..entry(0x01, "wire_in", Some(AccessMode::Exclusive))
                },
                entry(0x20, "wire_out", None),
            ],
        });
        s.address_ranges = vec![range(0x1000, 0x1FFF, Some(AccessMode::Shared))];
        s.metadata.insert("bitfile".into(), "dac.bit".into());

        let r = build_resource(s).unwrap();
        assert_eq!(r.id, "SN1:dac");
        assert_eq!(r.access_mode, Some(AccessMode::Shared));
        assert!(r.private);
        assert!(r.interfaces.classic && r.interfaces.axi_lite && !r.interfaces.axi_full);
        let eps = r.endpoints.unwrap();
        assert_eq!(eps.default_access, Some(AccessMode::Shared));
        assert_eq!(eps.entries.len(), 2);
        assert_eq!(eps.entries[0].access, AccessMode::Exclusive);
        assert!(eps.entries[0].private);
        assert_eq!(eps.entries[0].description.as_deref(), Some("ctrl"));
        assert_eq!(r.address_ranges[0].name.as_deref(), Some("csr"));
        assert_eq!(r.address_ranges[0].description.as_deref(), Some("control registers"));
        assert_eq!(r.metadata.get("bitfile").map(String::as_str), Some("dac.bit"));
    }

    #[test]
    fn test_build_resource_omitted_access_mode_with_endpoints_rejected() {
        let mut s = spec();
        s.access_mode = None;
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert!(err.detail.contains("D62"));
    }

    #[test]
    fn test_build_resource_omitted_endpoints_defaults_exclusive() {
        let mut s = spec();
        s.endpoints = None;
        s.access_mode = None;
        let r = build_resource(s).unwrap();
        assert_eq!(r.access_mode, Some(AccessMode::Exclusive));
        // D68: kept as `None`, not expanded into every classic EP.
        assert!(r.endpoints.is_none());
    }

    #[test]
    fn test_build_resource_padded_name_and_serial_trimmed() {
        let mut s = spec();
        s.name = "  dac ".into();
        s.device_serial = " SN1".into();
        let r = build_resource(s).unwrap();
        assert_eq!(r.id, "SN1:dac");
        assert_eq!(r.name, "dac");
        assert_eq!(r.device_serial, "SN1");
    }

    #[test]
    fn test_build_resource_read_only_declared_item_rejected() {
        // Entry access.
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x00, "wire_in", Some(AccessMode::ReadOnly))],
        });
        assert!(build_resource(s).unwrap_err().is_validation());
        // default_access.
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: Some(AccessMode::ReadOnly),
            entries: vec![],
        });
        assert!(build_resource(s).unwrap_err().is_validation());
        // Range access.
        let mut s = spec();
        s.address_ranges = vec![range(0x10, 0x20, Some(AccessMode::ReadOnly))];
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_omitted_endpoints_explicit_exclusive_accepted() {
        let mut s = spec();
        s.endpoints = None;
        s.access_mode = Some(AccessMode::Exclusive);
        assert_eq!(build_resource(s).unwrap().access_mode, Some(AccessMode::Exclusive));
    }

    #[test]
    fn test_build_resource_omitted_endpoints_shared_rejected() {
        let mut s = spec();
        s.endpoints = None;
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert!(err.detail.contains("D68"));
    }

    #[test]
    fn test_build_resource_empty_endpoints_claims_none() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec::default());
        let r = build_resource(s).unwrap();
        assert!(r.endpoints.unwrap().entries.is_empty());
    }

    #[test]
    fn test_build_resource_entry_uses_default_access() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: Some(AccessMode::Exclusive),
            entries: vec![entry(0x40, "trigger_in", None)],
        });
        let r = build_resource(s).unwrap();
        assert_eq!(r.endpoints.unwrap().entries[0].access, AccessMode::Exclusive);
    }

    #[test]
    fn test_build_resource_default_access_with_empty_entries_is_inert() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: Some(AccessMode::Shared),
            entries: vec![],
        });
        let r = build_resource(s).unwrap();
        assert!(r.endpoints.unwrap().entries.is_empty());
    }

    #[test]
    fn test_build_resource_entry_without_access_or_default_rejected() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x05, "wire_in", None)],
        });
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert_eq!(err.conflict.item_type, Some(ConflictItemType::Endpoint));
        assert_eq!(err.conflict.item.as_deref(), Some("0x05"));
    }

    #[test]
    fn test_build_resource_range_access_required() {
        let mut s = spec();
        s.address_ranges = vec![range(0x10, 0x20, None)];
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert_eq!(err.conflict.item.as_deref(), Some("0x00000010-0x00000020"));
    }

    #[test]
    fn test_build_resource_invalid_endpoint_type_address_rejected() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x20, "wire_in", Some(AccessMode::Shared))],
        });
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_unknown_endpoint_type_rejected() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x00, "wire", Some(AccessMode::Shared))],
        });
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_endpoint_address_above_0xbf_rejected() {
        let mut s = spec();
        s.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x1C0, "pipe_out", Some(AccessMode::Shared))],
        });
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_invalid_address_range_rejected() {
        let mut s = spec();
        s.address_ranges = vec![range(0x20, 0x10, Some(AccessMode::Shared))];
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_no_interfaces_rejected() {
        let mut s = spec();
        s.interfaces.clear();
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_unknown_interface_rejected() {
        let mut s = spec();
        s.interfaces = vec!["usb".into()];
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_build_resource_empty_name_rejected() {
        let mut s = spec();
        s.name = " ".into();
        assert!(build_resource(s).unwrap_err().is_validation());
    }

    #[test]
    fn test_parse_hex_u32_accepts_prefix_and_underscores() {
        assert_eq!(parse_hex_u32("0x44A6_0000", "start").unwrap(), 0x44A6_0000);
        assert_eq!(parse_hex_u32("ff", "start").unwrap(), 0xFF);
        assert!(parse_hex_u32("0xZZ", "start").unwrap_err().is_validation());
    }

    #[test]
    fn test_parse_declared_mode_read_only_rejected() {
        assert_eq!(parse_declared_mode("shared", "m").unwrap(), AccessMode::Shared);
        assert!(parse_declared_mode("read_only", "m").is_err());
    }

    #[test]
    fn test_check_overlap_exclusive_endpoint_names_resource_and_item() {
        let mut a = spec();
        a.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x05, "wire_in", Some(AccessMode::Exclusive))],
        });
        let existing = build_resource(a).unwrap();
        let mut b = spec();
        b.name = "adc".into();
        b.endpoints = Some(EndpointSpec {
            default_access: None,
            entries: vec![entry(0x05, "wire_in", Some(AccessMode::Exclusive))],
        });
        let new = build_resource(b).unwrap();

        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.code, ConflictCode::Overlap);
        assert_eq!(err.conflict.resource_id.as_deref(), Some("SN1:dac"));
        assert_eq!(err.conflict.resource_name.as_deref(), Some("dac"));
        assert_eq!(err.conflict.item_type, Some(ConflictItemType::Endpoint));
        assert_eq!(err.conflict.item.as_deref(), Some("0x05"));
    }

    #[test]
    fn test_check_overlap_exclusive_range_denied() {
        let mut a = spec();
        a.address_ranges = vec![range(0x1000, 0x1FFF, Some(AccessMode::Exclusive))];
        let existing = build_resource(a).unwrap();
        let mut b = spec();
        b.name = "adc".into();
        b.address_ranges = vec![range(0x1800, 0x2FFF, Some(AccessMode::Exclusive))];
        let new = build_resource(b).unwrap();

        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.item_type, Some(ConflictItemType::AddressRange));
        assert_eq!(err.conflict.item.as_deref(), Some("0x00001000-0x00001FFF"));
    }

    #[test]
    fn test_check_overlap_shared_same_type_same_private_allowed() {
        let existing = build_resource(spec()).unwrap();
        let mut b = spec();
        b.name = "adc".into();
        let new = build_resource(b).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }

    // ---- 44-B: shape rules inside one declaration -------------------------

    /// Small helper: a spec named `name` with the given endpoint entries.
    fn spec_with(name: &str, entries: Vec<EndpointEntrySpec>) -> ResourceSpec {
        let mut s = spec();
        s.name = name.into();
        s.endpoints = Some(EndpointSpec { default_access: None, entries });
        s
    }

    /// Small helper: a spec named `name` with no endpoint entries and the
    /// given address ranges.
    fn spec_ranges(name: &str, ranges: Vec<AddressRangeSpec>) -> ResourceSpec {
        let mut s = spec();
        s.name = name.into();
        s.endpoints = Some(EndpointSpec::default());
        s.address_ranges = ranges;
        s
    }

    /// Small helper: a spec named `name` without an `endpoints` field (it
    /// claims every classic endpoint).
    fn spec_wildcard(name: &str) -> ResourceSpec {
        let mut s = spec();
        s.name = name.into();
        s.endpoints = None;
        s.access_mode = None;
        s
    }

    #[test]
    fn test_build_resource_exclusive_with_shared_entry_rejected() {
        let mut s = spec_with("dac", vec![entry(0x05, "wire_in", Some(AccessMode::Shared))]);
        s.access_mode = Some(AccessMode::Exclusive);
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert_eq!(err.conflict.item.as_deref(), Some("0x05"));
    }

    #[test]
    fn test_build_resource_exclusive_with_shared_range_rejected() {
        let mut s = spec_ranges("dac", vec![range(0x10, 0x20, Some(AccessMode::Shared))]);
        s.access_mode = Some(AccessMode::Exclusive);
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert_eq!(err.conflict.item.as_deref(), Some("0x00000010-0x00000020"));
    }

    #[test]
    fn test_build_resource_exclusive_with_default_access_shared_rejected() {
        let mut s = spec();
        s.access_mode = Some(AccessMode::Exclusive);
        s.endpoints = Some(EndpointSpec {
            default_access: Some(AccessMode::Shared),
            entries: vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))],
        });
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert!(err.detail.contains("default_access"));
    }

    #[test]
    fn test_build_resource_exclusive_empty_entries_default_shared_rejected() {
        let mut s = spec();
        s.access_mode = Some(AccessMode::Exclusive);
        s.endpoints = Some(EndpointSpec { default_access: Some(AccessMode::Shared), entries: vec![] });
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert!(err.detail.contains("default_access"));
    }

    #[test]
    fn test_build_resource_shared_with_exclusive_items_accepted() {
        // Rule A only restricts exclusive resources; a shared resource may
        // hold exclusive items.
        let mut s = spec_with("dac", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]);
        s.address_ranges = vec![range(0x10, 0x20, Some(AccessMode::Exclusive))];
        assert!(build_resource(s).is_ok());
    }

    #[test]
    fn test_build_resource_duplicate_endpoint_address_rejected() {
        let s = spec_with(
            "dac",
            vec![
                entry(0x05, "wire_in", Some(AccessMode::Shared)),
                entry(0x05, "wire_in", Some(AccessMode::Shared)),
            ],
        );
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert_eq!(err.conflict.item.as_deref(), Some("0x05"));
    }

    #[test]
    fn test_build_resource_duplicate_address_pipe_vs_btpipe_rejected() {
        let s = spec_with(
            "dac",
            vec![
                entry(0x80, "pipe_in", Some(AccessMode::Shared)),
                entry(0x80, "bt_pipe_in", Some(AccessMode::Shared)),
            ],
        );
        let err = build_resource(s).unwrap_err();
        assert!(err.is_validation());
        assert_eq!(err.conflict.item.as_deref(), Some("0x80"));
    }

    #[test]
    fn test_parse_ep_address_strict_pattern() {
        assert_eq!(parse_ep_address("0x05", "address").unwrap(), 0x05);
        assert_eq!(parse_ep_address("0xbF", "address").unwrap(), 0xBF);
        for bad in ["5", "0x5", "0x005", "0X05", "0x0_5", "", "0x"] {
            assert!(parse_ep_address(bad, "address").is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn test_self_overlapping_ranges_reports_pairs() {
        let s = spec_ranges(
            "dac",
            vec![
                range(0x00, 0x0A, Some(AccessMode::Shared)),
                range(0x05, 0x0F, Some(AccessMode::Shared)),
                range(0x20, 0x30, Some(AccessMode::Shared)),
                range(0x25, 0x35, Some(AccessMode::Exclusive)),
            ],
        );
        let r = build_resource(s).unwrap();
        assert_eq!(self_overlapping_ranges(&r), vec![(0, 1), (2, 3)]);
    }

    #[test]
    fn test_self_overlapping_ranges_disjoint_and_adjacent_empty() {
        let s = spec_ranges(
            "dac",
            vec![
                range(0x00, 0x09, Some(AccessMode::Shared)),
                range(0x0A, 0x19, Some(AccessMode::Shared)),
            ],
        );
        let r = build_resource(s).unwrap();
        assert!(self_overlapping_ranges(&r).is_empty());
    }

    fn ifaces(classic: bool, axi_lite: bool, axi_full: bool, axi_stream: bool) -> InterfaceDeclaration {
        InterfaceDeclaration { classic, axi_lite, axi_full, axi_stream }
    }

    #[test]
    fn test_check_interfaces_for_dataport_axi_on_legacy_rejected() {
        let err = check_interfaces_for_dataport(&ifaces(true, true, false, false), DataportType::Legacy)
            .unwrap_err();
        assert!(err.is_validation());
        assert!(err.detail.contains("axi_lite"));
        assert!(err.detail.contains("legacy"));
    }

    #[test]
    fn test_check_interfaces_for_dataport_classic_on_legacy_accepted() {
        assert!(check_interfaces_for_dataport(&ifaces(true, false, false, false), DataportType::Legacy).is_ok());
    }

    #[test]
    fn test_check_interfaces_for_dataport_axi_on_axi_accepted() {
        assert!(check_interfaces_for_dataport(&ifaces(true, true, true, true), DataportType::Axi).is_ok());
    }

    #[test]
    fn test_check_interfaces_for_dataport_axi_on_unknown_rejected() {
        let err = check_interfaces_for_dataport(&ifaces(false, false, true, false), DataportType::Unknown)
            .unwrap_err();
        assert!(err.is_validation());
        assert!(err.detail.contains("axi_full"));
    }

    // ---- 44-B: R1 overlap between resources -------------------------------

    #[test]
    fn test_check_overlap_existing_shared_new_exclusive_denied() {
        let existing = build_resource(spec()).unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]))
            .unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.code, ConflictCode::Overlap);
        assert!(err.detail.contains("already declared shared"));
        assert_eq!(err.conflict.items, vec!["0x00"]);
    }

    #[test]
    fn test_check_overlap_existing_exclusive_new_shared_denied() {
        let existing = build_resource(spec_with("dac", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]))
            .unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x00, "wire_in", Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("already declared exclusive"));
    }

    #[test]
    fn test_check_overlap_exclusive_resource_item_denied() {
        let mut a = spec_with("dac", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]);
        a.access_mode = Some(AccessMode::Exclusive);
        let existing = build_resource(a).unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x00, "wire_in", Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("belongs to exclusive resource 'dac'"));
    }

    #[test]
    fn test_check_overlap_private_mismatch_denied() {
        let existing = build_resource(spec()).unwrap();
        let new = build_resource(spec_with(
            "adc",
            vec![EndpointEntrySpec { private: true, ..entry(0x00, "wire_in", Some(AccessMode::Shared)) }],
        ))
        .unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("private flag differs"));
    }

    #[test]
    fn test_check_overlap_resource_private_not_compared_allowed() {
        // Resource-level `private` controls ReadOnly acquisition only; it is
        // not part of the overlap rule (owner decision Q3).
        let existing = build_resource(spec()).unwrap();
        let mut b = spec();
        b.name = "adc".into();
        b.private = true;
        let new = build_resource(b).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }

    #[test]
    fn test_check_overlap_type_mismatch_denied() {
        let existing = build_resource(spec_with("dac", vec![entry(0x80, "pipe_in", Some(AccessMode::Shared))])).unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x80, "bt_pipe_in", Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("type differs"));
    }

    #[test]
    fn test_check_overlap_wildcard_blocks_later_entry_denied() {
        let existing = build_resource(spec_wildcard("all")).unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x00, "wire_in", Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.item.as_deref(), Some("0x00"));
        assert!(err.detail.contains("belongs to exclusive resource 'all'"));
    }

    #[test]
    fn test_check_overlap_new_wildcard_vs_existing_entry_denied() {
        let existing = build_resource(spec()).unwrap();
        let new = build_resource(spec_wildcard("all")).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("already declared shared"));
        assert_eq!(err.conflict.items, vec!["0x00"]);
    }

    #[test]
    fn test_check_overlap_wildcard_vs_wildcard_lists_all_endpoints() {
        let existing = build_resource(spec_wildcard("a")).unwrap();
        let new = build_resource(spec_wildcard("b")).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.items.len(), 128);
        assert_eq!(err.conflict.items[0], "0x00");
        assert_eq!(err.conflict.items[127], "0x7F");
        assert_eq!(err.conflict.item.as_deref(), Some("0x00"));
        assert!(err.detail.contains("(128 conflicting items in total)"));
    }

    #[test]
    fn test_check_overlap_wildcard_allows_pipe_only_resource() {
        // D77 + owner decision 44-E Q2: omitted `endpoints` claims 0x00..=0x7F
        // only, so a second resource declaring only Pipe/BTPipe (0x80..=0xBF)
        // does not overlap and is allowed (with a server warning).
        let existing = build_resource(spec_wildcard("all")).unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x80, "pipe_in", Some(AccessMode::Shared))])).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }

    #[test]
    fn test_check_overlap_wildcard_blocks_trigger_denied() {
        let existing = build_resource(spec_wildcard("all")).unwrap();
        let new = build_resource(spec_with("adc", vec![entry(0x7F, "trigger_out", Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.item.as_deref(), Some("0x7F"));
    }

    #[test]
    fn test_check_overlap_wildcard_vs_empty_entries_allowed() {
        let existing = build_resource(spec_wildcard("all")).unwrap();
        let new = build_resource(spec_ranges("adc", vec![])).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }

    #[test]
    fn test_check_overlap_shared_range_partial_allowed() {
        let existing = build_resource(spec_ranges("dac", vec![range(0x1000, 0x1FFF, Some(AccessMode::Shared))])).unwrap();
        let new = build_resource(spec_ranges("adc", vec![range(0x1800, 0x2FFF, Some(AccessMode::Shared))])).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }

    #[test]
    fn test_check_overlap_range_private_mismatch_denied() {
        let existing = build_resource(spec_ranges("dac", vec![range(0x1000, 0x1FFF, Some(AccessMode::Shared))])).unwrap();
        let new = build_resource(spec_ranges(
            "adc",
            vec![AddressRangeSpec { private: true, ..range(0x1800, 0x2FFF, Some(AccessMode::Shared)) }],
        ))
        .unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("private flag differs"));
    }

    #[test]
    fn test_check_overlap_existing_exclusive_range_vs_shared_denied() {
        let existing =
            build_resource(spec_ranges("dac", vec![range(0x1000, 0x1FFF, Some(AccessMode::Exclusive))])).unwrap();
        let new = build_resource(spec_ranges("adc", vec![range(0x1800, 0x2FFF, Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert!(err.detail.contains("already declared exclusive"));
    }

    #[test]
    fn test_check_overlap_adjacent_ranges_allowed() {
        let existing =
            build_resource(spec_ranges("dac", vec![range(0x1000, 0x1FFF, Some(AccessMode::Exclusive))])).unwrap();
        let new = build_resource(spec_ranges("adc", vec![range(0x2000, 0x2FFF, Some(AccessMode::Exclusive))])).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }

    #[test]
    fn test_check_overlap_inclusive_boundary_denied() {
        let existing =
            build_resource(spec_ranges("dac", vec![range(0x1000, 0x1FFF, Some(AccessMode::Exclusive))])).unwrap();
        let new = build_resource(spec_ranges("adc", vec![range(0x1FFF, 0x2FFF, Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.item.as_deref(), Some("0x00001000-0x00001FFF"));
    }

    #[test]
    fn test_check_overlap_multiple_items_listed() {
        let mut a = spec_with(
            "dac",
            vec![
                entry(0x01, "wire_in", Some(AccessMode::Exclusive)),
                entry(0x00, "wire_in", Some(AccessMode::Exclusive)),
            ],
        );
        a.address_ranges = vec![range(0x1000, 0x1FFF, Some(AccessMode::Exclusive))];
        let existing = build_resource(a).unwrap();
        let mut b = spec_with(
            "adc",
            vec![
                entry(0x00, "wire_in", Some(AccessMode::Exclusive)),
                entry(0x01, "wire_in", Some(AccessMode::Exclusive)),
            ],
        );
        b.address_ranges = vec![range(0x1800, 0x2FFF, Some(AccessMode::Exclusive))];
        let new = build_resource(b).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.items, vec!["0x00", "0x01", "0x00001000-0x00001FFF"]);
        assert_eq!(err.conflict.item.as_deref(), Some("0x00"));
        assert!(err.detail.contains("(3 conflicting items in total)"));
    }

    #[test]
    fn test_check_overlap_multiple_conflicting_resources_deterministic() {
        let a = spec_with("a", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]);
        let b = spec_with("b", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]);
        // Listed out of order on purpose: the check sorts by id.
        let existing = vec![build_resource(b).unwrap(), build_resource(a).unwrap()];
        let new = build_resource(spec_with("new", vec![entry(0x00, "wire_in", Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&existing, &new).unwrap_err();
        assert_eq!(err.conflict.resource_id.as_deref(), Some("SN1:a"));
    }

    #[test]
    fn test_check_overlap_multiple_conflicting_resources_reports_lowest_id_items() {
        // Only the items that clash with the *named* resource are listed.
        let a = spec_with("a", vec![entry(0x00, "wire_in", Some(AccessMode::Exclusive))]);
        let b = spec_with("b", vec![entry(0x01, "wire_in", Some(AccessMode::Exclusive))]);
        let existing = vec![build_resource(b).unwrap(), build_resource(a).unwrap()];
        let new = build_resource(spec_with(
            "new",
            vec![
                entry(0x00, "wire_in", Some(AccessMode::Shared)),
                entry(0x01, "wire_in", Some(AccessMode::Shared)),
            ],
        ))
        .unwrap();
        let err = check_overlap(&existing, &new).unwrap_err();
        assert_eq!(err.conflict.resource_id.as_deref(), Some("SN1:a"));
        assert_eq!(err.conflict.items, vec!["0x00"]);
    }

    #[test]
    fn test_check_overlap_existing_range_overlaps_several_new_ranges_finds_incompatible() {
        // The first overlapping new range is compatible; a later one is not.
        let existing =
            build_resource(spec_ranges("dac", vec![range(0x1000, 0x1FFF, Some(AccessMode::Shared))])).unwrap();
        let new = build_resource(spec_ranges(
            "adc",
            vec![
                range(0x0000, 0x0FFF, Some(AccessMode::Shared)),
                range(0x1800, 0x2FFF, Some(AccessMode::Shared)),
                AddressRangeSpec { private: true, ..range(0x1000, 0x10FF, Some(AccessMode::Shared)) },
            ],
        ))
        .unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.item.as_deref(), Some("0x00001000-0x00001FFF"));
        assert_eq!(err.conflict.items, vec!["0x00001000-0x00001FFF"]);
        assert!(err.detail.contains("private flag differs"));
    }

    #[test]
    fn test_check_overlap_identical_existing_ranges_second_incompatible_reports_once() {
        let existing = build_resource(spec_ranges(
            "dac",
            vec![
                range(0x1000, 0x1FFF, Some(AccessMode::Shared)),
                AddressRangeSpec { private: true, ..range(0x1000, 0x1FFF, Some(AccessMode::Shared)) },
            ],
        ))
        .unwrap();
        let new = build_resource(spec_ranges("adc", vec![range(0x1800, 0x2FFF, Some(AccessMode::Shared))])).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.items, vec!["0x00001000-0x00001FFF"]);
    }

    #[test]
    fn test_check_overlap_wildcard_vs_wildcard_with_ranges_lists_endpoints_then_ranges() {
        let mut a = spec_wildcard("a");
        a.address_ranges = vec![range(0x1000, 0x1FFF, Some(AccessMode::Exclusive))];
        let existing = build_resource(a).unwrap();
        let mut b = spec_wildcard("b");
        b.address_ranges = vec![range(0x1800, 0x2FFF, Some(AccessMode::Exclusive))];
        let new = build_resource(b).unwrap();
        let err = check_overlap(&[existing], &new).unwrap_err();
        assert_eq!(err.conflict.items.len(), 129);
        assert_eq!(err.conflict.items[127], "0x7F");
        assert_eq!(err.conflict.items[128], "0x00001000-0x00001FFF");
        assert_eq!(err.conflict.item_type, Some(ConflictItemType::Endpoint));
        assert!(err.detail.contains("(129 conflicting items in total)"));
    }
}
