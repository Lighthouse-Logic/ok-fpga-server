//! # Device Routes
//!
//! REST endpoints for device discovery, info, sensors, settings, SYZYGY,
//! and configuration. Implements the `/devices/*` paths from ADR-004.
//!
//! ## FFI Integration (Phase 19)
//!
//! The `configure_fpga`, `get_device_sensors`, `get_bitfile_info`, and
//! `get_dataport_type` handlers are wired to the FrontPanel DLL via the
//! `DeviceHandlePool` in `AppState`.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use ok_fpga_core::device::{BitfileRecord, DeviceState};
use ok_fpga_ffi::error::FfiError;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/devices/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/devices", get(list_devices))
        .route("/devices/{serial}", get(get_device_info))
        .route("/devices/{serial}/sensors", get(get_device_sensors))
        .route("/devices/{serial}/settings", get(get_device_settings))
        .route(
            "/devices/{serial}/settings/{key}",
            get(get_device_setting).put(set_device_setting),
        )
        .route("/devices/{serial}/syzygy", get(get_syzygy_ports))
        .route("/devices/{serial}/configure", post(configure_fpga))
        .route("/devices/{serial}/bitfile", get(get_bitfile_info))
        .route("/devices/{serial}/dataport", get(get_dataport_type))
}

// ============================================================================
// Response Types
// ============================================================================

/// Device list response body.
#[derive(Serialize)]
struct DeviceListBody {
    devices: Vec<DeviceSummary>,
}

/// Summary of a connected device (used in list responses).
#[derive(Serialize)]
struct DeviceSummary {
    serial: String,
    product_name: String,
    is_programmed: bool,
}

/// Request body for JSON-path-based FPGA configuration.
#[derive(Deserialize)]
struct ConfigurePathRequest {
    /// Local filesystem path to the bitfile.
    bitfile_path: String,
}

/// Successful configuration response.
#[derive(Serialize)]
struct ConfigureResponse {
    status: String,
    serial: String,
    message: String,
}

/// Sensor reading response.
#[derive(Serialize)]
struct SensorReading {
    id: i32,
    name: String,
    value: String,
    minimum: String,
    maximum: String,
    description: String,
}

// ============================================================================
// Helpers
// ============================================================================

/// Map an FFI error to an appropriate ProblemDetail HTTP error.
fn ffi_error_to_problem(e: &FfiError) -> ProblemDetail {
    match e {
        FfiError::FrontPanelError { code, message } => {
            let code_i32 = *code;
            // Map specific FrontPanel error codes to HTTP statuses.
            if code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_InvalidParameter as i32
                || code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_InvalidEndpoint as i32
            {
                ProblemDetail::bad_request(message.clone())
            } else if code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_DeviceNotOpen as i32 {
                ProblemDetail::new(
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    message.clone(),
                )
            } else if code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_Timeout as i32 {
                ProblemDetail::new(
                    axum::http::StatusCode::GATEWAY_TIMEOUT,
                    message.clone(),
                )
            } else if code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_UnsupportedFeature as i32
                || code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_FrontPanelNotEnabled as i32
            {
                ProblemDetail::not_implemented(message.clone())
            } else if code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_InvalidBitstream as i32
                || code_i32 == ok_fpga_ffi::ok_ErrorCode::ok_FileError as i32
            {
                ProblemDetail::bad_request(message.clone())
            } else {
                ProblemDetail::internal(message.clone())
            }
        }
        _ => ProblemDetail::internal(e.to_string()),
    }
}

// ============================================================================
// Handlers
// ============================================================================

/// `GET /devices` — List all connected FPGA devices.
async fn list_devices(State(state): State<Arc<AppState>>) -> Json<DeviceListBody> {
    let devices = state.device_manager.registry().list();
    let summaries = devices
        .iter()
        .map(|d| DeviceSummary {
            serial: d.serial().to_string(),
            product_name: d.product_name().to_string(),
            is_programmed: d.is_programmed(),
        })
        .collect();

    Json(DeviceListBody { devices: summaries })
}

/// `GET /devices/{serial}` — Get detailed device info.
async fn get_device_info(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let dev = state
        .device_manager
        .registry()
        .get(&serial)
        .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found", serial)))?;

    Ok(Json(serde_json::to_value(&dev.info).unwrap_or_default()))
}

/// `GET /devices/{serial}/sensors` — Get device sensor readings.
///
/// Reads sensors via the FrontPanel DLL: constructs an `okDeviceSensors`
/// handle, populates it via `GetDeviceSensors`, then iterates all sensors.
async fn get_device_sensors(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let pool = state
        .handle_pool
        .as_ref()
        .ok_or_else(|| ProblemDetail::not_implemented("FrontPanel DLL not loaded"))?;

    let device_arc = pool
        .get(&serial)
        .await
        .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found in handle pool", serial)))?;

    let device = device_arc.lock().await;
    let dll = device.dll();
    let handle = device.raw();

    // SAFETY: handle is valid (from pool). We construct a sensors snapshot,
    // populate it from the device, iterate, then destroy it.
    let sensors: Vec<SensorReading> = unsafe {
        let sensors_handle = (dll.sensors_construct)();
        if sensors_handle.is_null() {
            return Err(ProblemDetail::internal("Failed to construct sensor snapshot"));
        }

        // Populate the sensors from the device.
        let rc = (dll.fp_get_device_sensors)(handle, sensors_handle);
        if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.sensors_destruct)(sensors_handle);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: rc as i32,
                message: format!("GetDeviceSensors failed with code {}", rc as i32),
            }));
        }

        let count = (dll.sensors_get_count)(sensors_handle);
        let mut readings = Vec::with_capacity(count as usize);

        for i in 0..count {
            // sensors_get_sensor returns okTDeviceSensor by value.
            let sensor = (dll.sensors_get_sensor)(sensors_handle, i);
            let name = cstr_from_char_array(&sensor.name);
            let description = cstr_from_char_array(&sensor.description);
            readings.push(SensorReading {
                id: sensor.id,
                name,
                value: format!("{:.2}", sensor.value),
                minimum: format!("{:.2}", sensor.min),
                maximum: format!("{:.2}", sensor.max),
                description,
            });
        }

        (dll.sensors_destruct)(sensors_handle);
        readings
    };

    Ok(Json(serde_json::to_value(&sensors).unwrap_or_default()))
}

/// `GET /devices/{serial}/settings` — List all device settings.
async fn get_device_settings(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "Settings require FFI integration",
    ))
}

/// `GET /devices/{serial}/settings/{key}` — Get a single setting.
async fn get_device_setting(
    Path((_serial, _key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "Settings require FFI integration",
    ))
}

/// `PUT /devices/{serial}/settings/{key}` — Set a setting value.
async fn set_device_setting(
    Path((_serial, _key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "Settings require FFI integration",
    ))
}

/// `GET /devices/{serial}/syzygy` — Get SYZYGY port information.
async fn get_syzygy_ports(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented(
        "SYZYGY discovery requires FFI integration",
    ))
}

/// `POST /devices/{serial}/configure` — Program FPGA with a bitfile.
///
/// Supports two content types (ADR-017):
/// - `application/json` with `{"path": "/path/to/file.bit"}` — local path
/// - `multipart/form-data` with bitfile binary — remote upload
///
/// On success, updates the device state to `Programmed` and acquires
/// data port handles for subsequent endpoint operations.
async fn configure_fpga(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<ConfigureResponse>, ProblemDetail> {
    // Verify device exists in registry.
    if !state.device_manager.registry().contains(&serial) {
        return Err(ProblemDetail::not_found(format!("device '{}' not found", serial)));
    }

    let pool = state
        .handle_pool
        .as_ref()
        .ok_or_else(|| ProblemDetail::not_implemented("FrontPanel DLL not loaded"))?;

    // Determine configuration mode by Content-Type.
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json");

    if content_type.contains("multipart") || content_type.contains("octet-stream") {
        // Binary upload mode — configure from memory.
        pool.configure_from_memory(&serial, &body)
            .await
            .map_err(|e| ffi_error_to_problem(&e))?;
    } else {
        // JSON path mode — configure from local path.
        let req: ConfigurePathRequest = serde_json::from_slice(&body)
            .map_err(|e| ProblemDetail::bad_request(format!("invalid JSON: {}", e)))?;

        let path = std::path::Path::new(&req.bitfile_path);
        if !path.exists() {
            return Err(ProblemDetail::bad_request(format!(
                "bitfile not found: {}",
                req.bitfile_path
            )));
        }

        pool.configure_from_path(&serial, path)
            .await
            .map_err(|e| ffi_error_to_problem(&e))?;
    }

    // Update device state in the registry.
    state.device_manager.registry().update_state(
        &serial,
        DeviceState::Programmed {
            bitfile: BitfileRecord {
                filename: "uploaded".to_string(),
                description: None,
                programmed_at: chrono::Utc::now(),
                programmed_by: "api".to_string(),
            },
        },
    );

    Ok(Json(ConfigureResponse {
        status: "ok".to_string(),
        serial: serial.clone(),
        message: format!("FPGA '{}' configured successfully", serial),
    }))
}

/// `GET /devices/{serial}/bitfile` — Get current bitfile info.
async fn get_bitfile_info(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let dev = state
        .device_manager
        .registry()
        .get(&serial)
        .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found", serial)))?;

    match dev.state.bitfile() {
        Some(bf) => Ok(Json(serde_json::to_value(bf).unwrap_or_default())),
        None => Err(ProblemDetail::new(
            axum::http::StatusCode::CONFLICT,
            "FPGA is not programmed — no bitfile loaded",
        )),
    }
}

/// `GET /devices/{serial}/dataport` — Get device dataport type.
///
/// Returns the `okTDeviceInfo.dataport` value, which describes the physical
/// host interface (Legacy/AXI), NOT which HDL endpoint families are available.
async fn get_dataport_type(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    let dev = state
        .device_manager
        .registry()
        .get(&serial)
        .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found", serial)))?;

    Ok(Json(serde_json::json!({
        "serial": serial,
        "dataport": dev.info.dataport.to_string(),
        "dataport_raw": dev.info.dataport as i32,
    })))
}

/// Extract a Rust `String` from a C `char` array (null-terminated).
fn cstr_from_char_array(buf: &[::std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}
