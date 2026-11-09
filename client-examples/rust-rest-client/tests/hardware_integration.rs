// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Hardware Integration Tests — Rust REST Client
//!
//! These tests require:
//! 1. The ok-FPGA-Server running at http://localhost:8088
//! 2. An XEM7320-A75T FPGA connected via USB
//! 3. The Counters.bit bitfile at `OKFPGA_TEST_BITFILE`
//!
//! Run with: `cargo test -- --ignored --nocapture --test-threads=1`
//!
//! All tests are `#[ignore]` — they only run with `--ignored` flag.
//! Tests must run with `--test-threads=1` because they share hardware state.

use std::env;
use std::path::Path;
use std::time::Duration;

use rust_rest_client::{ClientError, OkFpgaRestClient};

/// Counters.bit endpoint map (from Counters-endpoints.md)
const WIRE_IN_EP: u8 = 0x00;    // RESET1=bit0, DISABLE1=bit1, AUTOCOUNT2=bit2
const TRIGGER_IN_EP: u8 = 0x40; // RESET2=bit0, UP2=bit1, DOWN2=bit2

/// Helper: get the server URL from env or use default.
fn server_url() -> String {
    env::var("OKFPGA_SERVER_URL").unwrap_or_else(|_| "http://localhost:8088".to_string())
}

/// Helper: get the bitfile path from env or use default.
fn bitfile_path() -> String {
    env::var("OKFPGA_TEST_BITFILE").unwrap_or_else(|_| {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        let p = Path::new(&manifest_dir)
            .parent().unwrap()  // client-examples/
            .parent().unwrap()  // project root
            .join("FrontPanel-6.1.0")
            .join("ExampleBitfiles")
            .join("20190501-FP50-XEM7320-A75")
            .join("XEM7320-A75")
            .join("Counters.bit");
        p.to_string_lossy().to_string()
    })
}

/// Helper: check if an error is a "not implemented" (501) or "conflict" (409).
fn is_expected_skip(e: &ClientError) -> bool {
    match e {
        ClientError::Server { status, .. } => *status == 501 || *status == 409,
        _ => false,
    }
}

// ============================================================================
// Phase 1: Server Connectivity & Device Discovery
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_01_server_status() {
    let client = OkFpgaRestClient::new(&server_url());
    let status = client.get_server_status().await.expect("server reachable");
    assert!(status.get("server_version").is_some());
    assert!(status.get("uptime_seconds").is_some());
    println!(
        "Server v{}, uptime={}s",
        status["server_version"], status["uptime_seconds"]
    );
}

#[tokio::test]
#[ignore]
async fn test_02_list_devices() {
    let client = OkFpgaRestClient::new(&server_url());
    let result = client.list_devices().await.expect("list devices");
    let devices = result["devices"].as_array().expect("devices array");
    assert!(!devices.is_empty(), "at least one device connected");
    println!("Found {} device(s): {}",
        devices.len(), devices[0]["serial"]);
}

#[tokio::test]
#[ignore]
async fn test_03_device_info() {
    let client = OkFpgaRestClient::new(&server_url());
    let devices = client.list_devices().await.unwrap();
    let serial = devices["devices"][0]["serial"].as_str().unwrap();
    let info = client.get_device_info(serial).await.expect("device info");
    assert_eq!(info["serial"].as_str().unwrap(), serial);
    println!("Device: {}", info.get("product_name").unwrap_or(&serde_json::Value::Null));
}

#[tokio::test]
#[ignore]
async fn test_04_device_sensors() {
    let client = OkFpgaRestClient::new(&server_url());
    let devices = client.list_devices().await.unwrap();
    let serial = devices["devices"][0]["serial"].as_str().unwrap();
    match client.get_device_sensors(serial).await {
        Ok(sensors) => {
            println!("Sensors: {}", sensors);
        }
        Err(e) if is_expected_skip(&e) => {
            println!("SKIP: sensors not supported on this device");
            return;
        }
        Err(e) => panic!("unexpected error: {}", e),
    }
}

// ============================================================================
// Phase 2: FPGA Configuration
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_05_configure_fpga() {
    let client = OkFpgaRestClient::new(&server_url());
    let path = bitfile_path();
    if !Path::new(&path).exists() {
        println!("SKIP: bitfile not found at {}", path);
        return;
    }
    let devices = client.list_devices().await.unwrap();
    let serial = devices["devices"][0]["serial"].as_str().unwrap();

    let result = client.configure_fpga(serial, &path).await;
    match result {
        Ok(r) => println!("Configure: {}", r),
        Err(e) if is_expected_skip(&e) => {
            println!("SKIP: configure not implemented");
            return;
        }
        Err(e) => panic!("configure failed: {}", e),
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let bitinfo = client.get_bitfile_info(serial).await;
    match bitinfo {
        Ok(b) => println!("Bitfile: {}", b),
        Err(e) if is_expected_skip(&e) => println!("SKIP: bitfile info stub"),
        Err(e) => panic!("bitfile info failed: {}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_06_dataport_type() {
    let client = OkFpgaRestClient::new(&server_url());
    let path = bitfile_path();
    let devices = client.list_devices().await.unwrap();
    let serial = devices["devices"][0]["serial"].as_str().unwrap();
    // Ensure programmed
    let _ = client.configure_fpga(serial, &path).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    match client.get_dataport_type(serial).await {
        Ok(result) => {
            assert!(result.get("dataport").is_some());
            println!("Dataport: {}", result["dataport"]);
        }
        Err(e) if is_expected_skip(&e) => println!("SKIP: dataport stub"),
        Err(e) => panic!("dataport failed: {}", e),
    }
}

// ============================================================================
// Phase 3: Classic Data Port — Wires
// ============================================================================

/// Helper: configure FPGA, register + acquire a covering resource for `enforce` mode,
/// and return `(client, serial)` (reusable by Classic tests).
async fn setup_programmed_device() -> (OkFpgaRestClient, String) {
    let user_id = "integration-test-user";
    let client = OkFpgaRestClient::new(&server_url()).with_user_id(user_id);
    let path = bitfile_path();
    let devices = client.list_devices().await.expect("list devices");
    let dev = &devices["devices"][0];
    let serial = dev["serial"].as_str().unwrap().to_string();
    let is_programmed = dev.get("bitfile").is_some();
    let resource_id = format!("{}:test-classic-counters", serial);

    if !is_programmed {
        let _ = client.configure_fpga(&serial, &path).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // Ensure the covering resource exists and is acquired by `integration-test-user`.
    let _ = client
        .create_resource(
            "test-classic-counters",
            &serial,
            Some("Counters.bit Classic resource"),
            "shared",
        )
        .await;
    let _ = client
        .acquire_resource(&resource_id, "shared", user_id)
        .await;

    (client, serial)
}

#[tokio::test]
#[ignore]
async fn test_07_classic_reset_counter1() {
    let (client, serial) = setup_programmed_device().await;

    // Assert RESET1 (bit 0)
    match client.set_wire_in(&serial, WIRE_IN_EP, 0x01, None).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; }
        Err(e) => panic!("{}", e),
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    let result = client.get_wire_out(&serial, 0x20).await.expect("wire out");
    println!("WireOut after reset: {}", result);
}

#[tokio::test]
#[ignore]
async fn test_08_classic_disable_counter1() {
    let (client, serial) = setup_programmed_device().await;

    // Set DISABLE1 (bit 1)
    match client.set_wire_in(&serial, WIRE_IN_EP, 0x02, None).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; }
        Err(e) => panic!("{}", e),
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    let val1 = client.get_wire_out(&serial, 0x20).await.expect("wire out 1");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let val2 = client.get_wire_out(&serial, 0x20).await.expect("wire out 2");
    println!("Disabled: val1={}, val2={}", val1, val2);
}

#[tokio::test]
#[ignore]
async fn test_09_classic_free_running_counter1() {
    let (client, serial) = setup_programmed_device().await;

    // Clear all control bits — counter should free-run
    match client.set_wire_in(&serial, WIRE_IN_EP, 0x00, None).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; }
        Err(e) => panic!("{}", e),
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let val1 = client.get_wire_out(&serial, 0x20).await.expect("wire out 1");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let val2 = client.get_wire_out(&serial, 0x20).await.expect("wire out 2");
    println!("Free-running: val1={}, val2={}", val1, val2);
}

// ============================================================================
// Phase 4: Classic Data Port — Triggers
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_10_classic_trigger_reset_counter2() {
    let (client, serial) = setup_programmed_device().await;

    // Enable autocount2 (bit 2 of WireIn) so counter2 counts
    let _ = client.set_wire_in(&serial, WIRE_IN_EP, 0x04, None).await;
    // Reset counter2 via TriggerIn bit 0
    match client.activate_trigger_in(&serial, TRIGGER_IN_EP, 0).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; }
        Err(e) => panic!("{}", e),
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("Trigger reset counter2");
}

#[tokio::test]
#[ignore]
async fn test_11_classic_trigger_count_up() {
    let (client, serial) = setup_programmed_device().await;

    // Disable autocount, reset counter2
    let _ = client.set_wire_in(&serial, WIRE_IN_EP, 0x00, None).await;
    let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 0).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Count up 5 times
    for _ in 0..5 {
        let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 1).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = client.get_wire_out(&serial, 0x21).await.expect("wire out");
    println!("After 5 count-ups: {}", result);
}

#[tokio::test]
#[ignore]
async fn test_12_classic_trigger_count_down() {
    let (client, serial) = setup_programmed_device().await;

    // Disable autocount, reset counter2
    let _ = client.set_wire_in(&serial, WIRE_IN_EP, 0x00, None).await;
    let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 0).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Count up 10, then down 3
    for _ in 0..10 {
        let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 1).await;
    }
    for _ in 0..3 {
        let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 2).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = client.get_wire_out(&serial, 0x21).await.expect("wire out");
    println!("After 10 up + 3 down: {}", result);
}

#[tokio::test]
#[ignore]
async fn test_13_classic_trigger_out() {
    let (client, serial) = setup_programmed_device().await;

    match client.get_trigger_out(&serial, 0x60).await {
        Ok(result) => println!("TriggerOut: {}", result),
        Err(e) if is_expected_skip(&e) => println!("SKIP"),
        Err(e) => panic!("{}", e),
    }
}

// ============================================================================
// Phase 5: Classic Data Port — Registers
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_14_classic_register_read() {
    let (client, serial) = setup_programmed_device().await;

    match client.read_register(&serial, 0x00).await {
        Ok(result) => println!("Register 0x00: {}", result),
        Err(e) if is_expected_skip(&e) => println!("SKIP"),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_15_classic_register_write() {
    let (client, serial) = setup_programmed_device().await;

    match client.write_register(&serial, 0x00, 0x01).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; }
        Err(e) => panic!("{}", e),
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = client.read_register(&serial, 0x00).await.expect("register read");
    println!("Register after write: {}", result);
}

// ============================================================================
// Phase 6: AXI (skipped on XEM7320 — okDATAPORT_LEGACY)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_16_axi_lite_read() {
    let (client, serial) = setup_programmed_device().await;

    match client.axi_lite_read(&serial, 0x00).await {
        Ok(r) => println!("AXI Lite read: {}", r),
        Err(e) if is_expected_skip(&e) => println!("SKIP: AXI not supported"),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_17_axi_lite_write() {
    let (client, serial) = setup_programmed_device().await;

    match client.axi_lite_write(&serial, 0x00, 0x01).await {
        Ok(r) => println!("AXI Lite write: {}", r),
        Err(e) if is_expected_skip(&e) => println!("SKIP: AXI not supported"),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_18_axi_lite_reset() {
    let (client, serial) = setup_programmed_device().await;

    match client.axi_lite_reset(&serial).await {
        Ok(r) => println!("AXI Lite reset: {}", r),
        Err(e) if is_expected_skip(&e) => println!("SKIP: AXI not supported"),
        Err(e) => panic!("{}", e),
    }
}

// ============================================================================
// Phase 7: Resources
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_19_create_resource() {
    let (client, serial) = setup_programmed_device().await;

    match client.create_resource(
        "test-classic-counters",
        &serial,
        Some("Counters.bit Classic resource"),
        "shared",
    ).await {
        Ok(r) => {
            println!("Created resource: {}", r);
            assert!(r.get("id").is_some());
        }
        Err(e) if is_expected_skip(&e) => println!("SKIP"),
        Err(e) => {
            // May fail with 409 if already exists — that's OK
            if let ClientError::Server { status: 409, .. } = &e {
                println!("Resource already exists (409) — OK");
            } else {
                panic!("{}", e);
            }
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_20_list_resources() {
    let client = OkFpgaRestClient::new(&server_url());

    let result = client.list_resources().await.expect("list resources");
    println!("Resources: {}", result);
}

#[tokio::test]
#[ignore]
async fn test_20a_lock_unlock_resource() {
    let (client, serial) = setup_programmed_device().await;

    let resource_id = format!("{}:test-classic-counters", serial);
    let user_id = "integration-test-user";

    // Acquire the resource first.
    match client.acquire_resource(&resource_id, "shared", user_id).await {
        Ok(r) => println!("Acquired: {}", r),
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; },
        Err(e) => { println!("Acquire failed (may be already held): {}", e); }
    }

    // Lock it.
    match client.lock_resource(&resource_id, user_id).await {
        Ok(r) => {
            println!("Locked: {}", r);
            assert_eq!(r["state"].as_str(), Some("in_use_locked"));
            assert_eq!(r["locked_by"].as_str(), Some(user_id));
        }
        Err(e) if is_expected_skip(&e) => { println!("SKIP"); return; },
        Err(e) => panic!("Lock failed: {}", e),
    }

    // Unlock it.
    match client.unlock_resource(&resource_id, user_id).await {
        Ok(r) => {
            println!("Unlocked: {}", r);
            assert_eq!(r["state"].as_str(), Some("in_use_shared"));
        }
        Err(e) if is_expected_skip(&e) => println!("SKIP"),
        Err(e) => panic!("Unlock failed: {}", e),
    }

    // Release it.
    let _ = client.release_resource(&resource_id, user_id).await;
}

#[tokio::test]
#[ignore]
async fn test_20b_heartbeat() {
    let client = OkFpgaRestClient::new(&server_url());

    match client.heartbeat("integration-test-user").await {
        Ok(r) => {
            println!("Heartbeat response: {}", r);
            assert!(r["server_time"].as_str().is_some());
            assert_eq!(r["user_id"].as_str(), Some("integration-test-user"));
        }
        Err(e) if is_expected_skip(&e) => println!("SKIP"),
        Err(e) => panic!("Heartbeat failed: {}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_21_delete_resource() {
    let (client, serial) = setup_programmed_device().await;

    let resource_id = format!("{}:test-classic-counters", serial);
    // Release any held lease before deleting so we don't hit 409 Resource Held.
    let _ = client
        .release_resource(&resource_id, "integration-test-user")
        .await;

    match client.delete_resource(&resource_id).await {
        Ok(_) => println!("Deleted resource: {}", resource_id),
        Err(e) if is_expected_skip(&e) => println!("SKIP"),
        Err(e) => {
            // May fail with 404 if already deleted
            if let ClientError::Server { status: 404, .. } = &e {
                println!("Resource not found (404) — OK");
            } else {
                panic!("{}", e);
            }
        }
    }
}

// ============================================================================
// Phase 8: Separable Dual-Board Isolation Test (Optional — Skips if < 2 Boards)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_22_dual_board_isolation_optional() {
    let user_a = "dual-board-user-a";
    let user_b = "dual-board-user-b";
    let client_a = OkFpgaRestClient::new(&server_url()).with_user_id(user_a);
    let client_b = OkFpgaRestClient::new(&server_url()).with_user_id(user_b);

    let devices_resp = client_a.list_devices().await.expect("list devices");
    let devices = devices_resp["devices"].as_array().expect("devices array");
    if devices.len() < 2 {
        println!(
            "SKIP: test_22_dual_board_isolation_optional requires >= 2 connected FPGAs (found {})",
            devices.len()
        );
        return;
    }

    let serial_a = devices[0]["serial"].as_str().unwrap().to_string();
    let serial_b = devices[1]["serial"].as_str().unwrap().to_string();
    println!(
        "Running dual-board REST isolation test on {} and {}",
        serial_a, serial_b
    );

    let path = bitfile_path();
    client_a
        .configure_fpga(&serial_a, &path)
        .await
        .expect("configure Board A");
    client_b
        .configure_fpga(&serial_b, &path)
        .await
        .expect("configure Board B");

    let res_a = format!("{}:test-classic-counters", serial_a);
    let res_b = format!("{}:test-classic-counters", serial_b);
    let _ = client_a
        .create_resource("test-classic-counters", &serial_a, None, "shared")
        .await;
    let _ = client_b
        .create_resource("test-classic-counters", &serial_b, None, "shared")
        .await;
    client_a
        .acquire_resource(&res_a, "shared", user_a)
        .await
        .expect("acquire Board A");
    client_b
        .acquire_resource(&res_b, "shared", user_b)
        .await
        .expect("acquire Board B");

    // Reset Counter #2 on both boards and pulse Board A 4 times, Board B 9 times.
    client_a
        .set_wire_in(&serial_a, WIRE_IN_EP, 0x00, None)
        .await
        .unwrap();
    client_b
        .set_wire_in(&serial_b, WIRE_IN_EP, 0x00, None)
        .await
        .unwrap();
    client_a
        .activate_trigger_in(&serial_a, TRIGGER_IN_EP, 0)
        .await
        .unwrap();
    client_b
        .activate_trigger_in(&serial_b, TRIGGER_IN_EP, 0)
        .await
        .unwrap();
    for _ in 0..4 {
        client_a
            .activate_trigger_in(&serial_a, TRIGGER_IN_EP, 1)
            .await
            .unwrap();
    }
    for _ in 0..9 {
        client_b
            .activate_trigger_in(&serial_b, TRIGGER_IN_EP, 1)
            .await
            .unwrap();
    }

    let out_a = client_a.get_wire_out(&serial_a, 0x21).await.unwrap();
    let out_b = client_b.get_wire_out(&serial_b, 0x21).await.unwrap();
    let c2_a = out_a["value"].as_u64().unwrap() & 0xFF;
    let c2_b = out_b["value"].as_u64().unwrap() & 0xFF;
    println!("Dual-board REST Counter #2: A={} B={}", c2_a, c2_b);
    assert_eq!(c2_a, 4);
    assert_eq!(c2_b, 9);

    let _ = client_a.release_resource(&res_a, user_a).await;
    let _ = client_b.release_resource(&res_b, user_b).await;
    let _ = client_a.delete_resource(&res_a).await;
    let _ = client_b.delete_resource(&res_b).await;
}
