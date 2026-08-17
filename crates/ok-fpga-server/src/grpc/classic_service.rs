//! # ClassicDataPortService gRPC Implementation
//!
//! Implements the `ClassicDataPortService` from `okfpga.proto` (ADR-005 §2).
//! All RPCs require FFI integration and return `Unimplemented` until complete.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::grpc::proto;
use crate::state::AppState;

/// gRPC implementation of the `ClassicDataPortService`.
pub struct ClassicDataPortServiceImpl {
    #[allow(dead_code)]
    state: Arc<AppState>,
}

impl ClassicDataPortServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl proto::classic_data_port_service_server::ClassicDataPortService
    for ClassicDataPortServiceImpl
{
    async fn reset_fpga(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented("ResetFPGA requires FFI integration"))
    }

    async fn set_wire_in_value(
        &self,
        _request: Request<proto::SetWireInValueRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "SetWireInValue requires FFI integration",
        ))
    }

    async fn get_wire_in_value(
        &self,
        _request: Request<proto::GetWireValueRequest>,
    ) -> Result<Response<proto::WireValueResponse>, Status> {
        Err(Status::unimplemented(
            "GetWireInValue requires FFI integration",
        ))
    }

    async fn update_wire_ins(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "UpdateWireIns requires FFI integration",
        ))
    }

    async fn get_wire_out_value(
        &self,
        _request: Request<proto::GetWireValueRequest>,
    ) -> Result<Response<proto::WireValueResponse>, Status> {
        Err(Status::unimplemented(
            "GetWireOutValue requires FFI integration",
        ))
    }

    async fn update_wire_outs(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "UpdateWireOuts requires FFI integration",
        ))
    }

    async fn activate_trigger_in(
        &self,
        _request: Request<proto::ActivateTriggerInRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "ActivateTriggerIn requires FFI integration",
        ))
    }

    async fn update_trigger_outs(
        &self,
        _request: Request<proto::DeviceRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "UpdateTriggerOuts requires FFI integration",
        ))
    }

    async fn is_triggered(
        &self,
        _request: Request<proto::IsTriggeredRequest>,
    ) -> Result<Response<proto::IsTriggeredResponse>, Status> {
        Err(Status::unimplemented(
            "IsTriggered requires FFI integration",
        ))
    }

    async fn get_trigger_out_vector(
        &self,
        _request: Request<proto::TriggerOutVectorRequest>,
    ) -> Result<Response<proto::TriggerOutVectorResponse>, Status> {
        Err(Status::unimplemented(
            "GetTriggerOutVector requires FFI integration",
        ))
    }

    async fn write_to_pipe_in(
        &self,
        _request: Request<proto::WriteToPipeInRequest>,
    ) -> Result<Response<proto::PipeTransferResponse>, Status> {
        Err(Status::unimplemented(
            "WriteToPipeIn requires FFI integration",
        ))
    }

    async fn read_from_pipe_out(
        &self,
        _request: Request<proto::ReadFromPipeOutRequest>,
    ) -> Result<Response<proto::PipeDataResponse>, Status> {
        Err(Status::unimplemented(
            "ReadFromPipeOut requires FFI integration",
        ))
    }

    async fn write_to_block_pipe_in(
        &self,
        _request: Request<proto::WriteToBlockPipeInRequest>,
    ) -> Result<Response<proto::PipeTransferResponse>, Status> {
        Err(Status::unimplemented(
            "WriteToBlockPipeIn requires FFI integration",
        ))
    }

    async fn read_from_block_pipe_out(
        &self,
        _request: Request<proto::ReadFromBlockPipeOutRequest>,
    ) -> Result<Response<proto::PipeDataResponse>, Status> {
        Err(Status::unimplemented(
            "ReadFromBlockPipeOut requires FFI integration",
        ))
    }

    async fn set_bt_pipe_polling_interval(
        &self,
        _request: Request<proto::SetBtPipePollingIntervalRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "SetBTPipePollingInterval requires FFI integration",
        ))
    }

    async fn read_register(
        &self,
        _request: Request<proto::ReadRegisterRequest>,
    ) -> Result<Response<proto::RegisterValueResponse>, Status> {
        Err(Status::unimplemented(
            "ReadRegister requires FFI integration",
        ))
    }

    async fn write_register(
        &self,
        _request: Request<proto::WriteRegisterRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "WriteRegister requires FFI integration",
        ))
    }

    async fn read_registers(
        &self,
        _request: Request<proto::ReadRegistersRequest>,
    ) -> Result<Response<proto::RegisterEntriesResponse>, Status> {
        Err(Status::unimplemented(
            "ReadRegisters requires FFI integration",
        ))
    }

    async fn write_registers(
        &self,
        _request: Request<proto::WriteRegistersRequest>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        Err(Status::unimplemented(
            "WriteRegisters requires FFI integration",
        ))
    }
}
