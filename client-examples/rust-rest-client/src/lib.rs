//! # ok-FPGA-Server REST Client Library
//!
//! This module provides a typed Rust client for every REST endpoint exposed by
//! the ok-FPGA-Server. Each method corresponds to one endpoint in the
//! [OpenAPI 3.0 spec](../../api/openapi.yaml).
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rust_rest_client::OkFpgaRestClient;
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = OkFpgaRestClient::new("http://localhost:8088");
//!     let status = client.get_server_status().await.unwrap();
//!     println!("Server version: {}", status["api_version"]);
//! }
//! ```

// Re-export submodules so users can access them from the crate root.
pub mod discovery;
pub mod mqtt;

use reqwest::Client;
use serde_json::Value;

/// Error type for REST client operations.
///
/// Wraps both HTTP transport errors (network issues, timeouts) and
/// server-side errors (4xx/5xx responses with RFC 7807 Problem Detail bodies).
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// HTTP transport error (connection refused, timeout, etc.)
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Server returned an error response (4xx or 5xx).
    /// Contains the HTTP status code and the response body.
    #[error("Server error {status}: {body}")]
    Server {
        status: u16,
        body: String,
    },
}

/// REST client for the ok-FPGA-Server.
///
/// Wraps `reqwest::Client` and provides typed methods for every REST endpoint.
/// All methods are async and return `Result<Value, ClientError>` where `Value`
/// is the parsed JSON response body.
///
/// # Design
///
/// Methods are grouped by API category matching the OpenAPI spec:
/// - `server_*` — Server status, services, restart
/// - `list_devices`, `get_device_*` — Device discovery and info
/// - `configure_fpga`, `get_bitfile_info`, `get_dataport_type` — FPGA config
/// - `classic_*` — Classic data port (wires, triggers, pipes, registers)
/// - `axi_*` — AXI data port (lite, full, stream)
/// - `resource_*` — Resource management (list, acquire, release)
/// - `user_*` — User management
/// - `watch_events` — SSE event stream
pub struct OkFpgaRestClient {
    /// The underlying HTTP client (connection-pooled).
    client: Client,
    /// Base URL of the REST API (e.g., "http://localhost:8088").
    base_url: String,
}

impl OkFpgaRestClient {
    /// Create a new REST client pointing at the given base URL.
    ///
    /// The `/api/v1` prefix is appended automatically — pass the bare server
    /// URL (e.g., "http://localhost:8088").
    ///
    /// # Arguments
    /// * `base_url` — The server's REST API base URL (e.g., "http://localhost:8088")
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: format!("{}/api/v1", base_url.trim_end_matches('/')),
        }
    }

    /// Internal helper: perform a GET request and parse the JSON response.
    async fn get(&self, path: &str) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        Ok(resp.json().await?)
    }

    /// Internal helper: perform a POST request with a JSON body.
    async fn post(&self, path: &str, body: Option<Value>) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        Ok(resp.json().await?)
    }

    /// Internal helper: perform a PUT request with a JSON body.
    async fn put(&self, path: &str, body: Value) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.client.put(&url).json(&body).send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        Ok(resp.json().await?)
    }

    // ========================================================================
    // Server Endpoints (/server/*)
    // ========================================================================

    /// `GET /server/status` — Get server status including uptime and services.
    ///
    /// Returns: `{ "api_version": "1.0.0", "uptime_secs": 123, "device_count": 1,
    ///            "services": [...] }`
    pub async fn get_server_status(&self) -> Result<Value, ClientError> {
        self.get("/server/status").await
    }

    /// `POST /server/restart` — Restart the server.
    ///
    /// The server sends a 202 Accepted response and then restarts.
    pub async fn restart_server(&self) -> Result<Value, ClientError> {
        self.post("/server/restart", None).await
    }

    /// `GET /server/services` — List all manageable services and their status.
    pub async fn list_services(&self) -> Result<Value, ClientError> {
        self.get("/server/services").await
    }

    /// `POST /server/services/{name}/start` — Start a stopped service.
    pub async fn start_service(&self, name: &str) -> Result<Value, ClientError> {
        self.post(&format!("/server/services/{}/start", name), None).await
    }

    /// `POST /server/services/{name}/stop` — Stop a running service.
    pub async fn stop_service(&self, name: &str) -> Result<Value, ClientError> {
        self.post(&format!("/server/services/{}/stop", name), None).await
    }

    /// `POST /server/services/{name}/restart` — Restart a service.
    pub async fn restart_service(&self, name: &str) -> Result<Value, ClientError> {
        self.post(&format!("/server/services/{}/restart", name), None).await
    }

    // ========================================================================
    // Device Endpoints (/devices/*)
    // ========================================================================

    /// `GET /devices` — List all connected FPGA devices.
    ///
    /// Returns: `{ "devices": [{ "serial": "...", "product_name": "...",
    ///            "is_programmed": false }] }`
    pub async fn list_devices(&self) -> Result<Value, ClientError> {
        self.get("/devices").await
    }

    /// `GET /devices/{serial}` — Get detailed info for a specific device.
    pub async fn get_device_info(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}", serial)).await
    }

    /// `GET /devices/{serial}/sensors` — Read device sensor values.
    pub async fn get_device_sensors(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/sensors", serial)).await
    }

    /// `GET /devices/{serial}/settings` — List all device settings.
    pub async fn get_device_settings(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/settings", serial)).await
    }

    /// `GET /devices/{serial}/settings/{key}` — Get a single setting.
    pub async fn get_device_setting(&self, serial: &str, key: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/settings/{}", serial, key)).await
    }

    /// `PUT /devices/{serial}/settings/{key}` — Set a setting value.
    pub async fn set_device_setting(
        &self,
        serial: &str,
        key: &str,
        value: Value,
    ) -> Result<Value, ClientError> {
        self.put(&format!("/devices/{}/settings/{}", serial, key), value).await
    }

    /// `GET /devices/{serial}/syzygy` — Get SYZYGY port information.
    pub async fn get_syzygy_ports(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/syzygy", serial)).await
    }

    // ========================================================================
    // FPGA Configuration Endpoints
    // ========================================================================

    /// `POST /devices/{serial}/configure` — Program the FPGA with a bitfile.
    ///
    /// # Arguments
    /// * `serial` — Device serial number
    /// * `bitfile_path` — Path to bitfile on server filesystem
    pub async fn configure_fpga(
        &self,
        serial: &str,
        bitfile_path: &str,
    ) -> Result<Value, ClientError> {
        self.post(
            &format!("/devices/{}/configure", serial),
            Some(serde_json::json!({ "bitfile_path": bitfile_path })),
        )
        .await
    }

    /// `GET /devices/{serial}/bitfile` — Get info about the loaded bitfile.
    pub async fn get_bitfile_info(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/bitfile", serial)).await
    }

    /// `GET /devices/{serial}/dataport` — Get the active data port type.
    pub async fn get_dataport_type(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/dataport", serial)).await
    }

    // ========================================================================
    // Classic Data Port Endpoints (/devices/{serial}/classic/*)
    // ========================================================================

    /// `POST /devices/{serial}/classic/wires/in` — Set a WireIn value.
    pub async fn set_wire_in(
        &self,
        serial: &str,
        endpoint: u8,
        value: u32,
        mask: Option<u32>,
    ) -> Result<Value, ClientError> {
        let mut body = serde_json::json!({ "endpoint": endpoint, "value": value });
        if let Some(m) = mask {
            body["mask"] = serde_json::json!(m);
        }
        self.post(&format!("/devices/{}/classic/wires/in", serial), Some(body)).await
    }

    /// `GET /devices/{serial}/classic/wires/out` — Read WireOut values.
    pub async fn get_wire_out(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/classic/wires/out", serial)).await
    }

    /// `POST /devices/{serial}/classic/triggers/in` — Activate a TriggerIn.
    pub async fn activate_trigger_in(
        &self,
        serial: &str,
        endpoint: u8,
        bit: u8,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "endpoint": endpoint, "bit": bit });
        self.post(&format!("/devices/{}/classic/triggers/in", serial), Some(body)).await
    }

    /// `GET /devices/{serial}/classic/triggers/out` — Read TriggerOut values.
    pub async fn get_trigger_out(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/classic/triggers/out", serial)).await
    }

    /// `POST /devices/{serial}/classic/pipes/in` — Write data to PipeIn.
    pub async fn write_pipe_in(
        &self,
        serial: &str,
        endpoint: u8,
        data: &[u8],
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "endpoint": endpoint,
            "data": data,
        });
        self.post(&format!("/devices/{}/classic/pipes/in", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/classic/pipes/out` — Read data from PipeOut.
    pub async fn read_pipe_out(
        &self,
        serial: &str,
        endpoint: u8,
        length: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "endpoint": endpoint,
            "length": length,
        });
        self.post(&format!("/devices/{}/classic/pipes/out", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/classic/pipes/block-in` — Write to BlockPipeIn.
    pub async fn write_block_pipe_in(
        &self,
        serial: &str,
        endpoint: u8,
        data: &[u8],
        block_size: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "endpoint": endpoint,
            "data": data,
            "block_size": block_size,
        });
        self.post(&format!("/devices/{}/classic/pipes/block-in", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/classic/pipes/block-out` — Read from BlockPipeOut.
    pub async fn read_block_pipe_out(
        &self,
        serial: &str,
        endpoint: u8,
        length: u32,
        block_size: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({
            "endpoint": endpoint,
            "length": length,
            "block_size": block_size,
        });
        self.post(&format!("/devices/{}/classic/pipes/block-out", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/classic/registers/read` — Read registers.
    pub async fn read_register(
        &self,
        serial: &str,
        address: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "address": address });
        self.post(&format!("/devices/{}/classic/registers/read", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/classic/registers/write` — Write registers.
    pub async fn write_register(
        &self,
        serial: &str,
        address: u32,
        value: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "address": address, "value": value });
        self.post(&format!("/devices/{}/classic/registers/write", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/classic/reset` — Reset the FPGA.
    pub async fn classic_reset(&self, serial: &str) -> Result<Value, ClientError> {
        self.post(&format!("/devices/{}/classic/reset", serial), None).await
    }

    // ========================================================================
    // AXI Data Port Endpoints (/devices/{serial}/axi/*)
    // ========================================================================

    /// `POST /devices/{serial}/axi/lite/read` — AXI Lite read.
    pub async fn axi_lite_read(
        &self,
        serial: &str,
        address: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "address": address });
        self.post(&format!("/devices/{}/axi/lite/read", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/axi/lite/write` — AXI Lite write.
    pub async fn axi_lite_write(
        &self,
        serial: &str,
        address: u32,
        value: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "address": address, "value": value });
        self.post(&format!("/devices/{}/axi/lite/write", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/axi/lite/reset` — AXI Lite reset.
    pub async fn axi_lite_reset(&self, serial: &str) -> Result<Value, ClientError> {
        self.post(&format!("/devices/{}/axi/lite/reset", serial), None).await
    }

    /// `POST /devices/{serial}/axi/full/read` — AXI Full read.
    pub async fn axi_full_read(
        &self,
        serial: &str,
        address: u32,
        length: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "address": address, "length": length });
        self.post(&format!("/devices/{}/axi/full/read", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/axi/full/write` — AXI Full write.
    pub async fn axi_full_write(
        &self,
        serial: &str,
        address: u32,
        data: &[u8],
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "address": address, "data": data });
        self.post(&format!("/devices/{}/axi/full/write", serial), Some(body)).await
    }

    /// `GET /devices/{serial}/axi/full/datapath-width` — Get AXI Full datapath width.
    pub async fn axi_full_datapath_width(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/axi/full/datapath-width", serial)).await
    }

    /// `POST /devices/{serial}/axi/full/reset` — AXI Full reset.
    pub async fn axi_full_reset(&self, serial: &str) -> Result<Value, ClientError> {
        self.post(&format!("/devices/{}/axi/full/reset", serial), None).await
    }

    /// `POST /devices/{serial}/axi/stream/read` — AXI Stream read.
    pub async fn axi_stream_read(
        &self,
        serial: &str,
        length: u32,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "length": length });
        self.post(&format!("/devices/{}/axi/stream/read", serial), Some(body)).await
    }

    /// `POST /devices/{serial}/axi/stream/write` — AXI Stream write.
    pub async fn axi_stream_write(
        &self,
        serial: &str,
        data: &[u8],
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "data": data });
        self.post(&format!("/devices/{}/axi/stream/write", serial), Some(body)).await
    }

    /// `GET /devices/{serial}/axi/stream/datapath-width` — Get AXI Stream widths.
    pub async fn axi_stream_datapath_width(&self, serial: &str) -> Result<Value, ClientError> {
        self.get(&format!("/devices/{}/axi/stream/datapath-width", serial)).await
    }

    /// `POST /devices/{serial}/axi/stream/reset` — AXI Stream reset.
    pub async fn axi_stream_reset(&self, serial: &str) -> Result<Value, ClientError> {
        self.post(&format!("/devices/{}/axi/stream/reset", serial), None).await
    }

    // ========================================================================
    // Resource Endpoints (/resources/*)
    // ========================================================================

    /// `GET /resources` — List all resources across all programmed FPGAs.
    pub async fn list_resources(&self) -> Result<Value, ClientError> {
        self.get("/resources").await
    }

    /// `GET /resources/{resource_id}` — Get details for a specific resource.
    pub async fn get_resource(&self, resource_id: &str) -> Result<Value, ClientError> {
        self.get(&format!("/resources/{}", resource_id)).await
    }

    /// `POST /resources/{resource_id}/acquire` — Acquire a resource.
    ///
    /// # Arguments
    /// * `mode` — "shared" or "exclusive"
    /// * `user_id` — Identifier for the user acquiring the resource
    pub async fn acquire_resource(
        &self,
        resource_id: &str,
        mode: &str,
        user_id: &str,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "mode": mode, "user_id": user_id });
        self.post(&format!("/resources/{}/acquire", resource_id), Some(body)).await
    }

    /// `POST /resources/{resource_id}/release` — Release a resource.
    pub async fn release_resource(
        &self,
        resource_id: &str,
        user_id: &str,
    ) -> Result<Value, ClientError> {
        let body = serde_json::json!({ "user_id": user_id });
        self.post(&format!("/resources/{}/release", resource_id), Some(body)).await
    }

    // ========================================================================
    // User Endpoints (/users/*)
    // ========================================================================

    /// `GET /users` — List all connected users.
    pub async fn list_users(&self) -> Result<Value, ClientError> {
        self.get("/users").await
    }

    /// `GET /users/{user_id}` — Get details for a specific user.
    pub async fn get_user(&self, user_id: &str) -> Result<Value, ClientError> {
        self.get(&format!("/users/{}", user_id)).await
    }

    // ========================================================================
    // Event Stream (/events/stream)
    // ========================================================================

    /// `GET /events/stream` — Subscribe to SSE event stream.
    ///
    /// Returns the raw response for the caller to process as an SSE stream.
    /// This is a long-lived connection — the caller should read events in a loop.
    pub async fn watch_events_raw(&self) -> Result<reqwest::Response, ClientError> {
        let url = format!("{}/events/stream", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status().as_u16();
        if status >= 400 {
            let body = resp.text().await.unwrap_or_default();
            return Err(ClientError::Server { status, body });
        }
        Ok(resp)
    }
}
