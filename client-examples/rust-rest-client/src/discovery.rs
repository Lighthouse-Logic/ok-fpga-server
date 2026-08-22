//! # mDNS Discovery Module
//!
//! Discovers ok-FPGA-Server instances on the local network using mDNS/DNS-SD.
//! Browses for `_okfpga._tcp.local.` and extracts connection details from
//! TXT records (rest port, grpc port, dashboard port, etc.).
//!
//! ## NOTE
//! This discovery module is identical in `rust-rest-client` and `rust-grpc-client`.
//! If you modify this file, apply the same changes to the other client.

use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use tracing::{debug, info, warn};

/// Information about a discovered ok-FPGA-Server instance.
#[derive(Debug, Clone)]
pub struct DiscoveredServer {
    /// The mDNS instance name (e.g., "okfpga-lab-bench-1").
    pub instance_name: String,
    /// The hostname (e.g., "lab-bench-1.local.").
    pub hostname: String,
    /// The management/dashboard port (from the SRV record).
    pub management_port: u16,
    /// REST API port (from TXT record "rest=8088"). None if REST is disabled.
    pub rest_port: Option<u16>,
    /// gRPC port (from TXT record "grpc=50051"). None if gRPC is disabled.
    pub grpc_port: Option<u16>,
    /// MQTT topic prefix (from TXT record "mqtt=okfpga/server01").
    pub mqtt_topic: Option<String>,
    /// Number of connected devices (from TXT record "devices=3").
    pub device_count: Option<u32>,
    /// API version (from TXT record "version=1.0.0").
    pub api_version: Option<String>,
    /// IP addresses resolved for this host.
    pub addresses: Vec<std::net::IpAddr>,
}

impl DiscoveredServer {
    /// Pick the best address from the resolved set.
    ///
    /// Prefers IPv4 because IPv6 link-local addresses (`fe80::…`) require
    /// a zone ID (`%iface`) that most HTTP/gRPC clients don't support.
    /// If only IPv6 is available, it is still returned — callers must
    /// bracket it in URLs per RFC 2732.
    fn preferred_addr(&self) -> Option<&std::net::IpAddr> {
        // Try IPv4 first.
        self.addresses.iter().find(|a| a.is_ipv4())
            .or_else(|| self.addresses.first())
    }

    /// Format an IP address for use in a URL.
    ///
    /// IPv4 addresses are returned as-is (e.g., `192.168.1.10`).
    /// IPv6 addresses are wrapped in brackets per RFC 2732
    /// (e.g., `[fe80::1]`) so the URL parser can distinguish
    /// the address from the port number.
    fn format_addr(addr: &std::net::IpAddr) -> String {
        match addr {
            std::net::IpAddr::V4(v4) => v4.to_string(),
            std::net::IpAddr::V6(v6) => format!("[{}]", v6),
        }
    }

    /// Get the REST API base URL (e.g., `http://192.168.1.10:8088`).
    /// Returns None if REST is not available or no addresses were resolved.
    pub fn rest_url(&self) -> Option<String> {
        let port = self.rest_port?;
        let addr = self.preferred_addr()?;
        Some(format!("http://{}:{}", Self::format_addr(addr), port))
    }

    /// Get the gRPC endpoint (e.g., `http://192.168.1.10:50051`).
    /// Returns None if gRPC is not available or no addresses were resolved.
    pub fn grpc_url(&self) -> Option<String> {
        let port = self.grpc_port?;
        let addr = self.preferred_addr()?;
        Some(format!("http://{}:{}", Self::format_addr(addr), port))
    }
}

/// Browse the local network for ok-FPGA-Server instances.
///
/// Waits up to `timeout` for mDNS responses. Returns all servers found.
///
/// # Arguments
/// * `timeout` — How long to wait for mDNS discovery responses.
pub fn discover_servers(timeout: Duration) -> Vec<DiscoveredServer> {
    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            warn!("Failed to create mDNS daemon: {}", e);
            return Vec::new();
        }
    };

    let service_type = "_okfpga._tcp.local.";
    let receiver = match mdns.browse(service_type) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to browse for {}: {}", service_type, e);
            return Vec::new();
        }
    };

    info!("Browsing for {} (timeout: {:?})...", service_type, timeout);

    let mut servers = Vec::new();
    let deadline = std::time::Instant::now() + timeout;

    // Collect mDNS responses until the timeout expires.
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                debug!("Resolved service: {:?}", info.get_fullname());

                // Extract TXT record values. TXT records are key=value pairs.
                let properties = info.get_properties();
                let rest_port = properties
                    .get_property_val_str("rest")
                    .and_then(|v| v.parse().ok());
                let grpc_port = properties
                    .get_property_val_str("grpc")
                    .and_then(|v| v.parse().ok());
                let mqtt_topic = properties
                    .get_property_val_str("mqtt")
                    .map(|s| s.to_string());
                let device_count = properties
                    .get_property_val_str("devices")
                    .and_then(|v| v.parse().ok());
                let api_version = properties
                    .get_property_val_str("version")
                    .map(|s| s.to_string());

                let server = DiscoveredServer {
                    instance_name: info.get_fullname().to_string(),
                    hostname: info.get_hostname().to_string(),
                    management_port: info.get_port(),
                    rest_port,
                    grpc_port,
                    mqtt_topic,
                    device_count,
                    api_version,
                    addresses: info.get_addresses().iter().copied().collect(),
                };

                info!(
                    "Found server: {} at {}:{}",
                    server.instance_name, server.hostname, server.management_port
                );
                servers.push(server);
            }
            Ok(_) => {
                // Other events (SearchStarted, ServiceFound, etc.) — ignore.
            }
            Err(_) => break, // Timeout or disconnected — stop browsing.
        }
    }

    // Clean up the mDNS daemon.
    let _ = mdns.shutdown();

    if servers.is_empty() {
        warn!("No ok-FPGA-Server instances found via mDNS");
    }

    servers
}
