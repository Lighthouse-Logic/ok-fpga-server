//! # ok-FPGA-Server REST GUI Common Library
//!
//! Shared API client for the Rust REST GUI demos (egui and iced).
//! Provides typed async methods for the Classic Data Port endpoints,
//! resource management, device discovery, and FPGA configuration
//! needed by the interactive Counters.bit demo.
//!
//! This crate is NOT standalone — it is used as a path dependency by
//! `rust-rest-gui-egui` and `rust-rest-gui-iced`.
//!
//! ## Architecture
//!
//! The [`CountersClient`] wraps a `reqwest::Client` and provides methods
//! specifically for the Counters.bit demo workflow:
//!
//! 1. Connect to server and list devices
//! 2. Program FPGA with Counters.bit
//! 3. Register and acquire a shared resource
//! 4. Read/write Classic Data Port endpoints (WireIn, WireOut, TriggerIn, TriggerOut)
//! 5. Lock/unlock resources
//! 6. Release resource on shutdown

pub mod mdns_browser;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

// ============================================================================
// Error Type
// ============================================================================

/// Error type for REST client operations.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// HTTP transport error (connection refused, timeout, etc.)
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Server returned an error response (4xx or 5xx).
    #[error("Server error {status}: {body}")]
    Server { status: u16, body: String },

    /// JSON deserialization failed — the response body did not match the
    /// expected schema. This replaces the old `unwrap_or_default()` pattern
    /// so deserialization failures are visible instead of silently returning
    /// empty defaults.
    #[error("Deserialization error: {0}")]
    Deserialize(#[from] serde_json::Error),
}

// ============================================================================
// Data Models
// ============================================================================

/// Counter state read from the FPGA via WireOut and TriggerOut endpoints.
///
/// Updated on each poll cycle by reading WireOut 0x20, 0x21, 0x22 and
/// TriggerOut 0x60, 0x61.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CounterState {
    /// Counter #1 value (WireOut 0x20, bits 7:0).
    pub count1: u8,
    /// Counter #2 value (WireOut 0x21, bits 7:0).
    pub count2: u8,
    /// On-board pushbutton status (WireOut 0x22, bits 3:0).
    pub buttons: u8,
    /// Counter #1 == 0x00 trigger (TriggerOut 0x60, bit 0).
    pub count1_eq_00: bool,
    /// Counter #1 == 0x80 trigger (TriggerOut 0x60, bit 1).
    pub count1_eq_80: bool,
    /// Counter #2 == 0xFF trigger (TriggerOut 0x61, bit 0).
    pub count2_eq_ff: bool,
}

/// WireIn control state — the current value of WireIn endpoint 0x00.
///
/// Tracks the RESET1, DISABLE1, and AUTOCOUNT2 control signals as booleans
/// and converts them to the bitmask format expected by the REST API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WireInState {
    /// Bit 0: When true, Counter #1 is held at 0x00 and does not count.
    pub reset1: bool,
    /// Bit 1: When true, Counter #1 holds its value and does not count.
    pub disable1: bool,
    /// Bit 2: When true, Counter #2 auto-counts instead of manual trigger control.
    pub autocount2: bool,
}

impl WireInState {
    /// Convert the boolean fields to the 16-bit value for WireIn 0x00.
    pub fn to_value(&self) -> u32 {
        let mut val: u32 = 0;
        if self.reset1 { val |= 1 << 0; }
        if self.disable1 { val |= 1 << 1; }
        if self.autocount2 { val |= 1 << 2; }
        val
    }
}

/// Summary of a connected device, extracted from the server's JSON response.
///
/// Field names and types match the OpenAPI `DeviceInfo` schema
/// (`api/openapi.yaml` lines 1425–1470). Per `ai-collaboration.md` §6,
/// clients must parse responses according to the spec.
///
/// Fields not needed by the Counters demo are kept for completeness and
/// use `#[serde(default)]` so missing fields don't cause parse failures.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    #[serde(default)]
    pub device_id: String,
    pub product_name: String,
    #[serde(default)]
    pub product_id: i32,
    #[serde(default)]
    pub device_interface: String,
    #[serde(default)]
    pub usb_speed: String,
    #[serde(default)]
    pub device_major_version: i32,
    #[serde(default)]
    pub device_minor_version: i32,
    #[serde(default)]
    pub wire_width: i32,
    #[serde(default)]
    pub trigger_width: i32,
    #[serde(default)]
    pub pipe_width: i32,
    #[serde(default)]
    pub register_address_width: i32,
    #[serde(default)]
    pub register_data_width: i32,
    #[serde(default)]
    pub fpga_vendor: String,
    #[serde(default)]
    pub dataport: String,
    #[serde(default)]
    pub is_front_panel_enabled: bool,
    /// Phase 34-B: SYZYGY label suffix (e.g. "0:DAC3283 | 2:ADC3241")
    #[serde(default)]
    pub syzygy_label: String,
}

impl DeviceInfo {
    /// Build a user-friendly display label: "XEM7320-A75T (SERIAL) | 0:DAC | 2:ADC"
    pub fn display_label(&self) -> String {
        if self.syzygy_label.is_empty() {
            format!("{} ({})", self.product_name, self.serial)
        } else {
            format!("{} ({}) | {}", self.product_name, self.serial, self.syzygy_label)
        }
    }
}

/// Wrapper for the `GET /devices` response body.
///
/// The server returns `{"devices": [...]}` per the OpenAPI spec,
/// not a bare array. This struct deserializes the wrapper.
#[derive(Debug, Deserialize)]
struct DeviceListResponse {
    devices: Vec<DeviceInfo>,
}

/// Summary of a resource, extracted from the server's JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub id: String,
    pub name: String,
    pub device_serial: String,
    pub access_mode: String,
    #[serde(default, deserialize_with = "deserialize_holders")]
    pub holders: Vec<String>,
    #[serde(default)]
    pub locked_by: Option<String>,
}

/// Deserializes `holders` from either ADR-034 `[{"user_id": "...", "mode": "..."}]`
/// objects (OpenAPI `Holder`) or legacy `["..."]` strings.
fn deserialize_holders<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum HolderItem {
        Object { user_id: String },
        UserId(String),
    }

    let items: Vec<HolderItem> = Vec::deserialize(deserializer)?;
    Ok(items
        .into_iter()
        .map(|item| match item {
            HolderItem::Object { user_id } => user_id,
            HolderItem::UserId(uid) => uid,
        })
        .collect())
}

// ============================================================================
// REST Client
// ============================================================================

/// REST API client for the Counters.bit interactive demo.
///
/// Provides async methods for every API call needed by the GUI:
/// server status, device listing, FPGA configuration, Classic Data Port
/// I/O, and resource management.
///
/// # Example
///
/// ```rust,no_run
/// use rust_rest_gui_common::CountersClient;
///
/// #[tokio::main]
/// async fn main() {
///     let client = CountersClient::new("http://localhost:8088", "Rusty Rester");
///     let devices = client.list_devices().await.unwrap();
///     println!("Found {} devices", devices.len());
/// }
/// ```
pub struct CountersClient {
    /// The underlying HTTP client (connection-pooled).
    client: Client,
    /// Base URL of the REST API (e.g., "http://localhost:8088/api/v1").
    base_url: String,
    /// User identity sent via X-User-Id header for session tracking.
    user_id: String,
    /// Phase 34-B: Cache SYZYGY labels per serial to avoid re-fetching on every refresh.
    syzygy_cache: Mutex<HashMap<String, String>>,
}

impl CountersClient {
    /// Create a new Counters demo REST client.
    ///
    /// # Arguments
    /// * `rest_url` — Server REST API base URL (e.g., "http://localhost:8088")
    /// * `user_id` — User identity for session tracking (e.g., "Rusty Rester")
    pub fn new(rest_url: &str, user_id: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("{}/api/v1", rest_url.trim_end_matches('/')),
            user_id: user_id.to_string(),
            syzygy_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the user identity string.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    // ========================================================================
    // Internal HTTP helpers
    // ========================================================================

    /// Perform a GET request with the X-User-Id header.
    async fn get(&self, path: &str) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client.get(&url)
            .header("X-User-Id", &self.user_id)
            .send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        Ok(resp.json().await?)
    }

    /// Perform a POST request with a JSON body and the X-User-Id header.
    async fn post(&self, path: &str, body: Option<Value>) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url)
            .header("X-User-Id", &self.user_id);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        // Some endpoints return empty bodies (204 No Content).
        let text = resp.text().await.unwrap_or_default();
        if text.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        }
    }

    /// Perform a PUT request with a JSON body and the X-User-Id header.
    async fn put(&self, path: &str, body: Value) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client.put(&url)
            .header("X-User-Id", &self.user_id)
            .json(&body)
            .send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        let text = resp.text().await.unwrap_or_default();
        if text.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        }
    }

    // ========================================================================
    // Server & Device Discovery
    // ========================================================================

    /// `GET /server/status` — Check if the server is reachable and get status info.
    pub async fn get_server_status(&self) -> Result<Value, ClientError> {
        self.get("/server/status").await
    }

    /// `GET /devices` — List all connected FPGA devices.
    ///
    /// The server returns `{"devices": [...]}` per the OpenAPI spec.
    /// Deserialization errors are propagated (not silently swallowed)
    /// so contract mismatches are immediately visible.
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>, ClientError> {
        let val = self.get("/devices").await?;
        // Unwrap the {"devices": [...]} wrapper per OpenAPI spec.
        let response: DeviceListResponse = serde_json::from_value(val)?;
        let mut devices = response.devices;
        // Phase 34-B: Populate SYZYGY labels (cached — only fetches once per serial).
        for dev in &mut devices {
            let cached = self.syzygy_cache.lock().unwrap().get(&dev.serial).cloned();
            if let Some(label) = cached {
                dev.syzygy_label = label;
            } else if let Ok(label) = self.get_syzygy_label(&dev.serial).await {
                self.syzygy_cache.lock().unwrap().insert(dev.serial.clone(), label.clone());
                dev.syzygy_label = label;
            }
        }
        Ok(devices)
    }

    /// Phase 34-B: Fetch SYZYGY ports and build a compact label string.
    async fn get_syzygy_label(&self, serial: &str) -> Result<String, ClientError> {
        let val = self.get(&format!("/devices/{}/syzygy", serial)).await?;
        let ports = val.get("ports").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        let parts: Vec<String> = ports
            .iter()
            .filter(|p| p.get("is_populated").and_then(|v| v.as_bool()).unwrap_or(false))
            .map(|p| {
                let idx = p.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                let name = p.get("product_name").and_then(|v| v.as_str()).unwrap_or("");
                let model = p.get("product_model").and_then(|v| v.as_str()).unwrap_or("");
                if model.is_empty() || model == name {
                    format!("{}:{}", idx, name)
                } else {
                    format!("{}:{} ({})", idx, name, model)
                }
            })
            .collect();
        Ok(parts.join(" | "))
    }

    // ========================================================================
    // FPGA Configuration
    // ========================================================================

    /// `POST /devices/{serial}/configure_from_server_path` — Program FPGA from a server-side path.
    ///
    /// Sends a JSON body `{"bitfile_path": "..."}` to the server. The bitfile
    /// must exist on the server's filesystem. Use this when the bitfile is
    /// hosted on the same machine as the server.
    ///
    /// Returns `BitfileInfo` on success (ADR-028).
    pub async fn configure_fpga(&self, serial: &str, bitfile_path: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "bitfile_path": bitfile_path });
        self.post(&format!("/devices/{}/configure_from_server_path", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/configure` — Program FPGA from a local file.
    ///
    /// Reads the bitfile from the client's local filesystem and uploads it
    /// to the server as `multipart/form-data`. The server calls
    /// `ConfigureFPGAFromMemory` with the received bytes.
    ///
    /// The multipart form includes:
    /// - `bitfile` — the binary data with the original filename
    /// - `filename` — the basename of the local file
    /// - `user_id` — the client's user identity
    ///
    /// Returns `BitfileInfo` on success (ADR-028).
    pub async fn configure_fpga_from_file(
        &self, serial: &str, local_path: &std::path::Path,
    ) -> Result<Value, ClientError> {
        let data = tokio::fs::read(local_path).await.map_err(|e| {
            ClientError::Server {
                status: 0,
                body: format!("failed to read local file '{}': {}", local_path.display(), e),
            }
        })?;

        // Extract the filename from the local path for the form fields.
        let filename = local_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "uploaded.bit".to_string());

        // Build a multipart form with the bitfile binary and metadata fields.
        let form = reqwest::multipart::Form::new()
            .part(
                "bitfile",
                reqwest::multipart::Part::bytes(data)
                    .file_name(filename.clone())
                    .mime_str("application/octet-stream")
                    .unwrap(),
            )
            .text("filename", filename)
            .text("user_id", self.user_id.clone());

        let url = format!("{}/devices/{}/configure", self.base_url, serial);
        let resp = self.client.post(&url)
            .header("X-User-Id", &self.user_id)
            .multipart(form)
            .send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        let text = resp.text().await.unwrap_or_default();
        if text.is_empty() {
            Ok(Value::Null)
        } else {
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        }
    }

    // ========================================================================
    // Classic Data Port — WireIn / WireOut
    // ========================================================================

    /// `PUT /devices/{serial}/classic/wires/in` — Set WireIn value.
    ///
    /// Sets the value and mask for a WireIn endpoint. The server calls
    /// `UpdateWireIns()` implicitly. For the Counters demo, endpoint 0x00
    /// controls RESET1 (bit 0), DISABLE1 (bit 1), and AUTOCOUNT2 (bit 2).
    pub async fn set_wire_in(
        &self,
        serial: &str,
        endpoint: u8,
        value: u32,
        mask: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "endpoint": endpoint,
            "value": value,
            "mask": mask,
        });
        self.put(&format!("/devices/{}/classic/wires/in", serial), body).await
    }

    /// `GET /devices/{serial}/classic/wires/out?endpoint={ep}` — Read a specific WireOut value.
    ///
    /// The server calls `UpdateWireOuts()` implicitly before reading.
    pub async fn get_wire_out_value(&self, serial: &str, endpoint: u8) -> Result<u32, ClientError> {
        let val = self.get(&format!("/devices/{}/classic/wires/out?endpoint={}", serial, endpoint)).await?;
        // Extract the value field from the JSON response.
        Ok(val["value"].as_u64().unwrap_or(0) as u32)
    }

    // ========================================================================
    // Classic Data Port — TriggerIn / TriggerOut
    // ========================================================================

    /// `POST /devices/{serial}/classic/triggers/in` — Fire a one-shot TriggerIn.
    ///
    /// For the Counters demo, endpoint 0x40:
    /// - bit 0 = RESET2 (reset counter #2)
    /// - bit 1 = UP2 (count up)
    /// - bit 2 = DOWN2 (count down)
    pub async fn activate_trigger_in(
        &self,
        serial: &str,
        endpoint: u8,
        bit: u8,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "endpoint": endpoint, "bit": bit });
        self.post(&format!("/devices/{}/classic/triggers/in", serial), Some(body)).await
    }

    /// `GET /devices/{serial}/classic/triggers/out?endpoint={ep}` — Read TriggerOut vector.
    ///
    /// Returns a bitmask of which triggers have fired since the last read.
    /// The server calls `UpdateTriggerOuts()` implicitly.
    pub async fn get_trigger_out_vector(&self, serial: &str, endpoint: u8) -> Result<u32, ClientError> {
        let val = self.get(&format!("/devices/{}/classic/triggers/out?endpoint={}", serial, endpoint)).await?;
        Ok(val["vector"].as_u64().unwrap_or(0) as u32)
    }

    // ========================================================================
    // Combined Poll — Read all outputs in one batch
    // ========================================================================

    /// Read all counter outputs in a single batch.
    ///
    /// Reads WireOut 0x20 (count1), 0x21 (count2), 0x22 (buttons),
    /// TriggerOut 0x60 (count1 comparators), and TriggerOut 0x61
    /// (count2 comparator). The server calls UpdateWireOuts/UpdateTriggerOuts
    /// implicitly on each GET.
    ///
    /// This is the main polling method called on each timer tick.
    pub async fn poll_counter_state(&self, serial: &str) -> Result<CounterState, ClientError> {
        // Read all three WireOut endpoints.
        let count1 = self.get_wire_out_value(serial, 0x20).await? as u8;
        let count2 = self.get_wire_out_value(serial, 0x21).await? as u8;
        let buttons = self.get_wire_out_value(serial, 0x22).await? as u8;

        // Read both TriggerOut endpoints.
        let trig_60 = self.get_trigger_out_vector(serial, 0x60).await?;
        let trig_61 = self.get_trigger_out_vector(serial, 0x61).await?;

        Ok(CounterState {
            count1,
            count2,
            buttons,
            count1_eq_00: (trig_60 & 0x01) != 0,
            count1_eq_80: (trig_60 & 0x02) != 0,
            count2_eq_ff: (trig_61 & 0x01) != 0,
        })
    }

    /// Send the current WireIn state to the FPGA.
    ///
    /// The server calls `UpdateWireIns()` implicitly on PUT.
    pub async fn send_wire_in(&self, serial: &str, state: &WireInState) -> Result<(), ClientError> {
        // Mask 0x07 covers bits 0-2 (RESET1, DISABLE1, AUTOCOUNT2).
        self.set_wire_in(serial, 0x00, state.to_value(), 0x07).await?;
        Ok(())
    }

    // ========================================================================
    // Resource Management
    // ========================================================================

    /// `POST /resources` — Create a new shared Counter Resource.
    /// Phase 34-E: `resource_type` parameter allows the caller to choose
    /// "shared" or "exclusive" when creating a resource.
    pub async fn create_resource(
        &self,
        name: &str,
        device_serial: &str,
        resource_type: &str,
    ) -> Result<ResourceInfo, ClientError> {
        let body = serde_json::json!({
            "name": name,
            "device_serial": device_serial,
            "access_mode": resource_type,
            "interfaces": ["classic"],
            "description": "Counters.bit hardware test resource",
        });
        let val = self.post("/resources", Some(body)).await?;
        let info: ResourceInfo = serde_json::from_value(val).unwrap_or_else(|_| ResourceInfo {
            id: String::new(),
            name: name.to_string(),
            device_serial: device_serial.to_string(),
            access_mode: resource_type.to_string(),
            holders: vec![],
            locked_by: None,
        });
        Ok(info)
    }

    /// `POST /resources/{id}/acquire` — Acquire the resource in shared mode.
    pub async fn acquire_resource(&self, resource_id: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "mode": "shared",
            "user_id": self.user_id,
        });
        self.post(&format!("/resources/{}/acquire", resource_id), Some(body)).await
    }

    /// `POST /resources/{id}/acquire` — Acquire the resource in read-only mode.
    ///
    /// ReadOnly holders can observe output endpoints and address ranges but
    /// cannot perform write operations or state-disturbing calls (ADR-026).
    pub async fn acquire_resource_readonly(&self, resource_id: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "mode": "read_only",
            "user_id": self.user_id,
        });
        self.post(&format!("/resources/{}/acquire", resource_id), Some(body)).await
    }

    /// `POST /resources/{id}/release` — Release the resource.
    pub async fn release_resource(&self, resource_id: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "user_id": self.user_id });
        self.post(&format!("/resources/{}/release", resource_id), Some(body)).await
    }

    /// `GET /resources/{id}` — Get resource details including current holders.
    pub async fn get_resource(&self, resource_id: &str) -> Result<ResourceInfo, ClientError> {
        let val = self.get(&format!("/resources/{}", resource_id)).await?;
        let info: ResourceInfo = serde_json::from_value(val).unwrap_or_else(|_| ResourceInfo {
            id: resource_id.to_string(),
            name: String::new(),
            device_serial: String::new(),
            access_mode: String::new(),
            holders: vec![],
            locked_by: None,
        });
        Ok(info)
    }

    /// `GET /resources` — List all resources, optionally filtered by device serial.
    ///
    /// Used for smart-skip logic: check if a resource already exists for a device.
    pub async fn list_resources(&self) -> Result<Vec<ResourceInfo>, ClientError> {
        let val = self.get("/resources").await?;
        // The server returns { "count": N, "resources": [...] }.
        let resources = val["resources"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let infos: Vec<ResourceInfo> = resources
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(infos)
    }

    /// `GET /devices/{serial}` — Get full device info for a single device.
    ///
    /// Used to check if a device is already programmed (state == "Programmed").
    pub async fn get_device_info(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}", serial)).await
    }

    /// `POST /resources/{id}/lock` — Lock the resource for exclusive control.
    pub async fn lock_resource(&self, resource_id: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "user_id": self.user_id });
        self.post(&format!("/resources/{}/lock", resource_id), Some(body)).await
    }

    /// `POST /resources/{id}/unlock` — Release the lock.
    pub async fn unlock_resource(&self, resource_id: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "user_id": self.user_id });
        self.post(&format!("/resources/{}/unlock", resource_id), Some(body)).await
    }

    /// `POST /heartbeat` — Send a liveness heartbeat to keep the session alive.
    pub async fn heartbeat(&self) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "user_id": self.user_id });
        self.post("/heartbeat", Some(body)).await
    }

    /// `POST /disconnect` — Cleanly disconnect this user from the server.
    ///
    /// Releases all resources held by this user on any device, removes
    /// the user session, and stops heartbeat tracking. This is the
    /// recommended way to end a client session (Phase 25-Fix-H, F10).
    ///
    /// Returns the server response with `released_resources` count.
    pub async fn disconnect(&self) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "user_id": self.user_id });
        self.post("/disconnect", Some(body)).await
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // WireInState bitmask tests
    // ========================================================================

    #[test]
    fn test_wire_in_state_default_is_zero() {
        let state = WireInState::default();
        assert_eq!(state.to_value(), 0);
    }

    #[test]
    fn test_wire_in_state_reset1_bit() {
        let state = WireInState { reset1: true, disable1: false, autocount2: false };
        assert_eq!(state.to_value(), 0x01);
    }

    #[test]
    fn test_wire_in_state_disable1_bit() {
        let state = WireInState { reset1: false, disable1: true, autocount2: false };
        assert_eq!(state.to_value(), 0x02);
    }

    #[test]
    fn test_wire_in_state_autocount2_bit() {
        let state = WireInState { reset1: false, disable1: false, autocount2: true };
        assert_eq!(state.to_value(), 0x04);
    }

    #[test]
    fn test_wire_in_state_all_bits_set() {
        let state = WireInState { reset1: true, disable1: true, autocount2: true };
        assert_eq!(state.to_value(), 0x07);
    }

    #[test]
    fn test_wire_in_state_reset1_disable1_combination() {
        let state = WireInState { reset1: true, disable1: true, autocount2: false };
        assert_eq!(state.to_value(), 0x03);
    }

    // ========================================================================
    // CounterState default tests
    // ========================================================================

    #[test]
    fn test_counter_state_default() {
        let state = CounterState::default();
        assert_eq!(state.count1, 0);
        assert_eq!(state.count2, 0);
        assert_eq!(state.buttons, 0);
        assert!(!state.count1_eq_00);
        assert!(!state.count1_eq_80);
        assert!(!state.count2_eq_ff);
    }

    // ========================================================================
    // DeviceInfo construction tests
    // ========================================================================

    #[test]
    fn test_device_info_construction() {
        let info = DeviceInfo {
            serial: "ABC123".to_string(),
            product_name: "XEM7320-A75T".to_string(),
            device_interface: "usb3".to_string(),
            usb_speed: "super".to_string(),
            ..Default::default()
        };
        assert_eq!(info.serial, "ABC123");
        assert_eq!(info.product_name, "XEM7320-A75T");
        assert_eq!(info.device_interface, "usb3");
    }

    // ========================================================================
    // ResourceInfo tests
    // ========================================================================

    #[test]
    fn test_resource_info_locked_by_none() {
        let info = ResourceInfo {
            id: "res-1".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "ABC123".to_string(),
            access_mode: "shared".to_string(),
            holders: vec!["user1".to_string()],
            locked_by: None,
        };
        assert!(info.locked_by.is_none());
        assert_eq!(info.holders.len(), 1);
    }

    #[test]
    fn test_resource_info_locked_by_present() {
        let info = ResourceInfo {
            id: "res-1".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "ABC123".to_string(),
            access_mode: "shared".to_string(),
            holders: vec![],
            locked_by: Some("Icy Rester".to_string()),
        };
        assert_eq!(info.locked_by.as_deref(), Some("Icy Rester"));
    }

    #[test]
    fn test_resource_info_empty_holders() {
        let info = ResourceInfo {
            id: "res-1".to_string(),
            name: "Test".to_string(),
            device_serial: "S1".to_string(),
            access_mode: "exclusive".to_string(),
            holders: vec![],
            locked_by: None,
        };
        assert!(info.holders.is_empty());
    }

    // ========================================================================
    // Client URL construction tests
    // ========================================================================

    #[test]
    fn test_client_base_url_no_trailing_slash() {
        let client = CountersClient::new("http://localhost:8088", "test-user");
        assert_eq!(client.user_id(), "test-user");
        // base_url should be "{url}/api/v1"
        assert_eq!(client.base_url, "http://localhost:8088/api/v1");
    }

    #[test]
    fn test_client_base_url_with_trailing_slash() {
        let client = CountersClient::new("http://localhost:8088/", "test-user");
        // Trailing slash should be stripped before appending /api/v1
        assert_eq!(client.base_url, "http://localhost:8088/api/v1");
    }

    #[test]
    fn test_client_user_id() {
        let client = CountersClient::new("http://localhost:8088", "Rusty Rester");
        assert_eq!(client.user_id(), "Rusty Rester");
    }

    // ========================================================================
    // ClientError display tests
    // ========================================================================

    #[test]
    fn test_client_error_server_display() {
        let err = ClientError::Server {
            status: 404,
            body: "Not Found".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("404"));
        assert!(msg.contains("Not Found"));
    }

    // ========================================================================
    // Mock HTTP integration tests (using mockito)
    // ========================================================================

    #[tokio::test]
    async fn test_list_devices_parses_response() {
        let mut server = mockito::Server::new_async().await;
        // Mock response matches the OpenAPI spec: {"devices": [...]} wrapper
        // with full DeviceInfo fields (per ai-collaboration.md §6).
        let mock = server.mock("GET", "/api/v1/devices")
            .match_header("X-User-Id", "test-user")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"devices":[{"serial":"SN001","product_name":"XEM7320","device_interface":"usb3","usb_speed":"super","is_front_panel_enabled":false}]}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let devices = client.list_devices().await.unwrap();

        mock.assert_async().await;
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "SN001");
        assert_eq!(devices[0].product_name, "XEM7320");
        assert_eq!(devices[0].device_interface, "usb3");
    }

    #[tokio::test]
    async fn test_list_devices_rejects_flat_array() {
        // Verify that a flat array (the OLD wrong format) now causes
        // a deserialization error instead of silently returning empty.
        let mut server = mockito::Server::new_async().await;
        let _mock = server.mock("GET", "/api/v1/devices")
            .match_header("X-User-Id", "test-user")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"serial":"SN001","product_name":"XEM7320"}]"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let result = client.list_devices().await;

        // Should fail with a deserialization error, NOT silently return empty.
        assert!(result.is_err(), "flat array should be rejected");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("Deserialization"), "expected Deserialization error, got: {}", err_msg);
    }

    #[tokio::test]
    async fn test_get_server_status_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/server/status")
            .with_status(200)
            .with_body(r#"{"status":"ok"}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let result = client.get_server_status().await;

        mock.assert_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_configure_fpga_sends_bitfile_path_to_server_path_endpoint() {
        let mut server = mockito::Server::new_async().await;
        // ADR-028: server-side path configuration now uses the dedicated endpoint.
        let mock = server.mock("POST", "/api/v1/devices/SN001/configure_from_server_path")
            .match_header("X-User-Id", "test-user")
            .match_body(mockito::Matcher::JsonString(r#"{"bitfile_path":"Counters.bit"}"#.to_string()))
            .with_status(200)
            .with_body(r#"{"filename":"Counters.bit","programmed_at":"2026-08-23T00:00:00Z","programmed_by":"test-user"}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let result = client.configure_fpga("SN001", "Counters.bit").await;

        mock.assert_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_wire_in_sends_correct_body() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("PUT", "/api/v1/devices/SN001/classic/wires/in")
            .match_header("X-User-Id", "test-user")
            .match_body(mockito::Matcher::JsonString(
                r#"{"endpoint":0,"value":7,"mask":7}"#.to_string()
            ))
            .with_status(200)
            .with_body(r#"{}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let result = client.set_wire_in("SN001", 0x00, 7, 0x07).await;

        mock.assert_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_send_wire_in_uses_correct_mask() {
        let mut server = mockito::Server::new_async().await;
        // send_wire_in calls set_wire_in only (server calls UpdateWireIns implicitly)
        let mock_set = server.mock("PUT", "/api/v1/devices/SN001/classic/wires/in")
            .match_body(mockito::Matcher::JsonString(
                r#"{"endpoint":0,"value":5,"mask":7}"#.to_string()
            ))
            .with_status(200)
            .with_body(r#"{}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let wire = WireInState { reset1: true, disable1: false, autocount2: true };
        let result = client.send_wire_in("SN001", &wire).await;

        mock_set.assert_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_server_error_returns_client_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/server/status")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let result = client.get_server_status().await;

        mock.assert_async().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::Server { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "Internal Server Error");
            }
            other => panic!("Expected Server error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_wire_out_value_parses_value() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/devices/SN001/classic/wires/out")
            .match_query(mockito::Matcher::UrlEncoded("endpoint".to_string(), "32".to_string()))
            .with_status(200)
            .with_body(r#"{"value":42}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let value = client.get_wire_out_value("SN001", 0x20).await.unwrap();

        mock.assert_async().await;
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn test_activate_trigger_in_sends_endpoint_and_bit() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/v1/devices/SN001/classic/triggers/in")
            .match_body(mockito::Matcher::JsonString(
                r#"{"endpoint":64,"bit":1}"#.to_string()
            ))
            .with_status(200)
            .with_body(r#"{}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let result = client.activate_trigger_in("SN001", 0x40, 1).await;

        mock.assert_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_resource_parses_response() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("POST", "/api/v1/resources")
            .match_header("X-User-Id", "test-user")
            .with_status(200)
            .with_body(r#"{"id":"res-abc","name":"Counter Resource","device_serial":"SN001","access_mode":"shared","holders":[],"locked_by":null}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let info = client.create_resource("Counter Resource", "SN001", "shared").await.unwrap();

        mock.assert_async().await;
        assert_eq!(info.id, "res-abc");
        assert_eq!(info.name, "Counter Resource");
        assert_eq!(info.access_mode, "shared");
        assert!(info.locked_by.is_none());
    }

    // ========================================================================
    // Phase 25-Fix-F tests
    // ========================================================================

    #[tokio::test]
    async fn test_get_trigger_out_vector_reads_vector_field() {
        // F2: The server now returns {"endpoint": N, "triggered": bool, "vector": V}.
        // Verify the client reads from the `vector` field (not `value`).
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/devices/SN001/classic/triggers/out")
            .match_query(mockito::Matcher::UrlEncoded("endpoint".to_string(), "96".to_string()))
            .with_status(200)
            .with_body(r#"{"endpoint":96,"triggered":true,"vector":255}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let vector = client.get_trigger_out_vector("SN001", 0x60).await.unwrap();

        mock.assert_async().await;
        assert_eq!(vector, 255);
    }

    #[tokio::test]
    async fn test_list_resources_parses_response() {
        // F4 / ADR-034: list_resources() should parse the {count, resources: [...]} envelope
        // with ADR-034 Holder objects [{"user_id": "...", "mode": "..."}].
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/resources")
            .with_status(200)
            .with_body(r#"{"count":2,"resources":[{"id":"r1","name":"Resource 1","device_serial":"SN001","access_mode":"shared","holders":[{"user_id":"alice","mode":"shared"}],"locked_by":null},{"id":"r2","name":"Resource 2","device_serial":"SN002","access_mode":"exclusive","holders":[],"locked_by":null}]}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let resources = client.list_resources().await.unwrap();

        mock.assert_async().await;
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].id, "r1");
        assert_eq!(resources[0].device_serial, "SN001");
        assert_eq!(resources[0].holders, vec!["alice"]);
        assert_eq!(resources[1].id, "r2");
        assert_eq!(resources[1].access_mode, "exclusive");
    }

    #[tokio::test]
    async fn test_list_resources_empty() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/resources")
            .with_status(200)
            .with_body(r#"{"count":0,"resources":[]}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let resources = client.list_resources().await.unwrap();

        mock.assert_async().await;
        assert!(resources.is_empty());
    }

    #[tokio::test]
    async fn test_get_device_info_returns_json() {
        // F4: get_device_info() should return the raw JSON value for a device.
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/api/v1/devices/SN001")
            .with_status(200)
            .with_body(r#"{"serial":"SN001","state":"Programmed","product_name":"XEM7320"}"#)
            .create_async().await;

        let client = CountersClient::new(&server.url(), "test-user");
        let info = client.get_device_info("SN001").await.unwrap();

        mock.assert_async().await;
        assert_eq!(info["serial"].as_str().unwrap(), "SN001");
        assert_eq!(info["state"].as_str().unwrap(), "Programmed");
    }
}
