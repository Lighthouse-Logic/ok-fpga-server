//! # Shared Application State
//!
//! This module defines the [`AppState`] struct that is shared across all
//! server components (REST routes, gRPC handlers, management API). It
//! holds `Arc` references to the core domain types so each request handler
//! can access device, resource, and service state without cloning.
//!
//! ## Thread Safety
//!
//! `AppState` is designed to be wrapped in `Arc` and shared across async
//! tasks. All interior types use their own synchronization (RwLock, broadcast
//! channels, etc.).

use std::sync::Arc;
use std::time::Instant;

use ok_fpga_core::manager::DeviceManager;
use ok_fpga_core::resource::ResourceRegistry;
use ok_fpga_core::service_manager::ServiceManager;

use crate::handle_pool::DeviceHandlePool;

/// Shared application state passed to all request handlers.
///
/// This struct is the "glue" between the network-facing API layers (REST,
/// gRPC, management) and the core domain logic. Each field is an `Arc`
/// so the state can be cheaply cloned into handler closures.
///
/// ## Usage
///
/// ```rust,ignore
/// // In REST route handlers (via Axum State extractor):
/// async fn list_devices(State(state): State<Arc<AppState>>) -> Json<...> {
///     let devices = state.device_manager.list_devices();
///     // ...
/// }
///
/// // In gRPC handlers:
/// impl DeviceService for MyService {
///     async fn list_devices(&self, request: Request<...>) -> Result<Response<...>, Status> {
///         let devices = self.state.device_manager.list_devices();
///         // ...
///     }
/// }
/// ```
#[derive(Clone)]
pub struct AppState {
    /// Manages FPGA device discovery, hot-plug events, and device state.
    pub device_manager: Arc<DeviceManager>,

    /// Tracks shared/exclusive resource ownership across users.
    pub resource_registry: Arc<ResourceRegistry>,

    /// Controls lifecycle of optional services (REST, gRPC, MQTT, mDNS).
    pub service_manager: Arc<ServiceManager>,

    /// Pool of open FrontPanel device handles for FFI operations (ADR-018).
    /// REST and gRPC handlers use this to perform hardware operations.
    /// `None` if the FrontPanel DLL is not available.
    pub handle_pool: Option<Arc<DeviceHandlePool>>,

    /// Authentication provider (ADR-008).
    #[allow(dead_code)] // Used when auth middleware is wired.
    pub auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider>,

    /// In-memory session store for the web dashboard.
    #[allow(dead_code)] // Used when auth middleware is wired.
    pub session_manager: Arc<ok_fpga_core::auth::SessionManager>,

    /// When the server process started. Used to calculate uptime.
    pub start_time: Instant,

    /// Server API version string (from Cargo.toml).
    pub api_version: &'static str,
}

impl AppState {
    /// Create a new `AppState` with the given core components.
    ///
    /// The `start_time` is captured automatically as `Instant::now()`.
    pub fn new(
        device_manager: Arc<DeviceManager>,
        resource_registry: Arc<ResourceRegistry>,
        service_manager: Arc<ServiceManager>,
        handle_pool: Option<Arc<DeviceHandlePool>>,
        auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider>,
        session_manager: Arc<ok_fpga_core::auth::SessionManager>,
    ) -> Self {
        Self {
            device_manager,
            resource_registry,
            service_manager,
            handle_pool,
            auth_provider,
            session_manager,
            start_time: Instant::now(),
            api_version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// Returns the server uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}
