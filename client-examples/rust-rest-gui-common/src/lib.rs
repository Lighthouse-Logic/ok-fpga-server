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

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub product_name: String,
    pub interface: String,
    pub usb_speed: String,
    pub state: String,
}

/// Summary of a resource, extracted from the server's JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub id: String,
    pub name: String,
    pub device_serial: String,
    pub access_mode: String,
    #[serde(default)]
    pub holders: Vec<String>,
    #[serde(default)]
    pub locked_by: Option<String>,
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

    /// `GET /status` — Check if the server is reachable and get status info.
    pub async fn get_server_status(&self) -> Result<Value, ClientError> {
        self.get("/status").await
    }

    /// `GET /devices` — List all connected FPGA devices.
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>, ClientError> {
        let val = self.get("/devices").await?;
        // The API returns an array of device objects.
        let devices: Vec<DeviceInfo> = serde_json::from_value(val).unwrap_or_default();
        Ok(devices)
    }

    // ========================================================================
    // FPGA Configuration
    // ========================================================================

    /// `POST /devices/{serial}/configure` — Program the FPGA with a bitfile.
    ///
    /// The bitfile path is resolved server-side. The client sends the path
    /// as a string; the server reads and programs the file.
    pub async fn configure_fpga(&self, serial: &str, bitfile_path: &str) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "bitfile_path": bitfile_path });
        self.post(&format!("/devices/{}/configure", serial), Some(body)).await
    }

    // ========================================================================
    // Classic Data Port — WireIn / WireOut
    // ========================================================================

    /// `PUT /devices/{serial}/classic/wires/in` — Set WireIn value.
    ///
    /// Sets the value and mask for a WireIn endpoint. For the Counters demo,
    /// endpoint 0x00 controls RESET1 (bit 0), DISABLE1 (bit 1), and
    /// AUTOCOUNT2 (bit 2).
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

    /// `POST /devices/{serial}/classic/wires/in/update` — Commit pending WireIn changes.
    ///
    /// Must be called after `set_wire_in` to push the values to the FPGA.
    /// This corresponds to the FrontPanel `UpdateWireIns()` call.
    pub async fn update_wire_ins(&self, serial: &str) -> Result<Value, ClientError> {
        self.post(&format!("/devices/{}/classic/wires/in/update", serial), None).await
    }

    /// `POST /devices/{serial}/classic/wires/out/update` — Refresh WireOut values from FPGA.
    ///
    /// Must be called before reading WireOut values. This corresponds to the
    /// FrontPanel `UpdateWireOuts()` call.
    pub async fn update_wire_outs(&self, serial: &str) -> Result<Value, ClientError> {
        self.post(&format!("/devices/{}/classic/wires/out/update", serial), None).await
    }

    /// `GET /devices/{serial}/classic/wires/out/{endpoint}` — Read a specific WireOut value.
    pub async fn get_wire_out_value(&self, serial: &str, endpoint: u8) -> Result<u32, ClientError> {
        let val = self.get(&format!("/devices/{}/classic/wires/out/0x{:02x}", serial, endpoint)).await?;
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

    /// `GET /devices/{serial}/classic/triggers/out/{endpoint}` — Read TriggerOut vector.
    ///
    /// Returns a bitmask of which triggers have fired since the last read.
    pub async fn get_trigger_out_vector(&self, serial: &str, endpoint: u8) -> Result<u32, ClientError> {
        let val = self.get(&format!("/devices/{}/classic/triggers/out/0x{:02x}", serial, endpoint)).await?;
        Ok(val["value"].as_u64().unwrap_or(0) as u32)
    }

    // ========================================================================
    // Combined Poll — Read all outputs in one batch
    // ========================================================================

    /// Read all counter outputs in a single batch.
    ///
    /// Calls `UpdateWireOuts`, then reads WireOut 0x20 (count1), 0x21 (count2),
    /// 0x22 (buttons), TriggerOut 0x60 (count1 comparators), and TriggerOut 0x61
    /// (count2 comparator).
    ///
    /// This is the main polling method called on each timer tick.
    pub async fn poll_counter_state(&self, serial: &str) -> Result<CounterState, ClientError> {
        // Step 1: Refresh wire outputs from FPGA.
        self.update_wire_outs(serial).await?;

        // Step 2: Read all three WireOut endpoints.
        let count1 = self.get_wire_out_value(serial, 0x20).await? as u8;
        let count2 = self.get_wire_out_value(serial, 0x21).await? as u8;
        let buttons = self.get_wire_out_value(serial, 0x22).await? as u8;

        // Step 3: Read both TriggerOut endpoints.
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
    /// Combines `set_wire_in` and `update_wire_ins` into one call.
    pub async fn send_wire_in(&self, serial: &str, state: &WireInState) -> Result<(), ClientError> {
        // Mask 0x07 covers bits 0-2 (RESET1, DISABLE1, AUTOCOUNT2).
        self.set_wire_in(serial, 0x00, state.to_value(), 0x07).await?;
        self.update_wire_ins(serial).await?;
        Ok(())
    }

    // ========================================================================
    // Resource Management
    // ========================================================================

    /// `POST /resources` — Create a new shared Counter Resource.
    pub async fn create_resource(
        &self,
        name: &str,
        device_serial: &str,
    ) -> Result<ResourceInfo, ClientError> {
        let body = serde_json::json!({
            "name": name,
            "device_serial": device_serial,
            "access_mode": "shared",
            "description": "Counters.bit hardware test resource",
        });
        let val = self.post("/resources", Some(body)).await?;
        let info: ResourceInfo = serde_json::from_value(val).unwrap_or_else(|_| ResourceInfo {
            id: String::new(),
            name: name.to_string(),
            device_serial: device_serial.to_string(),
            access_mode: "shared".to_string(),
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
}
