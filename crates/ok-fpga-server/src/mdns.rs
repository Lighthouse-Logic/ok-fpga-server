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
//! ## DNS-SD Record Chain (RFC 6763 §12)
//!
//! A compliant DNS-SD response must include:
//!
//! ```text
//! PTR   _okfpga._tcp.local.  →  instance._okfpga._tcp.local.
//! SRV   instance._okfpga._tcp.local.  →  hostname.local. port 9090
//! TXT   instance._okfpga._tcp.local.  →  version=0.1.0, ...
//! A     hostname.local.  →  192.168.x.x
//! ```
//!
//! The A record (IPv4 address) must be explicitly provided:
//! - **Windows**: `dns-sd-native` omits `pIp4` when calling
//!   `DnsServiceConstructInstance`, so we bypass the builder and call
//!   the Win32 API directly with the resolved IPv4 address.
//! - **Linux**: Avahi handles A/AAAA records automatically via D-Bus,
//!   so `dns-sd-native`'s builder works correctly as-is.
//! - **Fallback** (`mdns-sd`): requires `.enable_addr_auto()` to
//!   auto-detect and include A/AAAA records.
//!
//! ## TXT Record Conventions (ADR-012)
//!
//! - **Absent keys mean disabled**: if a service is not compiled/enabled,
//!   its key is omitted.
//! - **Dynamic updates**: the `devices` count updates when devices
//!   connect/disconnect.
//! - **All values are strings**.

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use tracing::{info, warn, error};

use crate::config::ServerConfig;
use crate::state::AppState;

/// The DNS-SD service type for ok-FPGA-Server instances.
/// Used by the fallback `mdns-sd` responder which expects the `.local.` suffix.
const SERVICE_TYPE: &str = "_okfpga._tcp.local.";

/// The DNS-SD service type without the `.local.` suffix, as required by
/// `dns_sd_native::ServiceRegistrationBuilder` and Win32
/// `DnsServiceConstructInstance`.
const SERVICE_TYPE_SHORT: &str = "_okfpga._tcp";

// ============================================================================
// Windows-specific Win32 registration (bypasses dns-sd-native builder)
// ============================================================================
//
// On Windows, we call `DnsServiceConstructInstance` directly to include the
// machine's IPv4 address as `pIp4`. Without this, the DNS_SERVICE_INSTANCE
// has no A record and compliant browsers like avahi-browse cannot resolve
// the hostname. See ADR-027 addendum.

#[cfg(target_os = "windows")]
#[path = "win32_mdns.rs"]
mod win32_mdns;

// ============================================================================
// Registration Backend — tracks which registration path is active
// ============================================================================

/// Tracks the active registration backend for clean shutdown.
///
/// The inner values are not read directly — they are held so their `Drop`
/// implementations deregister the service when the backend is dropped.
#[allow(dead_code)]
enum RegistrationBackend {
    /// Registered via the OS's native mDNS daemon.
    /// On Windows: our direct Win32 wrapper (includes A record).
    /// On Linux: dns-sd-native (Avahi handles A records automatically).
    /// The handle deregisters automatically when dropped.
    #[cfg(target_os = "windows")]
    OsNative(win32_mdns::Win32ServiceRegistration),

    /// Registered via the OS's native mDNS daemon (Linux: dns-sd-native
    /// uses Avahi D-Bus, which handles A/AAAA records automatically).
    #[cfg(not(target_os = "windows"))]
    OsNative(dns_sd_native::ServiceRegistration),

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

    /// Resolve the machine's primary LAN IPv4 address.
    ///
    /// Returns the address used for A records in DNS-SD responses.
    /// Falls back to `127.0.0.1` if no LAN interface is found.
    fn resolve_local_ipv4() -> Ipv4Addr {
        match local_ip_address::local_ip() {
            Ok(std::net::IpAddr::V4(addr)) => addr,
            Ok(std::net::IpAddr::V6(_)) => {
                // local_ip() returned IPv6 — try to find an IPv4 address
                // from the full list of interfaces.
                match local_ip_address::list_afinet_netifas() {
                    Ok(interfaces) => interfaces
                        .into_iter()
                        .find_map(|(_name, addr)| match addr {
                            std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
                            _ => None,
                        })
                        .unwrap_or(Ipv4Addr::LOCALHOST),
                    Err(_) => Ipv4Addr::LOCALHOST,
                }
            }
            Err(e) => {
                warn!(error = %e, "Could not resolve local IPv4 address, using localhost");
                Ipv4Addr::LOCALHOST
            }
        }
    }

    /// Attempt registration via the OS's native mDNS daemon.
    ///
    /// On Windows: calls Win32 `DnsServiceConstructInstance` directly with
    /// the machine's IPv4 address as `pIp4` (ADR-027 addendum).
    ///
    /// On Linux: uses `dns-sd-native`'s builder (Avahi handles A/AAAA records
    /// automatically via its D-Bus API).
    ///
    /// Returns `Ok(RegistrationBackend::OsNative)` on success, or an error
    /// string describing why it failed.
    async fn try_os_native_registration(
        &self,
        instance_name: &str,
        hostname: &str,
        ipv4_addr: Ipv4Addr,
        txt_props: &[(String, String)],
    ) -> Result<RegistrationBackend, (String, bool)> {
        // The return type is (error_message, daemon_absent).
        // daemon_absent = true means the OS daemon is not running (safe to fall back).
        // daemon_absent = false means the daemon IS running but registration failed.

        #[cfg(target_os = "windows")]
        {
            match win32_mdns::register_service(
                instance_name,
                SERVICE_TYPE_SHORT,
                hostname,
                self.config.server.management_port,
                ipv4_addr,
                txt_props,
            )
            .await
            {
                Ok(registration) => Ok(RegistrationBackend::OsNative(registration)),
                Err(e) => {
                    let daemon_absent = e.is_daemon_absent();
                    Err((format!("{}", e), daemon_absent))
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (hostname, ipv4_addr); // unused on Linux — Avahi handles addressing

            use dns_sd_native::{ServiceRegistrationBuilder, ServiceRegistrationError};

            let mut builder = ServiceRegistrationBuilder::new(
                SERVICE_TYPE_SHORT,
                self.config.server.management_port,
            );
            builder.name(instance_name);

            for (key, value) in txt_props {
                builder.add_txt_record_key_string(key, value);
            }

            match builder.register().await {
                Ok(registration) => Ok(RegistrationBackend::OsNative(registration)),
                Err(e) => {
                    let daemon_absent = matches!(
                        e,
                        ServiceRegistrationError::DnsSdUnavailable(_)
                    );
                    Err((format!("{}", e), daemon_absent))
                }
            }
        }
    }

    /// Fall back to the embedded mdns-sd responder.
    ///
    /// This should only be called when the OS daemon is NOT running —
    /// otherwise we'd recreate the dual-responder conflict.
    ///
    /// Uses `.enable_addr_auto()` so `mdns-sd` auto-detects local addresses
    /// and includes A/AAAA records in responses (ADR-027 addendum).
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

        // ADR-027 addendum: `.enable_addr_auto()` is required so that mdns-sd
        // auto-detects the machine's IP addresses and includes A/AAAA records
        // in DNS-SD responses. Without this, avahi-browse and other compliant
        // browsers cannot resolve the hostname.
        let service_info = match mdns_sd::ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &host_with_local,
            "", // Address determined by enable_addr_auto() below
            self.config.server.management_port,
            txt_refs.as_slice(),
        ) {
            Ok(info) => info.enable_addr_auto(),
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

        let ipv4_addr = Self::resolve_local_ipv4();
        let txt_props = self.build_txt_properties();

        // --- Phase 43: Clean up stale registrations before registering ---
        //
        // On Windows, the DNS Client service (dnscache) can retain stale
        // DNS-SD registrations from previous server runs that exited
        // ungracefully.  We attempt to deregister:
        //   1. Our own name (crash recovery from a previous run).
        //   2. The bare "okfpga-server" name (migration from pre-Phase 42).
        // This is best-effort; failures are logged at TRACE and ignored.
        #[cfg(target_os = "windows")]
        {
            // win32_mdns is a submodule of this (mdns) module.

            // 1. Crash recovery — deregister our own name from a previous crash.
            win32_mdns::try_deregister_stale(instance_name, SERVICE_TYPE_SHORT);

            // 2. Migration — deregister bare "okfpga-server" from pre-Phase 42.
            if instance_name != "okfpga-server" {
                win32_mdns::try_deregister_stale("okfpga-server", SERVICE_TYPE_SHORT);
            }

            // Give dnscache a moment to process the deregistrations.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // --- ADR-027: Tiered registration strategy ---
        //
        // 1. Try OS-native registration (dns-sd-native / Win32 direct).
        // 2. If the OS daemon is not running, fall back to mdns-sd.
        // 3. If the OS daemon IS running but registration fails, report error.

        let backend = match self
            .try_os_native_registration(instance_name, &hostname, ipv4_addr, &txt_props)
            .await
        {
            Ok(backend) => {
                info!(
                    instance = %instance_name,
                    service_type = SERVICE_TYPE_SHORT,
                    port = self.config.server.management_port,
                    hostname = %hostname,
                    ipv4 = %ipv4_addr,
                    method = "os-native",
                    txt_records = ?txt_props.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>(),
                    "mDNS service registered via OS daemon (PTR+SRV+TXT+A)"
                );
                backend
            }
            Err((os_err, daemon_absent)) => {
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
                                "mDNS service registered via embedded responder with addr_auto (fallback)"
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
        // For OsNative: dropping the registration handle deregisters.
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

// ============================================================================
// Tests
// ============================================================================

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

    #[test]
    fn resolve_local_ipv4_returns_non_zero() {
        let addr = MdnsDiscoveryService::resolve_local_ipv4();
        // Should return a valid address (either a real LAN address or localhost).
        assert!(!addr.is_unspecified(), "IPv4 address should not be 0.0.0.0");
    }

    #[test]
    fn resolve_local_ipv4_returns_non_broadcast() {
        let addr = MdnsDiscoveryService::resolve_local_ipv4();
        assert!(!addr.is_broadcast(), "IPv4 address should not be broadcast");
    }

    #[test]
    fn resolve_local_ipv4_returns_non_multicast() {
        let addr = MdnsDiscoveryService::resolve_local_ipv4();
        assert!(
            !addr.is_multicast(),
            "IPv4 address should not be multicast"
        );
    }
}
