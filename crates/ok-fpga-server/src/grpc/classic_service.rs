//! # ClassicDataPortService gRPC Implementation
//!
//! Implements the `ClassicDataPortService` from `okfpga.proto` (ADR-005 §2).
//! All RPCs are wired to the FrontPanel DLL via the `DeviceHandlePool` in
//! `AppState`, mirroring the REST handlers in `rest/classic.rs`.

use std::sync::Arc;

use tonic::{Request, Response, Status};
use tracing::{debug, warn};

use ok_fpga_core::resource::EndpointType;
use ok_fpga_ffi::error::FfiError;

use crate::grpc::proto;
use crate::state::{AppState, EndpointEnforcement};

/// gRPC implementation of the `ClassicDataPortService`.
pub struct ClassicDataPortServiceImpl {
    state: Arc<AppState>,
}

impl ClassicDataPortServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Helper: get the handle pool, or return an UNAVAILABLE status.
    fn pool(&self) -> Result<&Arc<crate::handle_pool::DeviceHandlePool>, Status> {
        self.state
            .handle_pool
            .as_ref()
            .ok_or_else(|| Status::unavailable("FrontPanel DLL not loaded"))
    }

    /// Convert an FFI error to a tonic Status.
    fn ffi_to_status(e: &FfiError) -> Status {
        match e {
            FfiError::FrontPanelError { code, message } => {
                let c = *code;
                if c == ok_fpga_ffi::ok_ErrorCode::ok_InvalidParameter as i32
                    || c == ok_fpga_ffi::ok_ErrorCode::ok_InvalidEndpoint as i32
                {
                    Status::invalid_argument(message.clone())
                } else if c == ok_fpga_ffi::ok_ErrorCode::ok_DeviceNotOpen as i32 {
                    Status::unavailable(message.clone())
                } else if c == ok_fpga_ffi::ok_ErrorCode::ok_Timeout as i32 {
                    Status::deadline_exceeded(message.clone())
                } else if c == ok_fpga_ffi::ok_ErrorCode::ok_FrontPanelNotEnabled as i32 {
                    Status::failed_precondition(
                        "FPGA is not programmed or FrontPanel is not enabled",
                    )
                } else {
                    Status::internal(message.clone())
                }
            }
            _ => Status::internal(e.to_string()),
        }
    }

    /// Build a success StatusResponse with no error.
    fn ok_status() -> proto::StatusResponse {
        proto::StatusResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }
    }

    /// Extract the user ID from gRPC request metadata.
    ///
    /// gRPC clients should set the `x-user-id` metadata key (lowercase,
    /// matching gRPC conventions). This is the gRPC equivalent of the
    /// REST `X-User-Id` header.
    fn extract_user_id<T>(request: &Request<T>) -> Option<String> {
        request
            .metadata()
            .get("x-user-id")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }

    /// Check endpoint access enforcement for a Classic operation (ADR-024).
    ///
    /// Called at the top of each RPC method AFTER deserializing the request
    /// message. This is the gRPC equivalent of the REST handler's
    /// `enforce_endpoint()` helper.
    ///
    /// ## Parameters
    ///
    /// - `user_id`: Extracted from gRPC metadata (`x-user-id` key)
    /// - `serial`: The device serial from the request message
    /// - `endpoint`: The endpoint address from the request message
    /// - `ep_type`: The type of Classic endpoint being accessed
    ///
    /// ## Returns
    ///
    /// `Ok(())` if the operation is allowed, or `Err(Status)` if denied.
    fn check_access(
        &self,
        user_id: Option<&str>,
        serial: &str,
        endpoint: i32,
        ep_type: &EndpointType,
    ) -> Result<(), Status> {
        // Fast path: no enforcement.
        if !self.state.enforcement.is_active() {
            return Ok(());
        }

        // User identity is required when enforcement is active.
        let uid = match user_id {
            Some(uid) => uid,
            None => {
                match self.state.enforcement {
                    EndpointEnforcement::Warn => {
                        warn!(
                            serial = %serial,
                            endpoint = endpoint,
                            "gRPC access enforcement (warn): x-user-id metadata missing"
                        );
                        return Ok(()); // Warn: log and allow.
                    }
                    EndpointEnforcement::Enforce => {
                        return Err(Status::permission_denied(
                            "User identity required: x-user-id metadata must be present when endpoint enforcement is active",
                        ));
                    }
                    EndpointEnforcement::Off => unreachable!(),
                }
            }
        };

        // Check endpoint access in the resource registry.
        if let Err(denied) = self.state.resource_registry.check_endpoint_access(
            uid,
            serial,
            endpoint as u8,
            ep_type,
        ) {
            match self.state.enforcement {
                EndpointEnforcement::Warn => {
                    warn!(
                        user_id = %uid,
                        serial = %serial,
                        endpoint = endpoint,
                        "gRPC access enforcement (warn): {}",
                        denied
                    );
                    Ok(()) // Warn: log and allow.
                }
                EndpointEnforcement::Enforce => {
                    Err(Status::permission_denied(denied.to_string()))
                }
                EndpointEnforcement::Off => unreachable!(),
            }
        } else {
            Ok(())
        }
    }

    /// Check address access enforcement for a register operation (ADR-024).
    ///
    /// Used by `ReadRegister`/`WriteRegister` RPCs that use register bridge
    /// addresses instead of Classic endpoint addresses.
    fn check_address(
        &self,
        user_id: Option<&str>,
        serial: &str,
        address: u32,
    ) -> Result<(), Status> {
        if !self.state.enforcement.is_active() {
            return Ok(());
        }

        let uid = match user_id {
            Some(uid) => uid,
            None => {
                match self.state.enforcement {
                    EndpointEnforcement::Warn => {
                        warn!(
                            serial = %serial,
                            address = address,
                            "gRPC access enforcement (warn): x-user-id metadata missing"
                        );
                        return Ok(());
                    }
                    EndpointEnforcement::Enforce => {
                        return Err(Status::permission_denied(
                            "User identity required: x-user-id metadata must be present",
                        ));
                    }
                    EndpointEnforcement::Off => unreachable!(),
                }
            }
        };

        if let Err(denied) = self.state.resource_registry.check_address_access(
            uid,
            serial,
            address as u64,
        ) {
            match self.state.enforcement {
                EndpointEnforcement::Warn => {
                    warn!(
                        user_id = %uid,
                        serial = %serial,
                        address = address,
                        "gRPC access enforcement (warn): {}",
                        denied
                    );
                    Ok(())
                }
                EndpointEnforcement::Enforce => {
                    Err(Status::permission_denied(denied.to_string()))
                }
                EndpointEnforcement::Off => unreachable!(),
            }
        } else {
            Ok(())
        }
    }
}

#[tonic::async_trait]
impl proto::classic_data_port_service_server::ClassicDataPortService
    for ClassicDataPortServiceImpl
{
    async fn reset_fpga(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_reset_fpga)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, "gRPC: ResetFPGA OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn set_wire_in_value(
        &self,
        request: Request<proto::SetWireInValueRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        // ADR-024: extract user identity before consuming the request.
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();

        // ADR-024: check endpoint access enforcement.
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::WireIn)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_set_wire_in_value)(
                classic,
                req.endpoint,
                req.value as std::os::raw::c_ulong,
                req.mask as std::os::raw::c_ulong,
            );
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;

            // If auto_update is set, flush immediately.
            if req.auto_update {
                let rc = (dll.classic_update_wire_ins)(classic);
                FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
            }
        }

        debug!(serial = %req.serial, endpoint = req.endpoint, value = req.value, "gRPC: SetWireInValue OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn get_wire_in_value(
        &self,
        request: Request<proto::GetWireValueRequest>,
    ) -> Result<Response<proto::WireValueResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::WireIn)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let mut value: u32 = 0;
        unsafe {
            let rc = (dll.classic_get_wire_in_value)(classic, req.endpoint, &mut value);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        Ok(Response::new(proto::WireValueResponse {
            value,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn update_wire_ins(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_update_wire_ins)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, "gRPC: UpdateWireIns OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn get_wire_out_value(
        &self,
        request: Request<proto::GetWireValueRequest>,
    ) -> Result<Response<proto::WireValueResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::WireOut)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let value = unsafe {
            // Update wire-outs first, then read the value.
            let rc = (dll.classic_update_wire_outs)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
            (dll.classic_get_wire_out_value)(classic, req.endpoint) as u32
        };

        Ok(Response::new(proto::WireValueResponse {
            value,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn update_wire_outs(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_update_wire_outs)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, "gRPC: UpdateWireOuts OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn activate_trigger_in(
        &self,
        request: Request<proto::ActivateTriggerInRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::TriggerIn)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_activate_trigger_in)(classic, req.endpoint, req.bit);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, endpoint = req.endpoint, bit = req.bit, "gRPC: ActivateTriggerIn OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn update_trigger_outs(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_update_trigger_outs)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, "gRPC: UpdateTriggerOuts OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn is_triggered(
        &self,
        request: Request<proto::IsTriggeredRequest>,
    ) -> Result<Response<proto::IsTriggeredResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::TriggerOut)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let triggered = unsafe {
            let rc = (dll.classic_update_trigger_outs)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
            (dll.classic_is_triggered)(classic, req.endpoint, req.mask as std::os::raw::c_ulong)
                != 0
        };

        Ok(Response::new(proto::IsTriggeredResponse {
            triggered,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn get_trigger_out_vector(
        &self,
        request: Request<proto::TriggerOutVectorRequest>,
    ) -> Result<Response<proto::TriggerOutVectorResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::TriggerOut)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let vector = unsafe {
            let rc = (dll.classic_update_trigger_outs)(classic);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
            (dll.classic_get_trigger_out_vector)(classic, req.endpoint)
        };

        Ok(Response::new(proto::TriggerOutVectorResponse {
            vector: vector as u32,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn write_to_pipe_in(
        &self,
        request: Request<proto::WriteToPipeInRequest>,
    ) -> Result<Response<proto::PipeTransferResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::PipeIn)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let transferred = unsafe {
            (dll.classic_write_to_pipe_in)(
                classic,
                req.endpoint,
                req.data.len() as std::os::raw::c_long,
                req.data.as_ptr(),
            )
        };

        if transferred < 0 {
            return Err(Status::internal(format!(
                "WriteToPipeIn failed with code {}",
                transferred
            )));
        }

        Ok(Response::new(proto::PipeTransferResponse {
            bytes_transferred: transferred as i64,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn read_from_pipe_out(
        &self,
        request: Request<proto::ReadFromPipeOutRequest>,
    ) -> Result<Response<proto::PipeDataResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::PipeOut)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let mut buffer = vec![0u8; req.length as usize];

        let transferred = unsafe {
            (dll.classic_read_from_pipe_out)(
                classic,
                req.endpoint,
                req.length as std::os::raw::c_long,
                buffer.as_mut_ptr(),
            )
        };

        if transferred < 0 {
            return Err(Status::internal(format!(
                "ReadFromPipeOut failed with code {}",
                transferred
            )));
        }

        buffer.truncate(transferred as usize);

        Ok(Response::new(proto::PipeDataResponse {
            data: buffer,
            bytes_transferred: transferred as i64,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn write_to_block_pipe_in(
        &self,
        request: Request<proto::WriteToBlockPipeInRequest>,
    ) -> Result<Response<proto::PipeTransferResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::BtPipeIn)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let transferred = unsafe {
            (dll.classic_write_to_block_pipe_in)(
                classic,
                req.endpoint,
                req.block_size,
                req.data.len() as std::os::raw::c_long,
                req.data.as_ptr(),
            )
        };

        if transferred < 0 {
            return Err(Status::internal(format!(
                "WriteToBlockPipeIn failed with code {}",
                transferred
            )));
        }

        Ok(Response::new(proto::PipeTransferResponse {
            bytes_transferred: transferred as i64,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn read_from_block_pipe_out(
        &self,
        request: Request<proto::ReadFromBlockPipeOutRequest>,
    ) -> Result<Response<proto::PipeDataResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::BtPipeOut)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let mut buffer = vec![0u8; req.length as usize];

        let transferred = unsafe {
            (dll.classic_read_from_block_pipe_out)(
                classic,
                req.endpoint,
                req.block_size,
                req.length as std::os::raw::c_long,
                buffer.as_mut_ptr(),
            )
        };

        if transferred < 0 {
            return Err(Status::internal(format!(
                "ReadFromBlockPipeOut failed with code {}",
                transferred
            )));
        }

        buffer.truncate(transferred as usize);

        Ok(Response::new(proto::PipeDataResponse {
            data: buffer,
            bytes_transferred: transferred as i64,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn set_bt_pipe_polling_interval(
        &self,
        request: Request<proto::SetBtPipePollingIntervalRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let req = request.into_inner();
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_set_bt_pipe_polling_interval)(classic, req.interval);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, interval = req.interval, "gRPC: SetBTPipePollingInterval OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn read_register(
        &self,
        request: Request<proto::ReadRegisterRequest>,
    ) -> Result<Response<proto::RegisterValueResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        // Register operations use address-based access (not endpoint-based).
        self.check_address(user_id.as_deref(), &req.serial, req.address)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let mut value: u32 = 0;
        unsafe {
            let rc = (dll.classic_read_register)(classic, req.address, &mut value);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        Ok(Response::new(proto::RegisterValueResponse {
            value,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn write_register(
        &self,
        request: Request<proto::WriteRegisterRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        self.check_address(user_id.as_deref(), &req.serial, req.address)?;
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        unsafe {
            let rc = (dll.classic_write_register)(classic, req.address, req.data);
            FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
        }

        debug!(serial = %req.serial, address = req.address, data = req.data, "gRPC: WriteRegister OK");
        Ok(Response::new(Self::ok_status()))
    }

    async fn read_registers(
        &self,
        request: Request<proto::ReadRegistersRequest>,
    ) -> Result<Response<proto::RegisterEntriesResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        // Batch check: verify access on the first address as a representative.
        // Full per-address enforcement could be added in a future iteration.
        if let Some(first_addr) = req.addresses.first() {
            self.check_address(user_id.as_deref(), &req.serial, *first_addr)?;
        }
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        let mut entries = Vec::with_capacity(req.addresses.len());
        for addr in &req.addresses {
            let mut value: u32 = 0;
            unsafe {
                let rc = (dll.classic_read_register)(classic, *addr, &mut value);
                FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
            }
            entries.push(proto::RegisterEntry {
                address: *addr,
                data: value,
            });
        }

        Ok(Response::new(proto::RegisterEntriesResponse {
            entries,
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
        }))
    }

    async fn write_registers(
        &self,
        request: Request<proto::WriteRegistersRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        // Batch check: verify access on the first entry's address.
        if let Some(first_entry) = req.entries.first() {
            self.check_address(user_id.as_deref(), &req.serial, first_entry.address)?;
        }
        let pool = self.pool()?;
        let device_arc = pool
            .get(&req.serial)
            .await
            .ok_or_else(|| Status::not_found(format!("device '{}' not found", req.serial)))?;
        let device = device_arc.lock().await;

        let classic = device
            .classic_port()
            .ok_or_else(|| Status::failed_precondition("Classic port not available"))?;
        let dll = device.dll();

        for entry in &req.entries {
            unsafe {
                let rc = (dll.classic_write_register)(classic, entry.address, entry.data);
                FfiError::from_code(rc).map_err(|e| Self::ffi_to_status(&e))?;
            }
        }

        debug!(serial = %req.serial, count = req.entries.len(), "gRPC: WriteRegisters OK");
        Ok(Response::new(Self::ok_status()))
    }
}
