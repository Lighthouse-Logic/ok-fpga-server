//! # Classic Data Port Routes (Phase 19C)
//!
//! REST endpoints for Classic data port operations (wire, trigger, pipe,
//! register). Implements the `/devices/{serial}/classic/*` paths from ADR-004.
//!
//! All handlers are wired to the FrontPanel DLL via the `DeviceHandlePool`
//! in `AppState`. Each handler locks the per-device mutex, performs the FFI
//! call, and releases the lock.

use std::sync::Arc;

use axum::extract::{Path, State};
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
        .route(
            "/devices/{serial}/classic/wires/out/{address}",
            get(get_wire_out),
        )
        // Batch wire-out (read all standard wire-outs at once)
        .route(
            "/devices/{serial}/classic/wires/out",
            get(get_all_wire_outs),
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
        .route(
            "/devices/{serial}/classic/triggers/out/{address}",
            get(is_triggered),
        )
        // Batch trigger-out (read all standard trigger-outs at once)
        .route(
            "/devices/{serial}/classic/triggers/out",
            get(get_all_trigger_outs),
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

#[derive(Deserialize)]
struct TriggerOutRequest {
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

#[derive(Serialize)]
struct WireOutResponse {
    endpoint: i32,
    value: u32,
}

#[derive(Serialize)]
struct TriggerOutResponse {
    endpoint: i32,
    triggered: bool,
    mask: u32,
}

#[derive(Serialize)]
struct RegisterResponse {
    address: u32,
    value: u32,
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
    Json(req): Json<WireInRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "endpoint": req.endpoint, "value": req.value})))
}

/// `POST /devices/{serial}/classic/wires/in/update` — Flush all wire-in values.
async fn update_wire_ins(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok"})))
}

/// `POST /devices/{serial}/classic/wires/out/update` — Read all wire-out values from FPGA.
async fn update_wire_outs(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok"})))
}

/// `GET /devices/{serial}/classic/wires/out/{address}` — Read a wire-out value.
///
/// Calls `UpdateWireOuts` then `GetWireOutValue` atomically under the device mutex.
async fn get_wire_out(
    State(state): State<Arc<AppState>>,
    Path((serial, address)): Path<(String, String)>,
) -> Result<Json<WireOutResponse>, ProblemDetail> {
    let endpoint: i32 = address.parse().map_err(|_| {
        ProblemDetail::bad_request(format!("invalid endpoint address: '{}'", address))
    })?;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    // SAFETY: classic handle valid, endpoint address validated above.
    let value = unsafe {
        let rc = (dll.classic_update_wire_outs)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
        (dll.classic_get_wire_out_value)(classic, endpoint) as u32
    };

    Ok(Json(WireOutResponse { endpoint, value }))
}

/// `GET /devices/{serial}/classic/wires/out` — Read all standard wire-out values.
///
/// Calls `UpdateWireOuts` then reads all standard wire-out endpoints (0x20–0x3F).
/// Returns a map of endpoint address → value.
async fn get_all_wire_outs(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    // SAFETY: classic handle is valid (from pool after configure).
    // Wire-out endpoints are 0x20..=0x3F (32 endpoints).
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

/// `POST /devices/{serial}/classic/triggers/in` — Activate a trigger-in bit.
async fn activate_trigger_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<TriggerInRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "endpoint": req.endpoint, "bit": req.bit})))
}

/// `POST /devices/{serial}/classic/triggers/out/update` — Read trigger-out state.
async fn update_trigger_outs(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok"})))
}

/// `GET /devices/{serial}/classic/triggers/out/{address}` — Check if triggered.
async fn is_triggered(
    State(state): State<Arc<AppState>>,
    Path((serial, address)): Path<(String, String)>,
    Json(req): Json<TriggerOutRequest>,
) -> Result<Json<TriggerOutResponse>, ProblemDetail> {
    let endpoint: i32 = address.parse().map_err(|_| {
        ProblemDetail::bad_request(format!("invalid endpoint address: '{}'", address))
    })?;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let triggered = unsafe {
        let rc = (dll.classic_update_trigger_outs)(classic);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
        (dll.classic_is_triggered)(classic, endpoint, req.mask as std::os::raw::c_ulong) != 0
    };

    Ok(Json(TriggerOutResponse {
        endpoint,
        triggered,
        mask: req.mask,
    }))
}

/// `GET /devices/{serial}/classic/triggers/out` — Read all trigger-out vectors.
///
/// Calls `UpdateTriggerOuts` then reads `GetTriggerOutVector` for all standard
/// trigger-out endpoints (0x60–0x7F). Returns a map of endpoint → vector value.
async fn get_all_trigger_outs(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    // SAFETY: classic handle is valid (from pool after configure).
    // Trigger-out endpoints are 0x60..=0x7F (32 endpoints).
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

/// `POST /devices/{serial}/classic/pipes/in` — Write data to a pipe-in endpoint.
async fn write_to_pipe_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<PipeRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "bytes_transferred": transferred})))
}

/// `POST /devices/{serial}/classic/pipes/out` — Read data from a pipe-out endpoint.
async fn read_from_pipe_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<PipeRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "bytes_transferred": transferred, "data": encoded})))
}

/// `POST /devices/{serial}/classic/pipes/block-in` — Write to a block pipe-in.
async fn write_to_block_pipe_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<BlockPipeRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "bytes_transferred": transferred})))
}

/// `POST /devices/{serial}/classic/pipes/block-out` — Read from a block pipe-out.
async fn read_from_block_pipe_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<BlockPipeRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "bytes_transferred": transferred, "data": encoded})))
}

/// `POST /devices/{serial}/classic/registers/read` — Read a register.
async fn read_register(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, ProblemDetail> {
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
    Json(req): Json<RegisterRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "address": req.address, "value": req.value})))
}

/// `POST /devices/{serial}/classic/reset` — Reset the FPGA via Classic data port.
async fn reset_fpga(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
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

    Ok(Json(serde_json::json!({"status": "ok", "serial": serial, "message": "FPGA reset"})))
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
