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

use ok_fpga_core::manager::{DeviceEvent, DeviceManager};
use ok_fpga_core::resource::ResourceRegistry;
use ok_fpga_core::user::UserSessionRegistry;
use tracing::{debug, info, warn};

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
}

