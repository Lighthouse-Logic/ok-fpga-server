// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # ok-FPGA-Server gRPC Client — Interactive Demo
//!
//! Demonstrates connecting to the ok-FPGA-Server via gRPC, exercising
//! key RPCs, and publishing status to MQTT.
//!
//! ## Usage
//! ```bash
//! cargo run                                      # Use mDNS discovery
//! cargo run -- --url http://localhost:50051         # Direct connection
//! ```

use std::time::Duration;

use clap::Parser;
use tracing::{error, info};

use rust_grpc_client::discovery;
use rust_grpc_client::mqtt::{MqttConfig, MqttPublisher};
use rust_grpc_client::OkFpgaGrpcClient;

/// ok-FPGA-Server gRPC client demo.
#[derive(Parser)]
#[command(name = "okfpga-grpc-client", about = "gRPC client for ok-FPGA-Server")]
struct Args {
    /// gRPC server URL (overrides mDNS discovery).
    #[arg(long, short)]
    url: Option<String>,

    /// MQTT broker host.
    #[arg(long, default_value = "127.0.0.1")]
    mqtt_host: String,

    /// MQTT broker port.
    #[arg(long, default_value = "1883")]
    mqtt_port: u16,

    /// Skip mDNS discovery.
    #[arg(long)]
    no_mdns: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    // ========================================================================
    // Step 1: Discover the server or use provided URL.
    // ========================================================================
    let grpc_url = if let Some(url) = args.url {
        info!("Using provided URL: {}", url);
        url
    } else if args.no_mdns {
        let default_url = "http://localhost:50051".to_string();
        info!("mDNS disabled, using default: {}", default_url);
        default_url
    } else {
        info!("Discovering ok-FPGA-Server via mDNS...");
        let servers = discovery::discover_servers(Duration::from_secs(3));
        if let Some(server) = servers.first() {
            if let Some(url) = server.grpc_url() {
                info!("Discovered server: {} → {}", server.instance_name, url);
                url
            } else {
                error!("Server found but gRPC is not enabled");
                return;
            }
        } else {
            let fallback = "http://localhost:50051".to_string();
            info!("No servers found via mDNS, falling back to {}", fallback);
            fallback
        }
    };

    // ========================================================================
    // Step 2: Set up MQTT (optional).
    // ========================================================================
    let mqtt = MqttPublisher::try_new(MqttConfig {
        broker_host: args.mqtt_host,
        broker_port: args.mqtt_port,
        client_id: "okfpga-rust-grpc-client".to_string(),
        topic_prefix: "okfpga-client/rust-grpc-01".to_string(),
        client_type: "rust-grpc".to_string(),
    });

    // ========================================================================
    // Step 3: Connect and exercise gRPC RPCs.
    // ========================================================================
    println!("\n╔══════════════════════════════════════════════════╗");
    println!("║       ok-FPGA-Server gRPC Client Demo           ║");
    println!("║  Connecting to: {:<33}║", &grpc_url);
    println!("╚══════════════════════════════════════════════════╝\n");

    let mut client = match OkFpgaGrpcClient::connect(&grpc_url).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to connect to gRPC server: {}", e);
            return;
        }
    };

    // --- Server Status ---
    println!("━━━ Server Status (ServerService) ━━━");
    match client.get_server_status().await {
        Ok(resp) => {
            let status = resp.into_inner();
            println!("  API Version:  {}", status.api_version);
            println!("  Uptime:       {}s", status.uptime_secs);
            println!("  Devices:      {}", status.device_count);
        }
        Err(e) => println!("  Error: {} (may not be implemented yet)", e.message()),
    }

    // --- List Services ---
    println!("\n━━━ Services ━━━");
    match client.list_services().await {
        Ok(resp) => {
            for svc in resp.into_inner().services {
                println!(
                    "  {} — status: {}, compiled: {}, enabled: {}",
                    svc.name, svc.status, svc.compiled, svc.enabled
                );
            }
        }
        Err(e) => println!("  Error: {}", e.message()),
    }

    // --- List Devices ---
    println!("\n━━━ Devices ━━━");
    let first_serial = match client.list_devices().await {
        Ok(resp) => {
            let inner = resp.into_inner();
            if inner.devices.is_empty() {
                println!("  No devices connected");
                None
            } else {
                for dev in &inner.devices {
                    println!("  {} — {} ({})", dev.serial, dev.product_name, dev.product_id);
                }
                Some(inner.devices[0].serial.clone())
            }
        }
        Err(e) => {
            println!("  Error: {}", e.message());
            None
        }
    };

    // --- Device Detail (if any device found) ---
    if let Some(serial) = &first_serial {
        // Device Info
        println!("\n━━━ Device Detail: {} ━━━", serial);
        match client.get_device_info(serial).await {
            Ok(resp) => {
                let info = resp.into_inner();
                if let Some(dev) = info.device {
                    println!("  Product:    {} (ID {})", dev.product_name, dev.product_id);
                    println!("  USB Speed:  {:?}", dev.usb_speed);
                    println!("  Firmware:   {}.{}", dev.device_major_version, dev.device_minor_version);
                    println!("  Dataport:   {:?}", dev.dataport);
                    if let Some(bf) = &dev.bitfile {
                        println!("  Bitfile:    {}", bf.filename);
                        if !bf.programmed_at.is_empty() {
                            println!("  Programmed: {} by {}", bf.programmed_at, bf.programmed_by);
                        }
                    } else {
                        println!("  Bitfile:    (not programmed)");
                    }
                }
            }
            Err(e) => println!("  Error: {}", e.message()),
        }

        // Device Settings
        println!("\n━━━ Device Settings ━━━");
        match client.get_device_settings(serial).await {
            Ok(resp) => {
                let settings = resp.into_inner();
                println!("  {} settings found:", settings.settings.len());
                for s in &settings.settings {
                    match &s.value {
                        Some(rust_grpc_client::okfpga::v1::device_setting::Value::IntValue(v)) => {
                            println!("    {} = {} (integer)", s.key, v);
                        }
                        Some(rust_grpc_client::okfpga::v1::device_setting::Value::StringValue(v)) => {
                            println!("    {} = \"{}\"", s.key, v);
                        }
                        None => {
                            println!("    {} = <unknown type>", s.key);
                        }
                    }
                }
            }
            Err(e) => println!("  Error: {}", e.message()),
        }

        // SYZYGY Ports
        println!("\n━━━ SYZYGY Ports ━━━");
        match client.get_syzygy_ports(serial).await {
            Ok(resp) => {
                let syzygy = resp.into_inner();
                if syzygy.ports.is_empty() {
                    println!("  No SYZYGY ports found");
                } else {
                    println!("  {} ports discovered:", syzygy.ports.len());
                    for port in &syzygy.ports {
                        if port.is_populated {
                            println!(
                                "    Port {}: {} {} ({}mV) — SN: {}",
                                port.index,
                                port.manufacturer_name,
                                port.product_name,
                                port.port_status_mv,
                                port.serial_number,
                            );
                        } else {
                            println!("    Port {}: empty (unpowered)", port.index);
                        }
                    }
                }
            }
            Err(e) => println!("  Error: {}", e.message()),
        }

        // Sensors
        println!("\n━━━ Sensors ━━━");
        match client.get_device_sensors(serial).await {
            Ok(resp) => {
                let sensors = resp.into_inner();
                if sensors.sensors.is_empty() {
                    println!("  No sensors found");
                } else {
                    for s in &sensors.sensors {
                        println!("  {} = {} ({})", s.name, s.value, s.description);
                    }
                }
            }
            Err(e) => println!(
                "  {} (expected on boards without on-board sensors)",
                e.message()
            ),
        }
    }

    // --- Resources ---
    println!("\n━━━ Resources ━━━");
    match client.list_resources().await {
        Ok(resp) => {
            let inner = resp.into_inner();
            if inner.resources.is_empty() {
                println!("  No resources registered");
            } else {
                for r in &inner.resources {
                    println!(
                        "  {} — {} [{}] (device: {})",
                        r.id, r.name, r.state, r.device_serial
                    );
                }
            }
        }
        Err(e) => println!("  Error: {}", e.message()),
    }

    // --- Users ---
    println!("\n━━━ Users ━━━");
    match client.list_users().await {
        Ok(resp) => {
            let inner = resp.into_inner();
            if inner.users.is_empty() {
                println!("  No active user sessions");
            } else {
                for u in &inner.users {
                    println!("  {} — connected: {} (resources: {:?})", u.user_id, u.connected_at, u.held_resource_ids);
                }
            }
        }
        Err(e) => println!("  Error: {}", e.message()),
    }

    // --- Heartbeat ---
    println!("\n━━━ Heartbeat ━━━");
    match client.ping("rust-grpc-demo").await {
        Ok(resp) => {
            let pong = resp.into_inner();
            println!("  Pong — server time: {}", pong.server_time);
        }
        Err(e) => println!("  Error: {}", e.message()),
    }

    // ========================================================================
    // Step 4: Publish MQTT status.
    // ========================================================================
    if let Some(ref mqtt) = mqtt {
        mqtt.publish_heartbeat(0);
        mqtt.publish_connection_status("localhost", true, &grpc_url, None);
        println!("\n━━━ MQTT ━━━");
        println!("  Published heartbeat and connection status");
    } else {
        println!("\n━━━ MQTT ━━━");
        println!("  Skipped (broker unavailable)");
    }

    if let Some(ref mqtt) = mqtt {
        mqtt.publish_offline();
    }

    println!("\n✅ Demo complete.");
}
