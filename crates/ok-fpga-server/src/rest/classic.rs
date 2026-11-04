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
            get(read_from_pipe_out),
        )
        .route(
            "/devices/{serial}/classic/pipes/block-in",
            post(write_to_block_pipe_in),
        )
        .route(
            "/devices/{serial}/classic/pipes/block-out",
            get(read_from_block_pipe_out),
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
    /// When `true` (the OpenAPI default, S-07), `UpdateWireIns` is called
    /// immediately after `SetWireInValue` under the same device lock.
    #[serde(default = "default_auto_update")]
    auto_update: bool,
}

fn default_mask() -> u32 {
    0xFFFF_FFFF
}

fn default_auto_update() -> bool {
    true
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
    /// Bits to test for `triggered` (`IsTriggered`). Defaults to **0**
    /// (D78), so `triggered` is always `false` until the client names the
    /// bits it cares about. Why not all-ones? With `0xFFFFFFFF` a client
    /// that forgot the mask would *appear* to work, but would also report
    /// `triggered = true` when some unrelated bit fired. The full vector is
    /// returned in `vector` regardless of the mask.
    #[serde(default)]
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

/// 422 for a required query parameter or invalid numeric bound on pipe routes.
fn validation_422(msg: impl Into<String>) -> ProblemDetail {
    ProblemDetail::from_resource_error(ok_fpga_core::resource::ResourceError::validation(
        msg.into(),
    ))
}

/// Query parameters for `POST /devices/{serial}/classic/pipes/in` (S-05).
#[derive(Deserialize)]
struct PipeInQuery {
    endpoint: Option<i32>,
}

/// Query parameters for `GET /devices/{serial}/classic/pipes/out` (S-05).
#[derive(Deserialize)]
struct PipeOutQuery {
    endpoint: Option<i32>,
    length: Option<i32>,
}

/// Query parameters for `POST /devices/{serial}/classic/pipes/block-in` (S-05).
#[derive(Deserialize)]
struct BlockPipeInQuery {
    endpoint: Option<i32>,
    block_size: Option<i32>,
}

/// Query parameters for `GET /devices/{serial}/classic/pipes/block-out` (S-05).
#[derive(Deserialize)]
struct BlockPipeOutQuery {
    endpoint: Option<i32>,
    block_size: Option<i32>,
    length: Option<i32>,
}

/// Bulk register read request — matches OpenAPI `POST /classic/registers/read` (S-06).
#[derive(Deserialize)]
struct RegisterReadRequest {
    addresses: Vec<u32>,
}

/// Single register address + data entry — matches OpenAPI `RegisterEntry` (S-06).
#[derive(Serialize, Deserialize)]
struct RegisterEntry {
    address: u32,
    data: u32,
}

/// Bulk register read response — matches OpenAPI `POST /classic/registers/read` (S-06).
#[derive(Serialize)]
struct RegisterReadResponse {
    entries: Vec<RegisterEntry>,
}

/// Bulk register write request — matches OpenAPI `PUT /classic/registers/write` (S-06).
#[derive(Deserialize)]
struct RegisterWriteRequest {
    entries: Vec<RegisterEntry>,
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
/// Run an access-control check closure against the caller's `X-User-Id`
/// header, applying `EndpointEnforcement::{Off, Warn, Enforce}` (ADR-024, 44-E).
pub(crate) fn enforce_with_user<F>(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    serial: &str,
    check: F,
) -> Result<(), ProblemDetail>
where
    F: FnOnce(&str) -> Result<(), ok_fpga_core::resource::AccessDenied>,
{
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
        None => match state.enforcement() {
            EndpointEnforcement::Warn => {
                tracing::warn!(
                    serial = %serial,
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
        },
    };

    if let Err(denied) = check(uid) {
        match state.enforcement() {
            EndpointEnforcement::Warn => {
                tracing::warn!(
                    user_id = %uid,
                    serial = %serial,
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

/// Enforce endpoint access for a Classic operation (ADR-024 §4, handler-level).
fn enforce_endpoint(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    serial: &str,
    endpoint: i32,
    ep_type: &ok_fpga_core::resource::EndpointType,
    value: Option<u32>,
) -> Result<(), ProblemDetail> {
    enforce_with_user(state, headers, serial, |uid| {
        let Ok(ep_u8) = u8::try_from(endpoint) else {
            return Err(ok_fpga_core::resource::AccessDenied::NoCoveringResource {
                user_id: uid.to_string(),
                device_serial: serial.to_string(),
                detail: format!("endpoint {} ({})", endpoint, ep_type),
            });
        };
        state
            .resource_registry
            .check_endpoint_access(uid, serial, ep_u8, ep_type, value)
    })
}

/// Enforce address access for a register operation (ADR-024 §4, handler-level).
fn enforce_address(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    serial: &str,
    address: u32,
    is_write: bool,
) -> Result<(), ProblemDetail> {
    enforce_with_user(state, headers, serial, |uid| {
        state
            .resource_registry
            .check_address_access(uid, serial, u64::from(address), is_write)
    })
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

        if req.auto_update {
            let rc = (dll.classic_update_wire_ins)(classic);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
        }
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
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_device_write_access(uid, &serial, "UpdateWireIns")
    })?;
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
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    enforce_with_user(&state, &headers, &serial, |uid| {
        state.resource_registry.check_device_access(uid, &serial)
    })?;
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
    // Validate `req.bit` in `0..=31` BEFORE `1u32 << req.bit` (avoiding shift
    // overflow panic in debug builds) and before enforcement (44-E).
    if !(0..=31).contains(&req.bit) {
        return Err(ProblemDetail::from_resource_error(
            ok_fpga_core::resource::ResourceError::validation(format!(
                "trigger bit {} is out of range (expected 0..=31)",
                req.bit
            )),
        ));
    }
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
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_trigger_out_update_access(uid, &serial)
    })?;
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
    if query.update {
        enforce_with_user(&state, &headers, &serial, |uid| {
            state
                .resource_registry
                .check_trigger_out_update_access(uid, &serial)
        })?;
    }
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

/// `POST /devices/{serial}/classic/pipes/in?endpoint={endpoint}` — Write raw binary data to a pipe-in endpoint (S-05).
async fn write_to_pipe_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<PipeInQuery>,
    bytes: axum::body::Bytes,
) -> Result<Json<PipeTransferResponse>, ProblemDetail> {
    let endpoint = query
        .endpoint
        .ok_or_else(|| validation_422("query parameter 'endpoint' is required (PipeIn address 0x80-0x9F)"))?;
    enforce_endpoint(
        &state,
        &headers,
        &serial,
        endpoint,
        &ok_fpga_core::resource::EndpointType::PipeIn,
        None,
    )?;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let transferred = unsafe {
        (dll.classic_write_to_pipe_in)(
            classic,
            endpoint,
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

/// `GET /devices/{serial}/classic/pipes/out?endpoint={endpoint}&length={length}` — Read raw binary data from a pipe-out endpoint (S-05).
async fn read_from_pipe_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<PipeOutQuery>,
) -> Result<([(axum::http::header::HeaderName, &'static str); 1], axum::body::Bytes), ProblemDetail> {
    let endpoint = query
        .endpoint
        .ok_or_else(|| validation_422("query parameter 'endpoint' is required (PipeOut address 0xA0-0xBF)"))?;
    let length = query
        .length
        .ok_or_else(|| validation_422("query parameter 'length' is required"))?;
    if length < 0 {
        return Err(validation_422(format!(
            "query parameter 'length' must be >= 0 (got {})",
            length
        )));
    }

    enforce_endpoint(
        &state,
        &headers,
        &serial,
        endpoint,
        &ok_fpga_core::resource::EndpointType::PipeOut,
        None,
    )?;

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
            endpoint,
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

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        axum::body::Bytes::from(buffer),
    ))
}

/// `POST /devices/{serial}/classic/pipes/block-in?endpoint={endpoint}&block_size={block_size}` — Write raw binary data to a block pipe-in (S-05).
async fn write_to_block_pipe_in(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<BlockPipeInQuery>,
    bytes: axum::body::Bytes,
) -> Result<Json<PipeTransferResponse>, ProblemDetail> {
    let endpoint = query
        .endpoint
        .ok_or_else(|| validation_422("query parameter 'endpoint' is required (BTPipeIn address 0x80-0x9F)"))?;
    let block_size = query
        .block_size
        .ok_or_else(|| validation_422("query parameter 'block_size' is required"))?;
    if block_size <= 0 {
        return Err(validation_422(format!(
            "query parameter 'block_size' must be > 0 (got {})",
            block_size
        )));
    }

    enforce_endpoint(
        &state,
        &headers,
        &serial,
        endpoint,
        &ok_fpga_core::resource::EndpointType::BtPipeIn,
        None,
    )?;

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let transferred = unsafe {
        (dll.classic_write_to_block_pipe_in)(
            classic,
            endpoint,
            block_size,
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

/// `GET /devices/{serial}/classic/pipes/block-out?endpoint={endpoint}&block_size={block_size}&length={length}` — Read raw binary data from a block pipe-out (S-05).
async fn read_from_block_pipe_out(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Query(query): Query<BlockPipeOutQuery>,
) -> Result<([(axum::http::header::HeaderName, &'static str); 1], axum::body::Bytes), ProblemDetail> {
    let endpoint = query
        .endpoint
        .ok_or_else(|| validation_422("query parameter 'endpoint' is required (BTPipeOut address 0xA0-0xBF)"))?;
    let block_size = query
        .block_size
        .ok_or_else(|| validation_422("query parameter 'block_size' is required"))?;
    let length = query
        .length
        .ok_or_else(|| validation_422("query parameter 'length' is required"))?;
    if block_size <= 0 {
        return Err(validation_422(format!(
            "query parameter 'block_size' must be > 0 (got {})",
            block_size
        )));
    }
    if length < 0 {
        return Err(validation_422(format!(
            "query parameter 'length' must be >= 0 (got {})",
            length
        )));
    }

    enforce_endpoint(
        &state,
        &headers,
        &serial,
        endpoint,
        &ok_fpga_core::resource::EndpointType::BtPipeOut,
        None,
    )?;

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
            endpoint,
            block_size,
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

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        axum::body::Bytes::from(buffer),
    ))
}

/// `POST /devices/{serial}/classic/registers/read` — Read one or more registers (S-06).
///
/// Checks `enforce_address` for every address in `req.addresses` BEFORE
/// acquiring the device lock or reading from hardware.
async fn read_register(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RegisterReadRequest>,
) -> Result<Json<RegisterReadResponse>, ProblemDetail> {
    for &addr in &req.addresses {
        enforce_address(&state, &headers, &serial, addr, false)?;
    }

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    let mut entries = Vec::with_capacity(req.addresses.len());
    for &addr in &req.addresses {
        let mut data: u32 = 0;
        unsafe {
            let rc = (dll.classic_read_register)(classic, addr, &mut data);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
        }
        entries.push(RegisterEntry {
            address: addr,
            data,
        });
    }

    Ok(Json(RegisterReadResponse { entries }))
}

/// `PUT /devices/{serial}/classic/registers/write` — Write one or more registers (S-06).
///
/// Checks `enforce_address` for every entry in `req.entries` BEFORE
/// acquiring the device lock or writing to hardware, so an access violation on
/// a later entry never leaves earlier entries partially written.
async fn write_register(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RegisterWriteRequest>,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    for entry in &req.entries {
        enforce_address(&state, &headers, &serial, entry.address, true)?;
    }

    let device_arc = get_device!(state, &serial);
    let device = device_arc.lock().await;

    let classic = device
        .classic_port()
        .ok_or_else(|| ProblemDetail::conflict("FPGA not programmed or Classic port not available"))?;
    let dll = device.dll();

    for entry in &req.entries {
        unsafe {
            let rc = (dll.classic_write_register)(classic, entry.address, entry.data);
            FfiError::from_code(rc).map_err(|e| ffi_error_to_problem(&e))?;
        }
    }

    Ok(Json(StatusResponse {
        success: true,
        message: format!("{} register(s) written", req.entries.len()),
    }))
}

/// `POST /devices/{serial}/classic/reset` — Reset the FPGA via Classic data port.
async fn reset_fpga(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<StatusResponse>, ProblemDetail> {
    enforce_with_user(&state, &headers, &serial, |uid| {
        state
            .resource_registry
            .check_reset_access(uid, &serial, "ResetFPGA")
    })?;
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

    // D78: an omitted mask must be 0, never all-ones.
    #[test]
    fn test_trigger_out_query_mask_omitted_defaults_to_zero() {
        let uri: axum::http::Uri = "http://h/x?endpoint=96".parse().unwrap();
        let Query(q) = Query::<TriggerOutQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(q.mask, 0);
    }

    #[test]
    fn test_trigger_out_query_mask_given_is_used() {
        let uri: axum::http::Uri = "http://h/x?endpoint=96&mask=5".parse().unwrap();
        let Query(q) = Query::<TriggerOutQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(q.mask, 5);
    }

    // =========================================================================
    // Phase 44-E: Enforcement Pipeline & Validation Tests (B2, S-12..S-20, D75)
    // =========================================================================

    fn test_enforced_state() -> Arc<AppState> {
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
            crate::state::EndpointEnforcement::Enforce,
            crate::config::ServerConfig::default(),
            None,
        ))
    }

    async fn call_app(
        state: Arc<AppState>,
        method: &str,
        uri: &str,
        user_id: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let app = routes()
            .merge(crate::rest::axi::routes())
            .with_state(state);
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(uid) = user_id {
            builder = builder.header("X-User-Id", uid);
        }
        let req_body = if let Some(b) = body {
            builder = builder.header("Content-Type", "application/json");
            axum::body::Body::from(b.to_string())
        } else {
            axum::body::Body::empty()
        };
        let request = builder.body(req_body).unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn test_activate_trigger_in_invalid_bit_returns_422() {
        let state = test_enforced_state();
        // `bit: 32` and `bit: -1` must return 422 before `1u32 << req.bit` and
        // before enforcement runs.
        for bad_bit in [32, -1] {
            let (status, json) = call_app(
                state.clone(),
                "POST",
                "/devices/SN1/classic/triggers/in",
                None,
                Some(serde_json::json!({ "endpoint": 0x40, "bit": bad_bit })),
            )
            .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(json["conflict"]["code"], "validation");
        }
    }

    #[tokio::test]
    async fn test_reset_fpga_readonly_holder_returns_403() {
        use ok_fpga_core::resource::{AccessMode, Resource};
        let state = test_enforced_state();
        let r = Resource::new(
            "SN1:r1".into(),
            "r1".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:r1", &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        let (status, _) = call_app(
            state,
            "POST",
            "/devices/SN1/classic/reset",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_reset_fpga_shared_holder_passes_enforcement() {
        use ok_fpga_core::resource::{AccessMode, Resource};
        let state = test_enforced_state();
        let r = Resource::new(
            "SN1:r1".into(),
            "r1".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Passes enforcement -> reaches `get_device!` -> 501 because `handle_pool` is `None`.
        let (status, _) = call_app(
            state,
            "POST",
            "/devices/SN1/classic/reset",
            Some("alice"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_axi_reset_readonly_holder_returns_403() {
        use ok_fpga_core::resource::{AccessMode, Resource};
        let state = test_enforced_state();
        let r = Resource::new(
            "SN1:r1".into(),
            "r1".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:r1", &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        for path in [
            "/devices/SN1/axi/lite/reset",
            "/devices/SN1/axi/full/reset",
            "/devices/SN1/axi/stream/reset",
        ] {
            let (status, _) = call_app(state.clone(), "POST", path, Some("carol"), None).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "expected 403 on {}", path);
        }
    }

    #[tokio::test]
    async fn test_update_trigger_outs_readonly_denied_when_device_has_private_trigger_out() {
        use ok_fpga_core::resource::{
            AccessMode, EndpointDeclaration, EndpointEntry, EndpointType, Resource,
        };
        let state = test_enforced_state();
        let mut r = Resource::new(
            "SN1:r1".into(),
            "r1".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x60,
                    ep_type: EndpointType::TriggerOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0x61,
                    ep_type: EndpointType::TriggerOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: true,
                },
            ],
        });
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:r1", &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        // Direct `POST /triggers/out/update` is denied for ReadOnly holder.
        let (status, _) = call_app(
            state.clone(),
            "POST",
            "/devices/SN1/classic/triggers/out/update",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // `GET /triggers/out?endpoint=96&update=true` on non-private 0x60 is
        // also denied when `update=true` because UpdateTriggerOuts clears the
        // private 0x61 latch too (D75 F2).
        let (status, _) = call_app(
            state.clone(),
            "GET",
            "/devices/SN1/classic/triggers/out?endpoint=96&update=true",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // Snapshot read without `update=true` on non-private 0x60 passes enforcement.
        let (status, _) = call_app(
            state,
            "GET",
            "/devices/SN1/classic/triggers/out?endpoint=96",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_update_trigger_outs_readonly_allowed_when_no_private_trigger_out() {
        use ok_fpga_core::resource::{
            AccessMode, EndpointDeclaration, EndpointEntry, EndpointType, Resource,
        };
        let state = test_enforced_state();
        let mut r = Resource::new(
            "SN1:r1".into(),
            "r1".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![EndpointEntry {
                address: 0x60,
                ep_type: EndpointType::TriggerOut,
                access: AccessMode::Shared,
                description: None,
                private: false,
            }],
        });
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:r1", &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        let (status, _) = call_app(
            state.clone(),
            "POST",
            "/devices/SN1/classic/triggers/out/update",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        let (status, _) = call_app(
            state,
            "GET",
            "/devices/SN1/classic/triggers/out?endpoint=96&update=true",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    async fn call_app_raw(
        state: Arc<AppState>,
        method: &str,
        uri: &str,
        user_id: Option<&str>,
        body: Vec<u8>,
    ) -> (StatusCode, serde_json::Value) {
        let app = routes()
            .merge(crate::rest::axi::routes())
            .with_state(state);
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Content-Type", "application/octet-stream");
        if let Some(uid) = user_id {
            builder = builder.header("X-User-Id", uid);
        }
        let request = builder.body(axum::body::Body::from(body)).unwrap();
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn test_block_pipe_routes_enforce_endpoint_access() {
        use ok_fpga_core::resource::{
            AccessMode, EndpointDeclaration, EndpointEntry, EndpointType, Resource,
        };
        let state = test_enforced_state();
        let mut r = Resource::new(
            "SN1:r1".into(),
            "r1".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x80,
                    ep_type: EndpointType::BtPipeIn,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0xA0,
                    ep_type: EndpointType::BtPipeOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
            ],
        });
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        state
            .resource_registry
            .acquire("SN1:r1", &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        // Unregistered user -> 403 on both block-in and block-out.
        let (status, _) = call_app_raw(
            state.clone(),
            "POST",
            "/devices/SN1/classic/pipes/block-in?endpoint=128&block_size=16",
            Some("mallory"),
            vec![0u8; 16],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // ReadOnly holder -> 403 on block-in (write), 501 (passes enforcement) on block-out (GET read).
        let (status, _) = call_app_raw(
            state.clone(),
            "POST",
            "/devices/SN1/classic/pipes/block-in?endpoint=128&block_size=16",
            Some("carol"),
            vec![0u8; 16],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _) = call_app(
            state.clone(),
            "GET",
            "/devices/SN1/classic/pipes/block-out?endpoint=160&block_size=16&length=16",
            Some("carol"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        // Shared holder -> passes enforcement on block-in.
        let (status, _) = call_app_raw(
            state,
            "POST",
            "/devices/SN1/classic/pipes/block-in?endpoint=128&block_size=16",
            Some("alice"),
            vec![0u8; 16],
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_pipe_in_octet_stream_and_missing_endpoint_422() {
        let state = test_state();

        // Missing endpoint -> 422 ProblemDetail.
        let (status, json) = call_app_raw(
            state.clone(),
            "POST",
            "/devices/SN1/classic/pipes/in",
            None,
            vec![1, 2, 3, 4],
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["conflict"]["code"], "validation");

        // Valid endpoint + binary body passes validation -> 501 (no DLL loaded).
        let (status, _) = call_app_raw(
            state,
            "POST",
            "/devices/SN1/classic/pipes/in?endpoint=128",
            None,
            vec![1, 2, 3, 4],
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_pipe_out_get_octet_stream_and_negative_length_422() {
        let state = test_state();

        // POST to /pipes/out is now 405 Method Not Allowed (S-05).
        let (status, _) = call_app(
            state.clone(),
            "POST",
            "/devices/SN1/classic/pipes/out?endpoint=160&length=16",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

        // Missing endpoint or length -> 422.
        let (status, _) = call_app(
            state.clone(),
            "GET",
            "/devices/SN1/classic/pipes/out?length=16",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        let (status, _) = call_app(
            state.clone(),
            "GET",
            "/devices/SN1/classic/pipes/out?endpoint=160",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        // Negative length -> 422 (prevents huge usize wrap panic).
        let (status, json) = call_app(
            state.clone(),
            "GET",
            "/devices/SN1/classic/pipes/out?endpoint=160&length=-1",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["conflict"]["code"], "validation");

        // Valid GET query passes validation -> 501.
        let (status, _) = call_app(
            state,
            "GET",
            "/devices/SN1/classic/pipes/out?endpoint=160&length=16",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[tokio::test]
    async fn test_block_pipe_in_out_octet_stream_and_validation_422() {
        let state = test_state();

        // block_size <= 0 -> 422 on block-in and block-out.
        let (status, _) = call_app_raw(
            state.clone(),
            "POST",
            "/devices/SN1/classic/pipes/block-in?endpoint=128&block_size=0",
            None,
            vec![0u8; 16],
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        let (status, _) = call_app(
            state.clone(),
            "GET",
            "/devices/SN1/classic/pipes/block-out?endpoint=160&block_size=-4&length=16",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        let (status, _) = call_app(
            state,
            "GET",
            "/devices/SN1/classic/pipes/block-out?endpoint=160&block_size=16&length=-16",
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_register_bulk_read_and_write_enforcement_atomicity() {
        use ok_fpga_core::resource::{AccessMode, AddressRange, Resource};
        let state = test_enforced_state();
        let mut r = Resource::new(
            "SN1:reg".into(),
            "reg".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.address_ranges = vec![AddressRange {
            name: None,
            description: None,
            start: 0x0000,
            end: 0x000F,
            access: AccessMode::Shared,
            private: false,
        }];
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:reg", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Bulk read where all addresses are covered -> passes enforcement (501).
        let (status, _) = call_app(
            state.clone(),
            "POST",
            "/devices/SN1/classic/registers/read",
            Some("alice"),
            Some(serde_json::json!({ "addresses": [0x0000, 0x0004, 0x000F] })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        // Bulk read where the second address is out of range -> 403 BEFORE get_device!.
        let (status, _) = call_app(
            state.clone(),
            "POST",
            "/devices/SN1/classic/registers/read",
            Some("alice"),
            Some(serde_json::json!({ "addresses": [0x0000, 0x0020] })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // Bulk write where all entries are covered -> passes enforcement (501).
        let (status, _) = call_app(
            state.clone(),
            "PUT",
            "/devices/SN1/classic/registers/write",
            Some("alice"),
            Some(serde_json::json!({
                "entries": [
                    { "address": 0x0000, "data": 0x1111_2222u32 },
                    { "address": 0x0004, "data": 0x3333_4444u32 }
                ]
            })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);

        // Bulk write where the second entry is out of range -> 403 BEFORE get_device!.
        let (status, _) = call_app(
            state,
            "PUT",
            "/devices/SN1/classic/registers/write",
            Some("alice"),
            Some(serde_json::json!({
                "entries": [
                    { "address": 0x0000, "data": 0x1111_2222u32 },
                    { "address": 0x0020, "data": 0x3333_4444u32 }
                ]
            })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_wire_in_auto_update_default_true_and_explicit_false() {
        let omitted: WireInRequest =
            serde_json::from_value(serde_json::json!({ "endpoint": 0, "value": 1 })).unwrap();
        assert!(omitted.auto_update);
        assert_eq!(omitted.mask, 0xFFFF_FFFF);

        let explicit_false: WireInRequest = serde_json::from_value(
            serde_json::json!({ "endpoint": 0, "value": 1, "auto_update": false }),
        )
        .unwrap();
        assert!(!explicit_false.auto_update);
    }

    #[tokio::test]
    async fn test_axi_lite_read_write_enforce_address_access() {
        use ok_fpga_core::resource::{AccessMode, AddressRange, Resource};
        let state = test_enforced_state();
        let mut r = Resource::new(
            "SN1:axi".into(),
            "axi".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.interfaces.classic = false;
        r.interfaces.axi_lite = true;
        r.address_ranges = vec![AddressRange {
            name: None,
            description: None,
            start: 0x1000,
            end: 0x100F,
            access: AccessMode::Shared,
            private: false,
        }];
        assert!(state.resource_registry.register(r));
        state
            .resource_registry
            .acquire("SN1:axi", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        state
            .resource_registry
            .acquire("SN1:axi", &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        // Covered 4-byte read [0x1000..=0x1003] passes enforcement for alice and carol.
        for uid in ["alice", "carol"] {
            let (status, _) = call_app(
                state.clone(),
                "POST",
                "/devices/SN1/axi/lite/read",
                Some(uid),
                Some(serde_json::json!({ "address": 0x1000 })),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        }

        // 4-byte read at 0x100E [0x100E..=0x1011] crosses the end of 0x1000..=0x100F -> 403 (Q4).
        let (status, _) = call_app(
            state.clone(),
            "POST",
            "/devices/SN1/axi/lite/read",
            Some("alice"),
            Some(serde_json::json!({ "address": 0x100E })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        // Write is denied for ReadOnly holder carol -> 403, allowed for alice -> 501.
        let (status, _) = call_app(
            state.clone(),
            "PUT",
            "/devices/SN1/axi/lite/write",
            Some("carol"),
            Some(serde_json::json!({ "address": 0x1000, "value": 0xDEAD_BEEFu32 })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _) = call_app(
            state,
            "PUT",
            "/devices/SN1/axi/lite/write",
            Some("alice"),
            Some(serde_json::json!({ "address": 0x1000, "value": 0xDEAD_BEEFu32 })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }
}
