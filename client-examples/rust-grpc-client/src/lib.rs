//! # ok-FPGA-Server gRPC Client Library
//!
//! This module provides typed Rust client wrappers for all 4 gRPC services
//! defined in the [proto3 spec](../../proto/okfpga/v1/okfpga.proto):
//!
//! - `DeviceService` — device discovery, sensors, settings, configuration
//! - `ClassicDataPortService` — wires, triggers, pipes, registers
//! - `AXIDataPortService` — AXI lite, full, stream operations
//! - `ServerService` — server status, service lifecycle
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rust_grpc_client::OkFpgaGrpcClient;
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut client = OkFpgaGrpcClient::connect("http://localhost:50051").await.unwrap();
//!     let status = client.get_server_status().await.unwrap();
//!     println!("Server: {:?}", status);
//! }
//! ```

pub mod discovery;
pub mod mqtt;

// Include the generated protobuf/gRPC code.
// tonic-build generates this in $OUT_DIR during `cargo build`.
pub mod okfpga {
    pub mod v1 {
        tonic::include_proto!("ok_fpga_service.v1");
    }
}

use tonic::transport::Channel;

use okfpga::v1::{
    device_service_client::DeviceServiceClient,
    classic_data_port_service_client::ClassicDataPortServiceClient,
    axi_data_port_service_client::AxiDataPortServiceClient,
    server_service_client::ServerServiceClient,
};

/// Combined gRPC client wrapping all 4 ok-FPGA-Server services.
///
/// Each service client shares the same gRPC channel (connection).
/// Methods are grouped by service for clarity.
pub struct OkFpgaGrpcClient {
    /// Client for device operations (discovery, sensors, config, resources, etc.)
    pub device: DeviceServiceClient<Channel>,
    /// Client for classic data port operations (wires, triggers, pipes, registers)
    pub classic: ClassicDataPortServiceClient<Channel>,
    /// Client for AXI data port operations (lite, full, stream)
    pub axi: AxiDataPortServiceClient<Channel>,
    /// Client for server management (status, services, restart)
    pub server: ServerServiceClient<Channel>,
}

impl OkFpgaGrpcClient {
    /// Connect to an ok-FPGA-Server gRPC endpoint.
    ///
    /// # Arguments
    /// * `addr` — The gRPC server address (e.g., "http://localhost:50051")
    pub async fn connect(addr: &str) -> Result<Self, tonic::transport::Error> {
        // Create a shared channel — all service clients will multiplex over it.
        let channel = Channel::from_shared(addr.to_string())
            .expect("valid URI")
            .connect()
            .await?;

        Ok(Self {
            device: DeviceServiceClient::new(channel.clone()),
            classic: ClassicDataPortServiceClient::new(channel.clone()),
            axi: AxiDataPortServiceClient::new(channel.clone()),
            server: ServerServiceClient::new(channel),
        })
    }

    // ========================================================================
    // DeviceService RPCs
    // ========================================================================

    /// List all connected FPGA devices.
    pub async fn list_devices(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListDevicesResponse>, tonic::Status> {
        self.device
            .list_devices(okfpga::v1::ListDevicesRequest {})
            .await
    }

    /// Get detailed info for a specific device.
    pub async fn get_device_info(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceInfoResponse>, tonic::Status> {
        self.device
            .get_device_info(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    /// Get all device settings (key-value pairs from device flash/EEPROM).
    pub async fn get_device_settings(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceSettingsResponse>, tonic::Status> {
        self.device
            .get_device_settings(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    /// Get a single device setting by key.
    pub async fn get_device_setting(
        &mut self,
        serial: &str,
        key: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceSettingResponse>, tonic::Status> {
        self.device
            .get_device_setting(okfpga::v1::GetDeviceSettingRequest {
                serial: serial.to_string(),
                key: key.to_string(),
            })
            .await
    }

    /// Discover SYZYGY peripheral ports on a device.
    pub async fn get_syzygy_ports(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::SyzygyPortsResponse>, tonic::Status> {
        self.device
            .get_syzygy_ports(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    /// Get device sensor readings.
    pub async fn get_device_sensors(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceSensorsResponse>, tonic::Status> {
        self.device
            .get_device_sensors(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    /// Get server status via DeviceService.
    pub async fn get_server_status_device(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ServerStatus>, tonic::Status> {
        self.device
            .get_server_status(okfpga::v1::ServerStatusRequest {})
            .await
    }

    // ========================================================================
    // ServerService RPCs
    // ========================================================================

    /// Get server status via ServerService.
    pub async fn get_server_status(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ServerStatusResponse>, tonic::Status> {
        self.server
            .get_server_status(okfpga::v1::StatusResponse { error: None })
            .await
    }

    /// List all manageable services.
    pub async fn list_services(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListServicesResponse>, tonic::Status> {
        self.server
            .list_services(okfpga::v1::StatusResponse { error: None })
            .await
    }

    // ========================================================================
    // ClassicDataPortService RPCs (stub wrappers)
    // ========================================================================

    /// Set a WireIn value.
    pub async fn set_wire_in_value(
        &mut self,
        serial: &str,
        endpoint: i32,
        value: u32,
        mask: u32,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .set_wire_in_value(okfpga::v1::SetWireInValueRequest {
                serial: serial.to_string(),
                endpoint,
                value,
                mask,
                auto_update: false,
            })
            .await
    }

    /// Update WireIns (transfer pending values to FPGA).
    pub async fn update_wire_ins(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .update_wire_ins(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    /// Activate a TriggerIn.
    pub async fn activate_trigger_in(
        &mut self,
        serial: &str,
        endpoint: i32,
        bit: i32,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .activate_trigger_in(okfpga::v1::ActivateTriggerInRequest {
                serial: serial.to_string(),
                endpoint,
                bit,
            })
            .await
    }

    /// Get WireOut value (calls UpdateWireOuts + GetWireOutValue).
    pub async fn get_wire_out_value(
        &mut self,
        serial: &str,
        endpoint: i32,
    ) -> Result<tonic::Response<okfpga::v1::WireValueResponse>, tonic::Status> {
        self.classic
            .get_wire_out_value(okfpga::v1::GetWireValueRequest {
                serial: serial.to_string(),
                endpoint,
            })
            .await
    }

    /// Update WireOuts (read all wire-out values from FPGA).
    pub async fn update_wire_outs(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .update_wire_outs(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    /// Get TriggerOut vector for an endpoint.
    pub async fn get_trigger_out_vector(
        &mut self,
        serial: &str,
        endpoint: i32,
    ) -> Result<tonic::Response<okfpga::v1::TriggerOutVectorResponse>, tonic::Status> {
        self.classic
            .get_trigger_out_vector(okfpga::v1::TriggerOutVectorRequest {
                serial: serial.to_string(),
                endpoint,
            })
            .await
    }

    /// Read a single register.
    pub async fn read_register(
        &mut self,
        serial: &str,
        address: u32,
    ) -> Result<tonic::Response<okfpga::v1::RegisterValueResponse>, tonic::Status> {
        self.classic
            .read_register(okfpga::v1::ReadRegisterRequest {
                serial: serial.to_string(),
                address,
            })
            .await
    }

    /// Write a single register.
    pub async fn write_register(
        &mut self,
        serial: &str,
        address: u32,
        data: u32,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .write_register(okfpga::v1::WriteRegisterRequest {
                serial: serial.to_string(),
                address,
                data,
            })
            .await
    }

    /// Reset the FPGA via Classic data port.
    pub async fn reset_fpga(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .reset_fpga(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            })
            .await
    }

    // ========================================================================
    // AXIDataPortService RPCs (stub wrappers)
    // ========================================================================

    /// AXI Lite read.
    pub async fn axi_lite_read(
        &mut self,
        serial: &str,
        address: u64,
    ) -> Result<tonic::Response<okfpga::v1::AxiLiteReadResponse>, tonic::Status> {
        self.axi
            .axi_lite_read(okfpga::v1::AxiLiteReadRequest {
                serial: serial.to_string(),
                address,
                timeout_ms: None,
            })
            .await
    }

    /// AXI Lite write.
    pub async fn axi_lite_write(
        &mut self,
        serial: &str,
        address: u64,
        value: u32,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.axi
            .axi_lite_write(okfpga::v1::AxiLiteWriteRequest {
                serial: serial.to_string(),
                address,
                value,
                timeout_ms: None,
            })
            .await
    }

    // ========================================================================
    // Resource Management (ADR-020)
    // ========================================================================

    /// Create a new resource for a programmed FPGA.
    pub async fn create_resource(
        &mut self,
        name: &str,
        device_serial: &str,
        description: &str,
        access_mode: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        self.device
            .create_resource(okfpga::v1::CreateResourceRequest {
                name: name.to_string(),
                device_serial: device_serial.to_string(),
                description: description.to_string(),
                resource_type: access_mode.to_string(),
                capabilities: vec![],
            })
            .await
    }

    /// List all resources.
    pub async fn list_resources(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListResourcesResponse>, tonic::Status> {
        self.device
            .list_resources(okfpga::v1::ListResourcesRequest {})
            .await
    }

    /// Delete a resource by ID.
    pub async fn delete_resource(
        &mut self,
        resource_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.device
            .delete_resource(okfpga::v1::ResourceRequest {
                resource_id: resource_id.to_string(),
            })
            .await
    }

    /// Lock a shared resource (ADR-020 §3).
    pub async fn lock_resource(
        &mut self,
        resource_id: &str,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        self.device
            .lock_resource(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: user_id.to_string(),
            })
            .await
    }

    /// Unlock a locked resource (ADR-020 §3).
    pub async fn unlock_resource(
        &mut self,
        resource_id: &str,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        self.device
            .unlock_resource(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: user_id.to_string(),
            })
            .await
    }

    // ========================================================================
    // Heartbeat / Liveness (ADR-020 §5)
    // ========================================================================

    /// Ping the server to keep a user session alive.
    pub async fn ping(
        &mut self,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::PingResponse>, tonic::Status> {
        self.device
            .ping(okfpga::v1::PingRequest {
                user_id: user_id.to_string(),
            })
            .await
    }

    // ========================================================================
    // User Sessions
    // ========================================================================

    /// List all active user sessions.
    pub async fn list_users(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListUsersResponse>, tonic::Status> {
        self.device
            .list_users(okfpga::v1::ListUsersRequest {})
            .await
    }
}
