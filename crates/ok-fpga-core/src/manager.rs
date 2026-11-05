//! # Device Manager & Hot-Plug Events
//!
//! This module implements the device lifecycle management from ADR-002.
//! It provides:
//!
//! - [`DeviceEvent`](crate::manager::DeviceEvent) — Events broadcast when devices are added, removed,
//!   or their state changes.
//! - [`DeviceRegistry`](crate::manager::DeviceRegistry) — Thread-safe map of serial → [`FpgaDevice`](crate::device::FpgaDevice).
//! - [`DeviceManager`](crate::manager::DeviceManager) — Orchestrates device discovery, hot-plug callbacks,
//!   and the event broadcast channel.
//!
//! ## Architecture (ADR-002)
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │              DeviceManager (Rust)                │
//! │                                                  │
//! │  ┌────────────────────┐  ┌───────────────────┐  │
//! │  │ FrontPanelManager  │  │ Device Registry   │  │
//! │  │ (C API callbacks)  │──│ HashMap<Serial,   │  │
//! │  │                    │  │   FpgaDevice>      │  │
//! │  └────────┬───────────┘  └───────────────────┘  │
//! │           │                                      │
//! │  ┌────────▼───────────┐                         │
//! │  │ Event Channel      │                         │
//! │  │ (tokio::broadcast) │                         │
//! │  └────────┬───────────┘                         │
//! │           │                                      │
//! └───────────┼──────────────────────────────────────┘
//!             │
//!      ┌──────┴──────┬──────────────┐
//!      ▼             ▼              ▼
//!   REST SSE    gRPC Stream    Web Dashboard
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::device::{DeviceInfo, DeviceState, FpgaDevice};

// ============================================================================
// Device Events
// ============================================================================

/// Events that occur during device lifecycle management.
///
/// These events are broadcast to all subscribers (REST SSE, gRPC streams,
/// web dashboard WebSocket) whenever a device's state changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceEvent {
    /// A new device was connected and recognized by the server.
    ///
    /// This fires after the device passes USB 2.0 filtering and board
    /// recognition (ADR-002 step 4).
    DeviceAdded {
        /// Serial number of the newly connected device.
        serial: String,
        /// Product name (e.g., "XEM8320-AU25P").
        product_name: String,
        /// Device interface type as a string (e.g., "USB 3.0").
        interface: String,
        /// When the device was detected.
        timestamp: DateTime<Utc>,
    },

    /// A device was disconnected (physically removed or powered off).
    ///
    /// All resources for this device have been invalidated.
    DeviceRemoved {
        /// Serial number of the disconnected device.
        serial: String,
        /// When the disconnection was detected.
        timestamp: DateTime<Utc>,
    },

    /// A device's FPGA was successfully programmed with a bitfile.
    DeviceProgrammed {
        /// Serial number of the programmed device.
        serial: String,
        /// The bitfile that was loaded.
        bitfile: String,
        /// Who programmed it.
        programmed_by: String,
        /// When the programming occurred.
        timestamp: DateTime<Utc>,
    },

    /// A resource's state changed (acquired, released, invalidated).
    ResourceChanged {
        /// The resource ID that changed.
        resource_id: String,
        /// The new state as a string.
        new_state: String,
        /// When the change occurred.
        timestamp: DateTime<Utc>,
    },

    // ── New variants (Phase 39, ADR-030) ─────────────────────────

    /// A user session was created (first API call from this user).
    UserConnected {
        /// The user who connected.
        user_id: String,
        /// When the session was created.
        timestamp: DateTime<Utc>,
    },

    /// A user session ended (explicit disconnect or sweep expiration).
    UserDisconnected {
        /// The user who disconnected.
        user_id: String,
        /// Why: "explicit" (user called disconnect) or "timeout" (sweep expired).
        reason: String,
        /// When the disconnection occurred.
        timestamp: DateTime<Utc>,
    },

    /// A new resource definition was registered (POST /resources).
    ResourceCreated {
        /// The newly created resource ID.
        resource_id: String,
        /// The device this resource belongs to.
        device_serial: String,
        /// Human-readable resource name.
        name: String,
        /// When the resource was created.
        timestamp: DateTime<Utc>,
    },

    /// A resource definition was deleted.
    ResourceDeleted {
        /// The deleted resource ID.
        resource_id: String,
        /// Why it was deleted (D58): `client` (DELETE /resources/{id} or the
        /// gRPC `DeleteResource` RPC), `reprogram` or `clear` (forced
        /// cleanup, sub-phase 44-F).
        reason: DeleteReason,
        /// When the resource was deleted.
        timestamp: DateTime<Utc>,
    },

    // ── ADR-034 (Phase 44-A) ──

    /// A holder locked a shared resource.
    ResourceLocked {
        /// The locked resource.
        resource_id: String,
        /// The user who took the lock.
        user_id: String,
        /// When the lock was taken.
        timestamp: DateTime<Utc>,
    },

    /// The lock holder unlocked a shared resource.
    ResourceUnlocked {
        /// The unlocked resource.
        resource_id: String,
        /// The user who released the lock.
        user_id: String,
        /// When the lock was released.
        timestamp: DateTime<Utc>,
    },

    /// The server is shutting down gracefully.
    ServerShutdown {
        /// Why: "signal" (SIGINT/SIGTERM).
        reason: String,
        /// When the shutdown was initiated.
        timestamp: DateTime<Utc>,
    },
}

/// Why a resource was deleted (ADR-034, D58, 44-F Q2).
///
/// Serialised as `client`, `reprogram`, `clear`, or `device_removed`, matching
/// the OpenAPI `ResourceDeletedEvent.reason` enum and proto `DeleteReason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteReason {
    /// A client deleted the resource.
    Client,
    /// The FPGA was reprogrammed (forced cleanup, 44-F).
    Reprogram,
    /// The FPGA configuration was cleared (forced cleanup, 44-F).
    Clear,
    /// The physical FPGA device disconnected or was swept as absent (44-F Q2).
    DeviceRemoved,
}

impl DeleteReason {
    /// The spec string: `"client"`, `"reprogram"`, `"clear"`, or `"device_removed"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            DeleteReason::Client => "client",
            DeleteReason::Reprogram => "reprogram",
            DeleteReason::Clear => "clear",
            DeleteReason::DeviceRemoved => "device_removed",
        }
    }
}

impl DeviceEvent {
    /// The SSE `event:` name for this variant (the variant name, e.g.
    /// `ResourceLocked`), as listed in the OpenAPI `DeviceEvent` description.
    pub fn name(&self) -> &'static str {
        match self {
            DeviceEvent::DeviceAdded { .. } => "DeviceAdded",
            DeviceEvent::DeviceRemoved { .. } => "DeviceRemoved",
            DeviceEvent::DeviceProgrammed { .. } => "DeviceProgrammed",
            DeviceEvent::ResourceChanged { .. } => "ResourceChanged",
            DeviceEvent::UserConnected { .. } => "UserConnected",
            DeviceEvent::UserDisconnected { .. } => "UserDisconnected",
            DeviceEvent::ResourceCreated { .. } => "ResourceCreated",
            DeviceEvent::ResourceDeleted { .. } => "ResourceDeleted",
            DeviceEvent::ResourceLocked { .. } => "ResourceLocked",
            DeviceEvent::ResourceUnlocked { .. } => "ResourceUnlocked",
            DeviceEvent::ServerShutdown { .. } => "ServerShutdown",
        }
    }
}

// ============================================================================
// Device Registry
// ============================================================================

/// Thread-safe registry of all connected FPGA devices.
///
/// The registry is the single source of truth for device state. It is
/// accessed by:
/// - The device manager (add/remove devices)
/// - REST/gRPC handlers (query device info)
/// - The resource manager (check device state before resource operations)
///
/// ## Thread Safety
///
/// The inner `HashMap` is wrapped in `RwLock` to allow multiple concurrent
/// readers (REST queries) with exclusive writers (device manager callbacks).
pub struct DeviceRegistry {
    /// Map from device serial number to device state.
    devices: RwLock<HashMap<String, FpgaDevice>>,
}

impl DeviceRegistry {
    /// Create a new empty device registry.
    pub fn new() -> Self {
        DeviceRegistry {
            devices: RwLock::new(HashMap::new()),
        }
    }

    /// Register a newly connected device.
    ///
    /// Returns `true` if the device was added, `false` if it already existed.
    pub fn add(&self, device: FpgaDevice) -> bool {
        let mut map = self.devices.write().unwrap();
        if map.contains_key(device.serial()) {
            false
        } else {
            map.insert(device.serial().to_string(), device);
            true
        }
    }

    /// Remove a device by serial number.
    ///
    /// Returns the removed device, or `None` if it wasn't registered.
    pub fn remove(&self, serial: &str) -> Option<FpgaDevice> {
        self.devices.write().unwrap().remove(serial)
    }

    /// Get a clone of a supported device by serial number.
    ///
    /// Per D70, devices whose `dataport` is [`crate::device::DataportType::Unknown`]
    /// are unsupported and return `None`.
    pub fn get(&self, serial: &str) -> Option<FpgaDevice> {
        self.devices
            .read()
            .unwrap()
            .get(serial)
            .filter(|d| d.info.dataport.is_supported())
            .cloned()
    }

    /// List all registered supported devices (returns cloned values).
    ///
    /// Per D70, devices with [`crate::device::DataportType::Unknown`] are omitted.
    pub fn list(&self) -> Vec<FpgaDevice> {
        self.devices
            .read()
            .unwrap()
            .values()
            .filter(|d| d.info.dataport.is_supported())
            .cloned()
            .collect()
    }

    /// Returns the number of registered supported devices (omitting `Unknown` dataport, D70).
    pub fn count(&self) -> usize {
        self.devices
            .read()
            .unwrap()
            .values()
            .filter(|d| d.info.dataport.is_supported())
            .count()
    }

    /// Update the state of a device by serial number and bump its
    /// `config_generation` counter (44-F).
    ///
    /// Returns `true` if the device was found and updated, `false` otherwise.
    pub fn update_state(&self, serial: &str, new_state: DeviceState) -> bool {
        let mut map = self.devices.write().unwrap();
        if let Some(device) = map.get_mut(serial) {
            device.state = new_state;
            device.config_generation = device.config_generation.wrapping_add(1);
            true
        } else {
            false
        }
    }

    /// Check if a supported device with the given serial is registered.
    pub fn contains(&self, serial: &str) -> bool {
        self.devices
            .read()
            .unwrap()
            .get(serial)
            .is_some_and(|d| d.info.dataport.is_supported())
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// DeviceManager
// ============================================================================

/// The capacity of the event broadcast channel.
///
/// This determines how many events can be buffered before slow receivers
/// start losing events. 256 is generous for typical FPGA lab usage.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Orchestrates device discovery, hot-plug management, and event broadcasting.
///
/// The `DeviceManager` is the entry point for all device lifecycle operations.
/// It owns the device registry and the event broadcast channel.
///
/// ## Usage
///
/// ```
/// use ok_fpga_core::manager::DeviceManager;
///
/// let manager = DeviceManager::new();
/// let mut rx = manager.subscribe();
/// assert_eq!(manager.device_count(), 0);
/// ```
pub struct DeviceManager {
    /// The device registry (thread-safe).
    registry: Arc<DeviceRegistry>,

    /// Event broadcast sender. All subscribers receive every event.
    event_tx: broadcast::Sender<DeviceEvent>,
}

impl DeviceManager {
    /// Create a new `DeviceManager` with an empty registry.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        DeviceManager {
            registry: Arc::new(DeviceRegistry::new()),
            event_tx,
        }
    }

    /// Get a reference to the device registry.
    pub fn registry(&self) -> &Arc<DeviceRegistry> {
        &self.registry
    }

    /// Subscribe to device events.
    ///
    /// Returns a `broadcast::Receiver` that receives clones of every
    /// `DeviceEvent` broadcast by the manager.
    pub fn subscribe(&self) -> broadcast::Receiver<DeviceEvent> {
        self.event_tx.subscribe()
    }

    /// Returns the number of currently registered devices.
    pub fn device_count(&self) -> usize {
        self.registry.count()
    }

    /// Handle a device being connected.
    ///
    /// This is called when the FrontPanel Manager's `OnDeviceAdded` callback
    /// fires (or from a channel that receives callback messages). It:
    ///
    /// 1. Registers the device in the registry.
    /// 2. Broadcasts a `DeviceAdded` event.
    ///
    /// Returns `true` if the device was newly added, `false` if it was
    /// already registered.
    pub fn on_device_added(&self, info: DeviceInfo) -> bool {
        if !info.dataport.is_supported() {
            return false;
        }
        let serial = info.serial.clone();
        let product_name = info.product_name.clone();
        let interface = info.device_interface.to_string();
        let device = FpgaDevice::new(info);

        if self.registry.add(device) {
            // Broadcast the event (ignore send errors — no subscribers is OK).
            let _ = self.event_tx.send(DeviceEvent::DeviceAdded {
                serial,
                product_name,
                interface,
                timestamp: Utc::now(),
            });
            true
        } else {
            false
        }
    }

    /// Handle a device being disconnected.
    ///
    /// This is called when the FrontPanel Manager's `OnDeviceRemoved` callback
    /// fires. It:
    ///
    /// 1. Removes the device from the registry.
    /// 2. Broadcasts a `DeviceRemoved` event.
    ///
    /// Returns the removed device, or `None` if it wasn't registered.
    pub fn on_device_removed(&self, serial: &str) -> Option<FpgaDevice> {
        let removed = self.registry.remove(serial);
        if removed.is_some() {
            let _ = self.event_tx.send(DeviceEvent::DeviceRemoved {
                serial: serial.to_string(),
                timestamp: Utc::now(),
            });
        }
        removed
    }

    /// Broadcast an event to all subscribers.
    ///
    /// Returns the number of subscribers that received the event.
    pub fn broadcast(&self, event: DeviceEvent) -> usize {
        self.event_tx.send(event).unwrap_or(0)
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::*;

    fn test_device_info(serial: &str) -> DeviceInfo {
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

    #[test]
    fn manager_starts_empty() {
        let manager = DeviceManager::new();
        assert_eq!(manager.device_count(), 0);
    }

    #[test]
    fn add_device_increments_count() {
        let manager = DeviceManager::new();
        assert!(manager.on_device_added(test_device_info("ABC123")));
        assert_eq!(manager.device_count(), 1);
    }

    #[test]
    fn duplicate_device_is_rejected() {
        let manager = DeviceManager::new();
        assert!(manager.on_device_added(test_device_info("ABC123")));
        assert!(!manager.on_device_added(test_device_info("ABC123")));
        assert_eq!(manager.device_count(), 1);
    }

    #[test]
    fn remove_device_decrements_count() {
        let manager = DeviceManager::new();
        manager.on_device_added(test_device_info("ABC123"));
        assert!(manager.on_device_removed("ABC123").is_some());
        assert_eq!(manager.device_count(), 0);
    }

    #[test]
    fn remove_unknown_device_returns_none() {
        let manager = DeviceManager::new();
        assert!(manager.on_device_removed("UNKNOWN").is_none());
    }

    #[tokio::test]
    async fn subscriber_receives_add_event() {
        let manager = DeviceManager::new();
        let mut rx = manager.subscribe();

        manager.on_device_added(test_device_info("ABC123"));

        let event = rx.recv().await.unwrap();
        match event {
            DeviceEvent::DeviceAdded { serial, .. } => {
                assert_eq!(serial, "ABC123");
            }
            _ => panic!("Expected DeviceAdded event"),
        }
    }

    #[tokio::test]
    async fn subscriber_receives_remove_event() {
        let manager = DeviceManager::new();
        let mut rx = manager.subscribe();

        manager.on_device_added(test_device_info("ABC123"));
        let _ = rx.recv().await; // consume the add event

        manager.on_device_removed("ABC123");

        let event = rx.recv().await.unwrap();
        match event {
            DeviceEvent::DeviceRemoved { serial, .. } => {
                assert_eq!(serial, "ABC123");
            }
            _ => panic!("Expected DeviceRemoved event"),
        }
    }

    #[test]
    fn registry_update_state() {
        let registry = DeviceRegistry::new();
        registry.add(FpgaDevice::new(test_device_info("ABC123")));

        assert!(registry.update_state(
            "ABC123",
            DeviceState::Programmed {
                bitfile: crate::device::BitfileRecord {
                    filename: "test.bit".to_string(),
                    description: None,
                    programmed_at: Utc::now(),
                    programmed_by: "test".to_string(),
                    file_path: None,
                    md5_hash: None,
                },
            },
        ));

        let device = registry.get("ABC123").unwrap();
        assert!(device.is_programmed());
    }

    // ── Phase 39 (ADR-030): New DeviceEvent serialization tests ──

    #[test]
    fn user_connected_event_serializes_correctly() {
        let event = DeviceEvent::UserConnected {
            user_id: "alice".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"UserConnected\"") || json.contains("\"user_id\":\"alice\""));
        // Deserialize round-trip
        let parsed: DeviceEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            DeviceEvent::UserConnected { user_id, .. } => assert_eq!(user_id, "alice"),
            _ => panic!("Expected UserConnected"),
        }
    }

    #[test]
    fn user_disconnected_event_serializes_correctly() {
        let event = DeviceEvent::UserDisconnected {
            user_id: "bob".to_string(),
            reason: "timeout".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"bob\""));
        assert!(json.contains("\"timeout\""));
        let parsed: DeviceEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            DeviceEvent::UserDisconnected { user_id, reason, .. } => {
                assert_eq!(user_id, "bob");
                assert_eq!(reason, "timeout");
            }
            _ => panic!("Expected UserDisconnected"),
        }
    }

    #[test]
    fn resource_created_event_serializes_correctly() {
        let event = DeviceEvent::ResourceCreated {
            resource_id: "XEM7320:counters".to_string(),
            device_serial: "XEM7320".to_string(),
            name: "counters".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"XEM7320:counters\""));
        let parsed: DeviceEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            DeviceEvent::ResourceCreated { resource_id, device_serial, name, .. } => {
                assert_eq!(resource_id, "XEM7320:counters");
                assert_eq!(device_serial, "XEM7320");
                assert_eq!(name, "counters");
            }
            _ => panic!("Expected ResourceCreated"),
        }
    }

    #[test]
    fn resource_deleted_event_serializes_correctly() {
        let event = DeviceEvent::ResourceDeleted {
            resource_id: "XEM7320:counters".to_string(),
            reason: DeleteReason::Client,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"XEM7320:counters\""));
        let parsed: DeviceEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            DeviceEvent::ResourceDeleted { resource_id, .. } => {
                assert_eq!(resource_id, "XEM7320:counters");
            }
            _ => panic!("Expected ResourceDeleted"),
        }
    }

    #[test]
    fn test_delete_reason_serializes_snake_case() {
        for (reason, text) in [
            (DeleteReason::Client, "client"),
            (DeleteReason::Reprogram, "reprogram"),
            (DeleteReason::Clear, "clear"),
            (DeleteReason::DeviceRemoved, "device_removed"),
        ] {
            assert_eq!(serde_json::to_string(&reason).unwrap(), format!("\"{}\"", text));
            assert_eq!(reason.as_str(), text);
        }
    }

    #[test]
    fn test_resource_deleted_event_reason_serialized() {
        let event = DeviceEvent::ResourceDeleted {
            resource_id: "r".to_string(),
            reason: DeleteReason::Reprogram,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"reason\":\"reprogram\""));
    }

    #[test]
    fn test_resource_lock_events_roundtrip_and_name() {
        let locked = DeviceEvent::ResourceLocked {
            resource_id: "r".to_string(),
            user_id: "alice".to_string(),
            timestamp: Utc::now(),
        };
        let unlocked = DeviceEvent::ResourceUnlocked {
            resource_id: "r".to_string(),
            user_id: "alice".to_string(),
            timestamp: Utc::now(),
        };
        assert_eq!(locked.name(), "ResourceLocked");
        assert_eq!(unlocked.name(), "ResourceUnlocked");
        let parsed: DeviceEvent =
            serde_json::from_str(&serde_json::to_string(&locked).unwrap()).unwrap();
        assert!(matches!(parsed, DeviceEvent::ResourceLocked { ref user_id, .. } if user_id == "alice"));
    }

    #[test]
    fn server_shutdown_event_serializes_correctly() {
        let event = DeviceEvent::ServerShutdown {
            reason: "signal".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"signal\""));
        let parsed: DeviceEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            DeviceEvent::ServerShutdown { reason, .. } => {
                assert_eq!(reason, "signal");
            }
            _ => panic!("Expected ServerShutdown"),
        }
    }

    #[test]
    fn test_device_registry_unknown_dataport_omitted_and_not_found() {
        let registry = DeviceRegistry::new();
        let mut unknown_info = test_device_info("UNK001");
        unknown_info.dataport = DataportType::Unknown;
        registry.add(FpgaDevice::new(unknown_info));

        let legacy_info = test_device_info("LEG001");
        registry.add(FpgaDevice::new(legacy_info));

        assert_eq!(registry.count(), 1);
        let listed = registry.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].serial(), "LEG001");
        assert!(registry.get("UNK001").is_none());
        assert!(!registry.contains("UNK001"));
        assert!(registry.get("LEG001").is_some());
        assert!(registry.contains("LEG001"));
    }
}
