//! # ok-fpga-ffi — Low-Level FFI Bindings to Opal Kelly FrontPanel 6.1.0
//!
//! This crate provides the lowest layer of the ok-FPGA-Server stack. It handles:
//!
//! - **Type generation** via `rust-bindgen`: Parses the C header file
//!   `FrontPanel-6.1.0/okFrontPanel.h` at build time and auto-generates Rust
//!   representations of all C structs, enums, and function signatures.
//!
//! - **Runtime DLL loading** via `libloading`: Loads `okFrontPanel.dll` (Windows)
//!   or `libokFrontPanel.so` (Linux) at runtime rather than linking at compile time.
//!   This allows the server to start gracefully even when the DLL is not present.
//!
//! - **Safe handle wrappers**: All C opaque handles (e.g., `okFrontPanel_HANDLE`,
//!   `okDeviceSettings_HANDLE`) are wrapped in Rust structs that automatically
//!   call the corresponding `*_Destruct()` function when dropped. This is the
//!   RAII (Resource Acquisition Is Initialization) pattern.
//!
//! ## Safety Boundary
//!
//! All `unsafe` code in the entire project is confined to this crate. Higher
//! layers (`ok-fpga-core`, `ok-fpga-server`) never use `unsafe` directly.
//!
//! ## Excluded APIs
//!
//! The following FrontPanel functions are excluded because they only work on
//! USB 2.0 devices, which this project does not support:
//! - `okFrontPanel_WriteI2C` / `okFrontPanel_ReadI2C`
//!
//! The Lua scripting API (`okBuffer_*`, `okScriptValue_*`, `okScriptEngine_*`)
//! is also excluded as it is not needed for the server.

/// Placeholder function to verify the crate compiles and links correctly.
///
/// Returns the crate name as a static string. This will be replaced by
/// real FFI bindings in Phase 3 of the development roadmap.
pub fn crate_name() -> &'static str {
    "ok-fpga-ffi"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_correct() {
        assert_eq!(crate_name(), "ok-fpga-ffi");
    }
}
