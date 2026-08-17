//! # gRPC ManagedService Implementation
//!
//! Implements [`ManagedService`](ok_fpga_core::service::ManagedService) for
//! the gRPC server, allowing it to be started, stopped, and restarted via
//! the [`ServiceManager`](ok_fpga_core::service_manager::ServiceManager).
//!
//! ## Lifecycle
//!
//! - **start()**: Binds the gRPC server to the configured port, spawns
//!   it as a background tokio task.
//! - **stop()**: Signals the server to shut down gracefully.
//! - **status()**: Returns the current lifecycle state.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use tokio::sync::Notify;
use tracing::{info, error};

use crate::state::AppState;

/// ManagedService implementation for the gRPC server.
///
/// Wraps the tonic server and manages its lifecycle via a background
/// tokio task and a shutdown signal.
pub struct GrpcManagedService {
    /// Atomic status: 0=Stopped, 1=Running, 2=Starting, 3=Stopping
    status: AtomicU8,

    /// The socket address to bind the gRPC server to.
    bind_addr: std::net::SocketAddr,

    /// Shared application state passed to gRPC service implementations.
    state: Arc<AppState>,

    /// Signal to trigger graceful shutdown of the gRPC server.
    shutdown_signal: Arc<Notify>,
}

impl GrpcManagedService {
    /// Create a new `GrpcManagedService`.
    ///
    /// # Arguments
    ///
    /// * `bind_addr` — The address to bind (e.g., `0.0.0.0:50051`)
    /// * `state` — Shared application state
    pub fn new(bind_addr: std::net::SocketAddr, state: Arc<AppState>) -> Self {
        Self {
            status: AtomicU8::new(0), // Stopped
            bind_addr,
            state,
            shutdown_signal: Arc::new(Notify::new()),
        }
    }

    /// Map the atomic u8 to a ServiceStatus.
    fn atomic_to_status(val: u8) -> ServiceStatus {
        match val {
            0 => ServiceStatus::Stopped,
            1 => ServiceStatus::Running,
            2 => ServiceStatus::Starting,
            3 => ServiceStatus::Stopping,
            _ => ServiceStatus::Failed {
                reason: "unknown state".to_string(),
            },
        }
    }
}

#[async_trait::async_trait]
impl ManagedService for GrpcManagedService {
    fn name(&self) -> &str {
        "grpc"
    }

    fn status(&self) -> ServiceStatus {
        Self::atomic_to_status(self.status.load(Ordering::SeqCst))
    }

    async fn start(&self) -> Result<(), ServiceError> {
        self.status.store(2, Ordering::SeqCst); // Starting

        let addr = self.bind_addr;
        let state = self.state.clone();
        let shutdown = self.shutdown_signal.clone();
        let status = Arc::new(AtomicU8::new(0));
        let status_clone = status.clone();

        // Spawn the gRPC server as a background task.
        let handle = tokio::spawn(async move {
            let result = super::start_grpc_server(
                addr,
                state,
                async move {
                    shutdown.notified().await;
                },
            )
            .await;

            if let Err(e) = &result {
                error!(error = %e, "gRPC server error");
                status_clone.store(4, Ordering::SeqCst); // Failed
            } else {
                status_clone.store(0, Ordering::SeqCst); // Stopped
            }
        });

        // Give the server a moment to bind and start.
        // In production, we'd use a readiness signal from the server.
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check if the task panicked or failed immediately.
        if handle.is_finished() {
            self.status.store(0, Ordering::SeqCst); // Stopped
            return Err(ServiceError::OperationFailed {
                name: "grpc".to_string(),
                message: "gRPC server failed to start (task exited immediately)".to_string(),
            });
        }

        self.status.store(1, Ordering::SeqCst); // Running
        info!(address = %self.bind_addr, "gRPC server is running");
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        self.status.store(3, Ordering::SeqCst); // Stopping

        // Signal the server to shut down.
        self.shutdown_signal.notify_one();

        // Give it a moment to drain connections.
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        self.status.store(0, Ordering::SeqCst); // Stopped
        info!("gRPC server stopped");
        Ok(())
    }
}
