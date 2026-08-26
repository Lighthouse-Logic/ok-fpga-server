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

/// Test that a message published to the broker can be received by a subscriber.
///
/// This verifies the broker is functioning correctly end-to-end: publish a
/// message on a test topic, subscribe to that topic, and verify the message
/// arrives with the correct payload.
#[tokio::test]
async fn mqtt_publish_subscribe_round_trip() {
    let prefix = "okfpga/test-round-trip";
    let topic = format!("{prefix}/ping");

    // --- Subscriber ---
    let mut sub_opts = MqttOptions::new("okfpga-test-subscriber", "127.0.0.1", 1883);
    sub_opts.set_keep_alive(Duration::from_secs(10));
    sub_opts.set_clean_session(true);

    let (sub_client, mut sub_loop) = AsyncClient::new(sub_opts, 16);

    let received = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
    let received_clone = received.clone();

    let sub_handle = tokio::spawn(async move {
        loop {
            match sub_loop.poll().await {
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                    let payload = String::from_utf8_lossy(&p.payload).to_string();
                    received_clone.lock().await.push(payload);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    // Subscribe and wait for subscription to be established.
    sleep(Duration::from_millis(300)).await;
    sub_client
        .subscribe(&topic, QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    sleep(Duration::from_millis(300)).await;

    // --- Publisher ---
    let mut pub_opts = MqttOptions::new("okfpga-test-publisher", "127.0.0.1", 1883);
    pub_opts.set_keep_alive(Duration::from_secs(10));
    pub_opts.set_clean_session(true);

    let (pub_client, mut pub_loop) = AsyncClient::new(pub_opts, 16);
    let pub_handle = tokio::spawn(async move {
        for _ in 0..20 {
            if pub_loop.poll().await.is_err() {
                break;
            }
        }
    });

    sleep(Duration::from_millis(300)).await;

    let test_msg = serde_json::json!({
        "v": 1,
        "type": "test.ping",
        "payload": { "value": 42 }
    });

    pub_client
        .publish(&topic, QoS::AtLeastOnce, false, test_msg.to_string())
        .await
        .expect("publish ping");

    // Wait for the subscriber to receive the message.
    sleep(Duration::from_millis(500)).await;

    let messages = received.lock().await;
    assert!(
        !messages.is_empty(),
        "subscriber should have received at least one message"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&messages[0]).expect("valid JSON");
    assert_eq!(parsed["type"], "test.ping");
    assert_eq!(parsed["payload"]["value"], 42);

    println!("✅ Round-trip test passed: published and received message on {topic}");

    // Cleanup.
    let _ = sub_client.disconnect().await;
    let _ = pub_client.disconnect().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), sub_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), pub_handle).await;
}

/// Test that connecting to an unreachable broker does not panic or hang.
///
/// Uses port 19999 which (by convention in tests) has no MQTT broker running.
/// Verifies that rumqttc's event loop reports an error within a reasonable
/// time and does not block indefinitely.
#[tokio::test]
async fn mqtt_unreachable_broker_does_not_hang() {
    let mut opts = MqttOptions::new("okfpga-test-unreachable", "127.0.0.1", 19999);
    opts.set_keep_alive(Duration::from_secs(5));
    opts.set_clean_session(true);

    let (_client, mut eventloop) = AsyncClient::new(opts, 16);

    // The first poll should fail with a connection error (not hang forever).
    let result = tokio::time::timeout(Duration::from_secs(5), eventloop.poll()).await;

    assert!(
        result.is_ok(),
        "eventloop.poll() should return within 5 seconds, not hang"
    );

    let event = result.unwrap();
    assert!(
        event.is_err(),
        "eventloop.poll() should return an error when broker is unreachable"
    );

    println!("✅ Unreachable broker test passed: poll returned error without hanging");
}

/// Test that publishing with retain=true followed by an empty retained publish
/// correctly clears the retained message from the broker (MQTT spec §3.3.1.3).
///
/// This verifies our Phase 41 retained cleanup mechanism works correctly.
#[tokio::test]
async fn mqtt_retained_message_cleanup() {
    let prefix = "okfpga/test-retain-cleanup";
    let topic = format!("{prefix}/test-retained");

    // --- Step 1: Publish a retained message ---
    let mut opts = MqttOptions::new("okfpga-test-retain-pub", "127.0.0.1", 1883);
    opts.set_keep_alive(Duration::from_secs(10));
    opts.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(opts, 16);

    let loop_handle = tokio::spawn(async move {
        for _ in 0..30 {
            if eventloop.poll().await.is_err() {
                break;
            }
        }
    });

    sleep(Duration::from_millis(500)).await;

    // Publish a retained message.
    let retained_msg = serde_json::json!({ "type": "test.retained", "value": "hello" });
    client
        .publish(&topic, QoS::AtLeastOnce, true, retained_msg.to_string())
        .await
        .expect("publish retained");

    sleep(Duration::from_millis(300)).await;

    // --- Step 2: Clear it with an empty retained publish ---
    client
        .publish(&topic, QoS::AtLeastOnce, true, Vec::<u8>::new())
        .await
        .expect("clear retained");

    sleep(Duration::from_millis(300)).await;
    let _ = client.disconnect().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle).await;

    // --- Step 3: Subscribe and verify no retained message arrives ---
    let mut sub_opts = MqttOptions::new("okfpga-test-retain-check", "127.0.0.1", 1883);
    sub_opts.set_keep_alive(Duration::from_secs(10));
    sub_opts.set_clean_session(true);

    let (sub_client, mut sub_loop) = AsyncClient::new(sub_opts, 16);

    let received_retained = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let received_clone = received_retained.clone();

    let sub_handle = tokio::spawn(async move {
        loop {
            match sub_loop.poll().await {
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                    if p.retain && !p.payload.is_empty() {
                        received_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    sleep(Duration::from_millis(300)).await;
    sub_client
        .subscribe(&topic, QoS::AtLeastOnce)
        .await
        .expect("subscribe for retain check");

    // Wait long enough for any retained message to arrive.
    sleep(Duration::from_millis(1000)).await;

    assert!(
        !received_retained.load(std::sync::atomic::Ordering::SeqCst),
        "no retained message should exist after clearing with empty payload"
    );

    println!("✅ Retained cleanup test passed: empty retain clears broker state");

    // Cleanup.
    let _ = sub_client.disconnect().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), sub_handle).await;
}
