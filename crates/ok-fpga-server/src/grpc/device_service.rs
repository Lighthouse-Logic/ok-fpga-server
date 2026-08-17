//! # DeviceService gRPC Implementation
//!
//! Implements the `DeviceService` from `okfpga.proto` (ADR-005).
//! Provides RPCs for device discovery, info, sensors, settings, SYZYGY,
//! configuration, PLL, flash, event streaming, resources, and users.

use std::sync::Arc;

use tonic::{Request, Response, Status};
use tracing::warn;

use crate::grpc::proto;
use crate::state::AppState;

/// gRPC implementation of the `DeviceService`.
pub struct DeviceServiceImpl {
    state: Arc<AppState>,
}

impl DeviceServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Convert a core FpgaDevice into a proto DeviceInfo message.
    fn device_to_proto(dev: &ok_fpga_core::device::FpgaDevice) -> proto::DeviceInfo {
        proto::DeviceInfo {
            serial: dev.info.serial.clone(),
            device_id: dev.info.device_id.clone(),
            product_name: dev.info.product_name.clone(),
            product_id: dev.info.product_id,
            device_interface: dev.info.device_interface as i32,
            usb_speed: dev.info.usb_speed as i32,
            device_major_version: dev.info.device_major_version,
            device_minor_version: dev.info.device_minor_version,
            wire_width: dev.info.wire_width,
            trigger_width: dev.info.trigger_width,
            pipe_width: dev.info.pipe_width,
            register_address_width: dev.info.register_address_width,
            register_data_width: dev.info.register_data_width,
            fpga_vendor: dev.info.fpga_vendor as i32,
            dataport: dev.info.dataport as i32,
            is_front_panel_enabled: dev.is_programmed(),
            bitfile: None,
        }
    }

    /// Convert a core Resource into a proto Resource message.
    fn resource_to_proto(r: &ok_fpga_core::resource::Resource) -> proto::Resource {
        use ok_fpga_core::resource::ResourceState;
        let state = match r.state {
            ResourceState::Unavailable => proto::ResourceState::Unavailable as i32,
            ResourceState::Initializing => proto::ResourceState::Initializing as i32,
            ResourceState::Available => proto::ResourceState::Available as i32,
            ResourceState::InUseShared => proto::ResourceState::InUseShared as i32,
            ResourceState::InUseLocked => proto::ResourceState::InUseLocked as i32,
        };
        proto::Resource {
            id: r.id.clone(),
            name: r.name.clone(),
            description: r.description.clone().unwrap_or_default(),
            device_serial: r.device_serial.clone(),
            state,
            holders: r.holders.clone(),
        }
    }
}

#[tonic::async_trait]
impl proto::device_service_server::DeviceService for DeviceServiceImpl {
    // ========================================================================
    // Device Discovery
    // ========================================================================

    async fn list_devices(
        &self,
        _request: Request<proto::ListDevicesRequest>,
    ) -> Result<Response<proto::ListDevicesResponse>, Status> {
        let devices = self.state.device_manager.registry().list();
        let device_infos: Vec<proto::DeviceInfo> =
            devices.iter().map(Self::device_to_proto).collect();

        Ok(Response::new(proto::ListDevicesResponse {
            devices: device_infos,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn get_device_info(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DeviceInfoResponse>, Status> {
        let serial = &request.get_ref().serial;
        let dev = self
            .state
            .device_manager
            .registry()
            .get(serial)
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", serial)))?;

        Ok(Response::new(proto::DeviceInfoResponse {
            device: Some(Self::device_to_proto(&dev)),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    // ========================================================================
    // Sensors & Settings
    // ========================================================================

    async fn get_device_sensors(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DeviceSensorsResponse>, Status> {
        let serial = &request.get_ref().serial;
        warn!(serial = %serial, "GetDeviceSensors: requires FFI — returning empty");
        Ok(Response::new(proto::DeviceSensorsResponse {
            sensors: vec![],
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn get_device_settings(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DeviceSettingsResponse>, Status> {
        let serial = &request.get_ref().serial;
        warn!(serial = %serial, "GetDeviceSettings: requires FFI — returning empty");
        Ok(Response::new(proto::DeviceSettingsResponse {
            settings: vec![],
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn get_device_setting(
        &self,
        _request: Request<proto::GetDeviceSettingRequest>,
    ) -> Result<Response<proto::DeviceSettingResponse>, Status> {
        Err(Status::unimplemented(
            "GetDeviceSetting requires FFI integration",
        ))
    }

    async fn set_device_setting(
        &self,
        _request: Request<proto::SetDeviceSettingRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "SetDeviceSetting requires FFI integration",
        ))
    }

    // ========================================================================
    // SYZYGY
    // ========================================================================

    async fn get_syzygy_ports(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::SyzygyPortsResponse>, Status> {
        Err(Status::unimplemented(
            "GetSyzygyPorts requires FFI integration",
        ))
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    async fn configure_fpga(
        &self,
        _request: Request<proto::ConfigureFpgaRequest>,
    ) -> Result<Response<proto::ConfigureFpgaResponse>, Status> {
        Err(Status::unimplemented(
            "ConfigureFPGA requires FFI integration",
        ))
    }

    async fn configure_fpga_from_memory(
        &self,
        _request: Request<tonic::Streaming<proto::ConfigureFpgaChunk>>,
    ) -> Result<Response<proto::ConfigureFpgaResponse>, Status> {
        Err(Status::unimplemented(
            "ConfigureFPGAFromMemory requires FFI integration",
        ))
    }

    async fn clear_fpga_configuration(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "ClearFPGAConfiguration requires FFI integration",
        ))
    }

    async fn get_bitfile_info(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::BitfileInfoResponse>, Status> {
        Err(Status::unimplemented(
            "GetBitfileInfo requires FFI integration",
        ))
    }

    async fn get_dataport_type(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DataportTypeResponse>, Status> {
        Err(Status::unimplemented(
            "GetDataportType requires FFI integration",
        ))
    }

    // ========================================================================
    // PLL
    // ========================================================================

    async fn get_pll_configuration(
        &self,
        _request: Request<proto::GetPllRequest>,
    ) -> Result<Response<proto::PllConfigurationResponse>, Status> {
        Err(Status::unimplemented(
            "GetPLLConfiguration requires FFI integration",
        ))
    }

    async fn set_pll_configuration(
        &self,
        _request: Request<proto::SetPllRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "SetPLLConfiguration requires FFI integration",
        ))
    }

    async fn load_default_pll_configuration(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "LoadDefaultPLLConfiguration requires FFI integration",
        ))
    }

    // ========================================================================
    // Flash
    // ========================================================================

    async fn flash_read(
        &self,
        _request: Request<proto::FlashReadRequest>,
    ) -> Result<Response<proto::FlashDataResponse>, Status> {
        Err(Status::unimplemented("FlashRead requires FFI integration"))
    }

    async fn flash_write(
        &self,
        _request: Request<proto::FlashWriteRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented("FlashWrite requires FFI integration"))
    }

    async fn flash_erase_sector(
        &self,
        _request: Request<proto::FlashEraseSectorRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "FlashEraseSector requires FFI integration",
        ))
    }

    // ========================================================================
    // Device Settings
    // ========================================================================

    async fn set_device_id(
        &self,
        _request: Request<proto::SetDeviceIdRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "SetDeviceID requires FFI integration",
        ))
    }

    async fn set_timeout(
        &self,
        _request: Request<proto::SetTimeoutRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented("SetTimeout requires FFI integration"))
    }

    // ========================================================================
    // Server Status (from DeviceService)
    // ========================================================================

    async fn get_server_status(
        &self,
        _request: Request<proto::ServerStatusRequest>,
    ) -> Result<Response<proto::ServerStatus>, Status> {
        let device_count = self.state.device_manager.device_count() as i32;
        let resource_count = self.state.resource_registry.count() as i32;
        let uptime_secs = self.state.uptime_secs();
        let uptime_str = format!(
            "{}h {}m {}s",
            uptime_secs / 3600,
            (uptime_secs % 3600) / 60,
            uptime_secs % 60
        );

        Ok(Response::new(proto::ServerStatus {
            version: self.state.api_version.to_string(),
            frontpanel_version: "6.1.0".to_string(),
            platform: std::env::consts::OS.to_string(),
            uptime: uptime_str,
            uptime_seconds: uptime_secs as i64,
            connected_devices: device_count,
            active_resources: resource_count,
        }))
    }

    // ========================================================================
    // Events (Server Streaming)
    // ========================================================================

    type WatchDeviceEventsStream =
        tokio_stream::wrappers::ReceiverStream<Result<proto::DeviceEvent, Status>>;

    async fn watch_device_events(
        &self,
        _request: Request<proto::WatchDeviceEventsRequest>,
    ) -> Result<Response<Self::WatchDeviceEventsStream>, Status> {
        Err(Status::unimplemented(
            "WatchDeviceEvents requires event bridge",
        ))
    }

    // ========================================================================
    // Resources
    // ========================================================================

    async fn list_resources(
        &self,
        _request: Request<proto::ListResourcesRequest>,
    ) -> Result<Response<proto::ListResourcesResponse>, Status> {
        Err(Status::unimplemented(
            "ListResources requires resource registry integration",
        ))
    }

    async fn get_resource(
        &self,
        _request: Request<proto::ResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        Err(Status::unimplemented(
            "GetResource requires resource registry integration",
        ))
    }

    async fn acquire_resource(
        &self,
        _request: Request<proto::AcquireResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        Err(Status::unimplemented(
            "AcquireResource requires resource registry integration",
        ))
    }

    async fn release_resource(
        &self,
        _request: Request<proto::ReleaseResourceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "ReleaseResource requires resource registry integration",
        ))
    }

    async fn create_resource(
        &self,
        request: Request<proto::CreateResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        let req = request.into_inner();

        // Validate required fields.
        if req.name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        if req.device_serial.is_empty() {
            return Err(Status::invalid_argument("device_serial is required"));
        }

        // Validate the device exists and is programmed.
        let registry = self.state.device_manager.registry();
        match registry.get(&req.device_serial) {
            Some(dev) => {
                if !dev.is_programmed() {
                    return Err(Status::failed_precondition(
                        "FPGA is not programmed — configure it first",
                    ));
                }
            }
            None => {
                return Err(Status::not_found(format!(
                    "Device '{}' not found",
                    req.device_serial
                )));
            }
        }

        // Generate resource ID: {serial}:{name}
        let resource_id = format!("{}:{}", req.device_serial, req.name);

        let resource = ok_fpga_core::resource::Resource::new(
            resource_id.clone(),
            req.name,
            if req.description.is_empty() {
                None
            } else {
                Some(req.description)
            },
            req.device_serial,
        );

        if self.state.resource_registry.register(resource) {
            let created = self.state.resource_registry.get(&resource_id).unwrap();
            Ok(Response::new(proto::ResourceResponse {
                resource: Some(Self::resource_to_proto(&created)),
                error: Some(proto::ErrorCluster {
                    status: false,
                    code: 0,
                    source: String::new(),
                }),
            }))
        } else {
            Err(Status::already_exists(format!(
                "Resource '{}' already exists",
                resource_id
            )))
        }
    }

    async fn delete_resource(
        &self,
        request: Request<proto::ResourceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();

        // Check if resource exists and is not currently held.
        match self.state.resource_registry.get(&req.resource_id) {
            Some(resource) => {
                if !resource.holders.is_empty() {
                    return Err(Status::failed_precondition(format!(
                        "Resource '{}' is currently held by {} user(s) — release it first",
                        req.resource_id,
                        resource.holders.len()
                    )));
                }
            }
            None => {
                return Err(Status::not_found(format!(
                    "Resource '{}' not found",
                    req.resource_id
                )));
            }
        }

        self.state.resource_registry.remove(&req.resource_id);

        Ok(Response::new(proto::StatusResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    // ========================================================================
    // Users
    // ========================================================================

    async fn list_users(
        &self,
        _request: Request<proto::ListUsersRequest>,
    ) -> Result<Response<proto::ListUsersResponse>, Status> {
        Ok(Response::new(proto::ListUsersResponse {
            users: vec![],
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn get_user(
        &self,
        _request: Request<proto::UserRequest>,
    ) -> Result<Response<proto::UserResponse>, Status> {
        Err(Status::unimplemented(
            "GetUser requires user session integration",
        ))
    }
}
