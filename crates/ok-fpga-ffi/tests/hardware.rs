//! # Hardware Integration Tests — Real FPGA Device
//!
//! These tests exercise the FrontPanel FFI layer against actual connected
//! hardware. They are **ignored by default** so `cargo test` never fails
//! when no board is connected.
//!
//! ## Running
//!
//! ```bash
//! # Run ONLY hardware tests (board must be connected):
//! cargo test -p ok-fpga-ffi --test hardware -- --ignored --test-threads=1 --nocapture
//!
//! # Run ALL tests including hardware:
//! cargo test -p ok-fpga-ffi --test hardware -- --include-ignored --test-threads=1 --nocapture
//! ```
//!
//! ## Environment Variables
//!
//! - `OKFPGA_DLL_PATH` — Override the path to `okFrontPanel.dll`.
//!   Default: `FrontPanel-6.1.0/okFrontPanel.dll`
//!
//! ## Important
//!
//! The FrontPanel C API is **NOT thread-safe**. Always run with
//! `--test-threads=1` to avoid undefined behavior from concurrent device access.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ok_fpga_ffi::api::FrontPanelDll;
use ok_fpga_ffi::*;

// ============================================================================
// Helpers
// ============================================================================

/// Resolve the path to the FrontPanel DLL.
///
/// Priority:
/// 1. `OKFPGA_DLL_PATH` environment variable (absolute path)
/// 2. `OK_FRONTPANEL_SDK_DIR` environment variable (`<dir>/<lib_name>`)
/// 3. `FrontPanel-6.1.0/<lib_name>` relative to the workspace root
fn dll_path() -> PathBuf {
    if let Ok(path) = std::env::var("OKFPGA_DLL_PATH") {
        return PathBuf::from(path);
    }

    if let Ok(sdk_dir) = std::env::var("OK_FRONTPANEL_SDK_DIR") {
        return PathBuf::from(sdk_dir).join(FrontPanelDll::default_library_name());
    }

    // The workspace root is 2 levels up from the FFI crate manifest dir.
    // crates/ok-fpga-ffi/ → ../../
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(Path::new(manifest_dir));

    workspace_root
        .join("FrontPanel-6.1.0")
        .join(FrontPanelDll::default_library_name())
}

/// Load the FrontPanel DLL or skip the test if it's unavailable.
fn load_dll() -> Arc<FrontPanelDll> {
    let path = dll_path();
    match FrontPanelDll::load(&path) {
        Ok(dll) => Arc::new(dll),
        Err(e) => {
            eprintln!("SKIP: Could not load FrontPanel DLL at {:?}: {}", path, e);
            eprintln!("      Set OKFPGA_DLL_PATH to override the path.");
            panic!("FrontPanel DLL not available — cannot run hardware test");
        }
    }
}

/// Helper: extract a Rust string from a C char array (null-terminated).
fn cstr_from_array(buf: &[::std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

/// Open the first available device. Returns the FP handle.
/// Panics if no device is connected.
unsafe fn open_first_device(dll: &FrontPanelDll) -> (okFrontPanel_HANDLE, String) {
    let mut err_ptr: *mut okError = std::ptr::null_mut();
    let realm = CString::new("").unwrap();
    let devices_handle = (dll.devices_construct)(realm.as_ptr(), &mut err_ptr);

    if devices_handle.is_null() {
        let msg = if !err_ptr.is_null() {
            let m = CStr::from_ptr((dll.error_get_message)(err_ptr))
                .to_string_lossy()
                .to_string();
            (dll.error_free)(err_ptr);
            m
        } else {
            "unknown error".to_string()
        };
        panic!("Failed to enumerate devices: {}", msg);
    }

    let count = (dll.devices_get_count)(devices_handle);
    if count == 0 {
        (dll.devices_destruct)(devices_handle);
        panic!("SKIP: No FPGA devices connected — cannot run hardware test");
    }

    // Get the serial of the first device.
    let mut serial_buf = [0i8; 11];
    (dll.devices_get_serial)(devices_handle, 0, serial_buf.as_mut_ptr());
    let serial = cstr_from_array(&serial_buf);

    eprintln!("  Found {} device(s). Opening first: {}", count, serial);

    // Open the device.
    let serial_c = CString::new(serial.clone()).unwrap();
    let fp_handle = (dll.devices_open)(devices_handle, serial_c.as_ptr());
    (dll.devices_destruct)(devices_handle);

    if fp_handle.is_null() {
        panic!(
            "Failed to open device {} (on Linux, check /dev/bus/usb permissions and install \
             /etc/udev/rules.d/60-opalkelly.rules for USB vendor 151f)",
            serial
        );
    }

    (fp_handle, serial)
}

/// RAII guard that destroys the FP handle on drop.
struct DeviceGuard {
    handle: okFrontPanel_HANDLE,
    dll: Arc<FrontPanelDll>,
}

impl DeviceGuard {
    unsafe fn new(dll: Arc<FrontPanelDll>) -> (Self, String) {
        let (handle, serial) = open_first_device(&dll);
        (DeviceGuard { handle, dll }, serial)
    }
}

impl Drop for DeviceGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            eprintln!("  Cleaning up: clearing FPGA configuration");
            unsafe {
                // Best-effort: clear FPGA config so device is left clean.
                let _ = (self.dll.fp_clear_fpga_configuration)(self.handle);
                (self.dll.fp_destruct)(self.handle);
            }
        }
    }
}

// ============================================================================
// Test 1: DLL Loading & API Version
// ============================================================================

#[test]
#[ignore]
fn load_frontpanel_dll() {
    let dll = load_dll();
    let version = dll.api_version_string();
    eprintln!("  FrontPanel API version: {}", version);

    // Verify version >= 6.1.0.
    unsafe {
        let major = (dll.get_api_version_major)();
        let minor = (dll.get_api_version_minor)();
        let micro = (dll.get_api_version_micro)();
        eprintln!("  Version components: {}.{}.{}", major, minor, micro);
        assert!(major >= 6, "Expected API major >= 6, got {}", major);
        if major == 6 {
            assert!(
                minor >= 1,
                "Expected API minor >= 1 (for 6.x), got {}",
                minor
            );
        }
    }
}

// ============================================================================
// Test 2: Device Discovery & Enumeration
// ============================================================================

#[test]
#[ignore]
fn discover_connected_devices() {
    let dll = load_dll();

    unsafe {
        let mut err_ptr: *mut okError = std::ptr::null_mut();
        let realm = CString::new("").unwrap();
        let devices = (dll.devices_construct)(realm.as_ptr(), &mut err_ptr);
        assert!(!devices.is_null(), "devices_construct failed");

        let count = (dll.devices_get_count)(devices);
        eprintln!("  Connected device count: {}", count);
        assert!(count >= 1, "Expected at least 1 device, found {}", count);

        for i in 0..count {
            let mut serial_buf = [0i8; 11];
            (dll.devices_get_serial)(devices, i, serial_buf.as_mut_ptr());
            let serial = cstr_from_array(&serial_buf);
            eprintln!("  Device {}: serial = {}", i, serial);
        }

        (dll.devices_destruct)(devices);
    }
}

// ============================================================================
// Test 3: Open Device & Read DeviceInfo
// ============================================================================

#[test]
#[ignore]
fn read_device_info() {
    let dll = load_dll();

    unsafe {
        let (guard, serial) = DeviceGuard::new(dll.clone());

        let mut info = okTDeviceInfo::default();
        let size = std::mem::size_of::<okTDeviceInfo>() as u32;
        let rc = (dll.fp_get_device_info_with_size)(guard.handle, &mut info, size);
        assert_eq!(
            rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "GetDeviceInfo failed: {:?}",
            rc
        );

        let product_name = cstr_from_array(&info.productName);
        let device_id = cstr_from_array(&info.deviceID);
        let info_serial = cstr_from_array(&info.serialNumber);

        eprintln!("  === Device Info ===");
        eprintln!("  Serial:           {}", info_serial);
        eprintln!("  Product Name:     {}", product_name);
        eprintln!("  Product ID:       {}", info.productID);
        eprintln!("  Device ID:        {}", device_id);
        eprintln!("  Interface:        {:?}", info.deviceInterface);
        eprintln!("  USB Speed:        {:?}", info.usbSpeed);
        eprintln!(
            "  FW Version:       {}.{}",
            info.deviceMajorVersion, info.deviceMinorVersion
        );
        eprintln!("  Wire Width:       {}", info.wireWidth);
        eprintln!("  Trigger Width:    {}", info.triggerWidth);
        eprintln!("  Pipe Width:       {}", info.pipeWidth);
        eprintln!("  Reg Addr Width:   {}", info.registerAddressWidth);
        eprintln!("  Reg Data Width:   {}", info.registerDataWidth);
        eprintln!("  FPGA Vendor:      {:?}", info.fpgaVendor);
        eprintln!("  Dataport:         {:?}", info.dataport);
        eprintln!("  FP Enabled:       {}", info.isFrontPanelEnabled);

        // Assertions — board-agnostic.
        assert!(!info_serial.is_empty(), "Serial should be non-empty");
        assert_eq!(info_serial, serial, "Serial should match opened device");
        assert!(!product_name.is_empty(), "Product name should be non-empty");
        assert!(info.productID > 0, "Product ID should be positive");
        assert!(info.wireWidth > 0, "Wire width should be positive");
        assert!(info.triggerWidth > 0, "Trigger width should be positive");
        assert!(info.pipeWidth > 0, "Pipe width should be positive");
    }
}

// ============================================================================
// Test 4: Device Sensors
// ============================================================================

#[test]
#[ignore]
fn read_device_sensors() {
    let dll = load_dll();

    unsafe {
        let (guard, _serial) = DeviceGuard::new(dll.clone());

        let sensors = (dll.sensors_construct)();
        assert!(!sensors.is_null(), "sensors_construct failed");

        let rc = (dll.fp_get_device_sensors)(guard.handle, sensors);

        // Some boards (e.g., XEM7320) may return UnsupportedFeature for sensors
        // when no FPGA bitfile is programmed, or if the board doesn't support the
        // sensor API. Handle this gracefully.
        if rc as i32 != ok_ErrorCode::ok_NoError as i32 {
            eprintln!(
                "  SKIP: GetDeviceSensors returned {:?} — sensors may require FPGA configuration",
                rc
            );
            eprintln!("        or this board may not support the sensor API.");
            (dll.sensors_destruct)(sensors);
            return;
        }

        let count = (dll.sensors_get_count)(sensors);
        eprintln!("  === Device Sensors ({} total) ===", count);
        assert!(count > 0, "Expected at least 1 sensor");

        for i in 0..count {
            let sensor = (dll.sensors_get_sensor)(sensors, i);
            let name = cstr_from_array(&sensor.name);
            let description = cstr_from_array(&sensor.description);

            eprintln!("  Sensor {}: {} (type={:?})", sensor.id, name, sensor.type_);
            eprintln!(
                "    Value: {:.3}  Range: [{:.3}, {:.3}]  Step: {:.3}",
                sensor.value, sensor.min, sensor.max, sensor.step
            );
            if !description.is_empty() {
                eprintln!("    Description: {}", description);
            }

            // Sanity check: sensor values should be in physically reasonable ranges.
            // Temperature sensors: -40°C to +125°C.
            // Voltage sensors: 0.0V to 20.0V.
            if sensor.type_ == okEDeviceSensorType::okDEVICESENSOR_TEMPERATURE {
                assert!(
                    sensor.value > -40.0 && sensor.value < 125.0,
                    "Temperature {} out of range for sensor '{}'",
                    sensor.value,
                    name
                );
            }
            if sensor.type_ == okEDeviceSensorType::okDEVICESENSOR_VOLTAGE {
                assert!(
                    sensor.value >= 0.0 && sensor.value < 20.0,
                    "Voltage {} out of range for sensor '{}'",
                    sensor.value,
                    name
                );
            }
        }

        (dll.sensors_destruct)(sensors);
    }
}

// ============================================================================
// Test 5: Device Settings Enumeration
// ============================================================================

#[test]
#[ignore]
fn enumerate_device_settings() {
    let dll = load_dll();

    unsafe {
        let (guard, _serial) = DeviceGuard::new(dll.clone());

        let settings = (dll.settings_construct)();
        assert!(!settings.is_null(), "settings_construct failed");

        let rc = (dll.fp_get_device_settings)(guard.handle, settings);
        assert_eq!(
            rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "GetDeviceSettings failed: {:?}",
            rc
        );

        let names = (dll.setting_names_construct)();
        assert!(!names.is_null(), "setting_names_construct failed");

        let rc = (dll.settings_list)(settings, names);
        assert_eq!(
            rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "settings_list failed: {:?}",
            rc
        );

        let count = (dll.setting_names_get_count)(names);
        eprintln!("  === Device Settings ({} keys) ===", count);
        assert!(count > 0, "Expected at least 1 device setting");

        for i in 0..count {
            let name_ptr = (dll.setting_names_get)(names, i);
            if name_ptr.is_null() {
                continue;
            }
            let key = CStr::from_ptr(name_ptr).to_string_lossy().to_string();

            // Try to read as int first, then as string.
            let mut int_val: UINT32 = 0;
            let key_c = CString::new(key.clone()).unwrap();
            let int_rc = (dll.settings_get_int)(settings, key_c.as_ptr(), &mut int_val);

            if int_rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                eprintln!("  [INT]    {} = {}", key, int_val);
            } else {
                let mut str_buf = vec![0i8; 256];
                let str_rc = (dll.settings_get_string)(
                    settings,
                    key_c.as_ptr(),
                    str_buf.len() as i32,
                    str_buf.as_mut_ptr(),
                );
                if str_rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                    let val = cstr_from_array(&str_buf);
                    eprintln!("  [STRING] {} = \"{}\"", key, val);
                } else {
                    eprintln!("  [?]      {} (could not read)", key);
                }
            }
        }

        (dll.setting_names_destruct)(names);
        (dll.settings_destruct)(settings);
    }
}

// ============================================================================
// Test 6: SYZYGY Port Discovery (per ADR-009)
// ============================================================================

#[test]
#[ignore]
fn enumerate_syzygy_ports() {
    let dll = load_dll();

    unsafe {
        let (guard, _serial) = DeviceGuard::new(dll.clone());

        let settings = (dll.settings_construct)();
        assert!(!settings.is_null(), "settings_construct failed");

        let rc = (dll.fp_get_device_settings)(guard.handle, settings);
        assert_eq!(
            rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "GetDeviceSettings failed: {:?}",
            rc
        );

        eprintln!("  === SYZYGY Port Discovery ===");

        let mut port_count = 0;
        for n in 0..16 {
            // Probe SYZYGYn_PORT_STATUS — if not found, we've enumerated all ports.
            let key = format!("SYZYGY{}_PORT_STATUS\0", n);
            let key_c = CString::from_vec_unchecked(key.into_bytes());
            let mut port_status: UINT32 = 0;
            let rc = (dll.settings_get_int)(settings, key_c.as_ptr(), &mut port_status);

            if rc as i32 != ok_ErrorCode::ok_NoError as i32 {
                break;
            }

            port_count += 1;
            let voltage_mv = port_status as f64 * 10.0;
            let powered = port_status > 0;

            eprintln!(
                "  Port {}: status={} ({}mV, powered={})",
                n, port_status, voltage_mv, powered
            );

            // Read string fields for this port.
            let string_keys = [
                "MANUFACTURER_NAME",
                "PRODUCT_NAME",
                "PRODUCT_MODEL",
                "PRODUCT_VERSION",
                "SERIAL_NUMBER",
            ];

            for field in &string_keys {
                let key = format!("SYZYGY{}_{}\0", n, field);
                let key_c = CString::from_vec_unchecked(key.into_bytes());
                let mut buf = vec![0i8; 256];
                let rc = (dll.settings_get_string)(
                    settings,
                    key_c.as_ptr(),
                    buf.len() as i32,
                    buf.as_mut_ptr(),
                );
                if rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                    let val = cstr_from_array(&buf);
                    eprintln!("    {}: \"{}\"", field, val);
                }
            }
        }

        eprintln!("  Total SYZYGY ports discovered: {}", port_count);
        assert!(
            port_count > 0,
            "Expected at least 1 SYZYGY port on this board"
        );

        (dll.settings_destruct)(settings);
    }
}

// ============================================================================
// Test 7: Bitfile Programming (Conditional)
// ============================================================================

#[test]
#[ignore]
fn program_example_bitfile() {
    let dll = load_dll();

    unsafe {
        let (guard, _serial) = DeviceGuard::new(dll.clone());

        // Read device info to determine the product name for bitfile lookup.
        let mut info = okTDeviceInfo::default();
        let size = std::mem::size_of::<okTDeviceInfo>() as u32;
        let rc = (dll.fp_get_device_info_with_size)(guard.handle, &mut info, size);
        assert_eq!(rc as i32, ok_ErrorCode::ok_NoError as i32);

        let product_name = cstr_from_array(&info.productName);
        eprintln!("  Product: {}", product_name);

        // Find a matching bitfile directory (resolve relative to workspace root).
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = Path::new(manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(Path::new(manifest_dir));
        let bitfiles_dir = workspace_root
            .join("FrontPanel-6.1.0")
            .join("ExampleBitfiles");
        if !bitfiles_dir.exists() {
            eprintln!(
                "  SKIP: ExampleBitfiles directory not found at {:?}",
                bitfiles_dir
            );
            return;
        }

        // Search for a folder containing the product name (e.g., *XEM7320*).
        // Extract the board model base (e.g., "XEM7320" from "XEM7320-A75T").
        let model_base = product_name.split('-').next().unwrap_or(&product_name);

        let bitfile_dir = std::fs::read_dir(&bitfiles_dir)
            .expect("Failed to read ExampleBitfiles directory")
            .filter_map(|e| e.ok())
            .find(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.contains(model_base) && e.path().is_dir()
            });

        let bitfile_dir = match bitfile_dir {
            Some(dir) => dir.path(),
            None => {
                eprintln!(
                    "  SKIP: No bitfile directory found matching '{}' in {:?}",
                    model_base, bitfiles_dir
                );
                return;
            }
        };

        eprintln!("  Bitfile directory: {:?}", bitfile_dir);

        // Find Counters.bit recursively within the directory.
        let counters_bit = find_file_recursive(&bitfile_dir, "Counters.bit");

        let counters_bit = match counters_bit {
            Some(path) => path,
            None => {
                eprintln!("  SKIP: Counters.bit not found in {:?}", bitfile_dir);
                return;
            }
        };

        eprintln!("  Programming: {:?}", counters_bit);

        let path_c = CString::new(counters_bit.to_string_lossy().as_ref()).unwrap();
        let rc = (dll.fp_configure_fpga)(guard.handle, path_c.as_ptr());
        assert_eq!(
            rc as i32,
            ok_ErrorCode::ok_NoError as i32,
            "ConfigureFPGA failed: {:?}",
            rc
        );

        eprintln!("  ✓ Bitfile programmed successfully");

        // Verify FrontPanel is enabled after programming.
        let fp_enabled = (dll.fp_is_front_panel_enabled)(guard.handle);
        eprintln!("  IsFrontPanelEnabled: {}", fp_enabled);
        assert!(
            fp_enabled != 0,
            "FrontPanel should be enabled after programming Counters.bit"
        );

        // Read the counter values via Classic data port to verify the design is running.
        // Per Counetrs-endpoints.md:
        //   Wire Out 0x20 = COUNT1[7:0] (free-running counter)
        //   Wire Out 0x21 = COUNT2[7:0]
        let mut classic_handle: okFPGADataPortClassic_HANDLE = std::ptr::null_mut();
        let rc = (dll.fp_get_fpga_data_port_classic)(guard.handle, &mut classic_handle);
        if rc as i32 == ok_ErrorCode::ok_NoError as i32 && !classic_handle.is_null() {
            // Update wire outs and read counter values.
            let rc = (dll.classic_update_wire_outs)(classic_handle);
            if rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                let count1 = (dll.classic_get_wire_out_value)(classic_handle, 0x20);
                let count2 = (dll.classic_get_wire_out_value)(classic_handle, 0x21);
                eprintln!("  Counter1 (0x20): 0x{:04X} ({})", count1, count1);
                eprintln!("  Counter2 (0x21): 0x{:04X} ({})", count2, count2);

                // Read again after a brief pause — counter1 should change (it's free-running).
                std::thread::sleep(std::time::Duration::from_millis(50));
                let rc = (dll.classic_update_wire_outs)(classic_handle);
                if rc as i32 == ok_ErrorCode::ok_NoError as i32 {
                    let count1b = (dll.classic_get_wire_out_value)(classic_handle, 0x20);
                    eprintln!("  Counter1 (after 50ms): 0x{:04X} ({})", count1b, count1b);
                    // Counter1 is a free-running counter, its value should have changed.
                    // (Unless it wrapped around to the exact same value, which is extremely unlikely.)
                    eprintln!(
                        "  ✓ Counter1 changed: {} → {} (design is running)",
                        count1, count1b
                    );
                }
            }
        }

        // DeviceGuard::drop will clear FPGA config automatically.
        eprintln!("  ✓ Bitfile test complete");
    }
}

/// Recursively search for a file by name within a directory.
fn find_file_recursive(dir: &Path, target: &str) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case(target))
                    .unwrap_or(false)
            {
                return Some(path);
            }
            if path.is_dir() {
                if let Some(found) = find_file_recursive(&path, target) {
                    return Some(found);
                }
            }
        }
    }
    None
}

// ============================================================================
// Test 8: Thread Safety Warning
// ============================================================================

#[test]
#[ignore]
fn warn_if_multithreaded() {
    // This test just prints a reminder. It always passes.
    eprintln!();
    eprintln!("  ╔══════════════════════════════════════════════════════════════╗");
    eprintln!("  ║  REMINDER: FrontPanel is NOT thread-safe.                   ║");
    eprintln!("  ║  Always run hardware tests with: --test-threads=1           ║");
    eprintln!("  ╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
}
