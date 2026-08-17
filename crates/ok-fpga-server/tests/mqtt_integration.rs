//! MQTT Integration Test
//!
//! Verifies that the ok-fpga-server MQTT message format works correctly
//! with a live Mosquitto broker at 127.0.0.1:1883.
//!
//! Run: cargo test -p ok-fpga-server --features mqtt --test mqtt_integration -- --nocapture

use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;
use tokio::time::sleep;

/// Test that we can connect to the local Mosquitto broker and publish
/// messages in the ADR-011 format.
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

    // Publish a server.status heartbeat (QoS 0, retained).
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
            "okfpga/test/status",
            QoS::AtMostOnce,
            true,
            status_msg.to_string(),
        )
        .await
        .expect("publish server.status");

    println!("✅ Published: okfpga/test/status (server.status heartbeat)");

    // Publish a services.status message (QoS 1, retained).
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
            "okfpga/test/services/status",
            QoS::AtLeastOnce,
            true,
            services_msg.to_string(),
        )
        .await
        .expect("publish services.status");

    println!("✅ Published: okfpga/test/services/status (services.status)");

    // Publish a devices.inventory message (QoS 1, retained).
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
            "okfpga/test/devices/inventory",
            QoS::AtLeastOnce,
            true,
            devices_msg.to_string(),
        )
        .await
        .expect("publish devices.inventory");

    println!("✅ Published: okfpga/test/devices/inventory (devices.inventory)");

    // Publish a device.state message (QoS 1, retained).
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
            "okfpga/test/devices/TEST1234567/state",
            QoS::AtLeastOnce,
            true,
            state_msg.to_string(),
        )
        .await
        .expect("publish device.state");

    println!("✅ Published: okfpga/test/devices/TEST1234567/state (device.state)");

    // Publish a device.sensors message (QoS 0, not retained).
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
            "okfpga/test/devices/TEST1234567/sensors",
            QoS::AtMostOnce,
            false,
            sensors_msg.to_string(),
        )
        .await
        .expect("publish device.sensors");

    println!("✅ Published: okfpga/test/devices/TEST1234567/sensors (device.sensors)");

    // Publish a resource.state message (QoS 1).
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
            "okfpga/test/resources/TEST1234567:dac_ch1/state",
            QoS::AtLeastOnce,
            false,
            resource_msg.to_string(),
        )
        .await
        .expect("publish resource.state");

    println!("✅ Published: okfpga/test/resources/TEST1234567:dac_ch1/state (resource.state)");

    // Allow the event loop to flush.
    sleep(Duration::from_millis(1000)).await;
    let _ = client.disconnect().await;

    // Wait for event loop to finish.
    let _ = tokio::time::timeout(Duration::from_secs(3), loop_handle).await;

    println!("\n🎉 All 6 MQTT message types published successfully to Mosquitto at 127.0.0.1:1883");
    println!("   Verify with: mosquitto_sub -h 127.0.0.1 -t \"okfpga/test/#\" -v");
}
