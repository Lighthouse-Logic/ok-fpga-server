//! # Classic Data Port Routes (Phase 19C)
//!
//! REST endpoints for Classic data port operations (wire, trigger, pipe,
//! register). Implements the `/devices/{serial}/classic/*` paths from ADR-004.
//!
//! All handlers are wired to the FrontPanel DLL via the `DeviceHandlePool`
//! in `AppState`. Each handler locks the per-device mutex, performs the FFI
//! call, and releases the lock.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use ok_fpga_ffi::error::FfiError;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/devices/{serial}/classic/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // Wires
        .route("/devices/{serial}/classic/wires/in", put(set_wire_in))
        .route(
            "/devices/{serial}/classic/wires/in/update",
            post(update_wire_ins),
        )
        .route(
            "/devices/{serial}/classic/wires/out/update",
            post(update_wire_outs),
        )
        // Wire-out: one endpoint per request (`?endpoint=N` is required;
        // whole-bank reads were removed, ADR-034 §9 / D76).
        .route(
            "/devices/{serial}/classic/wires/out",
            get(get_wire_out),
        )
        // Triggers
        .route(
            "/devices/{serial}/classic/triggers/in",
            post(activate_trigger_in),
        )
        .route(
            "/devices/{serial}/classic/triggers/out/update",
            post(update_trigger_outs),
        )
        // Trigger-out: one endpoint per request (`?endpoint=N` is required;
        // whole-bank reads were removed, ADR-034 §9 / D76).
        .route(
            "/devices/{serial}/classic/triggers/out",
            get(get_trigger_out),
        )
        // Pipes
        .route("/devices/{serial}/classic/pipes/in", post(write_to_pipe_in))
        .route(
            "/devices/{serial}/classic/pipes/out",
            post(read_from_pipe_out),
        )
        .route(
            "/devices/{serial}/classic/pipes/block-in",
            post(write_to_block_pipe_in),
        )
        .route(
            "/devices/{serial}/classic/pipes/block-out",
            post(read_from_block_pipe_out),
        )
        // Registers
        .route(
            "/devices/{serial}/classic/registers/read",
            post(read_register),
        )
        .route(
            "/devices/{serial}/classic/registers/write",
            put(write_register),
        )
        // Reset
        .route("/devices/{serial}/classic/reset", post(reset_fpga))
}

// ============================================================================
// Request / Response Types
// ============================================================================

#[derive(Deserialize)]
struct WireInRequest {
    endpoint: i32,
    value: u32,
    #[serde(default = "default_mask")]
    mask: u32,
}

fn default_mask() -> u32 {
    0xFFFF_FFFF
}

#[derive(Deserialize)]
struct TriggerInRequest {
    endpoint: i32,
    bit: i32,
}

/// Query parameters for `GET /devices/{serial}/classic/wires/out`.
///
/// `endpoint` is required by the spec. It is an `Option` here only so a
/// missing value becomes our own 422 ProblemDetail instead of Axum's plain
/// 400 text rejection.
#[derive(Deserialize)]
struct WireOutQuery {
    endpoint: Option<i32>,
    /// Run `UpdateWireOuts` immediately before the read (D76). Default
    /// `false`: read the current snapshot only, exactly like Opal Kelly's
    /// `GetWireOutValue`.
    #[serde(default)]
    update: bool,
}

/// Query parameters for `GET /devices/{serial}/classic/triggers/out`.
///
/// `endpoint` is required (see [`WireOutQuery`] for why it is an `Option`).
#[derive(Deserialize)]
struct TriggerOutQuery {
    endpoint: Option<i32>,
    #[serde(default = "default_mask")]
    mask: u32,
    /// Run `UpdateTriggerOuts` immediately before the read (D76). Default
    /// `false`: read the current snapshot only. Note that `UpdateTriggerOuts`
    /// clears the latches of **every** TriggerOut on the device, for every
    /// client.
    #[serde(default)]
    update: bool,
}

/// 422 for a single-endpoint read called without `endpoint` (D76: there
/// are no whole-bank reads).
fn missing_endpoint(kind: &str, range: &str) -> ProblemDetail {
    ProblemDetail::from_resource_error(ok_fpga_core::resource::ResourceError::validation(
        format!(
            "query parameter 'endpoint' is required ({} address {}); whole-bank reads are not supported",
            kind, range
        ),
    ))
}

#[derive(Deserialize)]
struct RegisterRequest {
    address: u32,
    #[serde(default)]
    value: u32,
}

#[derive(Deserialize)]
struct PipeRequest {
    endpoint: i32,
    #[serde(default = "default_pipe_length")]
    length: i32,
    /// Base64-encoded data for pipe-in operations.
    #[serde(default)]
    data: Option<String>,
}

fn default_pipe_length() -> i32 {
    1024
}

#[derive(Deserialize)]
struct BlockPipeRequest {
    endpoint: i32,
    block_size: i32,
    #[serde(default = "default_pipe_length")]
    length: i32,
    /// Base64-encoded data for block pipe-in operations.
    #[serde(default)]
    data: Option<String>,
}

/// Response for a single wire-out read — matches OpenAPI `WireOutResponse` (Phase 37).
#[derive(Serialize)]
struct WireOutResponse {
    endpoint: i32,
    value: u32,
}

/// Response for a single trigger-out read — matches OpenAPI `TriggerOutResponse` (Phase 37).
#[derive(Serialize)]
struct TriggerOutResponse {
    endpoint: i32,
    triggered: bool,
    /// Full 32-bit trigger vector.
    vector: u32,
}

/// Register read response — matches OpenAPI `RegisterEntry` (Phase 37).
#[derive(Serialize)]
struct RegisterResponse {
    address: u32,
    value: u32,
}

/// StatusResponse for operations that don't return data — matches OpenAPI `StatusResponse`.
#[derive(Serialize)]
struct StatusResponse {
    success: bool,
    message: String,
}

/// Pipe transfer response — matches OpenAPI `PipeTransferResponse` (Phase 37).
#[derive(Serialize)]
struct PipeTransferResponse {
    bytes_transferred: i32,
}

/// Pipe read response — extends `PipeTransferResponse` with base64 data.
#[derive(Serialize)]
struct PipeReadResponse {
    bytes_transferred: i32,
    data: String,
}

// ============================================================================
// Helper: FFI error → ProblemDetail
// ============================================================================

fn ffi_error_to_problem(e: &FfiError) -> ProblemDetail {
    match e {
        FfiError::FrontPanelError { code, message } => {
            let c = *code;
            if c == ok_fpga_ffi::ok_ErrorCode::ok_InvalidParameter as i32
                || c == ok_fpga_ffi::ok_ErrorCode::ok_InvalidEndpoint as i32
            {
                ProblemDetail::bad_request(message.clone())
            } else if c == ok_fpga_ffi::ok_ErrorCode::ok_DeviceNotOpen as i32 {
                ProblemDetail::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, message.clone())
            } else if c == ok_fpga_ffi::ok_ErrorCode::ok_Timeout as i32 {
                ProblemDetail::new(axum::http::StatusCode::GATEWAY_TIMEOUT, message.clone())
            } else if c == ok_fpga_ffi::ok_ErrorCode::ok_FrontPanelNotEnabled as i32 {
                ProblemDetail::conflict("FPGA is not programmed or FrontPanel is not enabled")
            } else {
                ProblemDetail::internal(message.clone())
            }
        }
        _ => ProblemDetail::internal(e.to_string()),
    }
}

/// Enforce endpoint access for a Classic operation (ADR-024 §4, handler-level).
///
/// This is the REST handler counterpart to the gRPC `check_access()` method.
/// It is called at the top of each Classic handler AFTER the request body has
/// been deserialized. The middleware has already done:
/// - Tier 1/2: identity check (X-User-Id present)
/// - Tier 3: device-level gating (user holds any resource on this device)
///
/// This function does the fine-grained endpoint-level check: does the user's
/// resource cover the specific endpoint address and type being accessed?
///
/// ## Returns
///
/// `Ok(())` if allowed, or `Err(ProblemDetail)` with 403 status if denied.
fn enforce_endpoint(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    serial: &str,
    endpoint: i32,
    ep_type: &ok_fpga_core::resource::EndpointType,
    value: Option<u32>,
) -> Result<(), ProblemDetail> {
    use crate::state::EndpointEnforcement;

    if !state.enforcement().is_active() {
        return Ok(());
    }

    let user_id = headers
        .get(crate::rest::heartbeat_middleware::USER_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty());

    let uid = match user_id {
        Some(uid) => uid,
        None => {
            // The middleware should have already rejected this request if
            // enforcement is Enforce. But for defense-in-depth:
            match state.enforcement() {
                EndpointEnforcement::Warn => {
                    tracing::warn!(
                        serial = %serial,
                        endpoint = endpoint,
                        "REST access enforcement (warn): X-User-Id header missing in handler"
                    );
                    return Ok(());
                }
                EndpointEnforcement::Enforce => {
                    return Err(ProblemDetail::new(
                        axum::http::StatusCode::FORBIDDEN,
                        "User identity required: X-User-Id header must be present",
                    ));
                }
                EndpointEnforcement::Off => return Ok(()),
            }
        }
    };

    if let Err(denied) = state.resource_registry.check_endpoint_access(
        uid,
        serial,
        endpoint as u8,
        ep_type,
        value,
    ) {
        match state.enforcement() {
            EndpointEnforcement::Warn => {
                tracing::warn!(
                    user_id = %uid,
                    serial = %serial,
                    endpoint = endpoint,
                    "REST access enforcement (warn): {}",
                    denied
                );
                Ok(())
            }
            EndpointEnforcement::Enforce => Err(ProblemDetail::new(
                axum::http::StatusCode::FORBIDDEN,
                denied.to_string(),
            )),
            EndpointEnforcement::Off => Ok(()),
        }
    } else {
        Ok(())
    }
}

/// Enforce address access for a register operation (ADR-024 §4, handler-level).
///
/// Same as [`enforce_endpoint`] but for address-based access (register bridge).
fn enforce_address(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    serial: &str,
    address: u32,
    is_write: bool,
) -> Result<(), ProblemDetail> {
    use crate::state::EndpointEnforcement;

    if !state.enforcement().is_active() {
        return Ok(());
    }

    let user_id = headers
        .get(crate::rest::heartbeat_middleware::USER_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty());

    let uid = match user_id {
        Some(uid) => uid,
        None => {
            match state.enforcement() {
                EndpointEnforcement::Warn => {
                    tracing::warn!(
                        serial = %serial,
                        address = address,
                        "REST access enforcement (warn): X-User-Id header missing in handler"
                    );
                    return Ok(());
                }
                EndpointEnforcement::Enforce => {
                    return Err(ProblemDetail::new(
                        axum::http::StatusCode::FORBIDDEN,
                        "User identity required: X-User-Id header must be present",
                    ));
                }
                EndpointEnforcement::Off => return Ok(()),
            }
        }
    };

    if let Err(denied) = state.resource_registry.check_address_access(
        uid,
        serial,
        address as u64,
        is_write,
    ) {
        match state.enforcement() {
            EndpointEnforcement::Warn => {
                tracing::warn!(
                    user_id = %uid,
                    serial = %serial,
                    address = address,
                    "REST access enforcement (warn): {}",
                    denied
                );
                Ok(())
            }
            EndpointEnforcement::Enforce => Err(ProblemDetail::new(
                axum::http::StatusCode::FORBIDDEN,
                denied.to_string(),
            )),
            EndpointEnforcement::Off => Ok(()),
        }
    } else {
        Ok(())
    }
}

/// Helper macro to get the handle pool and device, or return an error.
macro_rules! get_device {
    ($state:expr, $serial:expr) => {{
        let pool = $state
            .handle_pool
            .as_ref()
            .ok_or_else(|| ProblemDetail::not_implemented("FrontPanel DLL not loaded"))?;
        let device_arc = pool
            .get($serial)
            .await
            .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found in handle pool", $serial)))?;
        device_arc
    }};
}

// ============================================================================
// Handlers
// ============================================================================

/// `PUT /devices/{serial}/classic/wires/in` — Set a wire-in value.
///
/// Atomically calls `SetWireInValue` + `UpdateWireIns` under the device mutex.
async fn set_wire_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<WireInRequest>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    // ADR-024: endpoint-level enforcement.
    enforce_endpoint(&state, &headers, &serial, req.endpoint, &ok_fpga_core::resource::EndpointType::WireIn, Some(req.value))?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    // SAFETY: classic handle is valid (from pool), all parameters validated.
    unsafe {
        let rc = (dll.classic_set_wire_in_value)(
            classic,
            req.endpoint,
            req.value as std::os::raw::c_ulong,
            req.mask as std::os::raw::c_ulong,
        );
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;

        let rc = (dll.classic_update_wire_ins)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: format!("WireIn 0x{:02x} set to 0x{:08x}", req.endpoint, req.value),
    }))
}

/// `POST /devices/{serial}/classic/wires/in/update` — Flush all wire-in values.
async fn update_wire_ins(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.classic_update_wire_ins)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: "WireIns updated".to_string(),
    }))
}

/// `POST /devices/{serial}/classic/wires/out/update` — Read all wire-out values from FPGA.
async fn update_wire_outs(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.classic_update_wire_outs)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: "WireOuts updated".to_string(),
    }))
}

/// `GET /devices/{serial}/classic/wires/out?endpoint=N[&update=true]` — Read one WireOut.
///
/// Opal Kelly semantics (ADR-034 §9, D76): returns `GetWireOutValue(N)` from
/// the device's current snapshot, which is shared by all clients. With
/// `update=true` the server first calls `UpdateWireOuts` under the same
/// device lock, so the value is fresh. `UpdateWireOuts` only samples the
/// WireOut inputs; the FPGA's user logic cannot see it.
///
/// `endpoint` is required: a missing value is a 422 (no whole-bank reads).
async fn get_wire_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<WireOutQuery>,
) -> Result<Json<WireOutResponse>, ProblemDetail> {
    let endpoint = query
        .endpoint
        .ok_or_else(|| missing_endpoint("WireOut", "0x20-0x3F"))?;
    enforce_endpoint(&state, &headers, &serial, endpoint, &ok_fpga_core::resource::EndpointType::WireOut, None)?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    // SAFETY: `classic` comes from the locked device and stays valid while
    // `device` (the mutex guard) is alive, which covers both closures.
    let value = crate::snapshot::read_snapshot(
        query.update,
        || FfiError::from_code(unsafe { (dll.classic_update_wire_outs)(classic) })
            .map_err(|e| ffi_error_to_problem(&e)),
        || unsafe { (dll.classic_get_wire_out_value)(classic, endpoint) as u32 },
    )?;
    Ok(Json(WireOutResponse { endpoint, value }))
}

/// `POST /devices/{serial}/classic/triggers/in` — Activate a trigger-in bit.
async fn activate_trigger_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<TriggerInRequest>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    enforce_endpoint(&state, &headers, &serial, req.endpoint, &ok_fpga_core::resource::EndpointType::TriggerIn, Some(1u32 << req.bit))?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.classic_activate_trigger_in)(classic, req.endpoint, req.bit);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: format!("TriggerIn 0x{:02x} bit {} activated", req.endpoint, req.bit),
    }))
}

/// `POST /devices/{serial}/classic/triggers/out/update` — Read trigger-out state.
async fn update_trigger_outs(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.classic_update_trigger_outs)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: "TriggerOuts updated".to_string(),
    }))
}

/// `GET /devices/{serial}/classic/triggers/out?endpoint=N[&mask=M][&update=true]` — Read one TriggerOut.
///
/// Opal Kelly semantics (ADR-034 §9, D76): returns `IsTriggered(N, M)` and
/// `GetTriggerOutVector(N)` from the device's current snapshot, i.e. the
/// triggers seen by the **last** `UpdateTriggerOuts` call by **any** client.
/// With `update=true` the server first calls `UpdateTriggerOuts` under the
/// same device lock. That call reads and clears the latches of **every**
/// TriggerOut on the device, so other clients lose triggers they have not
/// read yet (guide §5.4).
///
/// `endpoint` is required: a missing value is a 422 (no whole-bank reads).
async fn get_trigger_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<TriggerOutQuery>,
) -> Result<Json<TriggerOutResponse>, ProblemDetail> {
    let endpoint = query
        .endpoint
        .ok_or_else(|| missing_endpoint("TriggerOut", "0x60-0x7F"))?;
    enforce_endpoint(&state, &headers, &serial, endpoint, &ok_fpga_core::resource::EndpointType::TriggerOut, None)?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    // SAFETY: as in `get_wire_out`, the guard keeps `classic` valid.
    let (triggered, vector) = crate::snapshot::read_snapshot(
        query.update,
        || FfiError::from_code(unsafe { (dll.classic_update_trigger_outs)(classic) })
            .map_err(|e| ffi_error_to_problem(&e)),
        || unsafe {
            let triggered = (dll.classic_is_triggered)(
                classic, endpoint, query.mask as std::os::raw::c_ulong,
            ) != 0;
            let vector = (dll.classic_get_trigger_out_vector)(classic, endpoint) as u32;
            (triggered, vector)
        },
    )?;
    Ok(Json(TriggerOutResponse { endpoint, triggered, vector }))
}

/// `POST /devices/{serial}/classic/pipes/in` — Write data to a pipe-in endpoint.
async fn write_to_pipe_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PipeRequest>,
) -> Result<Json<PipeTransferResponse>, ProblemDetail> {
    enforce_endpoint(&state, &headers, &serial, req.endpoint, &ok_fpga_core::resource::EndpointType::PipeIn, None)?;
    let data = req.data.as_deref().unwrap_or("");
    let bytes = base64_decode(data)?;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let transferred = unsafe {
        (dll.classic_write_to_pipe_in)(
            classic,
            req.endpoint,
            bytes.len() as std::os::raw::c_long,
            bytes.as_ptr(),
        )
    };

    if transferred < 0 {
        return Err(ProblemDetail::internal(format!(
            "WriteToPipeIn failed with code {}",
            transferred
        )));
    }

    Ok(Json(PipeTransferResponse {
        // `c_long` is 32-bit on Windows but 64-bit on Linux; the count is
        // bounded by the request size, so narrowing to the spec i32 is safe.
        bytes_transferred: transferred as i32,
    }))
}

/// `POST /devices/{serial}/classic/pipes/out` — Read data from a pipe-out endpoint.
async fn read_from_pipe_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PipeRequest>,
) -> Result<Json<PipeReadResponse>, ProblemDetail> {
    enforce_endpoint(&state, &headers, &serial, req.endpoint, &ok_fpga_core::resource::EndpointType::PipeOut, None)?;
    let length = req.length;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let mut buffer = vec![0u8; length as usize];

    let transferred = unsafe {
        (dll.classic_read_from_pipe_out)(
            classic,
            req.endpoint,
            length as std::os::raw::c_long,
            buffer.as_mut_ptr(),
        )
    };

    if transferred < 0 {
        return Err(ProblemDetail::internal(format!(
            "ReadFromPipeOut failed with code {}",
            transferred
        )));
    }

    buffer.truncate(transferred as usize);
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&buffer);

    Ok(Json(PipeReadResponse {
        // `c_long` is 32-bit on Windows but 64-bit on Linux; the count is
        // bounded by the request size, so narrowing to the spec i32 is safe.
        bytes_transferred: transferred as i32,
        data: encoded,
    }))
}

/// `POST /devices/{serial}/classic/pipes/block-in` — Write to a block pipe-in.
async fn write_to_block_pipe_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<BlockPipeRequest>,
) -> Result<Json<PipeTransferResponse>, ProblemDetail> {
    let data = req.data.as_deref().unwrap_or("");
    let bytes = base64_decode(data)?;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let transferred = unsafe {
        (dll.classic_write_to_block_pipe_in)(
            classic,
            req.endpoint,
            req.block_size,
            bytes.len() as std::os::raw::c_long,
            bytes.as_ptr(),
        )
    };

    if transferred < 0 {
        return Err(ProblemDetail::internal(format!(
            "WriteToBlockPipeIn failed with code {}",
            transferred
        )));
    }

    Ok(Json(PipeTransferResponse {
        // `c_long` is 32-bit on Windows but 64-bit on Linux; the count is
        // bounded by the request size, so narrowing to the spec i32 is safe.
        bytes_transferred: transferred as i32,
    }))
}

/// `POST /devices/{serial}/classic/pipes/block-out` — Read from a block pipe-out.
async fn read_from_block_pipe_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<BlockPipeRequest>,
) -> Result<Json<PipeReadResponse>, ProblemDetail> {
    let length = req.length;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let mut buffer = vec![0u8; length as usize];

    let transferred = unsafe {
        (dll.classic_read_from_block_pipe_out)(
            classic,
            req.endpoint,
            req.block_size,
            length as std::os::raw::c_long,
            buffer.as_mut_ptr(),
        )
    };

    if transferred < 0 {
        return Err(ProblemDetail::internal(format!(
            "ReadFromBlockPipeOut failed with code {}",
            transferred
        )));
    }

    buffer.truncate(transferred as usize);
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&buffer);

    Ok(Json(PipeReadResponse {
        // `c_long` is 32-bit on Windows but 64-bit on Linux; the count is
        // bounded by the request size, so narrowing to the spec i32 is safe.
        bytes_transferred: transferred as i32,
        data: encoded,
    }))
}

/// `POST /devices/{serial}/classic/registers/read` — Read a register.
async fn read_register(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, ProblemDetail> {
    enforce_address(&state, &headers, &serial, req.address, false)?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let mut value: u32 = 0;
    unsafe {
        let rc = (dll.classic_read_register)(classic, req.address, &mut value);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(RegisterResponse {
        address: req.address,
        value,
    }))
}

/// `PUT /devices/{serial}/classic/registers/write` — Write a register.
async fn write_register(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    enforce_address(&state, &headers, &serial, req.address, true)?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.classic_write_register)(classic, req.address, req.value);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: format!("Register 0x{:08x} written with 0x{:08x}", req.address, req.value),
    }))
}

/// `POST /devices/{serial}/classic/reset` — Reset the FPGA via Classic data port.
async fn reset_fpga(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.classic_reset_fpga)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: "FPGA reset".to_string(),
    }))
}

// ============================================================================
// Helpers
// ============================================================================

fn base64_decode(data: &str) -> Result<Vec<u8>, ProblemDetail> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| ProblemDetail::bad_request(format!("invalid base64 data: {}", e)))
}

#[cfg(test)]
mod tests {
    //! Tier 3 tests for the read routes. No FrontPanel DLL is loaded
    //! (`handle_pool` is `None`), so a request that passes validation stops
    //! with 501 "FrontPanel DLL not loaded". That is enough to prove the
    //! query was accepted; the snapshot ordering itself is tested in
    //! `crate::snapshot`.
    use super::*;
    use axum::http::{Request, StatusCode};
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::ResourceRegistry;
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        Arc::new(AppState::new(
            Arc::new(DeviceManager::new()),
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

    async fn get(uri: &str) -> (StatusCode, serde_json::Value) {
        let app = routes().with_state(test_state());
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn test_get_wire_out_missing_endpoint_returns_422() {
        let (status, json) = get("/devices/SN1/classic/wires/out").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["conflict"]["code"], "validation");
    }

    #[tokio::test]
    async fn test_get_trigger_out_missing_endpoint_returns_422() {
        let (status, _) = get("/devices/SN1/classic/triggers/out?mask=1").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_get_wire_out_endpoint_with_update_passes_validation() {
        let (status, _) = get("/devices/SN1/classic/wires/out?endpoint=32&update=true").await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_get_trigger_out_endpoint_without_update_passes_validation() {
        let (status, _) = get("/devices/SN1/classic/triggers/out?endpoint=96").await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_get_wire_out_endpoint_without_update_passes_validation() {
        let (status, _) = get("/devices/SN1/classic/wires/out?endpoint=32").await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_get_trigger_out_endpoint_with_update_passes_validation() {
        let (status, _) =
            get("/devices/SN1/classic/triggers/out?endpoint=96&update=true").await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }
}
