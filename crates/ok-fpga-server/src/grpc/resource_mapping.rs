//! # Resource ↔ Protobuf Mapping (ADR-034)
//!
//! Helpers that translate between the core resource types in
//! `ok-fpga-core` and the generated protobuf types, plus the gRPC error
//! convention for resource RPCs.
//!
//! ## Why a separate module?
//!
//! `device_service.rs` implements dozens of RPCs. Keeping the resource
//! conversions here makes them easy to unit-test without starting a gRPC
//! server, and mirrors the REST side where [`crate::views`] holds the JSON
//! shapes.
//!
//! ## Error convention (ADR-034 P7, D73)
//!
//! | REST | gRPC                  | When                                   |
//! |------|-----------------------|----------------------------------------|
//! | 404  | `NOT_FOUND`           | unknown resource / device              |
//! | 409  | `FAILED_PRECONDITION` | overlap, locked, duplicate, ...        |
//! | 422  | `INVALID_ARGUMENT`    | invalid declaration or mode            |
//!
//! When a D73 code applies, the machine-readable detail travels the
//! standard gRPC way: a `google.rpc.Status` message, serialised into the
//! `grpc-status-details-bin` trailer, whose `details` list holds one
//! `google.protobuf.Any` wrapping our `ResourceConflictDetail`. Every gRPC
//! ecosystem (Python `grpc_status`, Go `status.FromError`, ...) knows how to
//! unpack that. We define the two tiny Google messages locally with
//! `prost` derives instead of pulling in another crate.

use std::collections::HashMap;

use prost::Message;
use tonic::{Code, Status};

use ok_fpga_core::declaration::{AddressRangeSpec, EndpointEntrySpec, EndpointSpec, ResourceSpec};
use ok_fpga_core::resource::{
    AccessMode, AddressRange, EndpointDeclaration, Holding, Resource, ResourceConflict,
    ResourceError, ResourceState,
};
use ok_fpga_core::user::UserSession;

use crate::grpc::proto;

/// `type_url` under which a [`proto::ResourceConflictDetail`] is packed.
pub const CONFLICT_DETAIL_TYPE_URL: &str =
    "type.googleapis.com/okFPGAService.v1.ResourceConflictDetail";

// ============================================================================
// google.rpc.Status (local definition)
// ============================================================================

/// Local copy of `google.rpc.Status` (same field numbers as the original).
///
/// `#[derive(prost::Message)]` generates the protobuf encoder/decoder, so
/// this struct is wire-compatible with the official definition.
#[derive(Clone, PartialEq, Message)]
pub struct RpcStatus {
    /// The `google.rpc.Code` value (same numbers as `tonic::Code`).
    #[prost(int32, tag = "1")]
    pub code: i32,
    /// Developer-facing message.
    #[prost(string, tag = "2")]
    pub message: String,
    /// Typed error details.
    #[prost(message, repeated, tag = "3")]
    pub details: Vec<ProtoAny>,
}

/// Local copy of `google.protobuf.Any`: a type URL plus encoded bytes.
#[derive(Clone, PartialEq, Message)]
pub struct ProtoAny {
    /// Identifies the packed message type.
    #[prost(string, tag = "1")]
    pub type_url: String,
    /// The encoded message.
    #[prost(bytes = "vec", tag = "2")]
    pub value: Vec<u8>,
}

// ============================================================================
// Errors
// ============================================================================

/// Convert a core conflict into its protobuf form (empty strings for the
/// absent optional fields, as proto3 has no `None` for strings).
pub fn conflict_to_proto(c: &ResourceConflict) -> proto::ResourceConflictDetail {
    proto::ResourceConflictDetail {
        code: c.code.as_str().to_string(),
        resource_id: c.resource_id.clone().unwrap_or_default(),
        resource_name: c.resource_name.clone().unwrap_or_default(),
        item_type: c.item_type.map(|t| t.as_str().to_string()).unwrap_or_default(),
        item: c.item.clone().unwrap_or_default(),
        items: c.items.clone(),
    }
}

/// Build a `Status`, attaching a `ResourceConflictDetail` when given.
pub fn conflict_status(
    code: Code,
    message: impl Into<String>,
    conflict: Option<&ResourceConflict>,
) -> Status {
    let message = message.into();
    match conflict {
        None => Status::new(code, message),
        Some(c) => {
            let status = RpcStatus {
                code: code as i32,
                message: message.clone(),
                details: vec![ProtoAny {
                    type_url: CONFLICT_DETAIL_TYPE_URL.to_string(),
                    value: conflict_to_proto(c).encode_to_vec(),
                }],
            };
            Status::with_details(
                code,
                message,
                tonic::codegen::Bytes::from(status.encode_to_vec()),
            )
        }
    }
}

/// Map a core [`ResourceError`]: validation → `INVALID_ARGUMENT`, anything
/// else → `FAILED_PRECONDITION`; the conflict detail is always attached.
pub fn resource_error_status(err: &ResourceError) -> Status {
    let code = if err.is_validation() {
        Code::InvalidArgument
    } else {
        Code::FailedPrecondition
    };
    conflict_status(code, err.detail.clone(), Some(&err.conflict))
}

/// Decode the `ResourceConflictDetail` carried by a `Status`, if any.
///
/// The server never needs to decode its own errors, so this is test-only;
/// it doubles as a reference for client authors on how to unpack it.
#[cfg(test)]
pub fn decode_conflict_detail(status: &Status) -> Option<proto::ResourceConflictDetail> {
    let rpc = RpcStatus::decode(status.details()).ok()?;
    rpc.details
        .iter()
        .find(|a| a.type_url == CONFLICT_DETAIL_TYPE_URL)
        .and_then(|a| proto::ResourceConflictDetail::decode(a.value.as_slice()).ok())
}

// ============================================================================
// Enums
// ============================================================================

/// Core state → proto enum value.
pub fn state_to_proto(state: &ResourceState) -> i32 {
    match state {
        ResourceState::Unavailable => proto::ResourceState::Unavailable as i32,
        ResourceState::Initializing => proto::ResourceState::Initializing as i32,
        ResourceState::Available => proto::ResourceState::Available as i32,
        ResourceState::InUseShared => proto::ResourceState::InUseShared as i32,
        ResourceState::InUseLocked => proto::ResourceState::InUseLocked as i32,
    }
}

/// Core holder mode → proto `AccessMode` value.
pub fn holder_mode_to_proto(mode: AccessMode) -> i32 {
    match mode {
        AccessMode::Shared => proto::AccessMode::Shared as i32,
        AccessMode::Exclusive => proto::AccessMode::Exclusive as i32,
        AccessMode::ReadOnly => proto::AccessMode::ReadOnly as i32,
    }
}

/// Proto `AccessMode` value → core holder mode.
///
/// # Errors
/// A validation error for a number that is not a known `AccessMode`.
pub fn holder_mode_from_proto(value: i32) -> Result<AccessMode, ResourceError> {
    match proto::AccessMode::try_from(value) {
        Ok(proto::AccessMode::Shared) => Ok(AccessMode::Shared),
        Ok(proto::AccessMode::Exclusive) => Ok(AccessMode::Exclusive),
        Ok(proto::AccessMode::ReadOnly) => Ok(AccessMode::ReadOnly),
        Err(_) => Err(ResourceError::validation(format!(
            "mode {} is not a valid AccessMode",
            value
        ))),
    }
}

/// Core declared mode → proto `ResourceAccessMode` value.
///
/// `ReadOnly` cannot be declared (the declaration builder rejects it), so
/// it never reaches here; it maps to SHARED defensively.
pub fn declared_mode_to_proto(mode: AccessMode) -> i32 {
    match mode {
        AccessMode::Exclusive => proto::ResourceAccessMode::Exclusive as i32,
        AccessMode::Shared | AccessMode::ReadOnly => proto::ResourceAccessMode::Shared as i32,
    }
}

/// Proto `ResourceAccessMode` value → core declared mode.
///
/// # Errors
/// A validation error naming `field` for an unknown number.
pub fn declared_mode_from_proto(value: i32, field: &str) -> Result<AccessMode, ResourceError> {
    match proto::ResourceAccessMode::try_from(value) {
        Ok(proto::ResourceAccessMode::Shared) => Ok(AccessMode::Shared),
        Ok(proto::ResourceAccessMode::Exclusive) => Ok(AccessMode::Exclusive),
        Err(_) => Err(ResourceError::validation(format!(
            "{} {} is not a valid ResourceAccessMode",
            field, value
        ))),
    }
}

/// Parse an `optional ResourceAccessMode` field (prost gives `Option<i32>`).
fn optional_declared(value: Option<i32>, field: &str) -> Result<Option<AccessMode>, ResourceError> {
    value.map(|v| declared_mode_from_proto(v, field)).transpose()
}

/// Empty proto string → `None`.
fn non_empty(text: String) -> Option<String> {
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

// ============================================================================
// Messages
// ============================================================================

/// Turn a `CreateResourceRequest` into the transport-neutral spec that
/// [`ok_fpga_core::declaration::build_resource`] validates (the same path
/// REST uses).
///
/// # Errors
/// A validation error for an unknown enum number.
pub fn create_request_to_spec(req: proto::CreateResourceRequest) -> Result<ResourceSpec, ResourceError> {
    let endpoints = match req.endpoints {
        // Message presence matters (D68): not set = every classic EP.
        None => None,
        Some(decl) => {
            let mut entries = Vec::with_capacity(decl.entries.len());
            for e in decl.entries {
                entries.push(EndpointEntrySpec {
                    address: e.address,
                    ep_type: e.r#type,
                    access: optional_declared(e.access, "endpoint access")?,
                    private: e.private,
                    description: non_empty(e.description),
                });
            }
            Some(EndpointSpec {
                default_access: optional_declared(decl.default_access, "endpoints.default_access")?,
                entries,
            })
        }
    };

    let mut address_ranges = Vec::with_capacity(req.address_ranges.len());
    for r in req.address_ranges {
        address_ranges.push(AddressRangeSpec {
            name: non_empty(r.name),
            start: r.start,
            end: r.end,
            access: optional_declared(r.access, "address range access")?,
            private: r.private,
            description: non_empty(r.description),
        });
    }

    Ok(ResourceSpec {
        name: req.name,
        description: non_empty(req.description),
        device_serial: req.device_serial,
        access_mode: optional_declared(req.access_mode, "access_mode")?,
        private: req.private,
        interfaces: req.interfaces,
        endpoints,
        address_ranges,
        metadata: req.metadata,
    })
}

/// Core endpoint declaration → proto.
fn endpoints_to_proto(d: &EndpointDeclaration) -> proto::EndpointDeclaration {
    proto::EndpointDeclaration {
        default_access: d.default_access.map(declared_mode_to_proto),
        entries: d
            .entries
            .iter()
            .map(|e| proto::EndpointEntry {
                address: u32::from(e.address),
                r#type: e.ep_type.to_string(),
                access: Some(declared_mode_to_proto(e.access)),
                private: e.private,
                description: e.description.clone().unwrap_or_default(),
            })
            .collect(),
    }
}

/// Core address range → proto.
fn range_to_proto(a: &AddressRange) -> proto::AddressRange {
    proto::AddressRange {
        name: a.name.clone().unwrap_or_default(),
        start: a.start,
        end: a.end,
        access: Some(declared_mode_to_proto(a.access)),
        private: a.private,
        description: a.description.clone().unwrap_or_default(),
    }
}

/// Core resource → proto `Resource` (the gRPC twin of
/// [`crate::views::ResourceView`]).
pub fn resource_to_proto(r: &Resource) -> proto::Resource {
    let mut interfaces = Vec::new();
    if r.interfaces.classic {
        interfaces.push("classic".to_string());
    }
    if r.interfaces.axi_lite {
        interfaces.push("axi_lite".to_string());
    }
    if r.interfaces.axi_full {
        interfaces.push("axi_full".to_string());
    }
    if r.interfaces.axi_stream {
        interfaces.push("axi_stream".to_string());
    }

    proto::Resource {
        id: r.id.clone(),
        name: r.name.clone(),
        description: r.description.clone().unwrap_or_default(),
        device_serial: r.device_serial.clone(),
        state: state_to_proto(&r.state),
        private: r.private,
        locked_by: r.locked_by.clone().unwrap_or_default(),
        holders: r
            .holders
            .iter()
            .map(|h| proto::Holder {
                user_id: h.user_id.clone(),
                mode: holder_mode_to_proto(h.mode),
            })
            .collect(),
        access_mode: r.access_mode.map(declared_mode_to_proto),
        interfaces,
        endpoints: r.endpoints.as_ref().map(endpoints_to_proto),
        address_ranges: r.address_ranges.iter().map(range_to_proto).collect(),
        metadata: r.metadata.clone().into_iter().collect::<HashMap<_, _>>(),
    }
}

/// Core holding → proto.
pub fn holding_to_proto(h: &Holding) -> proto::Holding {
    proto::Holding {
        resource_id: h.resource_id.clone(),
        mode: holder_mode_to_proto(h.mode),
        locked: h.locked,
    }
}

/// User session + holdings → proto `UserSession`.
///
/// The core session does not record a connection time, so `connected_at`
/// carries the last-seen time (same value REST reports as `last_seen_at`).
pub fn user_session_to_proto(s: &UserSession, holdings: &[Holding]) -> proto::UserSession {
    proto::UserSession {
        user_id: s.user_id.clone(),
        connected_at: s.last_seen_at.to_rfc3339(),
        held_resource_ids: s.held_resource_ids.clone(),
        holdings: holdings.iter().map(holding_to_proto).collect(),
    }
}

// ============================================================================
// Tests (Tier 3: pure conversions, no server)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ok_fpga_core::declaration::build_resource;
    use ok_fpga_core::resource::{ConflictCode, ResourceRegistry};

    fn full_request() -> proto::CreateResourceRequest {
        proto::CreateResourceRequest {
            name: "counters".to_string(),
            description: "counter block".to_string(),
            device_serial: "S1".to_string(),
            access_mode: Some(proto::ResourceAccessMode::Shared as i32),
            interfaces: vec!["classic".to_string(), "axi_lite".to_string()],
            endpoints: Some(proto::EndpointDeclaration {
                default_access: Some(proto::ResourceAccessMode::Exclusive as i32),
                entries: vec![
                    proto::EndpointEntry {
                        address: 0x00,
                        r#type: "wire_in".to_string(),
                        access: Some(proto::ResourceAccessMode::Shared as i32),
                        private: true,
                        description: "reset".to_string(),
                    },
                    proto::EndpointEntry {
                        address: 0x20,
                        r#type: "wire_out".to_string(),
                        access: None,
                        private: false,
                        description: String::new(),
                    },
                ],
            }),
            address_ranges: vec![proto::AddressRange {
                name: "regs".to_string(),
                start: 0x44A6_0000,
                end: 0x44A6_FFFF,
                access: Some(proto::ResourceAccessMode::Exclusive as i32),
                private: false,
                description: "AXI regs".to_string(),
            }],
            metadata: HashMap::from([("version".to_string(), "1.2".to_string())]),
            private: true,
        }
    }

    #[test]
    fn test_grpc_register_resource_request_full_schema_roundtrips() {
        let spec = create_request_to_spec(full_request()).unwrap();
        let resource = build_resource(spec).unwrap();
        let p = resource_to_proto(&resource);

        assert_eq!(p.id, "S1:counters");
        assert_eq!(p.description, "counter block");
        assert_eq!(p.state, proto::ResourceState::Available as i32);
        assert_eq!(p.access_mode, Some(proto::ResourceAccessMode::Shared as i32));
        assert!(p.private);
        assert_eq!(p.interfaces, vec!["classic", "axi_lite"]);
        let eps = p.endpoints.expect("endpoints present");
        assert_eq!(eps.default_access, Some(proto::ResourceAccessMode::Exclusive as i32));
        assert_eq!(eps.entries[0].address, 0x00);
        assert_eq!(eps.entries[0].r#type, "wire_in");
        assert_eq!(eps.entries[0].access, Some(proto::ResourceAccessMode::Shared as i32));
        assert!(eps.entries[0].private);
        assert_eq!(eps.entries[0].description, "reset");
        // D61: the entry without `access` takes `default_access`.
        assert_eq!(eps.entries[1].access, Some(proto::ResourceAccessMode::Exclusive as i32));
        assert_eq!(p.address_ranges[0].name, "regs");
        assert_eq!(p.address_ranges[0].start, 0x44A6_0000);
        assert_eq!(p.address_ranges[0].end, 0x44A6_FFFF);
        assert_eq!(p.address_ranges[0].description, "AXI regs");
        assert_eq!(p.metadata.get("version").map(String::as_str), Some("1.2"));
    }

    #[test]
    fn test_create_request_to_spec_endpoints_unset_stays_none() {
        let mut req = full_request();
        req.endpoints = None;
        req.access_mode = None;
        let resource = build_resource(create_request_to_spec(req).unwrap()).unwrap();
        // D68: no endpoints → exclusive, every classic EP.
        assert!(resource.endpoints.is_none());
        assert_eq!(resource.access_mode, Some(AccessMode::Exclusive));
    }

    #[test]
    fn test_create_request_to_spec_bad_enum_is_validation() {
        let mut req = full_request();
        req.access_mode = Some(7);
        let err = create_request_to_spec(req).unwrap_err();
        assert!(err.is_validation());
    }

    #[test]
    fn test_grpc_user_session_holdings_include_mode() {
        let registry = ResourceRegistry::new();
        registry.register(Resource::new(
            "S:r".to_string(),
            "r".to_string(),
            None,
            "S".to_string(),
            AccessMode::Shared,
        ));
        registry
            .acquire("S:r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("S:r", &"alice".to_string()).unwrap();

        let session = UserSession::new("alice");
        let p = user_session_to_proto(&session, &registry.holdings_for("alice"));
        assert_eq!(p.holdings.len(), 1);
        assert_eq!(p.holdings[0].resource_id, "S:r");
        assert_eq!(p.holdings[0].mode, proto::AccessMode::Shared as i32);
        assert!(p.holdings[0].locked);
    }

    #[test]
    fn test_resource_to_proto_holders_include_mode() {
        let registry = ResourceRegistry::new();
        let mut r = Resource::new(
            "S:x".to_string(),
            "x".to_string(),
            None,
            "S".to_string(),
            AccessMode::Exclusive,
        );
        r.private = false;
        registry.register(r);
        registry
            .acquire("S:x", &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap();
        let p = resource_to_proto(&registry.get("S:x").unwrap());
        assert_eq!(p.holders.len(), 1);
        assert_eq!(p.holders[0].user_id, "bob");
        assert_eq!(p.holders[0].mode, proto::AccessMode::ReadOnly as i32);
    }

    #[test]
    fn test_grpc_status_details_decode_resource_conflict_detail() {
        let err = ResourceError {
            detail: "overlap".to_string(),
            conflict: ResourceConflict::new(ConflictCode::Overlap)
                .with_resource("S:a", "a")
                .with_endpoint(0x05),
        };
        let status = resource_error_status(&err);
        assert_eq!(status.code(), Code::FailedPrecondition);
        let detail = decode_conflict_detail(&status).expect("detail attached");
        assert_eq!(detail.code, "overlap");
        assert_eq!(detail.resource_id, "S:a");
        assert_eq!(detail.resource_name, "a");
        assert_eq!(detail.item_type, "endpoint");
        assert_eq!(detail.item, "0x05");
    }

    #[test]
    fn test_resource_error_status_validation_is_invalid_argument() {
        let status = resource_error_status(&ResourceError::validation("bad"));
        assert_eq!(status.code(), Code::InvalidArgument);
        assert_eq!(decode_conflict_detail(&status).unwrap().code, "validation");
    }
}
