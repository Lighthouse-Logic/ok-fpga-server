// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # API Views of Resources (ADR-034)
//!
//! A *view* is the JSON shape a client sees. The core [`Resource`] type is
//! built for the server's own bookkeeping; this module turns it into the
//! OpenAPI `Resource` schema (S-02):
//!
//! - `state` and modes are the spec enum strings (`in_use_shared`,
//!   `read_only`, ...), never Rust `Debug` / `Display` text (S-09).
//! - `access_mode` is the **declared** mode (`shared` / `exclusive`).
//! - `holders` are objects `{user_id, mode}` so clients see who holds the
//!   resource **and how** (D37).
//! - Endpoint addresses are written `0x05`, range limits `0x44A60000`, the
//!   same format clients use when they register.
//!
//! The REST API (`/api/v1/resources`) and the dashboard's management API
//! (`/mgmt/resources`) both use these types, so the two can never drift.
//! This module has no feature gate because the management API is always
//! compiled.

use std::collections::HashMap;

use serde::Serialize;

use ok_fpga_core::resource::{
    format_ep_address, format_u32_address, AddressRange, EndpointDeclaration, Holding, Resource,
};

/// JSON view of one resource (OpenAPI `Resource`).
#[derive(Debug, Serialize)]
pub struct ResourceView {
    /// `{device_serial}:{name}`.
    pub id: String,
    /// Resource name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Device that provides the resource.
    pub device_serial: String,
    /// Spec state string.
    pub state: &'static str,
    /// Declared mode: `shared` or `exclusive`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<&'static str>,
    /// Current holders with their acquire mode.
    pub holders: Vec<HolderView>,
    /// User holding the lock, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_by: Option<String>,
    /// Interface names (`classic`, `axi_lite`, `axi_full`, `axi_stream`).
    pub interfaces: Vec<&'static str>,
    /// Classic endpoint declaration; absent = every classic EP (D68).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<EndpointsView>,
    /// Declared address ranges.
    pub address_ranges: Vec<AddressRangeView>,
    /// Text-only metadata (D69).
    pub metadata: HashMap<String, String>,
    /// Resource-level `private` flag: an access rule, not visibility (D57).
    pub private: bool,
    /// Registration-time advisory warnings (populated on `POST /resources`
    /// when non-empty; omitted when empty and on `GET`/acquire/lock/unlock,
    /// Phase 45 Q2 Option A1).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// One holder of a resource.
#[derive(Debug, Serialize)]
pub struct HolderView {
    /// The holder's user ID.
    pub user_id: String,
    /// `shared`, `exclusive` or `read_only`.
    pub mode: &'static str,
}

/// The `endpoints` object of a resource view.
#[derive(Debug, Serialize)]
pub struct EndpointsView {
    /// Default used at registration for entries without `access` (D61).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_access: Option<&'static str>,
    /// Declared entries.
    pub entries: Vec<EndpointEntryView>,
}

/// One endpoint entry of a resource view.
#[derive(Debug, Serialize)]
pub struct EndpointEntryView {
    /// Address, e.g. `0x05`.
    pub address: String,
    /// Spec type name, e.g. `wire_in`.
    #[serde(rename = "type")]
    pub ep_type: String,
    /// Declared mode of the entry.
    pub access: &'static str,
    /// Item-level `private` flag.
    pub private: bool,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One address range of a resource view.
#[derive(Debug, Serialize)]
pub struct AddressRangeView {
    /// Optional range name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// First address, e.g. `0x44A60000`.
    pub start: String,
    /// Last address (inclusive).
    pub end: String,
    /// Declared mode.
    pub access: &'static str,
    /// Item-level `private` flag.
    pub private: bool,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One entry of `UserSession.holdings` (ADR-034 P5).
#[derive(Debug, Serialize)]
pub struct HoldingView {
    /// The held resource.
    pub resource_id: String,
    /// Acquire mode.
    pub mode: &'static str,
    /// `true` if this user holds the resource's lock.
    pub locked: bool,
}

impl From<&Holding> for HoldingView {
    fn from(h: &Holding) -> Self {
        HoldingView {
            resource_id: h.resource_id.clone(),
            mode: h.mode.as_str(),
            locked: h.locked,
        }
    }
}

impl From<&Resource> for ResourceView {
    fn from(r: &Resource) -> Self {
        let mut interfaces = Vec::new();
        if r.interfaces.classic {
            interfaces.push("classic");
        }
        if r.interfaces.axi_lite {
            interfaces.push("axi_lite");
        }
        if r.interfaces.axi_full {
            interfaces.push("axi_full");
        }
        if r.interfaces.axi_stream {
            interfaces.push("axi_stream");
        }

        ResourceView {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            device_serial: r.device_serial.clone(),
            state: r.state.as_str(),
            access_mode: r.access_mode.map(|m| m.as_str()),
            holders: r
                .holders
                .iter()
                .map(|h| HolderView {
                    user_id: h.user_id.clone(),
                    mode: h.mode.as_str(),
                })
                .collect(),
            locked_by: r.locked_by.clone(),
            interfaces,
            endpoints: r.endpoints.as_ref().map(EndpointsView::from),
            address_ranges: r.address_ranges.iter().map(AddressRangeView::from).collect(),
            metadata: r.metadata.clone(),
            private: r.private,
            warnings: Vec::new(),
        }
    }
}

impl ResourceView {
    /// Construct a [`ResourceView`] with registration-time `warnings` attached
    /// (used by `POST /api/v1/resources` `201 Created`, Phase 45 Q2).
    pub fn from_with_warnings(r: &Resource, warnings: Vec<String>) -> Self {
        let mut view = Self::from(r);
        view.warnings = warnings;
        view
    }
}

impl From<&EndpointDeclaration> for EndpointsView {
    fn from(d: &EndpointDeclaration) -> Self {
        EndpointsView {
            default_access: d.default_access.map(|m| m.as_str()),
            entries: d
                .entries
                .iter()
                .map(|e| EndpointEntryView {
                    address: format_ep_address(e.address),
                    ep_type: e.ep_type.to_string(),
                    access: e.access.as_str(),
                    private: e.private,
                    description: e.description.clone(),
                })
                .collect(),
        }
    }
}

impl From<&AddressRange> for AddressRangeView {
    fn from(a: &AddressRange) -> Self {
        AddressRangeView {
            name: a.name.clone(),
            start: format_u32_address(a.start),
            end: format_u32_address(a.end),
            access: a.access.as_str(),
            private: a.private,
            description: a.description.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ok_fpga_core::resource::{
        AccessMode, EndpointEntry, EndpointType, InterfaceDeclaration, ResourceRegistry,
    };

    fn sample() -> Resource {
        let mut r = Resource::new(
            "SN1:dac".into(),
            "dac".into(),
            Some("DAC".into()),
            "SN1".into(),
            AccessMode::Shared,
        );
        r.interfaces = InterfaceDeclaration {
            classic: true,
            axi_lite: true,
            axi_full: false,
            axi_stream: false,
        };
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![EndpointEntry {
                address: 0x05,
                ep_type: EndpointType::WireIn,
                access: AccessMode::Shared,
                description: Some("ctrl".into()),
                private: true,
            }],
        });
        r.address_ranges = vec![AddressRange {
            name: Some("csr".into()),
            description: None,
            start: 0x44A6_0000,
            end: 0x44A6_FFFF,
            access: AccessMode::Exclusive,
            private: false,
        }];
        r.metadata.insert("k".into(), "v".into());
        r
    }

    #[test]
    fn test_resource_view_holders_include_mode() {
        let reg = ResourceRegistry::new();
        reg.register(sample());
        reg.acquire("SN1:dac", &"alice".to_string(), AccessMode::Shared).unwrap();
        reg.acquire("SN1:dac", &"bob".to_string(), AccessMode::ReadOnly).unwrap();
        let v = serde_json::to_value(ResourceView::from(&reg.get("SN1:dac").unwrap())).unwrap();
        assert_eq!(v["holders"][0], serde_json::json!({"user_id": "alice", "mode": "shared"}));
        assert_eq!(v["holders"][1], serde_json::json!({"user_id": "bob", "mode": "read_only"}));
    }

    #[test]
    fn test_resource_view_state_uses_spec_enum_strings() {
        let reg = ResourceRegistry::new();
        reg.register(sample());
        let v = serde_json::to_value(ResourceView::from(&reg.get("SN1:dac").unwrap())).unwrap();
        assert_eq!(v["state"], "available");
        reg.acquire("SN1:dac", &"alice".to_string(), AccessMode::Shared).unwrap();
        reg.lock("SN1:dac", &"alice".to_string()).unwrap();
        let v = serde_json::to_value(ResourceView::from(&reg.get("SN1:dac").unwrap())).unwrap();
        assert_eq!(v["state"], "in_use_locked");
        assert_eq!(v["locked_by"], "alice");
    }

    #[test]
    fn test_resource_view_access_mode_uses_declared_spec_strings() {
        let v = serde_json::to_value(ResourceView::from(&sample())).unwrap();
        assert_eq!(v["access_mode"], "shared");
    }

    #[test]
    fn test_resource_view_includes_declarations_and_metadata() {
        let v = serde_json::to_value(ResourceView::from(&sample())).unwrap();
        assert_eq!(v["interfaces"], serde_json::json!(["classic", "axi_lite"]));
        assert_eq!(v["endpoints"]["default_access"], "shared");
        assert_eq!(
            v["endpoints"]["entries"][0],
            serde_json::json!({"address": "0x05", "type": "wire_in", "access": "shared",
                               "private": true, "description": "ctrl"})
        );
        assert_eq!(
            v["address_ranges"][0],
            serde_json::json!({"name": "csr", "start": "0x44A60000", "end": "0x44A6FFFF",
                               "access": "exclusive", "private": false})
        );
        assert_eq!(v["metadata"]["k"], "v");
        assert_eq!(v["private"], false);
    }

    #[test]
    fn test_resource_view_no_endpoints_field_omitted() {
        let mut r = sample();
        r.endpoints = None;
        let v = serde_json::to_value(ResourceView::from(&r)).unwrap();
        assert!(v.get("endpoints").is_none());
        assert!(v.get("warnings").is_none());
    }

    #[test]
    fn test_resource_view_warnings_omitted_when_empty_and_included_when_non_empty() {
        let r = sample();
        let v_empty = serde_json::to_value(ResourceView::from(&r)).unwrap();
        assert!(v_empty.get("warnings").is_none());

        let v_with = serde_json::to_value(ResourceView::from_with_warnings(
            &r,
            vec!["warn-1".to_string(), "warn-2".to_string()],
        ))
        .unwrap();
        assert_eq!(v_with["warnings"], serde_json::json!(["warn-1", "warn-2"]));
    }
}
