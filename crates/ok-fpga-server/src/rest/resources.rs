//! # Resource Routes
//!
//! REST endpoints for multi-user resource management.
//! Implements the `/resources/*` paths from ADR-004 and ADR-020.
//!
//! ## Client-Side Resource Registration (ADR-020)
//!
//! Resources are created by clients via `POST /resources` after programming
//! a bitfile. The server is agnostic to bitfile capabilities — only the
//! client knows what resources the bitfile exposes.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use ok_fpga_core::resource::{
    AccessMode, AddressRange, EndpointDeclaration, EndpointEntry, EndpointType,
    InterfaceDeclaration, Resource,
};

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/resources/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/resources", get(list_resources).post(create_resource))
        .route(
            "/resources/{id}",
            get(get_resource).delete(delete_resource),
        )
        .route("/resources/{id}/acquire", post(acquire_resource))
        .route("/resources/{id}/release", post(release_resource))
        .route("/resources/{id}/lock", post(lock_resource))
        .route("/resources/{id}/unlock", post(unlock_resource))
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request body for creating a new resource (POST /resources, ADR-020 §2).
#[derive(Deserialize)]
struct CreateResourceRequest {
    /// Human-readable resource name (e.g., "xem7320-classic-counters").
    name: String,
    /// Optional description of the resource.
    description: Option<String>,
    /// Serial of the FPGA that provides this resource.
    device_serial: String,
    /// Access mode constraint: "shared" or "exclusive" (ADR-020 §3).
    access_mode: String,
    /// Which data port interfaces this resource uses (ADR-020 §6).
    #[serde(default)]
    interfaces: Option<InterfacesBody>,
    /// Classic endpoint declarations (ADR-020 §7).
    #[serde(default)]
    endpoints: Option<EndpointsBody>,
    /// 32-bit address ranges (ADR-020 §8).
    #[serde(default)]
    address_ranges: Vec<AddressRangeBody>,
    /// Extensible key-value metadata (ADR-020 §9).
    #[serde(default)]
    metadata: HashMap<String, String>,
}

/// Interfaces sub-object in CreateResourceRequest.
#[derive(Deserialize)]
struct InterfacesBody {
    #[serde(default)]
    classic: bool,
    #[serde(default)]
    axi_lite: bool,
    #[serde(default)]
    axi_full: bool,
    #[serde(default)]
    axi_stream: bool,
}

/// Endpoints sub-object in CreateResourceRequest.
#[derive(Deserialize)]
struct EndpointsBody {
    #[serde(default = "default_exclusive")]
    default_access: String,
    #[serde(default)]
    entries: Vec<EndpointEntryBody>,
}

fn default_exclusive() -> String {
    "exclusive".to_string()
}

/// A single endpoint entry in the request body.
#[derive(Deserialize)]
struct EndpointEntryBody {
    /// Hex address string, e.g. "0x00".
    address: String,
    /// Endpoint type: "wire_in", "wire_out", "trigger_in", "trigger_out",
    /// "pipe_in", "pipe_out", "bt_pipe_in", "bt_pipe_out".
    #[serde(rename = "type")]
    ep_type: String,
    /// Access mode: "shared" or "exclusive".
    #[serde(default = "default_exclusive")]
    access: String,
    /// Optional description.
    description: Option<String>,
}

/// Address range entry in the request body.
#[derive(Deserialize)]
struct AddressRangeBody {
    /// Hex start address, e.g. "0x44A60000".
    start: String,
    /// Hex end address, e.g. "0x44A8FFFF".
    end: String,
    /// Access mode: "shared" or "exclusive".
    #[serde(default = "default_exclusive")]
    access: String,
}

/// Request body for acquiring a resource.
#[derive(Deserialize)]
struct AcquireResourceRequest {
    /// User identifier.
    user_id: String,
    /// Access mode: "shared" or "exclusive" (optional, defaults to the
    /// resource's declared access_mode).
    mode: Option<String>,
}

/// Request body for releasing a resource.
#[derive(Deserialize)]
struct ReleaseResourceRequest {
    /// User identifier.
    user_id: String,
}

/// Request body for lock/unlock operations.
#[derive(Deserialize)]
struct LockUnlockRequest {
    /// User identifier.
    user_id: String,
}

/// Resource list response.
#[derive(Serialize)]
struct ResourceListBody {
    resources: Vec<ResourceView>,
}

/// Serializable view of a resource for API responses.
#[derive(Serialize)]
struct ResourceView {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    device_serial: String,
    state: String,
    holders: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locked_by: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    metadata: HashMap<String, String>,
}

impl From<&Resource> for ResourceView {
    fn from(r: &Resource) -> Self {
        ResourceView {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            device_serial: r.device_serial.clone(),
            state: format!("{}", r.state),
            holders: r.holders.clone(),
            access_mode: r.access_mode.map(|m| format!("{}", m)),
            locked_by: r.locked_by.clone(),
            metadata: r.metadata.clone(),
        }
    }
}

// ============================================================================
// Parsing Helpers
// ============================================================================

/// Parse an access mode string into an `AccessMode` enum.
fn parse_access_mode(s: &str) -> Result<AccessMode, ProblemDetail> {
    match s {
        "shared" => Ok(AccessMode::Shared),
        "exclusive" => Ok(AccessMode::Exclusive),
        _ => Err(ProblemDetail::bad_request(
            "access_mode must be 'shared' or 'exclusive'",
        )),
    }
}

/// Parse a hex address string (e.g., "0x00" or "0xA0") into a u8.
fn parse_ep_address(s: &str) -> Result<u8, ProblemDetail> {
    let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u8::from_str_radix(stripped, 16).map_err(|_| {
        ProblemDetail::bad_request(&format!("Invalid endpoint address: '{}' (expected 0x00–0xBF)", s))
    })
}

/// Parse a hex address string into a u32 (for address ranges).
fn parse_u32_hex(s: &str) -> Result<u32, ProblemDetail> {
    let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    // Remove underscores (allow "0x44A6_0000" formatting).
    let clean: String = stripped.chars().filter(|c| *c != '_').collect();
    u32::from_str_radix(&clean, 16).map_err(|_| {
        ProblemDetail::bad_request(&format!("Invalid hex address: '{}' (expected 32-bit hex)", s))
    })
}

/// Parse an endpoint type string into an `EndpointType` enum.
fn parse_ep_type(s: &str) -> Result<EndpointType, ProblemDetail> {
    match s {
        "wire_in" => Ok(EndpointType::WireIn),
        "wire_out" => Ok(EndpointType::WireOut),
        "trigger_in" => Ok(EndpointType::TriggerIn),
        "trigger_out" => Ok(EndpointType::TriggerOut),
        "pipe_in" => Ok(EndpointType::PipeIn),
        "pipe_out" => Ok(EndpointType::PipeOut),
        "bt_pipe_in" => Ok(EndpointType::BtPipeIn),
        "bt_pipe_out" => Ok(EndpointType::BtPipeOut),
        _ => Err(ProblemDetail::bad_request(&format!(
            "Invalid endpoint type: '{}'. Expected one of: wire_in, wire_out, trigger_in, \
             trigger_out, pipe_in, pipe_out, bt_pipe_in, bt_pipe_out",
            s
        ))),
    }
}

// ============================================================================
// Route Handlers
// ============================================================================

/// `GET /resources` — List all registered resources across all devices.
async fn list_resources(State(state): State<Arc<AppState>>) -> Json<ResourceListBody> {
    let resources = state.resource_registry.list();
    let views = resources.iter().map(ResourceView::from).collect();
    Json(ResourceListBody { resources: views })
}

/// `GET /resources/{id}` — Get a specific resource by ID.
async fn get_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    match state.resource_registry.get(&id) {
        Some(resource) => Ok(Json(ResourceView::from(&resource))),
        None => Err(ProblemDetail::not_found(&format!(
            "Resource '{}' not found",
            id
        ))),
    }
}

/// `POST /resources` — Create a new resource (client-side registration, ADR-020).
///
/// The client creates resources after programming a bitfile, since the client
/// knows what capabilities the bitfile exposes. The server validates that the
/// referenced device exists and is programmed, that interfaces are supported,
/// that EP types match address ranges, and that exclusive EPs/ranges don't
/// overlap with existing resources.
async fn create_resource(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateResourceRequest>,
) -> Result<(StatusCode, Json<ResourceView>), ProblemDetail> {
    // 1. Validate the device exists and is programmed.
    let registry = state.device_manager.registry();
    match registry.get(&body.device_serial) {
        Some(dev) => {
            if !dev.is_programmed() {
                return Err(ProblemDetail::conflict(
                    "FPGA is not programmed — program it first with POST /configure",
                ));
            }
        }
        None => {
            return Err(ProblemDetail::not_found(&format!(
                "Device '{}' not found",
                body.device_serial
            )));
        }
    }

    // 2. Parse access mode.
    let access_mode = parse_access_mode(&body.access_mode)?;

    // 3. Parse interface declarations.
    let interfaces = match body.interfaces {
        Some(ifaces) => InterfaceDeclaration {
            classic: ifaces.classic,
            axi_lite: ifaces.axi_lite,
            axi_full: ifaces.axi_full,
            axi_stream: ifaces.axi_stream,
        },
        None => InterfaceDeclaration::default(),
    };

    if !interfaces.has_any() {
        return Err(ProblemDetail::bad_request(
            "At least one interface must be enabled (classic, axi_lite, axi_full, or axi_stream)",
        ));
    }

    // 4. Parse endpoint declarations (if classic is enabled).
    let endpoints = if let Some(ep_body) = body.endpoints {
        let default_access = parse_access_mode(&ep_body.default_access)?;
        let mut entries = Vec::new();
        for entry in ep_body.entries {
            let address = parse_ep_address(&entry.address)?;
            let ep_type = parse_ep_type(&entry.ep_type)?;
            let ep_access = parse_access_mode(&entry.access)?;

            // Validate type matches address range (ADR-020 §7).
            if !ep_type.is_valid_address(address) {
                let (lo, hi) = ep_type.address_range();
                return Err(ProblemDetail::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!(
                        "Endpoint 0x{:02X} is not in the {} range (0x{:02X}–0x{:02X})",
                        address, ep_type, lo, hi
                    ),
                ));
            }

            entries.push(EndpointEntry {
                address,
                ep_type,
                access: ep_access,
                description: entry.description,
            });
        }
        Some(EndpointDeclaration {
            default_access,
            entries,
        })
    } else {
        None
    };

    // 5. Parse address ranges.
    let mut address_ranges = Vec::new();
    for ar_body in &body.address_ranges {
        let start = parse_u32_hex(&ar_body.start)?;
        let end = parse_u32_hex(&ar_body.end)?;
        let ar_access = parse_access_mode(&ar_body.access)?;

        if start > end {
            return Err(ProblemDetail::bad_request(&format!(
                "Address range start (0x{:08X}) must be <= end (0x{:08X})",
                start, end
            )));
        }

        address_ranges.push(AddressRange {
            start,
            end,
            access: ar_access,
        });
    }

    // 6. Check for EP overlap with existing resources on the same device.
    if let Some(ref ep_decl) = endpoints {
        let existing_resources = state.resource_registry.list_for_device(&body.device_serial);
        for new_ep in &ep_decl.entries {
            if new_ep.access == AccessMode::Exclusive {
                for existing in &existing_resources {
                    if let Some(ref existing_eps) = existing.endpoints {
                        for existing_ep in &existing_eps.entries {
                            if existing_ep.address == new_ep.address
                                && existing_ep.access == AccessMode::Exclusive
                            {
                                return Err(ProblemDetail::conflict(&format!(
                                    "Endpoint 0x{:02X} is already declared exclusive by resource '{}'",
                                    new_ep.address, existing.name
                                )));
                            }
                        }
                    }
                }
            }
        }
    }

    // 7. Check for address range overlap with existing exclusive ranges.
    for new_range in &address_ranges {
        if new_range.access == AccessMode::Exclusive {
            let existing_resources = state.resource_registry.list_for_device(&body.device_serial);
            for existing in &existing_resources {
                for existing_range in &existing.address_ranges {
                    if existing_range.access == AccessMode::Exclusive
                        && new_range.overlaps(existing_range)
                    {
                        return Err(ProblemDetail::conflict(&format!(
                            "Address range 0x{:08X}–0x{:08X} overlaps with exclusive range in resource '{}'",
                            new_range.start, new_range.end, existing.name
                        )));
                    }
                }
            }
        }
    }

    // 8. Generate resource ID: {serial}:{name}
    let resource_id = format!("{}:{}", body.device_serial, body.name);

    let mut resource = Resource::new(
        resource_id.clone(),
        body.name.clone(),
        body.description.clone(),
        body.device_serial.clone(),
        access_mode,
    );
    resource.interfaces = interfaces;
    resource.endpoints = endpoints;
    resource.address_ranges = address_ranges;
    resource.metadata = body.metadata;

    if state.resource_registry.register(resource) {
        let created = state.resource_registry.get(&resource_id).unwrap();

        // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
        state.broadcast_event(&format!(
            r#"{{"type":"resource_registered","resource_id":"{}","device_serial":"{}"}}"#,
            resource_id, body.device_serial
        ));

        Ok((StatusCode::CREATED, Json(ResourceView::from(&created))))
    } else {
        Err(ProblemDetail::conflict(&format!(
            "Resource '{}' already exists for device '{}'",
            body.name, body.device_serial
        )))
    }
}

/// `DELETE /resources/{id}` — Delete a resource definition.
///
/// Returns 409 if the resource is currently held by any user.
/// Returns 204 No Content on success.
async fn delete_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, ProblemDetail> {
    // Check if resource exists and is not currently held.
    match state.resource_registry.get(&id) {
        Some(resource) => {
            if !resource.holders.is_empty() {
                return Err(ProblemDetail::conflict(&format!(
                    "Resource '{}' is currently held by {} user(s) — release it first",
                    id,
                    resource.holders.len()
                )));
            }
        }
        None => {
            return Err(ProblemDetail::not_found(&format!(
                "Resource '{}' not found",
                id
            )));
        }
    }

    state.resource_registry.remove(&id);

    // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_deleted","resource_id":"{}"}}"#,
        id
    ));

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /resources/{id}/acquire` — Acquire a resource for a user.
///
/// The access mode is determined by the resource's `access_mode` field
/// (set at registration time, ADR-020 §3), not by the caller.
async fn acquire_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AcquireResourceRequest>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    // Look up the resource's declared access mode.
    let resource = state
        .resource_registry
        .get(&id)
        .ok_or_else(|| ProblemDetail::not_found(&format!("Resource '{}' not found", id)))?;

    let mode = resource.access_mode.unwrap_or_else(|| {
        // Fallback for legacy resources: parse from request body.
        body.mode
            .as_deref()
            .and_then(|m| match m {
                "shared" => Some(AccessMode::Shared),
                "exclusive" => Some(AccessMode::Exclusive),
                _ => None,
            })
            .unwrap_or(AccessMode::Shared)
    });

    state
        .resource_registry
        .acquire(&id, &body.user_id, mode)
        .map_err(|e| ProblemDetail::conflict(&format!("{}", e)))?;

    // Track the user session (implicit heartbeat on acquire).
    state.user_sessions.touch(&body.user_id);

    let resource = state.resource_registry.get(&id).unwrap();

    // Broadcast ResourceChanged event (Option B — handlers broadcast).
    let _ = state.device_manager.broadcast(
        ok_fpga_core::manager::DeviceEvent::ResourceChanged {
            resource_id: id.clone(),
            new_state: format!("{:?}", resource.state),
            timestamp: chrono::Utc::now(),
        },
    );

    // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_acquired","resource_id":"{}","user_id":"{}"}}"#,
        id, body.user_id
    ));

    Ok(Json(ResourceView::from(&resource)))
}

/// `POST /resources/{id}/release` — Release a previously acquired resource.
async fn release_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ReleaseResourceRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    if state.resource_registry.release(&id, &body.user_id) {
        // Track the user session (implicit heartbeat on release).
        state.user_sessions.touch(&body.user_id);

        // Determine new state after release.
        let new_state = state
            .resource_registry
            .get(&id)
            .map(|r| format!("{:?}", r.state))
            .unwrap_or_else(|| "Available".to_string());

        // Broadcast ResourceChanged event (Option B).
        let _ = state.device_manager.broadcast(
            ok_fpga_core::manager::DeviceEvent::ResourceChanged {
                resource_id: id.clone(),
                new_state,
                timestamp: chrono::Utc::now(),
            },
        );

        // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
        state.broadcast_event(&format!(
            r#"{{"type":"resource_released","resource_id":"{}","user_id":"{}"}}"#,
            id, body.user_id
        ));

        Ok(Json(serde_json::json!({
            "status": "released",
            "resource_id": id,
            "user_id": body.user_id,
        })))
    } else {
        Err(ProblemDetail::not_found(&format!(
            "Resource '{}' not found or user '{}' was not holding it",
            id, body.user_id
        )))
    }
}

/// `POST /resources/{id}/lock` — Lock a shared resource (ADR-020 §3).
///
/// A holder of a shared resource can temporarily lock it for exclusive use.
/// Other holders remain in the holders list but cannot operate until unlock.
async fn lock_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<LockUnlockRequest>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    state
        .resource_registry
        .lock(&id, &body.user_id)
        .map_err(|e| ProblemDetail::conflict(&e))?;

    let resource = state.resource_registry.get(&id).unwrap();

    // Broadcast ResourceChanged event (Option B).
    let _ = state.device_manager.broadcast(
        ok_fpga_core::manager::DeviceEvent::ResourceChanged {
            resource_id: id.clone(),
            new_state: "In Use (Locked)".to_string(),
            timestamp: chrono::Utc::now(),
        },
    );

    // Broadcast event for dashboard auto-refresh (Phase 28D — was missing).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_locked","resource_id":"{}","user_id":"{}"}}"#,
        id, body.user_id
    ));

    Ok(Json(ResourceView::from(&resource)))
}

/// `POST /resources/{id}/unlock` — Unlock a previously locked resource (ADR-020 §3).
///
/// Only the user who locked the resource can unlock it.
async fn unlock_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<LockUnlockRequest>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    state
        .resource_registry
        .unlock(&id, &body.user_id)
        .map_err(|e| ProblemDetail::conflict(&e))?;

    let resource = state.resource_registry.get(&id).unwrap();

    // Broadcast ResourceChanged event (Option B).
    let _ = state.device_manager.broadcast(
        ok_fpga_core::manager::DeviceEvent::ResourceChanged {
            resource_id: id.clone(),
            new_state: "In Use (Shared)".to_string(),
            timestamp: chrono::Utc::now(),
        },
    );

    // Broadcast event for dashboard auto-refresh (Phase 28D — was missing).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
        id, body.user_id
    ));

    Ok(Json(ResourceView::from(&resource)))
}
