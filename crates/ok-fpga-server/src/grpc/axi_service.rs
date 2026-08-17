//! # AXIDataPortService gRPC Implementation
//!
//! Implements the `AXIDataPortService` from `okfpga.proto` (ADR-005 §3).
//! All RPCs require FFI integration and return `Unimplemented` until complete.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::grpc::proto;
use crate::state::AppState;

/// gRPC implementation of the `AXIDataPortService`.
pub struct AXIDataPortServiceImpl {
    #[allow(dead_code)]
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
        _request: Request<proto::AxiLiteReadRequest>,
    ) -> Result<Response<proto::AxiLiteReadResponse>, Status> {
        Err(Status::unimplemented(
            "AXILiteRead requires FFI integration",
        ))
    }

    async fn axi_lite_write(
        &self,
        _request: Request<proto::AxiLiteWriteRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "AXILiteWrite requires FFI integration",
        ))
    }

    async fn axi_lite_read_bulk(
        &self,
        _request: Request<proto::AxiLiteReadBulkRequest>,
    ) -> Result<Response<proto::AxiLiteReadBulkResponse>, Status> {
        Err(Status::unimplemented(
            "AXILiteReadBulk requires FFI integration",
        ))
    }

    async fn axi_lite_write_bulk(
        &self,
        _request: Request<proto::AxiLiteWriteBulkRequest>,
    ) -> Result<Response<proto::AxiOperationStats>, Status> {
        Err(Status::unimplemented(
            "AXILiteWriteBulk requires FFI integration",
        ))
    }

    async fn axi_lite_reset(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "AXILiteReset requires FFI integration",
        ))
    }

    // --- AXI Full ---

    async fn axi_full_get_datapath_width(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::DatapathWidthResponse>, Status> {
        Err(Status::unimplemented(
            "AXIFullGetDatapathWidth requires FFI integration",
        ))
    }

    async fn axi_full_read(
        &self,
        _request: Request<proto::AxiFullReadRequest>,
    ) -> Result<Response<proto::AxiFullReadResponse>, Status> {
        Err(Status::unimplemented(
            "AXIFullRead requires FFI integration",
        ))
    }

    async fn axi_full_write(
        &self,
        _request: Request<proto::AxiFullWriteRequest>,
    ) -> Result<Response<proto::AxiOperationStats>, Status> {
        Err(Status::unimplemented(
            "AXIFullWrite requires FFI integration",
        ))
    }

    async fn axi_full_reset(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "AXIFullReset requires FFI integration",
        ))
    }

    // --- AXI Stream ---

    async fn axi_stream_get_datapath_widths(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::AxiStreamDatapathWidthsResponse>, Status> {
        Err(Status::unimplemented(
            "AXIStreamGetDatapathWidths requires FFI integration",
        ))
    }

    async fn axi_stream_read(
        &self,
        _request: Request<proto::AxiStreamReadRequest>,
    ) -> Result<Response<proto::AxiStreamReadResponse>, Status> {
        Err(Status::unimplemented(
            "AXIStreamRead requires FFI integration",
        ))
    }

    async fn axi_stream_write(
        &self,
        _request: Request<proto::AxiStreamWriteRequest>,
    ) -> Result<Response<proto::AxiStreamWriteResponse>, Status> {
        Err(Status::unimplemented(
            "AXIStreamWrite requires FFI integration",
        ))
    }

    async fn axi_stream_reset(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "AXIStreamReset requires FFI integration",
        ))
    }
}
