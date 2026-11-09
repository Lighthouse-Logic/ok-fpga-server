// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # AXIDataPortService gRPC Implementation
//!
//! Implements the `AXIDataPortService` from `okfpga.proto` (ADR-005 §3).
//! All RPCs require FFI integration and return `Unimplemented` until complete.
//!
//! ## ADR-024 / ADR-034 Enforcement (S-15)
//!
//! Each RPC extracts `x-user-id` from metadata and runs access enforcement
//! (`check_address_span_access`, `check_reset_access`, `check_device_access`,
//! or `check_device_write_access`) before returning `Unimplemented`. This
//! matches the REST AXI handlers in `rest/axi.rs`.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::grpc::classic_service::ClassicDataPortServiceImpl;
use crate::grpc::proto;
use crate::state::AppState;

/// gRPC implementation of the `AXIDataPortService`.
pub struct AXIDataPortServiceImpl {
    state: Arc<AppState>,
}

impl AXIDataPortServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl proto::axi_data_port_service_server::AxiDataPortService for AXIDataPortServiceImpl {
    // --- AXI Lite ---

    async fn axi_lite_read(
        &self,
        request: Request<proto::AxiLiteReadRequest>,
    ) -> Result<Response<proto::AxiLiteReadResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXILiteRead",
            |uid| {
                self.state.resource_registry.check_address_span_access(
                    uid,
                    &req.serial,
                    req.address,
                    4,
                    false,
                )
            },
        )?;
        Err(Status::unimplemented(
            "AXILiteRead requires FFI integration",
        ))
    }

    async fn axi_lite_write(
        &self,
        request: Request<proto::AxiLiteWriteRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXILiteWrite",
            |uid| {
                self.state.resource_registry.check_address_span_access(
                    uid,
                    &req.serial,
                    req.address,
                    4,
                    true,
                )
            },
        )?;
        Err(Status::unimplemented(
            "AXILiteWrite requires FFI integration",
        ))
    }

    async fn axi_lite_read_bulk(
        &self,
        request: Request<proto::AxiLiteReadBulkRequest>,
    ) -> Result<Response<proto::AxiLiteReadBulkResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        let byte_len = req.count.saturating_mul(4).max(1);
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXILiteReadBulk",
            |uid| {
                self.state.resource_registry.check_address_span_access(
                    uid,
                    &req.serial,
                    req.address,
                    byte_len,
                    false,
                )
            },
        )?;
        Err(Status::unimplemented(
            "AXILiteReadBulk requires FFI integration",
        ))
    }

    async fn axi_lite_write_bulk(
        &self,
        request: Request<proto::AxiLiteWriteBulkRequest>,
    ) -> Result<Response<proto::AxiOperationStats>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        let byte_len = (req.values.len() as u64).saturating_mul(4).max(1);
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXILiteWriteBulk",
            |uid| {
                self.state.resource_registry.check_address_span_access(
                    uid,
                    &req.serial,
                    req.address,
                    byte_len,
                    true,
                )
            },
        )?;
        Err(Status::unimplemented(
            "AXILiteWriteBulk requires FFI integration",
        ))
    }

    async fn axi_lite_reset(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXILiteReset",
            |uid| {
                self.state
                    .resource_registry
                    .check_reset_access(uid, &req.serial, "AXILiteReset")
            },
        )?;
        Err(Status::unimplemented(
            "AXILiteReset requires FFI integration",
        ))
    }

    // --- AXI Full ---

    async fn axi_full_get_datapath_width(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DatapathWidthResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIFullGetDatapathWidth",
            |uid| {
                self.state
                    .resource_registry
                    .check_device_access(uid, &req.serial)
            },
        )?;
        Err(Status::unimplemented(
            "AXIFullGetDatapathWidth requires FFI integration",
        ))
    }

    async fn axi_full_read(
        &self,
        request: Request<proto::AxiFullReadRequest>,
    ) -> Result<Response<proto::AxiFullReadResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIFullRead",
            |uid| {
                self.state.resource_registry.check_address_span_access(
                    uid,
                    &req.serial,
                    req.address,
                    req.length.max(1),
                    false,
                )
            },
        )?;
        Err(Status::unimplemented(
            "AXIFullRead requires FFI integration",
        ))
    }

    async fn axi_full_write(
        &self,
        request: Request<proto::AxiFullWriteRequest>,
    ) -> Result<Response<proto::AxiOperationStats>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIFullWrite",
            |uid| {
                self.state.resource_registry.check_address_span_access(
                    uid,
                    &req.serial,
                    req.address,
                    (req.data.len() as u64).max(1),
                    true,
                )
            },
        )?;
        Err(Status::unimplemented(
            "AXIFullWrite requires FFI integration",
        ))
    }

    async fn axi_full_reset(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIFullReset",
            |uid| {
                self.state
                    .resource_registry
                    .check_reset_access(uid, &req.serial, "AXIFullReset")
            },
        )?;
        Err(Status::unimplemented(
            "AXIFullReset requires FFI integration",
        ))
    }

    // --- AXI Stream ---

    async fn axi_stream_get_datapath_widths(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::AxiStreamDatapathWidthsResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIStreamGetDatapathWidths",
            |uid| {
                self.state
                    .resource_registry
                    .check_device_access(uid, &req.serial)
            },
        )?;
        Err(Status::unimplemented(
            "AXIStreamGetDatapathWidths requires FFI integration",
        ))
    }

    async fn axi_stream_read(
        &self,
        request: Request<proto::AxiStreamReadRequest>,
    ) -> Result<Response<proto::AxiStreamReadResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIStreamRead",
            |uid| {
                self.state
                    .resource_registry
                    .check_device_access(uid, &req.serial)
            },
        )?;
        Err(Status::unimplemented(
            "AXIStreamRead requires FFI integration",
        ))
    }

    async fn axi_stream_write(
        &self,
        request: Request<proto::AxiStreamWriteRequest>,
    ) -> Result<Response<proto::AxiStreamWriteResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIStreamWrite",
            |uid| {
                self.state
                    .resource_registry
                    .check_device_write_access(uid, &req.serial, "AXIStreamWrite")
            },
        )?;
        Err(Status::unimplemented(
            "AXIStreamWrite requires FFI integration",
        ))
    }

    async fn axi_stream_reset(
        &self,
        request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        let user_id = ClassicDataPortServiceImpl::extract_user_id(&request);
        let req = request.into_inner();
        ClassicDataPortServiceImpl::check_with_user(
            &self.state,
            user_id.as_deref(),
            &req.serial,
            "AXIStreamReset",
            |uid| {
                self.state
                    .resource_registry
                    .check_reset_access(uid, &req.serial, "AXIStreamReset")
            },
        )?;
        Err(Status::unimplemented(
            "AXIStreamReset requires FFI integration",
        ))
    }
}
