//! # AXI Data Port Routes (Phase 19D)
//!
//! REST endpoints for AXI data port operations (Lite, Full, Stream).
//! Implements the `/devices/{serial}/axi/*` paths from ADR-004.
//!
//! ## FFI Integration
//!
//! AXI register read/write tries **AXI Lite first**, then falls back to
//! **AXI Full** (reading/writing 4-byte buffers). The Counters.bit bitfile
//! on the XEM7320 may use AXI Full rather than AXI Lite.
//!
//! Some devices (FP-Gen1/Gen2-USB, i.e. `okDATAPORT_LEGACY`) do not support
//! AXI at all. `GetFPGADataPortAXI()` returns `UnsupportedFeature` on those
//! devices, and all AXI handlers will return 409/501 accordingly.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use ok_fpga_ffi::error::FfiError;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/devices/{serial}/axi/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // AXI Lite
        .route("/devices/{serial}/axi/lite/read", post(axi_lite_read))
        .route("/devices/{serial}/axi/lite/write", put(axi_lite_write))
        .route("/devices/{serial}/axi/lite/reset", post(axi_lite_reset))
        // AXI Full
        .route("/devices/{serial}/axi/full/widths", get(axi_full_widths))
        .route("/devices/{serial}/axi/full/read", post(axi_full_read))
        .route("/devices/{serial}/axi/full/write", post(axi_full_write))
        .route("/devices/{serial}/axi/full/reset", post(axi_full_reset))
        // AXI Stream
        .route(
            "/devices/{serial}/axi/stream/widths",
            get(axi_stream_widths),
        )
        .route("/devices/{serial}/axi/stream/read", post(axi_stream_read))
        .route("/devices/{serial}/axi/stream/write", post(axi_stream_write))
        .route("/devices/{serial}/axi/stream/reset", post(axi_stream_reset))
}

// ============================================================================
// Request / Response Types
// ============================================================================

#[derive(Deserialize)]
struct AXILiteReadRequest {
    /// Register address (64-bit).
    address: u64,
}

#[derive(Deserialize)]
struct AXILiteWriteRequest {
    /// Register address (64-bit).
    address: u64,
    /// Value to write (32-bit).
    value: u32,
}

#[derive(Serialize)]
struct AXILiteReadResponse {
    address: u64,
    value: u32,
}

// ============================================================================
// Helper
// ============================================================================

fn ffi_error_to_problem(e: &FfiError) -> ProblemDetail {
    match e {
        FfiError::FrontPanelError { code, message } => {
            let c = *code;
            if c == ok_fpga_ffi::ok_ErrorCode::ok_InvalidParameter as i32 {
                ProblemDetail::bad_request(message.clone())
            } else if c == ok_fpga_ffi::ok_ErrorCode::ok_AXISlaveError as i32
                || c == ok_fpga_ffi::ok_ErrorCode::ok_AXIDecodeError as i32
            {
                ProblemDetail::bad_request(message.clone())
            } else if c == ok_fpga_ffi::ok_ErrorCode::ok_DeviceNotOpen as i32 {
                ProblemDetail::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, message.clone())
            } else if c == ok_fpga_ffi::ok_ErrorCode::ok_UnsupportedFeature as i32 {
                ProblemDetail::not_implemented(message.clone())
            } else {
                ProblemDetail::internal(message.clone())
            }
        }
        _ => ProblemDetail::internal(e.to_string()),
    }
}

/// Helper macro to get the handle pool and device.
macro_rules! get_device {
    ($state:expr, $serial:expr) => {{
        let pool = $state
            .handle_pool
            .as_ref()
            .ok_or_else(|| ProblemDetail::not_implemented("FrontPanel DLL not loaded"))?;
        pool.get($serial)
            .await
            .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found in handle pool", $serial)))?
    }};
}

// ============================================================================
// AXI Lite Handlers (ADR-019: uses ONLY the AXI Lite sub-handle)
//
// AXI Lite, Full, and Stream are independent sub-interfaces.
// There is NO fallback between them. If the AXI Lite handle is not
// available (device doesn't support AXI, or bitfile doesn't use Lite),
// we return 409 Conflict.
// ============================================================================

/// `POST /devices/{serial}/axi/lite/read` — Read a 32-bit AXI Lite register.
///
/// Uses only the AXI Lite sub-handle. Returns 409 if AXI Lite is not
/// available on this device or bitfile (per ADR-019).
async fn axi_lite_read(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<AXILiteReadRequest>,
) -> Result<Json<AXILiteReadResponse>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_lite = device
        .axi_lite()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Lite not available. Either the device does not support AXI \
             (okDATAPORT_LEGACY), the FPGA is not programmed, or the bitfile \
             does not implement an AXI Lite interface.",
        ))?;
    let dll = device.dll();

    let mut value: u32 = 0;
    // SAFETY: axi_lite handle is valid (from pool after configure), address
    // and value pointer are valid stack allocations.
    unsafe {
        let rc = (dll.axi_lite_read)(axi_lite, req.address, &mut value);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(AXILiteReadResponse {
        address: req.address,
        value,
    }))
}

/// `PUT /devices/{serial}/axi/lite/write` — Write a 32-bit AXI Lite register.
///
/// Uses only the AXI Lite sub-handle. Returns 409 if not available.
async fn axi_lite_write(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<AXILiteWriteRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_lite = device
        .axi_lite()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Lite not available. Either the device does not support AXI \
             (okDATAPORT_LEGACY), the FPGA is not programmed, or the bitfile \
             does not implement an AXI Lite interface.",
        ))?;
    let dll = device.dll();

    // SAFETY: axi_lite handle valid, address and value are plain values.
    unsafe {
        let rc = (dll.axi_lite_write)(axi_lite, req.address, req.value);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(serde_json::json!({"status": "ok", "address": req.address, "value": req.value})))
}

/// `POST /devices/{serial}/axi/lite/reset` — Reset the AXI Lite interface.
///
/// Uses only the AXI Lite sub-handle. Returns 409 if not available.
async fn axi_lite_reset(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_lite = device
        .axi_lite()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Lite not available on this device or bitfile.",
        ))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.axi_lite_reset)(axi_lite);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(serde_json::json!({"status": "ok", "message": "AXI Lite reset"})))
}

// ============================================================================
// AXI Full Handlers (wired to FFI)
// ============================================================================

/// `GET /devices/{serial}/axi/full/widths` — Get AXI Full datapath width.
async fn axi_full_widths(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_full = device
        .axi_full()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or AXI Full not available"))?;
    let dll = device.dll();

    let width = unsafe { (dll.axi_full_get_datapath_width)(axi_full) };

    Ok(Json(serde_json::json!({"datapath_width_bytes": width})))
}

/// `POST /devices/{serial}/axi/full/read` — Read via AXI Full.
async fn axi_full_read(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<AXILiteReadRequest>,
) -> Result<Json<AXILiteReadResponse>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_full = device
        .axi_full()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or AXI Full not available"))?;
    let dll = device.dll();

    let mut buffer = [0u8; 4];
    let mut transferred: u64 = 0;
    unsafe {
        let rc = (dll.axi_full_read)(
            axi_full,
            req.address,
            buffer.as_mut_ptr(),
            4,
            &mut transferred,
        );
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(AXILiteReadResponse {
        address: req.address,
        value: u32::from_le_bytes(buffer),
    }))
}

/// `POST /devices/{serial}/axi/full/write` — Write via AXI Full.
async fn axi_full_write(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    Json(req): Json<AXILiteWriteRequest>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_full = device
        .axi_full()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or AXI Full not available"))?;
    let dll = device.dll();

    let bytes = req.value.to_le_bytes();
    let mut stats: ok_fpga_ffi::okTOperationStatistics = unsafe { std::mem::zeroed() };
    unsafe {
        let rc = (dll.axi_full_write)(
            axi_full,
            req.address,
            bytes.as_ptr(),
            4,
            &mut stats,
        );
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(serde_json::json!({"status": "ok", "address": req.address, "value": req.value})))
}

/// `POST /devices/{serial}/axi/full/reset` — Reset AXI Full interface.
async fn axi_full_reset(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_full = device
        .axi_full()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or AXI Full not available"))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.axi_full_reset)(axi_full);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(serde_json::json!({"status": "ok", "message": "AXI Full reset"})))
}

// ============================================================================
// AXI Stream — Stubs
// ============================================================================

async fn axi_stream_widths(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "AXIStreamGetDatapathWidths requires FFI integration",
    ))
}

async fn axi_stream_read(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "AXIStreamRead requires FFI integration",
    ))
}

async fn axi_stream_write(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "AXIStreamWrite requires FFI integration",
    ))
}

async fn axi_stream_reset(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "AXIStreamReset requires FFI integration",
    ))
}
