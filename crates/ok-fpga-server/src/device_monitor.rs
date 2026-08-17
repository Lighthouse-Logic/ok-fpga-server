//! # Device Monitor — FrontPanelManager Hot-Plug Callbacks
//!
//! This module implements Phase 15 (ADR-002): a persistent device monitor that
//! uses the FrontPanel `FrontPanelManager` C API for real-time hot-plug
//! detection. When a device is connected or disconnected, the DLL fires
//! `OnDeviceAdded` / `OnDeviceRemoved` callbacks, which this module bridges
//! into the Rust async runtime via a channel.
//!
//! ## Architecture
//!
//! ```text
//! DLL thread                        Tokio runtime
//! ──────────                        ─────────────
//! on_device_added_cb() ──┐
//!                        ├─→ std::sync::mpsc ─→ event_processor task
//! on_device_removed_cb()─┘                      │
//!                                               ├─ open device, read info
//!                                               ├─ DeviceManager::on_device_added/removed
//!                                               └─ event_tx (WebSocket broadcast)
//! ```
//!
//! ## Fallback
//!
//! If the monitor cannot be constructed (e.g., DLL missing or old version),
//! `main.rs` falls back to the one-shot [`scanner`](super::scanner) from
//! Phase 14.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;

use ok_fpga_core::device::{
    DataportType, DeviceInfo, DeviceInterface, FpgaVendor, UsbSpeed,
};
use ok_fpga_core::manager::DeviceManager;
use ok_fpga_ffi::api::FrontPanelDll;
use ok_fpga_ffi::*;
use tracing::{debug, error, info, warn};

// ============================================================================
// Types
// ============================================================================

/// A wrapper around `okCFrontPanelManager_HANDLE` that is `Send`.
///
/// The FrontPanel DLL's Manager API is designed to be called from any thread
/// (StartMonitoring/StopMonitoring/Open are documented as thread-safe).
/// Raw pointers are not `Send` in Rust, so we wrap it to express this safety.
#[derive(Clone, Copy)]
struct SendableManagerHandle(okCFrontPanelManager_HANDLE);

// SAFETY: The FrontPanelManager handle is used via the DLL's thread-safe API.
// We only call Open/StopMonitoring/Destruct through it, and the DLL manages
// its own internal synchronization.
unsafe impl Send for SendableManagerHandle {}
unsafe impl Sync for SendableManagerHandle {}

/// Events sent from the DLL callback thread to the async event processor.
enum MonitorEvent {
    /// A device was physically connected (serial number).
    DeviceAdded(String),
    /// A device was physically disconnected (serial number).
    DeviceRemoved(String),
}

/// Context passed as the `void* context` parameter to the C callbacks.
///
/// This struct is heap-allocated (boxed) so its address is stable for the
/// lifetime of the monitor. The DLL's internal thread receives this pointer
/// and uses it to send events to the async runtime.
struct CallbackContext {
    /// Channel sender for bridging DLL thread → tokio task.
    tx: mpsc::Sender<MonitorEvent>,
}

/// A persistent device monitor using FrontPanelManager hot-plug callbacks.
///
/// The monitor owns the DLL, the Manager handle, and the callback context.
/// When started, it spawns a tokio task that processes device events and
/// updates the `DeviceManager` registry.
///
/// ## Lifecycle
///
/// 1. [`DeviceMonitor::try_new`] — loads DLL, constructs manager with callbacks
/// 2. [`DeviceMonitor::start`] — calls `StartMonitoring`, spawns event processor
/// 3. The server runs... hot-plug events flow automatically
/// 4. [`DeviceMonitor::stop`] — calls `StopMonitoring`
/// 5. `Drop` — destructs the manager handle
pub struct DeviceMonitor {
    /// The loaded FrontPanel shared library.
    dll: Arc<FrontPanelDll>,
    /// The C FrontPanelManager handle. Must outlive all device handles.
    manager_handle: okCFrontPanelManager_HANDLE,
    /// Receive side of the callback channel (consumed by `start()`).
    rx: Option<mpsc::Receiver<MonitorEvent>>,
    /// Stable context pointer for the C callbacks (must outlive the manager).
    /// Stored as a raw pointer because the DLL holds a reference to it.
    _callback_ctx: *mut CallbackContext,
}

// SAFETY: The DeviceMonitor can be sent between threads. The inner
// okCFrontPanelManager_HANDLE is an opaque pointer used only through the DLL's
// thread-safe API (StartMonitoring/StopMonitoring/Destruct). The raw
// callback context pointer is only read by the DLL's internal callback thread
// and is valid for the lifetime of the DeviceMonitor.
unsafe impl Send for DeviceMonitor {}

// ============================================================================
// C Callback Functions
// ============================================================================

/// `extern "C"` callback invoked by the DLL's internal thread when a device
/// is connected.
///
/// # Safety
///
/// This function is called from a foreign thread with a `void*` context
/// that must point to a valid `CallbackContext`. The `serial` parameter is
/// a null-terminated C string owned by the DLL.
unsafe extern "C" fn on_device_added_cb(context: *mut c_void, serial: *const c_char) {
    if context.is_null() || serial.is_null() {
        return;
    }
    let ctx = &*(context as *const CallbackContext);
    let serial_str = CStr::from_ptr(serial).to_string_lossy().to_string();
    debug!(serial = %serial_str, "DLL callback: device added");
    let _ = ctx.tx.send(MonitorEvent::DeviceAdded(serial_str));
}

/// `extern "C"` callback invoked by the DLL's internal thread when a device
/// is disconnected.
///
/// # Safety
///
/// Same safety requirements as [`on_device_added_cb`].
unsafe extern "C" fn on_device_removed_cb(context: *mut c_void, serial: *const c_char) {
    if context.is_null() || serial.is_null() {
        return;
    }
    let ctx = &*(context as *const CallbackContext);
    let serial_str = CStr::from_ptr(serial).to_string_lossy().to_string();
    debug!(serial = %serial_str, "DLL callback: device removed");
    let _ = ctx.tx.send(MonitorEvent::DeviceRemoved(serial_str));
}

// ============================================================================
// DeviceMonitor Implementation
// ============================================================================

impl DeviceMonitor {
    /// Try to create a device monitor with the FrontPanelManager API.
    ///
    /// Returns `Some(monitor)` on success, `None` if the DLL cannot be loaded
    /// or the manager cannot be constructed.
    ///
    /// ## Arguments
    ///
    /// * `realm` — `""` (empty) for `okREALM_LOCAL` (production), `"test"` for
    ///   `okREALM_TEST` (emulated devices for CI).
    pub fn try_new(realm: &str) -> Option<Self> {
        let path = resolve_dll_path();
        info!(dll_path = %path.display(), "attempting to load FrontPanel DLL for hot-plug monitor");

        let dll = match FrontPanelDll::load(&path) {
            Ok(dll) => {
                let version = dll.api_version_string();
                info!(
                    api_version = %version,
                    dll_path = %path.display(),
                    "FrontPanel DLL loaded for hot-plug monitor"
                );
                Arc::new(dll)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    dll_path = %path.display(),
                    "could not load FrontPanel DLL - hot-plug monitor unavailable"
                );
                return None;
            }
        };

        // Create the callback channel.
        let (tx, rx) = mpsc::channel();

        // Allocate callback context on the heap (stable address for C pointer).
        let ctx = Box::new(CallbackContext { tx });
        let ctx_ptr = Box::into_raw(ctx);

        // Construct the manager with callbacks.
        //
        // SAFETY: We pass the context pointer as `self` — the DLL stores it and
        // passes it back as the `void* context` parameter in both callbacks.
        // The context is valid for the lifetime of the DeviceMonitor (freed in Drop).
        let realm_cstr = CString::new(realm).unwrap_or_default();
        let handle = unsafe {
            (dll.manager_construct_with_callbacks)(
                ctx_ptr as okFrontPanelManager_HANDLE,
                realm_cstr.as_ptr(),
                on_device_added_cb,
                on_device_removed_cb,
            )
        };

        if handle.is_null() {
            error!("FrontPanelManager construction failed - returned null handle");
            // Reclaim the context box to avoid leaking.
            unsafe { drop(Box::from_raw(ctx_ptr)); }
            return None;
        }

        info!(realm = %realm, "FrontPanelManager constructed successfully");

        Some(DeviceMonitor {
            dll,
            manager_handle: handle,
            rx: Some(rx),
            _callback_ctx: ctx_ptr,
        })
    }

    /// Start monitoring for device connections/disconnections.
    ///
    /// This calls `okFrontPanelManager_StartMonitoring` and spawns a tokio
    /// task that processes the callback events. The manager automatically
    /// fires `OnDeviceAdded` for every already-connected device.
    ///
    /// ## Arguments
    ///
    /// * `device_manager` — The shared device registry to update on events.
    /// * `event_tx` — The WebSocket broadcast channel for dashboard notifications.
    pub fn start(
        &mut self,
        device_manager: Arc<DeviceManager>,
        event_tx: tokio::sync::broadcast::Sender<String>,
    ) -> bool {
        // Start monitoring — the DLL begins firing callbacks.
        let result = unsafe { (self.dll.manager_start_monitoring)(self.manager_handle) };
        if result as i32 != ok_ErrorCode::ok_NoError as i32 {
            error!(
                error_code = result as i32,
                "FrontPanelManager_StartMonitoring failed"
            );
            return false;
        }

        info!("FrontPanelManager monitoring started - hot-plug active");

        // Take the receiver (can only start once).
        let rx = match self.rx.take() {
            Some(rx) => rx,
            None => {
                error!("DeviceMonitor::start() called more than once");
                return false;
            }
        };

        // Clone the DLL Arc for the event processor task.
        let dll = self.dll.clone();
        let mgr_handle = SendableManagerHandle(self.manager_handle);

        // Spawn the event processor task.
        tokio::spawn(async move {
            event_processor(rx, dll, mgr_handle, device_manager, event_tx).await;
        });

        true
    }

    /// Stop monitoring for device events.
    ///
    /// This is called during server shutdown. It tells the DLL to stop
    /// firing callbacks. The event processor task will naturally exit
    /// when the channel sender is dropped.
    #[allow(dead_code)] // Will be used during graceful shutdown (Phase 16+).
    pub fn stop(&self) {
        info!("stopping FrontPanelManager monitoring");
        unsafe {
            (self.dll.manager_stop_monitoring)(self.manager_handle);
        }
    }
}

impl Drop for DeviceMonitor {
    fn drop(&mut self) {
        // Stop monitoring (idempotent — safe to call even if not started).
        unsafe {
            (self.dll.manager_stop_monitoring)(self.manager_handle);
        }

        // Destruct the manager handle.
        unsafe {
            (self.dll.manager_destruct)(self.manager_handle);
        }

        // Reclaim the callback context box.
        if !self._callback_ctx.is_null() {
            unsafe {
                drop(Box::from_raw(self._callback_ctx));
            }
            // Don't double-free.
            self._callback_ctx = std::ptr::null_mut();
        }

        debug!("DeviceMonitor dropped - FrontPanelManager destructed");
    }
}

// ============================================================================
// Event Processor Task
// ============================================================================

/// Async task that reads device events from the callback channel and processes
/// them — opens devices, reads info, updates the registry, broadcasts events.
///
/// This runs on the tokio runtime (not the DLL's callback thread), so it can
/// safely do async work, hold locks, and interact with the device manager.
async fn event_processor(
    rx: mpsc::Receiver<MonitorEvent>,
    dll: Arc<FrontPanelDll>,
    manager_handle: SendableManagerHandle,
    device_manager: Arc<DeviceManager>,
    event_tx: tokio::sync::broadcast::Sender<String>,
) {
    info!("device event processor started");

    // Bridge: std::sync::mpsc (blocking) → tokio::sync::mpsc (async).
    //
    // We spawn a dedicated OS thread that blocks on the std::sync::mpsc
    // receiver and forwards events to a tokio::sync::mpsc sender that
    // the async task can select/await on.
    let (async_tx, mut async_rx) = tokio::sync::mpsc::channel::<MonitorEvent>(64);

    std::thread::Builder::new()
        .name("device-monitor-bridge".into())
        .spawn(move || {
            loop {
                match rx.recv() {
                    Ok(event) => {
                        if async_tx.blocking_send(event).is_err() {
                            break; // async side dropped
                        }
                    }
                    Err(mpsc::RecvError) => {
                        debug!("callback channel disconnected - bridge thread exiting");
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn device-monitor-bridge thread");
    // Process events from the bridge channel.
    while let Some(event) = async_rx.recv().await {
        match event {
            MonitorEvent::DeviceAdded(serial) => {
                info!(serial = %serial, "processing device added event");

                // Try to open the device and read its info.
                match open_and_read_device_info(&dll, manager_handle.0, &serial) {
                    Some(info) => {
                        let product_name = info.product_name.clone();
                        let interface_str = info.device_interface.to_string();

                        if device_manager.on_device_added(info) {
                            info!(
                                serial = %serial,
                                product = %product_name,
                                interface = %interface_str,
                                "FPGA device registered (hot-plug)"
                            );

                            // Push event to WebSocket.
                            let _ = event_tx.send(
                                serde_json::json!({
                                    "type": "device_connected",
                                    "serial": serial,
                                    "product_name": product_name,
                                    "interface": interface_str,
                                    "timestamp": chrono::Utc::now().to_rfc3339()
                                })
                                .to_string(),
                            );
                        }
                    }
                    None => {
                        warn!(
                            serial = %serial,
                            "could not read device info - device may be in test realm or unsupported"
                        );

                        // For okREALM_TEST, create a minimal DeviceInfo.
                        let info = DeviceInfo {
                            serial: serial.clone(),
                            product_name: "Unknown (emulated)".to_string(),
                            device_id: String::new(),
                            product_id: 0,
                            device_interface: DeviceInterface::Unknown,
                            usb_speed: UsbSpeed::Unknown,
                            device_major_version: 0,
                            device_minor_version: 0,
                            wire_width: 0,
                            trigger_width: 0,
                            pipe_width: 0,
                            register_address_width: 0,
                            register_data_width: 0,
                            fpga_vendor: FpgaVendor::Unknown,
                            dataport: DataportType::Legacy,
                        };

                        if device_manager.on_device_added(info) {
                            info!(serial = %serial, "emulated device registered");
                            let _ = event_tx.send(
                                serde_json::json!({
                                    "type": "device_connected",
                                    "serial": serial,
                                    "product_name": "Unknown (emulated)",
                                    "timestamp": chrono::Utc::now().to_rfc3339()
                                })
                                .to_string(),
                            );
                        }
                    }
                }
            }

            MonitorEvent::DeviceRemoved(serial) => {
                info!(serial = %serial, "processing device removed event");

                if device_manager.on_device_removed(&serial).is_some() {
                    info!(serial = %serial, "FPGA device unregistered (hot-unplug)");

                    let _ = event_tx.send(
                        serde_json::json!({
                            "type": "device_disconnected",
                            "serial": serial,
                            "timestamp": chrono::Utc::now().to_rfc3339()
                        })
                        .to_string(),
                    );
                } else {
                    debug!(serial = %serial, "device removed but was not in registry");
                }
            }
        }
    }

    info!("device event processor stopped");
}

// ============================================================================
// Device Info Reader
// ============================================================================

/// Open a device via the manager and read its `okTDeviceInfo`.
///
/// Returns `None` if the device cannot be opened (e.g., emulated test device)
/// or if it's a USB 2.0 device (filtered per ADR-002).
fn open_and_read_device_info(
    dll: &FrontPanelDll,
    manager_handle: okCFrontPanelManager_HANDLE,
    serial: &str,
) -> Option<DeviceInfo> {
    let serial_cstr = CString::new(serial).ok()?;

    // Open the device via the manager (not FrontPanelDevices — the manager
    // owns the device handle lifetime).
    let fp_handle =
        unsafe { (dll.manager_open)(manager_handle, serial_cstr.as_ptr()) };

    if fp_handle.is_null() {
        warn!(serial = %serial, "FrontPanelManager_Open returned null");
        return None;
    }

    // Read device info.
    let mut raw_info: okTDeviceInfo = unsafe { std::mem::zeroed() };
    let result = unsafe {
        (dll.fp_get_device_info_with_size)(
            fp_handle,
            &mut raw_info,
            std::mem::size_of::<okTDeviceInfo>() as u32,
        )
    };

    // Close the device handle — we don't keep it open.
    unsafe { (dll.fp_close)(fp_handle) };

    if result as i32 != ok_ErrorCode::ok_NoError as i32 {
        warn!(
            serial = %serial,
            error_code = result as i32,
            "GetDeviceInfoWithSize failed"
        );
        return None;
    }

    // Filter USB 2.0 devices.
    let interface = DeviceInterface::from_raw(raw_info.deviceInterface);
    if interface == DeviceInterface::Usb2 {
        info!(
            serial = %serial,
            "skipping USB 2.0 device (not supported)"
        );
        return None;
    }

    // Convert to safe Rust type.
    Some(DeviceInfo {
        serial: cstr_from_char_array(&raw_info.serialNumber),
        product_name: cstr_from_char_array(&raw_info.productName),
        device_id: cstr_from_char_array(&raw_info.deviceID),
        product_id: raw_info.productID,
        device_interface: interface,
        usb_speed: UsbSpeed::from_raw(raw_info.usbSpeed),
        device_major_version: raw_info.deviceMajorVersion,
        device_minor_version: raw_info.deviceMinorVersion,
        wire_width: raw_info.wireWidth,
        trigger_width: raw_info.triggerWidth,
        pipe_width: raw_info.pipeWidth,
        register_address_width: raw_info.registerAddressWidth,
        register_data_width: raw_info.registerDataWidth,
        fpga_vendor: FpgaVendor::from_raw(raw_info.fpgaVendor as i32),
        dataport: DataportType::from_raw(raw_info.dataport),
    })
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Resolve the path to the FrontPanel shared library.
///
/// Priority:
/// 1. `OKFPGA_DLL_PATH` environment variable
/// 2. Platform-specific default relative to CWD
fn resolve_dll_path() -> PathBuf {
    if let Ok(path) = std::env::var("OKFPGA_DLL_PATH") {
        return PathBuf::from(path);
    }

    let lib_name = if cfg!(windows) {
        "okFrontPanel.dll"
    } else {
        "libokFrontPanel.so"
    };

    Path::new("FrontPanel-6.1.0").join(lib_name)
}

/// Extract a Rust `String` from a C `char` array (null-terminated).
fn cstr_from_char_array(buf: &[::std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}
