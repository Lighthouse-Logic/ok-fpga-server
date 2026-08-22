//! # ok-FPGA-Server gRPC GUI Common Library
//!
//! Shared gRPC API client for the Rust gRPC GUI demos (egui and iced).
//! Mirrors the REST common library but uses tonic/protobuf.

pub mod mdns_browser;

pub mod okfpga {
    pub mod v1 {
        tonic::include_proto!("ok_fpga_service.v1");
    }
}

use okfpga::v1::{
    classic_data_port_service_client::ClassicDataPortServiceClient,
    device_service_client::DeviceServiceClient,
    server_service_client::ServerServiceClient,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tonic::transport::Channel;

// ============================================================================
// Error Type
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("gRPC status error: {0}")]
    Status(#[from] tonic::Status),
}

// ============================================================================
// Data Models (same interface as REST common)
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CounterState {
    pub count1: u8,
    pub count2: u8,
    pub buttons: u8,
    pub count1_eq_00: bool,
    pub count1_eq_80: bool,
    pub count2_eq_ff: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WireInState {
    pub reset1: bool,
    pub disable1: bool,
    pub autocount2: bool,
}

impl WireInState {
    pub fn to_value(&self) -> u32 {
        let mut val: u32 = 0;
        if self.reset1 { val |= 1; }
        if self.disable1 { val |= 2; }
        if self.autocount2 { val |= 4; }
        val
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub product_name: String,
    /// Phase 34-B: SYZYGY label suffix (e.g. " | 0: DAC3283 | 2: ADC3241")
    pub syzygy_label: String,
}

impl DeviceInfo {
    /// Build a user-friendly display label: "XEM7320-A75T (SERIAL) | 0: DAC | 2: ADC"
    pub fn display_label(&self) -> String {
        if self.syzygy_label.is_empty() {
            format!("{} ({})", self.product_name, self.serial)
        } else {
            format!("{} ({}) | {}", self.product_name, self.serial, self.syzygy_label)
        }
    }
}

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
// gRPC Client
// ============================================================================

/// gRPC API client for the Counters.bit interactive demo.
pub struct CountersGrpcClient {
    device: DeviceServiceClient<Channel>,
    classic: ClassicDataPortServiceClient<Channel>,
    #[allow(dead_code)]
    server: ServerServiceClient<Channel>,
    user_id: String,
    /// Phase 34-B: Cache SYZYGY labels per serial to avoid re-fetching on every refresh.
    syzygy_cache: HashMap<String, String>,
}

impl CountersGrpcClient {
    /// Connect to the gRPC server.
    pub async fn connect(addr: &str, user_id: &str) -> Result<Self, ClientError> {
        let channel = Channel::from_shared(addr.to_string())
            .expect("valid URI")
            .connect()
            .await?;

        Ok(Self {
            device: DeviceServiceClient::new(channel.clone()),
            classic: ClassicDataPortServiceClient::new(channel.clone()),
            server: ServerServiceClient::new(channel),
            user_id: user_id.to_string(),
            syzygy_cache: HashMap::new(),
        })
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Wrap a protobuf message in a `tonic::Request` with `x-user-id` metadata.
    ///
    /// All gRPC calls should use this instead of passing bare messages.
    /// This is the gRPC equivalent of the REST client's `X-User-Id` header,
    /// required when the server's endpoint enforcement is active (Phase 32).
    fn req<T>(&self, msg: T) -> tonic::Request<T> {
        let mut request = tonic::Request::new(msg);
        request.metadata_mut().insert(
            "x-user-id",
            self.user_id.parse().expect("valid user_id"),
        );
        request
    }

    // ========================================================================
    // Server & Device
    // ========================================================================

    pub async fn get_server_status(&mut self) -> Result<(), ClientError> {
        // DeviceService has GetServerStatus that returns ServerStatus.
        self.device
            .get_server_status(self.req(okfpga::v1::ServerStatusRequest {}))
            .await?;
        Ok(())
    }

    pub async fn list_devices(&mut self) -> Result<Vec<DeviceInfo>, ClientError> {
        let resp = self
            .device
            .list_devices(self.req(okfpga::v1::ListDevicesRequest {}))
            .await?;
        let mut devices: Vec<DeviceInfo> = resp
            .into_inner()
            .devices
            .into_iter()
            .map(|d| DeviceInfo {
                serial: d.serial,
                product_name: d.product_name,
                syzygy_label: String::new(),
            })
            .collect();
        // Phase 34-B: Populate SYZYGY labels (cached — only fetches once per serial).
        for dev in &mut devices {
            if let Some(cached) = self.syzygy_cache.get(&dev.serial) {
                dev.syzygy_label = cached.clone();
            } else if let Ok(label) = self.get_syzygy_label(&dev.serial).await {
                self.syzygy_cache.insert(dev.serial.clone(), label.clone());
                dev.syzygy_label = label;
            }
        }
        Ok(devices)
    }

    /// Phase 34-B: Fetch SYZYGY ports and build a compact label string.
    async fn get_syzygy_label(&mut self, serial: &str) -> Result<String, ClientError> {
        let resp = self
            .device
            .get_syzygy_ports(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await?;
        let ports = resp.into_inner().ports;
        let parts: Vec<String> = ports
            .iter()
            .filter(|p| p.is_populated)
            .map(|p| {
                let name = &p.product_name;
                let model = &p.product_model;
                if model.is_empty() || model == name {
                    format!("{}:{}", p.index, name)
                } else {
                    format!("{}:{} ({})", p.index, name, model)
                }
            })
            .collect();
        Ok(parts.join(" | "))
    }

    /// `GetDeviceInfo` RPC — get full device info for a single device.
    ///
    /// Returns the response as a `serde_json::Value` for flexibility.
    /// Used by F4 smart-skip to check if a device is already programmed.
    pub async fn get_device_info(&mut self, serial: &str) -> Result<serde_json::Value, ClientError> {
        let resp = self
            .device
            .get_device_info(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await?;
        let inner = resp.into_inner();
        // Convert to JSON Value for compatibility with the REST client API.
        // Derive "state" from whether a bitfile is loaded (DeviceInfo.bitfile
        // is an optional field — present means programmed).
        let device = inner.device.unwrap_or_default();
        let state = if device.bitfile.is_some() { "Programmed" } else { "NotProgrammed" };
        let json = serde_json::json!({
            "serial": device.serial,
            "product_name": device.product_name,
            "state": state,
        });
        Ok(json)
    }

    /// `ListResources` RPC — list all resources across all devices.
    ///
    /// Used by F3 (409 conflict recovery) and F4 (smart-skip) to look up
    /// existing resources on a device.
    pub async fn list_resources(&mut self) -> Result<Vec<ResourceInfo>, ClientError> {
        let resp = self
            .device
            .list_resources(self.req(okfpga::v1::ListResourcesRequest {}))
            .await?;
        let resources = resp
            .into_inner()
            .resources
            .into_iter()
            .map(|r| ResourceInfo {
                id: r.id,
                name: r.name,
                device_serial: r.device_serial,
                access_mode: "shared".to_string(),
                holders: r.holders,
                locked_by: None,
            })
            .collect();
        Ok(resources)
    }

    /// Configure FPGA from a server-side bitfile path.
    ///
    /// Sends `ConfigureFPGA` RPC with the path. The bitfile must exist on
    /// the server's filesystem.
    pub async fn configure_fpga(&mut self, serial: &str, bitfile: &str) -> Result<(), ClientError> {
        self.device
            .configure_fpga(self.req(okfpga::v1::ConfigureFpgaRequest {
                serial: serial.to_string(),
                bitfile_path: bitfile.to_string(),
                user_id: self.user_id.clone(),
            }))
            .await?;
        Ok(())
    }

    /// Configure FPGA from a local file (client-side streaming).
    ///
    /// Reads the bitfile from the client's local filesystem and streams it
    /// to the server using `ConfigureFPGAFromMemory` RPC. The bitfile is
    /// sent as a single chunk for simplicity.
    pub async fn configure_fpga_from_file(
        &mut self, serial: &str, local_path: &std::path::Path,
    ) -> Result<(), ClientError> {
        let data = tokio::fs::read(local_path).await.map_err(|e| {
            ClientError::Status(tonic::Status::internal(format!(
                "failed to read local file '{}': {}", local_path.display(), e
            )))
        })?;
        // Send as a single chunk with is_last=true.
        let chunk = okfpga::v1::ConfigureFpgaChunk {
            serial: serial.to_string(),
            data,
            is_last: true,
            user_id: self.user_id.clone(),
            // Phase 34-C: Send the original filename to the server.
            filename: local_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        };
        // Wrap the stream in a Request with x-user-id metadata (Phase 32).
        let mut request = tonic::Request::new(tokio_stream::once(chunk));
        request.metadata_mut().insert(
            "x-user-id",
            self.user_id.parse().expect("valid user_id"),
        );
        self.device
            .configure_fpga_from_memory(request)
            .await?;
        Ok(())
    }

    // ========================================================================
    // Classic Data Port
    // ========================================================================

    pub async fn set_wire_in(
        &mut self,
        serial: &str,
        endpoint: i32,
        value: u32,
        mask: u32,
    ) -> Result<(), ClientError> {
        self.classic
            .set_wire_in_value(self.req(okfpga::v1::SetWireInValueRequest {
                serial: serial.to_string(),
                endpoint,
                value,
                mask,
                auto_update: false,
            }))
            .await?;
        Ok(())
    }

    pub async fn update_wire_ins(&mut self, serial: &str) -> Result<(), ClientError> {
        self.classic
            .update_wire_ins(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await?;
        Ok(())
    }

    pub async fn update_wire_outs(&mut self, serial: &str) -> Result<(), ClientError> {
        self.classic
            .update_wire_outs(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await?;
        Ok(())
    }

    pub async fn get_wire_out_value(&mut self, serial: &str, endpoint: i32) -> Result<u32, ClientError> {
        let resp = self
            .classic
            .get_wire_out_value(self.req(okfpga::v1::GetWireValueRequest {
                serial: serial.to_string(),
                endpoint,
            }))
            .await?;
        Ok(resp.into_inner().value)
    }

    pub async fn activate_trigger_in(
        &mut self,
        serial: &str,
        endpoint: i32,
        bit: i32,
    ) -> Result<(), ClientError> {
        self.classic
            .activate_trigger_in(self.req(okfpga::v1::ActivateTriggerInRequest {
                serial: serial.to_string(),
                endpoint,
                bit,
            }))
            .await?;
        Ok(())
    }

    pub async fn get_trigger_out_vector(&mut self, serial: &str, endpoint: i32) -> Result<u32, ClientError> {
        let resp = self
            .classic
            .get_trigger_out_vector(self.req(okfpga::v1::TriggerOutVectorRequest {
                serial: serial.to_string(),
                endpoint,
            }))
            .await?;
        // Field is `vector` in the proto, not `value`.
        Ok(resp.into_inner().vector)
    }

    // ========================================================================
    // Combined Poll
    // ========================================================================

    pub async fn poll_counter_state(&mut self, serial: &str) -> Result<CounterState, ClientError> {
        self.update_wire_outs(serial).await?;
        let count1 = self.get_wire_out_value(serial, 0x20).await? as u8;
        let count2 = self.get_wire_out_value(serial, 0x21).await? as u8;
        let buttons = self.get_wire_out_value(serial, 0x22).await? as u8;
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

    pub async fn send_wire_in(&mut self, serial: &str, state: &WireInState) -> Result<(), ClientError> {
        self.set_wire_in(serial, 0x00, state.to_value(), 0x07).await?;
        self.update_wire_ins(serial).await?;
        Ok(())
    }

    // ========================================================================
    // Resource Management
    // ========================================================================

    /// Helper to extract ResourceInfo from a ResourceResponse.
    fn resource_from_response(resp: okfpga::v1::ResourceResponse) -> ResourceInfo {
        if let Some(r) = resp.resource {
            ResourceInfo {
                id: r.id,
                name: r.name,
                device_serial: r.device_serial,
                access_mode: "shared".to_string(), // Not in proto; inferred.
                holders: r.holders,
                // Phase 34-A: Read locked_by from proto (empty string → None).
                locked_by: if r.locked_by.is_empty() { None } else { Some(r.locked_by) },
            }
        } else {
            ResourceInfo {
                id: String::new(),
                name: String::new(),
                device_serial: String::new(),
                access_mode: String::new(),
                holders: vec![],
                locked_by: None,
            }
        }
    }

    /// Phase 34-E: `resource_type` parameter allows the caller to choose
    /// "shared" or "exclusive" when creating a resource.
    pub async fn create_resource(&mut self, name: &str, device_serial: &str, resource_type: &str) -> Result<ResourceInfo, ClientError> {
        let resp = self
            .device
            .create_resource(self.req(okfpga::v1::CreateResourceRequest {
                name: name.to_string(),
                device_serial: device_serial.to_string(),
                description: "Counters.bit hardware test resource".to_string(),
                resource_type: resource_type.to_string(),
                capabilities: vec![],
            }))
            .await?;
        Ok(Self::resource_from_response(resp.into_inner()))
    }

    pub async fn acquire_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .acquire_resource(self.req(okfpga::v1::AcquireResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
                mode: 0, // ACCESS_MODE_SHARED
            }))
            .await?;
        Ok(())
    }

    /// Acquire the resource in read-only mode (ADR-026).
    ///
    /// ReadOnly holders can observe output endpoints and address ranges but
    /// cannot perform write operations or state-disturbing calls.
    pub async fn acquire_resource_readonly(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .acquire_resource(self.req(okfpga::v1::AcquireResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
                mode: 2, // ACCESS_MODE_READ_ONLY
            }))
            .await?;
        Ok(())
    }

    pub async fn release_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .release_resource(self.req(okfpga::v1::ReleaseResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
            }))
            .await?;
        Ok(())
    }

    pub async fn get_resource(&mut self, resource_id: &str) -> Result<ResourceInfo, ClientError> {
        let resp = self
            .device
            .get_resource(self.req(okfpga::v1::ResourceRequest {
                resource_id: resource_id.to_string(),
            }))
            .await?;
        Ok(Self::resource_from_response(resp.into_inner()))
    }

    pub async fn lock_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .lock_resource(self.req(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
            }))
            .await?;
        Ok(())
    }

    pub async fn unlock_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .unlock_resource(self.req(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
            }))
            .await?;
        Ok(())
    }

    pub async fn heartbeat(&mut self) -> Result<(), ClientError> {
        self.device
            .ping(self.req(okfpga::v1::PingRequest {
                user_id: self.user_id.clone(),
            }))
            .await?;
        Ok(())
    }

    /// Cleanly disconnect this user from the server (Phase 25-Fix-H, F10).
    ///
    /// Releases all resources held by this user on any device, removes
    /// the user session, and stops heartbeat tracking. This is the
    /// recommended way to end a client session.
    pub async fn disconnect(&mut self) -> Result<(), ClientError> {
        self.device
            .disconnect(self.req(okfpga::v1::DisconnectRequest {
                user_id: self.user_id.clone(),
            }))
            .await?;
        Ok(())
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
    fn test_wire_in_state_combinations() {
        // reset1 + autocount2 = 0x05
        let state = WireInState { reset1: true, disable1: false, autocount2: true };
        assert_eq!(state.to_value(), 0x05);
        // disable1 + autocount2 = 0x06
        let state = WireInState { reset1: false, disable1: true, autocount2: true };
        assert_eq!(state.to_value(), 0x06);
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

    #[test]
    fn test_counter_state_trigger_booleans() {
        let state = CounterState {
            count1: 0, count2: 0xFF, buttons: 0,
            count1_eq_00: true, count1_eq_80: false, count2_eq_ff: true,
        };
        assert!(state.count1_eq_00);
        assert!(!state.count1_eq_80);
        assert!(state.count2_eq_ff);
    }

    // ========================================================================
    // DeviceInfo construction tests
    // ========================================================================

    #[test]
    fn test_device_info_construction() {
        let info = DeviceInfo {
            serial: "SN001".to_string(),
            product_name: "XEM7320-A75T".to_string(),
            syzygy_label: String::new(),
        };
        assert_eq!(info.serial, "SN001");
        assert_eq!(info.product_name, "XEM7320-A75T");
    }

    // ========================================================================
    // ResourceInfo tests
    // ========================================================================

    #[test]
    fn test_resource_info_locked_by_none() {
        let info = ResourceInfo {
            id: "res-1".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "SN001".to_string(),
            access_mode: "shared".to_string(),
            holders: vec!["user1".to_string()],
            locked_by: None,
        };
        assert!(info.locked_by.is_none());
        assert_eq!(info.holders.len(), 1);
        assert_eq!(info.holders[0], "user1");
    }

    #[test]
    fn test_resource_info_locked_by_present() {
        let info = ResourceInfo {
            id: "res-1".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "SN001".to_string(),
            access_mode: "shared".to_string(),
            holders: vec![],
            locked_by: Some("Icy Garpac".to_string()),
        };
        assert_eq!(info.locked_by.as_deref(), Some("Icy Garpac"));
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
    // resource_from_response helper tests
    // ========================================================================

    #[test]
    fn test_resource_from_response_with_resource() {
        let proto_resource = okfpga::v1::Resource {
            id: "res-abc".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "SN001".to_string(),
            holders: vec!["user1".to_string(), "user2".to_string()],
            ..Default::default()
        };
        let resp = okfpga::v1::ResourceResponse {
            resource: Some(proto_resource),
            error: None,
        };
        let info = CountersGrpcClient::resource_from_response(resp);
        assert_eq!(info.id, "res-abc");
        assert_eq!(info.name, "Counter Resource");
        assert_eq!(info.device_serial, "SN001");
        assert_eq!(info.access_mode, "shared");
        assert_eq!(info.holders.len(), 2);
        assert!(info.locked_by.is_none());
    }

    #[test]
    fn test_resource_from_response_without_resource() {
        let resp = okfpga::v1::ResourceResponse { resource: None, error: None };
        let info = CountersGrpcClient::resource_from_response(resp);
        assert!(info.id.is_empty());
        assert!(info.name.is_empty());
        assert!(info.holders.is_empty());
    }

    // ========================================================================
    // Phase 25-Fix-F: F3/F4/F5 tests
    // ========================================================================

    #[test]
    fn test_resource_lookup_by_device_serial() {
        // Simulates the F3/F4 lookup pattern: find resource by device_serial.
        let resources = vec![
            ResourceInfo {
                id: "res-1".to_string(),
                name: "Counter".to_string(),
                device_serial: "SN001".to_string(),
                access_mode: "shared".to_string(),
                holders: vec![],
                locked_by: None,
            },
            ResourceInfo {
                id: "res-2".to_string(),
                name: "Other".to_string(),
                device_serial: "SN002".to_string(),
                access_mode: "shared".to_string(),
                holders: vec![],
                locked_by: None,
            },
        ];
        // F3/F4: Lookup by serial.
        let existing = resources.iter().find(|r| r.device_serial == "SN001");
        assert!(existing.is_some());
        assert_eq!(existing.unwrap().id, "res-1");
        // Missing serial returns None.
        let missing = resources.iter().find(|r| r.device_serial == "SN999");
        assert!(missing.is_none());
    }

    #[test]
    fn test_resource_info_from_proto_resource_matches_list_resources_pattern() {
        // Verify the conversion pattern used in list_resources() produces
        // correct ResourceInfo from a proto Resource message.
        let proto_resource = okfpga::v1::Resource {
            id: "res-abc".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "SN001".to_string(),
            holders: vec!["user1".to_string()],
            ..Default::default()
        };
        // Simulate the conversion from list_resources().
        let info = ResourceInfo {
            id: proto_resource.id.clone(),
            name: proto_resource.name.clone(),
            device_serial: proto_resource.device_serial.clone(),
            access_mode: "shared".to_string(),
            holders: proto_resource.holders.clone(),
            locked_by: None,
        };
        assert_eq!(info.id, "res-abc");
        assert_eq!(info.device_serial, "SN001");
        assert_eq!(info.holders, vec!["user1"]);
    }
}
