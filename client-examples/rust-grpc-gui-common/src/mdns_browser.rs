//! # mDNS Service Browser
//!
//! Discovers ok-FPGA-Server instances on the local network via mDNS/DNS-SD.
//! Browses for `_okfpga._tcp.local.` services and returns a list of discovered
//! servers with their connection details extracted from TXT records.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use rust_grpc_gui_common::mdns_browser::{MdnsBrowser, DiscoveredServer};
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut browser = MdnsBrowser::new().expect("Failed to start mDNS browser");
//!     
//!     // Browse for 3 seconds.
//!     let servers = browser.browse_for(std::time::Duration::from_secs(3)).await;
//!     for server in &servers {
//!         println!("{}: {} ({}:{})", server.instance_name, server.description,
//!                  server.host, server.rest_port.unwrap_or(0));
//!     }
//!     browser.stop();
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent};

/// The mDNS service type for ok-FPGA-Server instances.
const SERVICE_TYPE: &str = "_okfpga._tcp.local.";

/// A discovered ok-FPGA-Server instance on the local network.
#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    /// The mDNS instance name (e.g., "okfpga-lab-bench-1").
    pub instance_name: String,

    /// Human-readable description from the TXT record.
    /// Falls back to the instance name if not set.
    pub description: String,

    /// Hostname or IP address of the server.
    pub host: String,

    /// Port the mDNS service was registered on (management port).
    pub port: u16,

    /// REST API port (from TXT "rest" key), if the server has REST enabled.
    pub rest_port: Option<u16>,

    /// gRPC API port (from TXT "grpc" key), if the server has gRPC enabled.
    pub grpc_port: Option<u16>,

    /// Dashboard management port (from TXT "dashboard" key).
    pub dashboard_port: Option<u16>,

    /// API version string (from TXT "version" key).
    pub api_version: String,

    /// Number of connected devices (from TXT "devices" key).
    pub device_count: u32,
}

/// mDNS service browser for discovering ok-FPGA-Server instances.
///
/// Creates a background thread that listens for mDNS service announcements.
/// Discovered servers are accumulated in a thread-safe list and can be
/// retrieved at any time.
pub struct MdnsBrowser {
    /// The mDNS daemon (owns the background thread).
    daemon: ServiceDaemon,

    /// Thread-safe map of discovered servers, keyed by full service name.
    servers: Arc<Mutex<HashMap<String, DiscoveredServer>>>,

    /// Handle for the receiver task.
    _receiver_handle: tokio::task::JoinHandle<()>,
}

impl MdnsBrowser {
    /// Create a new mDNS browser and start listening for `_okfpga._tcp` services.
    pub fn new() -> Result<Self, String> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| format!("Failed to create mDNS daemon: {}", e))?;

        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| format!("Failed to start browsing: {}", e))?;

        let servers: Arc<Mutex<HashMap<String, DiscoveredServer>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let servers_clone = servers.clone();
        let daemon_clone = daemon.clone();

        let handle = tokio::task::spawn_blocking(move || {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let instance_name = info.get_fullname().to_string();

                        // Extract TXT record properties.
                        let props = info.get_properties();
                        let description = props
                            .get_property_val_str("description")
                            .unwrap_or("")
                            .to_string();
                        let api_version = props
                            .get_property_val_str("version")
                            .unwrap_or("unknown")
                            .to_string();
                        let rest_port = props
                            .get_property_val_str("rest")
                            .and_then(|s| s.parse().ok());
                        let grpc_port = props
                            .get_property_val_str("grpc")
                            .and_then(|s| s.parse().ok());
                        let dashboard_port = props
                            .get_property_val_str("dashboard")
                            .and_then(|s| s.parse().ok());
                        let device_count: u32 = props
                            .get_property_val_str("devices")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);

                        // Resolve the host address.
                        // Prefer IPv4 — the server binds to 0.0.0.0 (IPv4 only).
                        // The address set is a HashSet with non-deterministic order,
                        // so we explicitly look for an IPv4 address first to avoid
                        // picking a link-local IPv6 address (fe80::...) that would
                        // fail to connect (ADR-027 addendum).
                        let host = info
                            .get_addresses()
                            .iter()
                            .find(|a| a.is_ipv4())
                            .or_else(|| info.get_addresses().iter().next())
                            .map(|a| a.to_string())
                            .unwrap_or_else(|| info.get_hostname().to_string());

                        let server = DiscoveredServer {
                            instance_name: instance_name.clone(),
                            description: if description.is_empty() {
                                instance_name.clone()
                            } else {
                                description
                            },
                            host,
                            port: info.get_port(),
                            rest_port,
                            grpc_port,
                            dashboard_port,
                            api_version,
                            device_count,
                        };

                        servers_clone
                            .lock()
                            .unwrap()
                            .insert(instance_name, server);
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        servers_clone.lock().unwrap().remove(&fullname);
                    }
                    ServiceEvent::SearchStarted(_) => {}
                    ServiceEvent::SearchStopped(_) => break,
                    _ => {}
                }
            }

            // Clean up — stop browsing.
            let _ = daemon_clone.stop_browse(SERVICE_TYPE);
        });

        Ok(Self {
            daemon,
            servers,
            _receiver_handle: handle,
        })
    }

    /// Browse for the given duration, then return all discovered servers.
    ///
    /// This is a convenience method for the common "discover-then-show" pattern.
    pub async fn browse_for(&self, duration: Duration) -> Vec<DiscoveredServer> {
        tokio::time::sleep(duration).await;
        self.get_servers()
    }

    /// Get the current list of discovered servers.
    pub fn get_servers(&self) -> Vec<DiscoveredServer> {
        self.servers
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    /// Stop the mDNS browser and clean up resources.
    pub fn stop(&self) {
        let _ = self.daemon.stop_browse(SERVICE_TYPE);
        let _ = self.daemon.shutdown();
    }
}
