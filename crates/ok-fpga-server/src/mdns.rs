//! # mDNS Service Discovery
//!
//! Implements ADR-012 (service type, TXT records) and ADR-027 (OS-native
//! registration with embedded fallback).
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
//! ## Registration Strategy (ADR-027)
//!
//! 1. **Primary**: Register via the OS's native mDNS daemon using
//!    `dns-sd-native` (Windows: dnscache/`DnsServiceRegister`,
//!    Linux: avahi-daemon/D-Bus). This avoids the dual-responder conflict
//!    where an embedded mDNS stack competes with the OS daemon on UDP 5353.
//!
//! 2. **Fallback**: If the OS daemon is not running, fall back to the
//!    embedded `mdns-sd` responder. This is safe because without an OS
//!    daemon, there is no port conflict — `mdns-sd` is the sole mDNS
//!    stack on the machine.
//!
//! 3. **Error**: If the OS daemon IS running but registration fails,
//!    report an error (do not fall back — the dual-responder conflict
//!    would reappear).
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

use dns_sd_native::{ServiceRegistrationBuilder, ServiceRegistration, ServiceRegistrationError};
use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use tracing::{info, warn, error};

use crate::config::ServerConfig;
use crate::state::AppState;

/// The DNS-SD service type for ok-FPGA-Server instances.
/// Used by the fallback `mdns-sd` responder which expects the `.local.` suffix.
const SERVICE_TYPE: &str = "_okfpga._tcp.local.";

/// The DNS-SD service type without the `.local.` suffix, as required by
/// `dns_sd_native::ServiceRegistrationBuilder`.
const SERVICE_TYPE_SHORT: &str = "_okfpga._tcp";

// ============================================================================
// Registration Backend — tracks which registration path is active
// ============================================================================

/// Tracks the active registration backend for clean shutdown.
///
/// The inner values are not read directly — they are held so their `Drop`
/// implementations deregister the service when the backend is dropped.
#[allow(dead_code)]
enum RegistrationBackend {
    /// Registered via the OS's native mDNS daemon (dns-sd-native).
    /// The handle deregisters automatically when dropped, or explicitly
    /// via `unregister()`.
    OsNative(ServiceRegistration),

    /// Registered via the embedded mdns-sd responder (fallback).
    /// Stores the daemon for clean shutdown.
    Fallback(mdns_sd::ServiceDaemon),
}

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

    /// Active registration backend, if any.
    backend: std::sync::Mutex<Option<RegistrationBackend>>,
}

impl MdnsDiscoveryService {
    /// Create a new `MdnsDiscoveryService`.
    pub fn new(config: Arc<ServerConfig>, state: Arc<AppState>) -> Self {
        Self {
            status: AtomicU8::new(0), // Stopped
            config,
            state,
            backend: std::sync::Mutex::new(None),
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

    /// Build TXT record key-value pairs based on current config and state.
    fn build_txt_properties(&self) -> Vec<(String, String)> {
        let mut props = Vec::new();

        // Always include the API version.
        props.push(("version".to_string(), self.state.api_version.to_string()));

        // Server description — helps clients show a human-readable label
        // in the mDNS discovery dropdown (Phase 28A). Falls back to the
        // mDNS instance_name if no description is configured.
        let description = if self.config.server.description.is_empty() {
            self.config.mdns.instance_name.clone()
        } else {
            self.config.server.description.clone()
        };
        props.push(("description".to_string(), description));

        // REST port (if compiled and enabled).
        #[cfg(feature = "rest")]
        if self.config.rest.enabled {
            props.push(("rest".to_string(), self.config.rest.port.to_string()));
        }

        // gRPC port (if compiled and enabled).
        #[cfg(feature = "grpc")]
        if self.config.grpc.enabled {
            props.push(("grpc".to_string(), self.config.grpc.port.to_string()));
        }

        // MQTT topic prefix (if compiled and enabled).
        #[cfg(feature = "mqtt")]
        if self.config.mqtt.enabled {
            props.push(("mqtt".to_string(), self.config.mqtt.topic_prefix.clone()));
        }

        // Dashboard management port (if compiled and enabled).
        #[cfg(feature = "dashboard")]
        if self.config.dashboard.enabled {
            props.push((
                "dashboard".to_string(),
                self.config.server.management_port.to_string(),
            ));
        }

        // Current device count (dynamic).
        props.push((
            "devices".to_string(),
            self.state.device_manager.device_count().to_string(),
        ));

        props
    }

    /// Attempt registration via the OS's native mDNS daemon (dns-sd-native).
    ///
    /// Returns `Ok(RegistrationBackend::OsNative)` on success, or the
    /// `ServiceRegistrationError` describing why it failed.
    async fn try_os_native_registration(
        &self,
        instance_name: &str,
        txt_props: &[(String, String)],
    ) -> Result<RegistrationBackend, ServiceRegistrationError> {
        let mut builder =
            ServiceRegistrationBuilder::new(SERVICE_TYPE_SHORT, self.config.server.management_port);
        builder.name(instance_name);

        for (key, value) in txt_props {
            builder.add_txt_record_key_string(key, value);
        }

        let registration = builder.register().await?;
        Ok(RegistrationBackend::OsNative(registration))
    }

    /// Fall back to the embedded mdns-sd responder.
    ///
    /// This should only be called when the OS daemon is NOT running —
    /// otherwise we'd recreate the dual-responder conflict.
    fn try_fallback_registration(
        &self,
        instance_name: &str,
        hostname: &str,
        txt_props: &[(String, String)],
    ) -> Result<RegistrationBackend, ServiceError> {
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                return Err(ServiceError::OperationFailed {
                    name: "mdns".to_string(),
                    message: format!("failed to create fallback mDNS daemon: {}", e),
                });
            }
        };

        let host_with_local = format!("{}.local.", hostname);
        let txt_refs: Vec<(&str, &str)> = txt_props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let service_info = match mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &host_with_local,
            "", // Let mdns-sd determine the IP addresses
            self.config.server.management_port,
            txt_refs.as_slice(),
        ) {
            Ok(info) => info,
            Err(e) => {
                return Err(ServiceError::OperationFailed {
                    name: "mdns".to_string(),
                    message: format!("failed to create fallback service info: {}", e),
                });
            }
        };

        if let Err(e) = daemon.register(service_info) {
            return Err(ServiceError::OperationFailed {
                name: "mdns".to_string(),
                message: format!("failed to register fallback mDNS service: {}", e),
            });
        }

        Ok(RegistrationBackend::Fallback(daemon))
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

        let instance_name = &self.config.mdns.instance_name;
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "localhost".to_string());

        let txt_props = self.build_txt_properties();

        // --- ADR-027: Tiered registration strategy ---
        //
        // 1. Try OS-native registration (dns-sd-native).
        // 2. If the OS daemon is not running, fall back to mdns-sd.
        // 3. If the OS daemon IS running but registration fails, report error.

        let backend =
            match self.try_os_native_registration(instance_name, &txt_props).await {
                Ok(backend) => {
                    info!(
                        instance = %instance_name,
                        service_type = SERVICE_TYPE_SHORT,
                        port = self.config.server.management_port,
                        hostname = %hostname,
                        method = "os-native",
                        txt_records = ?txt_props.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>(),
                        "mDNS service registered via OS daemon"
                    );
                    backend
                }
                Err(os_err) => {
                    // Distinguish "daemon not running" from "daemon present but
                    // registration failed".
                    //
                    // dns-sd-native surfaces DnsSdUnavailable when the OS daemon
                    // is absent (Avahi D-Bus not reachable, Windows DNS Client
                    // service disabled). Other error variants indicate the daemon
                    // IS present but something else went wrong.
                    let daemon_absent = matches!(
                        os_err,
                        ServiceRegistrationError::DnsSdUnavailable(_)
                    );

                    if daemon_absent {
                        warn!(
                            error = %os_err,
                            "No host OS mDNS daemon detected, starting stand-alone mDNS server"
                        );

                        match self.try_fallback_registration(
                            instance_name, &hostname, &txt_props,
                        ) {
                            Ok(backend) => {
                                info!(
                                    instance = %instance_name,
                                    service_type = SERVICE_TYPE,
                                    port = self.config.server.management_port,
                                    hostname = %format!("{}.local.", hostname),
                                    method = "embedded-fallback",
                                    txt_records = ?txt_props.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>(),
                                    "mDNS service registered via embedded responder (fallback)"
                                );
                                backend
                            }
                            Err(fallback_err) => {
                                self.status.store(0, Ordering::SeqCst);
                                error!(
                                    os_error = %os_err,
                                    fallback_error = %fallback_err,
                                    "Both OS-native and fallback mDNS registration failed"
                                );
                                return Err(fallback_err);
                            }
                        }
                    } else {
                        // OS daemon appears to be running but registration failed.
                        // Do NOT fall back — that would recreate the dual-responder
                        // conflict. Report the error.
                        self.status.store(0, Ordering::SeqCst);
                        error!(
                            error = %os_err,
                            "OS mDNS daemon is present but service registration failed"
                        );
                        return Err(ServiceError::OperationFailed {
                            name: "mdns".to_string(),
                            message: format!(
                                "OS mDNS registration failed (daemon present): {}",
                                os_err
                            ),
                        });
                    }
                }
            };

        // Store the backend for clean shutdown.
        *self.backend.lock().unwrap() = Some(backend);

        self.status.store(1, Ordering::SeqCst); // Running
        info!("mDNS discovery service started");

        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        self.status.store(3, Ordering::SeqCst); // Stopping

        // Drop the backend — this deregisters the service.
        // For OsNative: dropping the ServiceRegistration deregisters.
        // For Fallback: dropping the ServiceDaemon stops its background thread.
        let backend = self.backend.lock().unwrap().take();
        match &backend {
            Some(RegistrationBackend::OsNative(_)) => {
                info!("Deregistering mDNS service (OS-native)");
            }
            Some(RegistrationBackend::Fallback(_)) => {
                info!("Stopping embedded mDNS responder (fallback)");
            }
            None => {
                info!("mDNS service was not registered");
            }
        }
        drop(backend);

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

    #[test]
    fn service_type_short_is_correct() {
        assert_eq!(SERVICE_TYPE_SHORT, "_okfpga._tcp");
    }

    #[test]
    fn service_type_consistency() {
        // Verify the short form + ".local." equals the full form.
        let reconstructed = format!("{}.local.", SERVICE_TYPE_SHORT);
        assert_eq!(reconstructed, SERVICE_TYPE);
    }
}
