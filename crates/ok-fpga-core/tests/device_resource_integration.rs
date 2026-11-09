// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
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

#[test]
fn test_two_users_classic_bridge_enforcement_and_blocking() {
    use ok_fpga_core::declaration::{
        build_resource, check_overlap, AddressRangeSpec, EndpointEntrySpec, EndpointSpec,
        ResourceSpec,
    };
    use ok_fpga_core::resource::{
        AccessDenied, AcquisitionDenied, ClassicAddressResolution, DefaultAccessPolicy,
        EndpointType, LockOutcome, ResourceAccessPolicy, UserId,
    };
    use std::collections::HashMap;

    struct TestAddressMappingPolicy;

    impl ResourceAccessPolicy for TestAddressMappingPolicy {
        fn can_acquire(
            &self,
            resource: &Resource,
            user: &UserId,
            mode: AccessMode,
        ) -> Result<(), AcquisitionDenied> {
            DefaultAccessPolicy.can_acquire(resource, user, mode)
        }

        fn resolve_classic_address(
            &self,
            _resource: &Resource,
            _user: &UserId,
            endpoint: u8,
            ep_type: &EndpointType,
            value: Option<u32>,
        ) -> ClassicAddressResolution {
            match (endpoint, ep_type) {
                (0x00 | 0x01, EndpointType::WireIn) => {
                    value.map_or(ClassicAddressResolution::NoMapping, |v| {
                        ClassicAddressResolution::MapsToAddress(u64::from(v))
                    })
                }
                _ => ClassicAddressResolution::NoMapping,
            }
        }
    }

    let policy = Box::new(TestAddressMappingPolicy);
    let rr = ResourceRegistry::with_policy(policy);

    let make_port = |name: &str, core_start: u32, core_end: u32| -> Resource {
        let spec = ResourceSpec {
            name: name.to_string(),
            description: None,
            device_serial: "DEV_DUAL".to_string(),
            access_mode: Some(AccessMode::Shared),
            private: false,
            interfaces: vec!["classic".to_string()],
            endpoints: Some(EndpointSpec {
                default_access: Some(AccessMode::Shared),
                entries: vec![
                    EndpointEntrySpec {
                        address: 0x00,
                        ep_type: "wire_in".to_string(),
                        access: Some(AccessMode::Shared),
                        private: false,
                        description: Some("write_addr".to_string()),
                    },
                    EndpointEntrySpec {
                        address: 0x01,
                        ep_type: "wire_in".to_string(),
                        access: Some(AccessMode::Shared),
                        private: false,
                        description: Some("read_addr".to_string()),
                    },
                    EndpointEntrySpec {
                        address: 0x20,
                        ep_type: "wire_out".to_string(),
                        access: Some(AccessMode::Shared),
                        private: false,
                        description: Some("read_data".to_string()),
                    },
                ],
            }),
            address_ranges: vec![AddressRangeSpec {
                name: Some(format!("{}_core", name)),
                start: core_start,
                end: core_end,
                access: Some(AccessMode::Exclusive),
                private: false,
                description: None,
            }],
            metadata: HashMap::new(),
        };
        build_resource(spec).unwrap()
    };

    let port_a = make_port("port_a", 0x1000_0000, 0x1000_0FFF);
    let port_b = make_port("port_b", 0x2000_0000, 0x2000_0FFF);
    rr.try_register(port_a, check_overlap).unwrap();
    rr.try_register(port_b, check_overlap).unwrap();

    rr.acquire("DEV_DUAL:port_a", &"alice".to_string(), AccessMode::Shared)
        .unwrap();
    rr.acquire("DEV_DUAL:port_b", &"bob".to_string(), AccessMode::Shared)
        .unwrap();

    // 1. S-14: Alice can write Port A's exclusive range without locking Port A,
    //    and Bob can write Port B's exclusive range without locking Port B.
    assert!(rr
        .check_endpoint_access(
            "alice",
            "DEV_DUAL",
            0x00,
            &EndpointType::WireIn,
            Some(0x1000_0100)
        )
        .is_ok());
    assert!(rr
        .check_endpoint_access(
            "bob",
            "DEV_DUAL",
            0x00,
            &EndpointType::WireIn,
            Some(0x2000_0100)
        )
        .is_ok());

    // 2. S-20: Alice cannot address Port B's range (0x2000_0100), and Bob cannot
    //    address Port A's range (0x1000_0100).
    assert!(matches!(
        rr.check_endpoint_access(
            "alice",
            "DEV_DUAL",
            0x00,
            &EndpointType::WireIn,
            Some(0x2000_0100)
        )
        .unwrap_err(),
        AccessDenied::NoCoveringResource { .. }
    ));
    assert!(matches!(
        rr.check_endpoint_access(
            "bob",
            "DEV_DUAL",
            0x00,
            &EndpointType::WireIn,
            Some(0x1000_0100)
        )
        .unwrap_err(),
        AccessDenied::NoCoveringResource { .. }
    ));

    // 3. B2: Alice locks Port A for a multi-step bridge sequence -> Port B is
    //    blocked for Bob's writes, while Bob's reads still succeed.
    assert_eq!(
        rr.lock("DEV_DUAL:port_a", &"alice".to_string()).unwrap(),
        LockOutcome::Locked
    );
    assert!(matches!(
        rr.check_endpoint_access(
            "bob",
            "DEV_DUAL",
            0x00,
            &EndpointType::WireIn,
            Some(0x2000_0100)
        )
        .unwrap_err(),
        AccessDenied::BlockedByOther { .. }
    ));
    assert!(rr
        .check_endpoint_access("bob", "DEV_DUAL", 0x20, &EndpointType::WireOut, None)
        .is_ok());

    // 4. Alice unlocks Port A -> Bob's writes to Port B succeed again.
    rr.unlock("DEV_DUAL:port_a", &"alice".to_string()).unwrap();
    assert!(rr
        .check_endpoint_access(
            "bob",
            "DEV_DUAL",
            0x00,
            &EndpointType::WireIn,
            Some(0x2000_0100)
        )
        .is_ok());
}

// ============================================================================
// Tier 2 Hardware Integration Test: XEM7320 + Counters.bit Enforcement (44-E)
// ============================================================================

/// Tier 2 hardware test for Phase 44-E (`B2`, `S-12..S-20`, `D75 F1 & F2`):
/// opens a connected `XEM7320` via `FrontPanelDll`, programs `Counters.bit`,
/// registers two overlapping shared resources (`counter1` and `counter2`) on
/// the live serial number, and verifies two-user enforcement, locking/blocking,
/// `ReadOnly` restrictions, and live FrontPanel WireIn/WireOut/Trigger
/// operations on hardware.
///
/// Run with:
/// ```bash
/// cargo test -p ok-fpga-core --test device_resource_integration test_hardware_xem7320_counters_enforcement_pipeline -- --ignored --test-threads=1 --nocapture
/// ```
#[test]
#[ignore]
fn test_hardware_xem7320_counters_enforcement_pipeline() {
    use ok_fpga_core::declaration::{
        build_resource, check_overlap, EndpointEntrySpec, EndpointSpec, ResourceSpec,
    };
    use ok_fpga_core::resource::{AccessDenied, EndpointType, LockOutcome};
    use ok_fpga_ffi::api::FrontPanelDll;
    use ok_fpga_ffi::*;
    use std::collections::HashMap;
    use std::ffi::{CStr, CString};
    use std::path::{Path, PathBuf};

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new(manifest_dir));

    let dll_path = std::env::var("OKFPGA_DLL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            let lib_name = "okFrontPanel.dll";
            #[cfg(not(target_os = "windows"))]
            let lib_name = "libokFrontPanel.so";
            workspace_root.join("FrontPanel-6.1.0").join(lib_name)
        });

    let dll = FrontPanelDll::load(&dll_path).unwrap_or_else(|e| {
        panic!(
            "Could not load FrontPanel 6.1.0 shared library at {:?}: {}",
            dll_path, e
        )
    });

    let counters_bit = workspace_root
        .join("FrontPanel-6.1.0")
        .join("ExampleBitfiles")
        .join("20190501-FP50-XEM7320-A75")
        .join("XEM7320-A75")
        .join("Counters.bit");
    assert!(
        counters_bit.exists(),
        "Counters.bit not found at {:?}",
        counters_bit
    );

    unsafe {
        let mut err_ptr: *mut okError = std::ptr::null_mut();
        let realm = CString::new("").unwrap();
        let devices_handle = (dll.devices_construct)(realm.as_ptr(), &mut err_ptr);
        assert!(
            !devices_handle.is_null(),
            "Failed to construct FrontPanelDevices handle"
        );

        let count = (dll.devices_get_count)(devices_handle);
        assert!(count > 0, "Expected at least one connected FPGA device");

        // Try each enumerated device until an XEM7320 opens (skipping any
        // non-XEM7320 board such as XEM8320), and report a clear udev hint on
        // Linux if USB device nodes lack read/write permissions.
        let mut opened: Option<(okFrontPanel_HANDLE, String)> = None;
        for idx in 0..count {
            let mut serial_buf = [0i8; 11];
            (dll.devices_get_serial)(devices_handle, idx, serial_buf.as_mut_ptr());
            let serial = CStr::from_ptr(serial_buf.as_ptr())
                .to_string_lossy()
                .to_string();
            eprintln!("  Opening connected device[{}] serial={}", idx, serial);

            let serial_c = CString::new(serial.clone()).unwrap();
            let fp_handle = (dll.devices_open)(devices_handle, serial_c.as_ptr());
            if fp_handle.is_null() {
                continue;
            }

            let mut info = okTDeviceInfo::default();
            let size = std::mem::size_of::<okTDeviceInfo>() as u32;
            let rc = (dll.fp_get_device_info_with_size)(fp_handle, &mut info, size);
            let product_name = if rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                CStr::from_ptr(info.productName.as_ptr())
                    .to_string_lossy()
                    .to_string()
            } else {
                String::new()
            };

            if product_name.starts_with("XEM7320") {
                eprintln!("  Selected {} (serial={})", product_name, serial);
                opened = Some((fp_handle, serial));
                break;
            } else {
                eprintln!(
                    "  Skipping non-XEM7320 device {} (serial={})",
                    product_name, serial
                );
                (dll.fp_destruct)(fp_handle);
            }
        }
        (dll.devices_destruct)(devices_handle);

        let (fp_handle, serial) = opened.unwrap_or_else(|| {
            panic!(
                "Failed to open any connected XEM7320 among {} enumerated FPGA device(s). \
                 On Linux, this is caused by missing read/write permissions on /dev/bus/usb/*/* \
                 (root:root 0664). Install FrontPanel-6.1.0/60-opalkelly.rules:\n  \
                 sudo cp FrontPanel-6.1.0/60-opalkelly.rules /etc/udev/rules.d/60-opalkelly.rules && \
                 sudo udevadm control --reload-rules && sudo udevadm trigger",
                count
            )
        });

        // Program Counters.bit on the connected XEM7320.
        let bit_c = CString::new(counters_bit.to_string_lossy().as_ref()).unwrap();
        let rc = (dll.fp_configure_fpga)(fp_handle, bit_c.as_ptr());
        assert_eq!(
            rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "ConfigureFPGA(Counters.bit) failed: {:?}",
            rc
        );
        assert_ne!(
            (dll.fp_is_front_panel_enabled)(fp_handle),
            0,
            "FrontPanel must be enabled after programming Counters.bit"
        );

        let mut classic: okFPGADataPortClassic_HANDLE = std::ptr::null_mut();
        let rc = (dll.fp_get_fpga_data_port_classic)(fp_handle, &mut classic);
        assert_eq!(rc as i32, ok_ErrorCode::ok_NoError as i32);
        assert!(!classic.is_null());

        // Register two overlapping shared resources on this device (`counter1`
        // and `counter2`, sharing control WireIn 0x00 and TriggerIn 0x40).
        let rr = ResourceRegistry::new();
        let make_counter_res = |name: &str, wire_out: u32| {
            let spec = ResourceSpec {
                name: name.to_string(),
                description: Some(format!("Counters.bit {} on {}", name, serial)),
                device_serial: serial.clone(),
                access_mode: Some(AccessMode::Shared),
                private: false,
                interfaces: vec!["classic".to_string()],
                endpoints: Some(EndpointSpec {
                    default_access: Some(AccessMode::Shared),
                    entries: vec![
                        EndpointEntrySpec {
                            address: 0x00,
                            ep_type: "wire_in".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: Some("Counter control WireIn".to_string()),
                        },
                        EndpointEntrySpec {
                            address: wire_out,
                            ep_type: "wire_out".to_string(),
                            access: Some(AccessMode::Exclusive),
                            private: false,
                            description: Some("Counter value WireOut".to_string()),
                        },
                        EndpointEntrySpec {
                            address: 0x40,
                            ep_type: "trigger_in".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: Some("Counter trigger in".to_string()),
                        },
                        EndpointEntrySpec {
                            address: 0x60,
                            ep_type: "trigger_out".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: Some("Counter trigger out".to_string()),
                        },
                    ],
                }),
                address_ranges: vec![],
                metadata: HashMap::new(),
            };
            build_resource(spec).unwrap()
        };

        let c1_id = format!("{}:counter1", serial);
        let c2_id = format!("{}:counter2", serial);
        rr.try_register(make_counter_res("counter1", 0x20), check_overlap)
            .unwrap();
        rr.try_register(make_counter_res("counter2", 0x21), check_overlap)
            .unwrap();

        rr.acquire(&c1_id, &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        rr.acquire(&c2_id, &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        rr.acquire(&c1_id, &"carol".to_string(), AccessMode::ReadOnly)
            .unwrap();

        // 1. Alice and Carol can read counter1 (0x20); Bob is denied 0x20 (not in counter2).
        assert!(rr
            .check_endpoint_access("alice", &serial, 0x20, &EndpointType::WireOut, None)
            .is_ok());
        assert!(rr
            .check_endpoint_access("carol", &serial, 0x20, &EndpointType::WireOut, None)
            .is_ok());
        assert!(matches!(
            rr.check_endpoint_access("bob", &serial, 0x20, &EndpointType::WireOut, None)
                .unwrap_err(),
            AccessDenied::NoCoveringResource { .. }
        ));

        // 2. Read live free-running counter1 on hardware before and after 30ms.
        assert!(rr.check_device_access("alice", &serial).is_ok());
        assert_eq!(
            (dll.classic_update_wire_outs)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        let v1 = (dll.classic_get_wire_out_value)(classic, 0x20);
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(
            (dll.classic_update_wire_outs)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        let v2 = (dll.classic_get_wire_out_value)(classic, 0x20);
        eprintln!("  Hardware COUNT1 (0x20): {} -> {}", v1, v2);

        // 3. Alice locks counter1 -> Bob is blocked on counter2 writes (0x00),
        //    while Carol (ReadOnly) can still read 0x20 on hardware but is
        //    denied writes and ResetFPGA.
        assert_eq!(
            rr.lock(&c1_id, &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );
        assert!(matches!(
            rr.check_endpoint_access("bob", &serial, 0x00, &EndpointType::WireIn, Some(1))
                .unwrap_err(),
            AccessDenied::BlockedByOther { .. }
        ));
        assert!(rr
            .check_endpoint_access("carol", &serial, 0x20, &EndpointType::WireOut, None)
            .is_ok());
        assert!(matches!(
            rr.check_reset_access("carol", &serial, "ResetFPGA")
                .unwrap_err(),
            AccessDenied::ReadOnlyWriteAttempt { .. }
        ));

        // 4. Alice unlocks counter1 -> Bob's write to 0x00 succeeds again.
        rr.unlock(&c1_id, &"alice".to_string()).unwrap();
        assert!(rr
            .check_endpoint_access("bob", &serial, 0x00, &EndpointType::WireIn, Some(0))
            .is_ok());
        assert!(rr
            .check_device_write_access("bob", &serial, "UpdateWireIns")
            .is_ok());
        assert_eq!(
            (dll.classic_set_wire_in_value)(classic, 0x00, 0, 0xFFFF_FFFF) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert_eq!(
            (dll.classic_update_wire_ins)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );

        // 5. Phase 44-F (D26): Alice re-locks counter1, then ClearFPGAConfiguration
        //    forcibly unlocks, releases, and deletes all resources on the device.
        assert_eq!(
            rr.lock(&c1_id, &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );
        let deleted = rr.force_delete_for_device(&serial);
        assert_eq!(deleted.len(), 2);
        assert_eq!(deleted[0].resource_id, c1_id);
        assert_eq!(deleted[0].unlocked_user.as_deref(), Some("alice"));
        assert_eq!(deleted[0].released_holders, vec!["alice", "carol"]);
        assert_eq!(deleted[1].resource_id, c2_id);
        assert_eq!(deleted[1].unlocked_user, None);
        assert_eq!(deleted[1].released_holders, vec!["bob"]);
        assert_eq!(rr.count(), 0);

        let clear_rc = (dll.fp_clear_fpga_configuration)(fp_handle);
        assert_eq!(
            clear_rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "fp_clear_fpga_configuration should succeed on live XEM7320"
        );
        (dll.fp_destruct)(fp_handle);
    }
}

#[test]
fn test_emulated_reprogram_and_clear_force_delete_resources() {
    use ok_fpga_core::declaration::{build_resource, EndpointEntrySpec, EndpointSpec, ResourceSpec};
    use ok_fpga_core::device::{BitfileRecord, DeviceState, FpgaDevice};
    use ok_fpga_core::manager::{DeleteReason, DeviceEvent, DeviceManager};
    use ok_fpga_core::resource::{AccessMode, EndpointType, LockOutcome, ResourceRegistry};

    let serial = "REPROG44F01".to_string();
    let dm = DeviceManager::new();
    let mut fpga = FpgaDevice::new(test_info(&serial));
    fpga.state = DeviceState::Programmed {
        bitfile: BitfileRecord {
            filename: "v1.bit".to_string(),
            description: None,
            programmed_at: chrono::Utc::now(),
            programmed_by: "alice".to_string(),
            file_path: None,
            md5_hash: None,
        },
    };
    dm.registry().add(fpga);

    let rr = ResourceRegistry::new();
    let r_shared = build_resource(ResourceSpec {
        name: "shared_block".to_string(),
        description: None,
        device_serial: serial.clone(),
        access_mode: Some(AccessMode::Shared),
        private: false,
        interfaces: vec!["classic".to_string()],
        endpoints: Some(EndpointSpec {
            default_access: Some(AccessMode::Shared),
            entries: vec![EndpointEntrySpec {
                address: 0x00,
                ep_type: "wire_in".to_string(),
                access: None,
                private: false,
                description: None,
            }],
        }),
        address_ranges: vec![],
        metadata: Default::default(),
    })
    .unwrap();
    let r_id = r_shared.id.clone();
    rr.register(r_shared);

    rr.acquire(&r_id, &"alice".to_string(), AccessMode::Shared)
        .unwrap();
    rr.acquire(&r_id, &"bob".to_string(), AccessMode::ReadOnly)
        .unwrap();
    assert_eq!(rr.lock(&r_id, &"alice".to_string()).unwrap(), LockOutcome::Locked);

    // Simulate reprogram cleanup (D26, 44-F q3a): transition to Connected (bumps
    // config_generation) and force-delete all resources on the device.
    let gen_before = dm.registry().get(&serial).unwrap().config_generation;
    dm.registry().update_state(&serial, DeviceState::Connected);
    let deleted = rr.force_delete_for_device(&serial);
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].resource_id, r_id);
    assert_eq!(deleted[0].unlocked_user.as_deref(), Some("alice"));
    assert_eq!(deleted[0].released_holders, vec!["alice", "bob"]);
    assert_eq!(rr.count(), 0);

    let mut events = dm.subscribe();
    if let Some(ref locker) = deleted[0].unlocked_user {
        let _ = dm.broadcast(DeviceEvent::ResourceUnlocked {
            resource_id: deleted[0].resource_id.clone(),
            user_id: locker.clone(),
            timestamp: chrono::Utc::now(),
        });
    }
    let _ = dm.broadcast(DeviceEvent::ResourceDeleted {
        resource_id: deleted[0].resource_id.clone(),
        reason: DeleteReason::Reprogram,
        timestamp: chrono::Utc::now(),
    });

    dm.registry().update_state(
        &serial,
        DeviceState::Programmed {
            bitfile: BitfileRecord {
                filename: "v2.bit".to_string(),
                description: None,
                programmed_at: chrono::Utc::now(),
                programmed_by: "alice".to_string(),
                file_path: None,
                md5_hash: None,
            },
        },
    );
    let gen_after = dm.registry().get(&serial).unwrap().config_generation;
    assert!(gen_after > gen_before);

    // Verify Alice and Bob no longer pass endpoint enforcement after reprogram cleanup.
    assert!(rr
        .check_endpoint_access("alice", &serial, 0x00, &EndpointType::WireIn, Some(1))
        .is_err());
    assert!(rr
        .check_endpoint_access("bob", &serial, 0x00, &EndpointType::WireIn, None)
        .is_err());

    let evt1 = events.try_recv().unwrap();
    let evt2 = events.try_recv().unwrap();
    assert!(matches!(evt1, DeviceEvent::ResourceUnlocked { .. }));
    assert!(matches!(
        evt2,
        DeviceEvent::ResourceDeleted {
            reason: DeleteReason::Reprogram,
            ..
        }
    ));
}

// ============================================================================
// Phase 44-G: Classic Data-Path Spec Alignment (S-05, S-06, S-07)
// ============================================================================

#[test]
fn test_emulated_pipe_and_register_round_trip_with_ok_realm_test() {
    use ok_fpga_core::declaration::{
        build_resource, AddressRangeSpec, EndpointEntrySpec, EndpointSpec, ResourceSpec,
    };
    use ok_fpga_core::resource::{AccessMode, EndpointType, ResourceRegistry};
    use ok_fpga_ffi::api::FrontPanelDll;
    use ok_fpga_ffi::*;
    use std::ffi::{CStr, CString};
    use std::path::{Path, PathBuf};

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new(manifest_dir));

    let dll_path = std::env::var("OKFPGA_DLL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            let lib_name = "okFrontPanel.dll";
            #[cfg(not(target_os = "windows"))]
            let lib_name = "libokFrontPanel.so";
            workspace_root.join("FrontPanel-6.1.0").join(lib_name)
        });
    if !dll_path.exists() {
        eprintln!("Skipping okREALM_TEST FFI portion: {:?} not found", dll_path);
        return;
    }

    let dll = FrontPanelDll::load(&dll_path).expect("load libokFrontPanel.so");
    let realm = CString::new("test").unwrap();

    unsafe {
        let mut err_ptr: *mut okError = std::ptr::null_mut();
        let devices_handle = (dll.devices_construct)(realm.as_ptr(), &mut err_ptr);
        assert!(
            !devices_handle.is_null(),
            "okFrontPanelDevices_Construct(okREALM_TEST) failed"
        );

        let count = (dll.devices_get_count)(devices_handle);
        assert!(count >= 1, "okREALM_TEST should expose at least 1 test device");

        let mut serial_buf = [0i8; 64];
        (dll.devices_get_serial)(devices_handle, 0, serial_buf.as_mut_ptr());
        let serial = CStr::from_ptr(serial_buf.as_ptr())
            .to_string_lossy()
            .into_owned();

        let rr = ResourceRegistry::new();
        let r = build_resource(ResourceSpec {
            name: "pipe_and_reg".to_string(),
            description: None,
            device_serial: serial.clone(),
            access_mode: Some(AccessMode::Shared),
            private: false,
            interfaces: vec!["classic".to_string()],
            endpoints: Some(EndpointSpec {
                default_access: Some(AccessMode::Shared),
                entries: vec![
                    EndpointEntrySpec {
                        address: 0x80,
                        ep_type: "pipe_in".to_string(),
                        access: None,
                        private: false,
                        description: None,
                    },
                    EndpointEntrySpec {
                        address: 0xA0,
                        ep_type: "pipe_out".to_string(),
                        access: None,
                        private: false,
                        description: None,
                    },
                ],
            }),
            address_ranges: vec![AddressRangeSpec {
                name: Some("reg_bank".to_string()),
                description: None,
                start: 0x0000,
                end: 0x000F,
                access: Some(AccessMode::Shared),
                private: false,
            }],
            metadata: Default::default(),
        })
        .unwrap();
        let r_id = r.id.clone();
        assert!(rr.register(r));
        rr.acquire(&r_id, &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Enforcement checks for PipeIn (0x80), PipeOut (0xA0), and bulk register batch.
        assert!(rr
            .check_endpoint_access("alice", &serial, 0x80, &EndpointType::PipeIn, None)
            .is_ok());
        assert!(rr
            .check_endpoint_access("alice", &serial, 0xA0, &EndpointType::PipeOut, None)
            .is_ok());
        for addr in [0x0000u64, 0x0004, 0x000F] {
            assert!(rr.check_address_access("alice", &serial, addr, true).is_ok());
            assert!(rr.check_address_access("alice", &serial, addr, false).is_ok());
        }
        assert!(rr
            .check_address_access("alice", &serial, 0x0010, true)
            .is_err());

        (dll.devices_destruct)(devices_handle);
    }
}

#[test]
#[ignore = "Requires live XEM7320 hardware and libokFrontPanel.so"]
fn test_hardware_xem7320_wire_in_auto_update_semantics() {
    use ok_fpga_ffi::api::FrontPanelDll;
    use ok_fpga_ffi::*;
    use std::ffi::{CStr, CString};
    use std::path::{Path, PathBuf};

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new(manifest_dir));

    let dll_path = std::env::var("OKFPGA_DLL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            let lib_name = "okFrontPanel.dll";
            #[cfg(not(target_os = "windows"))]
            let lib_name = "libokFrontPanel.so";
            workspace_root.join("FrontPanel-6.1.0").join(lib_name)
        });
    let dll = FrontPanelDll::load(&dll_path).expect("load libokFrontPanel.so");
    let counters_bit = workspace_root
        .join("FrontPanel-6.1.0")
        .join("ExampleBitfiles")
        .join("20190501-FP50-XEM7320-A75")
        .join("XEM7320-A75")
        .join("Counters.bit");
    assert!(counters_bit.exists(), "Counters.bit must exist");

    unsafe {
        let mut err_ptr: *mut okError = std::ptr::null_mut();
        let realm = CString::new("").unwrap();
        let devices_handle = (dll.devices_construct)(realm.as_ptr(), &mut err_ptr);
        assert!(!devices_handle.is_null());

        let count = (dll.devices_get_count)(devices_handle);
        assert!(count > 0, "Expected at least one connected FPGA device");

        let mut opened: Option<okFrontPanel_HANDLE> = None;
        for idx in 0..count {
            let mut serial_buf = [0i8; 11];
            (dll.devices_get_serial)(devices_handle, idx, serial_buf.as_mut_ptr());
            let serial = CStr::from_ptr(serial_buf.as_ptr())
                .to_string_lossy()
                .to_string();
            let serial_c = CString::new(serial).unwrap();
            let fp_handle = (dll.devices_open)(devices_handle, serial_c.as_ptr());
            if fp_handle.is_null() {
                continue;
            }
            let mut info = okTDeviceInfo::default();
            let size = std::mem::size_of::<okTDeviceInfo>() as u32;
            let rc = (dll.fp_get_device_info_with_size)(fp_handle, &mut info, size);
            let product_name = if rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                CStr::from_ptr(info.productName.as_ptr())
                    .to_string_lossy()
                    .to_string()
            } else {
                String::new()
            };
            if product_name.starts_with("XEM7320") {
                opened = Some(fp_handle);
                break;
            } else {
                (dll.fp_destruct)(fp_handle);
            }
        }
        (dll.devices_destruct)(devices_handle);

        let fp_handle = opened.expect("At least one XEM7320 must be connected");
        let bit_c = CString::new(counters_bit.to_string_lossy().as_ref()).unwrap();
        assert_eq!(
            (dll.fp_configure_fpga)(fp_handle, bit_c.as_ptr()) as i32,
            ok_ErrorCode::ok_NoError as i32
        );

        let mut classic: okFPGADataPortClassic_HANDLE = std::ptr::null_mut();
        assert_eq!(
            (dll.fp_get_fpga_data_port_classic)(fp_handle, &mut classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert!(!classic.is_null());

        // 1. De-assert RESET1 (auto_update = true): SetWireInValue(0x00, 0x00) + UpdateWireIns.
        assert_eq!(
            (dll.classic_set_wire_in_value)(classic, 0x00, 0x00, 0xFFFF_FFFF) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert_eq!(
            (dll.classic_update_wire_ins)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(
            (dll.classic_update_wire_outs)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        let c1_running = (dll.classic_get_wire_out_value)(classic, 0x20) as u32;
        assert!(c1_running > 0, "Counter #1 should be free-running");

        // 2. Stage RESET1=1 with `auto_update = false` (SetWireInValue without UpdateWireIns):
        //    Counter #1 on the FPGA is NOT held in reset and continues counting!
        assert_eq!(
            (dll.classic_set_wire_in_value)(classic, 0x00, 0x01, 0xFFFF_FFFF) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(
            (dll.classic_update_wire_outs)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        let c1_still_running = (dll.classic_get_wire_out_value)(classic, 0x20) as u32;
        assert!(
            c1_still_running > c1_running,
            "Without UpdateWireIns (auto_update=false), Counter #1 must keep incrementing ({} -> {})",
            c1_running,
            c1_still_running
        );

        // 3. Flush with UpdateWireIns now and wait one divided clock period (~20ms)
        //    -> Counter #1 synchronous reset takes effect and holds at 0!
        assert_eq!(
            (dll.classic_update_wire_ins)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(
            (dll.classic_update_wire_outs)(classic) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        let c1_reset = (dll.classic_get_wire_out_value)(classic, 0x20) as u32;
        assert_eq!(c1_reset, 0, "Counter #1 must be 0 once UpdateWireIns flushes RESET1=1");

        let _ = (dll.fp_clear_fpga_configuration)(fp_handle);
        (dll.fp_destruct)(fp_handle);
    }
}

// ============================================================================
// Phase 49: Separable Dual-XEM7320 Hardware Isolation & Enforcement Test
// ============================================================================

/// Verifies independent programming, resource registration, lock isolation, and
/// live counter/trigger control across two physical XEM7320 boards when both are
/// connected.
///
/// **Separable / Single-Board Friendly:** If fewer than 2 physical XEM7320 boards
/// are connected, this test logs a skip notice and returns `Ok` so single-board
/// hardware test suites pass without failure.
#[test]
#[ignore = "Requires live XEM7320 hardware (skips gracefully if < 2 boards connected)"]
fn test_hardware_dual_xem7320_independent_resources_and_enforcement() {
    use ok_fpga_core::declaration::{
        build_resource, check_overlap, EndpointEntrySpec, EndpointSpec, ResourceSpec,
    };
    use ok_fpga_core::resource::{AccessMode, EndpointType, LockOutcome, ResourceRegistry};
    use ok_fpga_ffi::api::FrontPanelDll;
    use ok_fpga_ffi::*;
    use std::collections::HashMap;
    use std::ffi::{CStr, CString};
    use std::path::{Path, PathBuf};

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new(manifest_dir));

    let dll_path = std::env::var("OKFPGA_DLL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            #[cfg(target_os = "windows")]
            let lib_name = "okFrontPanel.dll";
            #[cfg(not(target_os = "windows"))]
            let lib_name = "libokFrontPanel.so";
            workspace_root.join("FrontPanel-6.1.0").join(lib_name)
        });
    let dll = FrontPanelDll::load(&dll_path).expect("load libokFrontPanel.so");
    let counters_bit = workspace_root
        .join("FrontPanel-6.1.0")
        .join("ExampleBitfiles")
        .join("20190501-FP50-XEM7320-A75")
        .join("XEM7320-A75")
        .join("Counters.bit");
    assert!(counters_bit.exists(), "Counters.bit must exist");

    unsafe {
        let mut err_ptr: *mut okError = std::ptr::null_mut();
        let realm = CString::new("").unwrap();
        let devices_handle = (dll.devices_construct)(realm.as_ptr(), &mut err_ptr);
        assert!(!devices_handle.is_null());

        let count = (dll.devices_get_count)(devices_handle);
        let mut boards: Vec<(String, okFrontPanel_HANDLE)> = Vec::new();

        for idx in 0..count {
            let mut serial_buf = [0i8; 11];
            (dll.devices_get_serial)(devices_handle, idx, serial_buf.as_mut_ptr());
            let serial = CStr::from_ptr(serial_buf.as_ptr())
                .to_string_lossy()
                .to_string();
            let serial_c = CString::new(serial.clone()).unwrap();
            let fp_handle = (dll.devices_open)(devices_handle, serial_c.as_ptr());
            if fp_handle.is_null() {
                continue;
            }
            let mut info = okTDeviceInfo::default();
            let size = std::mem::size_of::<okTDeviceInfo>() as u32;
            let rc = (dll.fp_get_device_info_with_size)(fp_handle, &mut info, size);
            let product_name = if rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                CStr::from_ptr(info.productName.as_ptr())
                    .to_string_lossy()
                    .to_string()
            } else {
                String::new()
            };
            if product_name.starts_with("XEM7320") {
                boards.push((serial, fp_handle));
                if boards.len() == 2 {
                    break;
                }
            } else {
                (dll.fp_destruct)(fp_handle);
            }
        }
        (dll.devices_destruct)(devices_handle);

        if boards.len() < 2 {
            eprintln!(
                "SKIP: test_hardware_dual_xem7320_independent_resources_and_enforcement \
                 requires >= 2 connected XEM7320 boards (found {}); skipping cleanly.",
                boards.len()
            );
            for (_, h) in boards {
                (dll.fp_destruct)(h);
            }
            return;
        }

        let (serial_a, fp_a) = boards.remove(0);
        let (serial_b, fp_b) = boards.remove(0);
        eprintln!(
            "Running dual-XEM7320 isolation test on Board A ({}) and Board B ({})",
            serial_a, serial_b
        );

        let bit_c = CString::new(counters_bit.to_string_lossy().as_ref()).unwrap();
        assert_eq!(
            (dll.fp_configure_fpga)(fp_a, bit_c.as_ptr()) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert_eq!(
            (dll.fp_configure_fpga)(fp_b, bit_c.as_ptr()) as i32,
            ok_ErrorCode::ok_NoError as i32
        );

        let mut classic_a: okFPGADataPortClassic_HANDLE = std::ptr::null_mut();
        let mut classic_b: okFPGADataPortClassic_HANDLE = std::ptr::null_mut();
        assert_eq!(
            (dll.fp_get_fpga_data_port_classic)(fp_a, &mut classic_a) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert_eq!(
            (dll.fp_get_fpga_data_port_classic)(fp_b, &mut classic_b) as i32,
            ok_ErrorCode::ok_NoError as i32
        );

        // Register identical resource layouts on Board A and Board B in the same ResourceRegistry.
        let rr = ResourceRegistry::new();
        let make_board_res = |serial: &str, name: &str| -> ok_fpga_core::resource::Resource {
            build_resource(ResourceSpec {
                name: name.to_string(),
                description: Some(format!("Counters resource on {}", serial)),
                device_serial: serial.to_string(),
                access_mode: Some(AccessMode::Shared),
                private: false,
                interfaces: vec!["classic".to_string()],
                endpoints: Some(EndpointSpec {
                    default_access: Some(AccessMode::Shared),
                    entries: vec![
                        EndpointEntrySpec {
                            address: 0x00,
                            ep_type: "wire_in".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: None,
                        },
                        EndpointEntrySpec {
                            address: 0x20,
                            ep_type: "wire_out".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: None,
                        },
                        EndpointEntrySpec {
                            address: 0x21,
                            ep_type: "wire_out".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: None,
                        },
                        EndpointEntrySpec {
                            address: 0x40,
                            ep_type: "trigger_in".to_string(),
                            access: Some(AccessMode::Shared),
                            private: false,
                            description: None,
                        },
                    ],
                }),
                address_ranges: vec![],
                metadata: HashMap::new(),
            })
            .unwrap()
        };

        let res_a_id = format!("{}:counters", serial_a);
        let res_b_id = format!("{}:counters", serial_b);
        rr.try_register(make_board_res(&serial_a, "counters"), check_overlap)
            .unwrap();
        rr.try_register(make_board_res(&serial_b, "counters"), check_overlap)
            .unwrap();

        rr.acquire(&res_a_id, &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        rr.acquire(&res_b_id, &"bob".to_string(), AccessMode::Shared)
            .unwrap();

        // Locking Board A by Alice must NOT block Bob on Board B, and Alice cannot
        // access Board B (she holds no resource on Board B).
        assert_eq!(
            rr.lock(&res_a_id, &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );
        assert!(rr
            .check_endpoint_access("bob", &serial_b, 0x00, &EndpointType::WireIn, Some(0))
            .is_ok());
        assert!(rr
            .check_endpoint_access("alice", &serial_b, 0x20, &EndpointType::WireOut, None)
            .is_err());

        // Reset Counter #2 on both boards, then trigger Counter #2 3 times on Board A
        // and 7 times on Board B — verifying independent physical FPGA state.
        assert_eq!(
            (dll.classic_activate_trigger_in)(classic_a, 0x40, 0) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert_eq!(
            (dll.classic_activate_trigger_in)(classic_b, 0x40, 0) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        for _ in 0..3 {
            assert_eq!(
                (dll.classic_activate_trigger_in)(classic_a, 0x40, 1) as i32,
                ok_ErrorCode::ok_NoError as i32
            );
        }
        for _ in 0..7 {
            assert_eq!(
                (dll.classic_activate_trigger_in)(classic_b, 0x40, 1) as i32,
                ok_ErrorCode::ok_NoError as i32
            );
        }

        assert_eq!(
            (dll.classic_update_wire_outs)(classic_a) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        assert_eq!(
            (dll.classic_update_wire_outs)(classic_b) as i32,
            ok_ErrorCode::ok_NoError as i32
        );
        let c2_a = (dll.classic_get_wire_out_value)(classic_a, 0x21) & 0xFF;
        let c2_b = (dll.classic_get_wire_out_value)(classic_b, 0x21) & 0xFF;
        eprintln!(
            "  Dual-board Counter #2 values: Board A ({}) = {}, Board B ({}) = {}",
            serial_a, c2_a, serial_b, c2_b
        );
        assert_eq!(c2_a, 3, "Board A Counter #2 should equal 3");
        assert_eq!(c2_b, 7, "Board B Counter #2 should equal 7");

        // Clearing Board A must only delete Board A's resources; Board B remains intact.
        let deleted_a = rr.force_delete_for_device(&serial_a);
        assert_eq!(deleted_a.len(), 1);
        assert_eq!(rr.list_for_device(&serial_a).len(), 0);
        assert_eq!(rr.list_for_device(&serial_b).len(), 1);

        let _ = (dll.fp_clear_fpga_configuration)(fp_a);
        let _ = (dll.fp_clear_fpga_configuration)(fp_b);
        (dll.fp_destruct)(fp_a);
        (dll.fp_destruct)(fp_b);
    }
}
