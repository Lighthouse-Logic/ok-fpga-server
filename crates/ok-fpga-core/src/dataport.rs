//! # Data Port Abstractions — Classic vs AXI
//!
//! This module defines the data port types from ADR-003. After an FPGA is
//! configured with a bitfile, it may expose one or both of two HDL endpoint
//! families:
//!
//! - **Classic Data Port** — traditional FrontPanel wire/trigger/pipe/register
//!   operations (19 methods) via `okWireIn`, `okTriggerIn`, etc.
//! - **AXI Data Port** — memory-mapped and streaming operations via
//!   `okRegisterBridge` with sub-interfaces: AXI-Lite, AXI-Full, AXI-Stream.
//!
//! **Important**: The `okTDeviceInfo.dataport` field describes the **physical
//! host interface** (FP-Gen1/2 USB vs FP-Gen3 USB), NOT which endpoint
//! families the bitfile implements. A single bitfile can implement both
//! Classic and AXI endpoints simultaneously on the shared `okHost` bus.
//! See ADR-003 Addendum A for details.
//!
//! ## Ownership
//!
//! Data port handles are **owned by the device handle** — they must not
//! outlive the device, and the caller must not free them. In Rust, this
//! means data port references are tied to the device's lifetime.

use serde::{Deserialize, Serialize};

// ============================================================================
// DataPort Marker Trait
// ============================================================================

/// Marker trait for data port types.
///
/// This trait is implemented by both [`ClassicPortInfo`] and [`AXIPortInfo`].
/// It allows generic code to work with either port type.
pub trait DataPort: Send {}

// ============================================================================
// ActiveDataPort Enum
// ============================================================================

/// Represents which data port endpoint families a device currently exposes.
///
/// A device may expose Classic endpoints, AXI endpoints, both, or neither
/// (if not yet configured). The available families depend on the loaded
/// bitfile's HDL design, NOT on the `okTDeviceInfo.dataport` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActiveDataPort {
    /// No data port — FPGA is not yet configured.
    None,

    /// Only Classic FrontPanel endpoints are available.
    Classic(ClassicPortInfo),

    /// Only AXI endpoints are available.
    Axi(AXIPortInfo),

    /// Both Classic and AXI endpoints are available simultaneously.
    /// This is the case for bitfiles that implement both `okWireIn`/etc.
    /// and `okRegisterBridge` on the shared `okHost` bus.
    Both {
        /// Classic endpoint metadata.
        classic: ClassicPortInfo,
        /// AXI endpoint metadata.
        axi: AXIPortInfo,
    },
}

impl ActiveDataPort {
    /// Returns `true` if a data port is active (FPGA is configured).
    pub fn is_active(&self) -> bool {
        !matches!(self, ActiveDataPort::None)
    }
}

// ============================================================================
// Classic Data Port
// ============================================================================

/// Metadata about an active Classic data port.
///
/// The Classic data port provides the traditional FrontPanel operations:
/// - **WireIn/WireOut** — 32-bit status/control registers
/// - **TriggerIn/TriggerOut** — single-cycle pulse triggers
/// - **PipeIn/PipeOut** — streaming data transfers (bulk and block)
/// - **Registers** — direct register read/write
///
/// The actual FFI calls are made by the server layer using the
/// `FrontPanelDll` function pointers. This struct holds metadata only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassicPortInfo {
    /// Wire endpoint width in bits (typically 32).
    pub wire_width: i32,

    /// Trigger endpoint width in bits (typically 32).
    pub trigger_width: i32,

    /// Pipe endpoint width in bits (typically 32).
    pub pipe_width: i32,

    /// Register address width in bits.
    pub register_address_width: i32,

    /// Register data width in bits.
    pub register_data_width: i32,
}

impl DataPort for ClassicPortInfo {}

// ============================================================================
// AXI Data Port
// ============================================================================

/// Metadata about an active AXI data port.
///
/// The AXI data port provides three sub-interfaces:
///
/// - **AXI-Lite** — single and bulk register read/write (32-bit data)
/// - **AXI-Full** — bulk memory-mapped read/write (variable width)
/// - **AXI-Stream** — streaming data transfers (variable width)
///
/// Each sub-interface has its own handle obtained from the AXI port root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AXIPortInfo {
    /// Whether AXI-Lite sub-interface is available.
    pub has_axi_lite: bool,

    /// Whether AXI-Full sub-interface is available.
    pub has_axi_full: bool,

    /// Whether AXI-Stream sub-interface is available.
    pub has_axi_stream: bool,

    /// AXI-Full datapath width in bytes (if available).
    pub full_datapath_width: Option<u8>,

    /// AXI-Stream write datapath width in bytes (if available).
    pub stream_write_datapath_width: Option<u8>,

    /// AXI-Stream read datapath width in bytes (if available).
    pub stream_read_datapath_width: Option<u8>,
}

impl DataPort for AXIPortInfo {}

// ============================================================================
// Classic Port Endpoint Addresses
// ============================================================================

/// Wire-In endpoint address.
///
/// FrontPanel Classic data port uses endpoint addresses in the range 0x00-0x1F
/// for WireIn, 0x20-0x3F for WireOut. This newtype provides a safe wrapper
/// that validates the address range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireInAddress(pub u8);

impl WireInAddress {
    /// Create a new WireIn address, validating the range (0x00-0x1F).
    ///
    /// Returns `None` if the address is out of range.
    pub fn new(addr: u8) -> Option<Self> {
        if addr <= 0x1F {
            Some(WireInAddress(addr))
        } else {
            None
        }
    }
}

/// Wire-Out endpoint address.
///
/// FrontPanel Classic data port uses endpoint addresses 0x20-0x3F for WireOut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireOutAddress(pub u8);

impl WireOutAddress {
    /// Create a new WireOut address, validating the range (0x20-0x3F).
    pub fn new(addr: u8) -> Option<Self> {
        if (0x20..=0x3F).contains(&addr) {
            Some(WireOutAddress(addr))
        } else {
            None
        }
    }
}

/// Trigger-In endpoint address.
///
/// FrontPanel Classic data port uses endpoint addresses 0x40-0x5F for TriggerIn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerInAddress(pub u8);

impl TriggerInAddress {
    /// Create a new TriggerIn address, validating the range (0x40-0x5F).
    pub fn new(addr: u8) -> Option<Self> {
        if (0x40..=0x5F).contains(&addr) {
            Some(TriggerInAddress(addr))
        } else {
            None
        }
    }
}

/// Trigger-Out endpoint address.
///
/// FrontPanel Classic data port uses endpoint addresses 0x60-0x7F for TriggerOut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerOutAddress(pub u8);

impl TriggerOutAddress {
    /// Create a new TriggerOut address, validating the range (0x60-0x7F).
    pub fn new(addr: u8) -> Option<Self> {
        if (0x60..=0x7F).contains(&addr) {
            Some(TriggerOutAddress(addr))
        } else {
            None
        }
    }
}

/// Pipe-In endpoint address.
///
/// FrontPanel Classic data port uses endpoint addresses 0x80-0x9F for PipeIn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeInAddress(pub u8);

impl PipeInAddress {
    /// Create a new PipeIn address, validating the range (0x80-0x9F).
    pub fn new(addr: u8) -> Option<Self> {
        if (0x80..=0x9F).contains(&addr) {
            Some(PipeInAddress(addr))
        } else {
            None
        }
    }
}

/// Pipe-Out endpoint address.
///
/// FrontPanel Classic data port uses endpoint addresses 0xA0-0xBF for PipeOut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeOutAddress(pub u8);

impl PipeOutAddress {
    /// Create a new PipeOut address, validating the range (0xA0-0xBF).
    pub fn new(addr: u8) -> Option<Self> {
        if (0xA0..=0xBF).contains(&addr) {
            Some(PipeOutAddress(addr))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_in_address_valid_range() {
        assert!(WireInAddress::new(0x00).is_some());
        assert!(WireInAddress::new(0x1F).is_some());
        assert!(WireInAddress::new(0x20).is_none());
    }

    #[test]
    fn wire_out_address_valid_range() {
        assert!(WireOutAddress::new(0x20).is_some());
        assert!(WireOutAddress::new(0x3F).is_some());
        assert!(WireOutAddress::new(0x1F).is_none());
        assert!(WireOutAddress::new(0x40).is_none());
    }

    #[test]
    fn trigger_in_address_valid_range() {
        assert!(TriggerInAddress::new(0x40).is_some());
        assert!(TriggerInAddress::new(0x5F).is_some());
        assert!(TriggerInAddress::new(0x3F).is_none());
    }

    #[test]
    fn pipe_in_address_valid_range() {
        assert!(PipeInAddress::new(0x80).is_some());
        assert!(PipeInAddress::new(0x9F).is_some());
        assert!(PipeInAddress::new(0x7F).is_none());
    }

    #[test]
    fn pipe_out_address_valid_range() {
        assert!(PipeOutAddress::new(0xA0).is_some());
        assert!(PipeOutAddress::new(0xBF).is_some());
        assert!(PipeOutAddress::new(0x9F).is_none());
    }

    #[test]
    fn active_data_port_none_is_inactive() {
        let port = ActiveDataPort::None;
        assert!(!port.is_active());
    }

    #[test]
    fn active_data_port_classic_is_active() {
        let port = ActiveDataPort::Classic(ClassicPortInfo {
            wire_width: 32,
            trigger_width: 32,
            pipe_width: 32,
            register_address_width: 32,
            register_data_width: 32,
        });
        assert!(port.is_active());
    }
}
