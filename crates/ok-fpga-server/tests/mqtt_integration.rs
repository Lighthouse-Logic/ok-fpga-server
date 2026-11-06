#![cfg(feature = "mqtt")]
//! MQTT Integration Test
//!
//! Verifies that the `ok-fpga-server` MQTT message format works correctly
//! with a live MQTT broker.
//!
//! ## Broker Configuration
//!
//! The target MQTT broker host and port are configurable via environment
//! variables so tests can point at the shared lab broker today and switch
//! to a dedicated development or local Mosquitto broker without code changes:
//!
//! - `OKFPGA_TEST_MQTT_HOST` — broker hostname or IP (default:
//!   `127.0.0.1`)
//! - `OKFPGA_TEST_MQTT_PORT` — broker TCP port (default: `1883`)
//! - `OKFPGA_TEST_MQTT_STRICT` — when set to `1` or `true`, unreachable
//!   brokers cause an immediate test failure instead of a graceful skip.
//!
//! ## Collision Avoidance on Shared Brokers
//!
//! Each test process generates a unique run identifier (`<pid>-<timestamp_ms>`)
//! and monotonic atomic sequence numbers so that:
//! - Topic prefixes (`okfpga/test/<run_id>/<label>/...`) never collide with
//!   live `okfpga/server-<hostname>/...` topics or concurrent test runs.
//! - MQTT client IDs (`okfpga-test-<label>-<run_id>-<seq>`) never collide
//!   across parallel test threads or separate machines (which would cause the
//!   broker to disconnect one of the clients per MQTT 3.1.1 §3.1.4).
//!
//! All messages are published with `retain=false` (or explicitly cleared in
//! the retained-cleanup test) so no stale state persists on the broker.
//!
//! Run:
//! ```bash
//! cargo test -p ok-fpga-server --features mqtt --test mqtt_integration -- --nocapture
//! ```

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::time::sleep;

/// Default MQTT broker hostname used when `OKFPGA_TEST_MQTT_HOST` is unset.
const DEFAULT_MQTT_HOST: &str = "127.0.0.1";

/// Default MQTT broker TCP port used when `OKFPGA_TEST_MQTT_PORT` is unset.
const DEFAULT_MQTT_PORT: u16 = 1883;

/// Returns the MQTT broker hostname from `OKFPGA_TEST_MQTT_HOST`, falling back
/// to [`DEFAULT_MQTT_HOST`] when unset or blank.
fn broker_host() -> String {
    match std::env::var("OKFPGA_TEST_MQTT_HOST") {
        Ok(val) if !val.trim().is_empty() => val.trim().to_string(),
        _ => DEFAULT_MQTT_HOST.to_string(),
    }
}

/// Returns the MQTT broker TCP port from `OKFPGA_TEST_MQTT_PORT`, falling back
/// to [`DEFAULT_MQTT_PORT`] when unset or unparseable.
fn broker_port() -> u16 {
    std::env::var("OKFPGA_TEST_MQTT_PORT")
        .ok()
        .and_then(|val| val.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_MQTT_PORT)
}

/// Returns `true` when `OKFPGA_TEST_MQTT_STRICT` requests hard failure on an
/// unreachable broker instead of skipping.
fn strict_broker_required() -> bool {
    matches!(
        std::env::var("OKFPGA_TEST_MQTT_STRICT")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

/// Monotonic counter used to disambiguate MQTT client IDs within the same
/// test binary process.
static CLIENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Returns a per-process unique identifier (`<pid>-<epoch_ms>`) initialized
/// once via the Lazy Singleton pattern ([`OnceLock`]).
///
/// Why `OnceLock`: `cargo test` runs all `#[tokio::test]` functions in this
/// file inside the same OS process (potentially across multiple threads).
/// `OnceLock` guarantees the run ID is computed once and shared safely across
/// all test threads without locking overhead after initialization.
fn test_run_id() -> &'static str {
    static RUN_ID: OnceLock<String> = OnceLock::new();
    RUN_ID.get_or_init(|| {
        format!(
            "{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_millis()
        )
    })
}

/// Builds a globally unique MQTT client ID for a test participant.
///
/// Combines `label`, [`test_run_id`], and an atomic sequence counter so two
/// parallel tests never share a client ID on the broker.
fn unique_client_id(label: &str) -> String {
    let seq = CLIENT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("okfpga-test-{label}-{}-{seq}", test_run_id())
}

/// Builds an isolated MQTT topic prefix (`okfpga/test/<run_id>/<label>`) for
/// a specific test function.
fn test_prefix(label: &str) -> String {
    format!("okfpga/test/{}/{label}", test_run_id())
}

/// Performs a pre-flight TCP connection check to verify the configured MQTT
/// broker is reachable within 3 seconds.
///
/// If the broker is unreachable and `OKFPGA_TEST_MQTT_STRICT` is set, this
/// function panics immediately; otherwise it prints a skip notice and returns
/// `false` so offline test runs stay green.
async fn ensure_broker_available(test_name: &str) -> bool {
    let host = broker_host();
    let port = broker_port();
    let reachable = tokio::time::timeout(
        Duration::from_secs(3),
        TcpStream::connect((host.as_str(), port)),
    )
    .await
    .map(|res| res.is_ok())
    .unwrap_or(false);

    if !reachable {
        assert!(
            !strict_broker_required(),
            "{test_name}: MQTT broker {host}:{port} is unreachable (OKFPGA_TEST_MQTT_STRICT is set)"
        );
        println!("⏭️ Skipping {test_name}: MQTT broker {host}:{port} is unreachable");
    }
    reachable
}

/// Spawns a background task that drives `rumqttc`'s [`rumqttc::EventLoop`] and
/// forwards every event over an unbounded Tokio channel, waiting for the
/// initial MQTT `CONNACK` packet before returning.
///
/// Why an event-forwarding channel: `rumqttc` only makes progress (sending
/// queued packets and reading broker acknowledgements) while `eventloop.poll()`
/// is actively awaited. By polling continuously in a spawned Tokio task and
/// forwarding events to an [`UnboundedReceiver`], tests can deterministically
/// await `CONNACK`, `SUBACK`, `PUBACK`, and incoming `PUBLISH` packets over
/// real network RTT without brittle fixed `sleep()` calls.
async fn spawn_connected(
    mut eventloop: rumqttc::EventLoop,
    timeout: Duration,
) -> Result<(tokio::task::JoinHandle<()>, UnboundedReceiver<Event>), String> {
    let (tx, mut rx) = unbounded_channel();

    let handle = tokio::spawn(async move {
        while let Ok(event) = eventloop.poll().await {
            if tx.send(event).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("timed out waiting for MQTT CONNACK".to_string());
        }
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(Event::Incoming(Packet::ConnAck(_)))) => return Ok((handle, rx)),
            Ok(Some(_)) => continue,
            Ok(None) => return Err("MQTT event loop exited before CONNACK".to_string()),
            Err(_) => return Err("timed out waiting for MQTT CONNACK".to_string()),
        }
    }
}

/// Waits for an incoming MQTT `SUBACK` packet on the forwarded event stream.
async fn wait_for_suback(
    rx: &mut UnboundedReceiver<Event>,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("timed out waiting for MQTT SUBACK".to_string());
        }
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(Event::Incoming(Packet::SubAck(_)))) => return Ok(()),
            Ok(Some(_)) => continue,
            Ok(None) => return Err("MQTT event loop exited before SUBACK".to_string()),
            Err(_) => return Err("timed out waiting for MQTT SUBACK".to_string()),
        }
    }
}

/// Waits for an incoming MQTT `PUBACK` packet (QoS 1 acknowledgement) on the
/// forwarded event stream.
async fn wait_for_puback(
    rx: &mut UnboundedReceiver<Event>,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err("timed out waiting for MQTT PUBACK".to_string());
        }
        match tokio::time::timeout(deadline - now, rx.recv()).await {
            Ok(Some(Event::Incoming(Packet::PubAck(_)))) => return Ok(()),
            Ok(Some(_)) => continue,
            Ok(None) => return Err("MQTT event loop exited before PUBACK".to_string()),
            Err(_) => return Err("timed out waiting for MQTT PUBACK".to_string()),
        }
    }
}

/// Test that we can connect to the configured MQTT broker and publish
/// messages in the ADR-011 format.
///
/// Uses a unique `okfpga/test/<run_id>/publish` topic prefix to avoid
/// collisions with live server topics and concurrent test runs.
/// All messages are non-retained.
#[tokio::test]
async fn mqtt_publish_to_mosquitto() {
    if !ensure_broker_available("mqtt_publish_to_mosquitto").await {
        return;
    }

    let host = broker_host();
    let port = broker_port();
    let prefix = test_prefix("publish");
    let server_id = unique_client_id("server-id");

    let mut opts = MqttOptions::new(unique_client_id("publish"), host.as_str(), port);
    opts.set_keep_alive(Duration::from_secs(10));
    opts.set_clean_session(true);

    let (client, eventloop) = AsyncClient::new(opts, 16);
    let (loop_handle, mut rx) = spawn_connected(eventloop, Duration::from_secs(10))
        .await
        .expect("connect to MQTT broker");

    // Publish a server.status heartbeat (QoS 0, NOT retained).
    let status_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "server.status",
        "server_id": server_id,
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
        "server_id": server_id,
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
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("services.status PUBACK");

    println!("✅ Published: {prefix}/services/status (services.status)");

    // Publish a devices.inventory message (QoS 1, NOT retained).
    let devices_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "devices.inventory",
        "server_id": server_id,
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
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("devices.inventory PUBACK");

    println!("✅ Published: {prefix}/devices/inventory (devices.inventory)");

    // Publish a device.state message (QoS 1, NOT retained).
    let state_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "device.state",
        "server_id": server_id,
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
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("device.state PUBACK");

    println!("✅ Published: {prefix}/devices/TEST1234567/state (device.state)");

    // Publish a device.sensors message (QoS 0, NOT retained).
    let sensors_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "device.sensors",
        "server_id": server_id,
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
        "server_id": server_id,
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
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("resource.state PUBACK");

    println!("✅ Published: {prefix}/resources/TEST1234567:dac_ch1/state (resource.state)");

    // --- Phase 40: Publish a server.announce message (NOT retained in test) ---
    let announce_msg = serde_json::json!({
        "v": 1,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": "server.announce",
        "server_id": server_id,
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
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("server.announce PUBACK");

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
        let _ = wait_for_puback(&mut rx, Duration::from_secs(5)).await;
    }

    println!("🧹 Cleared retained messages for test prefix: {prefix}");

    // Allow any final packets to flush before disconnecting.
    sleep(Duration::from_millis(100)).await;
    let _ = client.disconnect().await;

    // Wait for event loop to finish.
    let _ = tokio::time::timeout(Duration::from_secs(3), loop_handle).await;

    println!("\n🎉 All 7 MQTT message types published successfully to broker {host}:{port}");
    println!("   (test topics cleaned up — no stale messages left on the broker)");
}

/// Test that a message published to the broker can be received by a subscriber.
///
/// This verifies the broker is functioning correctly end-to-end: subscribe to a
/// unique test topic, await `SUBACK`, publish a message on that topic, await
/// `PUBACK`, and verify the message arrives at the subscriber with the expected
/// JSON payload.
#[tokio::test]
async fn mqtt_publish_subscribe_round_trip() {
    if !ensure_broker_available("mqtt_publish_subscribe_round_trip").await {
        return;
    }

    let host = broker_host();
    let port = broker_port();
    let prefix = test_prefix("round-trip");
    let topic = format!("{prefix}/ping");

    // --- Subscriber ---
    let mut sub_opts = MqttOptions::new(unique_client_id("subscriber"), host.as_str(), port);
    sub_opts.set_keep_alive(Duration::from_secs(10));
    sub_opts.set_clean_session(true);

    let (sub_client, sub_loop) = AsyncClient::new(sub_opts, 16);
    let (sub_handle, mut sub_rx) = spawn_connected(sub_loop, Duration::from_secs(10))
        .await
        .expect("subscriber connect");

    // Subscribe and wait for the broker's SUBACK before publishing.
    sub_client
        .subscribe(&topic, QoS::AtLeastOnce)
        .await
        .expect("subscribe");
    wait_for_suback(&mut sub_rx, Duration::from_secs(10))
        .await
        .expect("subscriber SUBACK");

    // --- Publisher ---
    let mut pub_opts = MqttOptions::new(unique_client_id("publisher"), host.as_str(), port);
    pub_opts.set_keep_alive(Duration::from_secs(10));
    pub_opts.set_clean_session(true);

    let (pub_client, pub_loop) = AsyncClient::new(pub_opts, 16);
    let (pub_handle, mut pub_rx) = spawn_connected(pub_loop, Duration::from_secs(10))
        .await
        .expect("publisher connect");

    let test_msg = serde_json::json!({
        "v": 1,
        "type": "test.ping",
        "payload": { "value": 42 }
    });

    pub_client
        .publish(&topic, QoS::AtLeastOnce, false, test_msg.to_string())
        .await
        .expect("publish ping");
    wait_for_puback(&mut pub_rx, Duration::from_secs(10))
        .await
        .expect("publisher PUBACK");

    // Wait for the subscriber to receive the published message.
    let deadline = Instant::now() + Duration::from_secs(10);
    let payload_str: String = loop {
        let now = Instant::now();
        assert!(
            now < deadline,
            "subscriber did not receive message on {topic} within 10 seconds"
        );

        match tokio::time::timeout(deadline - now, sub_rx.recv()).await {
            Ok(Some(Event::Incoming(Packet::Publish(p)))) => {
                break String::from_utf8_lossy(&p.payload).to_string();
            }
            Ok(Some(_)) => continue,
            Ok(None) => panic!("subscriber event loop ended before receiving message"),
            Err(_) => panic!("subscriber timed out waiting for message on {topic}"),
        }
    };

    let parsed: serde_json::Value = serde_json::from_str(&payload_str).expect("valid JSON");
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
/// Uses port 19999 on localhost which (by convention in tests) has no MQTT
/// broker running. Verifies that rumqttc's event loop reports an error within
/// a reasonable time and does not block indefinitely.
#[tokio::test]
async fn mqtt_unreachable_broker_does_not_hang() {
    let mut opts = MqttOptions::new(unique_client_id("unreachable"), "127.0.0.1", 19999);
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
    if !ensure_broker_available("mqtt_retained_message_cleanup").await {
        return;
    }

    let host = broker_host();
    let port = broker_port();
    let prefix = test_prefix("retain-cleanup");
    let topic = format!("{prefix}/test-retained");

    // --- Step 1: Publish a retained message ---
    let mut opts = MqttOptions::new(unique_client_id("retain-pub"), host.as_str(), port);
    opts.set_keep_alive(Duration::from_secs(10));
    opts.set_clean_session(true);

    let (client, eventloop) = AsyncClient::new(opts, 16);
    let (loop_handle, mut rx) = spawn_connected(eventloop, Duration::from_secs(10))
        .await
        .expect("retain publisher connect");

    // Publish a retained message and await PUBACK.
    let retained_msg = serde_json::json!({ "type": "test.retained", "value": "hello" });
    client
        .publish(&topic, QoS::AtLeastOnce, true, retained_msg.to_string())
        .await
        .expect("publish retained");
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("retained publish PUBACK");

    // --- Step 2: Clear it with an empty retained publish and await PUBACK ---
    client
        .publish(&topic, QoS::AtLeastOnce, true, Vec::<u8>::new())
        .await
        .expect("clear retained");
    wait_for_puback(&mut rx, Duration::from_secs(10))
        .await
        .expect("retained clear PUBACK");

    let _ = client.disconnect().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), loop_handle).await;

    // --- Step 3: Subscribe and verify no retained message arrives ---
    let mut sub_opts = MqttOptions::new(unique_client_id("retain-check"), host.as_str(), port);
    sub_opts.set_keep_alive(Duration::from_secs(10));
    sub_opts.set_clean_session(true);

    let (sub_client, sub_loop) = AsyncClient::new(sub_opts, 16);
    let (sub_handle, mut sub_rx) = spawn_connected(sub_loop, Duration::from_secs(10))
        .await
        .expect("retain checker connect");

    sub_client
        .subscribe(&topic, QoS::AtLeastOnce)
        .await
        .expect("subscribe for retain check");
    wait_for_suback(&mut sub_rx, Duration::from_secs(10))
        .await
        .expect("retain checker SUBACK");

    // Wait briefly after SUBACK for any retained message the broker might push.
    let deadline = Instant::now() + Duration::from_millis(1000);
    let mut received_retained = false;

    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, sub_rx.recv()).await {
            Ok(Some(Event::Incoming(Packet::Publish(p)))) => {
                if p.retain && !p.payload.is_empty() {
                    received_retained = true;
                    break;
                }
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }

    assert!(
        !received_retained,
        "no retained message should exist after clearing with empty payload"
    );

    println!("✅ Retained cleanup test passed: empty retain clears broker state");

    // Cleanup.
    let _ = sub_client.disconnect().await;
    let _ = tokio::time::timeout(Duration::from_secs(2), sub_handle).await;
}
