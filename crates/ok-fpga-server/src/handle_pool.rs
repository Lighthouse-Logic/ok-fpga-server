//! # Device Handle Pool — Persistent Per-Device FFI Access (ADR-018)
//!
//! This module provides [`DeviceHandlePool`], which holds open FrontPanel device
//! handles for the lifetime of each device connection. REST and gRPC handlers
//! obtain handles from this pool to perform FFI operations (configure FPGA,
//! read wires, write registers, etc.).
//!
//! ## Why a Pool?
//!
//! The FrontPanel C API requires an `okFrontPanel_HANDLE` for every operation.
//! Opening/closing handles per-request would be expensive (USB enumeration +
//! open takes ~50ms) and would prevent maintaining state (e.g., programmed
//! bitfile, wire values). The pool keeps handles open for the lifetime of the
//! device connection.
//!
//! ## Thread Safety
//!
//! Each device handle is wrapped in a `tokio::sync::Mutex` to serialize FFI
//! calls. The FrontPanel C API is NOT thread-safe per-handle. Different devices
//! can be accessed concurrently — each has its own mutex.
//!
//! We use `tokio::sync::Mutex` instead of `std::sync::Mutex` because FFI calls
//! to USB devices may block for 10–100ms. `tokio::sync::Mutex::lock()` yields
//! the current task back to the runtime while waiting, preventing worker thread
//! starvation.

use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use ok_fpga_ffi::api::FrontPanelDll;
use ok_fpga_ffi::error::FfiError;
use ok_fpga_ffi::*;

/// State for a single open FrontPanel device.
///
/// Contains the main device handle and (after FPGA programming) cached
/// data port handles for Classic and AXI operations.
///
/// ## Lifetime
///
/// All data port handles are **borrowed** from the parent `okFrontPanel_HANDLE`.
/// They are invalidated when the FPGA is reprogrammed or the device disconnects.
/// The pool manages this by dropping and re-obtaining them as needed.
pub struct OpenDevice {
    /// The raw FrontPanel device handle.
    /// We manage this manually (not via FrontPanelHandle RAII) because we need
    /// fine-grained control over data port handle lifecycles.
    handle: okFrontPanel_HANDLE,

    /// Reference to the loaded DLL for making FFI calls.
    dll: Arc<FrontPanelDll>,

    /// Classic data port handle, obtained after ConfigureFPGA.
    /// `None` until the FPGA is programmed.
    classic_port: Option<okFPGADataPortClassic_HANDLE>,

    /// AXI data port handle, obtained after ConfigureFPGA.
    /// `None` until the FPGA is programmed.
    axi_port: Option<okFPGADataPortAXI_HANDLE>,

    /// AXI Lite sub-handle, obtained from the AXI data port.
    /// `None` until the FPGA is programmed and the AXI port is obtained.
    axi_lite: Option<okFrontPanelAXILite_HANDLE>,

    /// AXI Full sub-handle, obtained from the AXI data port.
    /// `None` until the FPGA is programmed and the AXI port is obtained.
    /// Counters.bit uses AXI Full (not AXI Lite) for register access.
    axi_full: Option<okFrontPanelAXIFull_HANDLE>,

    /// AXI Stream sub-handle, obtained from the AXI data port.
    /// `None` until the FPGA is programmed and the AXI port is obtained.
    axi_stream: Option<okFrontPanelAXIStream_HANDLE>,

    /// Whether the FPGA is currently programmed.
    pub is_programmed: bool,
}

// SAFETY: OpenDevice contains raw pointers (okFrontPanel_HANDLE, etc.) which
// are `!Send` by default. We guarantee safety by:
// 1. The pool wraps each OpenDevice in a `tokio::sync::Mutex`, ensuring only
//    one task accesses the raw pointers at a time.
// 2. The FrontPanel C API is single-threaded per-handle (not per-thread), so
//    moving the handle between threads is safe as long as access is serialized.
// 3. The raw pointers are opaque C handles — they don't reference thread-local
//    state and can be safely moved between threads.
unsafe impl Send for OpenDevice {}
unsafe impl Sync for OpenDevice {}

impl OpenDevice {
    /// Get the raw FrontPanel handle for device-level operations.
    pub fn raw(&self) -> okFrontPanel_HANDLE {
        self.handle
    }

    /// Get the DLL reference for calling FFI functions.
    pub fn dll(&self) -> &FrontPanelDll {
        &self.dll
    }

    /// Get the Classic data port handle (if FPGA is programmed).
    ///
    /// Returns `None` if the FPGA has not been configured with a bitfile
    /// that includes Classic endpoints (okWireIn, okWireOut, etc.).
    pub fn classic_port(&self) -> Option<okFPGADataPortClassic_HANDLE> {
        self.classic_port
    }

    /// Get the AXI Lite handle (if FPGA is programmed with okRegisterBridge).
    ///
    /// Returns `None` if the FPGA has not been configured or if the bitfile
    /// does not include an AXI register bridge.
    pub fn axi_lite(&self) -> Option<okFrontPanelAXILite_HANDLE> {
        self.axi_lite
    }

    /// Get the AXI Full handle (if FPGA is programmed with AXI Full interface).
    ///
    /// Counters.bit uses AXI Full for register-mapped access.
    pub fn axi_full(&self) -> Option<okFrontPanelAXIFull_HANDLE> {
        self.axi_full
    }

    /// Get the AXI Stream handle (if FPGA is programmed with AXI Stream).
    pub fn axi_stream(&self) -> Option<okFrontPanelAXIStream_HANDLE> {
        self.axi_stream
    }

    /// Obtain data port handles after FPGA configuration.
    ///
    /// Calls `GetFPGADataPortClassic` and `GetFPGADataPortAXI` to obtain
    /// the data port handles from the programmed FPGA. These handles are
    /// cached for subsequent endpoint operations.
    ///
    /// For the AXI data port, tries all three sub-interfaces:
    /// AXI Lite, AXI Full, and AXI Stream. The Counters.bit bitfile
    /// uses AXI Full (not AXI Lite).
    ///
    /// This is called internally after a successful `ConfigureFPGA`.
    fn acquire_data_ports(&mut self) {
        // Drop any existing data port handles (from previous programming).
        self.classic_port = None;
        self.axi_port = None;
        self.axi_lite = None;
        self.axi_full = None;
        self.axi_stream = None;

        // Obtain Classic data port.
        let mut classic_handle: okFPGADataPortClassic_HANDLE = std::ptr::null_mut();
        // SAFETY: The handle is valid and the FPGA was just programmed.
        // GetFPGADataPortClassic returns a borrowed handle (not owned).
        let rc = unsafe {
            (self.dll.fp_get_fpga_data_port_classic)(self.handle, &mut classic_handle)
        };
        if rc as i32 == ok_ErrorCode::ok_NoError as i32 && !classic_handle.is_null() {
            info!("Classic data port acquired");
            self.classic_port = Some(classic_handle);
        } else {
            info!(error_code = rc as i32, "Classic data port not available (this is OK for AXI-only bitfiles)");
        }

        // Obtain AXI data port.
        let mut axi_handle: okFPGADataPortAXI_HANDLE = std::ptr::null_mut();
        // SAFETY: Same as above — the handle is valid and FPGA is programmed.
        let rc = unsafe {
            (self.dll.fp_get_fpga_data_port_axi)(self.handle, &mut axi_handle)
        };
        if rc as i32 == ok_ErrorCode::ok_NoError as i32 && !axi_handle.is_null() {
            info!("AXI data port acquired");
            self.axi_port = Some(axi_handle);

            // Try to obtain all three AXI sub-handles.
            // The bitfile determines which are available.

            // AXI Lite
            let lite_handle = unsafe { (self.dll.axi_get_axi_lite)(axi_handle) };
            if !lite_handle.is_null() {
                info!("AXI Lite sub-handle acquired");
                self.axi_lite = Some(lite_handle);
            } else {
                info!("AXI Lite sub-handle is null (bitfile may not use AXI Lite)");
            }

            // AXI Full
            let full_handle = unsafe { (self.dll.axi_get_axi_full)(axi_handle) };
            if !full_handle.is_null() {
                info!("AXI Full sub-handle acquired");
                self.axi_full = Some(full_handle);
            } else {
                info!("AXI Full sub-handle is null (bitfile may not use AXI Full)");
            }

            // AXI Stream
            let stream_handle = unsafe { (self.dll.axi_get_axi_stream)(axi_handle) };
            if !stream_handle.is_null() {
                info!("AXI Stream sub-handle acquired");
                self.axi_stream = Some(stream_handle);
            } else {
                info!("AXI Stream sub-handle is null (bitfile may not use AXI Stream)");
            }
        } else {
            info!(error_code = rc as i32, "AXI data port not available (this is OK for Classic-only bitfiles)");
        }
    }
}

impl Drop for OpenDevice {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // Data port handles are borrowed from the FrontPanel handle
            // and do NOT have their own Destruct functions. They are
            // invalidated when the parent handle is destroyed.
            // We only need to destroy the main FrontPanel handle.
            //
            // SAFETY: We own this handle and are destroying it exactly once.
            unsafe {
                (self.dll.fp_destruct)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}

/// Pool of open FrontPanel device handles, keyed by serial number.
///
/// This is the bridge between the API layer and the FFI layer. It holds
/// one open device handle per connected FPGA, protected by a per-device
/// mutex to serialize FFI calls.
///
/// ## Usage
///
/// ```rust,ignore
/// // In a REST handler:
/// let device = state.handle_pool.get("22520013CC")
///     .ok_or(ProblemDetail::not_found("device not found"))?;
/// let guard = device.lock().await;
/// // Now you have exclusive access to the device FFI handle.
/// ```
pub struct DeviceHandlePool {
    /// The loaded FrontPanel shared library.
    /// Shared across all handles (function pointers are immutable).
    dll: Arc<FrontPanelDll>,

    /// Open device handles, keyed by serial number.
    /// Each device gets exactly ONE handle, wrapped in a tokio Mutex
    /// to serialize FFI calls.
    devices: RwLock<HashMap<String, Arc<Mutex<OpenDevice>>>>,
}

impl DeviceHandlePool {
    /// Create a new empty device handle pool with the given DLL reference.
    pub fn new(dll: Arc<FrontPanelDll>) -> Self {
        DeviceHandlePool {
            dll,
            devices: RwLock::new(HashMap::new()),
        }
    }

    /// Open a device by serial number and add it to the pool.
    ///
    /// This creates a new `okFrontPanel_HANDLE`, opens the device by serial,
    /// and stores it in the pool. If the device is already in the pool, this
    /// is a no-op.
    ///
    /// ## Errors
    ///
    /// Returns `Err` if the device cannot be opened (e.g., it disconnected
    /// between discovery and this call).
    pub async fn open(&self, serial: &str) -> Result<(), FfiError> {
        // Check if already open.
        {
            let devices = self.devices.read().await;
            if devices.contains_key(serial) {
                debug!(serial = %serial, "device already open in pool");
                return Ok(());
            }
        }

        // Perform all FFI calls synchronously (no raw pointers across await).
        let open_device = self.open_device_sync(serial)?;

        let mut devices = self.devices.write().await;
        devices.insert(serial.to_string(), Arc::new(Mutex::new(open_device)));
        info!(serial = %serial, "device handle opened and added to pool");

        Ok(())
    }

    /// Synchronous helper: opens a device by serial via the FrontPanelDevices
    /// snapshot API. All FFI calls happen here (no await points).
    fn open_device_sync(&self, serial: &str) -> Result<OpenDevice, FfiError> {
        let serial_c = CString::new(serial).map_err(|_| FfiError::InvalidUtf8 {
            context: format!("serial number '{}'", serial),
        })?;
        let realm = CString::new("").expect("empty CString is always valid");

        // SAFETY: All FFI calls below use validated function pointers from the
        // loaded DLL. We create a devices snapshot, open by serial, then destroy
        // the snapshot. The returned FrontPanel handle is owned by us.
        let opened_handle = unsafe {
            let mut err_ptr: *mut okError = std::ptr::null_mut();
            let devices_handle = (self.dll.devices_construct)(realm.as_ptr(), &mut err_ptr);

            if devices_handle.is_null() {
                if !err_ptr.is_null() {
                    let msg = std::ffi::CStr::from_ptr((self.dll.error_get_message)(err_ptr))
                        .to_string_lossy()
                        .to_string();
                    (self.dll.error_free)(err_ptr);
                    return Err(FfiError::FrontPanelError {
                        code: -1,
                        message: msg,
                    });
                }
                return Err(FfiError::NullHandle {
                    function: "okFrontPanelDevices_Construct".to_string(),
                });
            }

            let handle = (self.dll.devices_open)(devices_handle, serial_c.as_ptr());
            (self.dll.devices_destruct)(devices_handle);

            if handle.is_null() {
                return Err(FfiError::FrontPanelError {
                    code: -1,
                    message: format!("could not open device '{}'", serial),
                });
            }
            handle
        };

        Ok(OpenDevice {
            handle: opened_handle,
            dll: self.dll.clone(),
            classic_port: None,
            axi_port: None,
            axi_lite: None,
            axi_full: None,
            axi_stream: None,
            is_programmed: false,
        })
    }

    /// Close and remove a device handle from the pool.
    ///
    /// The handle is destroyed (via Drop on OpenDevice) when removed.
    pub async fn close(&self, serial: &str) {
        let mut devices = self.devices.write().await;
        if devices.remove(serial).is_some() {
            info!(serial = %serial, "device handle closed and removed from pool");
        }
    }

    /// Get a device handle by serial number.
    ///
    /// Returns `None` if the device is not in the pool (not connected or
    /// not yet opened).
    pub async fn get(&self, serial: &str) -> Option<Arc<Mutex<OpenDevice>>> {
        let devices = self.devices.read().await;
        devices.get(serial).cloned()
    }

    /// Configure an FPGA from a local bitfile path.
    ///
    /// Calls `okFrontPanel_ConfigureFPGA(handle, path)` and then acquires
    /// the data port handles (Classic + AXI).
    ///
    /// ## Errors
    ///
    /// Returns the FFI error if configuration fails (invalid bitfile, device
    /// error, etc.).
    pub async fn configure_from_path(
        &self,
        serial: &str,
        path: &Path,
    ) -> Result<(), FfiError> {
        let device_arc = self.get(serial).await.ok_or_else(|| FfiError::FrontPanelError {
            code: -1,
            message: format!("device '{}' not found in handle pool", serial),
        })?;

        let mut device = device_arc.lock().await;
        let path_c = CString::new(path.to_string_lossy().as_ref()).map_err(|_| {
            FfiError::InvalidUtf8 {
                context: format!("bitfile path '{}'", path.display()),
            }
        })?;

        // SAFETY: device.handle is valid (opened in pool.open()), path_c is
        // a valid null-terminated C string pointing to the bitfile.
        let rc = unsafe {
            (device.dll.fp_configure_fpga)(device.handle, path_c.as_ptr())
        };

        FfiError::from_code(rc)?;

        info!(serial = %serial, path = %path.display(), "FPGA configured successfully");
        device.is_programmed = true;
        device.acquire_data_ports();

        Ok(())
    }

    /// Configure an FPGA from bitfile data in memory.
    ///
    /// Calls `okFrontPanel_ConfigureFPGAFromMemory(handle, data, len)` and
    /// then acquires the data port handles.
    ///
    /// This is used for binary uploads (multipart/form-data).
    pub async fn configure_from_memory(
        &self,
        serial: &str,
        data: &[u8],
    ) -> Result<(), FfiError> {
        let device_arc = self.get(serial).await.ok_or_else(|| FfiError::FrontPanelError {
            code: -1,
            message: format!("device '{}' not found in handle pool", serial),
        })?;

        let mut device = device_arc.lock().await;

        // SAFETY: device.handle is valid, data is a valid byte slice.
        // ConfigureFPGAFromMemory reads `len` bytes from the pointer.
        let rc = unsafe {
            (device.dll.fp_configure_fpga_from_memory)(
                device.handle,
                data.as_ptr(),
                data.len() as std::os::raw::c_ulong,
            )
        };

        FfiError::from_code(rc)?;

        info!(serial = %serial, size_bytes = data.len(), "FPGA configured from memory successfully");
        device.is_programmed = true;
        device.acquire_data_ports();

        Ok(())
    }

    /// Get the DLL reference (needed by the scanner for device enumeration).
    pub fn dll(&self) -> &Arc<FrontPanelDll> {
        &self.dll
    }
}
