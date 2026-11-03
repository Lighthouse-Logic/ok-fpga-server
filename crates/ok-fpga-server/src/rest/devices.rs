//! # Device Routes
//!
//! REST endpoints for device discovery, info, sensors, settings, SYZYGY,
//! and configuration. Implements the `/devices/*` paths from ADR-004.
//!
//! ## FPGA Configuration (ADR-028)
//!
//! Two separate configure endpoints exist:
//! - `POST /configure` — client uploads bitfile via `multipart/form-data`
//! - `POST /configure_from_server_path` — server reads bitfile from its own disk
//!
//! Both return `BitfileRecord` (the `BitfileInfo` OpenAPI schema) on success.
//!
//! ## FFI Integration (Phase 19 + 19G)
//!
//! The configure handlers, `get_device_sensors`, `get_bitfile_info`,
//! `get_dataport_type`, `get_device_settings`, `get_device_setting`,
//! `set_device_setting`, and `get_syzygy_ports` handlers are wired to the
//! FrontPanel DLL via the `DeviceHandlePool` in `AppState`.
//! See ADR-017 and ADR-021.

use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use ok_fpga_core::device::{BitfileRecord, DeviceState};
use ok_fpga_core::manager::{DeleteReason, DeviceEvent};
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
        .route(
            "/devices/{serial}/configure",
            // Multipart uploads can be large (bitfiles are typically ~11MB, max ~50MB).
            // Override axum's default 2MB body limit for this route only.
            post(configure_fpga)
                .delete(clear_fpga_configuration)
                .layer(DefaultBodyLimit::max(100 * 1024 * 1024)),
        )
        .route("/devices/{serial}/configure_from_server_path", post(configure_fpga_from_server_path))
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
/// Maps to the OpenAPI `BitfileInfo` schema (Phase 37).
#[derive(Serialize)]
struct BitfileFullInfo {
    filename: String,
    description: String,
    programmed_at: String,
    programmed_by: String,
    /// Full path on the server filesystem (null for client uploads).
    file_path: Option<String>,
    /// MD5 hash of the bitfile content, hex-encoded (null if unavailable).
    md5_hash: Option<String>,
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
        file_path: b.file_path.clone(),
        md5_hash: b.md5_hash.clone(),
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

/// Request body for `POST /devices/{serial}/configure_from_server_path`.
///
/// The client sends a JSON object with the path to a bitfile on the server's
/// filesystem. The server reads and programs the file directly. Fields match
/// the OpenAPI schema for this endpoint (ADR-028).
#[derive(Deserialize)]
struct ConfigurePathRequest {
    /// Server filesystem path to the bitfile (e.g., "/opt/bitfiles/Counters.bit").
    bitfile_path: String,
    /// Optional human-readable description for dashboard display.
    description: Option<String>,
    /// User performing the programming (should match `X-User-Id` header).
    /// If absent, the handler falls back to the `X-User-Id` header value.
    user_id: Option<String>,
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

/// Sensor reading response — matches OpenAPI `DeviceSensor` schema (Phase 37).
#[derive(Serialize)]
struct SensorReading {
    id: i32,
    sensor_type: String,
    name: String,
    description: String,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
}

/// Device setting response — matches OpenAPI `DeviceSetting` schema (Phase 37).
#[derive(Serialize)]
struct DeviceSettingView {
    key: String,
    value_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    string_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    int_value: Option<u32>,
}

/// Settings list response body.
#[derive(Serialize)]
struct SettingsListBody {
    settings: Vec<DeviceSettingView>,
}

/// Sensors list response body.
#[derive(Serialize)]
struct SensorsListBody {
    sensors: Vec<SensorReading>,
}

/// SYZYGY port response — matches OpenAPI `SyzygyPort` schema (Phase 37).
#[derive(Serialize)]
struct SyzygyPortView {
    index: u32,
    port_status_mv: u32,
    is_populated: bool,
    manufacturer_name: String,
    product_name: String,
    product_model: String,
    product_version: String,
    serial_number: String,
}

/// SYZYGY ports list response body.
#[derive(Serialize)]
struct SyzygyPortsBody {
    ports: Vec<SyzygyPortView>,
}

/// Dataport response — matches OpenAPI schema (Phase 37).
#[derive(Serialize)]
struct DataportResponse {
    serial: String,
    dataport: String,
    dataport_raw: i32,
}

/// StatusResponse for device operations (Phase 37).
#[derive(Serialize)]
struct DeviceStatusResponse {
    success: bool,
    message: String,
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
///
/// Returns a `DeviceFullInfo` matching the OpenAPI `DeviceInfo` schema (Phase 37).
async fn get_device_info(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
) -> Result<Json<DeviceFullInfo>, ProblemDetail> {
    let dev = state
        .device_manager
        .registry()
        .get(&serial)
        .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found", serial)))?;

    Ok(Json(device_to_full_info(&dev)))
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
            // Map the C sensor type enum to the OpenAPI string.
            let sensor_type_str = match sensor.type_ as u32 {
                1 => "bool",
                2 => "integer",
                3 => "float",
                4 => "voltage",
                5 => "current",
                6 => "temperature",
                7 => "fan_rpm",
                8 => "power",
                _ => "invalid",
            };
            readings.push(SensorReading {
                id: sensor.id,
                sensor_type: sensor_type_str.to_string(),
                name,
                description,
                min: sensor.min as f64,
                max: sensor.max as f64,
                step: sensor.step as f64,
                value: sensor.value as f64,
            });
        }

        (dll.sensors_destruct)(sensors_handle);
        readings
    };

    Ok(Json(serde_json::to_value(SensorsListBody { sensors }).unwrap()))
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
    let settings_list: Vec<DeviceSettingView> = unsafe {
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
                result.push(DeviceSettingView {
                    key,
                    value_type: "integer".to_string(),
                    string_value: None,
                    int_value: Some(int_val),
                });
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
                    result.push(DeviceSettingView {
                        key,
                        value_type: "string".to_string(),
                        string_value: Some(val),
                        int_value: None,
                    });
                } else {
                    // Key exists but neither type read succeeded — report as unknown.
                    result.push(DeviceSettingView {
                        key,
                        value_type: "unknown".to_string(),
                        string_value: None,
                        int_value: None,
                    });
                }
            }
        }

        (dll.setting_names_destruct)(names);
        (dll.settings_destruct)(settings);
        result
    };

    Ok(Json(serde_json::to_value(SettingsListBody { settings: settings_list }).unwrap()))
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
            serde_json::to_value(DeviceSettingView {
                key: key.clone(),
                value_type: "integer".to_string(),
                string_value: None,
                int_value: Some(int_val),
            }).unwrap()
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
                serde_json::to_value(DeviceSettingView {
                    key: key.clone(),
                    value_type: "string".to_string(),
                    string_value: Some(val),
                    int_value: None,
                }).unwrap()
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

    Ok(Json(serde_json::to_value(DeviceStatusResponse {
        success: true,
        message: format!("Setting '{}' updated on device '{}'", key, serial),
    }).unwrap()))
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
    let ports: Vec<SyzygyPortView> = unsafe {
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

            result.push(SyzygyPortView {
                index: n,
                port_status_mv: port_status,
                is_populated,
                manufacturer_name: dna.get("MANUFACTURER_NAME").cloned().unwrap_or_default(),
                product_name: dna.get("PRODUCT_NAME").cloned().unwrap_or_default(),
                product_model: dna.get("PRODUCT_MODEL").cloned().unwrap_or_default(),
                product_version: dna.get("PRODUCT_VERSION").cloned().unwrap_or_default(),
                serial_number: dna.get("SERIAL_NUMBER").cloned().unwrap_or_default(),
            });
        }

        (dll.settings_destruct)(settings);
        result
    };

    Ok(Json(serde_json::to_value(SyzygyPortsBody { ports }).unwrap()))
}

// `resolve_user_id` is now a shared function in `crate::rest::mod.rs` (ADR-029).

/// Shared post-configure logic: update device state and broadcast `DeviceProgrammed`.
///
/// Called by both `configure_fpga` (multipart upload) and
/// `configure_fpga_from_server_path` (JSON path) after successful FPGA
/// programming. Note that forced cleanup of any previous resources on the
/// device (`D26`) already ran **before** the FFI call (`44-F q3a`).
///
/// Returns the `BitfileRecord` unchanged (for use as the HTTP response body).
fn finalize_configure(
    state: &AppState,
    serial: &str,
    bitfile: BitfileRecord,
) -> BitfileRecord {
    // 1. Update device state in the registry (bumps `config_generation`).
    state.device_manager.registry().update_state(
        serial,
        DeviceState::Programmed {
            bitfile: bitfile.clone(),
        },
    );

    // 2. Broadcast `DeviceEvent::DeviceProgrammed` for SSE / gRPC subscribers.
    let _ = state.device_manager.broadcast(DeviceEvent::DeviceProgrammed {
        serial: serial.to_string(),
        bitfile: bitfile.filename.clone(),
        programmed_by: bitfile.programmed_by.clone(),
        timestamp: chrono::Utc::now(),
    });

    // 3. Broadcast device_programmed event for dashboard auto-refresh (Phase 28D).
    state.broadcast_event(&format!(
        r#"{{"type":"device_programmed","device_serial":"{}"}}"#,
        serial
    ));

    bitfile
}

/// `POST /devices/{serial}/configure` — Program FPGA with an uploaded bitfile.
///
/// Accepts `multipart/form-data` with the following fields:
/// - `bitfile` (required) — the `.bit` or `.rbf` binary data
/// - `filename` (optional) — original filename for the `BitfileInfo` record
/// - `description` (optional) — human-readable description for dashboard display
/// - `user_id` (optional) — user performing the programming (falls back to `X-User-Id` header)
///
/// Returns `BitfileInfo` (serialized `BitfileRecord`) on success.
///
/// ## Forced resource cleanup (`D26`, `44-F q3a`, `q3b`)
///
/// Reprogramming invalidates the running FPGA design. After validating the
/// request inputs and before calling into FFI, all resources on the device
/// are forcibly unlocked, released, and deleted (`reason = "reprogram"`), and
/// the device transitions to `Connected` (unprogrammed) so a mid-transfer FFI
/// failure leaves the device unprogrammed with no stale resources (`q3a`).
/// When `handle_pool` is `None` (Tier 3 tests without `libokFrontPanel`), the
/// state transition and forced cleanup still run in memory (`q3b`).
async fn configure_fpga(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<BitfileRecord>, ProblemDetail> {
    // Verify device exists in registry.
    if !state.device_manager.registry().contains(&serial) {
        return Err(ProblemDetail::not_found(format!("device '{}' not found", serial)));
    }

    // Parse the multipart form fields. We iterate all fields and collect
    // the binary data and text metadata separately.
    let mut bitfile_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut description: Option<String> = None;
    let mut user_id: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ProblemDetail::bad_request(format!("multipart parse error: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "bitfile" => {
                // Extract filename from the multipart Content-Disposition if the
                // client set it (e.g., `filename="Counters.bit"` in the part header).
                if filename.is_none() {
                    if let Some(fn_str) = field.file_name() {
                        let fn_owned = fn_str.to_string();
                        if !fn_owned.is_empty() {
                            filename = Some(fn_owned);
                        }
                    }
                }
                bitfile_data = Some(
                    field.bytes().await
                        .map_err(|e| ProblemDetail::bad_request(format!("failed to read bitfile field: {}", e)))?
                        .to_vec()
                );
            }
            "filename" => {
                // Explicit filename field overrides the Content-Disposition filename.
                let text = field.text().await
                    .map_err(|e| ProblemDetail::bad_request(format!("failed to read filename field: {}", e)))?;
                if !text.is_empty() {
                    filename = Some(text);
                }
            }
            "description" => {
                let text = field.text().await
                    .map_err(|e| ProblemDetail::bad_request(format!("failed to read description field: {}", e)))?;
                if !text.is_empty() {
                    description = Some(text);
                }
            }
            "user_id" => {
                let text = field.text().await
                    .map_err(|e| ProblemDetail::bad_request(format!("failed to read user_id field: {}", e)))?;
                if !text.is_empty() {
                    user_id = Some(text);
                }
            }
            other => {
                // Ignore unknown fields but log them for debugging.
                tracing::debug!(field = %other, "Ignoring unknown multipart field");
            }
        }
    }

    // The bitfile field is required — reject the request if it was not provided.
    let data = bitfile_data.ok_or_else(|| {
        ProblemDetail::bad_request("missing required 'bitfile' field in multipart form data")
    })?;

    // Resolve user identity before touching hardware. When enforcement is "off"
    // (debugging / legacy unit tests, 44-E Q3b), anonymous programming is
    // permitted if neither `user_id` nor `X-User-Id` was supplied.
    let programmed_by = match super::resolve_user_id(user_id.as_deref(), &headers) {
        Ok(uid) => {
            state.user_sessions.touch(&uid);
            uid
        }
        Err(err)
            if state.enforcement() == crate::state::EndpointEnforcement::Off
                && user_id.as_deref().is_none_or(str::is_empty) =>
        {
            let _ = err;
            "Unknown".to_string()
        }
        Err(err) => return Err(err),
    };

    // D26 / 44-F q3a: Mark the device `Connected` (unprogrammed, bumping
    // `config_generation`) and forcibly unlock, release, and delete all
    // resources on the device BEFORE the FFI call so that if the bitfile
    // transfer fails mid-way, the device remains unprogrammed (`Connected`)
    // and no stale resources survive.
    state
        .device_manager
        .registry()
        .update_state(&serial, DeviceState::Connected);
    let cleared = state.force_cleanup_device_resources(&serial, DeleteReason::Reprogram);
    if cleared > 0 {
        tracing::info!(
            serial = %serial,
            cleared,
            "Force-deleted resources before re-programming device"
        );
    }

    // Program the FPGA from the received bytes when a hardware handle pool is
    // attached (44-F q3b: in Tier 3 tests without `handle_pool`, state and
    // resource cleanup still complete in memory).
    if let Some(pool) = state.handle_pool.as_ref() {
        pool.configure_from_memory(&serial, &data)
            .await
            .map_err(|e| ffi_error_to_problem(&e))?;
    }

    // Build the BitfileRecord for the response and device state.
    let record = BitfileRecord {
        filename: filename.unwrap_or_else(|| "uploaded".to_string()),
        description,
        programmed_at: chrono::Utc::now(),
        programmed_by,
        file_path: None, // Client uploads have no server-side path.
        md5_hash: Some(format!("{:x}", md5::compute(&data))),
    };

    // Update state to Programmed and broadcast events.
    let record = finalize_configure(&state, &serial, record);

    Ok(Json(record))
}

/// `DELETE /devices/{serial}/configure` — Clear FPGA configuration (`D26`).
///
/// Clears the loaded configuration on the FPGA, transitions the device state
/// back to `Connected` (unprogrammed), and forcibly unlocks, releases, and
/// deletes all resources registered on the device (`reason = "clear"`).
async fn clear_fpga_configuration(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: HeaderMap,
) -> Result<Json<DeviceStatusResponse>, ProblemDetail> {
    // Verify device exists in registry.
    if !state.device_manager.registry().contains(&serial) {
        return Err(ProblemDetail::not_found(format!("device '{}' not found", serial)));
    }

    // Resolve user identity from `X-User-Id` when enforcement is active.
    match super::resolve_user_id(None, &headers) {
        Ok(uid) => {
            state.user_sessions.touch(&uid);
        }
        Err(err) if state.enforcement() == crate::state::EndpointEnforcement::Off => {
            let _ = err;
        }
        Err(err) => return Err(err),
    }

    // D26 / 44-F q3a & q3b: Transition the device to `Connected`
    // (unprogrammed, bumping `config_generation`) and forcibly unlock, release,
    // and delete all resources on the device (`reason = "clear"`).
    state
        .device_manager
        .registry()
        .update_state(&serial, DeviceState::Connected);
    let cleared = state.force_cleanup_device_resources(&serial, DeleteReason::Clear);
    if cleared > 0 {
        tracing::info!(
            serial = %serial,
            cleared,
            "Force-deleted resources on clear FPGA configuration"
        );
    }

    if let Some(pool) = state.handle_pool.as_ref() {
        pool.clear_configuration(&serial)
            .await
            .map_err(|e| ffi_error_to_problem(&e))?;
    }

    state.broadcast_event(&format!(
        r#"{{"type":"device_cleared","device_serial":"{}"}}"#,
        serial
    ));

    Ok(Json(DeviceStatusResponse {
        success: true,
        message: format!("FPGA configuration cleared on device '{}'", serial),
    }))
}

/// `POST /devices/{serial}/configure_from_server_path` — Program FPGA from a server-side file.
///
/// Accepts `application/json` with:
/// ```json
/// {"bitfile_path": "/path/on/server/Counters.bit", "description": "optional", "user_id": "optional"}
/// ```
///
/// The bitfile must exist on the server's filesystem. Returns `BitfileInfo`
/// (serialized `BitfileRecord`) on success.
///
/// ## gRPC equivalent
/// Maps to `ConfigureFPGA(ConfigureFPGARequest)` in the proto definition.
async fn configure_fpga_from_server_path(
    State(state): State<Arc<AppState>>,
    Path(serial): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ConfigurePathRequest>,
) -> Result<Json<BitfileRecord>, ProblemDetail> {
    // Verify device exists in registry.
    if !state.device_manager.registry().contains(&serial) {
        return Err(ProblemDetail::not_found(format!("device '{}' not found", serial)));
    }

    // Verify the bitfile exists on the server's filesystem.
    let path = std::path::Path::new(&req.bitfile_path);
    if !path.exists() {
        return Err(ProblemDetail::bad_request(format!(
            "bitfile not found: {}",
            req.bitfile_path
        )));
    }

    // Resolve user identity before touching hardware. When enforcement is "off"
    // (debugging / legacy unit tests, 44-E Q3b), anonymous programming is
    // permitted if neither `user_id` nor `X-User-Id` was supplied.
    let programmed_by = match super::resolve_user_id(req.user_id.as_deref(), &headers) {
        Ok(uid) => {
            state.user_sessions.touch(&uid);
            uid
        }
        Err(err)
            if state.enforcement() == crate::state::EndpointEnforcement::Off
                && req.user_id.as_deref().is_none_or(str::is_empty) =>
        {
            let _ = err;
            "Unknown".to_string()
        }
        Err(err) => return Err(err),
    };

    // D26 / 44-F q3a: Mark the device `Connected` (unprogrammed, bumping
    // `config_generation`) and forcibly unlock, release, and delete all
    // resources on the device BEFORE the FFI call so that if the bitfile
    // transfer fails mid-way, the device remains unprogrammed (`Connected`)
    // and no stale resources survive.
    state
        .device_manager
        .registry()
        .update_state(&serial, DeviceState::Connected);
    let cleared = state.force_cleanup_device_resources(&serial, DeleteReason::Reprogram);
    if cleared > 0 {
        tracing::info!(
            serial = %serial,
            cleared,
            "Force-deleted resources before re-programming device"
        );
    }

    // Program the FPGA from the file on disk when a hardware handle pool is
    // attached (44-F q3b: in Tier 3 tests without `handle_pool`, state and
    // resource cleanup still complete in memory).
    if let Some(pool) = state.handle_pool.as_ref() {
        pool.configure_from_path(&serial, path)
            .await
            .map_err(|e| ffi_error_to_problem(&e))?;
    }

    // Extract filename as the basename of the path.
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| req.bitfile_path.clone());

    // Compute MD5 hash of the file on disk.
    let md5_hash = match std::fs::read(path) {
        Ok(data) => Some(format!("{:x}", md5::compute(&data))),
        Err(_) => None,
    };

    let record = BitfileRecord {
        filename,
        description: req.description,
        programmed_at: chrono::Utc::now(),
        programmed_by,
        file_path: Some(req.bitfile_path.clone()),
        md5_hash,
    };

    // Update state to Programmed and broadcast events.
    let record = finalize_configure(&state, &serial, record);

    Ok(Json(record))
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
) -> Result<Json<DataportResponse>, ProblemDetail> {
    let dev = state
        .device_manager
        .registry()
        .get(&serial)
        .ok_or_else(|| ProblemDetail::not_found(format!("device '{}' not found", serial)))?;

    Ok(Json(DataportResponse {
        serial,
        dataport: dev.info.dataport.to_string(),
        dataport_raw: dev.info.dataport as i32,
    }))
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

// ============================================================================
// Tests (Tier 3: forced resource cleanup on configure & clear, D26 / 44-F)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use ok_fpga_core::device::{
        DataportType, DeviceInfo, DeviceInterface, FpgaDevice, FpgaVendor, UsbSpeed,
    };
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::{AccessMode, Resource, ResourceRegistry};
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tower::ServiceExt;

    const SERIAL: &str = "DEV44F01";

    fn test_state() -> Arc<AppState> {
        let device_manager = Arc::new(DeviceManager::new());
        let info = DeviceInfo {
            serial: SERIAL.to_string(),
            product_name: "XEM7320-A75T".to_string(),
            product_id: 43,
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
            device_id: "XEM7320".to_string(),
        };
        device_manager.registry().add(FpgaDevice {
            info,
            state: DeviceState::Programmed {
                bitfile: BitfileRecord {
                    filename: "initial.bit".to_string(),
                    description: None,
                    programmed_at: chrono::Utc::now(),
                    programmed_by: "init".to_string(),
                    file_path: None,
                    md5_hash: None,
                },
            },
            config_generation: 1,
        });
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let mut app_state = AppState::new(
            device_manager,
            Arc::new(ResourceRegistry::new()),
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            Arc::new(ok_fpga_core::auth::SessionManager::new(3600)),
            Arc::new(UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Enforce,
            crate::config::ServerConfig::default(),
            None,
        );
        app_state.event_tx = Some(event_tx);
        Arc::new(app_state)
    }

    #[tokio::test]
    async fn test_configure_fpga_held_resources_force_deleted() {
        let state = test_state();

        // Register a shared resource (locked by alice, also held read_only by bob)
        // and an exclusive resource (held by charlie).
        let mut r_shared = Resource::new(
            format!("{SERIAL}:r_shared"),
            "r_shared".to_string(),
            None,
            SERIAL.to_string(),
            AccessMode::Shared,
        );
        r_shared.private = false;
        state.resource_registry.register(r_shared);
        state
            .resource_registry
            .acquire(&format!("{SERIAL}:r_shared"), &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        state
            .resource_registry
            .acquire(&format!("{SERIAL}:r_shared"), &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap();
        state
            .resource_registry
            .lock(&format!("{SERIAL}:r_shared"), &"alice".to_string())
            .unwrap();

        let r_excl = Resource::new(
            format!("{SERIAL}:r_excl"),
            "r_excl".to_string(),
            None,
            SERIAL.to_string(),
            AccessMode::Exclusive,
        );
        state.resource_registry.register(r_excl);
        state
            .resource_registry
            .acquire(&format!("{SERIAL}:r_excl"), &"charlie".to_string(), AccessMode::Exclusive)
            .unwrap();

        // Write a temporary dummy bitfile on disk.
        let tmp_path = std::env::temp_dir().join("test_44f_reprogram.bit");
        std::fs::write(&tmp_path, b"dummy-bitstream-bytes").unwrap();

        let mut events = state.device_manager.subscribe();
        let req_body = serde_json::json!({
            "bitfile_path": tmp_path.to_string_lossy(),
            "description": "reprogrammed in test",
            "user_id": "alice"
        });

        let req = Request::builder()
            .method("POST")
            .uri(format!("/devices/{SERIAL}/configure_from_server_path"))
            .header("content-type", "application/json")
            .header("x-user-id", "alice")
            .body(Body::from(req_body.to_string()))
            .unwrap();

        let resp = routes().with_state(state.clone()).oneshot(req).await.unwrap();
        let _ = std::fs::remove_file(&tmp_path);
        assert_eq!(resp.status(), StatusCode::OK);

        // Both resources must be gone from the registry, and all user holdings empty.
        assert_eq!(state.resource_registry.count(), 0);
        assert!(state.resource_registry.holdings_for("alice").is_empty());
        assert!(state.resource_registry.holdings_for("bob").is_empty());
        assert!(state.resource_registry.holdings_for("charlie").is_empty());

        // Device must be Programmed with bumped config_generation.
        let dev = state.device_manager.registry().get(SERIAL).unwrap();
        assert!(dev.is_programmed());
        assert!(dev.config_generation > 1);

        // Collect emitted DeviceEvents and verify ordering:
        // ResourceDeleted(r_excl, Reprogram), ResourceUnlocked(r_shared, alice) -> ResourceDeleted(r_shared, Reprogram), DeviceProgrammed
        let mut collected = Vec::new();
        while let Ok(evt) = events.try_recv() {
            collected.push(evt);
        }

        let unlock_pos = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    DeviceEvent::ResourceUnlocked { resource_id, user_id, .. }
                        if resource_id == &format!("{SERIAL}:r_shared") && user_id == "alice"
                )
            })
            .expect("ResourceUnlocked must be emitted for locked shared resource");
        let delete_shared_pos = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    DeviceEvent::ResourceDeleted { resource_id, reason: DeleteReason::Reprogram, .. }
                        if resource_id == &format!("{SERIAL}:r_shared")
                )
            })
            .expect("ResourceDeleted(Reprogram) must be emitted for r_shared");
        assert!(
            unlock_pos < delete_shared_pos,
            "ResourceUnlocked must precede ResourceDeleted for r_shared"
        );

        assert!(
            collected.iter().any(|e| matches!(
                e,
                DeviceEvent::ResourceDeleted { resource_id, reason: DeleteReason::Reprogram, .. }
                    if resource_id == &format!("{SERIAL}:r_excl")
            )),
            "ResourceDeleted(Reprogram) must be emitted for r_excl"
        );
        assert!(
            collected.iter().any(|e| matches!(
                e,
                DeviceEvent::DeviceProgrammed { serial, .. } if serial == SERIAL
            )),
            "DeviceProgrammed must be emitted after successful configure"
        );
    }

    #[tokio::test]
    async fn test_clear_fpga_notifies_all_holders() {
        let state = test_state();

        let r_shared = Resource::new(
            format!("{SERIAL}:r_shared"),
            "r_shared".to_string(),
            None,
            SERIAL.to_string(),
            AccessMode::Shared,
        );
        state.resource_registry.register(r_shared);
        state
            .resource_registry
            .acquire(&format!("{SERIAL}:r_shared"), &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        state
            .resource_registry
            .acquire(&format!("{SERIAL}:r_shared"), &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        state
            .resource_registry
            .lock(&format!("{SERIAL}:r_shared"), &"alice".to_string())
            .unwrap();

        let mut events = state.device_manager.subscribe();
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/devices/{SERIAL}/configure"))
            .header("x-user-id", "alice")
            .body(Body::empty())
            .unwrap();

        let resp = routes().with_state(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["success"], true);

        // Device is now Connected (unprogrammed) and all resources are deleted.
        let dev = state.device_manager.registry().get(SERIAL).unwrap();
        assert!(!dev.is_programmed());
        assert_eq!(state.resource_registry.count(), 0);
        assert!(state.resource_registry.holdings_for("alice").is_empty());
        assert!(state.resource_registry.holdings_for("bob").is_empty());

        let mut collected = Vec::new();
        while let Ok(evt) = events.try_recv() {
            collected.push(evt);
        }

        let unlock_pos = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    DeviceEvent::ResourceUnlocked { resource_id, user_id, .. }
                        if resource_id == &format!("{SERIAL}:r_shared") && user_id == "alice"
                )
            })
            .expect("ResourceUnlocked must be emitted before ResourceDeleted on clear");
        let delete_pos = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    DeviceEvent::ResourceDeleted { resource_id, reason: DeleteReason::Clear, .. }
                        if resource_id == &format!("{SERIAL}:r_shared")
                )
            })
            .expect("ResourceDeleted(Clear) must be emitted on clear");
        assert!(unlock_pos < delete_pos);
    }

    #[tokio::test]
    async fn test_reset_fpga_resources_retained() {
        // D27: ResetFPGA resets logic inside the running bitfile without
        // changing the bitfile itself, so registered resources and holders
        // are retained (unlike ConfigureFPGA / ClearFPGAConfiguration).
        let state = test_state();
        let r_shared = Resource::new(
            format!("{SERIAL}:r_keep"),
            "r_keep".to_string(),
            None,
            SERIAL.to_string(),
            AccessMode::Shared,
        );
        state.resource_registry.register(r_shared);
        state
            .resource_registry
            .acquire(&format!("{SERIAL}:r_keep"), &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Attempt POST /devices/{serial}/classic/reset (returns 501 without DLL,
        // and never touches resource_registry).
        let req = Request::builder()
            .method("POST")
            .uri(format!("/devices/{SERIAL}/classic/reset"))
            .header("x-user-id", "alice")
            .body(Body::empty())
            .unwrap();
        let _ = crate::rest::classic::routes()
            .with_state(state.clone())
            .oneshot(req)
            .await
            .unwrap();

        assert_eq!(state.resource_registry.count(), 1);
        assert_eq!(state.resource_registry.holdings_for("alice").len(), 1);
    }
}
