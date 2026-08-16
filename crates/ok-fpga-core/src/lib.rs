//! # ok-fpga-core — Safe Domain Logic for the ok-FPGA-Server
//!
//! This crate is the "brain" of the server. It sits between the low-level
//! FFI bindings (`ok-fpga-ffi`) and the network-facing API layer
//! (`ok-fpga-server`). It provides:
//!
//! - **Device lifecycle management** (`DeviceManager`): Wraps the FrontPanel
//!   `FrontPanelManager` to detect when FPGAs are connected or disconnected
//!   (hot-plug). See ADR-002.
//!
//! - **Data port abstractions** (`ClassicDataPort`, `AXIDataPort`): Safe Rust
//!   types that represent the two kinds of FPGA data interfaces. See ADR-003.
//!
//! - **Resource management** (`Resource`, `ResourceAccessPolicy`): A multi-user
//!   system where "resources" are user-defined capabilities exposed by a
//!   programmed FPGA. Resources can be shared or exclusively locked. See ADR-006.
//!
//! - **Board recognition & SYZYGY discovery**: Identifies supported Opal Kelly
//!   boards and auto-discovers SYZYGY peripheral ports. See ADR-009.
//!
//! ## Key Design Principle
//!
//! This crate never uses `unsafe` directly. All unsafe FFI calls are made
//! through the `ok-fpga-ffi` crate's safe wrapper API.

/// Placeholder function to verify the crate compiles and links correctly.
///
/// Returns the crate name as a static string. This will be replaced by
/// real domain types in Phase 4 of the development roadmap.
pub fn crate_name() -> &'static str {
    "ok-fpga-core"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_correct() {
        assert_eq!(crate_name(), "ok-fpga-core");
    }
}
