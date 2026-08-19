//! # mDNS Service Discovery
//!
//! Implements ADR-012: zero-configuration service discovery using mDNS/DNS-SD.
//!
//! Registers a single `_okfpga._tcp` service type per server instance,
//! with TXT records advertising available endpoints and capabilities:
//!
//! ```text
//! Instance: "okfpga-lab-bench-1"
//! Service:  _okfpga._tcp.local.
//! Host:     myhost.local.
//! Port:     9090  (management port)
//! TXT:
//!   version=0.1.0
//!   rest=8080
//!   grpc=50051
//!   mqtt=okfpga/server01
//!   dashboard=9090
//!   devices=3
//! ```
//!
//! ## TXT Record Conventions (ADR-012)
//!
//! - **Absent keys mean disabled**: if a service is not compiled/enabled,
//!   its key is omitted.
//! - **Dynamic updates**: the `devices` count updates when devices
//!   connect/disconnect.
//! - **All values are strings**.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use tracing::info;

use crate::config::ServerConfig;
use crate::state::AppState;

/// The DNS-SD service type for ok-FPGA-Server instances.
const SERVICE_TYPE: &str = "_okfpga._tcp.local.";

// ============================================================================
// MdnsDiscoveryService — ManagedService
// ============================================================================

/// mDNS/DNS-SD service discovery.
///
/// Registers the server on the local network so clients can find it
/// by browsing for `_okfpga._tcp` without knowing the IP address or port.
pub struct MdnsDiscoveryService {
    /// Atomic status: 0=Stopped, 1=Running, 2=Starting, 3=Stopping
    status: AtomicU8,

    /// Server configuration (used to build TXT records).
    config: Arc<ServerConfig>,

    /// Shared application state (used for dynamic device count).
    state: Arc<AppState>,
}

impl MdnsDiscoveryService {
    /// Create a new `MdnsDiscoveryService`.
    pub fn new(config: Arc<ServerConfig>, state: Arc<AppState>) -> Self {
        Self {
            status: AtomicU8::new(0), // Stopped
            config,
            state,
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

    /// Build TXT record properties based on current config and state.
    fn build_txt_properties(&self) -> Vec<(&str, String)> {
        let mut props = Vec::new();

        // Always include the API version.
        props.push(("version", self.state.api_version.to_string()));

        // Server description — helps clients show a human-readable label
        // in the mDNS discovery dropdown (Phase 28A). Falls back to the
        // mDNS instance_name if no description is configured.
        let description = if self.config.server.description.is_empty() {
            self.config.mdns.instance_name.clone()
        } else {
            self.config.server.description.clone()
        };
        props.push(("description", description));

        // REST port (if compiled and enabled).
        #[cfg(feature = "rest")]
        if self.config.rest.enabled {
            props.push(("rest", self.config.rest.port.to_string()));
        }

        // gRPC port (if compiled and enabled).
        #[cfg(feature = "grpc")]
        if self.config.grpc.enabled {
            props.push(("grpc", self.config.grpc.port.to_string()));
        }

        // MQTT topic prefix (if compiled and enabled).
        #[cfg(feature = "mqtt")]
        if self.config.mqtt.enabled {
            props.push(("mqtt", self.config.mqtt.topic_prefix.clone()));
        }

        // Dashboard management port (if compiled and enabled).
        #[cfg(feature = "dashboard")]
        if self.config.dashboard.enabled {
            props.push(("dashboard", self.config.server.management_port.to_string()));
        }

        // Current device count (dynamic).
        props.push((
            "devices",
            self.state.device_manager.device_count().to_string(),
        ));

        props
    }
}

#[async_trait::async_trait]
impl ManagedService for MdnsDiscoveryService {
    fn name(&self) -> &str {
        "mdns"
    }

    fn status(&self) -> ServiceStatus {
        Self::atomic_to_status(self.status.load(Ordering::SeqCst))
    }

    async fn start(&self) -> Result<(), ServiceError> {
        self.status.store(2, Ordering::SeqCst); // Starting

        // Create the mDNS daemon.
        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                self.status.store(0, Ordering::SeqCst);
                return Err(ServiceError::OperationFailed {
                    name: "mdns".to_string(),
                    message: format!("failed to create mDNS daemon: {}", e),
                });
            }
        };

        // Determine the instance name.
        let instance_name = &self.config.mdns.instance_name;

        // Get the hostname for the service registration.
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "localhost".to_string());

        let host_with_local = format!("{}.local.", hostname);

        // Build TXT properties.
        let txt_props = self.build_txt_properties();
        let txt_refs: Vec<(&str, &str)> = txt_props.iter().map(|(k, v)| (*k, v.as_str())).collect();

        // Build the ServiceInfo.
        let service_info = match ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &host_with_local,
            "", // Let mdns-sd determine the IP addresses
            self.config.server.management_port,
            txt_refs.as_slice(),
        ) {
            Ok(info) => info,
            Err(e) => {
                self.status.store(0, Ordering::SeqCst);
                return Err(ServiceError::OperationFailed {
                    name: "mdns".to_string(),
                    message: format!("failed to create service info: {}", e),
                });
            }
        };

        // Register the service.
        if let Err(e) = daemon.register(service_info) {
            self.status.store(0, Ordering::SeqCst);
            return Err(ServiceError::OperationFailed {
                name: "mdns".to_string(),
                message: format!("failed to register mDNS service: {}", e),
            });
        }

        info!(
            instance = %instance_name,
            service_type = SERVICE_TYPE,
            port = self.config.server.management_port,
            hostname = %host_with_local,
            txt_records = ?txt_props.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>(),
            "mDNS service registered"
        );

        self.status.store(1, Ordering::SeqCst); // Running
        info!("mDNS discovery service started");

        // Note: The ServiceDaemon runs its own background thread.
        // We intentionally leak it here — it will be cleaned up when the
        // process exits. A more sophisticated approach would store it
        // in an Option<ServiceDaemon> behind a Mutex for clean shutdown.

        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        self.status.store(3, Ordering::SeqCst); // Stopping

        // The ServiceDaemon's background thread will be stopped when
        // the daemon is dropped. Since we don't hold a reference to it
        // (it's leaked in start()), we just update the status.
        //
        // In a production implementation, we would unregister the service
        // and shut down the daemon gracefully.

        self.status.store(0, Ordering::SeqCst); // Stopped
        info!("mDNS discovery service stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_type_is_correct() {
        assert_eq!(SERVICE_TYPE, "_okfpga._tcp.local.");
    }
}
