//! # Device Manager Integration Tests
//!
//! Tests device registration, hot-plug event broadcasting, and
//! resource lifecycle tied to device connect/disconnect.

use ok_fpga_core::device::*;
use ok_fpga_core::manager::DeviceManager;
use ok_fpga_core::resource::{AccessMode, Resource, ResourceRegistry};

/// Helper to create a valid DeviceInfo for testing.
fn test_info(serial: &str) -> DeviceInfo {
    DeviceInfo {
        serial: serial.to_string(),
        product_name: "XEM8320-AU25P".to_string(),
        product_id: 56,
        device_interface: DeviceInterface::Usb3,
        usb_speed: UsbSpeed::Super,
        device_major_version: 1,
        device_minor_version: 0,
        wire_width: 32,
        trigger_width: 32,
        pipe_width: 32,
        register_address_width: 32,
        register_data_width: 32,
        fpga_vendor: FpgaVendor::Xilinx,
        dataport: DataportType::Legacy,
        device_id: "".to_string(),
    }
}

// ============================================================================
// Device Manager Integration Tests
// ============================================================================

#[test]
fn device_manager_add_and_list() {
    let dm = DeviceManager::new();

    assert!(dm.on_device_added(test_info("TEST001")));
    assert_eq!(dm.device_count(), 1);

    assert!(dm.on_device_added(test_info("TEST002")));
    assert_eq!(dm.device_count(), 2);

    let devices = dm.registry().list();
    assert_eq!(devices.len(), 2);
    let serials: Vec<&str> = devices.iter().map(|d| d.serial()).collect();
    assert!(serials.contains(&"TEST001"));
    assert!(serials.contains(&"TEST002"));
}

#[test]
fn device_manager_remove_and_recount() {
    let dm = DeviceManager::new();

    dm.on_device_added(test_info("TEST_REMOVE"));
    assert_eq!(dm.device_count(), 1);

    let removed = dm.on_device_removed("TEST_REMOVE");
    assert!(removed.is_some());
    assert_eq!(dm.device_count(), 0);
}

#[test]
fn device_manager_duplicate_rejected() {
    let dm = DeviceManager::new();

    assert!(dm.on_device_added(test_info("DUPE")));
    assert!(!dm.on_device_added(test_info("DUPE")));
    assert_eq!(dm.device_count(), 1);
}

#[tokio::test]
async fn device_events_are_broadcast() {
    let dm = DeviceManager::new();
    let mut rx = dm.subscribe();

    dm.on_device_added(test_info("EVENT_TEST"));

    let event = rx.recv().await.expect("should receive add event");
    let event_str = format!("{:?}", event);
    assert!(
        event_str.contains("EVENT_TEST"),
        "event should contain serial: {}",
        event_str
    );
}

#[tokio::test]
async fn device_remove_event_is_broadcast() {
    let dm = DeviceManager::new();
    let mut rx = dm.subscribe();

    dm.on_device_added(test_info("REMOVE_EVT"));
    let _ = rx.recv().await; // consume add event

    dm.on_device_removed("REMOVE_EVT");
    let event = rx.recv().await.expect("should receive remove event");
    let event_str = format!("{:?}", event);
    assert!(
        event_str.contains("REMOVE_EVT"),
        "event should contain serial: {}",
        event_str
    );
}

// ============================================================================
// Resource Registry Integration Tests
// ============================================================================

fn make_resource(device_serial: &str, name: &str) -> Resource {
    Resource::new(
        format!("{}:{}", device_serial, name),
        name.to_string(),
        None,
        device_serial.to_string(),
    )
}

#[test]
fn resource_lifecycle_acquire_release() {
    let rr = ResourceRegistry::new();

    rr.register(make_resource("DEV001", "dac_ch1"));
    assert_eq!(rr.count(), 1);

    let user = "alice".to_string();
    let result = rr.acquire("DEV001:dac_ch1", &user, AccessMode::Exclusive);
    assert!(result.is_ok(), "acquire should succeed: {:?}", result);

    let released = rr.release("DEV001:dac_ch1", &user);
    assert!(released, "release should succeed");
}

#[test]
fn exclusive_resource_blocks_second_user() {
    let rr = ResourceRegistry::new();

    rr.register(make_resource("DEV001", "exclusive_res"));

    let alice = "alice".to_string();
    let bob = "bob".to_string();

    let r1 = rr.acquire("DEV001:exclusive_res", &alice, AccessMode::Exclusive);
    assert!(r1.is_ok());

    let r2 = rr.acquire("DEV001:exclusive_res", &bob, AccessMode::Exclusive);
    assert!(r2.is_err(), "exclusive resource should block second user");
}

#[test]
fn shared_resource_allows_multiple_users() {
    let rr = ResourceRegistry::new();

    rr.register(make_resource("DEV001", "shared_res"));

    let alice = "alice".to_string();
    let bob = "bob".to_string();

    let r1 = rr.acquire("DEV001:shared_res", &alice, AccessMode::Shared);
    assert!(r1.is_ok());

    let r2 = rr.acquire("DEV001:shared_res", &bob, AccessMode::Shared);
    assert!(r2.is_ok(), "shared resource should allow multiple users");
}

#[test]
fn invalidate_device_clears_all_resources() {
    let rr = ResourceRegistry::new();

    rr.register(make_resource("DEV_CLEAR", "res1"));
    rr.register(make_resource("DEV_CLEAR", "res2"));
    rr.register(make_resource("DEV_OTHER", "res3"));

    let alice = "alice".to_string();
    let bob = "bob".to_string();
    let carol = "carol".to_string();

    let _ = rr.acquire("DEV_CLEAR:res1", &alice, AccessMode::Exclusive);
    let _ = rr.acquire("DEV_CLEAR:res2", &bob, AccessMode::Shared);
    let _ = rr.acquire("DEV_OTHER:res3", &carol, AccessMode::Exclusive);

    // invalidate_device sets state to Unavailable and clears holders,
    // but does NOT remove the resources from the registry.
    rr.invalidate_device("DEV_CLEAR");

    let dev_clear_resources = rr.list_for_device("DEV_CLEAR");
    assert_eq!(
        dev_clear_resources.len(),
        2,
        "resources should still exist but be invalidated"
    );
    for res in &dev_clear_resources {
        assert_eq!(
            res.state,
            ok_fpga_core::resource::ResourceState::Unavailable,
            "resource {} should be Unavailable",
            res.id
        );
        assert!(res.holders.is_empty(), "holders should be cleared");
        assert!(res.access_mode.is_none(), "access_mode should be None");
    }

    // DEV_OTHER resources should be unaffected (still held).
    let dev_other_resources = rr.list_for_device("DEV_OTHER");
    assert_eq!(
        dev_other_resources.len(),
        1,
        "DEV_OTHER resources should be unaffected"
    );
    assert!(
        !dev_other_resources[0].holders.is_empty(),
        "DEV_OTHER holder should remain"
    );
}
