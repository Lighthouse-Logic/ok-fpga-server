// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Service Lifecycle Types
//!
//! This module defines the core abstractions for managing optional server
//! services (REST API, gRPC API, MQTT publisher, mDNS discovery). Each
//! service implements the [`ManagedService`](crate::service::ManagedService)
//! trait, allowing the [`ServiceManager`](crate::service_manager::ServiceManager)
//! to start, stop, and restart them uniformly.
//!
//! ## Design (ADR-010)
//!
//! The ok-FPGA-Server has multiple network-facing services that can be:
//! - **Excluded at compile time** via Cargo feature flags
//! - **Enabled/disabled at runtime** via TOML configuration
//! - **Started/stopped dynamically** via the management API or gRPC/REST
//!
//! This module provides the common vocabulary types that all services share.
//!
//! ## Key Types
//!
//! - [`ManagedService`](crate::service::ManagedService) — Trait that all
//!   manageable services implement.
//! - [`ServiceStatus`](crate::service::ServiceStatus) — Current lifecycle
//!   state of a service.
//! - [`ServiceEvent`](crate::service::ServiceEvent) — Events broadcast
//!   when a service's status changes.
//! - [`ServiceError`](crate::service::ServiceError) — Errors that can
//!   occur during service lifecycle operations.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Service Status
// ============================================================================

/// Current lifecycle state of a managed service.
///
/// A service transitions through these states during its lifecycle:
///
/// ```text
///                ┌─────────┐
///                │ Stopped │◄──────────────────────┐
///                └────┬────┘                        │
///                     │ start()                     │
///                ┌────▼─────┐                  ┌────┴─────┐
///                │ Starting │                  │ Stopping │
///                └────┬─────┘                  └────▲─────┘
///                     │ success                     │ stop()
///                ┌────▼────┐                        │
///                │ Running │────────────────────────┘
///                └────┬────┘
///                     │ error
///                ┌────▼────┐
///                │ Failed  │
///                └─────────┘
/// ```
///
/// The `NotCompiled` state is a special terminal state for services whose
/// Cargo feature flag was not enabled at compile time. These services can
/// never be started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceStatus {
    /// The service is actively accepting connections or performing work.
    Running,

    /// The service is not running. It can be started.
    Stopped,

    /// The service is in the process of starting up (binding ports, etc.).
    /// This is a transient state — it will transition to `Running` or `Failed`.
    Starting,

    /// The service is in the process of shutting down (draining connections, etc.).
    /// This is a transient state — it will transition to `Stopped`.
    Stopping,

    /// The service failed to start or crashed during operation.
    /// The `reason` field contains a human-readable error message.
    Failed {
        /// A description of what went wrong (e.g., "port 8080 already in use").
        reason: String,
    },

    /// The service's Cargo feature flag was not enabled at compile time.
    /// This service can never be started in the current binary.
    ///
    /// # Example
    ///
    /// If the server was built without `--features grpc`, the gRPC service
    /// will always report `NotCompiled`.
    NotCompiled,
}

impl ServiceStatus {
    /// Returns `true` if the service is currently accepting work.
    ///
    /// # Example
    ///
    /// ```
    /// use ok_fpga_core::service::ServiceStatus;
    ///
    /// assert!(ServiceStatus::Running.is_running());
    /// assert!(!ServiceStatus::Stopped.is_running());
    /// ```
    pub fn is_running(&self) -> bool {
        matches!(self, ServiceStatus::Running)
    }

    /// Returns `true` if the service is in a transient state (starting or stopping).
    ///
    /// Callers should avoid sending start/stop commands while a service is
    /// in a transient state — wait for it to settle first.
    pub fn is_transitioning(&self) -> bool {
        matches!(self, ServiceStatus::Starting | ServiceStatus::Stopping)
    }

    /// Returns `true` if the service can potentially be started.
    ///
    /// A service can be started only if it is currently `Stopped` or `Failed`.
    /// Services that are `Running`, `Starting`, `Stopping`, or `NotCompiled`
    /// cannot be started.
    pub fn can_start(&self) -> bool {
        matches!(self, ServiceStatus::Stopped | ServiceStatus::Failed { .. })
    }

    /// Returns `true` if the service can be stopped.
    ///
    /// A service can only be stopped if it is currently `Running`.
    pub fn can_stop(&self) -> bool {
        matches!(self, ServiceStatus::Running)
    }

    /// Returns a short string label for this status (e.g., "running", "stopped").
    ///
    /// This matches the values used in the gRPC `ServiceInfo.status` field
    /// and the MQTT `services/status` message.
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceStatus::Running => "running",
            ServiceStatus::Stopped => "stopped",
            ServiceStatus::Starting => "starting",
            ServiceStatus::Stopping => "stopping",
            ServiceStatus::Failed { .. } => "failed",
            ServiceStatus::NotCompiled => "not_compiled",
        }
    }
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceStatus::Failed { reason } => write!(f, "failed: {}", reason),
            other => write!(f, "{}", other.as_str()),
        }
    }
}

// ============================================================================
// Service Events
// ============================================================================

/// Events broadcast when a service's lifecycle state changes.
///
/// These events are published to the `ServiceManager`'s broadcast channel.
/// Subscribers include the MQTT publisher (which publishes them to
/// `{prefix}/services/status`), the mDNS service (which updates TXT records),
/// and the web dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceEvent {
    /// A service has fully started and is now accepting work.
    Started {
        /// The name of the service (e.g., "rest", "grpc", "mqtt", "mdns").
        name: String,
    },

    /// A service has fully stopped and is no longer accepting work.
    Stopped {
        /// The name of the service (e.g., "rest", "grpc", "mqtt", "mdns").
        name: String,
    },

    /// A service failed to start or crashed during operation.
    Failed {
        /// The name of the service (e.g., "rest", "grpc", "mqtt", "mdns").
        name: String,
        /// A description of what went wrong.
        reason: String,
    },
}

impl fmt::Display for ServiceEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceEvent::Started { name } => write!(f, "service '{}' started", name),
            ServiceEvent::Stopped { name } => write!(f, "service '{}' stopped", name),
            ServiceEvent::Failed { name, reason } => {
                write!(f, "service '{}' failed: {}", name, reason)
            }
        }
    }
}

// ============================================================================
// Service Errors
// ============================================================================

/// Errors that can occur when managing a service's lifecycle.
///
/// These errors are returned by the `ServiceManager` methods (`start()`,
/// `stop()`, `restart()`) and are mapped to appropriate gRPC status codes
/// or HTTP error responses by the API layers.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// The requested service was not found in the registry.
    ///
    /// This means either:
    /// - The service name is misspelled
    /// - The service's Cargo feature flag was not enabled at compile time
    #[error("service '{name}' not found")]
    NotFound {
        /// The service name that was requested.
        name: String,
    },

    /// The service's Cargo feature was not compiled into this binary.
    ///
    /// The user needs to rebuild with the appropriate `--features` flag.
    #[error("service '{name}' was not compiled (feature flag not enabled)")]
    NotCompiled {
        /// The service name that was requested.
        name: String,
    },

    /// The service is already in the requested state.
    ///
    /// For example, trying to start a service that is already running,
    /// or trying to stop a service that is already stopped.
    #[error("service '{name}' is already {status}")]
    AlreadyInState {
        /// The service name.
        name: String,
        /// The current status of the service (e.g., "running", "stopped").
        status: String,
    },

    /// The service is currently transitioning (starting or stopping).
    ///
    /// Wait for the transition to complete before sending another command.
    #[error("service '{name}' is currently {status} — wait for it to finish")]
    Transitioning {
        /// The service name.
        name: String,
        /// The current transient status ("starting" or "stopping").
        status: String,
    },

    /// The service failed to start or stop due to an internal error.
    ///
    /// The `source` field contains the underlying error from the service
    /// implementation (e.g., "address already in use" for a port conflict).
    #[error("service '{name}' operation failed: {message}")]
    OperationFailed {
        /// The service name.
        name: String,
        /// A description of what went wrong.
        message: String,
    },
}

// ============================================================================
// Service Info (snapshot)
// ============================================================================

/// A point-in-time snapshot of a service's state.
///
/// This is the type returned by `ServiceManager::list()` and is directly
/// serialized to JSON for REST responses and MQTT messages. It maps to the
/// gRPC `ServiceInfo` message in `okfpga.proto`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name (e.g., "rest", "grpc", "mqtt", "mdns").
    pub name: String,

    /// Current lifecycle status.
    pub status: ServiceStatus,

    /// Whether this service's Cargo feature flag was enabled at compile time.
    /// If `false`, the service can never be started.
    pub compiled: bool,

    /// Whether this service is enabled in the TOML configuration file.
    /// A service can be compiled but disabled in config.
    pub enabled: bool,
}

impl fmt::Display for ServiceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} (compiled={}, enabled={})",
            self.name, self.status, self.compiled, self.enabled
        )
    }
}

// ============================================================================
// ManagedService Trait
// ============================================================================

/// Trait that all manageable server services must implement.
///
/// Each optional service (REST, gRPC, MQTT, mDNS) implements this trait,
/// allowing the [`ServiceManager`](crate::service_manager::ServiceManager)
/// to control their lifecycle uniformly.
///
/// ## Implementor Responsibilities
///
/// - **`start()`** must bind ports, connect to brokers, etc. and return
///   only when the service is fully ready to accept work.
/// - **`stop()`** must gracefully drain connections and release resources
///   before returning.
/// - **`restart()`** has a default implementation (stop then start), but
///   implementors can override it for more efficient restart logic.
/// - **`name()`** must return a stable, lowercase identifier (e.g., "rest").
/// - **`status()`** must return the current lifecycle state. It must be
///   safe to call from any thread at any time.
///
/// ## Thread Safety
///
/// All methods take `&self` (not `&mut self`) because the `ServiceManager`
/// holds services behind `Arc`. Implementations must handle their own
/// interior mutability (typically via `tokio::sync::Mutex` or atomics).
///
/// ## Example
///
/// ```rust,no_run
/// use ok_fpga_core::service::{ManagedService, ServiceStatus, ServiceError};
///
/// struct MyService {
///     // ... internal state ...
/// }
///
/// #[async_trait::async_trait]
/// impl ManagedService for MyService {
///     fn name(&self) -> &str { "my_service" }
///
///     fn status(&self) -> ServiceStatus {
///         ServiceStatus::Stopped
///     }
///
///     async fn start(&self) -> Result<(), ServiceError> {
///         // Bind ports, start listeners, etc.
///         Ok(())
///     }
///
///     async fn stop(&self) -> Result<(), ServiceError> {
///         // Drain connections, release resources, etc.
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait ManagedService: Send + Sync {
    /// Returns the unique, lowercase name of this service.
    ///
    /// Examples: `"rest"`, `"grpc"`, `"mqtt"`, `"mdns"`.
    /// This name is used as the key in the `ServiceManager` registry,
    /// in TOML config section names, and in API responses.
    fn name(&self) -> &str;

    /// Returns the current lifecycle status of this service.
    ///
    /// This method must be safe to call concurrently from any thread.
    /// Implementations typically use an `AtomicU8` or `RwLock` to store
    /// the status internally.
    fn status(&self) -> ServiceStatus;

    /// Start the service.
    ///
    /// This method should:
    /// 1. Transition the internal status to `Starting`
    /// 2. Perform startup work (bind ports, connect to brokers, etc.)
    /// 3. Transition to `Running` on success, or `Failed` on error
    ///
    /// Returns `Ok(())` when the service is fully started and ready.
    /// Returns `Err(ServiceError)` if startup fails.
    async fn start(&self) -> Result<(), ServiceError>;

    /// Gracefully stop the service.
    ///
    /// This method should:
    /// 1. Transition the internal status to `Stopping`
    /// 2. Stop accepting new connections/work
    /// 3. Drain existing connections (with a timeout)
    /// 4. Release all resources (ports, file handles, etc.)
    /// 5. Transition to `Stopped`
    ///
    /// Returns `Ok(())` when the service is fully stopped.
    /// Returns `Err(ServiceError)` if shutdown fails.
    async fn stop(&self) -> Result<(), ServiceError>;

    /// Restart the service (stop then start).
    ///
    /// The default implementation calls `stop()` then `start()`. Services
    /// may override this for more efficient restart logic (e.g., keeping
    /// internal state across restarts).
    ///
    /// Returns `Ok(())` when the service is fully restarted and ready.
    async fn restart(&self) -> Result<(), ServiceError> {
        self.stop().await?;
        self.start().await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- ServiceStatus tests --

    #[test]
    fn status_is_running_returns_true_for_running() {
        assert!(ServiceStatus::Running.is_running());
    }

    #[test]
    fn status_is_running_returns_false_for_stopped() {
        assert!(!ServiceStatus::Stopped.is_running());
    }

    #[test]
    fn status_is_running_returns_false_for_failed() {
        let status = ServiceStatus::Failed {
            reason: "port in use".to_string(),
        };
        assert!(!status.is_running());
    }

    #[test]
    fn status_is_transitioning_returns_true_for_starting() {
        assert!(ServiceStatus::Starting.is_transitioning());
    }

    #[test]
    fn status_is_transitioning_returns_true_for_stopping() {
        assert!(ServiceStatus::Stopping.is_transitioning());
    }

    #[test]
    fn status_is_transitioning_returns_false_for_running() {
        assert!(!ServiceStatus::Running.is_transitioning());
    }

    #[test]
    fn status_can_start_from_stopped() {
        assert!(ServiceStatus::Stopped.can_start());
    }

    #[test]
    fn status_can_start_from_failed() {
        let status = ServiceStatus::Failed {
            reason: "test".to_string(),
        };
        assert!(status.can_start());
    }

    #[test]
    fn status_cannot_start_from_running() {
        assert!(!ServiceStatus::Running.can_start());
    }

    #[test]
    fn status_cannot_start_from_not_compiled() {
        assert!(!ServiceStatus::NotCompiled.can_start());
    }

    #[test]
    fn status_can_stop_from_running() {
        assert!(ServiceStatus::Running.can_stop());
    }

    #[test]
    fn status_cannot_stop_from_stopped() {
        assert!(!ServiceStatus::Stopped.can_stop());
    }

    #[test]
    fn status_as_str_matches_expected_values() {
        // These string values must match the proto `ServiceInfo.status` field
        // and the MQTT message format.
        assert_eq!(ServiceStatus::Running.as_str(), "running");
        assert_eq!(ServiceStatus::Stopped.as_str(), "stopped");
        assert_eq!(ServiceStatus::Starting.as_str(), "starting");
        assert_eq!(ServiceStatus::Stopping.as_str(), "stopping");
        assert_eq!(ServiceStatus::NotCompiled.as_str(), "not_compiled");
        let failed = ServiceStatus::Failed {
            reason: "test".to_string(),
        };
        assert_eq!(failed.as_str(), "failed");
    }

    #[test]
    fn status_display_includes_reason_for_failed() {
        let status = ServiceStatus::Failed {
            reason: "port 8080 already in use".to_string(),
        };
        let display = format!("{}", status);
        assert!(display.contains("port 8080 already in use"));
    }

    // -- ServiceEvent tests --

    #[test]
    fn event_display_started() {
        let event = ServiceEvent::Started {
            name: "rest".to_string(),
        };
        assert_eq!(format!("{}", event), "service 'rest' started");
    }

    #[test]
    fn event_display_stopped() {
        let event = ServiceEvent::Stopped {
            name: "grpc".to_string(),
        };
        assert_eq!(format!("{}", event), "service 'grpc' stopped");
    }

    #[test]
    fn event_display_failed() {
        let event = ServiceEvent::Failed {
            name: "mqtt".to_string(),
            reason: "broker unreachable".to_string(),
        };
        let display = format!("{}", event);
        assert!(display.contains("mqtt"));
        assert!(display.contains("broker unreachable"));
    }

    // -- ServiceInfo tests --

    #[test]
    fn service_info_display() {
        let info = ServiceInfo {
            name: "rest".to_string(),
            status: ServiceStatus::Running,
            compiled: true,
            enabled: true,
        };
        let display = format!("{}", info);
        assert!(display.contains("rest"));
        assert!(display.contains("running"));
        assert!(display.contains("compiled=true"));
        assert!(display.contains("enabled=true"));
    }

    #[test]
    fn service_info_serializes_to_json() {
        let info = ServiceInfo {
            name: "grpc".to_string(),
            status: ServiceStatus::Stopped,
            compiled: true,
            enabled: false,
        };
        let json = serde_json::to_string(&info).expect("should serialize");
        assert!(json.contains("\"name\":\"grpc\""));
        assert!(json.contains("\"compiled\":true"));
        assert!(json.contains("\"enabled\":false"));
    }

    // -- ServiceError tests --

    #[test]
    fn error_not_found_display() {
        let err = ServiceError::NotFound {
            name: "foobar".to_string(),
        };
        assert!(format!("{}", err).contains("foobar"));
        assert!(format!("{}", err).contains("not found"));
    }

    #[test]
    fn error_already_in_state_display() {
        let err = ServiceError::AlreadyInState {
            name: "rest".to_string(),
            status: "running".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("rest"));
        assert!(display.contains("running"));
    }

    #[test]
    fn error_transitioning_display() {
        let err = ServiceError::Transitioning {
            name: "grpc".to_string(),
            status: "starting".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("grpc"));
        assert!(display.contains("starting"));
    }
}
