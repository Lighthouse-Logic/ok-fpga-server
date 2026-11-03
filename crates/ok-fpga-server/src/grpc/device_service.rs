//! # DeviceService gRPC Implementation
//!
//! Implements the `DeviceService` from `okfpga.proto` (ADR-005).
//! Provides RPCs for device discovery, info, sensors, settings, SYZYGY,
//! configuration, PLL, flash, event streaming, resources, and users.

use std::sync::Arc;

use tonic::{Code, Request, Response, Status};
use tracing::{info, warn};

use ok_fpga_core::declaration::{build_resource, check_interfaces_for_dataport, check_overlap};
use ok_fpga_core::manager::{DeleteReason, DeviceEvent};
use ok_fpga_core::resource::{AccessMode, AcquireOutcome, LockOutcome, RegisterError};

use crate::grpc::proto;
use crate::grpc::resource_mapping::{
    conflict_status, create_request_to_spec, device_event_to_proto, holder_mode_from_proto,
    lock_error_status, resource_error_status, user_session_to_proto,
};
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

    /// Convert a core Resource into a proto Resource message (ADR-034).
    fn resource_to_proto(r: &ok_fpga_core::resource::Resource) -> proto::Resource {
        crate::grpc::resource_mapping::resource_to_proto(r)
    }

    /// A successful `ErrorCluster` (no error).
    fn ok_cluster() -> Option<proto::ErrorCluster> {
        Some(proto::ErrorCluster {
            status: false,
            code: 0,
            source: String::new(),
        })
    }

    /// Broadcast `ResourceChanged` with the resource's spec state string,
    /// exactly as the REST handlers do (one event stream for both APIs).
    fn emit_resource_changed(&self, id: &str) {
        if let Some(resource) = self.state.resource_registry.get(id) {
            let _ = self.state.device_manager.broadcast(DeviceEvent::ResourceChanged {
                resource_id: id.to_string(),
                new_state: resource.state.as_str().to_string(),
                timestamp: chrono::Utc::now(),
            });
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

        // Phase 34-C: Extract filename as basename, track full path, compute MD5.
        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| bitfile_path.to_string());
        let file_path_str = bitfile_path.to_string();
        let md5_hash = match std::fs::read(path) {
            Ok(data) => Some(format!("{:x}", md5::compute(&data))),
            Err(_) => None,
        };

        // Update device state in the registry.
        let now = chrono::Utc::now();
        self.state.device_manager.registry().update_state(
            serial,
            ok_fpga_core::device::DeviceState::Programmed {
                bitfile: ok_fpga_core::device::BitfileRecord {
                    filename: filename.clone(),
                    description: None,
                    programmed_at: now,
                    programmed_by: req.user_id.clone(),
                    file_path: Some(file_path_str.clone()),
                    md5_hash: md5_hash.clone(),
                },
            },
        );

        info!(serial = %serial, path = %bitfile_path, "gRPC ConfigureFPGA succeeded");

        // Broadcast events for dashboard auto-refresh (Phase 28D).
        self.state.broadcast_event(&format!(
            r#"{{"type":"device_programmed","device_serial":"{}"}}"#,
            serial
        ));

        // Clear stale resources for this device (Phase 25-Fix-F, F3).
        let cleared = self.state.resource_registry.remove_for_device(serial);
        if cleared > 0 {
            info!(serial = %serial, cleared, "Cleared resources for re-programmed device");

            self.state.broadcast_event(&format!(
                r#"{{"type":"resources_cleared","device_serial":"{}","cleared_count":{}}}"#,
                serial, cleared
            ));
        }

        Ok(Response::new(proto::ConfigureFpgaResponse {
            bitfile: Some(proto::BitfileInfo {
                filename,
                description: String::new(),
                programmed_at: now.to_rfc3339(),
                programmed_by: req.user_id,
                file_path: file_path_str,
                md5_hash: md5_hash.unwrap_or_default(),
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
        let mut filename = String::new();
        let mut bitfile_data = Vec::new();

        // Collect all chunks from the stream.
        while let Some(chunk) = stream.message().await? {
            // Serial, user_id, and filename are only required in the first chunk.
            if serial.is_empty() && !chunk.serial.is_empty() {
                serial = chunk.serial;
            }
            if user_id.is_empty() && !chunk.user_id.is_empty() {
                user_id = chunk.user_id;
            }
            // Phase 34-C: Extract optional filename from the first chunk.
            if filename.is_empty() && !chunk.filename.is_empty() {
                filename = chunk.filename;
            }
            bitfile_data.extend_from_slice(&chunk.data);
        }

        // Phase 34-C: Fall back to "uploaded" if no filename was provided.
        if filename.is_empty() {
            filename = "uploaded".to_string();
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

        // Phase 34-C: Compute MD5 hash from accumulated bitfile bytes.
        let md5_hash = Some(format!("{:x}", md5::compute(&bitfile_data)));

        // Update device state in the registry.
        let now = chrono::Utc::now();
        self.state.device_manager.registry().update_state(
            &serial,
            ok_fpga_core::device::DeviceState::Programmed {
                bitfile: ok_fpga_core::device::BitfileRecord {
                    filename: filename.clone(),
                    description: None,
                    programmed_at: now,
                    programmed_by: user_id.clone(),
                    file_path: None,
                    md5_hash: md5_hash.clone(),
                },
            },
        );

        info!(
            serial = %serial,
            size_bytes = bitfile_data.len(),
            "gRPC ConfigureFPGAFromMemory succeeded"
        );

        // Clear stale resources for this device (Phase 25-Fix-F, F3).
        let cleared = self.state.resource_registry.remove_for_device(&serial);
        if cleared > 0 {
            info!(serial = %serial, cleared, "Cleared resources for re-programmed device");
        }

        Ok(Response::new(proto::ConfigureFpgaResponse {
            bitfile: Some(proto::BitfileInfo {
                filename,
                description: String::new(),
                programmed_at: now.to_rfc3339(),
                programmed_by: user_id,
                file_path: String::new(),
                md5_hash: md5_hash.unwrap_or_default(),
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
        let mut rx = self.state.device_manager.subscribe();
        let (tx, stream_rx) = tokio::sync::mpsc::channel(64);

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let proto_event = device_event_to_proto(&event);
                        if tx.send(Ok(proto_event)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            skipped = skipped,
                            "gRPC WatchDeviceEvents subscriber lagged, skipped events"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            stream_rx,
        )))
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

        // ADR-034 P3: `mode` is optional. When omitted, use the resource's
        // declared mode (falling back to Shared for legacy resources).
        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::not_found(format!("Resource '{}' not found", req.resource_id)))?;
        let mode = match req.mode {
            Some(value) => holder_mode_from_proto(value).map_err(|e| resource_error_status(&e))?,
            None => resource.access_mode.unwrap_or(AccessMode::Shared),
        };

        let outcome = match self
            .state
            .resource_registry
            .acquire(&req.resource_id, &req.user_id, mode)
        {
            Ok(outcome) => outcome,
            Err(denied) => {
                return Err(conflict_status(
                    Code::FailedPrecondition,
                    denied.to_string(),
                    denied.to_conflict(&resource).as_ref(),
                ));
            }
        };

        // Track the user session (implicit heartbeat on acquire, same as REST).
        self.state.user_sessions.touch(&req.user_id);

        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::internal("Resource vanished after acquire"))?;

        if outcome == AcquireOutcome::Acquired {
            self.emit_resource_changed(&req.resource_id);

            // Broadcast event for dashboard auto-refresh (Phase 28D).
            self.state.broadcast_event(&format!(
                r#"{{"type":"resource_acquired","resource_id":"{}","user_id":"{}"}}"#,
                req.resource_id, req.user_id
            ));
        }

        Ok(Response::new(proto::ResourceResponse {
            resource: Some(Self::resource_to_proto(&resource)),
            error: Self::ok_cluster(),
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

        let Some(released) = self
            .state
            .resource_registry
            .release_with_info(&req.resource_id, &req.user_id)
        else {
            return Err(Status::not_found(format!(
                "User '{}' is not holding resource '{}'",
                req.user_id, req.resource_id
            )));
        };

        // Track the user session (implicit heartbeat on release, same as REST).
        self.state.user_sessions.touch(&req.user_id);

        // 44-D Q3: If the releasing user held the lock on a shared resource,
        // broadcast ResourceUnlocked before ResourceChanged.
        if released.was_locked {
            let _ = self
                .state
                .device_manager
                .broadcast(DeviceEvent::ResourceUnlocked {
                    resource_id: req.resource_id.clone(),
                    user_id: req.user_id.clone(),
                    timestamp: chrono::Utc::now(),
                });
            self.state.broadcast_event(&format!(
                r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
                req.resource_id, req.user_id
            ));
        }

        self.emit_resource_changed(&req.resource_id);

        // Broadcast event for dashboard auto-refresh (Phase 28D).
        self.state.broadcast_event(&format!(
            r#"{{"type":"resource_released","resource_id":"{}","user_id":"{}"}}"#,
            req.resource_id, req.user_id
        ));

        Ok(Response::new(proto::StatusResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    /// `CreateResource` — register a resource (ADR-034).
    ///
    /// Same steps and rules as REST `POST /resources`: build and validate
    /// the declaration (`INVALID_ARGUMENT`), check the device (`NOT_FOUND` /
    /// `FAILED_PRECONDITION`), then run the overlap check and insert
    /// atomically. A duplicate name is `FAILED_PRECONDITION` (REST 409).
    async fn create_resource(
        &self,
        request: Request<proto::CreateResourceRequest>,
    ) -> Result<Response<proto::ResourceResponse>, Status> {
        let req = request.into_inner();

        let spec = create_request_to_spec(req).map_err(|e| resource_error_status(&e))?;
        let resource = build_resource(spec).map_err(|e| resource_error_status(&e))?;

        // The device must exist; its data port must support the declared
        // interfaces (INVALID_ARGUMENT); only then must it be programmed.
        // Same order as REST `create_resource`.
        match self.state.device_manager.registry().get(&resource.device_serial) {
            Some(dev) => {
                check_interfaces_for_dataport(&resource.interfaces, dev.info.dataport)
                    .map_err(|e| resource_error_status(&e))?;
                if !dev.is_programmed() {
                    return Err(Status::failed_precondition(
                        "FPGA is not programmed — configure it first",
                    ));
                }
            }
            None => {
                return Err(Status::not_found(format!(
                    "Device '{}' not found",
                    resource.device_serial
                )));
            }
        }

        // Overlapping ranges inside one declaration are allowed (owner
        // decision Q1); log a warning if the setting is on.
        let (warn_self_overlap, warn_readonly_side_effects) = self
            .state
            .config
            .read()
            .map(|c| {
                (
                    c.access_control.warn_range_self_overlap,
                    c.access_control.warn_readonly_side_effects,
                )
            })
            .unwrap_or((true, true));
        if warn_self_overlap {
            crate::warnings::warn_self_overlapping_ranges(&resource);
        }

        let resource_id = resource.id.clone();
        let device_serial = resource.device_serial.clone();
        let name = resource.name.clone();
        match self.state.resource_registry.try_register(resource, check_overlap) {
            Ok(()) => {}
            Err(RegisterError::Duplicate) => {
                return Err(Status::failed_precondition(format!(
                    "Resource '{}' already exists",
                    resource_id
                )));
            }
            Err(RegisterError::Rejected(err)) => return Err(resource_error_status(&err)),
        }

        let created = self
            .state
            .resource_registry
            .get(&resource_id)
            .ok_or_else(|| Status::internal("Resource vanished after registration"))?;

        if warn_readonly_side_effects {
            crate::warnings::warn_readonly_side_effects(&created);
        }
        let existing_on_device = self.state.resource_registry.list_for_device(&device_serial);
        crate::warnings::warn_wildcard_endpoints(&existing_on_device, &created);

        let _ = self.state.device_manager.broadcast(DeviceEvent::ResourceCreated {
            resource_id: resource_id.clone(),
            device_serial: device_serial.clone(),
            name,
            timestamp: chrono::Utc::now(),
        });
        self.state.broadcast_event(&format!(
            r#"{{"type":"resource_registered","resource_id":"{}","device_serial":"{}"}}"#,
            resource_id, device_serial
        ));

        Ok(Response::new(proto::ResourceResponse {
            resource: Some(Self::resource_to_proto(&created)),
            error: Self::ok_cluster(),
        }))
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

        let _ = self.state.device_manager.broadcast(DeviceEvent::ResourceDeleted {
            resource_id: req.resource_id.clone(),
            reason: DeleteReason::Client,
            timestamp: chrono::Utc::now(),
        });
        self.state.broadcast_event(&format!(
            r#"{{"type":"resource_deleted","resource_id":"{}"}}"#,
            req.resource_id
        ));

        Ok(Response::new(proto::StatusResponse {
            error: Self::ok_cluster(),
        }))
    }

    // ========================================================================
    // Users
    // ========================================================================

    /// `ListUsers` — every active session with its holdings (ADR-034 P5).
    async fn list_users(
        &self,
        _request: Request<proto::ListUsersRequest>,
    ) -> Result<Response<proto::ListUsersResponse>, Status> {
        let mut sessions = self.state.user_sessions.list();
        sessions.sort_by(|a, b| a.user_id.cmp(&b.user_id));
        let users = sessions
            .iter()
            .map(|s| {
                let holdings = self.state.resource_registry.holdings_for(&s.user_id);
                user_session_to_proto(s, &holdings)
            })
            .collect();
        Ok(Response::new(proto::ListUsersResponse {
            users,
            error: Self::ok_cluster(),
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

        let outcome = self
            .state
            .resource_registry
            .lock(&req.resource_id, &req.user_id)
            .map_err(|e| lock_error_status(&e))?;

        // Track the user session (implicit heartbeat on lock).
        self.state.user_sessions.touch(&req.user_id);

        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::internal("Resource disappeared after lock"))?;

        // 44-D Q2: Only emit ResourceChanged and ResourceLocked when a new lock
        // was acquired; repeat lock by the current locker is idempotent.
        if outcome == LockOutcome::Locked {
            self.emit_resource_changed(&req.resource_id);
            let _ = self.state.device_manager.broadcast(DeviceEvent::ResourceLocked {
                resource_id: req.resource_id.clone(),
                user_id: req.user_id.clone(),
                timestamp: chrono::Utc::now(),
            });

            // Broadcast event for dashboard auto-refresh (Phase 28D).
            self.state.broadcast_event(&format!(
                r#"{{"type":"resource_locked","resource_id":"{}","user_id":"{}"}}"#,
                req.resource_id, req.user_id
            ));
        }

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
            .map_err(|e| lock_error_status(&e))?;

        // Track the user session (implicit heartbeat on unlock).
        self.state.user_sessions.touch(&req.user_id);

        let resource = self
            .state
            .resource_registry
            .get(&req.resource_id)
            .ok_or_else(|| Status::internal("Resource disappeared after unlock"))?;

        self.emit_resource_changed(&req.resource_id);
        let _ = self.state.device_manager.broadcast(DeviceEvent::ResourceUnlocked {
            resource_id: req.resource_id.clone(),
            user_id: req.user_id.clone(),
            timestamp: chrono::Utc::now(),
        });

        // Broadcast event for dashboard auto-refresh (Phase 28D).
        self.state.broadcast_event(&format!(
            r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
            req.resource_id, req.user_id
        ));

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

    // ========================================================================
    // Disconnect (Phase 25-Fix-H, F10)
    // ========================================================================

    /// `Disconnect` RPC — User-initiated disconnect.
    ///
    /// Releases all resources held by the specified user across all devices,
    /// removes the user session, and ceases heartbeat checking. This is the
    /// recommended way for clients to end their session cleanly.
    async fn disconnect(
        &self,
        request: Request<proto::DisconnectRequest>,
    ) -> Result<Response<proto::DisconnectResponse>, Status> {
        let req = request.into_inner();

        if req.user_id.is_empty() {
            return Err(Status::invalid_argument("user_id is required"));
        }

        // Release all resources held by this user across all devices.
        let released_items = self
            .state
            .resource_registry
            .release_all_for_user_with_info(&req.user_id);
        let released = released_items.len();

        for item in &released_items {
            if item.was_locked {
                let _ = self
                    .state
                    .device_manager
                    .broadcast(DeviceEvent::ResourceUnlocked {
                        resource_id: item.resource_id.clone(),
                        user_id: req.user_id.clone(),
                        timestamp: chrono::Utc::now(),
                    });
                self.state.broadcast_event(&format!(
                    r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
                    item.resource_id, req.user_id
                ));
            }
            let _ = self
                .state
                .device_manager
                .broadcast(DeviceEvent::ResourceChanged {
                    resource_id: item.resource_id.clone(),
                    new_state: item.new_state.as_str().to_string(),
                    timestamp: chrono::Utc::now(),
                });
            self.state.broadcast_event(&format!(
                r#"{{"type":"resource_released","resource_id":"{}","user_id":"{}"}}"#,
                item.resource_id, req.user_id
            ));
        }

        // Remove the user session entirely.
        self.state.user_sessions.remove(&req.user_id);

        info!(
            user_id = %req.user_id,
            released_count = released,
            "User disconnected via gRPC Disconnect"
        );

        let _ = self
            .state
            .device_manager
            .broadcast(DeviceEvent::UserDisconnected {
                user_id: req.user_id.clone(),
                reason: "explicit".to_string(),
                timestamp: chrono::Utc::now(),
            });

        // Broadcast event for dashboard auto-refresh (Phase 28D).
        self.state.broadcast_event(&format!(
            r#"{{"type":"user_disconnected","user_id":"{}"}}"#,
            req.user_id
        ));

        Ok(Response::new(proto::DisconnectResponse {
            user_id: req.user_id,
            released_resources: released as i32,
            message: "Disconnected successfully".to_string(),
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

// ============================================================================
// Tests (Tier 3: call the service directly, no network)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::proto::device_service_server::DeviceService;
    use crate::grpc::resource_mapping::decode_conflict_detail;
    use ok_fpga_core::device::*;
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::ResourceRegistry;
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;

    const SERIAL: &str = "GRPC0001";
    /// A connected but unprogrammed device with a Classic data port.
    const UNPROGRAMMED_SERIAL: &str = "GRPC0003";

    /// A service whose state holds one programmed (emulated) device,
    /// [`SERIAL`], plus the unprogrammed [`UNPROGRAMMED_SERIAL`].
    fn service() -> DeviceServiceImpl {
        let device_manager = Arc::new(DeviceManager::new());
        let info = |serial: &str| DeviceInfo {
            serial: serial.to_string(),
            product_name: "XEM8320-AU25P".to_string(),
            product_id: 56,
            device_interface: DeviceInterface::Usb3,
            usb_speed: UsbSpeed::Super,
            device_major_version: 1,
            device_minor_version: 0,
            wire_width: 32,
            trigger_width: 32,
            pipe_width: 32,
            register_address_width: 32,
            register_data_width: 32,
            fpga_vendor: FpgaVendor::Xilinx,
            dataport: DataportType::Legacy,
            device_id: String::new(),
        };
        device_manager.registry().add(FpgaDevice {
            info: info(UNPROGRAMMED_SERIAL),
            state: DeviceState::Connected,
        });
        device_manager.registry().add(FpgaDevice {
            info: info(SERIAL),
            state: DeviceState::Programmed {
                bitfile: BitfileRecord {
                    filename: "test.bit".to_string(),
                    description: None,
                    programmed_at: chrono::Utc::now(),
                    programmed_by: "tester".to_string(),
                    file_path: None,
                    md5_hash: None,
                },
            },
        });
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        DeviceServiceImpl::new(Arc::new(AppState::new(
            device_manager,
            Arc::new(ResourceRegistry::new()),
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            Arc::new(ok_fpga_core::auth::SessionManager::new(3600)),
            Arc::new(UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Off,
            crate::config::ServerConfig::default(),
            None,
        )))
    }

    /// An exclusive resource declaring wire-in 0x05.
    fn create_request(name: &str) -> proto::CreateResourceRequest {
        proto::CreateResourceRequest {
            name: name.to_string(),
            device_serial: SERIAL.to_string(),
            access_mode: Some(proto::ResourceAccessMode::Exclusive as i32),
            interfaces: vec!["classic".to_string()],
            endpoints: Some(proto::EndpointDeclaration {
                default_access: None,
                entries: vec![proto::EndpointEntry {
                    address: 0x05,
                    r#type: "wire_in".to_string(),
                    access: Some(proto::ResourceAccessMode::Exclusive as i32),
                    private: false,
                    description: String::new(),
                }],
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_grpc_create_resource_duplicate_returns_failed_precondition() {
        let svc = service();
        svc.create_resource(Request::new(create_request("dup"))).await.unwrap();
        let status = svc
            .create_resource(Request::new(create_request("dup")))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(decode_conflict_detail(&status).is_none());
    }

    #[tokio::test]
    async fn test_grpc_create_resource_overlap_returns_conflict_detail() {
        let svc = service();
        svc.create_resource(Request::new(create_request("a"))).await.unwrap();
        let status = svc
            .create_resource(Request::new(create_request("b")))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        let detail = decode_conflict_detail(&status).expect("conflict detail");
        assert_eq!(detail.code, "overlap");
        assert_eq!(detail.resource_id, format!("{SERIAL}:a"));
        assert_eq!(detail.item, "0x05");
        assert_eq!(detail.items, vec!["0x05".to_string()]);
    }

    #[tokio::test]
    async fn test_grpc_create_resource_shared_without_endpoints_returns_invalid_argument() {
        let svc = service();
        let mut req = create_request("bad");
        req.endpoints = None;
        req.access_mode = Some(proto::ResourceAccessMode::Shared as i32);
        let status = svc.create_resource(Request::new(req)).await.unwrap_err();
        assert_eq!(status.code(), Code::InvalidArgument);
        assert_eq!(decode_conflict_detail(&status).unwrap().code, "validation");
    }

    #[tokio::test]
    async fn test_grpc_create_resource_exclusive_with_shared_item_invalid_argument() {
        let svc = service();
        let mut req = create_request("rule_a");
        req.endpoints.as_mut().unwrap().entries[0].access =
            Some(proto::ResourceAccessMode::Shared as i32);
        let status = svc.create_resource(Request::new(req)).await.unwrap_err();
        assert_eq!(status.code(), Code::InvalidArgument);
        let detail = decode_conflict_detail(&status).unwrap();
        assert_eq!(detail.code, "validation");
        assert_eq!(detail.item, "0x05");
    }

    #[tokio::test]
    async fn test_grpc_create_resource_axi_on_legacy_invalid_argument() {
        let svc = service();
        let mut req = create_request("axi");
        req.interfaces = vec!["axi_lite".to_string()];
        let status = svc.create_resource(Request::new(req)).await.unwrap_err();
        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(status.message().contains("axi_lite"));
    }

    #[tokio::test]
    async fn test_grpc_create_resource_unprogrammed_legacy_with_axi_invalid_argument() {
        // Same order as REST: the data-port check (INVALID_ARGUMENT) runs
        // before the "device is programmed" check (FAILED_PRECONDITION).
        let svc = service();
        let mut req = create_request("axi_unprog");
        req.device_serial = UNPROGRAMMED_SERIAL.to_string();
        req.interfaces = vec!["axi_lite".to_string()];
        let status = svc.create_resource(Request::new(req)).await.unwrap_err();
        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(status.message().contains("axi_lite"));
    }

    /// An address range for gRPC requests.
    fn proto_range(start: u32, end: u32, access: proto::ResourceAccessMode, private: bool) -> proto::AddressRange {
        proto::AddressRange {
            start,
            end,
            access: Some(access as i32),
            private,
            ..Default::default()
        }
    }

    /// An endpoint entry for gRPC requests.
    fn proto_entry(address: u32, access: proto::ResourceAccessMode) -> proto::EndpointEntry {
        proto::EndpointEntry {
            address,
            r#type: "wire_in".to_string(),
            access: Some(access as i32),
            private: false,
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn test_grpc_create_resource_self_overlapping_ranges_accepted() {
        let svc = service();
        let mut req = create_request("self");
        req.address_ranges = vec![
            proto_range(0x1000, 0x1FFF, proto::ResourceAccessMode::Exclusive, false),
            proto_range(0x1800, 0x2FFF, proto::ResourceAccessMode::Exclusive, true),
        ];
        assert!(svc.create_resource(Request::new(req)).await.is_ok());
    }

    #[tokio::test]
    async fn test_grpc_create_resource_overlap_items_include_endpoints_and_ranges() {
        use proto::ResourceAccessMode::Exclusive;
        let svc = service();
        let mut first = create_request("a");
        first.endpoints.as_mut().unwrap().entries = vec![proto_entry(0x01, Exclusive), proto_entry(0x00, Exclusive)];
        first.address_ranges = vec![proto_range(0x1000, 0x1FFF, Exclusive, false)];
        svc.create_resource(Request::new(first)).await.unwrap();

        let mut second = create_request("b");
        second.endpoints.as_mut().unwrap().entries = vec![proto_entry(0x00, Exclusive), proto_entry(0x01, Exclusive)];
        second.address_ranges = vec![proto_range(0x1800, 0x2FFF, Exclusive, false)];
        let status = svc.create_resource(Request::new(second)).await.unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        let detail = decode_conflict_detail(&status).unwrap();
        assert_eq!(detail.item, "0x00");
        assert_eq!(detail.items, vec!["0x00", "0x01", "0x00001000-0x00001FFF"]);
    }

    #[tokio::test]
    async fn test_grpc_create_resource_duplicate_endpoint_invalid_argument() {
        let svc = service();
        let mut req = create_request("dup_ep");
        let eps = req.endpoints.as_mut().unwrap();
        let again = eps.entries[0].clone();
        eps.entries.push(again);
        let status = svc.create_resource(Request::new(req)).await.unwrap_err();
        assert_eq!(status.code(), Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_grpc_acquire_resource_mode_omitted_uses_declared_mode() {
        let svc = service();
        svc.create_resource(Request::new(create_request("x"))).await.unwrap();
        let resp = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: format!("{SERIAL}:x"),
                user_id: "alice".to_string(),
                mode: None,
            }))
            .await
            .unwrap()
            .into_inner();
        let resource = resp.resource.unwrap();
        assert_eq!(resource.holders[0].mode, proto::AccessMode::Exclusive as i32);
        assert_eq!(resource.state, proto::ResourceState::InUseLocked as i32);
    }

    #[tokio::test]
    async fn test_grpc_list_users_includes_holdings() {
        let svc = service();
        svc.create_resource(Request::new(create_request("u"))).await.unwrap();
        svc.state.user_sessions.touch("alice");
        svc.acquire_resource(Request::new(proto::AcquireResourceRequest {
            resource_id: format!("{SERIAL}:u"),
            user_id: "alice".to_string(),
            mode: None,
        }))
        .await
        .unwrap();
        let users = svc
            .list_users(Request::new(proto::ListUsersRequest {}))
            .await
            .unwrap()
            .into_inner()
            .users;
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].holdings[0].resource_id, format!("{SERIAL}:u"));
        assert_eq!(users[0].holdings[0].mode, proto::AccessMode::Exclusive as i32);
    }

    // ── Phase 44-C: Acquisition rules (A1) ─────────────────────

    #[tokio::test]
    async fn test_grpc_acquire_same_mode_idempotent() {
        let svc = service();
        svc.create_resource(Request::new(create_request("idem"))).await.unwrap();
        let id = format!("{SERIAL}:idem");

        svc.acquire_resource(Request::new(proto::AcquireResourceRequest {
            resource_id: id.clone(),
            user_id: "alice".to_string(),
            mode: None,
        }))
        .await
        .unwrap();

        let mut events = svc.state.device_manager.subscribe();
        let resp = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: id,
                user_id: "alice".to_string(),
                mode: Some(proto::AccessMode::Exclusive as i32),
            }))
            .await
            .unwrap()
            .into_inner();
        let resource = resp.resource.unwrap();
        assert_eq!(resource.holders.len(), 1);
        assert_eq!(resource.locked_by, "alice");
        assert!(events.try_recv().is_err(), "no event on idempotent repeat acquire");
    }

    #[tokio::test]
    async fn test_grpc_acquire_different_mode_failed_precondition() {
        let svc = service();
        svc.create_resource(Request::new(create_request("diff"))).await.unwrap();
        let id = format!("{SERIAL}:diff");

        svc.acquire_resource(Request::new(proto::AcquireResourceRequest {
            resource_id: id.clone(),
            user_id: "alice".to_string(),
            mode: Some(proto::AccessMode::Exclusive as i32),
        }))
        .await
        .unwrap();

        // Same user requesting ReadOnly without releasing first -> FAILED_PRECONDITION + mode_mismatch.
        let status = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: id.clone(),
                user_id: "alice".to_string(),
                mode: Some(proto::AccessMode::ReadOnly as i32),
            }))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        let detail = decode_conflict_detail(&status).expect("conflict detail");
        assert_eq!(detail.code, "mode_mismatch");
        assert_eq!(detail.resource_id, id);

        // Requesting Shared on an Exclusive resource -> FAILED_PRECONDITION + mode_mismatch (D62).
        let status = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: id,
                user_id: "bob".to_string(),
                mode: Some(proto::AccessMode::Shared as i32),
            }))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert_eq!(decode_conflict_detail(&status).unwrap().code, "mode_mismatch");
    }

    #[tokio::test]
    async fn test_grpc_acquire_readonly_private_denied() {
        let svc = service();
        let mut req = create_request("priv");
        req.private = true;
        svc.create_resource(Request::new(req)).await.unwrap();
        let id = format!("{SERIAL}:priv");

        let status = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: id.clone(),
                user_id: "bob".to_string(),
                mode: Some(proto::AccessMode::ReadOnly as i32),
            }))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        let detail = decode_conflict_detail(&status).expect("conflict detail");
        assert_eq!(detail.code, "private");
        assert_eq!(detail.resource_id, id);
        assert_eq!(detail.resource_name, "priv");
    }

    #[tokio::test]
    async fn test_grpc_acquire_exclusive_with_readonly_holders_allowed() {
        let svc = service();
        svc.create_resource(Request::new(create_request("ro_excl"))).await.unwrap();
        let id = format!("{SERIAL}:ro_excl");

        // D25b: ReadOnly acquires available Exclusive resource -> InUseShared.
        let r1 = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: id.clone(),
                user_id: "bob".to_string(),
                mode: Some(proto::AccessMode::ReadOnly as i32),
            }))
            .await
            .unwrap()
            .into_inner()
            .resource
            .unwrap();
        assert_eq!(r1.state, proto::ResourceState::InUseShared as i32);
        assert!(r1.locked_by.is_empty());

        // D25c: Exclusive holder joins -> InUseLocked, locked_by = "alice".
        let r2 = svc
            .acquire_resource(Request::new(proto::AcquireResourceRequest {
                resource_id: id,
                user_id: "alice".to_string(),
                mode: None,
            }))
            .await
            .unwrap()
            .into_inner()
            .resource
            .unwrap();
        assert_eq!(r2.state, proto::ResourceState::InUseLocked as i32);
        assert_eq!(r2.locked_by, "alice");
        assert_eq!(r2.holders.len(), 2);
    }

    // ── Phase 44-D: gRPC Locking, Blocking & WatchDeviceEvents ──

    fn shared_create_request(name: &str, ep: u32) -> proto::CreateResourceRequest {
        let mut req = create_request(name);
        req.access_mode = Some(proto::ResourceAccessMode::Shared as i32);
        req.endpoints = Some(proto::EndpointDeclaration {
            default_access: Some(proto::ResourceAccessMode::Shared as i32),
            entries: vec![proto::EndpointEntry {
                address: ep,
                r#type: "wire_in".to_string(),
                access: Some(proto::ResourceAccessMode::Shared as i32),
                private: false,
                description: String::new(),
            }],
        });
        req
    }

    #[tokio::test]
    async fn test_grpc_lock_events_emitted() {
        let svc = service();
        svc.create_resource(Request::new(shared_create_request("lk_r", 0x01)))
            .await
            .unwrap();
        svc.create_resource(Request::new(shared_create_request("lk_s", 0x01)))
            .await
            .unwrap();

        let id_r = format!("{SERIAL}:lk_r");
        let id_s = format!("{SERIAL}:lk_s");

        svc.acquire_resource(Request::new(proto::AcquireResourceRequest {
            resource_id: id_r.clone(),
            user_id: "alice".to_string(),
            mode: None,
        }))
        .await
        .unwrap();
        svc.acquire_resource(Request::new(proto::AcquireResourceRequest {
            resource_id: id_s.clone(),
            user_id: "bob".to_string(),
            mode: None,
        }))
        .await
        .unwrap();

        let mut events = svc.state.device_manager.subscribe();

        // Alice locks lk_r -> emits ResourceChanged and ResourceLocked.
        let locked = svc
            .lock_resource(Request::new(proto::LockResourceRequest {
                resource_id: id_r.clone(),
                user_id: "alice".to_string(),
            }))
            .await
            .unwrap()
            .into_inner()
            .resource
            .unwrap();
        assert_eq!(locked.locked_by, "alice");
        assert_eq!(locked.state, proto::ResourceState::InUseLocked as i32);

        let mut saw_locked = false;
        while let Ok(evt) = events.try_recv() {
            if let DeviceEvent::ResourceLocked { resource_id, user_id, .. } = evt {
                assert_eq!(resource_id, id_r);
                assert_eq!(user_id, "alice");
                saw_locked = true;
            }
        }
        assert!(saw_locked, "gRPC LockResource must emit ResourceLocked");

        // 44-D Q2: Repeat lock by Alice succeeds idempotently with no duplicate event.
        let repeat = svc
            .lock_resource(Request::new(proto::LockResourceRequest {
                resource_id: id_r.clone(),
                user_id: "alice".to_string(),
            }))
            .await
            .unwrap()
            .into_inner()
            .resource
            .unwrap();
        assert_eq!(repeat.locked_by, "alice");
        assert!(events.try_recv().is_err(), "no event on idempotent repeat lock");

        // Bob tries to lock lk_s -> FAILED_PRECONDITION with ConflictCode::Blocked naming lk_r.
        let status = svc
            .lock_resource(Request::new(proto::LockResourceRequest {
                resource_id: id_s.clone(),
                user_id: "bob".to_string(),
            }))
            .await
            .unwrap_err();
        assert_eq!(status.code(), Code::FailedPrecondition);
        let detail = decode_conflict_detail(&status).expect("Blocked conflict detail");
        assert_eq!(detail.code, "blocked");
        assert_eq!(detail.resource_id, id_r);
        assert_eq!(detail.resource_name, "lk_r");
        assert!(detail.item.is_empty());
        assert!(detail.items.is_empty());

        // Alice unlocks lk_r -> emits ResourceUnlocked.
        svc.unlock_resource(Request::new(proto::LockResourceRequest {
            resource_id: id_r.clone(),
            user_id: "alice".to_string(),
        }))
        .await
        .unwrap();

        let mut saw_unlocked = false;
        while let Ok(evt) = events.try_recv() {
            if let DeviceEvent::ResourceUnlocked { resource_id, user_id, .. } = evt {
                assert_eq!(resource_id, id_r);
                assert_eq!(user_id, "alice");
                saw_unlocked = true;
            }
        }
        assert!(saw_unlocked, "gRPC UnlockResource must emit ResourceUnlocked");
    }

    #[tokio::test]
    async fn test_grpc_watch_device_events_streams_resource_events() {
        use tokio_stream::StreamExt;

        let svc = service();
        let mut stream = svc
            .watch_device_events(Request::new(proto::WatchDeviceEventsRequest {}))
            .await
            .unwrap()
            .into_inner();

        let id = format!("{SERIAL}:stream_res");

        // 1. Create resource -> ResourceCreated
        svc.create_resource(Request::new(shared_create_request("stream_res", 0x02)))
            .await
            .unwrap();
        let evt1 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt1.event,
            Some(proto::device_event::Event::ResourceCreated(ref e))
                if e.resource_id == id && e.name == "stream_res"
        ));

        // 2. Acquire resource -> ResourceStateChanged (InUseShared)
        svc.acquire_resource(Request::new(proto::AcquireResourceRequest {
            resource_id: id.clone(),
            user_id: "alice".to_string(),
            mode: None,
        }))
        .await
        .unwrap();
        let evt2 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt2.event,
            Some(proto::device_event::Event::ResourceStateChanged(ref e))
                if e.resource_id == id && e.new_state == proto::ResourceState::InUseShared as i32
        ));

        // 3. Lock resource -> ResourceStateChanged (InUseLocked) + ResourceLocked
        svc.lock_resource(Request::new(proto::LockResourceRequest {
            resource_id: id.clone(),
            user_id: "alice".to_string(),
        }))
        .await
        .unwrap();
        let evt3 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt3.event,
            Some(proto::device_event::Event::ResourceStateChanged(ref e))
                if e.resource_id == id && e.new_state == proto::ResourceState::InUseLocked as i32
        ));
        let evt4 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt4.event,
            Some(proto::device_event::Event::ResourceLocked(ref e))
                if e.resource_id == id && e.user_id == "alice"
        ));

        // 4. Unlock resource -> ResourceStateChanged (InUseShared) + ResourceUnlocked
        svc.unlock_resource(Request::new(proto::LockResourceRequest {
            resource_id: id.clone(),
            user_id: "alice".to_string(),
        }))
        .await
        .unwrap();
        let evt5 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt5.event,
            Some(proto::device_event::Event::ResourceStateChanged(ref e))
                if e.resource_id == id && e.new_state == proto::ResourceState::InUseShared as i32
        ));
        let evt6 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt6.event,
            Some(proto::device_event::Event::ResourceUnlocked(ref e))
                if e.resource_id == id && e.user_id == "alice"
        ));

        // 5. Release resource -> ResourceStateChanged (Available)
        svc.release_resource(Request::new(proto::ReleaseResourceRequest {
            resource_id: id.clone(),
            user_id: "alice".to_string(),
        }))
        .await
        .unwrap();
        let evt7 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt7.event,
            Some(proto::device_event::Event::ResourceStateChanged(ref e))
                if e.resource_id == id && e.new_state == proto::ResourceState::Available as i32
        ));

        // 6. Delete resource -> ResourceDeleted (Client)
        svc.delete_resource(Request::new(proto::ResourceRequest {
            resource_id: id.clone(),
        }))
        .await
        .unwrap();
        let evt8 = stream.next().await.unwrap().unwrap();
        assert!(matches!(
            evt8.event,
            Some(proto::device_event::Event::ResourceDeleted(ref e))
                if e.resource_id == id && e.reason == proto::DeleteReason::Client as i32
        ));
    }
}
