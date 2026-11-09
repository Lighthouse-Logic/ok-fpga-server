// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # ClassicDataPortService gRPC Implementation
//!
//! Implements the `ClassicDataPortService` from `okfpga.proto` (ADR-005 §2).
//! All RPCs are wired to the FrontPanel DLL via the `DeviceHandlePool` in
//! `AppState`, mirroring the REST handlers in `rest/classic.rs`.

use std::sync::Arc;

use tonic::{Request, Response, Status};
use tracing::{debug, warn};

use ok_fpga_core::resource::{AccessDenied, EndpointType, ResourceError};
use ok_fpga_ffi::error::FfiError;

use crate::grpc::proto;
use crate::grpc::resource_mapping::resource_error_status;
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
    pub(crate) fn extract_user_id<T>(request: &Request<T>) -> Option<String> {
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
    /// - `value`: The wire value / trigger mask (if available). Passed to the
    ///   developer's `resolve_classic_address()` hook (ADR-025).
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
        value: Option<u32>,
    ) -> Result<(), Status> {
        // Fast path: no enforcement.
        if !self.state.enforcement().is_active() {
            return Ok(());
        }

        // User identity is required when enforcement is active.
        let uid = match user_id {
            Some(uid) => uid,
            None => {
                match self.state.enforcement() {
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

        // Check endpoint access in the resource registry. Reject out-of-u8
        // endpoint values instead of wrapping `endpoint as u8` (e.g. 0x100 -> 0x00).
        let check_res = match u8::try_from(endpoint) {
            Ok(ep_u8) => self
                .state
                .resource_registry
                .check_endpoint_access(uid, serial, ep_u8, ep_type, value),
            Err(_) => Err(AccessDenied::NoCoveringResource {
                user_id: uid.to_string(),
                device_serial: serial.to_string(),
                detail: format!("endpoint {} ({})", endpoint, ep_type),
            }),
        };
        if let Err(denied) = check_res {
            match self.state.enforcement() {
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
        is_write: bool,
    ) -> Result<(), Status> {
        if !self.state.enforcement().is_active() {
            return Ok(());
        }

        let uid = match user_id {
            Some(uid) => uid,
            None => {
                match self.state.enforcement() {
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
            is_write,
        ) {
            match self.state.enforcement() {
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

    /// Run a device-level enforcement check (`check_device_access`,
    /// `check_device_write_access`, `check_reset_access`,
    /// `check_trigger_out_update_access`) against the extracted gRPC caller
    /// identity (ADR-034 §7, S-15).
    pub(crate) fn check_with_user<F>(
        state: &AppState,
        user_id: Option<&str>,
        serial: &str,
        op: &str,
        check: F,
    ) -> Result<(), Status>
    where
        F: FnOnce(&str) -> Result<(), AccessDenied>,
    {
        if !state.enforcement().is_active() {
            return Ok(());
        }

        let uid = match user_id {
            Some(uid) => uid,
            None => match state.enforcement() {
                EndpointEnforcement::Warn => {
                    warn!(
                        serial = %serial,
                        op = %op,
                        "gRPC access enforcement (warn): x-user-id metadata missing"
                    );
                    return Ok(());
                }
                EndpointEnforcement::Enforce => {
                    return Err(Status::permission_denied(
                        "User identity required: x-user-id metadata must be present when endpoint enforcement is active",
                    ));
                }
                EndpointEnforcement::Off => unreachable!(),
            },
        };

        if let Err(denied) = check(uid) {
            match state.enforcement() {
                EndpointEnforcement::Warn => {
                    warn!(
                        user_id = %uid,
                        serial = %serial,
                        op = %op,
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
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        Self::check_with_user(&self.state, user_id.as_deref(), &req.serial, "ResetFPGA", |uid| {
            self.state
                .resource_registry
                .check_reset_access(uid, &req.serial, "ResetFPGA")
        })?;
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
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::WireIn, Some(req.value as u32))?;
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
        // `GetWireValueRequest` is shared with GetWireOutValue. Its `update`
        // field is ignored here on purpose: WireIns live in the host-side
        // buffer, so there is no FPGA snapshot to refresh (D76).
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::WireIn, None)?;
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
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        Self::check_with_user(&self.state, user_id.as_deref(), &req.serial, "UpdateWireIns", |uid| {
            self.state
                .resource_registry
                .check_device_write_access(uid, &req.serial, "UpdateWireIns")
        })?;
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
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::WireOut, None)?;
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

        // Opal Kelly semantics (ADR-034 §9, D76): read the device snapshot;
        // refresh it first only when the client sets `update`. Both steps
        // run under the device lock held by `device`.
        let value = crate::snapshot::read_snapshot(
            req.update.unwrap_or(false),
            || FfiError::from_code(unsafe { (dll.classic_update_wire_outs)(classic) })
                .map_err(|e| Self::ffi_to_status(&e)),
            || unsafe { (dll.classic_get_wire_out_value)(classic, req.endpoint) as u32 },
        )?;

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
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        Self::check_with_user(&self.state, user_id.as_deref(), &req.serial, "UpdateWireOuts", |uid| {
            self.state
                .resource_registry
                .check_device_access(uid, &req.serial)
        })?;
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
        // Validate `bit` in 0..=31 before computing `1u32 << req.bit` and
        // before running enforcement so invalid bit indices never panic in
        // debug builds or silently wrap in release builds.
        if !(0..=31).contains(&req.bit) {
            return Err(resource_error_status(&ResourceError::validation(format!(
                "TriggerIn bit index must be in 0..=31 (got {})",
                req.bit
            ))));
        }
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::TriggerIn, Some(1u32 << req.bit))?;
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
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        // D75 F2: UpdateTriggerOuts clears latches device-wide; ReadOnly
        // holders are denied when any TriggerOut on the device is private.
        Self::check_with_user(&self.state, user_id.as_deref(), &req.serial, "UpdateTriggerOuts", |uid| {
            self.state
                .resource_registry
                .check_trigger_out_update_access(uid, &req.serial)
        })?;
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
        let do_update = req.update.unwrap_or(false);
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::TriggerOut, None)?;
        if do_update {
            Self::check_with_user(&self.state, user_id.as_deref(), &req.serial, "IsTriggered(update=true)", |uid| {
                self.state
                    .resource_registry
                    .check_trigger_out_update_access(uid, &req.serial)
            })?;
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

        // Snapshot read (D76). With `update`, UpdateTriggerOuts clears every
        // TriggerOut latch on the device for all clients.
        let triggered = crate::snapshot::read_snapshot(
            do_update,
            || FfiError::from_code(unsafe { (dll.classic_update_trigger_outs)(classic) })
                .map_err(|e| Self::ffi_to_status(&e)),
            || unsafe {
                (dll.classic_is_triggered)(classic, req.endpoint, req.mask as std::os::raw::c_ulong)
                    != 0
            },
        )?;

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
        let do_update = req.update.unwrap_or(false);
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::TriggerOut, None)?;
        if do_update {
            Self::check_with_user(&self.state, user_id.as_deref(), &req.serial, "GetTriggerOutVector(update=true)", |uid| {
                self.state
                    .resource_registry
                    .check_trigger_out_update_access(uid, &req.serial)
            })?;
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

        // Snapshot read (D76), same rules as `is_triggered`.
        let vector = crate::snapshot::read_snapshot(
            do_update,
            || FfiError::from_code(unsafe { (dll.classic_update_trigger_outs)(classic) })
                .map_err(|e| Self::ffi_to_status(&e)),
            || unsafe { (dll.classic_get_trigger_out_vector)(classic, req.endpoint) },
        )?;

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
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::PipeIn, None)?;
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
        if req.length < 0 {
            return Err(Status::invalid_argument(format!(
                "length must be >= 0 (got {})",
                req.length
            )));
        }
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::PipeOut, None)?;
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
        if req.block_size <= 0 {
            return Err(Status::invalid_argument(format!(
                "block_size must be > 0 (got {})",
                req.block_size
            )));
        }
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::BtPipeIn, None)?;
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
        if req.block_size <= 0 {
            return Err(Status::invalid_argument(format!(
                "block_size must be > 0 (got {})",
                req.block_size
            )));
        }
        if req.length < 0 {
            return Err(Status::invalid_argument(format!(
                "length must be >= 0 (got {})",
                req.length
            )));
        }
        self.check_access(user_id.as_deref(), &req.serial, req.endpoint, &EndpointType::BtPipeOut, None)?;
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
        let user_id = Self::extract_user_id(&request);
        let req = request.into_inner();
        Self::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "SetBTPipePollingInterval",
            |uid| {
                self.state
                    .resource_registry
                    .check_device_write_access(uid, &req.serial, "SetBTPipePollingInterval")
            },
        )?;
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
        self.check_address(user_id.as_deref(), &req.serial, req.address, false)?;
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
        self.check_address(user_id.as_deref(), &req.serial, req.address, true)?;
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
        // Verify access on every requested register address before touching
        // hardware.
        for addr in &req.addresses {
            self.check_address(user_id.as_deref(), &req.serial, *addr, false)?;
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
        // Verify write access on every entry's address before touching
        // hardware.
        for entry in &req.entries {
            self.check_address(user_id.as_deref(), &req.serial, entry.address, true)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::proto::classic_data_port_service_server::ClassicDataPortService;
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::{
        AccessMode, AddressRange, EndpointDeclaration, EndpointEntry, Resource, ResourceRegistry,
    };
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tonic::Code;

    fn test_service() -> ClassicDataPortServiceImpl {
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        let state = Arc::new(AppState::new(
            Arc::new(DeviceManager::new()),
            Arc::new(ResourceRegistry::new()),
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            Arc::new(ok_fpga_core::auth::SessionManager::new(3600)),
            Arc::new(UserSessionRegistry::new()),
            crate::state::EndpointEnforcement::Enforce,
            crate::config::ServerConfig::default(),
            None,
        ));
        ClassicDataPortServiceImpl::new(state)
    }

    fn req_with_user<T>(inner: T, user_id: &str) -> Request<T> {
        let mut req = Request::new(inner);
        req.metadata_mut()
            .insert("x-user-id", user_id.parse().unwrap());
        req
    }

    #[tokio::test]
    async fn test_grpc_classic_pipe_register_parity() {
        let svc = test_service();
        let mut r = Resource::new(
            "SN1:pipe-reg".into(),
            "pipe-reg".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x80,
                    ep_type: EndpointType::PipeIn,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0xA0,
                    ep_type: EndpointType::PipeOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
            ],
        });
        r.address_ranges = vec![AddressRange {
            name: None,
            description: None,
            start: 0x0000,
            end: 0x000F,
            access: AccessMode::Shared,
            private: false,
        }];
        assert!(svc.state.resource_registry.register(r));
        svc.state
            .resource_registry
            .acquire("SN1:pipe-reg", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // 1. Negative pipe length / non-positive block_size -> InvalidArgument.
        let err = svc
            .read_from_pipe_out(req_with_user(
                proto::ReadFromPipeOutRequest {
                    serial: "SN1".into(),
                    endpoint: 0xA0,
                    length: -1,
                },
                "alice",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);

        let err = svc
            .write_to_block_pipe_in(req_with_user(
                proto::WriteToBlockPipeInRequest {
                    serial: "SN1".into(),
                    endpoint: 0x80,
                    block_size: 0,
                    data: vec![1, 2, 3, 4],
                },
                "alice",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);

        // 2. Covered PipeIn / PipeOut passes enforcement -> Unavailable (no DLL pool).
        let err = svc
            .write_to_pipe_in(req_with_user(
                proto::WriteToPipeInRequest {
                    serial: "SN1".into(),
                    endpoint: 0x80,
                    data: vec![1, 2, 3, 4],
                },
                "alice",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::Unavailable);

        let err = svc
            .read_from_pipe_out(req_with_user(
                proto::ReadFromPipeOutRequest {
                    serial: "SN1".into(),
                    endpoint: 0xA0,
                    length: 16,
                },
                "alice",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::Unavailable);

        // 3. Bulk registers: covered batch passes enforcement -> Unavailable;
        // batch containing an uncovered address fails BEFORE pool() -> PermissionDenied.
        let err = svc
            .read_registers(req_with_user(
                proto::ReadRegistersRequest {
                    serial: "SN1".into(),
                    addresses: vec![0x0000, 0x0004],
                },
                "alice",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::Unavailable);

        let err = svc
            .write_registers(req_with_user(
                proto::WriteRegistersRequest {
                    serial: "SN1".into(),
                    entries: vec![
                        proto::RegisterEntry {
                            address: 0x0000,
                            data: 1,
                        },
                        proto::RegisterEntry {
                            address: 0x0020,
                            data: 2,
                        },
                    ],
                },
                "alice",
            ))
            .await
            .unwrap_err();
        assert_eq!(err.code(), Code::PermissionDenied);
    }
}
