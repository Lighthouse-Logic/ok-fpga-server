// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # AXI Data Port Routes (Phase 19D — ADR-019)
//!
//! REST endpoints for AXI data port operations (Lite, Full, Stream).
//! Implements the `/devices/{serial}/axi/*` paths from ADR-004.
//!
//! ## AXI Sub-Interface Independence (ADR-019)
//!
//! AXI Lite, Full, and Stream are independent, non-polymorphic sub-interfaces.
//! Each handler uses only its own sub-handle — there is NO fallback between them.
//!
//! ## AXI Stream (Addressless)
//!
//! Unlike Lite and Full, AXI Stream is addressless — it operates as a FIFO.
//! REST endpoints accept/return raw byte payloads via JSON-encoded base64 or
//! length-specified buffers. For high-throughput streaming, gRPC bidirectional
//! streaming is recommended (see notes-on-AXI.md).

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

/// StatusResponse for AXI operations — matches OpenAPI `StatusResponse` (Phase 37).
#[derive(Serialize)]
struct StatusResponse {
    success: bool,
    message: String,
}

/// Datapath width response (Phase 37).
#[derive(Serialize)]
struct DatapathWidthResponse {
    width: i32,
}

/// AXI Stream datapath widths response (Phase 37).
#[derive(Serialize)]
struct StreamWidthsResponse {
    write_datapath_width_bytes: i32,
    read_datapath_width_bytes: i32,
}

/// AXI Stream read response (Phase 37).
#[derive(Serialize)]
struct AXIStreamReadResponse {
    bytes_transferred: u64,
    data_base64: String,
}

/// AXI Stream write response (Phase 37).
#[derive(Serialize)]
struct AXIStreamWriteResponse {
    bytes_written: u64,
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
    headers: axum::http::HeaderMap,
    Json(req): Json<AXILiteReadRequest>,
) -> Result<Json<AXILiteReadResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_address_span_access(uid, &serial, req.address, 4, false)
    })?;
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
    headers: axum::http::HeaderMap,
    Json(req): Json<AXILiteWriteRequest>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_address_span_access(uid, &serial, req.address, 4, true)
    })?;
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

    Ok(Json(StatusResponse {
        success: true,
        message: format!("AXI Lite 0x{:016x} written with 0x{:08x}", req.address, req.value),
    }))
}

/// `POST /devices/{serial}/axi/lite/reset` — Reset the AXI Lite interface.
///
/// Uses only the AXI Lite sub-handle. Returns 409 if not available.
async fn axi_lite_reset(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_reset_access(uid, &serial, "AXILiteReset")
    })?;
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

    Ok(Json(StatusResponse {
        success: true,
        message: "AXI Lite reset".to_string(),
    }))
}

// ============================================================================
// AXI Full Handlers (wired to FFI)
// ============================================================================

/// `GET /devices/{serial}/axi/full/widths` — Get AXI Full datapath width.
async fn axi_full_widths(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<DatapathWidthResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state.resource_registry.check_device_access(uid, &serial)
    })?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_full = device
        .axi_full()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or AXI Full not available"))?;
    let dll = device.dll();

    let width = unsafe { (dll.axi_full_get_datapath_width)(axi_full) };

    Ok(Json(DatapathWidthResponse { width: width.into() }))
}

/// `POST /devices/{serial}/axi/full/read` — Read via AXI Full.
async fn axi_full_read(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AXILiteReadRequest>,
) -> Result<Json<AXILiteReadResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_address_span_access(uid, &serial, req.address, 4, false)
    })?;
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
    headers: axum::http::HeaderMap,
    Json(req): Json<AXILiteWriteRequest>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_address_span_access(uid, &serial, req.address, 4, true)
    })?;
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

    Ok(Json(StatusResponse {
        success: true,
        message: format!("AXI Full 0x{:016x} written with 0x{:08x}", req.address, req.value),
    }))
}

/// `POST /devices/{serial}/axi/full/reset` — Reset AXI Full interface.
async fn axi_full_reset(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_reset_access(uid, &serial, "AXIFullReset")
    })?;
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

    Ok(Json(StatusResponse {
        success: true,
        message: "AXI Full reset".to_string(),
    }))
}

// ============================================================================
// AXI Stream Handlers (ADR-019: uses ONLY the AXI Stream sub-handle)
//
// AXI Stream is ADDRESSLESS — it operates as a FIFO. Read/Write transfer
// byte buffers without specifying an address. Each sub-interface is
// independent per ADR-019.
// ============================================================================

/// Request body for AXI Stream read (addressless).
#[derive(Deserialize)]
struct AXIStreamReadRequest {
    /// Number of bytes to read from the stream FIFO.
    length: u64,
}

/// Request body for AXI Stream write (addressless).
#[derive(Deserialize)]
struct AXIStreamWriteRequest {
    /// Base64-encoded data to write to the stream FIFO.
    data_base64: String,
}

/// `GET /devices/{serial}/axi/stream/widths` — Get AXI Stream datapath widths.
///
/// Returns both read and write datapath widths (they may differ).
async fn axi_stream_widths(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<StreamWidthsResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state.resource_registry.check_device_access(uid, &serial)
    })?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_stream = device
        .axi_stream()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Stream not available. Either the device does not support AXI \
             (okDATAPORT_LEGACY), the FPGA is not programmed, or the bitfile \
             does not implement an AXI Stream interface.",
        ))?;
    let dll = device.dll();

    let write_width = unsafe { (dll.axi_stream_get_write_datapath_width)(axi_stream) };
    let read_width = unsafe { (dll.axi_stream_get_read_datapath_width)(axi_stream) };

    Ok(Json(StreamWidthsResponse {
        write_datapath_width_bytes: write_width.into(),
        read_datapath_width_bytes: read_width.into(),
    }))
}

/// `POST /devices/{serial}/axi/stream/read` — Read bytes from AXI Stream FIFO.
///
/// AXI Stream is addressless — reads the next `length` bytes from the FIFO.
/// Returns the data as base64-encoded JSON.
async fn axi_stream_read(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AXIStreamReadRequest>,
) -> Result<Json<AXIStreamReadResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state.resource_registry.check_device_access(uid, &serial)
    })?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_stream = device
        .axi_stream()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Stream not available on this device or bitfile.",
        ))?;
    let dll = device.dll();

    // Allocate buffer for the requested read length.
    let mut buffer = vec![0u8; req.length as usize];
    let mut transferred: u64 = 0;

    // SAFETY: axi_stream handle is valid (from pool), buffer is valid allocation.
    unsafe {
        let rc = (dll.axi_stream_read)(
            axi_stream,
            buffer.as_mut_ptr(),
            req.length,
            &mut transferred,
        );
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    // Truncate buffer to actual bytes transferred.
    buffer.truncate(transferred as usize);

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&buffer);

    Ok(Json(AXIStreamReadResponse {
        bytes_transferred: transferred,
        data_base64: encoded,
    }))
}

/// `POST /devices/{serial}/axi/stream/write` — Write bytes to AXI Stream FIFO.
///
/// AXI Stream is addressless — writes the provided bytes into the FIFO.
/// Accepts base64-encoded data in the request body.
async fn axi_stream_write(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AXIStreamWriteRequest>,
) -> Result<Json<AXIStreamWriteResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_device_write_access(uid, &serial, "AXIStreamWrite")
    })?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_stream = device
        .axi_stream()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Stream not available on this device or bitfile.",
        ))?;
    let dll = device.dll();

    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&req.data_base64)
        .map_err(|e| ProblemDetail::bad_request(format!("Invalid base64: {}", e)))?;

    let mut transferred: u64 = 0;

    // SAFETY: axi_stream handle is valid, data buffer is valid.
    unsafe {
        let rc = (dll.axi_stream_write)(
            axi_stream,
            data.as_ptr(),
            data.len() as u64,
            &mut transferred,
        );
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(AXIStreamWriteResponse {
        bytes_written: transferred,
    }))
}

/// `POST /devices/{serial}/axi/stream/reset` — Reset the AXI Stream interface.
async fn axi_stream_reset(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    super::classic::enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_reset_access(uid, &serial, "AXIStreamReset")
    })?;
    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let axi_stream = device
        .axi_stream()
        .ok_or_else(|| ProblemDetail::conflict(
            "AXI Stream not available on this device or bitfile.",
        ))?;
    let dll = device.dll();

    unsafe {
        let rc = (dll.axi_stream_reset)(axi_stream);
        FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
    }

    Ok(Json(StatusResponse {
        success: true,
        message: "AXI Stream reset".to_string(),
    }))
}
