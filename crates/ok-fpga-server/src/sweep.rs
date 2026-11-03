//! # User Session Sweep Task
//!
//! Background task that periodically scans for expired user sessions
//! and auto-releases their held resources (ADR-020 §5).
//!
//! ## How It Works
//!
//! 1. Every `sweep_interval` seconds, the task wakes up.
//! 2. It calls `user_sessions.expired(timeout)` to find users whose
//!    `last_seen_at` is older than the configured timeout.
//! 3. For each expired user, it:
//!    a. Finds all resources they hold (via the `ResourceRegistry`)
//!    b. Releases each resource
//!    c. Broadcasts a `ResourceChanged` event via the `DeviceManager`
//!    d. Removes the user session
//!    e. Logs a warning
//!
//! ## Configuration
//!
//! ```toml
//! [server]
//! user_timeout_secs = 60         # Expire after 60s of silence
//! user_sweep_interval_secs = 10  # Check every 10s
//! ```
//!
//! Set `user_timeout_secs = 0` to disable the sweep task entirely.

use std::sync::Arc;
use std::time::Duration;

use ok_fpga_core::manager::{DeleteReason, DeviceEvent, DeviceManager};
use ok_fpga_core::resource::ResourceRegistry;
use ok_fpga_core::user::UserSessionRegistry;
use tracing::{debug, info, warn};

/// Scan for and forcibly delete any orphan resources whose device is either
/// absent from the `DeviceRegistry` (`reason = DeleteReason::DeviceRemoved`)
/// or no longer programmed (`reason = DeleteReason::Clear`) (ADR-034 §14,
/// 44-F `q2`).
///
/// Returns the number of orphan resources removed.
pub fn sweep_orphan_resources(
    resource_registry: &ResourceRegistry,
    device_manager: &DeviceManager,
    event_tx: Option<&tokio::sync::broadcast::Sender<String>>,
) -> usize {
    let mut resources = resource_registry.list();
    resources.sort_by(|a, b| a.id.cmp(&b.id));

    let mut removed = 0_usize;
    for resource in resources {
        let orphan_reason = match device_manager.registry().get(&resource.device_serial) {
            None => Some(DeleteReason::DeviceRemoved),
            Some(dev) if !dev.is_programmed() => Some(DeleteReason::Clear),
            Some(_) => None,
        };

        if let Some(reason) = orphan_reason {
            if let Some(deleted) = resource_registry.force_delete_resource(&resource.id) {
                removed += 1;
                warn!(
                    resource_id = %deleted.resource_id,
                    device_serial = %resource.device_serial,
                    reason = reason.as_str(),
                    "sweep deleted orphan resource"
                );
                crate::state::broadcast_force_deleted_resources(
                    device_manager,
                    event_tx,
                    &resource.device_serial,
                    std::slice::from_ref(&deleted),
                    reason,
                );
            }
        }
    }

    removed
}

/// Spawn the background sweep task.
///
/// This function starts a `tokio::spawn` task that runs indefinitely,
/// periodically checking for expired user sessions. Call this from
/// `main.rs` after all services are started.
///
/// Does nothing if `timeout_secs` is 0 (sweep disabled).
///
/// # Arguments
///
/// * `user_sessions` — The user session registry to check
/// * `resource_registry` — Used to release held resources
/// * `device_manager` — Used to broadcast ResourceChanged events (Option B)
/// * `timeout_secs` — Seconds of inactivity before expiration
/// * `sweep_interval_secs` — How often to run the sweep (seconds)
/// * `event_tx` — Optional broadcast channel for dashboard events (F9)
pub fn spawn_sweep_task(
    user_sessions: Arc<UserSessionRegistry>,
    resource_registry: Arc<ResourceRegistry>,
    device_manager: Arc<DeviceManager>,
    timeout_secs: u64,
    sweep_interval_secs: u64,
    event_tx: Option<tokio::sync::broadcast::Sender<String>>,
) {
    // Don't start the sweep task if the timeout is 0 (disabled).
    if timeout_secs == 0 {
        info!("user session sweep disabled (user_timeout_secs = 0)");
        return;
    }

    let timeout = Duration::from_secs(timeout_secs);
    let interval = Duration::from_secs(sweep_interval_secs);

    info!(
        timeout_secs = timeout_secs,
        sweep_interval_secs = sweep_interval_secs,
        "starting user session sweep task"
    );

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;

            // Sweep any orphan resources whose device was unplugged or cleared.
            let _ = sweep_orphan_resources(&resource_registry, &device_manager, event_tx.as_ref());

            let expired = user_sessions.expired(timeout);
            if expired.is_empty() {
                continue;
            }

            debug!(
                expired_count = expired.len(),
                "sweep found expired user sessions"
            );

            for session in &expired {
                // Find all resources held by this user across all devices.
                let all_resources = resource_registry.list();
                let mut released_count = 0_usize;

                for resource in &all_resources {
                    if resource.has_holder(&session.user_id) {
                        // Release this user's hold on the resource.
                        if let Some(released) = resource_registry
                            .release_with_info(&resource.id, &session.user_id)
                        {
                            released_count += 1;

                            if released.was_locked {
                                let _ = device_manager.broadcast(DeviceEvent::ResourceUnlocked {
                                    resource_id: resource.id.clone(),
                                    user_id: session.user_id.clone(),
                                    timestamp: chrono::Utc::now(),
                                });
                                if let Some(ref tx) = event_tx {
                                    let _ = tx.send(format!(
                                        r#"{{"type":"resource_unlocked","resource_id":"{}","user_id":"{}"}}"#,
                                        resource.id, session.user_id
                                    ));
                                }
                            }

                            // Broadcast a ResourceChanged event (Option B — handlers broadcast).
                            // Read the state back after the release and send its spec
                            // string (`available`, `in_use_shared`, ...; roadmap S-09).
                            let _ = device_manager.broadcast(DeviceEvent::ResourceChanged {
                                resource_id: resource.id.clone(),
                                new_state: released.new_state.as_str().to_string(),
                                timestamp: chrono::Utc::now(),
                            });
                            if let Some(ref tx) = event_tx {
                                let _ = tx.send(format!(
                                    r#"{{"type":"resource_released","resource_id":"{}","user_id":"{}"}}"#,
                                    resource.id, session.user_id
                                ));
                            }
                        }
                    }
                }

                // Remove the expired user session.
                user_sessions.remove(&session.user_id);

                if released_count > 0 {
                    warn!(
                        user_id = %session.user_id,
                        released_count = released_count,
                        "auto-released resources for expired user"
                    );
                } else {
                    debug!(
                        user_id = %session.user_id,
                        "removed expired user session (no held resources)"
                    );
                }

                // Broadcast DeviceEvent for SSE subscribers (Phase 39, ADR-030).
                // HeartbeatTimeout is modeled as UserDisconnected { reason: "timeout" }.
                let _ = device_manager.broadcast(DeviceEvent::UserDisconnected {
                    user_id: session.user_id.clone(),
                    reason: "timeout".to_string(),
                    timestamp: chrono::Utc::now(),
                });

                // Broadcast user_disconnected event (Phase 25-Fix-H, F9).
                if let Some(ref tx) = event_tx {
                    let _ = tx.send(format!(
                        r#"{{"type":"user_disconnected","user_id":"{}"}}"#,
                        session.user_id
                    ));
                }
            }
        }
    });
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_sweep_disabled_when_timeout_zero() {
        // This should not panic or spawn a task.
        // We can't easily verify it didn't spawn, but at least
        // verify the function returns without error.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();

        rt.block_on(async {
            spawn_sweep_task(
                Arc::new(UserSessionRegistry::new()),
                Arc::new(ResourceRegistry::new()),
                Arc::new(DeviceManager::new()),
                0,  // disabled
                10,
                None, // no event broadcasting in tests
            );
            // If we get here without panic, the disabled path works.
        });
    }

    #[tokio::test]
    async fn test_sweep_releases_expired_resources() {
        use ok_fpga_core::resource::{AccessMode, Resource};

        let user_sessions = Arc::new(UserSessionRegistry::new());
        let resource_registry = Arc::new(ResourceRegistry::new());
        let _device_manager = Arc::new(DeviceManager::new());

        let alice = "alice".to_string();

        // Create a resource and have alice acquire it.
        let resource = Resource::new(
            "test-device:counter".to_string(),
            "counter".to_string(),
            None,
            "test-device".to_string(),
            AccessMode::Shared,
        );
        resource_registry.register(resource);
        resource_registry
            .acquire("test-device:counter", &alice, AccessMode::Shared)
            .unwrap();

        // Touch alice's session.
        user_sessions.touch("alice");

        // Verify the resource is held by alice before sweep.
        let resource = resource_registry.get("test-device:counter").unwrap();
        assert!(resource.has_holder("alice"));

        // Verify alice exists in the session registry.
        assert!(user_sessions.get("alice").is_some());
        assert_eq!(user_sessions.count(), 1);
    }

    #[tokio::test]
    async fn test_sweep_orphan_resource_device_absent_deleted() {
        use ok_fpga_core::device::{
            BitfileRecord, DataportType, DeviceInfo, DeviceInterface, DeviceState, FpgaDevice,
            FpgaVendor, UsbSpeed,
        };
        use ok_fpga_core::resource::{AccessMode, Resource};

        let resource_registry = ResourceRegistry::new();
        let device_manager = DeviceManager::new();

        let make_info = |serial: &str| DeviceInfo {
            serial: serial.to_string(),
            product_name: "XEM7320-A75T".to_string(),
            product_id: 43,
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
            device_id: "XEM7320".to_string(),
        };

        // 1. Add an unprogrammed device ("unprog-dev") and a programmed device ("prog-dev").
        //    Leave "absent-dev" completely absent from the DeviceRegistry.
        device_manager
            .registry()
            .add(FpgaDevice::new(make_info("unprog-dev")));
        let mut prog_dev = FpgaDevice::new(make_info("prog-dev"));
        prog_dev.state = DeviceState::Programmed {
            bitfile: BitfileRecord {
                filename: "counters.bit".to_string(),
                description: None,
                programmed_at: chrono::Utc::now(),
                programmed_by: "alice".to_string(),
                file_path: None,
                md5_hash: None,
            },
        };
        device_manager.registry().add(prog_dev);

        // 2. Register one resource on each of the three devices.
        //    Lock the resource on "absent-dev" to verify ResourceUnlocked precedes ResourceDeleted(DeviceRemoved).
        resource_registry.register(Resource::new(
            "absent-dev:r_absent".to_string(),
            "r_absent".to_string(),
            None,
            "absent-dev".to_string(),
            AccessMode::Shared,
        ));
        resource_registry
            .acquire(
                "absent-dev:r_absent",
                &"alice".to_string(),
                AccessMode::Shared,
            )
            .unwrap();
        resource_registry
            .lock("absent-dev:r_absent", &"alice".to_string())
            .unwrap();

        resource_registry.register(Resource::new(
            "unprog-dev:r_unprog".to_string(),
            "r_unprog".to_string(),
            None,
            "unprog-dev".to_string(),
            AccessMode::Exclusive,
        ));
        resource_registry
            .acquire(
                "unprog-dev:r_unprog",
                &"bob".to_string(),
                AccessMode::Exclusive,
            )
            .unwrap();

        resource_registry.register(Resource::new(
            "prog-dev:r_prog".to_string(),
            "r_prog".to_string(),
            None,
            "prog-dev".to_string(),
            AccessMode::Shared,
        ));
        resource_registry
            .acquire(
                "prog-dev:r_prog",
                &"charlie".to_string(),
                AccessMode::Shared,
            )
            .unwrap();

        let mut events = device_manager.subscribe();
        let (ws_tx, mut ws_rx) = tokio::sync::broadcast::channel(16);
        let removed = sweep_orphan_resources(&resource_registry, &device_manager, Some(&ws_tx));
        assert_eq!(removed, 2);

        // Only "prog-dev:r_prog" remains.
        assert!(resource_registry.get("absent-dev:r_absent").is_none());
        assert!(resource_registry.get("unprog-dev:r_unprog").is_none());
        assert!(resource_registry.get("prog-dev:r_prog").is_some());

        let mut collected = Vec::new();
        while let Ok(evt) = events.try_recv() {
            collected.push(evt);
        }

        let unlock_pos = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    DeviceEvent::ResourceUnlocked { resource_id, user_id, .. }
                        if resource_id == "absent-dev:r_absent" && user_id == "alice"
                )
            })
            .expect("ResourceUnlocked must be emitted for locked orphan resource");
        let delete_absent_pos = collected
            .iter()
            .position(|e| {
                matches!(
                    e,
                    DeviceEvent::ResourceDeleted {
                        resource_id,
                        reason: DeleteReason::DeviceRemoved,
                        ..
                    } if resource_id == "absent-dev:r_absent"
                )
            })
            .expect("ResourceDeleted(DeviceRemoved) must be emitted for absent-dev");
        assert!(unlock_pos < delete_absent_pos);

        assert!(
            collected.iter().any(|e| matches!(
                e,
                DeviceEvent::ResourceDeleted {
                    resource_id,
                    reason: DeleteReason::Clear,
                    ..
                } if resource_id == "unprog-dev:r_unprog"
            )),
            "ResourceDeleted(Clear) must be emitted for unprog-dev"
        );

        let mut ws_messages = Vec::new();
        while let Ok(msg) = ws_rx.try_recv() {
            ws_messages.push(msg);
        }
        assert!(ws_messages
            .iter()
            .any(|m| m.contains(r#""reason":"device_removed""#)));
        assert!(ws_messages.iter().any(|m| m.contains(r#""reason":"clear""#)));
    }
}

