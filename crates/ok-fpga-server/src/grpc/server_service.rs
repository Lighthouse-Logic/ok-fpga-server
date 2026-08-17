//! # ServerService gRPC Implementation
//!
//! Implements the `ServerService` from `okfpga.proto` (ADR-010).
//! Provides RPCs for querying server status and managing optional services.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::grpc::proto;
use crate::state::AppState;

/// gRPC implementation of the `ServerService`.
///
/// Delegates to [`ServiceManager`](ok_fpga_core::service_manager::ServiceManager)
/// for all lifecycle operations.
pub struct ServerServiceImpl {
    state: Arc<AppState>,
}

impl ServerServiceImpl {
    /// Create a new `ServerServiceImpl` backed by the given application state.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Convert a core `ServiceInfo` into the proto `ServiceInfo` message.
    fn to_proto_service_info(
        info: &ok_fpga_core::service::ServiceInfo,
    ) -> proto::ServiceInfo {
        proto::ServiceInfo {
            name: info.name.clone(),
            status: info.status.as_str().to_string(),
            compiled: info.compiled,
            enabled: info.enabled,
        }
    }
}

#[tonic::async_trait]
impl proto::server_service_server::ServerService for ServerServiceImpl {
    /// Get detailed server status including uptime and service states.
    async fn get_server_status(
        &self,
        _request: Request<proto::StatusResponse>,
    ) -> Result<Response<proto::ServerStatusResponse>, Status> {
        let services = self.state.service_manager.list();
        let device_count = self.state.device_manager.device_count() as i32;

        let proto_services: Vec<proto::ServiceInfo> = services
            .iter()
            .map(Self::to_proto_service_info)
            .collect();

        Ok(Response::new(proto::ServerStatusResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
            api_version: self.state.api_version.to_string(),
            uptime_secs: self.state.uptime_secs(),
            device_count,
            services: proto_services,
        }))
    }

    /// Restart the entire server process.
    async fn restart_server(
        &self,
        _request: Request<proto::StatusResponse>,
    ) -> Result<Response<proto::StatusResponse>, Status> {
        // Per ADR-010: respond first, then restart after a short delay.
        // The actual process restart is handled by the management layer.
        Err(Status::unimplemented(
            "Server restart via gRPC is not yet implemented. \
             Use the management API on port 9090 instead.",
        ))
    }

    /// List all manageable services and their current status.
    async fn list_services(
        &self,
        _request: Request<proto::StatusResponse>,
    ) -> Result<Response<proto::ListServicesResponse>, Status> {
        let services = self.state.service_manager.list();
        let proto_services: Vec<proto::ServiceInfo> = services
            .iter()
            .map(Self::to_proto_service_info)
            .collect();

        Ok(Response::new(proto::ListServicesResponse {
            error: Some(proto::ErrorCluster {
                status: false,
                code: 0,
                source: String::new(),
            }),
            services: proto_services,
        }))
    }

    /// Start a stopped service.
    async fn start_service(
        &self,
        request: Request<proto::ServiceRequest>,
    ) -> Result<Response<proto::ServiceOperationResponse>, Status> {
        let name = &request.get_ref().name;

        match self.state.service_manager.start(name).await {
            Ok(()) => {
                let info = self
                    .state
                    .service_manager
                    .get_status(name)
                    .ok_or_else(|| Status::not_found(format!("service '{}' not found", name)))?;

                Ok(Response::new(proto::ServiceOperationResponse {
                    error: Some(proto::ErrorCluster {
                        status: false,
                        code: 0,
                        source: String::new(),
                    }),
                    service: Some(Self::to_proto_service_info(&info)),
                }))
            }
            Err(e) => Err(Status::failed_precondition(e.to_string())),
        }
    }

    /// Stop a running service.
    async fn stop_service(
        &self,
        request: Request<proto::ServiceRequest>,
    ) -> Result<Response<proto::ServiceOperationResponse>, Status> {
        let name = &request.get_ref().name;

        match self.state.service_manager.stop(name).await {
            Ok(()) => {
                let info = self
                    .state
                    .service_manager
                    .get_status(name)
                    .ok_or_else(|| Status::not_found(format!("service '{}' not found", name)))?;

                Ok(Response::new(proto::ServiceOperationResponse {
                    error: Some(proto::ErrorCluster {
                        status: false,
                        code: 0,
                        source: String::new(),
                    }),
                    service: Some(Self::to_proto_service_info(&info)),
                }))
            }
            Err(e) => Err(Status::failed_precondition(e.to_string())),
        }
    }

    /// Restart a running service.
    async fn restart_service(
        &self,
        request: Request<proto::ServiceRequest>,
    ) -> Result<Response<proto::ServiceOperationResponse>, Status> {
        let name = &request.get_ref().name;

        match self.state.service_manager.restart(name).await {
            Ok(()) => {
                let info = self
                    .state
                    .service_manager
                    .get_status(name)
                    .ok_or_else(|| Status::not_found(format!("service '{}' not found", name)))?;

                Ok(Response::new(proto::ServiceOperationResponse {
                    error: Some(proto::ErrorCluster {
                        status: false,
                        code: 0,
                        source: String::new(),
                    }),
                    service: Some(Self::to_proto_service_info(&info)),
                }))
            }
            Err(e) => Err(Status::failed_precondition(e.to_string())),
        }
    }
}
