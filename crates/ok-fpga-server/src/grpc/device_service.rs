//! # DeviceService gRPC Implementation
//!
//! Implements the `DeviceService` from `okfpga.proto` (ADR-005).
//! Provides RPCs for device discovery, info, sensors, settings, SYZYGY,
//! configuration, PLL, flash, event streaming, resources, and users.

use std::sync::Arc;

use tonic::{Request, Response, Status};
use tracing::{info, warn};

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

    /// `GetDeviceSettings` — enumerate all settings on a device.
    ///
    /// Reads settings via FFI (ADR-021): construct settings handle, populate
    /// from device, enumerate keys, read each value, then clean up.
    async fn get_device_settings(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DeviceSettingsResponse>, Status> {
        let serial = &request.get_ref().serial;

        let pool = self
            .state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unimplemented("FrontPanel DLL not loaded"))?;

        let device_arc = pool
            .get(serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not in handle pool", serial)))?;

        let device = device_arc.lock().await;
        let dll = device.dll();
        let handle = device.raw();

        // SAFETY: handle is valid (from pool). We construct settings + names
        // containers, populate from the device, iterate keys, then destroy both.
        let settings_vec: Vec<proto::DeviceSetting> = unsafe {
            let settings = (dll.settings_construct)();
            if settings.is_null() {
                return Err(Status::internal("Failed to construct settings handle"));
            }

            let rc = (dll.fp_get_device_settings)(handle, settings);
            if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                (dll.settings_destruct)(settings);
                return Err(Status::internal(format!(
                    "GetDeviceSettings failed with code {}",
                    rc as i32
                )));
            }

            let names = (dll.setting_names_construct)();
            if names.is_null() {
                (dll.settings_destruct)(settings);
                return Err(Status::internal("Failed to construct setting names handle"));
            }

            let rc = (dll.settings_list)(settings, names);
            if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                (dll.setting_names_destruct)(names);
                (dll.settings_destruct)(settings);
                return Err(Status::internal(format!(
                    "Settings_List failed with code {}",
                    rc as i32
                )));
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

                let key_c = std::ffi::CString::new(key.clone()).unwrap_or_default();
                let mut int_val: ok_fpga_ffi::UINT32 = 0;
                let int_rc =
                    (dll.settings_get_int)(settings, key_c.as_ptr(), &mut int_val);

                if int_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                    result.push(proto::DeviceSetting {
                        key,
                        value: Some(proto::device_setting::Value::IntValue(int_val)),
                    });
                } else {
                    let mut str_buf = vec![0i8; 256];
                    let str_rc = (dll.settings_get_string)(
                        settings,
                        key_c.as_ptr(),
                        str_buf.len() as i32,
                        str_buf.as_mut_ptr(),
                    );
                    if str_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                        let val = cstr_from_char_array(&str_buf);
                        result.push(proto::DeviceSetting {
                            key,
                            value: Some(proto::device_setting::Value::StringValue(val)),
                        });
                    } else {
                        // Key exists but neither type read succeeded.
                        result.push(proto::DeviceSetting {
                            key,
                            value: None,
                        });
                    }
                }
            }

            (dll.setting_names_destruct)(names);
            (dll.settings_destruct)(settings);
            result
        };

        info!(serial = %serial, count = settings_vec.len(), "GetDeviceSettings: returned settings");
        Ok(Response::new(proto::DeviceSettingsResponse {
            settings: settings_vec,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    /// `GetDeviceSetting` — read a single setting by key.
    async fn get_device_setting(
        &self,
        request: Request<proto::GetDeviceSettingRequest>,
    ) -> Result<Response<proto::DeviceSettingResponse>, Status> {
        let req = request.get_ref();
        let serial = &req.serial;
        let key = &req.key;

        let pool = self
            .state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unimplemented("FrontPanel DLL not loaded"))?;

        let device_arc = pool
            .get(serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not in handle pool", serial)))?;

        let device = device_arc.lock().await;
        let dll = device.dll();
        let handle = device.raw();

        // SAFETY: handle is valid (from pool). Construct, populate, read, destroy.
        let setting: proto::DeviceSetting = unsafe {
            let settings = (dll.settings_construct)();
            if settings.is_null() {
                return Err(Status::internal("Failed to construct settings handle"));
            }

            let rc = (dll.fp_get_device_settings)(handle, settings);
            if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                (dll.settings_destruct)(settings);
                return Err(Status::internal(format!(
                    "GetDeviceSettings failed with code {}",
                    rc as i32
                )));
            }

            let key_c = std::ffi::CString::new(key.clone()).map_err(|_| {
                (dll.settings_destruct)(settings);
                Status::invalid_argument(format!("invalid setting key: {}", key))
            })?;

            // Try integer first.
            let mut int_val: ok_fpga_ffi::UINT32 = 0;
            let int_rc = (dll.settings_get_int)(settings, key_c.as_ptr(), &mut int_val);

            let value = if int_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                Some(proto::device_setting::Value::IntValue(int_val))
            } else {
                let mut str_buf = vec![0i8; 256];
                let str_rc = (dll.settings_get_string)(
                    settings,
                    key_c.as_ptr(),
                    str_buf.len() as i32,
                    str_buf.as_mut_ptr(),
                );
                if str_rc as i32 == ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                    let val = cstr_from_char_array(&str_buf);
                    Some(proto::device_setting::Value::StringValue(val))
                } else {
                    (dll.settings_destruct)(settings);
                    return Err(Status::not_found(format!(
                        "setting '{}' not found on device '{}'",
                        key, serial
                    )));
                }
            };

            (dll.settings_destruct)(settings);
            proto::DeviceSetting {
                key: key.clone(),
                value,
            }
        };

        Ok(Response::new(proto::DeviceSettingResponse {
            setting: Some(setting),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    /// `SetDeviceSetting` — write a setting and persist to device.
    async fn set_device_setting(
        &self,
        request: Request<proto::SetDeviceSettingRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.get_ref();
        let serial = &req.serial;
        let setting = req
            .setting
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("setting field is required"))?;

        let pool = self
            .state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unimplemented("FrontPanel DLL not loaded"))?;

        let device_arc = pool
            .get(serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not in handle pool", serial)))?;

        let device = device_arc.lock().await;
        let dll = device.dll();
        let handle = device.raw();

        // SAFETY: handle is valid (from pool). Construct, populate, write, save, destroy.
        unsafe {
            let settings_h = (dll.settings_construct)();
            if settings_h.is_null() {
                return Err(Status::internal("Failed to construct settings handle"));
            }

            let rc = (dll.fp_get_device_settings)(handle, settings_h);
            if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                (dll.settings_destruct)(settings_h);
                return Err(Status::internal(format!(
                    "GetDeviceSettings failed with code {}",
                    rc as i32
                )));
            }

            let key_c = std::ffi::CString::new(setting.key.clone()).map_err(|_| {
                (dll.settings_destruct)(settings_h);
                Status::invalid_argument(format!("invalid setting key: {}", setting.key))
            })?;

            // Write the value based on the oneof variant.
            let write_rc = match &setting.value {
                Some(proto::device_setting::Value::IntValue(val)) => {
                    (dll.settings_set_int)(settings_h, key_c.as_ptr(), *val)
                }
                Some(proto::device_setting::Value::StringValue(val)) => {
                    let val_c = std::ffi::CString::new(val.as_str()).map_err(|_| {
                        (dll.settings_destruct)(settings_h);
                        Status::invalid_argument("string_value contains null bytes")
                    })?;
                    (dll.settings_set_string)(settings_h, key_c.as_ptr(), val_c.as_ptr())
                }
                None => {
                    (dll.settings_destruct)(settings_h);
                    return Err(Status::invalid_argument(
                        "setting value is required (string_value or int_value)",
                    ));
                }
            };

            if write_rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                (dll.settings_destruct)(settings_h);
                return Err(Status::internal(format!(
                    "Failed to set setting '{}' with code {}",
                    setting.key, write_rc as i32
                )));
            }

            // Persist changes to device flash/EEPROM.
            let save_rc = (dll.settings_save)(settings_h);
            (dll.settings_destruct)(settings_h);

            if save_rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                return Err(Status::internal(format!(
                    "settings_save failed with code {}",
                    save_rc as i32
                )));
            }
        }

        info!(serial = %serial, key = %setting.key, "SetDeviceSetting: saved");
        Ok(Response::new(proto::StatusResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    // ========================================================================
    // SYZYGY
    // ========================================================================

    /// `GetSyzygyPorts` — discover SYZYGY peripheral ports via device settings.
    ///
    /// Probes `SYZYGYn_PORT_STATUS` keys (n = 0..15) to discover ports,
    /// then reads DNA fields for each port found (ADR-009).
    async fn get_syzygy_ports(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::SyzygyPortsResponse>, Status> {
        let serial = &request.get_ref().serial;

        let pool = self
            .state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unimplemented("FrontPanel DLL not loaded"))?;

        let device_arc = pool
            .get(serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not in handle pool", serial)))?;

        let device = device_arc.lock().await;
        let dll = device.dll();
        let handle = device.raw();

        // SAFETY: handle is valid (from pool). Construct settings, populate,
        // probe SYZYGY keys, then destroy.
        let ports: Vec<proto::SyzygyPort> = unsafe {
            let settings = (dll.settings_construct)();
            if settings.is_null() {
                return Err(Status::internal("Failed to construct settings handle"));
            }

            let rc = (dll.fp_get_device_settings)(handle, settings);
            if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                (dll.settings_destruct)(settings);
                return Err(Status::internal(format!(
                    "GetDeviceSettings failed with code {}",
                    rc as i32
                )));
            }

            let mut result = Vec::new();

            for n in 0u32..16 {
                let status_key = format!("SYZYGY{}_PORT_STATUS\0", n);
                let status_key_c = std::ffi::CString::from_vec_unchecked(status_key.into_bytes());
                let mut port_status: ok_fpga_ffi::UINT32 = 0;
                let rc = (dll.settings_get_int)(settings, status_key_c.as_ptr(), &mut port_status);

                if rc as i32 != ok_fpga_ffi::ok_ErrorCode::ok_NoError as i32 {
                    break;
                }

                // Read DNA string fields.
                let fields = [
                    "MANUFACTURER_NAME",
                    "PRODUCT_NAME",
                    "PRODUCT_MODEL",
                    "PRODUCT_VERSION",
                    "SERIAL_NUMBER",
                ];

                let mut dna = std::collections::HashMap::new();
                for field in &fields {
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

                result.push(proto::SyzygyPort {
                    index: n,
                    port_status_mv: port_status,
                    is_populated: port_status > 0,
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

        info!(serial = %serial, count = ports.len(), "GetSyzygyPorts: discovered ports");
        Ok(Response::new(proto::SyzygyPortsResponse {
            ports,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// `ConfigureFPGA` — program the FPGA from a bitfile path on the server.
    ///
    /// Delegates to `DeviceHandlePool::configure_from_path()` which calls
    /// `okFrontPanel_ConfigureFPGA(handle, path)` via FFI.
    async fn configure_fpga(
        &self,
        request: Request<proto::ConfigureFpgaRequest>,
    ) -> Result<Response<proto::ConfigureFpgaResponse>, Status> {
        let req = request.into_inner();
        let serial = &req.serial;
        let bitfile_path = &req.bitfile_path;

        // Verify device exists in registry.
        if !self.state.device_manager.registry().contains(serial) {
            return Err(Status::not_found(format!("device '{}' not found", serial)));
        }

        let pool = self
            .state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unavailable("FrontPanel DLL not loaded"))?;

        let path = std::path::Path::new(bitfile_path);
        if !path.exists() {
            return Err(Status::invalid_argument(format!(
                "bitfile not found: {}", bitfile_path
            )));
        }

        pool.configure_from_path(serial, path)
            .await
            .map_err(|e| Status::internal(format!("FFI error: {}", e)))?;

        // Update device state in the registry.
        let now = chrono::Utc::now();
        self.state.device_manager.registry().update_state(
            serial,
            ok_fpga_core::device::DeviceState::Programmed {
                bitfile: ok_fpga_core::device::BitfileRecord {
                    filename: path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| bitfile_path.to_string()),
                    description: None,
                    programmed_at: now,
                    programmed_by: req.user_id.clone(),
                },
            },
        );

        info!(serial = %serial, path = %bitfile_path, "gRPC ConfigureFPGA succeeded");

        Ok(Response::new(proto::ConfigureFpgaResponse {
            bitfile: Some(proto::BitfileInfo {
                filename: bitfile_path.to_string(),
                description: String::new(),
                programmed_at: now.to_rfc3339(),
                programmed_by: req.user_id,
            }),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    /// `ConfigureFPGAFromMemory` — program the FPGA from streamed bitfile chunks.
    ///
    /// Collects all chunks from the client stream, reassembles the bitfile
    /// in memory, then delegates to `DeviceHandlePool::configure_from_memory()`
    /// which calls `okFrontPanel_ConfigureFPGAFromMemory(handle, data, len)`.
    async fn configure_fpga_from_memory(
        &self,
        request: Request<tonic::Streaming<proto::ConfigureFpgaChunk>>,
    ) -> Result<Response<proto::ConfigureFpgaResponse>, Status> {
        let mut stream = request.into_inner();
        let mut serial = String::new();
        let mut user_id = String::new();
        let mut bitfile_data = Vec::new();

        // Collect all chunks from the stream.
        while let Some(chunk) = stream.message().await? {
            // Serial and user_id are only required in the first chunk.
            if serial.is_empty() && !chunk.serial.is_empty() {
                serial = chunk.serial;
            }
            if user_id.is_empty() && !chunk.user_id.is_empty() {
                user_id = chunk.user_id;
            }
            bitfile_data.extend_from_slice(&chunk.data);
        }

        if serial.is_empty() {
            return Err(Status::invalid_argument(
                "serial number must be provided in the first chunk",
            ));
        }

        // Verify device exists in registry.
        if !self.state.device_manager.registry().contains(&serial) {
            return Err(Status::not_found(format!("device '{}' not found", serial)));
        }

        let pool = self
            .state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unavailable("FrontPanel DLL not loaded"))?;

        pool.configure_from_memory(&serial, &bitfile_data)
            .await
            .map_err(|e| Status::internal(format!("FFI error: {}", e)))?;

        // Update device state in the registry.
        let now = chrono::Utc::now();
        self.state.device_manager.registry().update_state(
            &serial,
            ok_fpga_core::device::DeviceState::Programmed {
                bitfile: ok_fpga_core::device::BitfileRecord {
                    filename: "streamed".to_string(),
                    description: None,
                    programmed_at: now,
                    programmed_by: user_id.clone(),
                },
            },
        );

        info!(
            serial = %serial,
            size_bytes = bitfile_data.len(),
            "gRPC ConfigureFPGAFromMemory succeeded"
        );

        Ok(Response::new(proto::ConfigureFpgaResponse {
            bitfile: Some(proto::BitfileInfo {
                filename: "streamed".to_string(),
                description: String::new(),
                programmed_at: now.to_rfc3339(),
                programmed_by: user_id,
            }),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
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
        let resources: Vec<proto::Resource> = self
            .state
            .resource_registry
            .list()
            .iter()
            .map(Self::resource_to_proto)
            .collect();

        Ok(Response::new(proto::ListResourcesResponse {
            resources,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn get_resource(
        &self,
        request: Request<proto::ResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        let req = request.into_inner();
        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::not_found(format!("Resource '{}' not found", req.resource_id)))?;

        Ok(Response::new(proto::ResourceResponse {
            resource: Some(Self::resource_to_proto(&resource)),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn acquire_resource(
        &self,
        request: Request<proto::AcquireResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        let req = request.into_inner();
        if req.resource_id.is_empty() {
            return Err(Status::invalid_argument("resource_id is required"));
        }
        if req.user_id.is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }

        let mode = if req.mode == proto::AccessMode::Exclusive as i32 {
            ok_fpga_core::resource::AccessMode::Exclusive
        } else {
            ok_fpga_core::resource::AccessMode::Shared
        };

        self.state
            .resource_registry
            .acquire(&req.resource_id, &req.user_id, mode)
            .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::internal("Resource vanished after acquire"))?;

        Ok(Response::new(proto::ResourceResponse {
            resource: Some(Self::resource_to_proto(&resource)),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn release_resource(
        &self,
        request: Request<proto::ReleaseResourceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();
        if req.resource_id.is_empty() {
            return Err(Status::invalid_argument("resource_id is required"));
        }
        if req.user_id.is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }

        let released = self
            .state
            .resource_registry
            .release(&req.resource_id, &req.user_id);

        if !released {
            return Err(Status::not_found(format!(
                "User '{}' is not holding resource '{}'",
                req.user_id, req.resource_id
            )));
        }

        Ok(Response::new(proto::StatusResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
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

        // Parse access mode (default to Shared for backward compatibility).
        let access_mode = match req.resource_type.as_str() {
            "exclusive" => ok_fpga_core::resource::AccessMode::Exclusive,
            _ => ok_fpga_core::resource::AccessMode::Shared,
        };

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
            access_mode,
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

    // ========================================================================
    // Lock / Unlock (ADR-020 §3)
    // ========================================================================

    async fn lock_resource(
        &self,
        request: Request<proto::LockResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        let req = request.into_inner();

        if req.resource_id.is_empty() {
            return Err(Status::invalid_argument("resource_id is required"));
        }
        if req.user_id.is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }

        self.state
            .resource_registry
            .lock(&req.resource_id, &req.user_id)
            .map_err(|e| Status::failed_precondition(e))?;

        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::internal("Resource disappeared after lock"))?;

        Ok(Response::new(proto::ResourceResponse {
            resource: Some(Self::resource_to_proto(&resource)),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn unlock_resource(
        &self,
        request: Request<proto::LockResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        let req = request.into_inner();

        if req.resource_id.is_empty() {
            return Err(Status::invalid_argument("resource_id is required"));
        }
        if req.user_id.is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }

        self.state
            .resource_registry
            .unlock(&req.resource_id, &req.user_id)
            .map_err(|e| Status::failed_precondition(e))?;

        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::internal("Resource disappeared after unlock"))?;

        Ok(Response::new(proto::ResourceResponse {
            resource: Some(Self::resource_to_proto(&resource)),
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    // ========================================================================
    // Heartbeat / Liveness (ADR-020 §5)
    // ========================================================================

    async fn ping(
        &self,
        request: Request<proto::PingRequest>,
    ) -> Result<Response<proto::PingResponse>, Status> {
        let req = request.into_inner();

        if req.user_id.is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }

        // Update the user session's last_seen_at timestamp (ADR-020 §5).
        self.state.user_sessions.touch(&req.user_id);

        let server_time = chrono::Utc::now().to_rfc3339();

        Ok(Response::new(proto::PingResponse {
            server_time,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }
}

/// Extract a Rust `String` from a C `char` array (null-terminated).
///
/// Used by the settings handlers to convert FFI string buffers to Rust strings.
/// Same implementation as `rest::devices::cstr_from_char_array`.
fn cstr_from_char_array(buf: &[::std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}
