//! # Device Routes
//!
//! REST endpoints for device discovery, info, sensors, settings, SYZYGY,
//! and configuration. Implements the `/devices/*` paths from ADR-004.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/devices/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/devices", get(list_devices))
        .route("/devices/{serial}", get(get_device_info))
        .route("/devices/{serial}/sensors", get(get_device_sensors))
        .route("/devices/{serial}/settings", get(get_device_settings))
        .route("/devices/{serial}/settings/{key}", get(get_device_setting).put(set_device_setting))
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

// ============================================================================
// Handlers
// ============================================================================

/// `GET /devices` — List all connected FPGA devices.
async fn list_devices(
    State(state): State<Arc<AppState>>,
) -> Json<DeviceListBody> {
    let devices = state.device_manager.registry().list();
    let summaries = devices
        .iter()
        .map(|d| DeviceSummary {
            serial: d.serial().to_string(),
            product_name: d.product_name().to_string(),
            is_programmed: d.is_programmed(),
        })
        .collect();

    Json(DeviceListBody {
        devices: summaries,
    })
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
async fn get_device_sensors(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("Sensor reading requires FFI integration"))
}

/// `GET /devices/{serial}/settings` — List all device settings.
async fn get_device_settings(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("Settings require FFI integration"))
}

/// `GET /devices/{serial}/settings/{key}` — Get a single setting.
async fn get_device_setting(
    Path((_serial, _key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("Settings require FFI integration"))
}

/// `PUT /devices/{serial}/settings/{key}` — Set a setting value.
async fn set_device_setting(
    Path((_serial, _key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("Settings require FFI integration"))
}

/// `GET /devices/{serial}/syzygy` — Get SYZYGY port information.
async fn get_syzygy_ports(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("SYZYGY discovery requires FFI integration"))
}

/// `POST /devices/{serial}/configure` — Program FPGA with a bitfile.
async fn configure_fpga(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("FPGA configuration requires FFI integration"))
}

/// `GET /devices/{serial}/bitfile` — Get current bitfile info.
async fn get_bitfile_info(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("Bitfile info requires FFI integration"))
}

/// `GET /devices/{serial}/dataport` — Get active data port type.
async fn get_dataport_type(
    Path(_serial): Path<String>,
) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("Data port type requires FFI integration"))
}
