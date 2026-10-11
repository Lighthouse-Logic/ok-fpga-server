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
//! ## What is checked here (sub-phase 44-A)
//!
//! Only rules that the request *shape* forces:
//! - `access_mode` is required when `endpoints` is present (D62).
//! - No `endpoints` field = an exclusive resource that claims every classic
//!   EP; `access_mode` may be omitted, `shared` is rejected (D68).
//! - Each endpoint entry takes its `access` from the entry, else from
//!   `default_access`, else the request is rejected (D61).
//! - Every address range has an `access` and `start <= end`.
//! - EP type matches its address; at least one known interface.
//!
//! The full overlap rule R1 (shared items may overlap, type / `private`
//! mismatch, "exclusive resources contain only exclusive items", AXI
//! interfaces vs. dataport) arrives in sub-phase 44-B. Until then
//! [`check_overlap`] keeps the previous rule: two **exclusive** items may not
//! overlap.
//!
//! All errors are [`ResourceError`] values. A `Validation` code means the
//! request is wrong (REST 422, gRPC `INVALID_ARGUMENT`); `Overlap` means it
//! clashes with an existing resource (REST 409, gRPC `FAILED_PRECONDITION`).

use std::collections::HashMap;

use crate::resource::{
    AccessMode, AddressRange, ConflictCode, EndpointDeclaration, EndpointEntry, EndpointType,
    InterfaceDeclaration, Resource, ResourceConflict, ResourceError,
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

    let id = format!("{}:{}", device_serial, name);
    let mut resource = Resource::new(id, name, spec.description, device_serial, access_mode);
    resource.interfaces = interfaces;
    resource.endpoints = endpoints;
    resource.address_ranges = address_ranges;
    resource.metadata = spec.metadata;
    resource.private = spec.private;
    Ok(resource)
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
// check_overlap (pre-R1 rule)
// ============================================================================

/// Check a new resource against the existing resources of its device.
///
/// **Interim rule (until 44-B):** two *exclusive* endpoint entries with the
/// same address, or two *exclusive* address ranges that overlap, clash. This
/// is the rule the REST handler applied before 44-A; it now lives here so
/// gRPC applies it too.
///
/// # Errors
/// An `Overlap` [`ResourceError`] naming the first existing resource and
/// item that clash (first registration wins).
pub fn check_overlap(existing: &[Resource], new: &Resource) -> Result<(), ResourceError> {
    if let Some(new_eps) = &new.endpoints {
        for new_ep in new_eps.entries.iter().filter(|e| e.access == AccessMode::Exclusive) {
            for other in existing {
                let Some(other_eps) = &other.endpoints else { continue };
                if other_eps
                    .entries
                    .iter()
                    .any(|e| e.address == new_ep.address && e.access == AccessMode::Exclusive)
                {
                    return Err(ResourceError {
                        detail: format!(
                            "Endpoint 0x{:02X} is already declared exclusive by resource '{}'",
                            new_ep.address, other.name
                        ),
                        conflict: ResourceConflict::new(ConflictCode::Overlap)
                            .with_resource(&other.id, &other.name)
                            .with_endpoint(new_ep.address),
                    });
                }
            }
        }
    }

    for new_range in new.address_ranges.iter().filter(|r| r.access == AccessMode::Exclusive) {
        for other in existing {
            if let Some(hit) = other
                .address_ranges
                .iter()
                .find(|r| r.access == AccessMode::Exclusive && new_range.overlaps(r))
            {
                return Err(ResourceError {
                    detail: format!(
                        "Address range 0x{:08X}-0x{:08X} overlaps exclusive range \
                         0x{:08X}-0x{:08X} of resource '{}'",
                        new_range.start, new_range.end, hit.start, hit.end, other.name
                    ),
                    conflict: ResourceConflict::new(ConflictCode::Overlap)
                        .with_resource(&other.id, &other.name)
                        .with_range(hit.start, hit.end),
                });
            }
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
    fn test_check_overlap_shared_items_allowed() {
        let existing = build_resource(spec()).unwrap();
        let mut b = spec();
        b.name = "adc".into();
        let new = build_resource(b).unwrap();
        assert!(check_overlap(&[existing], &new).is_ok());
    }
}
