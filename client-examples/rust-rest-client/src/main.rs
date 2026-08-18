//! # ok-FPGA-Server REST Client — Interactive Demo
//!
//! This binary demonstrates how to:
//! 1. Discover the ok-FPGA-Server via mDNS
//! 2. Connect to the REST API
//! 3. Exercise key endpoints (server status, devices, resources)
//! 4. Publish client status to MQTT
//!
//! ## Usage
//! ```bash
//! cargo run                              # Use mDNS discovery
//! cargo run -- --url http://localhost:8088  # Direct connection
//! ```

use std::time::Duration;

use clap::Parser;
use tracing::{error, info};

use rust_rest_client::discovery;
use rust_rest_client::mqtt::{MqttConfig, MqttPublisher};
use rust_rest_client::OkFpgaRestClient;

/// ok-FPGA-Server REST client demo.
#[derive(Parser)]
#[command(name = "okfpga-rest-client", about = "REST client for ok-FPGA-Server")]
struct Args {
    /// REST API base URL (overrides mDNS discovery).
    #[arg(long, short)]
    url: Option<String>,

    /// MQTT broker host.
    #[arg(long, default_value = "127.0.0.1")]
    mqtt_host: String,

    /// MQTT broker port.
    #[arg(long, default_value = "1883")]
    mqtt_port: u16,

    /// Skip mDNS discovery and connect directly.
    #[arg(long)]
    no_mdns: bool,
}

#[tokio::main]
async fn main() {
    // Initialize structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    // ========================================================================
    // Step 1: Discover the server via mDNS or use the provided URL.
    // ========================================================================
    let base_url = if let Some(url) = args.url {
        info!("Using provided URL: {}", url);
        url
    } else if args.no_mdns {
        let default_url = "http://localhost:8088".to_string();
        info!("mDNS disabled, using default: {}", default_url);
        default_url
    } else {
        info!("Discovering ok-FPGA-Server via mDNS...");
        let servers = discovery::discover_servers(Duration::from_secs(3));
        if let Some(server) = servers.first() {
            if let Some(url) = server.rest_url() {
                info!("Discovered server: {} → {}", server.instance_name, url);
                url
            } else {
                error!("Server found but REST is not enabled");
                return;
            }
        } else {
            let fallback = "http://localhost:8088".to_string();
            info!("No servers found via mDNS, falling back to {}", fallback);
            fallback
        }
    };

    // ========================================================================
    // Step 2: Set up MQTT status publisher (optional).
    // ========================================================================
    let mqtt = MqttPublisher::try_new(MqttConfig {
        broker_host: args.mqtt_host,
        broker_port: args.mqtt_port,
        ..MqttConfig::default()
    });

    // ========================================================================
    // Step 3: Create the REST client and exercise endpoints.
    // ========================================================================
    let client = OkFpgaRestClient::new(&base_url);

    println!("\n╔══════════════════════════════════════════════════╗");
    println!("║       ok-FPGA-Server REST Client Demo           ║");
    println!("║  Connecting to: {:<33}║", &base_url);
    println!("╚══════════════════════════════════════════════════╝\n");

    // --- Server Status ---
    println!("━━━ Server Status ━━━");
    match client.get_server_status().await {
        Ok(status) => {
            println!("  API Version:  {}", status["api_version"]);
            println!("  Uptime:       {}s", status["uptime_secs"]);
            println!("  Devices:      {}", status["device_count"]);
            if let Some(services) = status["services"].as_array() {
                for svc in services {
                    println!("  Service:      {} → {}", svc["name"], svc["status"]);
                }
            }
        }
        Err(e) => error!("Failed to get server status: {}", e),
    }

    // --- List Services ---
    println!("\n━━━ Services ━━━");
    match client.list_services().await {
        Ok(resp) => {
            if let Some(services) = resp["services"].as_array() {
                for svc in services {
                    println!(
                        "  {} — status: {}, compiled: {}, enabled: {}",
                        svc["name"], svc["status"], svc["compiled"], svc["enabled"]
                    );
                }
            }
        }
        Err(e) => error!("Failed to list services: {}", e),
    }

    // --- List Devices ---
    println!("\n━━━ Devices ━━━");
    let devices = match client.list_devices().await {
        Ok(resp) => {
            if let Some(devices) = resp["devices"].as_array() {
                if devices.is_empty() {
                    println!("  No devices connected");
                }
                for dev in devices {
                    println!(
                        "  {} — {} (programmed: {})",
                        dev["serial"], dev["product_name"], dev["is_programmed"]
                    );
                }
                devices.clone()
            } else {
                println!("  No devices connected");
                Vec::new()
            }
        }
        Err(e) => {
            error!("Failed to list devices: {}", e);
            Vec::new()
        }
    };

    // --- Device Detail (if any devices found) ---
    if let Some(first_dev) = devices.first() {
        if let Some(serial) = first_dev["serial"].as_str() {
            println!("\n━━━ Device Detail: {} ━━━", serial);
            match client.get_device_info(serial).await {
                Ok(info) => println!("  Info: {}", serde_json::to_string_pretty(&info).unwrap()),
                Err(e) => println!("  Device info: {} (expected for stub endpoints)", e),
            }

            // Device settings — wired to FFI (Phase 19G, ADR-021)
            println!("\n━━━ Device Settings ━━━");
            match client.get_device_settings(serial).await {
                Ok(v) => println!("  {}", serde_json::to_string_pretty(&v).unwrap()),
                Err(e) => println!("  Settings: {} (unexpected error)", e),
            }

            // SYZYGY port discovery — reads from device settings (ADR-009)
            println!("\n━━━ SYZYGY Ports ━━━");
            match client.get_syzygy_ports(serial).await {
                Ok(v) => println!("  {}", serde_json::to_string_pretty(&v).unwrap()),
                Err(e) => println!("  SYZYGY: {} (unexpected error)", e),
            }

            // Device-level info endpoints (always available)
            println!("\n━━━ Device Endpoints ━━━");
            match client.get_dataport_type(serial).await {
                Ok(v) => println!("  Dataport: {}", v),
                Err(e) => println!("  Dataport: {} (unexpected error)", e),
            }
            match client.get_bitfile_info(serial).await {
                Ok(v) => println!("  Bitfile: {}", v),
                Err(e) => println!("  Bitfile: {} (not programmed — expected if no ConfigureFPGA was called)", e),
            }

            // Sensors: XEM7320 does not have on-board sensors (code -15 = UnsupportedFeature)
            println!("\n━━━ Sensors (expected 501 on XEM7320 — no on-board sensors) ━━━");
            match client.get_device_sensors(serial).await {
                Ok(v) => println!("  Sensors: {}", v),
                Err(e) => println!("  Sensors: {} ✓ (expected — this board has no sensors)", e),
            }
        }
    }

    // --- Resources ---
    println!("\n━━━ Resources ━━━");
    match client.list_resources().await {
        Ok(resp) => println!("  {}", resp),
        Err(e) => println!("  Resources: {} (expected for stub)", e),
    }

    // --- Users ---
    println!("\n━━━ Users ━━━");
    match client.list_users().await {
        Ok(resp) => println!("  {}", resp),
        Err(e) => println!("  Users: {} (expected for stub)", e),
    }

    // ========================================================================
    // Step 4: Publish status to MQTT.
    // ========================================================================
    if let Some(ref mqtt) = mqtt {
        mqtt.publish_heartbeat(0);
        mqtt.publish_connection_status("localhost", true, &base_url, Some(8088));
        println!("\n━━━ MQTT ━━━");
        println!("  Published heartbeat and connection status");
    } else {
        println!("\n━━━ MQTT ━━━");
        println!("  Skipped (broker unavailable)");
    }

    // Clean shutdown.
    if let Some(ref mqtt) = mqtt {
        mqtt.publish_offline();
    }

    println!("\n✅ Demo complete.");
}
