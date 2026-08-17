//! # Runtime DLL Loading — FrontPanel API Dispatch Table
//!
//! This module loads the FrontPanel shared library at runtime using
//! [`libloading`](https://docs.rs/libloading) and resolves all ~160 C API
//! function symbols into a single [`FrontPanelDll`](crate::api::FrontPanelDll) struct.
//!
//! ## Why Runtime Loading?
//!
//! We use `libloading` (runtime binding) rather than linking at compile time
//! because:
//!
//! 1. **Graceful degradation**: The server can start even when the FrontPanel
//!    DLL is missing, reporting "DLL not found" via its API.
//! 2. **Cross-platform**: We load `okFrontPanel.dll` on Windows and
//!    `libokFrontPanel.so` on Linux without conditional compilation.
//! 3. **No import library needed**: The Rust toolchain doesn't need the
//!    `.lib` file that MSVC would require for link-time binding.
//!
//! ## How It Works
//!
//! 1. [`FrontPanelDll::load`](crate::api::FrontPanelDll::load) opens the shared library from a given path.
//! 2. It resolves every exported C function symbol using `Library::get()`.
//! 3. Each resolved symbol is stored as a function pointer in the struct.
//! 4. Caller code invokes methods on `FrontPanelDll` which delegate to
//!    the stored function pointers via `unsafe` blocks.
//!
//! ## Safety
//!
//! All `unsafe` code is concentrated in this module. The function pointers
//! stored in `FrontPanelDll` are `unsafe extern "C" fn(...)` — calling them
//! is unsafe because they cross the FFI boundary. The safe wrapper methods
//! on `FrontPanelDll` encapsulate this unsafety.

use std::path::Path;

use libloading::Library;

use crate::error::{FfiError, LoadError};
use crate::*;

// ============================================================================
// Macro for declaring and loading function pointer fields
// ============================================================================

/// This macro generates:
/// 1. A struct field for each function pointer
/// 2. A symbol-loading line in the `load` method
///
/// Usage: `load_symbol!(lib, field_name, "CSymbolName", fn_type)`
///
/// The macro wraps the `Library::get` call and converts errors into
/// `LoadError::SymbolNotFound` with a clear message.
macro_rules! load_symbol {
    ($lib:expr, $name:expr) => {{
        // SAFETY: We trust that the FrontPanel DLL exports this symbol
        // with the correct signature. The C header (okFrontPanel.h) defines
        // the function signature, and bindgen verified it at build time.
        // The raw symbol is cast to the correct function pointer type by
        // the caller.
        $lib.get($name.as_bytes())
            .map_err(|e| LoadError::SymbolNotFound {
                symbol: $name.to_string(),
                message: e.to_string(),
            })
    }};
}

// ============================================================================
// FrontPanelDll — The Runtime Dispatch Table
// ============================================================================

/// Holds the loaded FrontPanel shared library and all resolved function pointers.
///
/// This struct is the single point of contact between Rust and the FrontPanel
/// C API. Every C function we need is stored as a function pointer field.
///
/// ## Lifetime
///
/// The `Library` must outlive all function pointer calls. Since the function
/// pointers are stored alongside the `Library` in the same struct, this is
/// guaranteed — when `FrontPanelDll` is dropped, the library is unloaded.
///
/// ## Thread Safety
///
/// `FrontPanelDll` is `Send` and `Sync`. All fields are function pointers
/// (immutable addresses in the DLL's code segment) and a `Library` handle.
/// The DLL functions themselves may not be thread-safe — callers must
/// serialize access (the `ok-fpga-core` layer handles this via `Mutex`).
pub struct FrontPanelDll {
    /// The loaded shared library handle. This must be kept alive for as
    /// long as any function pointers are in use. When this is dropped,
    /// the OS unloads the DLL and all function pointers become invalid.
    #[allow(dead_code)]
    library: Library,

    // ---- Version & Library (§1) ----
    pub get_api_version_major: unsafe extern "C" fn() -> ::std::os::raw::c_int,
    pub get_api_version_minor: unsafe extern "C" fn() -> ::std::os::raw::c_int,
    pub get_api_version_micro: unsafe extern "C" fn() -> ::std::os::raw::c_int,
    pub get_api_version_string: unsafe extern "C" fn() -> *const ::std::os::raw::c_char,
    pub check_api_version: unsafe extern "C" fn(
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
    ) -> Bool,

    // ---- Error Handling (§2) ----
    pub get_error_message: unsafe extern "C" fn(ok_ErrorCode) -> *const ::std::os::raw::c_char,
    pub get_last_error: unsafe extern "C" fn(okFrontPanel_HANDLE) -> ok_ErrorCode,
    pub get_last_error_message:
        unsafe extern "C" fn(okFrontPanel_HANDLE) -> *const ::std::os::raw::c_char,

    // ---- Device Core (§3) ----
    pub fp_construct: unsafe extern "C" fn() -> okFrontPanel_HANDLE,
    pub fp_destruct: unsafe extern "C" fn(okFrontPanel_HANDLE),
    pub fp_is_open: unsafe extern "C" fn(okFrontPanel_HANDLE) -> Bool,
    pub fp_is_remote: unsafe extern "C" fn(okFrontPanel_HANDLE) -> Bool,
    pub fp_close: unsafe extern "C" fn(okFrontPanel_HANDLE),
    pub fp_set_timeout: unsafe extern "C" fn(okFrontPanel_HANDLE, ::std::os::raw::c_int),
    pub fp_set_device_id:
        unsafe extern "C" fn(okFrontPanel_HANDLE, *const ::std::os::raw::c_char) -> ok_ErrorCode,
    pub fp_get_device_info_with_size: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        *mut okTDeviceInfo,
        ::std::os::raw::c_uint,
    ) -> ok_ErrorCode,
    pub fp_is_front_panel_enabled: unsafe extern "C" fn(okFrontPanel_HANDLE) -> Bool,
    pub fp_get_last_transfer_length:
        unsafe extern "C" fn(okFrontPanel_HANDLE) -> ::std::os::raw::c_long,

    // ---- FPGA Configuration (§4) ----
    pub fp_configure_fpga:
        unsafe extern "C" fn(okFrontPanel_HANDLE, *const ::std::os::raw::c_char) -> ok_ErrorCode,
    pub fp_configure_fpga_with_reset: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        *const ::std::os::raw::c_char,
        *const okTFPGAResetProfile,
    ) -> ok_ErrorCode,
    pub fp_configure_fpga_from_memory: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        *const ::std::os::raw::c_uchar,
        ::std::os::raw::c_ulong,
    ) -> ok_ErrorCode,
    pub fp_configure_fpga_from_memory_with_progress: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        *const ::std::os::raw::c_uchar,
        ::std::os::raw::c_ulong,
        okTProgressCallback,
        *mut ::std::os::raw::c_void,
    ) -> ok_ErrorCode,
    pub fp_configure_fpga_from_memory_with_reset: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        *const ::std::os::raw::c_uchar,
        ::std::os::raw::c_ulong,
        *const okTFPGAResetProfile,
    ) -> ok_ErrorCode,
    pub fp_configure_fpga_from_flash:
        unsafe extern "C" fn(okFrontPanel_HANDLE, ::std::os::raw::c_ulong) -> ok_ErrorCode,
    pub fp_clear_fpga_configuration: unsafe extern "C" fn(okFrontPanel_HANDLE) -> ok_ErrorCode,

    // ---- Reset Profile (§5) ----
    pub fp_get_fpga_reset_profile: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        ok_FPGAConfigurationMethod,
        *mut okTFPGAResetProfile,
    ) -> ok_ErrorCode,
    pub fp_set_fpga_reset_profile: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        ok_FPGAConfigurationMethod,
        *const okTFPGAResetProfile,
    ) -> ok_ErrorCode,

    // ---- Data Port Acquisition (§6) ----
    pub fp_get_fpga_data_port_classic: unsafe extern "C" fn(
        okFrontPanel_HANDLE,
        *mut okFPGADataPortClassic_HANDLE,
    ) -> ok_ErrorCode,
    pub fp_get_fpga_data_port_axi:
        unsafe extern "C" fn(okFrontPanel_HANDLE, *mut okFPGADataPortAXI_HANDLE) -> ok_ErrorCode,

    // ---- Classic Data Port (§7) — 19 functions ----
    pub classic_reset_fpga: unsafe extern "C" fn(okFPGADataPortClassic_HANDLE) -> ok_ErrorCode,
    pub classic_set_wire_in_value: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_ulong,
        ::std::os::raw::c_ulong,
    ) -> ok_ErrorCode,
    pub classic_get_wire_in_value: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        *mut UINT32,
    ) -> ok_ErrorCode,
    pub classic_update_wire_ins: unsafe extern "C" fn(okFPGADataPortClassic_HANDLE) -> ok_ErrorCode,
    pub classic_get_wire_out_value: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_ulong,
    pub classic_update_wire_outs:
        unsafe extern "C" fn(okFPGADataPortClassic_HANDLE) -> ok_ErrorCode,
    pub classic_activate_trigger_in: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
    ) -> ok_ErrorCode,
    pub classic_update_trigger_outs:
        unsafe extern "C" fn(okFPGADataPortClassic_HANDLE) -> ok_ErrorCode,
    pub classic_is_triggered: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_ulong,
    ) -> Bool,
    pub classic_get_trigger_out_vector:
        unsafe extern "C" fn(okFPGADataPortClassic_HANDLE, ::std::os::raw::c_int) -> UINT32,
    pub classic_write_to_pipe_in: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_long,
        *const ::std::os::raw::c_uchar,
    ) -> ::std::os::raw::c_long,
    pub classic_read_from_pipe_out: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_long,
        *mut ::std::os::raw::c_uchar,
    ) -> ::std::os::raw::c_long,
    pub classic_write_to_block_pipe_in: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
        ::std::os::raw::c_long,
        *const ::std::os::raw::c_uchar,
    ) -> ::std::os::raw::c_long,
    pub classic_read_from_block_pipe_out: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
        ::std::os::raw::c_long,
        *mut ::std::os::raw::c_uchar,
    ) -> ::std::os::raw::c_long,
    pub classic_set_bt_pipe_polling_interval:
        unsafe extern "C" fn(okFPGADataPortClassic_HANDLE, ::std::os::raw::c_int) -> ok_ErrorCode,
    pub classic_read_register:
        unsafe extern "C" fn(okFPGADataPortClassic_HANDLE, UINT32, *mut UINT32) -> ok_ErrorCode,
    pub classic_write_register:
        unsafe extern "C" fn(okFPGADataPortClassic_HANDLE, UINT32, UINT32) -> ok_ErrorCode,
    pub classic_read_registers: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_uint,
        *mut okTRegisterEntry,
    ) -> ok_ErrorCode,
    pub classic_write_registers: unsafe extern "C" fn(
        okFPGADataPortClassic_HANDLE,
        ::std::os::raw::c_uint,
        *const okTRegisterEntry,
    ) -> ok_ErrorCode,

    // ---- AXI Data Port Root (§8) ----
    pub axi_get_axi_lite:
        unsafe extern "C" fn(okFPGADataPortAXI_HANDLE) -> okFrontPanelAXILite_HANDLE,
    pub axi_get_axi_full:
        unsafe extern "C" fn(okFPGADataPortAXI_HANDLE) -> okFrontPanelAXIFull_HANDLE,
    pub axi_get_axi_stream:
        unsafe extern "C" fn(okFPGADataPortAXI_HANDLE) -> okFrontPanelAXIStream_HANDLE,

    // ---- AXI Lite (§8) ----
    pub axi_lite_read:
        unsafe extern "C" fn(okFrontPanelAXILite_HANDLE, UINT64, *mut UINT32) -> ok_ErrorCode,
    pub axi_lite_write:
        unsafe extern "C" fn(okFrontPanelAXILite_HANDLE, UINT64, UINT32) -> ok_ErrorCode,
    pub axi_lite_read_bulk: unsafe extern "C" fn(
        okFrontPanelAXILite_HANDLE,
        UINT64,
        *mut UINT32,
        UINT64,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_lite_write_bulk: unsafe extern "C" fn(
        okFrontPanelAXILite_HANDLE,
        UINT64,
        *const UINT32,
        UINT64,
        *mut okTOperationStatistics,
    ) -> ok_ErrorCode,
    pub axi_lite_read_ex: unsafe extern "C" fn(
        okFrontPanelAXILite_HANDLE,
        UINT64,
        *mut UINT32,
        UINT32,
    ) -> ok_ErrorCode,
    pub axi_lite_write_ex:
        unsafe extern "C" fn(okFrontPanelAXILite_HANDLE, UINT64, UINT32, UINT32) -> ok_ErrorCode,
    pub axi_lite_read_bulk_ex: unsafe extern "C" fn(
        okFrontPanelAXILite_HANDLE,
        UINT64,
        *mut UINT32,
        UINT64,
        UINT32,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_lite_write_bulk_ex: unsafe extern "C" fn(
        okFrontPanelAXILite_HANDLE,
        UINT64,
        *const UINT32,
        UINT64,
        UINT32,
        *mut okTOperationStatistics,
    ) -> ok_ErrorCode,
    pub axi_lite_reset: unsafe extern "C" fn(okFrontPanelAXILite_HANDLE) -> ok_ErrorCode,

    // ---- AXI Full (§8) ----
    pub axi_full_get_datapath_width: unsafe extern "C" fn(okFrontPanelAXIFull_HANDLE) -> UINT8,
    pub axi_full_read: unsafe extern "C" fn(
        okFrontPanelAXIFull_HANDLE,
        UINT64,
        *mut ::std::os::raw::c_uchar,
        UINT64,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_full_write: unsafe extern "C" fn(
        okFrontPanelAXIFull_HANDLE,
        UINT64,
        *const ::std::os::raw::c_uchar,
        UINT64,
        *mut okTOperationStatistics,
    ) -> ok_ErrorCode,
    pub axi_full_read_ex: unsafe extern "C" fn(
        okFrontPanelAXIFull_HANDLE,
        UINT64,
        *mut ::std::os::raw::c_uchar,
        UINT64,
        UINT32,
        UINT32,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_full_write_ex: unsafe extern "C" fn(
        okFrontPanelAXIFull_HANDLE,
        UINT64,
        *const ::std::os::raw::c_uchar,
        UINT64,
        UINT32,
        UINT32,
        *mut okTOperationStatistics,
    ) -> ok_ErrorCode,
    pub axi_full_reset: unsafe extern "C" fn(okFrontPanelAXIFull_HANDLE) -> ok_ErrorCode,

    // ---- AXI Stream (§8) ----
    pub axi_stream_get_write_datapath_width:
        unsafe extern "C" fn(okFrontPanelAXIStream_HANDLE) -> UINT8,
    pub axi_stream_get_read_datapath_width:
        unsafe extern "C" fn(okFrontPanelAXIStream_HANDLE) -> UINT8,
    pub axi_stream_read: unsafe extern "C" fn(
        okFrontPanelAXIStream_HANDLE,
        *mut ::std::os::raw::c_uchar,
        UINT64,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_stream_write: unsafe extern "C" fn(
        okFrontPanelAXIStream_HANDLE,
        *const ::std::os::raw::c_uchar,
        UINT64,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_stream_read_ex: unsafe extern "C" fn(
        okFrontPanelAXIStream_HANDLE,
        *mut ::std::os::raw::c_uchar,
        UINT64,
        UINT32,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_stream_write_ex: unsafe extern "C" fn(
        okFrontPanelAXIStream_HANDLE,
        *const ::std::os::raw::c_uchar,
        UINT64,
        UINT32,
        *mut UINT64,
    ) -> ok_ErrorCode,
    pub axi_stream_reset: unsafe extern "C" fn(okFrontPanelAXIStream_HANDLE) -> ok_ErrorCode,

    // ---- Device Settings (§9) ----
    pub settings_construct: unsafe extern "C" fn() -> okDeviceSettings_HANDLE,
    pub settings_destruct: unsafe extern "C" fn(okDeviceSettings_HANDLE),
    pub settings_get_string: unsafe extern "C" fn(
        okDeviceSettings_HANDLE,
        *const ::std::os::raw::c_char,
        ::std::os::raw::c_int,
        *mut ::std::os::raw::c_char,
    ) -> ok_ErrorCode,
    pub settings_set_string: unsafe extern "C" fn(
        okDeviceSettings_HANDLE,
        *const ::std::os::raw::c_char,
        *const ::std::os::raw::c_char,
    ) -> ok_ErrorCode,
    pub settings_get_int: unsafe extern "C" fn(
        okDeviceSettings_HANDLE,
        *const ::std::os::raw::c_char,
        *mut UINT32,
    ) -> ok_ErrorCode,
    pub settings_set_int: unsafe extern "C" fn(
        okDeviceSettings_HANDLE,
        *const ::std::os::raw::c_char,
        UINT32,
    ) -> ok_ErrorCode,
    pub settings_delete: unsafe extern "C" fn(
        okDeviceSettings_HANDLE,
        *const ::std::os::raw::c_char,
    ) -> ok_ErrorCode,
    pub settings_save: unsafe extern "C" fn(okDeviceSettings_HANDLE) -> ok_ErrorCode,
    pub settings_list:
        unsafe extern "C" fn(okDeviceSettings_HANDLE, okDeviceSettingNames_HANDLE) -> ok_ErrorCode,
    pub setting_names_construct: unsafe extern "C" fn() -> okDeviceSettingNames_HANDLE,
    pub setting_names_destruct: unsafe extern "C" fn(okDeviceSettingNames_HANDLE),
    pub setting_names_get_count:
        unsafe extern "C" fn(okDeviceSettingNames_HANDLE) -> ::std::os::raw::c_int,
    pub setting_names_get: unsafe extern "C" fn(
        okDeviceSettingNames_HANDLE,
        ::std::os::raw::c_int,
    ) -> *const ::std::os::raw::c_char,
    pub fp_get_device_settings:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okDeviceSettings_HANDLE) -> ok_ErrorCode,

    // ---- Device Sensors (§10) ----
    pub sensors_construct: unsafe extern "C" fn() -> okDeviceSensors_HANDLE,
    pub sensors_destruct: unsafe extern "C" fn(okDeviceSensors_HANDLE),
    pub sensors_get_count: unsafe extern "C" fn(okDeviceSensors_HANDLE) -> ::std::os::raw::c_int,
    pub sensors_get_sensor:
        unsafe extern "C" fn(okDeviceSensors_HANDLE, ::std::os::raw::c_int) -> okTDeviceSensor,
    pub fp_get_device_sensors:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okDeviceSensors_HANDLE) -> ok_ErrorCode,

    // ---- PLL22150 (§11) ----
    pub pll22150_construct: unsafe extern "C" fn() -> okPLL22150_HANDLE,
    pub pll22150_destruct: unsafe extern "C" fn(okPLL22150_HANDLE),
    pub pll22150_set_crystal_load: unsafe extern "C" fn(okPLL22150_HANDLE, f64),
    pub pll22150_set_reference: unsafe extern "C" fn(okPLL22150_HANDLE, f64, Bool),
    pub pll22150_get_reference: unsafe extern "C" fn(okPLL22150_HANDLE) -> f64,
    pub pll22150_set_vco_parameters: unsafe extern "C" fn(
        okPLL22150_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
    ) -> Bool,
    pub pll22150_get_vco_p: unsafe extern "C" fn(okPLL22150_HANDLE) -> ::std::os::raw::c_int,
    pub pll22150_get_vco_q: unsafe extern "C" fn(okPLL22150_HANDLE) -> ::std::os::raw::c_int,
    pub pll22150_get_vco_frequency: unsafe extern "C" fn(okPLL22150_HANDLE) -> f64,
    pub pll22150_set_div1:
        unsafe extern "C" fn(okPLL22150_HANDLE, ok_DividerSource, ::std::os::raw::c_int),
    pub pll22150_set_div2:
        unsafe extern "C" fn(okPLL22150_HANDLE, ok_DividerSource, ::std::os::raw::c_int),
    pub pll22150_get_div1_source: unsafe extern "C" fn(okPLL22150_HANDLE) -> ok_DividerSource,
    pub pll22150_get_div2_source: unsafe extern "C" fn(okPLL22150_HANDLE) -> ok_DividerSource,
    pub pll22150_get_div1_divider: unsafe extern "C" fn(okPLL22150_HANDLE) -> ::std::os::raw::c_int,
    pub pll22150_get_div2_divider: unsafe extern "C" fn(okPLL22150_HANDLE) -> ::std::os::raw::c_int,
    pub pll22150_set_output_source:
        unsafe extern "C" fn(okPLL22150_HANDLE, ::std::os::raw::c_int, ok_ClockSource_22150),
    pub pll22150_set_output_enable:
        unsafe extern "C" fn(okPLL22150_HANDLE, ::std::os::raw::c_int, Bool),
    pub pll22150_get_output_source:
        unsafe extern "C" fn(okPLL22150_HANDLE, ::std::os::raw::c_int) -> ok_ClockSource_22150,
    pub pll22150_get_output_frequency:
        unsafe extern "C" fn(okPLL22150_HANDLE, ::std::os::raw::c_int) -> f64,
    pub pll22150_is_output_enabled:
        unsafe extern "C" fn(okPLL22150_HANDLE, ::std::os::raw::c_int) -> Bool,
    pub fp_get_pll22150_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode,
    pub fp_set_pll22150_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode,
    pub fp_get_eeprom_pll22150_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode,
    pub fp_set_eeprom_pll22150_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22150_HANDLE) -> ok_ErrorCode,

    // ---- PLL22393 (§11) ----
    pub pll22393_construct: unsafe extern "C" fn() -> okPLL22393_HANDLE,
    pub pll22393_destruct: unsafe extern "C" fn(okPLL22393_HANDLE),
    pub pll22393_set_crystal_load: unsafe extern "C" fn(okPLL22393_HANDLE, f64) -> Bool,
    pub pll22393_set_reference: unsafe extern "C" fn(okPLL22393_HANDLE, f64),
    pub pll22393_get_reference: unsafe extern "C" fn(okPLL22393_HANDLE) -> f64,
    pub pll22393_set_pll_parameters: unsafe extern "C" fn(
        okPLL22393_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
        Bool,
    ) -> Bool,
    pub pll22393_set_pll_lf: unsafe extern "C" fn(
        okPLL22393_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
    ) -> Bool,
    pub pll22393_set_output_divider: unsafe extern "C" fn(
        okPLL22393_HANDLE,
        ::std::os::raw::c_int,
        ::std::os::raw::c_int,
    ) -> Bool,
    pub pll22393_set_output_source: unsafe extern "C" fn(
        okPLL22393_HANDLE,
        ::std::os::raw::c_int,
        ok_ClockSource_22393,
    ) -> Bool,
    pub pll22393_set_output_enable:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int, Bool),
    pub pll22393_get_pll_p:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> ::std::os::raw::c_int,
    pub pll22393_get_pll_q:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> ::std::os::raw::c_int,
    pub pll22393_get_pll_frequency:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> f64,
    pub pll22393_get_output_divider:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> ::std::os::raw::c_int,
    pub pll22393_get_output_source:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> ok_ClockSource_22393,
    pub pll22393_get_output_frequency:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> f64,
    pub pll22393_is_output_enabled:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> Bool,
    pub pll22393_is_pll_enabled:
        unsafe extern "C" fn(okPLL22393_HANDLE, ::std::os::raw::c_int) -> Bool,
    pub fp_get_pll22393_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode,
    pub fp_set_pll22393_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode,
    pub fp_get_eeprom_pll22393_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode,
    pub fp_set_eeprom_pll22393_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE, okPLL22393_HANDLE) -> ok_ErrorCode,
    pub fp_load_default_pll_configuration:
        unsafe extern "C" fn(okFrontPanel_HANDLE) -> ok_ErrorCode,

    // ---- Flash Memory (§12) ----
    pub fp_flash_erase_sector: unsafe extern "C" fn(okFrontPanel_HANDLE, UINT32) -> ok_ErrorCode,
    pub fp_flash_write:
        unsafe extern "C" fn(okFrontPanel_HANDLE, UINT32, UINT32, *const UINT8) -> ok_ErrorCode,
    pub fp_flash_read:
        unsafe extern "C" fn(okFrontPanel_HANDLE, UINT32, UINT32, *mut UINT8) -> ok_ErrorCode,

    // ---- Custom Device Registration (§13) ----
    pub fp_add_custom_device:
        unsafe extern "C" fn(*const okTDeviceMatchInfo, *const okTDeviceInfo) -> ok_ErrorCode,
    pub fp_remove_custom_device: unsafe extern "C" fn(::std::os::raw::c_int) -> ok_ErrorCode,

    // ---- FrontPanelManager — Hot-Plug (§14) ----
    pub manager_construct: unsafe extern "C" fn(
        okFrontPanelManager_HANDLE,
        *const ::std::os::raw::c_char,
    ) -> okCFrontPanelManager_HANDLE,
    /// Construct a FrontPanelManager with device-added/removed callbacks.
    ///
    /// Callback signature: `void (*)(void* context, const char* serial)`
    /// where `context` is the `self` (first) parameter passed to this function.
    pub manager_construct_with_callbacks: unsafe extern "C" fn(
        okFrontPanelManager_HANDLE,                // self (context for callbacks)
        *const ::std::os::raw::c_char,             // realm
        unsafe extern "C" fn(*mut ::std::os::raw::c_void, *const ::std::os::raw::c_char), // onAdded
        unsafe extern "C" fn(*mut ::std::os::raw::c_void, *const ::std::os::raw::c_char), // onRemoved
    ) -> okCFrontPanelManager_HANDLE,
    pub manager_destruct: unsafe extern "C" fn(okCFrontPanelManager_HANDLE),
    pub manager_start_monitoring: unsafe extern "C" fn(okCFrontPanelManager_HANDLE) -> ok_ErrorCode,
    pub manager_start_monitoring_with_cb_info:
        unsafe extern "C" fn(okCFrontPanelManager_HANDLE, *mut okTCallbackInfo) -> ok_ErrorCode,
    pub manager_stop_monitoring: unsafe extern "C" fn(okCFrontPanelManager_HANDLE) -> ok_ErrorCode,
    pub manager_enter_monitor_loop: unsafe extern "C" fn(
        okCFrontPanelManager_HANDLE,
        *const okTCallbackInfo,
    ) -> ::std::os::raw::c_int,
    pub manager_enter_monitor_loop_with_timeout: unsafe extern "C" fn(
        okCFrontPanelManager_HANDLE,
        *const okTCallbackInfo,
        ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int,
    pub manager_exit_monitor_loop:
        unsafe extern "C" fn(okCFrontPanelManager_HANDLE, ::std::os::raw::c_int),
    pub manager_open: unsafe extern "C" fn(
        okCFrontPanelManager_HANDLE,
        *const ::std::os::raw::c_char,
    ) -> okFrontPanel_HANDLE,
    pub manager_emulate_test_device_connection:
        unsafe extern "C" fn(*const ::std::os::raw::c_char, Bool) -> ok_ErrorCode,

    // ---- FrontPanelDevices — Snapshot Enumeration (§15) ----
    pub devices_construct: unsafe extern "C" fn(
        *const ::std::os::raw::c_char,
        *mut *mut okError,
    ) -> okCFrontPanelDevices_HANDLE,
    pub devices_destruct: unsafe extern "C" fn(okCFrontPanelDevices_HANDLE),
    pub devices_get_count:
        unsafe extern "C" fn(okCFrontPanelDevices_HANDLE) -> ::std::os::raw::c_int,
    pub devices_get_serial: unsafe extern "C" fn(
        okCFrontPanelDevices_HANDLE,
        ::std::os::raw::c_int,
        *mut ::std::os::raw::c_char,
    ),
    pub devices_open: unsafe extern "C" fn(
        okCFrontPanelDevices_HANDLE,
        *const ::std::os::raw::c_char,
    ) -> okFrontPanel_HANDLE,

    // ---- Error (§2) - additional ----
    pub error_get_message: unsafe extern "C" fn(*const okError) -> *const ::std::os::raw::c_char,
    pub error_free: unsafe extern "C" fn(*mut okError),
}

// SAFETY: FrontPanelDll contains only function pointers (immutable addresses
// in the loaded DLL's text segment) and a `Library` handle. Function pointers
// are inherently thread-safe to share — they're just code addresses.
// The underlying C functions may not be thread-safe, but that's the caller's
// responsibility to enforce via serialization (e.g., Mutex).
unsafe impl Send for FrontPanelDll {}
unsafe impl Sync for FrontPanelDll {}

// ============================================================================
// Loading Implementation
// ============================================================================

impl FrontPanelDll {
    /// Load the FrontPanel shared library from the given path and resolve
    /// all function symbols.
    ///
    /// ## Arguments
    ///
    /// * `path` — Path to `okFrontPanel.dll` (Windows) or
    ///   `libokFrontPanel.so` (Linux).
    ///
    /// ## Errors
    ///
    /// Returns [`LoadError::LibraryNotFound`] if the file doesn't exist.
    /// Returns [`LoadError::SymbolNotFound`] if a required symbol is missing.
    /// Returns [`LoadError::VersionMismatch`] if the API version is < 6.1.0.
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let dll = FrontPanelDll::load(Path::new("okFrontPanel.dll"))?;
    /// let version = dll.api_version_string();
    /// println!("Loaded FrontPanel {}", version);
    /// ```
    pub fn load(path: &Path) -> Result<Self, FfiError> {
        // SAFETY: Loading a shared library is inherently unsafe because we
        // trust that the DLL exports functions with the correct signatures.
        // The C header (okFrontPanel.h) defines these signatures, and our
        // bindgen build script verified them at compile time.
        let library = unsafe {
            Library::new(path).map_err(|e| LoadError::LibraryNotFound {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?
        };

        // SAFETY: Each `load_symbol!` call reads a function pointer from the
        // loaded DLL. We trust that the DLL exports these symbols with the
        // signatures declared in okFrontPanel.h. Each symbol name must
        // exactly match the C function name, including the null terminator
        // (which `as_bytes()` provides when used with a &str).
        unsafe {
            let dll = FrontPanelDll {
                // ---- Version & Library (§1) ----
                get_api_version_major: *load_symbol!(library, "okFrontPanel_GetAPIVersionMajor")?,
                get_api_version_minor: *load_symbol!(library, "okFrontPanel_GetAPIVersionMinor")?,
                get_api_version_micro: *load_symbol!(library, "okFrontPanel_GetAPIVersionMicro")?,
                get_api_version_string: *load_symbol!(library, "okFrontPanel_GetAPIVersionString")?,
                check_api_version: *load_symbol!(library, "okFrontPanel_CheckAPIVersion")?,

                // ---- Error Handling (§2) ----
                get_error_message: *load_symbol!(library, "okFrontPanel_GetErrorMessage")?,
                get_last_error: *load_symbol!(library, "okFrontPanel_GetLastError")?,
                get_last_error_message: *load_symbol!(library, "okFrontPanel_GetLastErrorMessage")?,
                error_get_message: *load_symbol!(library, "okError_GetMessage")?,
                error_free: *load_symbol!(library, "okError_Free")?,

                // ---- Device Core (§3) ----
                fp_construct: *load_symbol!(library, "okFrontPanel_Construct")?,
                fp_destruct: *load_symbol!(library, "okFrontPanel_Destruct")?,
                fp_is_open: *load_symbol!(library, "okFrontPanel_IsOpen")?,
                fp_is_remote: *load_symbol!(library, "okFrontPanel_IsRemote")?,
                fp_close: *load_symbol!(library, "okFrontPanel_Close")?,
                fp_set_timeout: *load_symbol!(library, "okFrontPanel_SetTimeout")?,
                fp_set_device_id: *load_symbol!(library, "okFrontPanel_SetDeviceID")?,
                fp_get_device_info_with_size: *load_symbol!(
                    library,
                    "okFrontPanel_GetDeviceInfoWithSize"
                )?,
                fp_is_front_panel_enabled: *load_symbol!(
                    library,
                    "okFrontPanel_IsFrontPanelEnabled"
                )?,
                fp_get_last_transfer_length: *load_symbol!(
                    library,
                    "okFrontPanel_GetLastTransferLength"
                )?,

                // ---- FPGA Configuration (§4) ----
                fp_configure_fpga: *load_symbol!(library, "okFrontPanel_ConfigureFPGA")?,
                fp_configure_fpga_with_reset: *load_symbol!(
                    library,
                    "okFrontPanel_ConfigureFPGAWithReset"
                )?,
                fp_configure_fpga_from_memory: *load_symbol!(
                    library,
                    "okFrontPanel_ConfigureFPGAFromMemory"
                )?,
                fp_configure_fpga_from_memory_with_progress: *load_symbol!(
                    library,
                    "okFrontPanel_ConfigureFPGAFromMemoryWithProgress"
                )?,
                fp_configure_fpga_from_memory_with_reset: *load_symbol!(
                    library,
                    "okFrontPanel_ConfigureFPGAFromMemoryWithReset"
                )?,
                fp_configure_fpga_from_flash: *load_symbol!(
                    library,
                    "okFrontPanel_ConfigureFPGAFromFlash"
                )?,
                fp_clear_fpga_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_ClearFPGAConfiguration"
                )?,

                // ---- Reset Profile (§5) ----
                fp_get_fpga_reset_profile: *load_symbol!(
                    library,
                    "okFrontPanel_GetFPGAResetProfile"
                )?,
                fp_set_fpga_reset_profile: *load_symbol!(
                    library,
                    "okFrontPanel_SetFPGAResetProfile"
                )?,

                // ---- Data Port Acquisition (§6) ----
                fp_get_fpga_data_port_classic: *load_symbol!(
                    library,
                    "okFrontPanel_GetFPGADataPortClassic"
                )?,
                fp_get_fpga_data_port_axi: *load_symbol!(
                    library,
                    "okFrontPanel_GetFPGADataPortAXI"
                )?,

                // ---- Classic Data Port (§7) ----
                classic_reset_fpga: *load_symbol!(library, "okFPGADataPortClassic_ResetFPGA")?,
                classic_set_wire_in_value: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_SetWireInValue"
                )?,
                classic_get_wire_in_value: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_GetWireInValue"
                )?,
                classic_update_wire_ins: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_UpdateWireIns"
                )?,
                classic_get_wire_out_value: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_GetWireOutValue"
                )?,
                classic_update_wire_outs: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_UpdateWireOuts"
                )?,
                classic_activate_trigger_in: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_ActivateTriggerIn"
                )?,
                classic_update_trigger_outs: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_UpdateTriggerOuts"
                )?,
                classic_is_triggered: *load_symbol!(library, "okFPGADataPortClassic_IsTriggered")?,
                classic_get_trigger_out_vector: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_GetTriggerOutVector"
                )?,
                classic_write_to_pipe_in: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_WriteToPipeIn"
                )?,
                classic_read_from_pipe_out: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_ReadFromPipeOut"
                )?,
                classic_write_to_block_pipe_in: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_WriteToBlockPipeIn"
                )?,
                classic_read_from_block_pipe_out: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_ReadFromBlockPipeOut"
                )?,
                classic_set_bt_pipe_polling_interval: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_SetBTPipePollingInterval"
                )?,
                classic_read_register: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_ReadRegister"
                )?,
                classic_write_register: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_WriteRegister"
                )?,
                classic_read_registers: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_ReadRegisters"
                )?,
                classic_write_registers: *load_symbol!(
                    library,
                    "okFPGADataPortClassic_WriteRegisters"
                )?,

                // ---- AXI Data Port Root (§8) ----
                axi_get_axi_lite: *load_symbol!(library, "okFPGADataPortAXI_GetAXILite")?,
                axi_get_axi_full: *load_symbol!(library, "okFPGADataPortAXI_GetAXIFull")?,
                axi_get_axi_stream: *load_symbol!(library, "okFPGADataPortAXI_GetAXIStream")?,

                // ---- AXI Lite (§8) ----
                axi_lite_read: *load_symbol!(library, "okFrontPanelAXILite_Read")?,
                axi_lite_write: *load_symbol!(library, "okFrontPanelAXILite_Write")?,
                axi_lite_read_bulk: *load_symbol!(library, "okFrontPanelAXILite_ReadBulk")?,
                axi_lite_write_bulk: *load_symbol!(library, "okFrontPanelAXILite_WriteBulk")?,
                axi_lite_read_ex: *load_symbol!(library, "okFrontPanelAXILite_ReadEx")?,
                axi_lite_write_ex: *load_symbol!(library, "okFrontPanelAXILite_WriteEx")?,
                axi_lite_read_bulk_ex: *load_symbol!(library, "okFrontPanelAXILite_ReadBulkEx")?,
                axi_lite_write_bulk_ex: *load_symbol!(library, "okFrontPanelAXILite_WriteBulkEx")?,
                axi_lite_reset: *load_symbol!(library, "okFrontPanelAXILite_Reset")?,

                // ---- AXI Full (§8) ----
                axi_full_get_datapath_width: *load_symbol!(
                    library,
                    "okFrontPanelAXIFull_GetDatapathWidthByteCount"
                )?,
                axi_full_read: *load_symbol!(library, "okFrontPanelAXIFull_Read")?,
                axi_full_write: *load_symbol!(library, "okFrontPanelAXIFull_Write")?,
                axi_full_read_ex: *load_symbol!(library, "okFrontPanelAXIFull_ReadEx")?,
                axi_full_write_ex: *load_symbol!(library, "okFrontPanelAXIFull_WriteEx")?,
                axi_full_reset: *load_symbol!(library, "okFrontPanelAXIFull_Reset")?,

                // ---- AXI Stream (§8) ----
                axi_stream_get_write_datapath_width: *load_symbol!(
                    library,
                    "okFrontPanelAXIStream_GetWriteDatapathWidthByteCount"
                )?,
                axi_stream_get_read_datapath_width: *load_symbol!(
                    library,
                    "okFrontPanelAXIStream_GetReadDatapathWidthByteCount"
                )?,
                axi_stream_read: *load_symbol!(library, "okFrontPanelAXIStream_Read")?,
                axi_stream_write: *load_symbol!(library, "okFrontPanelAXIStream_Write")?,
                axi_stream_read_ex: *load_symbol!(library, "okFrontPanelAXIStream_ReadEx")?,
                axi_stream_write_ex: *load_symbol!(library, "okFrontPanelAXIStream_WriteEx")?,
                axi_stream_reset: *load_symbol!(library, "okFrontPanelAXIStream_Reset")?,

                // ---- Device Settings (§9) ----
                settings_construct: *load_symbol!(library, "okDeviceSettings_Construct")?,
                settings_destruct: *load_symbol!(library, "okDeviceSettings_Destruct")?,
                settings_get_string: *load_symbol!(library, "okDeviceSettings_GetString")?,
                settings_set_string: *load_symbol!(library, "okDeviceSettings_SetString")?,
                settings_get_int: *load_symbol!(library, "okDeviceSettings_GetInt")?,
                settings_set_int: *load_symbol!(library, "okDeviceSettings_SetInt")?,
                settings_delete: *load_symbol!(library, "okDeviceSettings_Delete")?,
                settings_save: *load_symbol!(library, "okDeviceSettings_Save")?,
                settings_list: *load_symbol!(library, "okDeviceSettings_List")?,
                setting_names_construct: *load_symbol!(library, "okDeviceSettingNames_Construct")?,
                setting_names_destruct: *load_symbol!(library, "okDeviceSettingNames_Destruct")?,
                setting_names_get_count: *load_symbol!(library, "okDeviceSettingNames_GetCount")?,
                setting_names_get: *load_symbol!(library, "okDeviceSettingNames_Get")?,
                fp_get_device_settings: *load_symbol!(library, "okFrontPanel_GetDeviceSettings")?,

                // ---- Device Sensors (§10) ----
                sensors_construct: *load_symbol!(library, "okDeviceSensors_Construct")?,
                sensors_destruct: *load_symbol!(library, "okDeviceSensors_Destruct")?,
                sensors_get_count: *load_symbol!(library, "okDeviceSensors_GetSensorCount")?,
                sensors_get_sensor: *load_symbol!(library, "okDeviceSensors_GetSensor")?,
                fp_get_device_sensors: *load_symbol!(library, "okFrontPanel_GetDeviceSensors")?,

                // ---- PLL22150 (§11) ----
                pll22150_construct: *load_symbol!(library, "okPLL22150_Construct")?,
                pll22150_destruct: *load_symbol!(library, "okPLL22150_Destruct")?,
                pll22150_set_crystal_load: *load_symbol!(library, "okPLL22150_SetCrystalLoad")?,
                pll22150_set_reference: *load_symbol!(library, "okPLL22150_SetReference")?,
                pll22150_get_reference: *load_symbol!(library, "okPLL22150_GetReference")?,
                pll22150_set_vco_parameters: *load_symbol!(library, "okPLL22150_SetVCOParameters")?,
                pll22150_get_vco_p: *load_symbol!(library, "okPLL22150_GetVCOP")?,
                pll22150_get_vco_q: *load_symbol!(library, "okPLL22150_GetVCOQ")?,
                pll22150_get_vco_frequency: *load_symbol!(library, "okPLL22150_GetVCOFrequency")?,
                pll22150_set_div1: *load_symbol!(library, "okPLL22150_SetDiv1")?,
                pll22150_set_div2: *load_symbol!(library, "okPLL22150_SetDiv2")?,
                pll22150_get_div1_source: *load_symbol!(library, "okPLL22150_GetDiv1Source")?,
                pll22150_get_div2_source: *load_symbol!(library, "okPLL22150_GetDiv2Source")?,
                pll22150_get_div1_divider: *load_symbol!(library, "okPLL22150_GetDiv1Divider")?,
                pll22150_get_div2_divider: *load_symbol!(library, "okPLL22150_GetDiv2Divider")?,
                pll22150_set_output_source: *load_symbol!(library, "okPLL22150_SetOutputSource")?,
                pll22150_set_output_enable: *load_symbol!(library, "okPLL22150_SetOutputEnable")?,
                pll22150_get_output_source: *load_symbol!(library, "okPLL22150_GetOutputSource")?,
                pll22150_get_output_frequency: *load_symbol!(
                    library,
                    "okPLL22150_GetOutputFrequency"
                )?,
                pll22150_is_output_enabled: *load_symbol!(library, "okPLL22150_IsOutputEnabled")?,
                fp_get_pll22150_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_GetPLL22150Configuration"
                )?,
                fp_set_pll22150_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_SetPLL22150Configuration"
                )?,
                fp_get_eeprom_pll22150_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_GetEepromPLL22150Configuration"
                )?,
                fp_set_eeprom_pll22150_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_SetEepromPLL22150Configuration"
                )?,

                // ---- PLL22393 (§11) ----
                pll22393_construct: *load_symbol!(library, "okPLL22393_Construct")?,
                pll22393_destruct: *load_symbol!(library, "okPLL22393_Destruct")?,
                pll22393_set_crystal_load: *load_symbol!(library, "okPLL22393_SetCrystalLoad")?,
                pll22393_set_reference: *load_symbol!(library, "okPLL22393_SetReference")?,
                pll22393_get_reference: *load_symbol!(library, "okPLL22393_GetReference")?,
                pll22393_set_pll_parameters: *load_symbol!(library, "okPLL22393_SetPLLParameters")?,
                pll22393_set_pll_lf: *load_symbol!(library, "okPLL22393_SetPLLLF")?,
                pll22393_set_output_divider: *load_symbol!(library, "okPLL22393_SetOutputDivider")?,
                pll22393_set_output_source: *load_symbol!(library, "okPLL22393_SetOutputSource")?,
                pll22393_set_output_enable: *load_symbol!(library, "okPLL22393_SetOutputEnable")?,
                pll22393_get_pll_p: *load_symbol!(library, "okPLL22393_GetPLLP")?,
                pll22393_get_pll_q: *load_symbol!(library, "okPLL22393_GetPLLQ")?,
                pll22393_get_pll_frequency: *load_symbol!(library, "okPLL22393_GetPLLFrequency")?,
                pll22393_get_output_divider: *load_symbol!(library, "okPLL22393_GetOutputDivider")?,
                pll22393_get_output_source: *load_symbol!(library, "okPLL22393_GetOutputSource")?,
                pll22393_get_output_frequency: *load_symbol!(
                    library,
                    "okPLL22393_GetOutputFrequency"
                )?,
                pll22393_is_output_enabled: *load_symbol!(library, "okPLL22393_IsOutputEnabled")?,
                pll22393_is_pll_enabled: *load_symbol!(library, "okPLL22393_IsPLLEnabled")?,
                fp_get_pll22393_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_GetPLL22393Configuration"
                )?,
                fp_set_pll22393_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_SetPLL22393Configuration"
                )?,
                fp_get_eeprom_pll22393_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_GetEepromPLL22393Configuration"
                )?,
                fp_set_eeprom_pll22393_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_SetEepromPLL22393Configuration"
                )?,
                fp_load_default_pll_configuration: *load_symbol!(
                    library,
                    "okFrontPanel_LoadDefaultPLLConfiguration"
                )?,

                // ---- Flash Memory (§12) ----
                fp_flash_erase_sector: *load_symbol!(library, "okFrontPanel_FlashEraseSector")?,
                fp_flash_write: *load_symbol!(library, "okFrontPanel_FlashWrite")?,
                fp_flash_read: *load_symbol!(library, "okFrontPanel_FlashRead")?,

                // ---- Custom Device Registration (§13) ----
                fp_add_custom_device: *load_symbol!(library, "okFrontPanel_AddCustomDevice")?,
                fp_remove_custom_device: *load_symbol!(library, "okFrontPanel_RemoveCustomDevice")?,

                // ---- FrontPanelManager (§14) ----
                manager_construct: *load_symbol!(library, "okFrontPanelManager_Construct")?,
                manager_construct_with_callbacks: *load_symbol!(
                    library,
                    "okFrontPanelManager_ConstructWithCallbacks"
                )?,
                manager_destruct: *load_symbol!(library, "okFrontPanelManager_Destruct")?,
                manager_start_monitoring: *load_symbol!(
                    library,
                    "okFrontPanelManager_StartMonitoring"
                )?,
                manager_start_monitoring_with_cb_info: *load_symbol!(
                    library,
                    "okFrontPanelManager_StartMonitoringWithCBInfo"
                )?,
                manager_stop_monitoring: *load_symbol!(
                    library,
                    "okFrontPanelManager_StopMonitoring"
                )?,
                manager_enter_monitor_loop: *load_symbol!(
                    library,
                    "okFrontPanelManager_EnterMonitorLoop"
                )?,
                manager_enter_monitor_loop_with_timeout: *load_symbol!(
                    library,
                    "okFrontPanelManager_EnterMonitorLoopWithTimeout"
                )?,
                manager_exit_monitor_loop: *load_symbol!(
                    library,
                    "okFrontPanelManager_ExitMonitorLoop"
                )?,
                manager_open: *load_symbol!(library, "okFrontPanelManager_Open")?,
                manager_emulate_test_device_connection: *load_symbol!(
                    library,
                    "okFrontPanelManager_EmulateTestDeviceConnection"
                )?,

                // ---- FrontPanelDevices (§15) ----
                devices_construct: *load_symbol!(library, "okFrontPanelDevices_Construct")?,
                devices_destruct: *load_symbol!(library, "okFrontPanelDevices_Destruct")?,
                devices_get_count: *load_symbol!(library, "okFrontPanelDevices_GetCount")?,
                devices_get_serial: *load_symbol!(library, "okFrontPanelDevices_GetSerial")?,
                devices_open: *load_symbol!(library, "okFrontPanelDevices_Open")?,

                library,
            };

            // Verify the DLL reports API version >= 6.1.0.
            let major = (dll.get_api_version_major)();
            let minor = (dll.get_api_version_minor)();
            let micro = (dll.get_api_version_micro)();

            if major < 6 || (major == 6 && minor < 1) {
                return Err(LoadError::VersionMismatch {
                    actual: format!("{}.{}.{}", major, minor, micro),
                    required: "6.1.0".to_string(),
                }
                .into());
            }

            Ok(dll)
        }
    }

    /// Returns the FrontPanel API version string (e.g., "6.1.0").
    ///
    /// This calls the loaded DLL's `okFrontPanel_GetAPIVersionString` function.
    pub fn api_version_string(&self) -> String {
        // SAFETY: The function returns a pointer to a static string owned by
        // the DLL. The pointer is valid for the lifetime of the loaded library.
        unsafe {
            let ptr = (self.get_api_version_string)();
            if ptr.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        }
    }

    /// Returns the default DLL filename for the current platform.
    ///
    /// - Windows: `okFrontPanel.dll`
    /// - Linux: `libokFrontPanel.so`
    pub fn default_library_name() -> &'static str {
        if cfg!(windows) {
            "okFrontPanel.dll"
        } else {
            "libokFrontPanel.so"
        }
    }
}
