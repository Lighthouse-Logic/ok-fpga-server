// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
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

use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use tokio::sync::broadcast;

use ok_fpga_core::manager::{DeleteReason, DeviceEvent, DeviceManager};
use ok_fpga_core::resource::{ForceDeletedResource, ResourceRegistry};
use ok_fpga_core::service_manager::ServiceManager;
use ok_fpga_core::user::UserSessionRegistry;

use crate::handle_pool::DeviceHandlePool;

// ============================================================================
// Endpoint Access Enforcement Level (ADR-024 §1)
// ============================================================================

/// Controls whether FPGA data port operations require resource ownership.
///
/// This enum is the runtime representation of the `endpoint_enforcement`
/// config key. It is stored on [`AppState`] so that middleware and handlers
/// can do a cheap enum comparison instead of parsing a string on every request.
///
/// ## Levels
///
/// - **`Off`**: No checks. Current v1 behavior. `X-User-Id` is not required.
/// - **`Warn`**: Check passes, but violations are logged at WARN level.
///   `X-User-Id` is required on all requests.
/// - **`Enforce`**: Unauthorized operations are rejected with `403 Forbidden`
///   (REST) or `PERMISSION_DENIED` (gRPC). `X-User-Id` is required.
///
/// Default: `Off` — preserves backward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointEnforcement {
    /// No enforcement. All operations pass through without checks.
    Off,
    /// Log violations at WARN level but allow the operation to proceed.
    Warn,
    /// Reject unauthorized operations.
    Enforce,
}

impl EndpointEnforcement {
    /// Parse the enforcement level from a config string.
    ///
    /// Accepts `"off"`, `"warn"`, or `"enforce"` (case-insensitive).
    /// Returns `Off` for unrecognized values (with a logged warning).
    pub fn from_config(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "warn" => EndpointEnforcement::Warn,
            "enforce" => EndpointEnforcement::Enforce,
            "off" => EndpointEnforcement::Off,
            other => {
                tracing::warn!(
                    "Unknown endpoint_enforcement value '{}', defaulting to 'off'",
                    other
                );
                EndpointEnforcement::Off
            }
        }
    }

    /// Returns `true` if enforcement is active (i.e., not `Off`).
    ///
    /// When active, `X-User-Id` must be present on all requests.
    pub fn is_active(&self) -> bool {
        *self != EndpointEnforcement::Off
    }
}

impl fmt::Display for EndpointEnforcement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointEnforcement::Off => write!(f, "off"),
            EndpointEnforcement::Warn => write!(f, "warn"),
            EndpointEnforcement::Enforce => write!(f, "enforce"),
        }
    }
}

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
// Note: Clone is implemented manually below because `enforcement`
// uses `RwLock` for runtime mutability (Phase 32).
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

    /// Endpoint access enforcement level (ADR-024 §1).
    ///
    /// Controls whether FPGA data port operations (Classic wires/triggers/pipes,
    /// AXI reads/writes) require the caller to hold a resource covering the
    /// target endpoint. See [`EndpointEnforcement`] for level descriptions.
    ///
    /// Wrapped in `RwLock` so the Settings panel can update the enforcement
    /// level at runtime without restarting the server (Phase 32).
    /// Use [`enforcement()`](Self::enforcement) and
    /// [`set_enforcement()`](Self::set_enforcement) to access.
    enforcement: RwLock<EndpointEnforcement>,

    /// Runtime-mutable server configuration.
    ///
    /// Wrapped in `RwLock` so the Settings panel can update fields like
    /// `server.description` and `access_control.endpoint_enforcement` at
    /// runtime. Changes are persisted to the TOML file via `save_config()`.
    pub config: Arc<RwLock<crate::config::ServerConfig>>,

    /// Path to the TOML config file (for write-back on settings changes).
    /// `None` if the config was loaded from defaults with no file.
    pub config_path: Option<PathBuf>,
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
        enforcement: EndpointEnforcement,
        config: crate::config::ServerConfig,
        config_path: Option<PathBuf>,
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
            enforcement: RwLock::new(enforcement),
            config: Arc::new(RwLock::new(config)),
            config_path,
        }
    }

    /// Returns the server uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Read the current enforcement level.
    ///
    /// This acquires a brief read lock. The returned value is `Copy`,
    /// so the lock is released immediately.
    pub fn enforcement(&self) -> EndpointEnforcement {
        *self.enforcement.read().expect("enforcement lock poisoned")
    }

    /// Update the enforcement level at runtime.
    ///
    /// Called by the Settings management handler when the user changes
    /// the enforcement level via the dashboard (Phase 32).
    pub fn set_enforcement(&self, level: EndpointEnforcement) {
        *self.enforcement.write().expect("enforcement lock poisoned") = level;
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

    /// Forcibly unlock, release all holders, and delete all resources on
    /// `serial`, emitting `ResourceUnlocked` (for any shared resource with an
    /// active lock) followed by `ResourceDeleted { reason }` (ADR-034 §7 & §14,
    /// D26, `resource-model.typ` §3.6).
    ///
    /// Returns the number of resources deleted.
    pub fn force_cleanup_device_resources(&self, serial: &str, reason: DeleteReason) -> usize {
        let deleted = self.resource_registry.force_delete_for_device(serial);
        let count = deleted.len();
        broadcast_force_deleted_resources(
            &self.device_manager,
            self.event_tx.as_ref(),
            serial,
            &deleted,
            reason,
        );
        count
    }
}

/// Broadcast `ResourceUnlocked` (if a shared resource had an active lock) and
/// `ResourceDeleted` events for a slice of force-deleted resources, plus a
/// dashboard WebSocket `resources_cleared` summary when non-empty (D26, §3.6).
pub fn broadcast_force_deleted_resources(
    device_manager: &DeviceManager,
    event_tx: Option<&broadcast::Sender<String>>,
    device_serial: &str,
    deleted: &[ForceDeletedResource],
    reason: DeleteReason,
) {
    for item in deleted {
        if let Some(ref user_id) = item.unlocked_user {
            let _ = device_manager.broadcast(DeviceEvent::ResourceUnlocked {
                resource_id: item.resource_id.clone(),
                user_id: user_id.clone(),
                timestamp: chrono::Utc::now(),
            });
            if let Some(tx) = event_tx {
                let _ = tx.send(format!(
                    r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
                    item.resource_id, user_id
                ));
            }
        }

        let _ = device_manager.broadcast(DeviceEvent::ResourceDeleted {
            resource_id: item.resource_id.clone(),
            reason,
            timestamp: chrono::Utc::now(),
        });
        if let Some(tx) = event_tx {
            let _ = tx.send(format!(
                r#"{{"type":"resource_deleted","resource_id":"{}","reason":"{}"}}"#,
                item.resource_id,
                reason.as_str()
            ));
        }
    }

    if !deleted.is_empty() {
        if let Some(tx) = event_tx {
            let _ = tx.send(format!(
                r#"{{"type":"resources_cleared","device_serial":"{}","cleared_count":{},"reason":"{}"}}"#,
                device_serial,
                deleted.len(),
                reason.as_str()
            ));
        }
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            device_manager: self.device_manager.clone(),
            resource_registry: self.resource_registry.clone(),
            service_manager: self.service_manager.clone(),
            handle_pool: self.handle_pool.clone(),
            auth_provider: self.auth_provider.clone(),
            session_manager: self.session_manager.clone(),
            user_sessions: self.user_sessions.clone(),
            event_tx: self.event_tx.clone(),
            start_time: self.start_time,
            api_version: self.api_version,
            enforcement: RwLock::new(self.enforcement()),
            config: self.config.clone(),
            config_path: self.config_path.clone(),
        }
    }
}
