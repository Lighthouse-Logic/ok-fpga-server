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
    /// User identity attached via `x-user-id` gRPC metadata on every RPC (S-18, ADR-029).
    user_id: String,
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
            user_id: "rust-grpc-client".to_string(),
        })
    }

    /// Override the `x-user-id` caller identity sent with gRPC requests.
    pub fn with_user_id(mut self, user_id: &str) -> Self {
        self.user_id = user_id.to_string();
        self
    }

    /// Return the configured caller identity (`x-user-id`).
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Attach `x-user-id` metadata to an outgoing gRPC request.
    fn req<T>(&self, msg: T) -> tonic::Request<T> {
        let mut r = tonic::Request::new(msg);
        if let Ok(val) = tonic::metadata::MetadataValue::try_from(self.user_id.as_str()) {
            r.metadata_mut().insert("x-user-id", val);
        }
        r
    }

    // ========================================================================
    // DeviceService RPCs
    // ========================================================================

    /// List all connected FPGA devices.
    pub async fn list_devices(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListDevicesResponse>, tonic::Status> {
        self.device
            .list_devices(self.req(okfpga::v1::ListDevicesRequest {}))
            .await
    }

    /// Get detailed info for a specific device.
    pub async fn get_device_info(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceInfoResponse>, tonic::Status> {
        self.device
            .get_device_info(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Get all device settings (key-value pairs from device flash/EEPROM).
    pub async fn get_device_settings(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceSettingsResponse>, tonic::Status> {
        self.device
            .get_device_settings(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Get a single device setting by key.
    pub async fn get_device_setting(
        &mut self,
        serial: &str,
        key: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceSettingResponse>, tonic::Status> {
        self.device
            .get_device_setting(self.req(okfpga::v1::GetDeviceSettingRequest {
                serial: serial.to_string(),
                key: key.to_string(),
            }))
            .await
    }

    /// Discover SYZYGY peripheral ports on a device.
    pub async fn get_syzygy_ports(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::SyzygyPortsResponse>, tonic::Status> {
        self.device
            .get_syzygy_ports(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Get device sensor readings.
    pub async fn get_device_sensors(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DeviceSensorsResponse>, tonic::Status> {
        self.device
            .get_device_sensors(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Program the FPGA from a bitfile path on the server's filesystem.
    pub async fn configure_fpga(
        &mut self,
        serial: &str,
        bitfile_path: &str,
    ) -> Result<tonic::Response<okfpga::v1::ConfigureFpgaResponse>, tonic::Status> {
        self.device
            .configure_fpga(self.req(okfpga::v1::ConfigureFpgaRequest {
                serial: serial.to_string(),
                bitfile_path: bitfile_path.to_string(),
                user_id: self.user_id.clone(),
            }))
            .await
    }

    /// Clear the current FPGA configuration.
    pub async fn clear_fpga_configuration(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.device
            .clear_fpga_configuration(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Get the active data port type for a device.
    pub async fn get_dataport_type(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::DataportTypeResponse>, tonic::Status> {
        self.device
            .get_dataport_type(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Get server status via DeviceService.
    pub async fn get_server_status_device(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ServerStatus>, tonic::Status> {
        self.device
            .get_server_status(self.req(okfpga::v1::ServerStatusRequest {}))
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
            .get_server_status(self.req(okfpga::v1::StatusResponse { error: None }))
            .await
    }

    /// List all manageable services.
    pub async fn list_services(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListServicesResponse>, tonic::Status> {
        self.server
            .list_services(self.req(okfpga::v1::StatusResponse { error: None }))
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
            .set_wire_in_value(self.req(okfpga::v1::SetWireInValueRequest {
                serial: serial.to_string(),
                endpoint,
                value,
                mask,
                auto_update: false,
            }))
            .await
    }

    /// Update WireIns (transfer pending values to FPGA).
    pub async fn update_wire_ins(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .update_wire_ins(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
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
            .activate_trigger_in(self.req(okfpga::v1::ActivateTriggerInRequest {
                serial: serial.to_string(),
                endpoint,
                bit,
            }))
            .await
    }

    /// Get WireOut value (calls UpdateWireOuts + GetWireOutValue).
    pub async fn get_wire_out_value(
        &mut self,
        serial: &str,
        endpoint: i32,
    ) -> Result<tonic::Response<okfpga::v1::WireValueResponse>, tonic::Status> {
        self.classic
            .get_wire_out_value(self.req(okfpga::v1::GetWireValueRequest {
                serial: serial.to_string(),
                endpoint,
                update: Some(true),
            }))
            .await
    }

    /// Update WireOuts (read all wire-out values from FPGA).
    pub async fn update_wire_outs(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .update_wire_outs(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
            .await
    }

    /// Get TriggerOut vector for an endpoint.
    pub async fn get_trigger_out_vector(
        &mut self,
        serial: &str,
        endpoint: i32,
    ) -> Result<tonic::Response<okfpga::v1::TriggerOutVectorResponse>, tonic::Status> {
        self.classic
            .get_trigger_out_vector(self.req(okfpga::v1::TriggerOutVectorRequest {
                serial: serial.to_string(),
                endpoint,
                update: Some(true),
            }))
            .await
    }

    /// Read a single register.
    pub async fn read_register(
        &mut self,
        serial: &str,
        address: u32,
    ) -> Result<tonic::Response<okfpga::v1::RegisterValueResponse>, tonic::Status> {
        self.classic
            .read_register(self.req(okfpga::v1::ReadRegisterRequest {
                serial: serial.to_string(),
                address,
            }))
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
            .write_register(self.req(okfpga::v1::WriteRegisterRequest {
                serial: serial.to_string(),
                address,
                data,
            }))
            .await
    }

    /// Reset the FPGA via Classic data port.
    pub async fn reset_fpga(
        &mut self,
        serial: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.classic
            .reset_fpga(self.req(okfpga::v1::DeviceRequest {
                serial: serial.to_string(),
            }))
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
            .axi_lite_read(self.req(okfpga::v1::AxiLiteReadRequest {
                serial: serial.to_string(),
                address,
                timeout_ms: None,
            }))
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
            .axi_lite_write(self.req(okfpga::v1::AxiLiteWriteRequest {
                serial: serial.to_string(),
                address,
                value,
                timeout_ms: None,
            }))
            .await
    }

    // ========================================================================
    // Resource Management (ADR-020)
    // ========================================================================

    /// Create a new resource for a programmed FPGA (ADR-020 / ADR-034).
    ///
    /// When `access_mode == "shared"`, populates the standard `Counters.bit`
    /// endpoint and register declarations so `ADR-034 D68` validation succeeds.
    pub async fn create_resource(
        &mut self,
        name: &str,
        device_serial: &str,
        description: &str,
        access_mode: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        let shared_enum = okfpga::v1::ResourceAccessMode::Shared as i32;
        let mode_enum = match access_mode {
            "exclusive" => okfpga::v1::ResourceAccessMode::Exclusive as i32,
            _ => shared_enum,
        };
        let (endpoints, address_ranges) = if access_mode == "shared" {
            let ep = okfpga::v1::EndpointDeclaration {
                default_access: Some(shared_enum),
                entries: vec![
                    okfpga::v1::EndpointEntry {
                        address: 0x00,
                        r#type: "wire_in".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                    okfpga::v1::EndpointEntry {
                        address: 0x20,
                        r#type: "wire_out".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                    okfpga::v1::EndpointEntry {
                        address: 0x21,
                        r#type: "wire_out".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                    okfpga::v1::EndpointEntry {
                        address: 0x22,
                        r#type: "wire_out".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                    okfpga::v1::EndpointEntry {
                        address: 0x40,
                        r#type: "trigger_in".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                    okfpga::v1::EndpointEntry {
                        address: 0x60,
                        r#type: "trigger_out".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                    okfpga::v1::EndpointEntry {
                        address: 0x61,
                        r#type: "trigger_out".to_string(),
                        access: Some(shared_enum),
                        private: false,
                        description: String::new(),
                    },
                ],
            };
            let ranges = vec![okfpga::v1::AddressRange {
                name: "classic_registers".to_string(),
                start: 0,
                end: 255,
                access: Some(shared_enum),
                private: false,
                description: String::new(),
            }];
            (Some(ep), ranges)
        } else {
            (None, vec![])
        };

        self.device
            .create_resource(self.req(okfpga::v1::CreateResourceRequest {
                name: name.to_string(),
                device_serial: device_serial.to_string(),
                description: description.to_string(),
                access_mode: Some(mode_enum),
                interfaces: vec!["classic".to_string()],
                endpoints,
                address_ranges,
                metadata: std::collections::HashMap::new(),
                private: false,
            }))
            .await
    }

    /// Acquire a resource in `"shared"`, `"exclusive"`, or `"read_only"` mode.
    pub async fn acquire_resource(
        &mut self,
        resource_id: &str,
        mode: &str,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        let mode_enum = match mode {
            "exclusive" => okfpga::v1::AccessMode::Exclusive as i32,
            "read_only" => okfpga::v1::AccessMode::ReadOnly as i32,
            _ => okfpga::v1::AccessMode::Shared as i32,
        };
        self.device
            .acquire_resource(self.req(okfpga::v1::AcquireResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: user_id.to_string(),
                mode: Some(mode_enum),
            }))
            .await
    }

    /// Release a previously acquired resource.
    pub async fn release_resource(
        &mut self,
        resource_id: &str,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.device
            .release_resource(self.req(okfpga::v1::ReleaseResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: user_id.to_string(),
            }))
            .await
    }

    /// List all resources.
    pub async fn list_resources(
        &mut self,
    ) -> Result<tonic::Response<okfpga::v1::ListResourcesResponse>, tonic::Status> {
        self.device
            .list_resources(self.req(okfpga::v1::ListResourcesRequest {}))
            .await
    }

    /// Delete a resource by ID.
    pub async fn delete_resource(
        &mut self,
        resource_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::StatusResponse>, tonic::Status> {
        self.device
            .delete_resource(self.req(okfpga::v1::ResourceRequest {
                resource_id: resource_id.to_string(),
            }))
            .await
    }

    /// Lock a shared resource (ADR-020 §3).
    pub async fn lock_resource(
        &mut self,
        resource_id: &str,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        self.device
            .lock_resource(self.req(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: user_id.to_string(),
            }))
            .await
    }

    /// Unlock a locked resource (ADR-020 §3).
    pub async fn unlock_resource(
        &mut self,
        resource_id: &str,
        user_id: &str,
    ) -> Result<tonic::Response<okfpga::v1::ResourceResponse>, tonic::Status> {
        self.device
            .unlock_resource(self.req(okfpga::v1::LockResourceRequest {
                resource_id: resource_id.to_string(),
                user_id: user_id.to_string(),
            }))
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
            .ping(self.req(okfpga::v1::PingRequest {
                user_id: user_id.to_string(),
            }))
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
            .list_users(self.req(okfpga::v1::ListUsersRequest {}))
            .await
    }
}
