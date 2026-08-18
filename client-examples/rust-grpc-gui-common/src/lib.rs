//! # ok-FPGA-Server gRPC GUI Common Library
//!
//! Shared gRPC API client for the Rust gRPC GUI demos (egui and iced).
//! Mirrors the REST common library but uses tonic/protobuf.

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
        })
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    // ========================================================================
    // Server & Device
    // ========================================================================

    pub async fn get_server_status(&mut self) -> Result<(), ClientError> {
        // DeviceService has GetServerStatus that returns ServerStatus.
        self.device
            .get_server_status(okfpga::v1::ServerStatusRequest {})
            .await?;
        Ok(())
    }

    pub async fn list_devices(&mut self) -> Result<Vec<DeviceInfo>, ClientError> {
        let resp = self
            .device
            .list_devices(okfpga::v1::ListDevicesRequest {})
            .await?;
        let devices = resp
            .into_inner()
            .devices
            .into_iter()
            .map(|d| DeviceInfo {
                serial: d.serial,
                product_name: d.product_name,
            })
            .collect();
        Ok(devices)
    }

    pub async fn configure_fpga(&mut self, serial: &str, bitfile: &str) -> Result<(), ClientError> {
        self.device
            .configure_fpga(okfpga::v1::ConfigureFpgaRequest {
                serial: serial.to_string(),
                bitfile_path: bitfile.to_string(),
                user_id: self.user_id.clone(),
            })
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
            .set_wire_in_value(okfpga::v1::SetWireInValueRequest {
                serial: serial.to_string(),
                endpoint,
                value,
                mask,
                auto_update: false,
            })
            .await?;
        Ok(())
    }

    pub async fn update_wire_ins(&mut self, serial: &str) -> Result<(), ClientError> {
        self.classic
            .update_wire_ins(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn update_wire_outs(&mut self, serial: &str) -> Result<(), ClientError> {
        self.classic
            .update_wire_outs(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn get_wire_out_value(&mut self, serial: &str, endpoint: i32) -> Result<u32, ClientError> {
        let resp = self
            .classic
            .get_wire_out_value(okfpga::v1::GetWireValueRequest {
                serial: serial.to_string(),
                endpoint,
            })
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
            .activate_trigger_in(okfpga::v1::ActivateTriggerInRequest {
                serial: serial.to_string(),
                endpoint,
                bit,
            })
            .await?;
        Ok(())
    }

    pub async fn get_trigger_out_vector(&mut self, serial: &str, endpoint: i32) -> Result<u32, ClientError> {
        let resp = self
            .classic
            .get_trigger_out_vector(okfpga::v1::TriggerOutVectorRequest {
                serial: serial.to_string(),
                endpoint,
            })
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
                locked_by: None, // Not in proto; would need extension.
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

    pub async fn create_resource(&mut self, name: &str, device_serial: &str) -> Result<ResourceInfo, ClientError> {
        let resp = self
            .device
            .create_resource(okfpga::v1::CreateResourceRequest {
                name: name.to_string(),
                device_serial: device_serial.to_string(),
                description: "Counters.bit hardware test resource".to_string(),
                resource_type: "shared".to_string(),
                capabilities: vec![],
            })
            .await?;
        Ok(Self::resource_from_response(resp.into_inner()))
    }

    pub async fn acquire_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .acquire_resource(okfpga::v1::AcquireResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
                mode: 0, // ACCESS_MODE_SHARED
            })
            .await?;
        Ok(())
    }

    pub async fn release_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .release_resource(okfpga::v1::ReleaseResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
            })
            .await?;
        Ok(())
    }

    pub async fn get_resource(&mut self, resource_id: &str) -> Result<ResourceInfo, ClientError> {
        let resp = self
            .device
            .get_resource(okfpga::v1::ResourceRequest {
                resource_id: resource_id.to_string(),
            })
            .await?;
        Ok(Self::resource_from_response(resp.into_inner()))
    }

    pub async fn lock_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .lock_resource(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
            })
            .await?;
        Ok(())
    }

    pub async fn unlock_resource(&mut self, resource_id: &str) -> Result<(), ClientError> {
        self.device
            .unlock_resource(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: self.user_id.clone(),
            })
            .await?;
        Ok(())
    }

    pub async fn heartbeat(&mut self) -> Result<(), ClientError> {
        self.device
            .ping(okfpga::v1::PingRequest {
                user_id: self.user_id.clone(),
            })
            .await?;
        Ok(())
    }
}
