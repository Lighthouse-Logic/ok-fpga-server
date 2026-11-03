//! # Device Scanner — FrontPanel DLL → DeviceManager Bridge
//!
//! This module implements ADR-013: it loads the FrontPanel shared library at
//! server startup and performs a one-shot scan of all connected FPGA devices
//! using the `FrontPanelDevices` snapshot API (§15 in fp6-api-surface.md).
//!
//! ## How It Works
//!
//! 1. [`DeviceScanner::try_new`] loads the FrontPanel DLL from `OKFPGA_DLL_PATH`
//!    (or a platform-specific default). If the DLL is missing, it returns `None`
//!    instead of crashing — the server runs without hardware access.
//!
//! 2. [`DeviceScanner::scan_devices`] enumerates every connected USB/PCIe device,
//!    opens each one briefly to read `okTDeviceInfo`, converts it to the safe
//!    Rust [`DeviceInfo`] type, and filters out unsupported USB 2.0 boards.
//!
//! 3. The caller (in `main.rs`) feeds each `DeviceInfo` into
//!    `DeviceManager::on_device_added()` so the registry is populated.
//!
//! ## Future Work (ADR-002 Phase 2)
//!
//! This module will be extended (or replaced) with `FrontPanelManager`
//! callback-based hot-plug monitoring in Phase 15.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ok_fpga_core::device::{
    DataportType, DeviceInfo, DeviceInterface, FpgaVendor, UsbSpeed,
};
use ok_fpga_ffi::api::FrontPanelDll;
use ok_fpga_ffi::*;
use tracing::{debug, info, warn};

/// Scans for connected Opal Kelly FPGA devices via the FrontPanel DLL.
///
/// The scanner holds an `Arc<FrontPanelDll>` for the lifetime of the scan.
/// After `scan_devices()` returns, all device handles have been closed —
/// the scanner does NOT keep devices open.
pub struct DeviceScanner {
    /// The loaded FrontPanel shared library.
    dll: Arc<FrontPanelDll>,
}

impl DeviceScanner {
    /// Try to load the FrontPanel DLL and create a scanner.
    ///
    /// Returns `Some(scanner)` on success, or `None` if the DLL cannot be
    /// loaded (file missing, symbol mismatch, version mismatch, etc.).
    ///
    /// This design enables **graceful degradation** — the server can start
    /// and serve the management dashboard even without hardware access.
    ///
    /// ## DLL Path Resolution
    ///
    /// 1. `OKFPGA_DLL_PATH` environment variable (absolute or relative path)
    /// 2. Platform default:
    ///    - Windows: `FrontPanel-6.1.0/okFrontPanel.dll`
    ///    - Linux: `FrontPanel-6.1.0/libokFrontPanel.so`
    pub fn try_new() -> Option<Self> {
        let path = resolve_dll_path();
        info!(dll_path = %path.display(), "attempting to load FrontPanel DLL");

        match FrontPanelDll::load(&path) {
            Ok(dll) => {
                // Log the loaded API version for diagnostics.
                let version = dll.api_version_string();
                info!(
                    api_version = %version,
                    dll_path = %path.display(),
                    "FrontPanel DLL loaded successfully"
                );
                Some(DeviceScanner { dll: Arc::new(dll) })
            }
            Err(e) => {
                warn!(
                    error = %e,
                    dll_path = %path.display(),
                    "could not load FrontPanel DLL — running without hardware"
                );
                None
            }
        }
    }

    /// Returns a clone of the `Arc<FrontPanelDll>` held by this scanner.
    ///
    /// This allows the `DeviceHandlePool` to share the same loaded DLL
    /// instance rather than loading it a second time.
    pub fn dll(&self) -> Arc<FrontPanelDll> {
        self.dll.clone()
    }

    /// Enumerate all connected FPGA devices and return their info.
    ///
    /// This performs a **point-in-time snapshot** using the `FrontPanelDevices`
    /// C API (§15). For each device it:
    ///
    /// 1. Opens the device to get a `FrontPanel_HANDLE`
    /// 2. Reads `okTDeviceInfo` via `GetDeviceInfoWithSize`
    /// 3. Converts the C struct fields to the safe Rust `DeviceInfo` type
    /// 4. Filters through `is_supported_device()` (skips USB 2.0 boards)
    /// 5. Closes the device handle (we don't keep it open)
    ///
    /// Returns an empty `Vec` if no supported devices are found. Never panics.
    pub fn scan_devices(&self) -> Vec<DeviceInfo> {
        let mut devices = Vec::new();

        // SAFETY: We call the FrontPanel C API through our validated function
        // pointers. All handles are properly constructed and destroyed.
        unsafe {
            // Step 1: Create a FrontPanelDevices snapshot.
            // The empty realm string "" means okREALM_LOCAL (real hardware).
            let mut err_ptr: *mut okError = std::ptr::null_mut();
            let realm = CString::new("").expect("empty CString is always valid");
            let devices_handle =
                (self.dll.devices_construct)(realm.as_ptr(), &mut err_ptr);

            if devices_handle.is_null() {
                // The DLL loaded but device enumeration failed — this can
                // happen if no USB driver is installed or a permission error.
                let msg = if !err_ptr.is_null() {
                    let m = CStr::from_ptr((self.dll.error_get_message)(err_ptr))
                        .to_string_lossy()
                        .to_string();
                    (self.dll.error_free)(err_ptr);
                    m
                } else {
                    "unknown error".to_string()
                };
                warn!(error = %msg, "FrontPanelDevices construction failed");
                return devices;
            }

            // Step 2: Get the count of connected devices.
            let count = (self.dll.devices_get_count)(devices_handle);
            debug!(raw_device_count = count, "FrontPanelDevices snapshot taken");

            // Step 3: For each device, open it, read info, filter, close.
            for i in 0..count {
                // Read the serial number from the snapshot.
                let mut serial_buf = [0i8; 11]; // Max 10 chars + NUL
                (self.dll.devices_get_serial)(
                    devices_handle,
                    i,
                    serial_buf.as_mut_ptr(),
                );
                let serial = cstr_from_char_array(&serial_buf);

                if serial.is_empty() {
                    warn!(device_index = i, "skipping device with empty serial");
                    continue;
                }

                // Open the device by serial to read its full info.
                let serial_c = match CString::new(serial.clone()) {
                    Ok(s) => s,
                    Err(_) => {
                        warn!(serial = %serial, "invalid serial string");
                        continue;
                    }
                };
                let fp_handle =
                    (self.dll.devices_open)(devices_handle, serial_c.as_ptr());

                if fp_handle.is_null() {
                    #[cfg(target_os = "linux")]
                    warn!(
                        serial = %serial,
                        "could not open device — skipping (check /dev/bus/usb read/write \
                         permissions and /etc/udev/rules.d/60-opalkelly.rules for vendor 151f)"
                    );
                    #[cfg(not(target_os = "linux"))]
                    warn!(serial = %serial, "could not open device — skipping");
                    continue;
                }

                // Read okTDeviceInfo.
                match self.read_device_info(fp_handle, &serial) {
                    Some(info) => {
                        // Filter: only accept supported devices (skip USB 2.0).
                        if ok_fpga_core::device::is_supported_device(
                            info.product_id,
                            info.device_interface,
                        ) {
                            devices.push(info);
                        } else {
                            info!(
                                serial = %serial,
                                interface = %info.device_interface,
                                product_id = info.product_id,
                                "skipping unsupported device"
                            );
                        }
                    }
                    None => {
                        warn!(serial = %serial, "could not read device info");
                    }
                }

                // Close the device handle — we don't keep it open.
                // The server will re-open devices on demand when API
                // operations are requested.
                (self.dll.fp_destruct)(fp_handle);
            }

            // Clean up the snapshot handle.
            (self.dll.devices_destruct)(devices_handle);
        }

        devices
    }

    /// Enumerate the serial numbers of all physically connected devices.
    ///
    /// Unlike [`scan_devices`], this method does **not** open any devices.
    /// It only calls `FrontPanelDevices::GetCount()` and `GetSerial()` to
    /// list serial numbers. This is critical for the `device_poll_loop`:
    ///
    /// - `scan_devices()` tries to `devices_open()` each device to read
    ///   `okTDeviceInfo`. If the device is already opened exclusively by the
    ///   handle pool, `devices_open()` fails and the device is **not** returned.
    ///   This made disconnect detection impossible (see Phase 23 bug fix).
    ///
    /// - `scan_serials()` never opens any device, so it correctly reports
    ///   all physically connected devices regardless of handle pool state.
    ///
    /// Returns an empty `HashSet` if no devices are connected or the DLL
    /// fails to enumerate. Never panics.
    pub fn scan_serials(&self) -> std::collections::HashSet<String> {
        let mut serials = std::collections::HashSet::new();

        // SAFETY: We call the FrontPanel C API through our validated function
        // pointers. Only read-only operations (GetCount, GetSerial) are used.
        unsafe {
            // Create a FrontPanelDevices snapshot (okREALM_LOCAL).
            let mut err_ptr: *mut okError = std::ptr::null_mut();
            let realm = CString::new("").expect("empty CString is always valid");
            let devices_handle =
                (self.dll.devices_construct)(realm.as_ptr(), &mut err_ptr);

            if devices_handle.is_null() {
                if !err_ptr.is_null() {
                    let msg = CStr::from_ptr((self.dll.error_get_message)(err_ptr))
                        .to_string_lossy()
                        .to_string();
                    (self.dll.error_free)(err_ptr);
                    warn!(error = %msg, "FrontPanelDevices construction failed (scan_serials)");
                }
                return serials;
            }

            let count = (self.dll.devices_get_count)(devices_handle);

            for i in 0..count {
                let mut serial_buf = [0i8; 11]; // Max 10 chars + NUL
                (self.dll.devices_get_serial)(
                    devices_handle,
                    i,
                    serial_buf.as_mut_ptr(),
                );
                let serial = cstr_from_char_array(&serial_buf);
                if !serial.is_empty() {
                    serials.insert(serial);
                }
            }

            // Clean up the snapshot handle.
            (self.dll.devices_destruct)(devices_handle);
        }

        serials
    }

    /// Read `okTDeviceInfo` from an open device handle and convert it
    /// to the safe Rust `DeviceInfo` type.
    ///
    /// Returns `None` if the C API call fails.
    ///
    /// # Safety
    ///
    /// The caller must ensure `fp_handle` is a valid, open FrontPanel handle.
    unsafe fn read_device_info(
        &self,
        fp_handle: okFrontPanel_HANDLE,
        serial: &str,
    ) -> Option<DeviceInfo> {
        // Allocate a zeroed okTDeviceInfo on the stack.
        let mut raw = okTDeviceInfo::default();
        let size = std::mem::size_of::<okTDeviceInfo>() as u32;

        let rc = (self.dll.fp_get_device_info_with_size)(
            fp_handle, &mut raw, size,
        );

        if rc as i32 != ok_ErrorCode::ok_NoError as i32 {
            warn!(
                serial = %serial,
                error_code = rc as i32,
                "GetDeviceInfoWithSize failed"
            );
            return None;
        }

        // Convert C struct fields → safe Rust DeviceInfo.
        //
        // The C struct has fixed-size char arrays for strings (e.g.,
        // serialNumber[11], productName[128]). We extract them as Rust
        // Strings, trimming at the first NUL byte.
        let info = DeviceInfo {
            serial: cstr_from_char_array(&raw.serialNumber),
            product_name: cstr_from_char_array(&raw.productName),
            device_id: cstr_from_char_array(&raw.deviceID),
            product_id: raw.productID,
            device_interface: DeviceInterface::from_raw(
                raw.deviceInterface,
            ),
            usb_speed: UsbSpeed::from_raw(raw.usbSpeed),
            device_major_version: raw.deviceMajorVersion,
            device_minor_version: raw.deviceMinorVersion,
            wire_width: raw.wireWidth,
            trigger_width: raw.triggerWidth,
            pipe_width: raw.pipeWidth,
            register_address_width: raw.registerAddressWidth,
            register_data_width: raw.registerDataWidth,
            fpga_vendor: FpgaVendor::from_raw(raw.fpgaVendor as i32),
            dataport: DataportType::from_raw(raw.dataport),
        };

        Some(info)
    }
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

    // Default path: FrontPanel-6.1.0/<platform library name>
    let lib_name = if cfg!(windows) {
        "okFrontPanel.dll"
    } else {
        "libokFrontPanel.so"
    };

    Path::new("FrontPanel-6.1.0").join(lib_name)
}

/// Extract a Rust `String` from a C `char` array (null-terminated).
///
/// The C API uses fixed-size arrays like `char serialNumber[11]` where the
/// string is terminated by a NUL byte (`\0`). This function reads bytes
/// until the first NUL and converts them to a UTF-8 string.
///
/// Non-UTF-8 bytes are replaced with the Unicode replacement character (U+FFFD).
fn cstr_from_char_array(buf: &[::std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0) // Stop at first NUL byte.
        .map(|&c| c as u8) // c_char → u8 (safe because we stop at NUL).
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}
