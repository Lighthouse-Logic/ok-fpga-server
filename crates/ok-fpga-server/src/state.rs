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

use tokio::sync::broadcast;

use ok_fpga_core::manager::DeviceManager;
use ok_fpga_core::resource::ResourceRegistry;
use ok_fpga_core::service_manager::ServiceManager;
use ok_fpga_core::user::UserSessionRegistry;

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

    /// Tracks active user sessions with last-seen timestamps (ADR-020 §5).
    /// Used by the heartbeat middleware and sweep task for liveness tracking.
    pub user_sessions: Arc<UserSessionRegistry>,

    /// Broadcast channel for real-time events (WebSocket dashboard).
    ///
    /// Resource and user lifecycle events are sent through this channel
    /// so the dashboard auto-refreshes when state changes (Phase 25-Fix-H, F9).
    /// `None` in unit tests that don't need event broadcasting.
    pub event_tx: Option<broadcast::Sender<String>>,

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
        user_sessions: Arc<UserSessionRegistry>,
    ) -> Self {
        Self {
            device_manager,
            resource_registry,
            service_manager,
            handle_pool,
            auth_provider,
            session_manager,
            user_sessions,
            event_tx: None,
            start_time: Instant::now(),
            api_version: env!("CARGO_PKG_VERSION"),
        }
    }

    /// Returns the server uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Broadcast an event to all connected WebSocket clients.
    ///
    /// This is a fire-and-forget operation — if no subscribers are
    /// listening (or `event_tx` is `None` in tests), the event is silently
    /// dropped. The JSON string should contain a `type` field that the
    /// dashboard's `handleWsEvent()` uses to dispatch actions.
    pub fn broadcast_event(&self, json: &str) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(json.to_string());
        }
    }
}
