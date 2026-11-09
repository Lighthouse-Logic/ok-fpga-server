// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # RAII Handle Wrappers for FrontPanel Opaque Handles
//!
//! Every FrontPanel C API handle (e.g., `okFrontPanel_HANDLE`) is an opaque
//! pointer that must be explicitly freed by calling a `*_Destruct` function.
//! Forgetting to call destruct causes a memory leak.
//!
//! This module wraps each handle type in a Rust struct that automatically
//! calls the correct `*_Destruct` function when the struct is dropped.
//! This is the **RAII (Resource Acquisition Is Initialization)** pattern —
//! the Rust equivalent of C++ destructors or Python context managers.
//!
//! ## Handle Ownership Model
//!
//! FrontPanel has two categories of handles:
//!
//! 1. **Owned handles** — Created with `*_Construct()`, destroyed with
//!    `*_Destruct()`. These include:
//!    - `okFrontPanel_HANDLE`
//!    - `okPLL22150_HANDLE`
//!    - `okPLL22393_HANDLE`
//!    - `okDeviceSensors_HANDLE`
//!    - `okDeviceSettings_HANDLE`
//!    - `okDeviceSettingNames_HANDLE`
//!    - `okCFrontPanelManager_HANDLE`
//!    - `okCFrontPanelDevices_HANDLE`
//!
//! 2. **Borrowed handles** — Obtained from an owned handle, NOT freed
//!    independently. The parent handle owns them. These include:
//!    - `okFPGADataPortClassic_HANDLE` (from `GetFPGADataPortClassic`)
//!    - `okFPGADataPortAXI_HANDLE` (from `GetFPGADataPortAXI`)
//!    - `okFrontPanelAXILite_HANDLE` (from `GetAXILite`)
//!    - `okFrontPanelAXIFull_HANDLE` (from `GetAXIFull`)
//!    - `okFrontPanelAXIStream_HANDLE` (from `GetAXIStream`)
//!
//! Borrowed handles are represented using raw handles with lifetime markers
//! (implemented in Phase 4 when the core layer wraps them).

use std::sync::Arc;

use crate::api::FrontPanelDll;
use crate::*;

// ============================================================================
// FrontPanelHandle — The Main Device Handle
// ============================================================================

/// RAII wrapper around `okFrontPanel_HANDLE`.
///
/// This is the most important handle — it represents a connection to a
/// FrontPanel device. When dropped, it automatically calls
/// `okFrontPanel_Destruct` to release the underlying C handle and any
/// associated resources.
///
/// ## Example
///
/// ```ignore
/// let dll = Arc::new(FrontPanelDll::load(path)?);
/// let handle = FrontPanelHandle::new(dll.clone());
/// // ... use handle ...
/// // When `handle` goes out of scope, okFrontPanel_Destruct is called
/// // automatically.
/// ```
///
/// ## Thread Safety
///
/// `FrontPanelHandle` is `Send` (can be moved between threads) but NOT
/// `Sync` (cannot be shared between threads). The underlying C API is
/// not thread-safe.
pub struct FrontPanelHandle {
    /// The raw C handle. This is `null` after `Drop` runs.
    handle: okFrontPanel_HANDLE,
    /// Reference to the loaded DLL so we can call `fp_destruct`.
    /// We use `Arc` because multiple handles may reference the same DLL.
    dll: Arc<FrontPanelDll>,
}

impl FrontPanelHandle {
    /// Create a new FrontPanel device handle.
    ///
    /// This calls `okFrontPanel_Construct()` in the loaded DLL to allocate
    /// a new device handle. The handle must be opened (via
    /// `FrontPanelDevicesHandle::open`) before it can communicate with hardware.
    ///
    /// ## Panics
    ///
    /// Panics if the C API returns a null handle (should never happen with
    /// a correctly loaded DLL).
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        // SAFETY: `fp_construct` returns a new opaque handle allocated by the
        // FrontPanel DLL. The DLL is loaded and the function pointer is valid
        // (verified during FrontPanelDll::load). The returned handle is non-null
        // on success.
        let handle = unsafe { (dll.fp_construct)() };
        assert!(
            !handle.is_null(),
            "okFrontPanel_Construct returned null — this should never happen"
        );
        FrontPanelHandle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    ///
    /// ## Safety Contract
    ///
    /// The returned handle is only valid as long as this `FrontPanelHandle`
    /// is alive. Do not store the raw handle beyond the lifetime of this
    /// wrapper.
    pub fn raw(&self) -> okFrontPanel_HANDLE {
        self.handle
    }
}

impl Drop for FrontPanelHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle (created in `new`), and we're
            // destroying it exactly once. After this call, the handle is
            // invalid and must not be used again. Setting it to null
            // prevents double-free if Drop is somehow called twice
            // (which Rust guarantees won't happen, but defense-in-depth).
            unsafe {
                (self.dll.fp_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// PLL22150Handle
// ============================================================================

/// RAII wrapper around `okPLL22150_HANDLE`.
///
/// Represents a PLL22150 clock synthesizer configuration. When dropped,
/// automatically calls `okPLL22150_Destruct`.
pub struct PLL22150Handle {
    handle: okPLL22150_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl PLL22150Handle {
    /// Create a new PLL22150 configuration handle.
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        // SAFETY: `pll22150_construct` allocates a new PLL handle.
        let handle = unsafe { (dll.pll22150_construct)() };
        assert!(!handle.is_null(), "okPLL22150_Construct returned null");
        PLL22150Handle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okPLL22150_HANDLE {
        self.handle
    }
}

impl Drop for PLL22150Handle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.pll22150_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// PLL22393Handle
// ============================================================================

/// RAII wrapper around `okPLL22393_HANDLE`.
///
/// Represents a PLL22393 clock synthesizer configuration. When dropped,
/// automatically calls `okPLL22393_Destruct`.
pub struct PLL22393Handle {
    handle: okPLL22393_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl PLL22393Handle {
    /// Create a new PLL22393 configuration handle.
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        // SAFETY: `pll22393_construct` allocates a new PLL handle.
        let handle = unsafe { (dll.pll22393_construct)() };
        assert!(!handle.is_null(), "okPLL22393_Construct returned null");
        PLL22393Handle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okPLL22393_HANDLE {
        self.handle
    }
}

impl Drop for PLL22393Handle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.pll22393_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// DeviceSensorsHandle
// ============================================================================

/// RAII wrapper around `okDeviceSensors_HANDLE`.
///
/// Used to read sensor data (temperature, voltage, current, etc.) from
/// a FrontPanel device. When dropped, calls `okDeviceSensors_Destruct`.
pub struct DeviceSensorsHandle {
    handle: okDeviceSensors_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl DeviceSensorsHandle {
    /// Create a new sensors handle.
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        // SAFETY: `sensors_construct` allocates a new sensors container.
        let handle = unsafe { (dll.sensors_construct)() };
        assert!(!handle.is_null(), "okDeviceSensors_Construct returned null");
        DeviceSensorsHandle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okDeviceSensors_HANDLE {
        self.handle
    }
}

impl Drop for DeviceSensorsHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.sensors_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// DeviceSettingsHandle
// ============================================================================

/// RAII wrapper around `okDeviceSettings_HANDLE`.
///
/// Used to read and write persistent device settings (key-value pairs
/// stored on the device). When dropped, calls `okDeviceSettings_Destruct`.
pub struct DeviceSettingsHandle {
    handle: okDeviceSettings_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl DeviceSettingsHandle {
    /// Create a new settings handle.
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        // SAFETY: `settings_construct` allocates a new settings container.
        let handle = unsafe { (dll.settings_construct)() };
        assert!(
            !handle.is_null(),
            "okDeviceSettings_Construct returned null"
        );
        DeviceSettingsHandle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okDeviceSettings_HANDLE {
        self.handle
    }
}

impl Drop for DeviceSettingsHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.settings_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// DeviceSettingNamesHandle
// ============================================================================

/// RAII wrapper around `okDeviceSettingNames_HANDLE`.
///
/// Used to enumerate setting key names on a device. When dropped, calls
/// `okDeviceSettingNames_Destruct`.
pub struct DeviceSettingNamesHandle {
    handle: okDeviceSettingNames_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl DeviceSettingNamesHandle {
    /// Create a new setting names handle.
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        // SAFETY: `setting_names_construct` allocates a new names container.
        let handle = unsafe { (dll.setting_names_construct)() };
        assert!(
            !handle.is_null(),
            "okDeviceSettingNames_Construct returned null"
        );
        DeviceSettingNamesHandle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okDeviceSettingNames_HANDLE {
        self.handle
    }
}

impl Drop for DeviceSettingNamesHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.setting_names_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// FrontPanelManagerHandle
// ============================================================================

/// RAII wrapper around `okCFrontPanelManager_HANDLE`.
///
/// The FrontPanelManager handles hot-plug detection — it monitors for
/// devices being connected and disconnected. When dropped, calls
/// `okFrontPanelManager_Destruct`.
///
/// ## Usage Notes
///
/// The manager requires a "realm" string at construction time:
/// - `""` (empty string) — real hardware detection
/// - `"test"` — emulated test device environment (`okREALM_TEST`)
pub struct FrontPanelManagerHandle {
    handle: okCFrontPanelManager_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl FrontPanelManagerHandle {
    /// Create a new FrontPanel manager for device hot-plug monitoring.
    ///
    /// ## Arguments
    ///
    /// * `dll` — The loaded FrontPanel DLL.
    /// * `realm` — The realm string. Use `""` for real hardware or
    ///   `"test"` for the emulated test environment.
    pub fn new(dll: Arc<FrontPanelDll>, realm: &std::ffi::CStr) -> Self {
        // SAFETY: `manager_construct` takes a null self-handle (for the C++
        // callback pattern) and a realm string. We pass null for the self
        // handle because we don't use the C++ callback mechanism.
        let handle = unsafe { (dll.manager_construct)(std::ptr::null_mut(), realm.as_ptr()) };
        assert!(
            !handle.is_null(),
            "okFrontPanelManager_Construct returned null"
        );
        FrontPanelManagerHandle { handle, dll }
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okCFrontPanelManager_HANDLE {
        self.handle
    }
}

impl Drop for FrontPanelManagerHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.manager_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

// ============================================================================
// FrontPanelDevicesHandle
// ============================================================================

/// RAII wrapper around `okCFrontPanelDevices_HANDLE`.
///
/// Represents a snapshot of currently connected FrontPanel devices.
/// Use this to enumerate devices and open them by serial number.
/// When dropped, calls `okFrontPanelDevices_Destruct`.
///
/// ## Example
///
/// ```ignore
/// let devices = FrontPanelDevicesHandle::new(dll.clone(), c"test")?;
/// let count = devices.count();
/// for i in 0..count {
///     let serial = devices.serial(i);
///     println!("Device {}: {}", i, serial);
/// }
/// ```
pub struct FrontPanelDevicesHandle {
    handle: okCFrontPanelDevices_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl FrontPanelDevicesHandle {
    /// Create a new device enumeration snapshot.
    ///
    /// ## Arguments
    ///
    /// * `dll` — The loaded FrontPanel DLL.
    /// * `realm` — The realm string. Use `""` for real hardware or
    ///   `"test"` for the emulated test environment.
    ///
    /// ## Errors
    ///
    /// Returns `FfiError` if the enumeration fails (e.g., permission denied).
    pub fn new(
        dll: Arc<FrontPanelDll>,
        realm: &std::ffi::CStr,
    ) -> Result<Self, crate::error::FfiError> {
        let mut err_ptr: *mut okError = std::ptr::null_mut();

        // SAFETY: `devices_construct` creates a snapshot of connected devices.
        // It takes a realm string and an error pointer. If an error occurs,
        // the error pointer is set and must be freed.
        let handle = unsafe { (dll.devices_construct)(realm.as_ptr(), &mut err_ptr) };

        // Check if an error occurred during construction.
        if !err_ptr.is_null() {
            // SAFETY: The error pointer was set by the DLL and must be
            // read then freed to avoid a memory leak.
            let message = unsafe {
                let msg_ptr = (dll.error_get_message)(err_ptr);
                let msg = if msg_ptr.is_null() {
                    "Unknown error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(msg_ptr)
                        .to_string_lossy()
                        .into_owned()
                };
                (dll.error_free)(err_ptr);
                msg
            };

            return Err(crate::error::FfiError::FrontPanelError { code: -1, message });
        }

        if handle.is_null() {
            return Err(crate::error::FfiError::NullHandle {
                function: "okFrontPanelDevices_Construct".to_string(),
            });
        }

        Ok(FrontPanelDevicesHandle { handle, dll })
    }

    /// Returns the raw C handle for use in C API calls.
    pub fn raw(&self) -> okCFrontPanelDevices_HANDLE {
        self.handle
    }

    /// Returns the number of devices in this snapshot.
    pub fn count(&self) -> i32 {
        // SAFETY: The handle is valid (created in `new`, not yet dropped).
        unsafe { (self.dll.devices_get_count)(self.handle) }
    }

    /// Returns the serial number of the device at index `n`.
    ///
    /// ## Panics
    ///
    /// Panics if `n >= count()`.
    pub fn serial(&self, n: i32) -> String {
        assert!(
            n < self.count(),
            "Device index {} out of range (count={})",
            n,
            self.count()
        );

        // Serial numbers are at most 10 characters + null terminator.
        let mut buf = [0i8; 16];

        // SAFETY: The handle is valid, `n` is in range, and the buffer
        // is large enough to hold any serial number.
        unsafe {
            (self.dll.devices_get_serial)(self.handle, n, buf.as_mut_ptr());
        }

        // Convert the C string to a Rust String.
        // SAFETY: The buffer was written by the DLL and is null-terminated.
        unsafe {
            std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}

impl Drop for FrontPanelDevicesHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.devices_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}
