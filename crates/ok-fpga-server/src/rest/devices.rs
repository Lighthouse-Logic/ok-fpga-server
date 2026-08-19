//! # Device Routes
//!
//! REST endpoints for device discovery, info, sensors, settings, SYZYGY,
//! and configuration. Implements the `/devices/*` paths from ADR-004.
//!
//! ## FFI Integration (Phase 19 + 19G)
//!
//! The `configure_fpga`, `get_device_sensors`, `get_bitfile_info`,
//! `get_dataport_type`, `get_device_settings`, `get_device_setting`,
//! `set_device_setting`, and `get_syzygy_ports` handlers are wired to the
//! FrontPanel DLL via the `DeviceHandlePool` in `AppState`.
//! See ADR-017 and ADR-021.

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
///
/// Wraps the device array in a `{"devices": [...]}` object
/// per the OpenAPI spec (`GET /devices` → `DeviceListBody`).
#[derive(Serialize)]
struct DeviceListBody {
    devices: Vec<DeviceFullInfo>,
}

/// Full device info returned in list and detail responses.
///
/// Maps to the OpenAPI `DeviceInfo` schema — every field defined in
/// `api/openapi.yaml` components/schemas/DeviceInfo is included here.
/// Per `ai-collaboration.md` §6, the server must return what the spec says.
#[derive(Serialize)]
struct DeviceFullInfo {
    serial: String,
    device_id: String,
    product_name: String,
    product_id: i32,
    device_interface: String,
    usb_speed: String,
    device_major_version: i32,
    device_minor_version: i32,
    wire_width: i32,
    trigger_width: i32,
    pipe_width: i32,
    register_address_width: i32,
    register_data_width: i32,
    fpga_vendor: String,
    dataport: String,
    /// True if the FPGA has been programmed with a FrontPanel-enabled bitfile.
    is_front_panel_enabled: bool,
    /// Bitfile info, present only if the device is programmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    bitfile: Option<BitfileFullInfo>,
}

/// Bitfile info returned as part of DeviceFullInfo.
///
/// Maps to the OpenAPI `BitfileInfo` schema.
#[derive(Serialize)]
struct BitfileFullInfo {
    filename: String,
    description: String,
    programmed_at: String,
    programmed_by: String,
}

/// Build a `DeviceFullInfo` from a core `FpgaDevice`.
///
/// Converts the core domain types (enums) into the string representations
/// expected by the OpenAPI spec (e.g., `DeviceInterface::Usb3` → `"usb3"`).
fn device_to_full_info(d: &ok_fpga_core::device::FpgaDevice) -> DeviceFullInfo {
    let bitfile = d.state.bitfile().map(|b| BitfileFullInfo {
        filename: b.filename.clone(),
        description: b.description.clone().unwrap_or_default(),
        programmed_at: b.programmed_at.to_rfc3339(),
        programmed_by: b.programmed_by.clone(),
    });
    DeviceFullInfo {
        serial: d.info.serial.clone(),
        device_id: d.info.device_id.clone(),
        product_name: d.info.product_name.clone(),
        product_id: d.info.product_id,
        device_interface: format!("{:?}", d.info.device_interface).to_lowercase(),
        usb_speed: format!("{:?}", d.info.usb_speed).to_lowercase(),
        device_major_version: d.info.device_major_version,
        device_minor_version: d.info.device_minor_version,
        wire_width: d.info.wire_width,
        trigger_width: d.info.trigger_width,
        pipe_width: d.info.pipe_width,
        register_address_width: d.info.register_address_width,
        register_data_width: d.info.register_data_width,
        fpga_vendor: format!("{:?}", d.info.fpga_vendor).to_lowercase(),
        dataport: format!("{:?}", d.info.dataport).to_lowercase(),
        is_front_panel_enabled: d.is_programmed(),
        bitfile,
    }
}

/// Request body for JSON-path-based FPGA configuration.
#[derive(Deserialize)]
struct ConfigurePathRequest {
    /// Local filesystem path to the bitfile.
    bitfile_path: String,
}

/// Request body for `PUT /devices/{serial}/settings/{key}`.
///
/// The caller specifies the value type ("string" or "integer") and the
/// corresponding value field. Matches the `DeviceSettingValue` OpenAPI schema.
#[derive(Deserialize)]
struct SetDeviceSettingRequest {
    /// "string" or "integer" — determines which value field is used.
    value_type: String,
    /// String value (used when `value_type == "string"`).
    string_value: Option<String>,
    /// Integer value (used when `value_type == "integer"`).
    int_value: Option<u32>,
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
///
/// Returns the full `DeviceInfo` object for each device, matching the
/// OpenAPI `DeviceInfo` schema. Response is wrapped in `{"devices": [...]}`.
async fn list_devices(State(state): State<Arc<AppState>>) -> Json<DeviceListBody> {
    let devices = state.device_manager.registry().list();
    let full_infos = devices.iter().map(|d| device_to_full_info(d)).collect();
    Json(DeviceListBody { devices: full_infos })
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
///
/// Reads settings via the FrontPanel DLL (ADR-021): constructs an
/// `okDeviceSettings` handle, populates it from the device, enumerates
/// all key names, reads each value (int or string), then cleans up.
///
/// Settings are device-level metadata and do NOT require FPGA programming.
async fn get_device_settings(
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

    // SAFETY: handle is valid (from pool). We construct settings + names
    // containers, populate from the device, iterate keys, then destroy both.
    let settings_list: Vec<serde_json::Value> = unsafe {
        let settings = (dll.settings_construct)();
        if settings.is_null() {
            return Err(ProblemDetail::internal("Failed to construct settings handle"));
        }

        // Populate settings from the device.
        let rc = (dll.fp_get_device_settings)(handle, settings);
        if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.settings_destruct)(settings);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: rc as i32,
                message: format!("GetDeviceSettings failed with code {}", rc as i32),
            }));
        }

        // Enumerate key names.
        let names = (dll.setting_names_construct)();
        if names.is_null() {
            (dll.settings_destruct)(settings);
            return Err(ProblemDetail::internal("Failed to construct setting names handle"));
        }

        let rc = (dll.settings_list)(settings, names);
        if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.setting_names_destruct)(names);
            (dll.settings_destruct)(settings);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: rc as i32,
                message: format!("Settings_List failed with code {}", rc as i32),
            }));
        }

        let count = (dll.setting_names_get_count)(names);
        let mut result = Vec::with_capacity(count as usize);

        for i in 0..count {
            let name_ptr = (dll.setting_names_get)(names, i);
            if name_ptr.is_null() {
                continue;
            }
            let key = std::ffi::CStr::from_ptr(name_ptr)
                .to_string_lossy()
                .to_string();

            // Try reading as integer first, then as string.
            // The FrontPanel API doesn't expose a "type" query, so we probe.
            let key_c = std::ffi::CString::new(key.clone()).unwrap_or_default();
            let mut int_val: ok_fpga_ffi::UINT32 = 0;
            let int_rc = (dll.settings_get_int)(settings, key_c.as_ptr(), &mut int_val);

            if int_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                result.push(serde_json::json!({
                    "key": key,
                    "value_type": "integer",
                    "int_value": int_val,
                }));
            } else {
                // Try as string.
                let mut str_buf = vec![0i8; 256];
                let str_rc = (dll.settings_get_string)(
                    settings,
                    key_c.as_ptr(),
                    str_buf.len() as i32,
                    str_buf.as_mut_ptr(),
                );
                if str_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                    let val = cstr_from_char_array(&str_buf);
                    result.push(serde_json::json!({
                        "key": key,
                        "value_type": "string",
                        "string_value": val,
                    }));
                } else {
                    // Key exists but neither type read succeeded — report as unknown.
                    result.push(serde_json::json!({
                        "key": key,
                        "value_type": "unknown",
                    }));
                }
            }
        }

        (dll.setting_names_destruct)(names);
        (dll.settings_destruct)(settings);
        result
    };

    Ok(Json(serde_json::json!({ "settings": settings_list })))
}

/// `GET /devices/{serial}/settings/{key}` — Get a single setting.
///
/// Reads a single device setting by key name. Tries integer first, then
/// string. Returns 404 if the key is not found on the device.
async fn get_device_setting(
    State(state): State<Arc<AppState>>,
    Path((serial, key)): Path<(String, String)>,
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

    // SAFETY: handle is valid (from pool). Construct settings, populate,
    // read the single key, then destroy.
    let result: Result<serde_json::Value, ProblemDetail> = unsafe {
        let settings = (dll.settings_construct)();
        if settings.is_null() {
            return Err(ProblemDetail::internal("Failed to construct settings handle"));
        }

        let rc = (dll.fp_get_device_settings)(handle, settings);
        if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.settings_destruct)(settings);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: rc as i32,
                message: format!("GetDeviceSettings failed with code {}", rc as i32),
            }));
        }

        let key_c = std::ffi::CString::new(key.clone()).map_err(|_| {
            (dll.settings_destruct)(settings);
            ProblemDetail::bad_request(format!("invalid setting key: {}", key))
        })?;

        // Try integer first.
        let mut int_val: ok_fpga_ffi::UINT32 = 0;
        let int_rc = (dll.settings_get_int)(settings, key_c.as_ptr(), &mut int_val);

        let value = if int_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            serde_json::json!({
                "key": key,
                "value_type": "integer",
                "int_value": int_val,
            })
        } else {
            // Try string.
            let mut str_buf = vec![0i8; 256];
            let str_rc = (dll.settings_get_string)(
                settings,
                key_c.as_ptr(),
                str_buf.len() as i32,
                str_buf.as_mut_ptr(),
            );
            if str_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                let val = cstr_from_char_array(&str_buf);
                serde_json::json!({
                    "key": key,
                    "value_type": "string",
                    "string_value": val,
                })
            } else {
                (dll.settings_destruct)(settings);
                return Err(ProblemDetail::not_found(format!(
                    "setting '{}' not found on device '{}'",
                    key, serial
                )));
            }
        };

        (dll.settings_destruct)(settings);
        Ok(value)
    };

    Ok(Json(result?))
}

/// `PUT /devices/{serial}/settings/{key}` — Set a setting value.
///
/// Writes a single device setting by key name and persists via `settings_save`.
/// The request body specifies `value_type` ("string" or "integer") and the
/// corresponding value field.
async fn set_device_setting(
    State(state): State<Arc<AppState>>,
    Path((serial, key)): Path<(String, String)>,
    Json(body): Json<SetDeviceSettingRequest>,
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

    // SAFETY: handle is valid (from pool). Construct settings, populate from
    // device, write the new value, save back to device, then destroy.
    unsafe {
        let settings = (dll.settings_construct)();
        if settings.is_null() {
            return Err(ProblemDetail::internal("Failed to construct settings handle"));
        }

        let rc = (dll.fp_get_device_settings)(handle, settings);
        if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.settings_destruct)(settings);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: rc as i32,
                message: format!("GetDeviceSettings failed with code {}", rc as i32),
            }));
        }

        let key_c = std::ffi::CString::new(key.clone()).map_err(|_| {
            (dll.settings_destruct)(settings);
            ProblemDetail::bad_request(format!("invalid setting key: {}", key))
        })?;

        // Write the value based on the specified type.
        let write_rc = match body.value_type.as_str() {
            "integer" => {
                let val = body.int_value.ok_or_else(|| {
                    // Clean up before returning error.
                    (dll.settings_destruct)(settings);
                    ProblemDetail::bad_request("int_value is required when value_type is \"integer\"")
                })?;
                (dll.settings_set_int)(settings, key_c.as_ptr(), val)
            }
            "string" => {
                let val = body.string_value.as_deref().ok_or_else(|| {
                    (dll.settings_destruct)(settings);
                    ProblemDetail::bad_request(
                        "string_value is required when value_type is \"string\"",
                    )
                })?;
                let val_c = std::ffi::CString::new(val).map_err(|_| {
                    (dll.settings_destruct)(settings);
                    ProblemDetail::bad_request("string_value contains null bytes")
                })?;
                (dll.settings_set_string)(settings, key_c.as_ptr(), val_c.as_ptr())
            }
            other => {
                (dll.settings_destruct)(settings);
                return Err(ProblemDetail::bad_request(format!(
                    "unsupported value_type '{}' — use \"string\" or \"integer\"",
                    other
                )));
            }
        };

        if write_rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.settings_destruct)(settings);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: write_rc as i32,
                message: format!("Failed to set setting '{}' with code {}", key, write_rc as i32),
            }));
        }

        // Persist changes to device flash/EEPROM.
        let save_rc = (dll.settings_save)(settings);
        (dll.settings_destruct)(settings);

        if save_rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: save_rc as i32,
                message: format!("settings_save failed with code {}", save_rc as i32),
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "status": "ok",
        "key": key,
        "message": format!("Setting '{}' updated on device '{}'", key, serial),
    })))
}

/// `GET /devices/{serial}/syzygy` — Discover SYZYGY ports.
///
/// SYZYGY ports are discovered by probing device settings keys (ADR-009):
/// `SYZYGYn_PORT_STATUS` for port indices 0..15. For each port found,
/// the DNA fields (manufacturer, product, model, version, serial) are read.
///
/// This works without FPGA programming — SYZYGY data is stored in device
/// flash/EEPROM alongside other device settings.
async fn get_syzygy_ports(
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

    // SAFETY: handle is valid (from pool). Construct settings, populate,
    // probe SYZYGY keys, then destroy.
    let ports: Vec<serde_json::Value> = unsafe {
        let settings = (dll.settings_construct)();
        if settings.is_null() {
            return Err(ProblemDetail::internal("Failed to construct settings handle"));
        }

        let rc = (dll.fp_get_device_settings)(handle, settings);
        if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
            (dll.settings_destruct)(settings);
            return Err(ffi_error_to_problem(&FfiError::FrontPanelError {
                code: rc as i32,
                message: format!("GetDeviceSettings failed with code {}", rc as i32),
            }));
        }

        let mut result = Vec::new();

        // Probe ports 0..15. Stop when SYZYGYn_PORT_STATUS is not found.
        for n in 0u32..16 {
            let status_key = format!("SYZYGY{}_PORT_STATUS\0", n);
            let status_key_c = std::ffi::CString::from_vec_unchecked(status_key.into_bytes());
            let mut port_status: ok_fpga_ffi::UINT32 = 0;
            let rc = (dll.settings_get_int)(settings, status_key_c.as_ptr(), &mut port_status);

            if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                break; // No more SYZYGY ports.
            }

            let is_populated = port_status > 0;

            // Read string DNA fields for this port.
            let string_fields = [
                "MANUFACTURER_NAME",
                "PRODUCT_NAME",
                "PRODUCT_MODEL",
                "PRODUCT_VERSION",
                "SERIAL_NUMBER",
            ];

            let mut dna = std::collections::HashMap::new();
            for field in &string_fields {
                let key = format!("SYZYGY{}_{}\0", n, field);
                let key_c = std::ffi::CString::from_vec_unchecked(key.into_bytes());
                let mut buf = vec![0i8; 256];
                let str_rc = (dll.settings_get_string)(
                    settings,
                    key_c.as_ptr(),
                    buf.len() as i32,
                    buf.as_mut_ptr(),
                );
                if str_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                    dna.insert(field.to_string(), cstr_from_char_array(&buf));
                }
            }

            result.push(serde_json::json!({
                "index": n,
                "port_status_mv": port_status,
                "is_populated": is_populated,
                "manufacturer_name": dna.get("MANUFACTURER_NAME").cloned().unwrap_or_default(),
                "product_name": dna.get("PRODUCT_NAME").cloned().unwrap_or_default(),
                "product_model": dna.get("PRODUCT_MODEL").cloned().unwrap_or_default(),
                "product_version": dna.get("PRODUCT_VERSION").cloned().unwrap_or_default(),
                "serial_number": dna.get("SERIAL_NUMBER").cloned().unwrap_or_default(),
            }));
        }

        (dll.settings_destruct)(settings);
        result
    };

    Ok(Json(serde_json::json!({ "ports": ports })))
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

    // Clear any existing resources for this device — the old bitfile's
    // resources are invalid after re-programming (Phase 25-Fix-F, F3).
    let cleared = state.resource_registry.remove_for_device(&serial);
    if cleared > 0 {
        tracing::info!(serial = %serial, cleared, "Cleared resources for re-programmed device");

        // Broadcast event for dashboard auto-refresh (Phase 25-Fix-H, F9).
        state.broadcast_event(&format!(
            r#"{{"type":"resources_cleared","device_serial":"{}","cleared_count":{}}}"#,
            serial, cleared
        ));
    }

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
