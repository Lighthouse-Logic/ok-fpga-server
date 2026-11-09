// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Service Manager
//!
//! Centralized lifecycle controller for all optional server services.
//! Implements ADR-010: each service (REST, gRPC, MQTT, mDNS) is registered
//! at startup, and the manager controls start/stop/restart for all of them.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │                  ServiceManager                      │
//! │                                                      │
//! │  ┌──────────────────────────────────────────────┐   │
//! │  │ services: IndexMap<String, Arc<dyn Managed>>  │   │
//! │  │  "rest"  → RestService                        │   │
//! │  │  "grpc"  → GrpcService                        │   │
//! │  │  "mqtt"  → MqttPublisher                      │   │
//! │  │  "mdns"  → MdnsService                        │   │
//! │  └──────────────────────────────────────────────┘   │
//! │                                                      │
//! │  ┌──────────────────────┐  ┌─────────────────────┐  │
//! │  │ event_tx (broadcast) │  │ enabled: HashMap     │  │
//! │  │ ServiceEvent         │  │ (from TOML config)   │  │
//! │  └──────────┬───────────┘  └─────────────────────┘  │
//! │             │                                        │
//! └─────────────┼────────────────────────────────────────┘
//!               │
//!        ┌──────┴──────┬──────────────┐
//!        ▼             ▼              ▼
//!     MQTT          mDNS        Web Dashboard
//!   (publishes    (updates     (shows service
//!    status)     TXT records)    status)
//! ```
//!
//! ## Shutdown Ordering (ADR-010 §4)
//!
//! During graceful shutdown, services are stopped in reverse-registration
//! order, **except** MQTT is stopped last so it can publish final status
//! messages about other services shutting down.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::service::{ManagedService, ServiceError, ServiceEvent, ServiceInfo, ServiceStatus};

// ============================================================================
// Configuration
// ============================================================================

/// Default capacity for the service event broadcast channel.
///
/// This must be large enough to buffer events while subscribers are
/// processing previous events (e.g., MQTT publishing, mDNS TXT updates).
/// 64 is generous for the typical 4-service setup.
const SERVICE_EVENT_CHANNEL_CAPACITY: usize = 64;

// ============================================================================
// ServiceManager
// ============================================================================

/// Centralized controller for all optional server services.
///
/// The `ServiceManager` maintains an ordered registry of services and provides
/// methods to start, stop, restart, and query them. It broadcasts
/// [`ServiceEvent`]s to subscribers whenever a
/// service's state changes.
///
/// ## Usage
///
/// ```rust,no_run
/// use ok_fpga_core::service_manager::ServiceManager;
///
/// let manager = ServiceManager::new();
///
/// // Register services (typically done at startup from main.rs)
/// // manager.register(Arc::new(my_rest_service));
/// // manager.register(Arc::new(my_grpc_service));
///
/// // Subscribe to lifecycle events
/// let mut rx = manager.subscribe();
/// ```
///
/// ## Thread Safety
///
/// The `ServiceManager` is designed to be shared via `Arc<ServiceManager>`.
/// All internal state is protected by appropriate synchronization primitives.
/// Service lifecycle operations acquire a per-service lock to prevent
/// concurrent start/stop races on the same service.
pub struct ServiceManager {
    /// Ordered map of service name → service implementation.
    ///
    /// Uses `Vec` to preserve insertion order, which determines shutdown
    /// order (reverse iteration). A `HashMap` is kept separately for O(1)
    /// lookups by name.
    service_order: std::sync::RwLock<Vec<String>>,

    /// Fast lookup: service name → (service implementation, enabled flag).
    ///
    /// The `enabled` flag comes from TOML configuration and determines
    /// whether the service should be started automatically.
    services: std::sync::RwLock<HashMap<String, ServiceEntry>>,

    /// Broadcast channel for service lifecycle events.
    ///
    /// Subscribers (MQTT, mDNS, dashboard) receive events when services
    /// start, stop, or fail.
    event_tx: broadcast::Sender<ServiceEvent>,
}

/// Internal entry for a registered service.
///
/// Combines the service implementation with its configuration flag.
struct ServiceEntry {
    /// The service implementation.
    service: Arc<dyn ManagedService>,

    /// Whether this service is enabled in the TOML config file.
    /// An enabled service will be auto-started by the manager at startup.
    enabled: bool,
}

impl ServiceManager {
    /// Creates a new, empty `ServiceManager`.
    ///
    /// No services are registered initially. Call [`register()`](Self::register)
    /// to add services, then [`start_enabled()`](Self::start_enabled) to
    /// start all services that are enabled in configuration.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(SERVICE_EVENT_CHANNEL_CAPACITY);
        Self {
            service_order: std::sync::RwLock::new(Vec::new()),
            services: std::sync::RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    // ========================================================================
    // Registration
    // ========================================================================

    /// Register a service with the manager.
    ///
    /// The `enabled` flag indicates whether this service should be
    /// auto-started (from TOML config). Registration order determines
    /// shutdown order (last registered = first stopped, except MQTT).
    ///
    /// # Panics
    ///
    /// Panics if a service with the same name is already registered.
    /// This is a programming error — service names must be unique.
    pub fn register(&self, service: Arc<dyn ManagedService>, enabled: bool) {
        let name = service.name().to_string();

        let mut services = self
            .services
            .write()
            .expect("ServiceManager services lock poisoned");

        if services.contains_key(&name) {
            panic!(
                "BUG: service '{}' registered twice — each service name must be unique",
                name
            );
        }

        services.insert(name.clone(), ServiceEntry { service, enabled });

        // Track insertion order for shutdown sequencing.
        let mut order = self
            .service_order
            .write()
            .expect("ServiceManager order lock poisoned");
        order.push(name.clone());

        info!(service = %name, enabled = enabled, "registered service");
    }

    // ========================================================================
    // Lifecycle Operations
    // ========================================================================

    /// Start a single service by name.
    ///
    /// Returns `Ok(())` if the service started successfully.
    /// Returns an error if the service is not found, not compiled,
    /// already running, or fails to start.
    pub async fn start(&self, name: &str) -> Result<(), ServiceError> {
        let service = self.get_service(name)?;
        let status = service.status();

        // Pre-flight checks
        if status == ServiceStatus::NotCompiled {
            return Err(ServiceError::NotCompiled {
                name: name.to_string(),
            });
        }
        if status.is_running() {
            return Err(ServiceError::AlreadyInState {
                name: name.to_string(),
                status: status.as_str().to_string(),
            });
        }
        if status.is_transitioning() {
            return Err(ServiceError::Transitioning {
                name: name.to_string(),
                status: status.as_str().to_string(),
            });
        }

        info!(service = %name, "starting service");

        match service.start().await {
            Ok(()) => {
                info!(service = %name, "service started");
                // Broadcast the event. Ignore send errors (no subscribers is fine).
                let _ = self.event_tx.send(ServiceEvent::Started {
                    name: name.to_string(),
                });
                Ok(())
            }
            Err(e) => {
                error!(service = %name, error = %e, "service failed to start");
                let _ = self.event_tx.send(ServiceEvent::Failed {
                    name: name.to_string(),
                    reason: e.to_string(),
                });
                Err(e)
            }
        }
    }

    /// Stop a single service by name.
    ///
    /// Returns `Ok(())` if the service stopped successfully.
    /// Returns an error if the service is not found, not running, or
    /// fails to stop gracefully.
    pub async fn stop(&self, name: &str) -> Result<(), ServiceError> {
        let service = self.get_service(name)?;
        let status = service.status();

        if !status.can_stop() {
            return Err(ServiceError::AlreadyInState {
                name: name.to_string(),
                status: status.as_str().to_string(),
            });
        }
        if status.is_transitioning() {
            return Err(ServiceError::Transitioning {
                name: name.to_string(),
                status: status.as_str().to_string(),
            });
        }

        info!(service = %name, "stopping service");

        match service.stop().await {
            Ok(()) => {
                info!(service = %name, "service stopped");
                let _ = self.event_tx.send(ServiceEvent::Stopped {
                    name: name.to_string(),
                });
                Ok(())
            }
            Err(e) => {
                error!(service = %name, error = %e, "service failed to stop");
                Err(e)
            }
        }
    }

    /// Restart a single service by name.
    ///
    /// Delegates to the service's own `restart()` method, which defaults
    /// to stop-then-start but can be overridden for efficiency.
    ///
    /// Returns `Ok(())` when the service is fully restarted.
    pub async fn restart(&self, name: &str) -> Result<(), ServiceError> {
        let service = self.get_service(name)?;
        let status = service.status();

        if status == ServiceStatus::NotCompiled {
            return Err(ServiceError::NotCompiled {
                name: name.to_string(),
            });
        }
        if status.is_transitioning() {
            return Err(ServiceError::Transitioning {
                name: name.to_string(),
                status: status.as_str().to_string(),
            });
        }

        info!(service = %name, "restarting service");

        // If the service is currently stopped/failed, just start it.
        if !status.is_running() {
            return self.start(name).await;
        }

        match service.restart().await {
            Ok(()) => {
                info!(service = %name, "service restarted");
                // Broadcast both stop and start events so subscribers
                // see the full lifecycle.
                let _ = self.event_tx.send(ServiceEvent::Stopped {
                    name: name.to_string(),
                });
                let _ = self.event_tx.send(ServiceEvent::Started {
                    name: name.to_string(),
                });
                Ok(())
            }
            Err(e) => {
                error!(service = %name, error = %e, "service restart failed");
                let _ = self.event_tx.send(ServiceEvent::Failed {
                    name: name.to_string(),
                    reason: e.to_string(),
                });
                Err(e)
            }
        }
    }

    /// Start all services that are marked as `enabled` in configuration.
    ///
    /// This is called once at server startup. Services that fail to start
    /// are logged as errors but do not prevent other services from starting.
    pub async fn start_enabled(&self) {
        let names: Vec<String> = {
            let order = self
                .service_order
                .read()
                .expect("ServiceManager order lock poisoned");
            order.clone()
        };

        for name in &names {
            let should_start = {
                let services = self
                    .services
                    .read()
                    .expect("ServiceManager services lock poisoned");
                services
                    .get(name)
                    .map(|entry| entry.enabled)
                    .unwrap_or(false)
            };

            if !should_start {
                info!(service = %name, "service disabled in config — skipping");
                continue;
            }

            // Check if it's a NotCompiled service (enabled in config but
            // feature flag not set at compile time).
            let status = {
                let services = self
                    .services
                    .read()
                    .expect("ServiceManager services lock poisoned");
                services
                    .get(name)
                    .map(|entry| entry.service.status())
                    .unwrap_or(ServiceStatus::NotCompiled)
            };

            if status == ServiceStatus::NotCompiled {
                warn!(
                    service = %name,
                    "service enabled in config but feature flag was not compiled — skipping"
                );
                continue;
            }

            if let Err(e) = self.start(name).await {
                error!(
                    service = %name,
                    error = %e,
                    "failed to start enabled service — continuing with remaining services"
                );
            }
        }
    }

    /// Gracefully stop all running services.
    ///
    /// Services are stopped in reverse-registration order, **except** MQTT
    /// is stopped last so it can publish final shutdown status messages
    /// for other services (ADR-010 §4).
    ///
    /// Errors during shutdown are logged but do not prevent other services
    /// from being stopped.
    pub async fn stop_all(&self) {
        let names: Vec<String> = {
            let order = self
                .service_order
                .read()
                .expect("ServiceManager order lock poisoned");
            order.clone()
        };

        // Partition into MQTT (deferred) and everything else.
        let mut mqtt_name: Option<String> = None;
        let mut others: Vec<String> = Vec::new();

        for name in names.into_iter().rev() {
            if name == "mqtt" {
                mqtt_name = Some(name);
            } else {
                others.push(name);
            }
        }

        // Stop non-MQTT services in reverse order.
        for name in &others {
            let should_stop = {
                let services = self
                    .services
                    .read()
                    .expect("ServiceManager services lock poisoned");
                services
                    .get(name)
                    .map(|entry| entry.service.status().is_running())
                    .unwrap_or(false)
            };

            if should_stop {
                if let Err(e) = self.stop(name).await {
                    error!(
                        service = %name,
                        error = %e,
                        "error stopping service during shutdown — continuing"
                    );
                }
            }
        }

        // Stop MQTT last so it can publish the shutdown events.
        if let Some(name) = mqtt_name {
            let should_stop = {
                let services = self
                    .services
                    .read()
                    .expect("ServiceManager services lock poisoned");
                services
                    .get(&name)
                    .map(|entry| entry.service.status().is_running())
                    .unwrap_or(false)
            };

            if should_stop {
                if let Err(e) = self.stop(&name).await {
                    error!(
                        service = %name,
                        error = %e,
                        "error stopping MQTT during shutdown"
                    );
                }
            }
        }

        info!("all services stopped");
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// List all registered services with their current status.
    ///
    /// Returns a snapshot of each service's name, status, compiled flag,
    /// and enabled flag. The list is in registration order.
    pub fn list(&self) -> Vec<ServiceInfo> {
        let order = self
            .service_order
            .read()
            .expect("ServiceManager order lock poisoned");
        let services = self
            .services
            .read()
            .expect("ServiceManager services lock poisoned");

        order
            .iter()
            .filter_map(|name| {
                services.get(name).map(|entry| ServiceInfo {
                    name: name.clone(),
                    status: entry.service.status(),
                    compiled: entry.service.status() != ServiceStatus::NotCompiled,
                    enabled: entry.enabled,
                })
            })
            .collect()
    }

    /// Get the status of a single service by name.
    ///
    /// Returns `None` if no service with that name is registered.
    pub fn get_status(&self, name: &str) -> Option<ServiceInfo> {
        let services = self
            .services
            .read()
            .expect("ServiceManager services lock poisoned");

        services.get(name).map(|entry| ServiceInfo {
            name: name.to_string(),
            status: entry.service.status(),
            compiled: entry.service.status() != ServiceStatus::NotCompiled,
            enabled: entry.enabled,
        })
    }

    /// Get the number of registered services.
    pub fn service_count(&self) -> usize {
        let services = self
            .services
            .read()
            .expect("ServiceManager services lock poisoned");
        services.len()
    }

    // ========================================================================
    // Event Subscription
    // ========================================================================

    /// Subscribe to service lifecycle events.
    ///
    /// Returns a `broadcast::Receiver` that yields [`ServiceEvent`]s
    /// whenever any service starts, stops, or fails. Multiple subscribers
    /// can exist simultaneously.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ok_fpga_core::service_manager::ServiceManager;
    ///
    /// let manager = ServiceManager::new();
    /// let mut rx = manager.subscribe();
    ///
    /// // In a background task:
    /// // while let Ok(event) = rx.recv().await {
    /// //     println!("Service event: {}", event);
    /// // }
    /// ```
    pub fn subscribe(&self) -> broadcast::Receiver<ServiceEvent> {
        self.event_tx.subscribe()
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Look up a service by name, returning a clone of its `Arc`.
    ///
    /// Returns `Err(ServiceError::NotFound)` if no service with that name
    /// is registered.
    fn get_service(&self, name: &str) -> Result<Arc<dyn ManagedService>, ServiceError> {
        let services = self
            .services
            .read()
            .expect("ServiceManager services lock poisoned");

        services
            .get(name)
            .map(|entry| Arc::clone(&entry.service))
            .ok_or_else(|| ServiceError::NotFound {
                name: name.to_string(),
            })
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};

    // -- Mock Service --

    /// A mock service for testing the ServiceManager.
    ///
    /// Uses an atomic to track status transitions without needing a mutex.
    struct MockService {
        name: String,
        /// Atomic status: 0=Stopped, 1=Running, 2=Starting, 3=Stopping,
        /// 4=Failed, 5=NotCompiled
        state: AtomicU8,
        /// If true, start() will fail with an error.
        should_fail: bool,
    }

    impl MockService {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                state: AtomicU8::new(0), // Stopped
                should_fail: false,
            }
        }

        fn new_failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                state: AtomicU8::new(0), // Stopped
                should_fail: true,
            }
        }

        fn new_not_compiled(name: &str) -> Self {
            Self {
                name: name.to_string(),
                state: AtomicU8::new(5), // NotCompiled
                should_fail: false,
            }
        }

        fn atomic_to_status(val: u8) -> ServiceStatus {
            match val {
                0 => ServiceStatus::Stopped,
                1 => ServiceStatus::Running,
                2 => ServiceStatus::Starting,
                3 => ServiceStatus::Stopping,
                4 => ServiceStatus::Failed {
                    reason: "mock failure".to_string(),
                },
                5 => ServiceStatus::NotCompiled,
                _ => unreachable!("invalid mock state"),
            }
        }
    }

    #[async_trait::async_trait]
    impl ManagedService for MockService {
        fn name(&self) -> &str {
            &self.name
        }

        fn status(&self) -> ServiceStatus {
            Self::atomic_to_status(self.state.load(Ordering::SeqCst))
        }

        async fn start(&self) -> Result<(), ServiceError> {
            if self.should_fail {
                self.state.store(4, Ordering::SeqCst); // Failed
                return Err(ServiceError::OperationFailed {
                    name: self.name.clone(),
                    message: "mock start failure".to_string(),
                });
            }
            self.state.store(1, Ordering::SeqCst); // Running
            Ok(())
        }

        async fn stop(&self) -> Result<(), ServiceError> {
            self.state.store(0, Ordering::SeqCst); // Stopped
            Ok(())
        }
    }

    // -- Tests --

    #[test]
    fn new_manager_has_no_services() {
        let manager = ServiceManager::new();
        assert_eq!(manager.service_count(), 0);
        assert!(manager.list().is_empty());
    }

    #[test]
    fn register_adds_service() {
        let manager = ServiceManager::new();
        let svc = Arc::new(MockService::new("test"));
        manager.register(svc, true);

        assert_eq!(manager.service_count(), 1);
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test");
        assert!(list[0].enabled);
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn register_duplicate_panics() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);
        manager.register(Arc::new(MockService::new("rest")), false);
    }

    #[test]
    fn list_preserves_registration_order() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);
        manager.register(Arc::new(MockService::new("grpc")), true);
        manager.register(Arc::new(MockService::new("mqtt")), false);
        manager.register(Arc::new(MockService::new("mdns")), false);

        let list = manager.list();
        let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["rest", "grpc", "mqtt", "mdns"]);
    }

    #[tokio::test]
    async fn start_transitions_to_running() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        manager.start("rest").await.unwrap();

        let info = manager.get_status("rest").unwrap();
        assert!(info.status.is_running());
    }

    #[tokio::test]
    async fn stop_transitions_to_stopped() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        manager.start("rest").await.unwrap();
        manager.stop("rest").await.unwrap();

        let info = manager.get_status("rest").unwrap();
        assert_eq!(info.status, ServiceStatus::Stopped);
    }

    #[tokio::test]
    async fn start_not_found_returns_error() {
        let manager = ServiceManager::new();
        let result = manager.start("nonexistent").await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::NotFound { .. }));
    }

    #[tokio::test]
    async fn start_already_running_returns_error() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        manager.start("rest").await.unwrap();
        let result = manager.start("rest").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ServiceError::AlreadyInState { .. }
        ));
    }

    #[tokio::test]
    async fn stop_already_stopped_returns_error() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        let result = manager.stop("rest").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ServiceError::AlreadyInState { .. }
        ));
    }

    #[tokio::test]
    async fn start_not_compiled_returns_error() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new_not_compiled("grpc")), true);

        let result = manager.start("grpc").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ServiceError::NotCompiled { .. }
        ));
    }

    #[tokio::test]
    async fn restart_running_service() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        manager.start("rest").await.unwrap();
        manager.restart("rest").await.unwrap();

        let info = manager.get_status("rest").unwrap();
        assert!(info.status.is_running());
    }

    #[tokio::test]
    async fn restart_stopped_service_starts_it() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        // Service is stopped — restart should just start it.
        manager.restart("rest").await.unwrap();

        let info = manager.get_status("rest").unwrap();
        assert!(info.status.is_running());
    }

    #[tokio::test]
    async fn start_enabled_starts_only_enabled_services() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);
        manager.register(Arc::new(MockService::new("grpc")), false);
        manager.register(Arc::new(MockService::new("mqtt")), true);

        manager.start_enabled().await;

        let list = manager.list();
        // rest: enabled=true → should be running
        assert!(list[0].status.is_running(), "rest should be running");
        // grpc: enabled=false → should still be stopped
        assert_eq!(
            list[1].status,
            ServiceStatus::Stopped,
            "grpc should be stopped"
        );
        // mqtt: enabled=true → should be running
        assert!(list[2].status.is_running(), "mqtt should be running");
    }

    #[tokio::test]
    async fn start_enabled_skips_not_compiled_services() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);
        manager.register(Arc::new(MockService::new_not_compiled("grpc")), true);

        manager.start_enabled().await;

        let list = manager.list();
        assert!(list[0].status.is_running(), "rest should be running");
        assert_eq!(
            list[1].status,
            ServiceStatus::NotCompiled,
            "grpc should remain NotCompiled"
        );
    }

    #[tokio::test]
    async fn start_enabled_continues_on_failure() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new_failing("rest")), true);
        manager.register(Arc::new(MockService::new("grpc")), true);

        manager.start_enabled().await;

        let list = manager.list();
        // rest failed to start but grpc should still be running.
        assert!(
            matches!(list[0].status, ServiceStatus::Failed { .. }),
            "rest should be Failed"
        );
        assert!(list[1].status.is_running(), "grpc should be running");
    }

    #[tokio::test]
    async fn stop_all_stops_in_reverse_order_mqtt_last() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);
        manager.register(Arc::new(MockService::new("grpc")), true);
        manager.register(Arc::new(MockService::new("mqtt")), true);
        manager.register(Arc::new(MockService::new("mdns")), true);

        // Start all services.
        manager.start_enabled().await;

        // Subscribe to events to verify order.
        let mut rx = manager.subscribe();

        manager.stop_all().await;

        // All services should be stopped.
        let list = manager.list();
        for info in &list {
            assert_eq!(
                info.status,
                ServiceStatus::Stopped,
                "{} should be stopped",
                info.name
            );
        }

        // Collect stop events to verify MQTT was last.
        let mut stop_order = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let ServiceEvent::Stopped { name } = event {
                stop_order.push(name);
            }
        }

        // MQTT should be the last in the stop_order.
        assert_eq!(
            stop_order.last().map(|s| s.as_str()),
            Some("mqtt"),
            "MQTT should be stopped last, but stop order was: {:?}",
            stop_order
        );
    }

    #[tokio::test]
    async fn subscribe_receives_start_event() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        let mut rx = manager.subscribe();

        manager.start("rest").await.unwrap();

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, ServiceEvent::Started { name } if name == "rest"));
    }

    #[tokio::test]
    async fn subscribe_receives_stop_event() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        manager.start("rest").await.unwrap();

        let mut rx = manager.subscribe();
        manager.stop("rest").await.unwrap();

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, ServiceEvent::Stopped { name } if name == "rest"));
    }

    #[tokio::test]
    async fn subscribe_receives_failed_event_on_start_failure() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new_failing("rest")), true);

        let mut rx = manager.subscribe();

        let _ = manager.start("rest").await; // Will fail

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, ServiceEvent::Failed { name, .. } if name == "rest"),
            "expected Failed event"
        );
    }

    #[test]
    fn get_status_returns_none_for_unknown_service() {
        let manager = ServiceManager::new();
        assert!(manager.get_status("unknown").is_none());
    }

    #[test]
    fn get_status_returns_info_for_registered_service() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new("rest")), true);

        let info = manager.get_status("rest").unwrap();
        assert_eq!(info.name, "rest");
        assert_eq!(info.status, ServiceStatus::Stopped);
        assert!(info.compiled);
        assert!(info.enabled);
    }

    #[test]
    fn not_compiled_service_shows_compiled_false() {
        let manager = ServiceManager::new();
        manager.register(Arc::new(MockService::new_not_compiled("grpc")), true);

        let info = manager.get_status("grpc").unwrap();
        assert!(!info.compiled);
    }
}
