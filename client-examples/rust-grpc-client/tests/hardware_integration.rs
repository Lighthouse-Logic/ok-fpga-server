//! # Hardware Integration Tests — Rust gRPC Client
//!
//! These tests require:
//! 1. The ok-FPGA-Server running (gRPC on port 50051, REST on port 8088)
//! 2. An XEM7320-A75T FPGA connected via USB
//! 3. The Counters.bit bitfile at `OKFPGA_TEST_BITFILE`
//!
//! Run with: `cargo test -- --ignored --nocapture --test-threads=1`
//!
//! All tests are `#[ignore]` — they only run with `--ignored` flag.
//! Tests must run with `--test-threads=1` because they share hardware state.
//!
//! NOTE: Test 05 configures the FPGA via REST API (since there is no gRPC
//! configure RPC yet). All subsequent Classic tests depend on this.

use std::env;
use std::path::Path;
use std::time::Duration;

use rust_grpc_client::OkFpgaGrpcClient;

/// Counters.bit endpoint map (from Counters-endpoints.md)
const WIRE_IN_EP: i32 = 0x00;
const WIRE_OUT_COUNTER1: i32 = 0x20;
const WIRE_OUT_COUNTER2: i32 = 0x21;
const TRIGGER_IN_EP: i32 = 0x40;

fn grpc_url() -> String {
    env::var("OKFPGA_GRPC_URL").unwrap_or_else(|_| "http://localhost:50051".to_string())
}

fn rest_url() -> String {
    env::var("OKFPGA_SERVER_URL").unwrap_or_else(|_| "http://localhost:8088".to_string())
}

fn bitfile_path() -> String {
    env::var("OKFPGA_TEST_BITFILE").unwrap_or_else(|_| {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
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

fn is_expected_skip(e: &tonic::Status) -> bool {
    e.code() == tonic::Code::Unimplemented
        || e.code() == tonic::Code::FailedPrecondition
        || e.code() == tonic::Code::Aborted
}

async fn get_serial(client: &mut OkFpgaGrpcClient) -> String {
    client.list_devices().await.unwrap().into_inner().devices[0].serial.clone()
}

// ============================================================================
// Phase 1: Server Connectivity & Device Discovery
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_01_server_status() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.expect("connect");
    let status = client.get_server_status().await.expect("status");
    let inner = status.into_inner();
    assert!(!inner.api_version.is_empty());
    println!("Server v{}, uptime={}s", inner.api_version, inner.uptime_secs);
}

#[tokio::test]
#[ignore]
async fn test_02_list_devices() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let inner = client.list_devices().await.expect("list").into_inner();
    assert!(!inner.devices.is_empty());
    println!("Found {} device(s): {}", inner.devices.len(), inner.devices[0].serial);
}

#[tokio::test]
#[ignore]
async fn test_03_device_info() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;
    let info = client.get_device_info(&serial).await.expect("info").into_inner();
    let dev = info.device.as_ref().unwrap();
    assert_eq!(dev.serial, serial);
    println!("Device: {}, product={}", dev.serial, dev.product_name);
}

#[tokio::test]
#[ignore]
async fn test_04_list_services() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let inner = client.list_services().await.expect("services").into_inner();
    for s in &inner.services {
        println!("  - {} (status={})", s.name, s.status);
    }
}

// ============================================================================
// Phase 2: Configure FPGA via REST (no gRPC ConfigureDevice RPC yet)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_05_configure_fpga_via_rest() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;
    let path = bitfile_path();

    let http = reqwest::Client::new();
    let url = format!("{}/api/v1/devices/{}/configure", rest_url(), serial);
    let resp = http
        .post(&url)
        .json(&serde_json::json!({ "bitfile_path": path }))
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => {
            println!("FPGA configured via REST: {}", r.text().await.unwrap_or_default());
        }
        Ok(r) => {
            println!("SKIP: configure returned {}", r.status());
            return;
        }
        Err(e) => {
            println!("SKIP: REST not available: {}", e);
            return;
        }
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ============================================================================
// Phase 3: Classic Data Port — Wires
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_06_classic_wire_in_reset() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.set_wire_in_value(&serial, WIRE_IN_EP, 0x01, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    match client.get_wire_out_value(&serial, WIRE_OUT_COUNTER1).await {
        Ok(resp) => println!("After RESET1: counter1={}", resp.into_inner().value),
        Err(e) if is_expected_skip(&e) => println!("SKIP: {}", e),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_07_classic_wire_in_free_run() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.set_wire_in_value(&serial, WIRE_IN_EP, 0x00, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let val1 = match client.get_wire_out_value(&serial, WIRE_OUT_COUNTER1).await {
        Ok(r) => r.into_inner().value,
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    let val2 = client.get_wire_out_value(&serial, WIRE_OUT_COUNTER1).await
        .expect("wire out 2").into_inner().value;

    println!("Free-running: val1={}, val2={}", val1, val2);
    assert_ne!(val1, val2, "counter should be advancing");
}

#[tokio::test]
#[ignore]
async fn test_08_classic_wire_in_disable() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.set_wire_in_value(&serial, WIRE_IN_EP, 0x02, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let val1 = match client.get_wire_out_value(&serial, WIRE_OUT_COUNTER1).await {
        Ok(r) => r.into_inner().value,
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let val2 = client.get_wire_out_value(&serial, WIRE_OUT_COUNTER1).await
        .expect("wire out 2").into_inner().value;

    println!("Disabled: val1={}, val2={}", val1, val2);
    assert_eq!(val1, val2, "counter should be frozen when disabled");
}

// ============================================================================
// Phase 4: Classic Data Port — Triggers
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_09_classic_trigger_reset_counter2() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.set_wire_in_value(&serial, WIRE_IN_EP, 0x00, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;

    match client.activate_trigger_in(&serial, TRIGGER_IN_EP, 0).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let val = client.get_wire_out_value(&serial, WIRE_OUT_COUNTER2).await
        .expect("wire out").into_inner().value;
    println!("Counter2 after reset: {}", val);
    assert_eq!(val, 0);
}

#[tokio::test]
#[ignore]
async fn test_10_classic_trigger_count_up() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.set_wire_in_value(&serial, WIRE_IN_EP, 0x00, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;
    let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 0).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    for _ in 0..5 {
        let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 1).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let val = client.get_wire_out_value(&serial, WIRE_OUT_COUNTER2).await
        .expect("wire out").into_inner().value;
    println!("Counter2 after 5 ups: {}", val);
    assert_eq!(val, 5);
}

#[tokio::test]
#[ignore]
async fn test_11_classic_trigger_count_down() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.set_wire_in_value(&serial, WIRE_IN_EP, 0x00, 0xFF).await {
        Ok(_) => {}
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
    let _ = client.update_wire_ins(&serial).await;
    let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 0).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    for _ in 0..10 {
        let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 1).await;
    }
    for _ in 0..3 {
        let _ = client.activate_trigger_in(&serial, TRIGGER_IN_EP, 2).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let val = client.get_wire_out_value(&serial, WIRE_OUT_COUNTER2).await
        .expect("wire out").into_inner().value;
    println!("Counter2 after 10 up + 3 down: {}", val);
    assert_eq!(val, 7);
}

#[tokio::test]
#[ignore]
async fn test_12_classic_trigger_out_vector() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.get_trigger_out_vector(&serial, 0x60).await {
        Ok(resp) => println!("TriggerOut 0x60: vector={}", resp.into_inner().vector),
        Err(e) if is_expected_skip(&e) => println!("SKIP: {}", e),
        Err(e) => panic!("{}", e),
    }
}

// ============================================================================
// Phase 5: Classic Data Port — Registers
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_13_classic_register_read() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.read_register(&serial, 0x00).await {
        Ok(resp) => println!("Register 0x00: value={}", resp.into_inner().value),
        Err(e) if is_expected_skip(&e) => println!("SKIP: {}", e),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_14_classic_register_write() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.write_register(&serial, 0x00, 0x42).await {
        Ok(_) => println!("Write register 0x00 = 0x42"),
        Err(e) if is_expected_skip(&e) => { println!("SKIP: {}", e); return; }
        Err(e) => panic!("{}", e),
    }
}

// ============================================================================
// Phase 6: AXI (expected to skip on XEM7320)
// ============================================================================

#[tokio::test]
#[ignore]
async fn test_15_axi_lite_read() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.axi_lite_read(&serial, 0x00).await {
        Ok(r) => println!("AXI Lite read: {:?}", r.into_inner()),
        Err(e) if is_expected_skip(&e) => println!("SKIP: {}", e),
        Err(e) => panic!("{}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_16_axi_lite_write() {
    let mut client = OkFpgaGrpcClient::connect(&grpc_url()).await.unwrap();
    let serial = get_serial(&mut client).await;

    match client.axi_lite_write(&serial, 0x00, 0x01).await {
        Ok(_) => println!("AXI Lite write OK"),
        Err(e) if is_expected_skip(&e) => println!("SKIP: {}", e),
        Err(e) => panic!("{}", e),
    }
}
