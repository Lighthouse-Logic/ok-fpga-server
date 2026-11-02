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
        AccessMode::Shared,
    )
}

fn make_exclusive_resource(device_serial: &str, name: &str) -> Resource {
    Resource::new(
        format!("{}:{}", device_serial, name),
        name.to_string(),
        None,
        device_serial.to_string(),
        AccessMode::Exclusive,
    )
}

#[test]
fn resource_lifecycle_acquire_release() {
    let rr = ResourceRegistry::new();

    rr.register(make_exclusive_resource("DEV001", "dac_ch1"));
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

    rr.register(make_exclusive_resource("DEV001", "exclusive_res"));

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
fn invalidate_device_deletes_all_resources() {
    let rr = ResourceRegistry::new();

    rr.register(make_exclusive_resource("DEV_CLEAR", "res1"));
    rr.register(make_resource("DEV_CLEAR", "res2"));
    rr.register(make_exclusive_resource("DEV_OTHER", "res3"));

    let alice = "alice".to_string();
    let bob = "bob".to_string();
    let carol = "carol".to_string();

    let _ = rr.acquire("DEV_CLEAR:res1", &alice, AccessMode::Exclusive);
    let _ = rr.acquire("DEV_CLEAR:res2", &bob, AccessMode::Shared);
    let _ = rr.acquire("DEV_OTHER:res3", &carol, AccessMode::Exclusive);

    // ADR-020 §4: invalidate_device DELETES resources (not just marks unavailable).
    let deleted = rr.invalidate_device("DEV_CLEAR");

    assert_eq!(deleted.len(), 2, "two resources should be deleted");

    let dev_clear_resources = rr.list_for_device("DEV_CLEAR");
    assert_eq!(
        dev_clear_resources.len(),
        0,
        "DEV_CLEAR resources should be deleted, not just unavailable"
    );

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

#[test]
fn test_two_users_two_overlapping_shared_resources_lock_blocking() {
    use ok_fpga_core::declaration::{
        build_resource, check_overlap, EndpointEntrySpec, EndpointSpec, ResourceSpec,
    };
    use ok_fpga_core::resource::{ConflictCode, LockOutcome, ResourceState};
    use std::collections::HashMap;

    let dm = DeviceManager::new();
    dm.on_device_added(test_info("DEV_LOCK"));
    let rr = ResourceRegistry::new();

    // Register two overlapping shared resources on DEV_LOCK:
    // - res_r declares shared WireIn 0x00 and 0x01
    // - res_s declares shared WireIn 0x01 and 0x02
    let make_spec = |name: &str, ep1: u32, ep2: u32| ResourceSpec {
        name: name.to_string(),
        description: None,
        device_serial: "DEV_LOCK".to_string(),
        access_mode: Some(AccessMode::Shared),
        interfaces: vec!["classic".to_string()],
        endpoints: Some(EndpointSpec {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntrySpec {
                    address: ep1,
                    ep_type: "wire_in".to_string(),
                    access: Some(AccessMode::Shared),
                    private: false,
                    description: None,
                },
                EndpointEntrySpec {
                    address: ep2,
                    ep_type: "wire_in".to_string(),
                    access: Some(AccessMode::Shared),
                    private: false,
                    description: None,
                },
            ],
        }),
        address_ranges: Vec::new(),
        metadata: HashMap::new(),
        private: false,
    };

    let res_r = build_resource(make_spec("res_r", 0x00, 0x01)).unwrap();
    let res_s = build_resource(make_spec("res_s", 0x01, 0x02)).unwrap();

    rr.try_register(res_r, check_overlap).unwrap();
    rr.try_register(res_s, check_overlap).unwrap();

    let alice = "alice".to_string();
    let bob = "bob".to_string();

    // Alice acquires res_r; Bob acquires res_s.
    rr.acquire("DEV_LOCK:res_r", &alice, AccessMode::Shared)
        .unwrap();
    rr.acquire("DEV_LOCK:res_s", &bob, AccessMode::Shared)
        .unwrap();

    // Alice locks res_r.
    assert_eq!(
        rr.lock("DEV_LOCK:res_r", &alice).unwrap(),
        LockOutcome::Locked
    );
    // Repeat lock by Alice is idempotent (44-D Q2).
    assert_eq!(
        rr.lock("DEV_LOCK:res_r", &alice).unwrap(),
        LockOutcome::AlreadyLockedBySelf
    );

    // Bob tries to lock res_s -> denied with ConflictCode::Blocked naming res_r.
    let err = rr.lock("DEV_LOCK:res_s", &bob).unwrap_err();
    let conflict = err.conflict().expect("must return Blocked conflict");
    assert_eq!(conflict.code, ConflictCode::Blocked);
    assert_eq!(conflict.resource_id.as_deref(), Some("DEV_LOCK:res_r"));
    assert_eq!(conflict.resource_name.as_deref(), Some("res_r"));

    // Alice unlocks res_r -> Bob can now lock res_s.
    rr.unlock("DEV_LOCK:res_r", &alice).unwrap();
    assert_eq!(
        rr.get("DEV_LOCK:res_r").unwrap().state,
        ResourceState::InUseShared
    );
    assert_eq!(
        rr.lock("DEV_LOCK:res_s", &bob).unwrap(),
        LockOutcome::Locked
    );
    assert_eq!(
        rr.get("DEV_LOCK:res_s").unwrap().state,
        ResourceState::InUseLocked
    );
}
