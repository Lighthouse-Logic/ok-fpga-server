//! # REST ManagedService Implementation
//!
//! Implements [`ManagedService`](ok_fpga_core::service::ManagedService) for
//! the REST API server, allowing it to be started, stopped, and restarted
//! via the [`ServiceManager`](ok_fpga_core::service_manager::ServiceManager).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use tokio::sync::Notify;
use tracing::{error, info};

use crate::state::AppState;

/// ManagedService implementation for the REST API server.
///
/// Wraps an Axum server and manages its lifecycle via a background
/// tokio task and a shutdown signal.
pub struct RestManagedService {
    /// Atomic status: 0=Stopped, 1=Running, 2=Starting, 3=Stopping
    status: AtomicU8,

    /// The socket address to bind the REST server to.
    bind_addr: std::net::SocketAddr,

    /// Shared application state passed to route handlers.
    state: Arc<AppState>,

    /// Signal to trigger graceful shutdown.
    shutdown_signal: Arc<Notify>,
}

impl RestManagedService {
    /// Create a new `RestManagedService`.
    pub fn new(bind_addr: std::net::SocketAddr, state: Arc<AppState>) -> Self {
        Self {
            status: AtomicU8::new(0), // Stopped
            bind_addr,
            state,
            shutdown_signal: Arc::new(Notify::new()),
        }
    }

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
impl ManagedService for RestManagedService {
    fn name(&self) -> &str {
        "rest"
    }

    fn status(&self) -> ServiceStatus {
        Self::atomic_to_status(self.status.load(Ordering::SeqCst))
    }

    async fn start(&self) -> Result<(), ServiceError> {
        self.status.store(2, Ordering::SeqCst); // Starting

        let router = super::build_router(self.state.clone());
        let addr = self.bind_addr;
        let shutdown = self.shutdown_signal.clone();

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                self.status.store(0, Ordering::SeqCst); // Back to Stopped
                return Err(ServiceError::OperationFailed {
                    name: "rest".to_string(),
                    message: format!("failed to bind {}: {}", addr, e),
                });
            }
        };

        info!(address = %addr, "REST server bound successfully");

        // Spawn the Axum server as a background task.
        tokio::spawn(async move {
            let result = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown.notified().await;
                })
                .await;

            if let Err(e) = result {
                error!(error = %e, "REST server error");
            }
        });

        self.status.store(1, Ordering::SeqCst); // Running
        info!(address = %addr, "REST server is running");
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        self.status.store(3, Ordering::SeqCst); // Stopping

        self.shutdown_signal.notify_one();

        // Give it a moment to drain connections.
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        self.status.store(0, Ordering::SeqCst); // Stopped
        info!("REST server stopped");
        Ok(())
    }
}
