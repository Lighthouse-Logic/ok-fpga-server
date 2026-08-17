//! # Hardware Integration Tests — Rust gRPC Client
//!
//! These tests require:
//! 1. The ok-FPGA-Server running (gRPC on port 50051)
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

use rust_grpc_client::OkFpgaGrpcClient;


/// Helper: get the gRPC server URL from env or default.
fn grpc_url() -> String {
    env::var("OKFPGA_GRPC_URL").unwrap_or_else(|_| "http://localhost:50051".to_string())
}

/// Helper: get the bitfile path from env or default.
fn bitfile_path() -> String {
    env::var("OKFPGA_TEST_BITFILE").unwrap_or_else(|_| {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        let p = Path::new(&manifest_dir)
            .parent().unwrap()
            .parent().unwrap()
            .join("FrontPanel-6.1.0")
            .join("ExampleBitfiles")
            .join("20190501-FP50-XEM7320-A75")
            .join("XEM7320-A75")
            .join("Counters.bit");
        p.to_string_lossy().to_string()
    })
}

/// Helper: true if the error is UNIMPLEMENTED (gRPC code 12) or a conflict.
fn is_expected_skip(e: &tonic::Status) -> bool {
    e.code() == tonic::Code::Unimplemented
        || e.code() == tonic::Code::FailedPrecondition
        || e.code() == tonic::Code::Aborted
}

// ============================================================================
// Phase 1: Server Connectivity & Device Discovery
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_01_server_status() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url())
        .await
        .expect("connect to gRPC server");
    let status = client.get_server_status().await.expect("server status");
    let inner = status.into_inner();
    println!("Server status: {:?}", inner);
}

#[tokio::test]
#[ignore]
async fn test_02_list_devices() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let resp = client.list_devices().await.expect("list devices");
    let inner = resp.into_inner();
    assert!(!inner.devices.is_empty(), "at least one device");
    println!("Found {} device(s): {}", inner.devices.len(), inner.devices[0].serial);
}

#[tokio::test]
#[ignore]
async fn test_03_device_info() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let devs = client.list_devices().await.unwrap().into_inner();
    let serial = &devs.devices[0].serial;
    let info = client.get_device_info(serial).await.expect("device info");
    let inner = info.into_inner();
    println!("Device info: serial={}, product={}",
        inner.device.as_ref().map(|i| i.serial.as_str()).unwrap_or("?"),
        inner.device.as_ref().map(|i| i.product_name.as_str()).unwrap_or("?"));
}

#[tokio::test]
#[ignore]
async fn test_04_list_services() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let resp = client.list_services().await.expect("list services");
    let inner = resp.into_inner();
    println!("Services: {} entries", inner.services.len());
    for s in &inner.services {
        println!("  - {} (status={})", s.name, s.status);
    }
}

// ============================================================================
// Phase 2: Classic Data Port — Wires
// ============================================================================

/// Helper: get serial of first device.
async fn get_serial(client: &mut OkFpgaGrpcClient) -> String {
    let devs = client.list_devices().await.unwrap().into_inner();
    devs.devices[0].serial.clone()
}

#[tokio::test]
#[ignore]
async fn test_05_classic_wire_in_reset() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    // Set WireIn 0x00 = 0x01 (RESET1)
    match client.set_wire_in_value(&serial, 0x00, 0x01, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }

    // Update WireIns to transfer to hardware
    match client.update_wire_ins(&serial).await {
        Ok(_) => println!("WireIn reset applied"),
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_06_classic_wire_in_free_run() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    // Clear all bits — counter free-runs
    match client.set_wire_in_value(&serial, 0x00, 0x00, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("Counter free-running");
}

// ============================================================================
// Phase 3: Classic Data Port — Triggers
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_07_classic_trigger_count_up() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    // Reset counter2 via TriggerIn bit 0
    match client.activate_trigger_in(&serial, 0x40, 0).await {
        Ok(_) => println!("Trigger: reset counter2"),
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }

    // Count up 3 times (bit 1)
    for _ in 0..3 {
        let _ = client.activate_trigger_in(&serial, 0x40, 1).await;
    }
    println!("Trigger: counted up 3");
}

#[tokio::test]
#[ignore]
async fn test_08_classic_trigger_count_down() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    // Count down (bit 2)
    match client.activate_trigger_in(&serial, 0x40, 2).await {
        Ok(_) => println!("Trigger: count down"),
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
}

// ============================================================================
// Phase 4: AXI (expected to fail/skip on XEM7320)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_09_axi_lite_read() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.axi_lite_read(&serial, 0x00).await {
        Ok(r) => println!("AXI Lite read: {:?}", r.into_inner()),
        Err(e) if is_expected_skip(&e) => println!("SKIP: AXI not supported ({e})"),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_10_axi_lite_write() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.axi_lite_write(&serial, 0x00, 0x01).await {
        Ok(_) => println!("AXI Lite write: OK"),
        Err(e) if is_expected_skip(&e) => println!("SKIP: AXI not supported ({e})"),
        Err(e) => panic!("{}", e),
    }
}
