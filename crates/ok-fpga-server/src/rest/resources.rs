//! # Resource Routes
//!
//! REST endpoints for multi-user resource management.
//! Implements the `/resources/*` paths from ADR-004, ADR-020 and ADR-034.
//!
//! ## Client-Side Resource Registration (ADR-020, ADR-034)
//!
//! Resources are created by clients via `POST /resources` after programming
//! a bitfile. The server is agnostic to bitfile capabilities — only the
//! client knows what resources the bitfile exposes.
//!
//! ## How a request flows (ADR-034)
//!
//! 1. The JSON body is decoded into a *request* struct whose fields are the
//!    wire strings (`"0x05"`, `"shared"`, ...).
//! 2. The request is converted into a transport-neutral
//!    [`ResourceSpec`](ok_fpga_core::declaration::ResourceSpec) — the same type
//!    the gRPC service builds — and
//!    [`build_resource`](ok_fpga_core::declaration::build_resource) applies
//!    every shape rule (including "an exclusive resource contains only
//!    exclusive items" and "each endpoint address once"). Any problem is a
//!    **422** with a `conflict` object whose `code` is `validation`.
//! 3. The device must exist (404). Its data port must support the declared
//!    interfaces: `classic` always, `axi_*` only on an AXI data-port device
//!    (422, SD-09). Only then must it be programmed (409), so a bad
//!    declaration is never hidden behind "not programmed".
//! 4. [`ResourceRegistry::try_register`] runs the overlap check (R1) and the
//!    insert under one lock. An overlap is a **409** whose `conflict` names
//!    the existing resource, the first clashing item and, in `items`, every
//!    clashing item (D73, 44-B).
//! 5. Ranges of one declaration that overlap each other are accepted; the
//!    server logs a warning when `warn_range_self_overlap` is on.
//!
//! Responses use the shared [`ResourceView`] so REST, the dashboard API and
//! the docs all show the same JSON.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use ok_fpga_core::declaration::{
    build_resource, check_interfaces_for_dataport, check_overlap, parse_declared_mode,
    parse_ep_address, parse_hex_u32, AddressRangeSpec, EndpointEntrySpec, EndpointSpec,
    ResourceSpec,
};
use ok_fpga_core::manager::{DeleteReason, DeviceEvent};
use ok_fpga_core::resource::{
    AccessMode, AcquireOutcome, LockError, LockOutcome, RegisterError, ResourceError,
};

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;
use crate::views::ResourceView;

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

/// Request body for creating a new resource (OpenAPI `CreateResourceRequest`).
///
/// Fields hold the raw wire values; [`CreateResourceRequest::into_spec`]
/// parses them. Unknown fields are ignored (serde's default), so a client
/// that still sends the retired `resource_type` / `capabilities` fields is
/// not rejected for that alone.
#[derive(Deserialize)]
struct CreateResourceRequest {
    /// Human-readable resource name (e.g., "xem7320-classic-counters").
    name: String,
    /// Optional description of the resource.
    #[serde(default)]
    description: Option<String>,
    /// Serial of the FPGA that provides this resource.
    device_serial: String,
    /// Declared mode: "shared" or "exclusive". May be omitted only when
    /// `endpoints` is omitted too (D62/D68: it then defaults to exclusive).
    #[serde(default)]
    access_mode: Option<String>,
    /// Resource-level `private` flag (an access rule, not visibility).
    #[serde(default)]
    private: bool,
    /// Data-port interfaces this resource uses, e.g. `["classic"]`.
    #[serde(default)]
    interfaces: Vec<String>,
    /// Classic endpoint declaration. Absent = the resource claims every
    /// classic endpoint (D68).
    #[serde(default)]
    endpoints: Option<EndpointsBody>,
    /// 32-bit address ranges (ADR-020 §8).
    #[serde(default)]
    address_ranges: Vec<AddressRangeBody>,
    /// Text metadata (D69). Typed as JSON values so a non-string value can
    /// be reported as a clear 422 instead of a generic decode error.
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

/// The `endpoints` object of a create request.
#[derive(Deserialize)]
struct EndpointsBody {
    /// Mode used for entries that have no `access` of their own (D61).
    #[serde(default)]
    default_access: Option<String>,
    /// Declared entries.
    #[serde(default)]
    entries: Vec<EndpointEntryBody>,
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
    /// "shared" or "exclusive"; falls back to `default_access`.
    #[serde(default)]
    access: Option<String>,
    /// Item-level `private` flag.
    #[serde(default)]
    private: bool,
    /// Optional description.
    #[serde(default)]
    description: Option<String>,
}

/// Address range entry in the request body.
#[derive(Deserialize)]
struct AddressRangeBody {
    /// Optional range name.
    #[serde(default)]
    name: Option<String>,
    /// Hex start address, e.g. "0x44A60000".
    start: String,
    /// Hex end address (inclusive), e.g. "0x44A8FFFF".
    end: String,
    /// "shared" or "exclusive" (required; checked in `build_resource`).
    #[serde(default)]
    access: Option<String>,
    /// Item-level `private` flag.
    #[serde(default)]
    private: bool,
    /// Optional description.
    #[serde(default)]
    description: Option<String>,
}

impl CreateResourceRequest {
    /// Parse the wire strings into a transport-neutral [`ResourceSpec`].
    ///
    /// # Errors
    /// A validation [`ResourceError`] for a bad mode, hex address or
    /// non-string metadata value.
    fn into_spec(self) -> Result<ResourceSpec, ResourceError> {
        // `Option::map(..).transpose()` turns `Option<Result<T>>` into
        // `Result<Option<T>>` so `?` can bail out on a parse error while
        // still allowing the field to be absent.
        let access_mode = self
            .access_mode
            .as_deref()
            .map(|m| parse_declared_mode(m, "access_mode"))
            .transpose()?;

        let endpoints = match self.endpoints {
            None => None,
            Some(body) => {
                let default_access = body
                    .default_access
                    .as_deref()
                    .map(|m| parse_declared_mode(m, "endpoints.default_access"))
                    .transpose()?;
                let mut entries = Vec::with_capacity(body.entries.len());
                for entry in body.entries {
                    entries.push(EndpointEntrySpec {
                        // Strict OpenAPI pattern `^0x[0-9A-Fa-f]{2}$`: `5` or
                        // `0x5` are rejected here (gRPC sends a number).
                        address: parse_ep_address(&entry.address, "endpoint address")?,
                        ep_type: entry.ep_type,
                        access: entry
                            .access
                            .as_deref()
                            .map(|m| parse_declared_mode(m, "endpoint access"))
                            .transpose()?,
                        private: entry.private,
                        description: entry.description,
                    });
                }
                Some(EndpointSpec {
                    default_access,
                    entries,
                })
            }
        };

        let mut address_ranges = Vec::with_capacity(self.address_ranges.len());
        for range in self.address_ranges {
            address_ranges.push(AddressRangeSpec {
                name: range.name,
                start: parse_hex_u32(&range.start, "address range start")?,
                end: parse_hex_u32(&range.end, "address range end")?,
                access: range
                    .access
                    .as_deref()
                    .map(|m| parse_declared_mode(m, "address range access"))
                    .transpose()?,
                private: range.private,
                description: range.description,
            });
        }

        let mut metadata = HashMap::with_capacity(self.metadata.len());
        for (key, value) in self.metadata {
            match value {
                serde_json::Value::String(text) => {
                    metadata.insert(key, text);
                }
                _ => {
                    return Err(ResourceError::validation(format!(
                        "metadata values must be strings (key '{}')",
                        key
                    )))
                }
            }
        }

        Ok(ResourceSpec {
            name: self.name,
            description: self.description,
            device_serial: self.device_serial,
            access_mode,
            private: self.private,
            interfaces: self.interfaces,
            endpoints,
            address_ranges,
            metadata,
        })
    }
}

/// Request body for acquiring a resource.
///
/// `user_id` is optional in the Rust struct (ADR-029): if absent, the handler
/// falls back to the `X-User-Id` header.
#[derive(Deserialize)]
struct AcquireResourceRequest {
    /// User identifier (optional — falls back to X-User-Id header).
    #[serde(default)]
    user_id: Option<String>,
    /// Requested mode: "shared", "exclusive" or "read_only". Optional; the
    /// resource's declared mode is used when omitted.
    #[serde(default)]
    mode: Option<String>,
}

/// Request body for releasing a resource.
///
/// `user_id` is optional in the Rust struct (ADR-029): if absent, the handler
/// falls back to the `X-User-Id` header.
#[derive(Deserialize)]
struct ReleaseResourceRequest {
    /// User identifier (optional — falls back to X-User-Id header).
    user_id: Option<String>,
}

/// Request body for lock/unlock operations.
///
/// `user_id` is optional in the Rust struct (ADR-029): if absent, the handler
/// falls back to the `X-User-Id` header.
#[derive(Deserialize)]
struct LockUnlockRequest {
    /// User identifier (optional — falls back to X-User-Id header).
    user_id: Option<String>,
}

/// Resource list response.
#[derive(Serialize)]
struct ResourceListBody {
    resources: Vec<ResourceView>,
}

// ============================================================================
// Helpers
// ============================================================================

/// Decode a JSON body, reporting any problem as a 422 ProblemDetail.
///
/// axum's `Json` extractor answers malformed JSON with a plain-text 400 (or
/// 422 for type errors). The spec promises a ProblemDetail with
/// `conflict.code = "validation"` for every invalid create/acquire body, so
/// these handlers take the raw bytes and decode them here instead.
fn decode_json<T: DeserializeOwned>(body: &Bytes) -> Result<T, ProblemDetail> {
    serde_json::from_slice(body).map_err(|e| {
        ProblemDetail::from_resource_error(ResourceError::validation(format!(
            "invalid request body: {}",
            e
        )))
    })
}

/// Broadcast a `ResourceChanged` event carrying the resource's current
/// spec state string (e.g. `in_use_locked`).
fn emit_resource_changed(state: &AppState, id: &str) {
    if let Some(resource) = state.resource_registry.get(id) {
        let _ = state.device_manager.broadcast(DeviceEvent::ResourceChanged {
            resource_id: id.to_string(),
            new_state: resource.state.as_str().to_string(),
            timestamp: chrono::Utc::now(),
        });
    }
}

/// Turn a [`LockError`] into a [`ProblemDetail`] (404 or 409, with `conflict`
/// attached for `locked` and `blocked`).
fn lock_failure(err: LockError) -> ProblemDetail {
    match err {
        LockError::NotFound(detail) => ProblemDetail::not_found(detail),
        LockError::PreconditionFailed(detail) => ProblemDetail::conflict(detail),
        LockError::Conflict(res_err) => ProblemDetail::from_resource_error(res_err),
    }
}

// ============================================================================
// Route Handlers
// ============================================================================

/// `GET /resources` — List all registered resources across all devices.
async fn list_resources(State(state): State<Arc<AppState>>) -> Json<ResourceListBody> {
    let mut resources = state.resource_registry.list();
    // Sort for stable output (the registry is a HashMap).
    resources.sort_by(|a, b| a.id.cmp(&b.id));
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
        None => Err(ProblemDetail::not_found(format!(
            "Resource '{}' not found",
            id
        ))),
    }
}

/// `POST /resources` — Create a new resource (client-side registration).
///
/// Status codes (OpenAPI): 201 + `Location` header, 404 unknown device,
/// 409 device not programmed / duplicate name / overlap (with `conflict`),
/// 422 invalid declaration (with `conflict.code = "validation"`).
async fn create_resource(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, ProblemDetail> {
    // 1 + 2. Decode and validate the declaration (422 on any problem).
    let request: CreateResourceRequest = decode_json(&body)?;
    let spec = request.into_spec().map_err(ProblemDetail::from_resource_error)?;
    let resource = build_resource(spec).map_err(ProblemDetail::from_resource_error)?;

    // 3. The device must exist; its data port must support the declared
    //    interfaces (422); only then must it be programmed (409).
    match state.device_manager.registry().get(&resource.device_serial) {
        Some(dev) => {
            check_interfaces_for_dataport(&resource.interfaces, dev.info.dataport)
                .map_err(ProblemDetail::from_resource_error)?;
            if !dev.is_programmed() {
                return Err(ProblemDetail::conflict(
                    "FPGA is not programmed -- program it first with POST /configure or /configure_from_server_path",
                ));
            }
        }
        None => {
            return Err(ProblemDetail::not_found(format!(
                "Device '{}' not found",
                resource.device_serial
            )));
        }
    }

    // Ranges of one declaration may overlap each other (owner decision Q1);
    // that is allowed, so it is only a warning, and only if enabled.
    let (warn_self_overlap, warn_readonly_side_effects) = state
        .config
        .read()
        .map(|c| {
            (
                c.access_control.warn_range_self_overlap,
                c.access_control.warn_readonly_side_effects,
            )
        })
        .unwrap_or((true, true));
    if warn_self_overlap {
        crate::warnings::warn_self_overlapping_ranges(&resource);
    }

    // 4. Overlap check + insert, atomically.
    let resource_id = resource.id.clone();
    let device_serial = resource.device_serial.clone();
    let name = resource.name.clone();
    match state.resource_registry.try_register(resource, check_overlap) {
        Ok(()) => {}
        Err(RegisterError::Duplicate) => {
            return Err(ProblemDetail::conflict(format!(
                "Resource '{}' already exists for device '{}'",
                name, device_serial
            )));
        }
        Err(RegisterError::Rejected(err)) => {
            return Err(ProblemDetail::from_resource_error(err));
        }
    }

    let created = state
        .resource_registry
        .get(&resource_id)
        .ok_or_else(|| ProblemDetail::internal("resource vanished after registration"))?;

    if warn_readonly_side_effects {
        crate::warnings::warn_readonly_side_effects(&created);
    }
    let existing_on_device = state.resource_registry.list_for_device(&device_serial);
    crate::warnings::warn_wildcard_endpoints(&existing_on_device, &created);

    // Broadcast DeviceEvent for SSE subscribers (Phase 39, ADR-030).
    let _ = state.device_manager.broadcast(DeviceEvent::ResourceCreated {
        resource_id: resource_id.clone(),
        device_serial: device_serial.clone(),
        name,
        timestamp: chrono::Utc::now(),
    });

    // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_registered","resource_id":"{}","device_serial":"{}"}}"#,
        resource_id, device_serial
    ));

    // `Location` points at the new resource (the REST API is nested under
    // `/api/v1`). Resource IDs are `{serial}:{name}` and the name is free
    // text, so the ID is percent-encoded as one path segment. The result is
    // plain ASCII, so it is always a valid header value; Axum's `Path`
    // extractor decodes it again on `GET /resources/{id}`.
    let mut response = (StatusCode::CREATED, Json(ResourceView::from(&created))).into_response();
    let location = format!("/api/v1/resources/{}", encode_path_segment(&resource_id));
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    Ok(response)
}

/// Percent-encode `text` for use as a single URL path segment.
///
/// Keeps the RFC 3986 "unreserved" characters plus `:` (allowed inside a
/// segment and used in resource IDs); every other byte of the UTF-8
/// encoding becomes `%XX`. Written by hand to avoid a new dependency.
fn encode_path_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b':' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// `DELETE /resources/{id}` — Delete a resource definition.
///
/// Returns 409 if the resource is currently held by any user.
/// Returns 204 No Content on success and emits `ResourceDeleted` with
/// `reason = client`.
async fn delete_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, ProblemDetail> {
    // Check if resource exists and is not currently held.
    match state.resource_registry.get(&id) {
        Some(resource) => {
            if !resource.holders.is_empty() {
                return Err(ProblemDetail::conflict(format!(
                    "Resource '{}' is currently held by {} user(s) — release it first",
                    id,
                    resource.holders.len()
                )));
            }
        }
        None => {
            return Err(ProblemDetail::not_found(format!(
                "Resource '{}' not found",
                id
            )));
        }
    }

    state.resource_registry.remove(&id);

    // Broadcast DeviceEvent for SSE subscribers (Phase 39, ADR-030).
    let _ = state.device_manager.broadcast(DeviceEvent::ResourceDeleted {
        resource_id: id.clone(),
        reason: DeleteReason::Client,
        timestamp: chrono::Utc::now(),
    });

    // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_deleted","resource_id":"{}"}}"#,
        id
    ));

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /resources/{id}/acquire` — Acquire a resource for a user (ADR-034 A1).
///
/// `mode` is optional (`D62`): when omitted, the resource's declared
/// `access_mode` is used (falling back to `shared` for legacy resources).
/// When supplied, `mode` takes precedence (`S-03`) so a caller can request
/// `read_only` on a `shared` or `exclusive` resource. An unknown `mode`
/// string is a 422 (`validation`). A denial is a 409 with a `conflict`
/// object (`locked`, `mode_mismatch`, or `private`).
async fn acquire_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ResourceView>, ProblemDetail> {
    let request: AcquireResourceRequest = decode_json(&body)?;

    // Resolve user identity: body first, then X-User-Id header (ADR-029).
    let user_id = super::resolve_user_id(request.user_id.as_deref(), &headers)?;

    // Look the resource up before validating `mode`, so an unknown resource
    // is a 404 even with a bad mode — the same order as gRPC.
    let resource = state
        .resource_registry
        .get(&id)
        .ok_or_else(|| ProblemDetail::not_found(format!("Resource '{}' not found", id)))?;

    let requested = match request.mode.as_deref() {
        None => None,
        Some(text) => Some(AccessMode::parse_holder(text).ok_or_else(|| {
            ProblemDetail::from_resource_error(ResourceError::validation(format!(
                "mode must be 'shared', 'exclusive' or 'read_only' (got '{}')",
                text
            )))
        })?),
    };

    let mode = requested
        .or(resource.access_mode)
        .unwrap_or(AccessMode::Shared);

    let outcome = match state.resource_registry.acquire(&id, &user_id, mode) {
        Ok(outcome) => outcome,
        Err(denied) => {
            let problem = ProblemDetail::conflict(denied.to_string());
            return Err(match denied.to_conflict(&resource) {
                Some(conflict) => problem.with_conflict(conflict),
                None => problem,
            });
        }
    };

    // Track the user session (implicit heartbeat on acquire).
    state.user_sessions.touch(&user_id);

    // Only emit change events when a new holder entry was added; an
    // idempotent repeat acquire (AlreadyHeld) did not change state.
    if outcome == AcquireOutcome::Acquired {
        emit_resource_changed(&state, &id);

        // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
        state.broadcast_event(&format!(
            r#"{{"type":"resource_acquired","resource_id":"{}","user_id":"{}"}}"#,
            id, user_id
        ));
    }

    let resource = state
        .resource_registry
        .get(&id)
        .ok_or_else(|| ProblemDetail::not_found(format!("Resource '{}' not found", id)))?;
    Ok(Json(ResourceView::from(&resource)))
}

/// `POST /resources/{id}/release` — Release a previously acquired resource.
async fn release_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ReleaseResourceRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    // Resolve user identity: body first, then X-User-Id header (ADR-029).
    let user_id = super::resolve_user_id(body.user_id.as_deref(), &headers)?;

    if let Some(released) = state.resource_registry.release_with_info(&id, &user_id) {
        // Track the user session (implicit heartbeat on release).
        state.user_sessions.touch(&user_id);

        // 44-D Q3: If the releasing user held the lock on a shared resource,
        // broadcast ResourceUnlocked before ResourceChanged so blocked holders
        // and other holders of this resource know the lock has ended.
        if released.was_locked {
            let _ = state.device_manager.broadcast(DeviceEvent::ResourceUnlocked {
                resource_id: id.clone(),
                user_id: user_id.clone(),
                timestamp: chrono::Utc::now(),
            });
            state.broadcast_event(&format!(
                r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
                id, user_id
            ));
        }

        emit_resource_changed(&state, &id);

        // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
        state.broadcast_event(&format!(
            r#"{{"type":"resource_released","resource_id":"{}","user_id":"{}"}}"#,
            id, user_id
        ));

        Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Resource '{}' released by '{}'", id, user_id),
        })))
    } else {
        Err(ProblemDetail::not_found(format!(
            "Resource '{}' not found or user '{}' was not holding it",
            id, user_id
        )))
    }
}

/// `POST /resources/{id}/lock` — Lock a shared resource (ADR-020 §3, ADR-034 B1–B4).
///
/// A holder of a shared resource can temporarily lock it for exclusive use.
/// Other holders remain in the holders list; overlapping shared resources are
/// blocked for other users until unlock. Emits `ResourceChanged` and
/// `ResourceLocked` when a new lock is acquired (idempotent repeat lock by the
/// current locker affirms the lock without duplicate events, 44-D Q2).
async fn lock_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LockUnlockRequest>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    // Resolve user identity: body first, then X-User-Id header (ADR-029).
    let user_id = super::resolve_user_id(body.user_id.as_deref(), &headers)?;

    let outcome = state
        .resource_registry
        .lock(&id, &user_id)
        .map_err(lock_failure)?;

    // Track the user session (implicit heartbeat on lock).
    state.user_sessions.touch(&user_id);

    if outcome == LockOutcome::Locked {
        emit_resource_changed(&state, &id);
        let _ = state.device_manager.broadcast(DeviceEvent::ResourceLocked {
            resource_id: id.clone(),
            user_id: user_id.clone(),
            timestamp: chrono::Utc::now(),
        });

        // Broadcast event for dashboard auto-refresh (Phase 28D — was missing).
        state.broadcast_event(&format!(
            r#"{{"type":"resource_locked","resource_id":"{}","user_id":"{}"}}"#,
            id, user_id
        ));
    }

    let resource = state
        .resource_registry
        .get(&id)
        .ok_or_else(|| ProblemDetail::not_found(format!("Resource '{}' not found", id)))?;
    Ok(Json(ResourceView::from(&resource)))
}

/// `POST /resources/{id}/unlock` — Unlock a previously locked resource (ADR-020 §3).
///
/// Only the user who locked the resource can unlock it.
/// Emits `ResourceChanged` and `ResourceUnlocked`.
async fn unlock_resource(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<LockUnlockRequest>,
) -> Result<Json<ResourceView>, ProblemDetail> {
    // Resolve user identity: body first, then X-User-Id header (ADR-029).
    let user_id = super::resolve_user_id(body.user_id.as_deref(), &headers)?;

    state
        .resource_registry
        .unlock(&id, &user_id)
        .map_err(lock_failure)?;

    // Track the user session (implicit heartbeat on unlock).
    state.user_sessions.touch(&user_id);

    emit_resource_changed(&state, &id);
    let _ = state.device_manager.broadcast(DeviceEvent::ResourceUnlocked {
        resource_id: id.clone(),
        user_id: user_id.clone(),
        timestamp: chrono::Utc::now(),
    });

    // Broadcast event for dashboard auto-refresh (Phase 28D — was missing).
    state.broadcast_event(&format!(
        r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
        id, user_id
    ));

    let resource = state
        .resource_registry
        .get(&id)
        .ok_or_else(|| ProblemDetail::not_found(format!("Resource '{}' not found", id)))?;
    Ok(Json(ResourceView::from(&resource)))
}

// ============================================================================
// Tests (Tier 3: in-process router, no hardware)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use ok_fpga_core::device::*;
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::ResourceRegistry;
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tower::ServiceExt;

    const SERIAL: &str = "TEST0001";
    /// A programmed device with an AXI data port (AXI interfaces allowed).
    const AXI_SERIAL: &str = "TEST0002";
    /// A connected but unprogrammed device with a Classic data port.
    const UNPROGRAMMED_SERIAL: &str = "TEST0003";

    /// A bitfile record for the `Programmed` test devices.
    fn test_bitfile() -> BitfileRecord {
        BitfileRecord {
            filename: "test.bit".to_string(),
            description: None,
            programmed_at: chrono::Utc::now(),
            programmed_by: "tester".to_string(),
            file_path: None,
            md5_hash: None,
        }
    }

    /// Device info for an emulated XEM8320.
    fn test_info(serial: &str) -> DeviceInfo {
        DeviceInfo {
            serial: serial.to_string(),
            product_name: "XEM8320-AU25P".to_string(),
            product_id: 56,
            device_interface: DeviceInterface::Usb3,
            usb_speed: UsbSpeed::Super,
            device_major_version: 1,
            device_minor_version: 0,
            wire_width: 32,
            trigger_width: 32,
            pipe_width: 32,
            register_address_width: 32,
            register_data_width: 32,
            fpga_vendor: FpgaVendor::Xilinx,
            dataport: DataportType::Legacy,
            device_id: "".to_string(),
        }
    }

    /// Build an `AppState` with three devices: [`SERIAL`] (programmed,
    /// Classic data port), [`AXI_SERIAL`] (programmed, AXI data port) and
    /// [`UNPROGRAMMED_SERIAL`] (connected only, Classic data port).
    fn test_state() -> Arc<AppState> {
        let device_manager = Arc::new(DeviceManager::new());
        device_manager.registry().add(FpgaDevice {
            info: test_info(SERIAL),
            state: DeviceState::Programmed { bitfile: test_bitfile() },
        });
        let mut axi_info = test_info(AXI_SERIAL);
        axi_info.dataport = DataportType::Axi;
        device_manager.registry().add(FpgaDevice {
            info: axi_info,
            state: DeviceState::Programmed { bitfile: test_bitfile() },
        });
        device_manager.registry().add(FpgaDevice {
            info: test_info(UNPROGRAMMED_SERIAL),
            state: DeviceState::Connected,
        });
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        Arc::new(AppState::new(
            device_manager,
            Arc::new(ResourceRegistry::new()),
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            Arc::new(ok_fpga_core::auth::SessionManager::new(3600)),
            Arc::new(UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Off,
            crate::config::ServerConfig::default(),
            None,
        ))
    }

    /// Send one request through the resource router.
    async fn send(
        state: &Arc<AppState>,
        method: &str,
        uri: &str,
        body: serde_json::Value,
    ) -> (StatusCode, HeaderMap, serde_json::Value) {
        send_raw(state, method, uri, body.to_string()).await
    }

    /// Like [`send`] but with a raw text body (for malformed JSON).
    async fn send_raw(
        state: &Arc<AppState>,
        method: &str,
        uri: &str,
        body: String,
    ) -> (StatusCode, HeaderMap, serde_json::Value) {
        let app = routes().with_state(state.clone());
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, headers, json)
    }

    /// A shared resource declaring one wire-in.
    fn shared_wire_resource(name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "device_serial": SERIAL,
            "access_mode": "shared",
            "interfaces": ["classic"],
            "endpoints": {"entries": [
                {"address": "0x01", "type": "wire_in", "access": "shared"}
            ]}
        })
    }

    #[tokio::test]
    async fn test_create_resource_request_full_schema_roundtrips() {
        let state = test_state();
        let body = serde_json::json!({
            "name": "counters",
            "description": "counter block",
            // `axi_lite` needs a device with an AXI data port.
            "device_serial": AXI_SERIAL,
            "access_mode": "shared",
            "private": true,
            "interfaces": ["classic", "axi_lite"],
            "endpoints": {
                "default_access": "exclusive",
                "entries": [
                    {"address": "0x00", "type": "wire_in", "access": "shared",
                     "private": true, "description": "reset"},
                    {"address": "0x20", "type": "wire_out"}
                ]
            },
            "address_ranges": [
                {"name": "regs", "start": "0x44A60000", "end": "0x44A6FFFF",
                 "access": "exclusive", "private": false, "description": "AXI regs"}
            ],
            "metadata": {"version": "1.2"}
        });
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        assert_eq!(json["id"], format!("{AXI_SERIAL}:counters"));
        assert_eq!(json["state"], "available");
        assert_eq!(json["access_mode"], "shared");
        assert_eq!(json["private"], true);
        assert_eq!(json["interfaces"], serde_json::json!(["classic", "axi_lite"]));
        assert_eq!(json["endpoints"]["default_access"], "exclusive");
        let entries = json["endpoints"]["entries"].as_array().unwrap();
        assert_eq!(entries[0]["address"], "0x00");
        assert_eq!(entries[0]["type"], "wire_in");
        assert_eq!(entries[0]["access"], "shared");
        assert_eq!(entries[0]["private"], true);
        assert_eq!(entries[0]["description"], "reset");
        // D61: an entry without `access` takes `default_access`.
        assert_eq!(entries[1]["access"], "exclusive");
        let range = &json["address_ranges"][0];
        assert_eq!(range["name"], "regs");
        assert_eq!(range["start"], "0x44A60000");
        assert_eq!(range["end"], "0x44A6FFFF");
        assert_eq!(range["access"], "exclusive");
        assert_eq!(range["description"], "AXI regs");
        assert_eq!(json["metadata"]["version"], "1.2");
        assert_eq!(json["holders"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_create_resource_request_malformed_json_returns_422() {
        let state = test_state();
        let (status, _, json) =
            send_raw(&state, "POST", "/resources", "{not json".to_string()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["conflict"]["code"], "validation");
    }

    #[tokio::test]
    async fn test_create_resource_metadata_non_string_returns_422() {
        let state = test_state();
        let mut body = shared_wire_resource("meta");
        body["metadata"] = serde_json::json!({"count": 3});
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(json["detail"].as_str().unwrap().contains("metadata"));
        assert_eq!(json["conflict"]["code"], "validation");
    }

    #[tokio::test]
    async fn test_create_resource_unknown_device_returns_404() {
        let state = test_state();
        let mut body = shared_wire_resource("ghost");
        body["device_serial"] = serde_json::json!("NOPE");
        let (status, _, _) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_resource_duplicate_returns_409_without_conflict() {
        let state = test_state();
        let (status, _, _) = send(&state, "POST", "/resources", shared_wire_resource("dup")).await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _, json) =
            send(&state, "POST", "/resources", shared_wire_resource("dup")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(json.get("conflict").is_none());
    }

    #[tokio::test]
    async fn test_conflict_error_body_names_resource_and_item() {
        let state = test_state();
        let first = serde_json::json!({
            "name": "a", "device_serial": SERIAL, "access_mode": "exclusive",
            "interfaces": ["classic"],
            "endpoints": {"entries": [{"address": "0x05", "type": "wire_in", "access": "exclusive"}]}
        });
        let mut second = first.clone();
        second["name"] = serde_json::json!("b");
        let (status, _, _) = send(&state, "POST", "/resources", first).await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _, json) = send(&state, "POST", "/resources", second).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(json["conflict"]["code"], "overlap");
        assert_eq!(json["conflict"]["resource_id"], format!("{SERIAL}:a"));
        assert_eq!(json["conflict"]["resource_name"], "a");
        assert_eq!(json["conflict"]["item_type"], "endpoint");
        assert_eq!(json["conflict"]["item"], "0x05");
        assert_eq!(json["conflict"]["items"], serde_json::json!(["0x05"]));
    }

    // ---- 44-B: registration rules over REST ---------------------------------

    #[tokio::test]
    async fn test_create_resource_axi_on_legacy_dataport_rejected() {
        let state = test_state();
        let mut body = shared_wire_resource("axi");
        body["interfaces"] = serde_json::json!(["classic", "axi_lite"]);
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
        assert_eq!(json["conflict"]["code"], "validation");
        assert!(json["detail"].as_str().unwrap().contains("axi_lite"));
    }

    #[tokio::test]
    async fn test_create_resource_unprogrammed_legacy_with_axi_rejected_422() {
        // The data-port check runs before the "device is programmed" check:
        // a declaration that can never be valid on this device gets 422,
        // not a 409 that suggests programming the device would help.
        let state = test_state();
        let mut body = shared_wire_resource("axi");
        body["device_serial"] = serde_json::json!(UNPROGRAMMED_SERIAL);
        body["interfaces"] = serde_json::json!(["axi_lite"]);
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    }

    #[tokio::test]
    async fn test_create_resource_unprogrammed_classic_returns_409() {
        let state = test_state();
        let mut body = shared_wire_resource("early");
        body["device_serial"] = serde_json::json!(UNPROGRAMMED_SERIAL);
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
    }

    #[tokio::test]
    async fn test_create_resource_endpoint_address_not_two_hex_digits_rejected() {
        let state = test_state();
        for bad in ["5", "0x5", "0x005", "0X05"] {
            let mut body = shared_wire_resource("addr");
            body["endpoints"]["entries"][0]["address"] = serde_json::json!(bad);
            let (status, _, json) = send(&state, "POST", "/resources", body).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {json}");
            assert_eq!(json["conflict"]["code"], "validation");
        }
    }

    #[tokio::test]
    async fn test_create_resource_duplicate_endpoint_address_rejected() {
        let state = test_state();
        let mut body = shared_wire_resource("dup_ep");
        body["endpoints"]["entries"] = serde_json::json!([
            {"address": "0x01", "type": "wire_in", "access": "shared"},
            {"address": "0x01", "type": "wire_in", "access": "exclusive"}
        ]);
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
        assert_eq!(json["conflict"]["item"], "0x01");
    }

    #[tokio::test]
    async fn test_create_resource_exclusive_with_shared_item_rejected() {
        let state = test_state();
        let mut body = shared_wire_resource("rule_a");
        body["access_mode"] = serde_json::json!("exclusive");
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
        assert_eq!(json["conflict"]["code"], "validation");
        assert_eq!(json["conflict"]["item"], "0x01");
    }

    #[tokio::test]
    async fn test_create_resource_shared_overlap_allowed() {
        let state = test_state();
        let (status, _, _) = send(&state, "POST", "/resources", shared_wire_resource("one")).await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _, json) = send(&state, "POST", "/resources", shared_wire_resource("two")).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
    }

    #[tokio::test]
    async fn test_create_resource_no_endpoints_blocks_later_classic() {
        let state = test_state();
        let all = serde_json::json!({
            "name": "all", "device_serial": SERIAL, "interfaces": ["classic"]
        });
        let (status, _, json) = send(&state, "POST", "/resources", all).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        let (status, _, json) = send(&state, "POST", "/resources", shared_wire_resource("late")).await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
        assert_eq!(json["conflict"]["code"], "overlap");
        assert_eq!(json["conflict"]["resource_id"], format!("{SERIAL}:all"));
        assert_eq!(json["conflict"]["items"], serde_json::json!(["0x01"]));
    }

    #[tokio::test]
    async fn test_create_resource_self_overlapping_ranges_accepted() {
        // Overlapping ranges inside one declaration are allowed (only a
        // warning is logged).
        let state = test_state();
        let mut body = shared_wire_resource("self");
        body["address_ranges"] = serde_json::json!([
            {"start": "0x1000", "end": "0x1FFF", "access": "shared"},
            {"start": "0x1800", "end": "0x2FFF", "access": "exclusive", "private": true}
        ]);
        let (status, _, json) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
    }

    #[tokio::test]
    async fn test_create_resource_overlap_items_include_endpoints_and_ranges() {
        let state = test_state();
        let decl = |name: &str, first: &str, second: &str, start: &str, end: &str| {
            serde_json::json!({
                "name": name, "device_serial": SERIAL, "access_mode": "exclusive",
                "interfaces": ["classic"],
                "endpoints": {"entries": [
                    {"address": first, "type": "wire_in", "access": "exclusive"},
                    {"address": second, "type": "wire_in", "access": "exclusive"}
                ]},
                "address_ranges": [{"start": start, "end": end, "access": "exclusive"}]
            })
        };
        let (status, _, _) =
            send(&state, "POST", "/resources", decl("a", "0x01", "0x00", "0x1000", "0x1FFF")).await;
        assert_eq!(status, StatusCode::CREATED);
        let (status, _, json) =
            send(&state, "POST", "/resources", decl("b", "0x00", "0x01", "0x1800", "0x2FFF")).await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
        assert_eq!(json["conflict"]["item"], "0x00");
        assert_eq!(
            json["conflict"]["items"],
            serde_json::json!(["0x00", "0x01", "0x00001000-0x00001FFF"])
        );
    }

    #[tokio::test]
    async fn test_create_resource_sets_location_header() {
        let state = test_state();
        let (status, headers, _) =
            send(&state, "POST", "/resources", shared_wire_resource("loc")).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(
            headers.get(header::LOCATION).unwrap(),
            &format!("/api/v1/resources/{SERIAL}:loc")
        );
    }

    #[tokio::test]
    async fn test_acquire_resource_invalid_mode_returns_422() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("acq")).await;
        let uri = format!("/resources/{SERIAL}:acq/acquire");
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "alice", "mode": "bogus"}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["conflict"]["code"], "validation");
    }

    #[tokio::test]
    async fn test_acquire_resource_unknown_with_invalid_mode_returns_404() {
        // Same order as gRPC: the lookup comes before the mode check.
        let state = test_state();
        let (status, _, _) = send(
            &state,
            "POST",
            "/resources/nope/acquire",
            serde_json::json!({"user_id": "alice", "mode": "bogus"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_encode_path_segment_special_chars_percent_encoded() {
        assert_eq!(encode_path_segment("SN1:dac-0_a.b~"), "SN1:dac-0_a.b~");
        assert_eq!(encode_path_segment("SN1:my dac/1"), "SN1:my%20dac%2F1");
        assert_eq!(encode_path_segment("é"), "%C3%A9");
    }

    #[tokio::test]
    async fn test_acquire_resource_mode_omitted_returns_holder_view() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("acq2")).await;
        let uri = format!("/resources/{SERIAL}:acq2/acquire");
        let (status, _, json) =
            send(&state, "POST", &uri, serde_json::json!({"user_id": "alice"})).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["state"], "in_use_shared");
        assert_eq!(
            json["holders"],
            serde_json::json!([{"user_id": "alice", "mode": "shared"}])
        );
    }

    #[tokio::test]
    async fn test_acquire_resource_exclusively_held_returns_locked_conflict() {
        let state = test_state();
        let body = serde_json::json!({
            "name": "ex", "device_serial": SERIAL, "interfaces": ["classic"]
        });
        let (status, _, _) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::CREATED);
        let uri = format!("/resources/{SERIAL}:ex/acquire");
        send(&state, "POST", &uri, serde_json::json!({"user_id": "alice"})).await;
        let (status, _, json) =
            send(&state, "POST", &uri, serde_json::json!({"user_id": "bob"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(json["conflict"]["code"], "locked");
        assert_eq!(json["conflict"]["resource_id"], format!("{SERIAL}:ex"));
    }

    #[tokio::test]
    async fn test_lock_resource_success_emits_resource_locked_event() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("lk")).await;
        let id = format!("{SERIAL}:lk");
        send(
            &state,
            "POST",
            &format!("/resources/{id}/acquire"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        let mut events = state.device_manager.subscribe();
        let (status, _, json) = send(
            &state,
            "POST",
            &format!("/resources/{id}/lock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["locked_by"], "alice");

        let mut saw_locked = false;
        while let Ok(event) = events.try_recv() {
            if let DeviceEvent::ResourceLocked { resource_id, user_id, .. } = event {
                assert_eq!(resource_id, id);
                assert_eq!(user_id, "alice");
                saw_locked = true;
            }
        }
        assert!(saw_locked, "ResourceLocked event not broadcast");
    }

    #[tokio::test]
    async fn test_lock_resource_locked_by_other_returns_locked_conflict() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("lk2")).await;
        let id = format!("{SERIAL}:lk2");
        for user in ["alice", "bob"] {
            send(
                &state,
                "POST",
                &format!("/resources/{id}/acquire"),
                serde_json::json!({"user_id": user}),
            )
            .await;
        }
        send(
            &state,
            "POST",
            &format!("/resources/{id}/lock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        let (status, _, json) = send(
            &state,
            "POST",
            &format!("/resources/{id}/lock"),
            serde_json::json!({"user_id": "bob"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(json["conflict"]["code"], "locked");
    }

    #[tokio::test]
    async fn test_delete_resource_emits_client_reason() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("del")).await;
        let mut events = state.device_manager.subscribe();
        let (status, _, _) = send(
            &state,
            "DELETE",
            &format!("/resources/{SERIAL}:del"),
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        match events.try_recv() {
            Ok(DeviceEvent::ResourceDeleted { reason, .. }) => {
                assert_eq!(reason, DeleteReason::Client)
            }
            other => panic!("expected ResourceDeleted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resource_changed_event_new_state_uses_spec_strings() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("chg")).await;
        let mut events = state.device_manager.subscribe();
        send(
            &state,
            "POST",
            &format!("/resources/{SERIAL}:chg/acquire"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        match events.try_recv() {
            Ok(DeviceEvent::ResourceChanged { new_state, .. }) => {
                assert_eq!(new_state, "in_use_shared")
            }
            other => panic!("expected ResourceChanged, got {other:?}"),
        }
    }

    // ── Phase 44-C: Acquisition rules (A1) ─────────────────────

    #[tokio::test]
    async fn test_acquire_same_mode_idempotent() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("idem")).await;
        let uri = format!("/resources/{SERIAL}:idem/acquire");

        let (status, _, _) =
            send(&state, "POST", &uri, serde_json::json!({"user_id": "alice"})).await;
        assert_eq!(status, StatusCode::OK);

        // Subscribe after first acquire: a repeat acquire in the same mode
        // must succeed with 1 holder and no duplicate ResourceChanged event.
        let mut events = state.device_manager.subscribe();
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "alice", "mode": "shared"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(
            json["holders"],
            serde_json::json!([{"user_id": "alice", "mode": "shared"}])
        );
        assert!(events.try_recv().is_err(), "no event on idempotent repeat acquire");
    }

    #[tokio::test]
    async fn test_acquire_different_mode_conflict() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("diff")).await;
        let uri = format!("/resources/{SERIAL}:diff/acquire");

        let (status, _, _) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "alice", "mode": "shared"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Existing holder requesting read_only without releasing first -> 409 mode_mismatch.
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "alice", "mode": "read_only"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
        assert_eq!(json["conflict"]["code"], "mode_mismatch");
        assert_eq!(json["conflict"]["resource_id"], format!("{SERIAL}:diff"));

        // Requesting exclusive on a shared resource -> 409 mode_mismatch (D62).
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "bob", "mode": "exclusive"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
        assert_eq!(json["conflict"]["code"], "mode_mismatch");
    }

    #[tokio::test]
    async fn test_acquire_omitted_mode_uses_declared_mode() {
        let state = test_state();
        let excl = serde_json::json!({
            "name": "excl_def", "device_serial": SERIAL, "interfaces": ["classic"]
        });
        send(&state, "POST", "/resources", excl).await;
        let uri = format!("/resources/{SERIAL}:excl_def/acquire");

        // Omitted mode on an exclusive resource uses "exclusive" and sets locked_by.
        let (status, _, json) =
            send(&state, "POST", &uri, serde_json::json!({"user_id": "alice"})).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["state"], "in_use_locked");
        assert_eq!(json["locked_by"], "alice");
        assert_eq!(
            json["holders"],
            serde_json::json!([{"user_id": "alice", "mode": "exclusive"}])
        );

        // Explicit mode: "read_only" on an exclusive resource uses "read_only" (S-03).
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "bob", "mode": "read_only"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["state"], "in_use_locked");
        assert_eq!(json["locked_by"], "alice");
        assert_eq!(json["holders"].as_array().unwrap().len(), 2);
        assert_eq!(json["holders"][1]["mode"], "read_only");
    }

    #[tokio::test]
    async fn test_acquire_readonly_private_denied() {
        let state = test_state();
        let mut body = shared_wire_resource("priv_res");
        body["private"] = serde_json::json!(true);
        let (status, _, _) = send(&state, "POST", "/resources", body).await;
        assert_eq!(status, StatusCode::CREATED);

        let uri = format!("/resources/{SERIAL}:priv_res/acquire");
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "bob", "mode": "read_only"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
        assert_eq!(json["conflict"]["code"], "private");
        assert_eq!(json["conflict"]["resource_id"], format!("{SERIAL}:priv_res"));
        assert_eq!(json["conflict"]["resource_name"], "priv_res");
    }

    #[tokio::test]
    async fn test_acquire_exclusive_with_readonly_holders_allowed() {
        let state = test_state();
        let excl = serde_json::json!({
            "name": "excl_ro", "device_serial": SERIAL, "interfaces": ["classic"]
        });
        send(&state, "POST", "/resources", excl).await;
        let uri = format!("/resources/{SERIAL}:excl_ro/acquire");

        // D25b: ReadOnly acquires available Exclusive resource -> in_use_shared.
        let (status, _, json) = send(
            &state,
            "POST",
            &uri,
            serde_json::json!({"user_id": "bob", "mode": "read_only"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["state"], "in_use_shared");
        assert!(json["locked_by"].is_null());

        // D25c: Exclusive holder joins -> in_use_locked, locked_by = "alice".
        let (status, _, json) =
            send(&state, "POST", &uri, serde_json::json!({"user_id": "alice"})).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["state"], "in_use_locked");
        assert_eq!(json["locked_by"], "alice");
        assert_eq!(json["holders"].as_array().unwrap().len(), 2);
    }

    // ── Phase 44-D: Locking, blocking (B1–B4) & events ──────────

    #[tokio::test]
    async fn test_lock_blocked_resource_conflict() {
        let state = test_state();
        // Register two shared resources that share WireIn 0x01.
        send(&state, "POST", "/resources", shared_wire_resource("blk_r")).await;
        send(&state, "POST", "/resources", shared_wire_resource("blk_s")).await;

        let id_r = format!("{SERIAL}:blk_r");
        let id_s = format!("{SERIAL}:blk_s");

        send(
            &state,
            "POST",
            &format!("/resources/{id_r}/acquire"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        send(
            &state,
            "POST",
            &format!("/resources/{id_s}/acquire"),
            serde_json::json!({"user_id": "bob"}),
        )
        .await;

        // Alice locks blk_r.
        let (status, _, json) = send(
            &state,
            "POST",
            &format!("/resources/{id_r}/lock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["locked_by"], "alice");

        // 44-D Q2: Repeat lock by Alice succeeds idempotently with no duplicate event.
        let mut events = state.device_manager.subscribe();
        let (status, _, json) = send(
            &state,
            "POST",
            &format!("/resources/{id_r}/lock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["locked_by"], "alice");
        assert!(events.try_recv().is_err(), "no event on idempotent repeat lock");

        // Bob tries to lock blk_s -> 409 Conflict with conflict.code == "blocked" naming blk_r.
        let (status, _, json) = send(
            &state,
            "POST",
            &format!("/resources/{id_s}/lock"),
            serde_json::json!({"user_id": "bob"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{json}");
        assert_eq!(json["conflict"]["code"], "blocked");
        assert_eq!(json["conflict"]["resource_id"], id_r);
        assert_eq!(json["conflict"]["resource_name"], "blk_r");
        assert!(json["conflict"].get("item").is_none());
        assert!(json["conflict"].get("items").is_none());
    }

    #[tokio::test]
    async fn test_unlock_notifies_blocked_holders() {
        let state = test_state();
        send(&state, "POST", "/resources", shared_wire_resource("unlk_r")).await;
        send(&state, "POST", "/resources", shared_wire_resource("unlk_s")).await;

        let id_r = format!("{SERIAL}:unlk_r");
        let id_s = format!("{SERIAL}:unlk_s");

        send(
            &state,
            "POST",
            &format!("/resources/{id_r}/acquire"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        send(
            &state,
            "POST",
            &format!("/resources/{id_s}/acquire"),
            serde_json::json!({"user_id": "bob"}),
        )
        .await;
        send(
            &state,
            "POST",
            &format!("/resources/{id_r}/lock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;

        // Subscribe on the event bus (which serves both holders of unlk_r and
        // holders of blocked resource unlk_s) and unlock unlk_r.
        let mut events = state.device_manager.subscribe();
        let (status, _, _) = send(
            &state,
            "POST",
            &format!("/resources/{id_r}/unlock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let mut saw_unlocked = false;
        while let Ok(evt) = events.try_recv() {
            if let DeviceEvent::ResourceUnlocked { resource_id, user_id, .. } = evt {
                assert_eq!(resource_id, id_r);
                assert_eq!(user_id, "alice");
                saw_unlocked = true;
            }
        }
        assert!(saw_unlocked, "ResourceUnlocked event must be broadcast on /unlock");

        // Now test implicit unlock when the locker releases without calling /unlock (44-D Q3).
        send(
            &state,
            "POST",
            &format!("/resources/{id_r}/lock"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        let mut events2 = state.device_manager.subscribe();
        let (status, _, _) = send(
            &state,
            "POST",
            &format!("/resources/{id_r}/release"),
            serde_json::json!({"user_id": "alice"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let mut saw_implicit_unlocked = false;
        while let Ok(evt) = events2.try_recv() {
            if let DeviceEvent::ResourceUnlocked { resource_id, user_id, .. } = evt {
                assert_eq!(resource_id, id_r);
                assert_eq!(user_id, "alice");
                saw_implicit_unlocked = true;
            }
        }
        assert!(
            saw_implicit_unlocked,
            "ResourceUnlocked event must be broadcast when locker releases"
        );
    }
}
