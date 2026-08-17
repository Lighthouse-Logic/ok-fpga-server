//! # ok-fpga-core — Safe Domain Logic for the ok-FPGA-Server
//!
//! This crate is the "brain" of the server. It sits between the low-level
//! FFI bindings (`ok-fpga-ffi`) and the network-facing API layer
//! (`ok-fpga-server`). It provides:
//!
//! - **Device lifecycle management** ([`manager::DeviceManager`]): Wraps the
//!   FrontPanel `FrontPanelManager` to detect when FPGAs are connected or
//!   disconnected (hot-plug). See ADR-002.
//!
//! - **Data port abstractions** ([`dataport::ClassicPortInfo`],
//!   [`dataport::AXIPortInfo`]): Safe Rust types that represent the two
//!   kinds of FPGA data interfaces. See ADR-003.
//!
//! - **Resource management** ([`resource::Resource`],
//!   [`resource::ResourceAccessPolicy`]): A multi-user system where
//!   "resources" are user-defined capabilities exposed by a programmed FPGA.
//!   Resources can be shared or exclusively locked. See ADR-006.
//!
//! - **Board recognition & SYZYGY discovery**
//!   ([`device::is_supported_device`], [`syzygy::SyzygyPort`]):
//!   Identifies supported Opal Kelly boards and auto-discovers SYZYGY
//!   peripheral ports. See ADR-009.
//!
//! - **Service lifecycle management** ([`service::ManagedService`],
//!   [`service_manager::ServiceManager`]): Centralized start/stop/restart
//!   control for optional server services (REST, gRPC, MQTT, mDNS).
//!   See ADR-010.
//!
//! ## Key Design Principle
//!
//! This crate never uses `unsafe` directly. All unsafe FFI calls are made
//! through the `ok-fpga-ffi` crate's safe wrapper API.

// ============================================================================
// Module declarations
// ============================================================================

/// Device types, board recognition, and device info conversion.
/// Implements ADR-002 (device lifecycle) and ADR-009 (board matrix).
pub mod device;

/// Device sensor types and conversions from the C API.
pub mod sensors;

/// SYZYGY port auto-discovery types and constants.
/// Implements ADR-009 discovery algorithm.
pub mod syzygy;

/// Data port abstractions — Classic vs AXI.
/// Implements ADR-003 (data port abstraction).
pub mod dataport;

/// Multi-user resource management.
/// Implements ADR-006 (resource model, access policies, registry).
pub mod resource;

/// Device manager, registry, and hot-plug event system.
/// Implements ADR-002 (device lifecycle, FrontPanelManager integration).
pub mod manager;

/// Service lifecycle types: [`ManagedService`](service::ManagedService) trait,
/// [`ServiceStatus`](service::ServiceStatus), [`ServiceEvent`](service::ServiceEvent),
/// and [`ServiceError`](service::ServiceError).
/// Implements ADR-010 (service manager abstraction).
pub mod service;

/// Centralized service lifecycle controller.
/// Manages start/stop/restart for all optional server services.
/// Implements ADR-010 (service manager, shutdown ordering, event broadcasting).
pub mod service_manager;

/// Authentication — pluggable auth provider system.
/// Supports no-auth (default), local user store, and LDAP/LDAPS.
/// Implements ADR-008 (authentication stubs).
pub mod auth;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        // Verify core types are accessible — catches module visibility issues.
        let _dm = super::manager::DeviceManager::new();
        let _rr = super::resource::ResourceRegistry::new();
    }
}
