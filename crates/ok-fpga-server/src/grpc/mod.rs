//! # gRPC Server Module
//!
//! This module implements the gRPC API defined in `proto/okfpga/v1/okfpga.proto`.
//! It is only compiled when the `grpc` Cargo feature is enabled.
//!
//! ## Services
//!
//! Four gRPC services are implemented:
//!
//! - [`server_service`] — Server lifecycle management (ADR-010)
//! - [`device_service`] — Device discovery, info, sensors, settings, SYZYGY
//! - [`classic_service`] — Classic data port operations (wire/trigger/pipe/register)
//! - [`axi_service`] — AXI data port operations (Lite/Full/Stream)
//!
//! ## Architecture (ADR-005)
//!
//! Each service implementation holds an `Arc<AppState>` and delegates to
//! the `ok-fpga-core` domain types. The gRPC server itself implements
//! `ManagedService` so it can be started/stopped by the `ServiceManager`.

pub mod axi_service;
pub mod classic_service;
pub mod device_service;
pub mod managed;
pub mod server_service;

use std::sync::Arc;

use tonic::transport::Server;
use tracing::info;

use crate::state::AppState;

/// The generated Rust types from `okfpga.proto`.
///
/// This module is produced by `tonic-build` in `build.rs`. It contains:
/// - All `message` types as Rust structs (e.g., `DeviceRequest`, `StatusResponse`)
/// - All `enum` types as Rust enums
/// - All `service` traits as Rust traits with default `_server` suffix
///
/// We re-export it here so other modules can use `crate::grpc::proto::*`.
pub mod proto {
    #![allow(clippy::enum_variant_names)] // Proto-generated enums use prefixed variants (protobuf convention).
    tonic::include_proto!("ok_fpga_service.v1");
}

/// Start the gRPC server on the given address.
///
/// This function creates all four gRPC service implementations, adds them
/// to a `tonic::transport::Server`, and begins serving. It blocks until
/// the server is shut down.
///
/// The server includes an implicit heartbeat interceptor that extracts
/// the `x-user-id` metadata key from every request and calls
/// `user_sessions.touch()` for liveness tracking (ADR-020 §5).
///
/// # Arguments
///
/// * `addr` — The socket address to bind (e.g., `"0.0.0.0:50051"`)
/// * `state` — Shared application state
/// * `shutdown` — A future that resolves when the server should stop
pub async fn start_grpc_server(
    addr: std::net::SocketAddr,
    state: Arc<AppState>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), tonic::transport::Error> {
    use proto::axi_data_port_service_server::AxiDataPortServiceServer;
    use proto::classic_data_port_service_server::ClassicDataPortServiceServer;
    use proto::device_service_server::DeviceServiceServer;
    use proto::server_service_server::ServerServiceServer;

    let device_svc = device_service::DeviceServiceImpl::new(state.clone());
    let classic_svc = classic_service::ClassicDataPortServiceImpl::new(state.clone());
    let axi_svc = axi_service::AXIDataPortServiceImpl::new(state.clone());
    let server_svc = server_service::ServerServiceImpl::new(state.clone());

    // Build the heartbeat interceptor that touches user sessions on every
    // gRPC request that includes the `x-user-id` metadata key.
    // Phase 28D: Also broadcasts `user_connected` for new users so the
    // dashboard Users page auto-refreshes (matching the REST middleware).
    let heartbeat_state = state.clone();
    let heartbeat_interceptor = move |req: tonic::Request<()>| -> Result<tonic::Request<()>, tonic::Status> {
        if let Some(user_id) = req.metadata().get("x-user-id") {
            if let Ok(user_id_str) = user_id.to_str() {
                if !user_id_str.is_empty() {
                    // Check if this is a new user (first time we've seen them).
                    let is_new = heartbeat_state.user_sessions.get(user_id_str).is_none();

                    heartbeat_state.user_sessions.touch(user_id_str);

                    // Broadcast user_connected for dashboard auto-refresh.
                    if is_new {
                        heartbeat_state.broadcast_event(&format!(
                            r#"{{"type":"user_connected","user_id":"{}"}}"#,
                            user_id_str
                        ));
                    }
                }
            }
        }
        Ok(req)
    };

    info!(address = %addr, "starting gRPC server");

    Server::builder()
        .add_service(DeviceServiceServer::with_interceptor(device_svc, heartbeat_interceptor.clone()))
        .add_service(ClassicDataPortServiceServer::new(classic_svc))
        .add_service(AxiDataPortServiceServer::new(axi_svc))
        .add_service(ServerServiceServer::new(server_svc))
        .serve_with_shutdown(addr, shutdown)
        .await?;

    info!("gRPC server stopped");
    Ok(())
}

