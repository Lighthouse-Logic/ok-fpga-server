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
        // Wire-out: single handler dispatches on ?endpoint query param.
        // - With ?endpoint=N → returns {"endpoint": N, "value": V}
        // - Without query param → returns batch map of all wire-outs
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
        // Trigger-out: single handler dispatches on ?endpoint query param.
        // - With ?endpoint=N → returns per-endpoint response
        // - Without query param → returns batch map of all trigger-outs
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
/// Per OpenAPI spec, `endpoint` is a query parameter. When present,
/// returns a single wire-out value; when absent, returns all wire-outs.
#[derive(Deserialize)]
struct WireOutQuery {
    endpoint: Option<i32>,
}

/// Query parameters for `GET /devices/{serial}/classic/triggers/out`.
///
/// Per OpenAPI spec, `endpoint` and `mask` are query parameters.
/// When `endpoint` is present, returns a single trigger-out result;
/// when absent, returns all trigger-outs.
#[derive(Deserialize)]
struct TriggerOutQuery {
    endpoint: Option<i32>,
    #[serde(default = "default_mask")]
    mask: u32,
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

/// `GET /devices/{serial}/classic/wires/out?endpoint=N` — Read wire-out value(s).
///
/// Per OpenAPI spec, `endpoint` is a **query parameter** (not a path segment).
///
/// - When `?endpoint=N` is present: calls `UpdateWireOuts` then returns
///   `{"endpoint": N, "value": V}` for that specific endpoint.
/// - When no query param: returns a batch map of all standard wire-outs
///   (0x20–0x3F) as `{"0x20": V, "0x21": V, ...}`.
async fn get_wire_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<WireOutQuery>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    if let Some(endpoint) = query.endpoint {
        enforce_endpoint(&state, &headers, &serial, endpoint, &ok_fpga_core::resource::EndpointType::WireOut, None)?;
    }
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    if let Some(endpoint) = query.endpoint {
        // Single endpoint query — return typed WireOutResponse.
        let value = unsafe {
            let rc = (dll.classic_update_wire_outs)(classic);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
            (dll.classic_get_wire_out_value)(classic, endpoint) as u32
        };
        Ok(Json(serde_json::to_value(WireOutResponse { endpoint, value }).unwrap()))
    } else {
        // Batch query — return all standard wire-outs (0x20–0x3F).
        let wires = unsafe {
            let rc = (dll.classic_update_wire_outs)(classic);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
            let mut map = serde_json::Map::new();
            for ep in 0x20..=0x3F_i32 {
                let val = (dll.classic_get_wire_out_value)(classic, ep) as u32;
                map.insert(format!("0x{:02x}", ep), serde_json::json!(val));
            }
            map
        };
        Ok(Json(serde_json::Value::Object(wires)))
    }
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

/// `GET /devices/{serial}/classic/triggers/out?endpoint=N&mask=M` — Read trigger-out.
///
/// Per OpenAPI spec, `endpoint` and `mask` are **query parameters**.
///
/// - When `?endpoint=N` is present: calls `UpdateTriggerOuts`, reads the
///   trigger vector, checks against `mask` (default 0xFFFFFFFF), and returns
///   `{"endpoint": N, "triggered": bool, "vector": V}`.
/// - When no query param: returns a batch map of all standard trigger-outs
///   (0x60–0x7F) as `{"0x60": V, "0x61": V, ...}`.
async fn get_trigger_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<TriggerOutQuery>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    if let Some(endpoint) = query.endpoint {
        enforce_endpoint(&state, &headers, &serial, endpoint, &ok_fpga_core::resource::EndpointType::TriggerOut, None)?;
    }
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    if let Some(endpoint) = query.endpoint {
        // Single endpoint query.
        let (triggered, vector) = unsafe {
            let rc = (dll.classic_update_trigger_outs)(classic);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
            let is_triggered = (dll.classic_is_triggered)(
                classic, endpoint, query.mask as std::os::raw::c_ulong,
            ) != 0;
            let vec_val = (dll.classic_get_trigger_out_vector)(classic, endpoint) as u32;
            (is_triggered, vec_val)
        };
        Ok(Json(serde_json::to_value(TriggerOutResponse {
            endpoint,
            triggered,
            vector,
        }).unwrap()))
    } else {
        // Batch query — return all standard trigger-outs (0x60–0x7F).
        let triggers = unsafe {
            let rc = (dll.classic_update_trigger_outs)(classic);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
            let mut map = serde_json::Map::new();
            for ep in 0x60..=0x7F_i32 {
                let val = (dll.classic_get_trigger_out_vector)(classic, ep);
                map.insert(format!("0x{:02x}", ep), serde_json::json!(val));
            }
            map
        };
        Ok(Json(serde_json::Value::Object(triggers)))
    }
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
