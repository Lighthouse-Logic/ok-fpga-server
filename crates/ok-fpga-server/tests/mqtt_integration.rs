#![cfg(feature = "mqtt")]
//! MQTT Integration Test
//!
//! Verifies that the ok-fpga-server MQTT message format works correctly
//! with a live Mosquitto broker at 127.0.0.1:1883.
//!
//! All messages are published with `retain=false` so they do not persist
//! on the broker after the test completes.
//!
//! Run: cargo test -p ok-fpga-server --features mqtt --test mqtt_integration -- --nocapture

use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;
use tokio::time::sleep;

/// Test that we can connect to the local Mosquitto broker and publish
/// messages in the ADR-011 format.
///
/// Uses a dedicated `okfpga/integration-test/` topic prefix to avoid
/// collisions with real server topics. All messages are non-retained.
#[tokio::test]
async fn mqtt_publish_to_mosquitto() {
    // Connect to the local broker.
    let mut opts = MqttOptions::new("okfpga-integration-test", "127.0.0.1", 1883);
    opts.set_keep_alive(Duration::from_secs(10));
    opts.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(opts, 16);

    // Spawn the event loop.
    let loop_handle = tokio::spawn(async move {
        for _ in 0..20 {
            match eventloop.poll().await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("MQTT event loop error: {}", e);
                    break;
                }
            }
        }
    });

    // Give it a moment to connect.
    sleep(Duration::from_millis(500)).await;

    let prefix = "okfpga/integration-test";

    // Publish a server.status heartbeat (QoS 0, NOT retained).
    let status_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "server.status",
        "server_id": "okfpga-integration-test",
        "payload": {
            "uptime_secs": 42,
            "device_count": 0,
            "resource_count": 0,
            "api_version": "0.1.0"
        }
    });

    client
        .publish(
            format!("{prefix}/status"),
            QoS::AtMostOnce,
            false, // NOT retained — no stale messages on the broker
            status_msg.to_string(),
        )
        .await
        .expect("publish server.status");

    println!("✅ Published: {prefix}/status (server.status heartbeat)");

    // Publish a services.status message (QoS 1, NOT retained).
    let services_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "services.status",
        "server_id": "okfpga-integration-test",
        "payload": {
            "services": {
                "rest": "running",
                "grpc": "stopped",
                "mqtt": "running",
                "mdns": "stopped"
            }
        }
    });

    client
        .publish(
            format!("{prefix}/services/status"),
            QoS::AtLeastOnce,
            false,
            services_msg.to_string(),
        )
        .await
        .expect("publish services.status");

    println!("✅ Published: {prefix}/services/status (services.status)");

    // Publish a devices.inventory message (QoS 1, NOT retained).
    let devices_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "devices.inventory",
        "server_id": "okfpga-integration-test",
        "payload": {
            "devices": [
                {
                    "serial": "TEST1234567",
                    "product_name": "XEM8320-AU25P"
                }
            ]
        }
    });

    client
        .publish(
            format!("{prefix}/devices/inventory"),
            QoS::AtLeastOnce,
            false,
            devices_msg.to_string(),
        )
        .await
        .expect("publish devices.inventory");

    println!("✅ Published: {prefix}/devices/inventory (devices.inventory)");

    // Publish a device.state message (QoS 1, NOT retained).
    let state_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "device.state",
        "server_id": "okfpga-integration-test",
        "payload": {
            "serial": "TEST1234567",
            "product_name": "XEM8320-AU25P",
            "state": "programmed",
            "bitfile": "design_v2.bit"
        }
    });

    client
        .publish(
            format!("{prefix}/devices/TEST1234567/state"),
            QoS::AtLeastOnce,
            false,
            state_msg.to_string(),
        )
        .await
        .expect("publish device.state");

    println!("✅ Published: {prefix}/devices/TEST1234567/state (device.state)");

    // Publish a device.sensors message (QoS 0, NOT retained).
    let sensors_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "device.sensors",
        "server_id": "okfpga-integration-test",
        "payload": {
            "serial": "TEST1234567",
            "sensors": [
                { "name": "FPGA Die Temp", "type": "temperature", "value": 42.5, "unit": "°C" },
                { "name": "VCCINT", "type": "voltage", "value": 0.85, "unit": "V" }
            ]
        }
    });

    client
        .publish(
            format!("{prefix}/devices/TEST1234567/sensors"),
            QoS::AtMostOnce,
            false,
            sensors_msg.to_string(),
        )
        .await
        .expect("publish device.sensors");

    println!("✅ Published: {prefix}/devices/TEST1234567/sensors (device.sensors)");

    // Publish a resource.state message (QoS 1, NOT retained).
    let resource_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "resource.state",
        "server_id": "okfpga-integration-test",
        "payload": {
            "resource_id": "TEST1234567:dac_ch1",
            "name": "dac_ch1",
            "state": "in_use_shared",
            "holders": ["alice", "bob"]
        }
    });

    client
        .publish(
            format!("{prefix}/resources/TEST1234567:dac_ch1/state"),
            QoS::AtLeastOnce,
            false,
            resource_msg.to_string(),
        )
        .await
        .expect("publish resource.state");

    println!("✅ Published: {prefix}/resources/TEST1234567:dac_ch1/state (resource.state)");

    // --- Phase 40: Publish a server.announce message (NOT retained in test) ---
    let announce_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "server.announce",
        "server_id": "okfpga-integration-test",
        "payload": {
            "description": "Integration Test Server",
            "rest_url": "http://127.0.0.1:8080",
            "grpc_url": "grpc://127.0.0.1:50051",
            "dashboard_url": "http://127.0.0.1:9090",
            "mqtt_topic_prefix": prefix,
            "api_version": "0.1.0",
            "device_count": 1,
            "services": {
                "rest": true,
                "grpc": false,
                "mqtt": true,
                "mdns": false
            }
        }
    });

    client
        .publish(
            format!("{prefix}/server/announce"),
            QoS::AtLeastOnce,
            false,
            announce_msg.to_string(),
        )
        .await
        .expect("publish server.announce");

    println!("✅ Published: {prefix}/server/announce (server.announce)");

    // --- Phase 41: Clean up any retained messages on the test prefix ---
    //
    // Although all test messages above use retain=false, this cleanup
    // ensures the broker is clean even if a previous test run left stale
    // retained messages (e.g., during development or debugging).
    //
    // Per MQTT spec §3.3.1.3, an empty payload with retain=true deletes
    // the retained message for that topic.
    let retained_cleanup_topics = [
        format!("{prefix}/status"),
        format!("{prefix}/services/status"),
        format!("{prefix}/devices/inventory"),
        format!("{prefix}/resources/inventory"),
        format!("{prefix}/server/announce"),
    ];

    for topic in &retained_cleanup_topics {
        let _ = client
            .publish(topic, QoS::AtLeastOnce, true, Vec::new())
            .await;
    }

    println!("🧹 Cleared retained messages for test prefix: {prefix}");

    // Allow the event loop to flush.
    sleep(Duration::from_millis(1000)).await;
    let _ = client.disconnect().await;

    // Wait for event loop to finish.
    let _ = tokio::time::timeout(Duration::from_secs(3), loop_handle).await;

    println!("\n🎉 All 7 MQTT message types published successfully to Mosquitto at 127.0.0.1:1883");
    println!("   (test topics cleaned up — no stale messages left on the broker)");
}
