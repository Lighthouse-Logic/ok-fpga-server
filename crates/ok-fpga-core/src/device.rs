//! # Device Types & Board Recognition
//!
//! This module defines the core domain types for FPGA devices managed by the
//! server. It includes:
//!
//! - [`FpgaDevice`](crate::device::FpgaDevice) — The main struct representing a connected FPGA device.
//! - [`DeviceState`](crate::device::DeviceState) — Tracks whether a device is connected, programmed, etc.
//! - [`BitfileRecord`](crate::device::BitfileRecord) — Metadata about the bitfile loaded on a device.
//! - [`is_supported_device`](crate::device::is_supported_device) — Board recognition logic from ADR-009.
//!
//! ## Ownership Model
//!
//! `FpgaDevice` is the central type that owns:
//! - The device serial number (unique identifier)
//! - Product metadata (name, ID, interface type)
//! - Current state (connected vs programmed)
//! - Bitfile record (if programmed)

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Board Recognition (ADR-009)
// ============================================================================

/// Set of supported Opal Kelly product IDs.
///
/// Only devices with these product IDs are accepted by the server. Devices
/// with other product IDs (e.g., older USB 2.0 boards) are logged and skipped.
///
/// See ADR-009 for the complete supported board matrix with interface types
/// and documentation links.
const SUPPORTED_PRODUCTS: &[i32] = &[
    60, // XEM8305AU15P
    57, // XEM8310AU25P
    45, 46, // XEM7320 (A75T, A200T)
    56, // XEM8320AU25P
    49, 50, 51, // XEM8350 (KU060, KU085, KU115)
    59, // XEM8370KU11P
    61, 62, // HUB1450 (AU10P, AU25P)
];

/// Device interface types from the FrontPanel C API.
///
/// These correspond to `OK_INTERFACE_*` constants in `okFrontPanel.h`.
/// We only support USB3, PCIe, and Gen3 — NOT USB2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceInterface {
    /// Unknown interface — should not normally occur.
    Unknown,
    /// USB 2.0 — NOT supported by this project.
    Usb2,
    /// PCI Express interface.
    Pcie,
    /// USB 3.0 (SuperSpeed).
    Usb3,
    /// Gen3 (USB 3.0 + additional capabilities).
    Gen3,
}

impl DeviceInterface {
    /// Convert from the raw `OK_INTERFACE_*` integer constant.
    ///
    /// The integer values are defined in `okFrontPanel.h`:
    /// - 0 = Unknown
    /// - 1 = USB2
    /// - 2 = PCIe
    /// - 3 = USB3
    /// - 4 = Gen3
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => DeviceInterface::Unknown,
            1 => DeviceInterface::Usb2,
            2 => DeviceInterface::Pcie,
            3 => DeviceInterface::Usb3,
            4 => DeviceInterface::Gen3,
            _ => DeviceInterface::Unknown,
        }
    }

    /// Returns `true` if this interface type is supported by the server.
    ///
    /// USB 2.0 devices are explicitly excluded (project constraint).
    pub fn is_supported(&self) -> bool {
        matches!(
            self,
            DeviceInterface::Usb3 | DeviceInterface::Pcie | DeviceInterface::Gen3
        )
    }
}

impl fmt::Display for DeviceInterface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceInterface::Unknown => write!(f, "Unknown"),
            DeviceInterface::Usb2 => write!(f, "USB 2.0"),
            DeviceInterface::Pcie => write!(f, "PCIe"),
            DeviceInterface::Usb3 => write!(f, "USB 3.0"),
            DeviceInterface::Gen3 => write!(f, "Gen3"),
        }
    }
}

/// USB speed enumeration from the FrontPanel C API.
///
/// Corresponds to `OK_USBSPEED_*` constants in `okFrontPanel.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsbSpeed {
    Unknown,
    Full,
    High,
    Super,
    SuperGen1x2,
    SuperGen2,
    SuperGen2x2,
}

impl UsbSpeed {
    /// Convert from the raw `OK_USBSPEED_*` integer constant.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => UsbSpeed::Unknown,
            1 => UsbSpeed::Full,
            2 => UsbSpeed::High,
            3 => UsbSpeed::Super,
            4 => UsbSpeed::SuperGen1x2,
            5 => UsbSpeed::SuperGen2,
            6 => UsbSpeed::SuperGen2x2,
            _ => UsbSpeed::Unknown,
        }
    }
}

impl fmt::Display for UsbSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsbSpeed::Unknown => write!(f, "Unknown"),
            UsbSpeed::Full => write!(f, "Full Speed (1.5 Mbps)"),
            UsbSpeed::High => write!(f, "High Speed (480 Mbps)"),
            UsbSpeed::Super => write!(f, "SuperSpeed (5 Gbps)"),
            UsbSpeed::SuperGen1x2 => write!(f, "SuperSpeed Gen1x2 (10 Gbps)"),
            UsbSpeed::SuperGen2 => write!(f, "SuperSpeed Gen2 (10 Gbps)"),
            UsbSpeed::SuperGen2x2 => write!(f, "SuperSpeed Gen2x2 (20 Gbps)"),
        }
    }
}

/// Data port type for a device.
///
/// After FPGA configuration, the device exposes either a Classic or AXI
/// data port depending on the gateware and device family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataportType {
    /// Unknown or not yet determined.
    Unknown,
    /// Classic FrontPanel data port (wires, triggers, pipes, registers).
    Legacy,
    /// AXI data port (AXI-Lite, AXI-Full, AXI-Stream).
    Axi,
}

impl DataportType {
    /// Convert from the raw `OK_DATAPORT_*` integer constant.
    ///
    /// - 0 = Unknown
    /// - 1 = Legacy (Classic)
    /// - 2 = AXI
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => DataportType::Unknown,
            1 => DataportType::Legacy,
            2 => DataportType::Axi,
            _ => DataportType::Unknown,
        }
    }
}

impl fmt::Display for DataportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataportType::Unknown => write!(f, "Unknown"),
            DataportType::Legacy => write!(f, "Classic"),
            DataportType::Axi => write!(f, "AXI"),
        }
    }
}

/// FPGA vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FpgaVendor {
    Unknown,
    Xilinx,
    Intel,
}

impl FpgaVendor {
    /// Convert from raw `okEFPGAVendor` value.
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => FpgaVendor::Unknown,
            1 => FpgaVendor::Xilinx,
            2 => FpgaVendor::Intel,
            _ => FpgaVendor::Unknown,
        }
    }
}

// ============================================================================
// Device Info (populated from okTDeviceInfo)
// ============================================================================

/// Rust-friendly representation of `okTDeviceInfo` from the C API.
///
/// This struct contains all the metadata about a connected FPGA device,
/// extracted from the C struct and converted to proper Rust types.
///
/// ## Usage
///
/// A `DeviceInfo` is created by calling `DeviceInfo::from_raw()` with the
/// raw `okTDeviceInfo` struct returned by `okFrontPanel_GetDeviceInfoWithSize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The hardware serial number (unique per device, max 10 characters).
    pub serial: String,

    /// The product name (e.g., "XEM8320-AU25P").
    pub product_name: String,

    /// The product ID (matches `OK_PRODUCT_*` constants).
    pub product_id: i32,

    /// The device interface type (USB3, PCIe, Gen3, etc.).
    pub device_interface: DeviceInterface,

    /// The USB speed (if applicable).
    pub usb_speed: UsbSpeed,

    /// Firmware major version.
    pub device_major_version: i32,

    /// Firmware minor version.
    pub device_minor_version: i32,

    /// Wire endpoint width in bits.
    pub wire_width: i32,

    /// Trigger endpoint width in bits.
    pub trigger_width: i32,

    /// Pipe endpoint width in bits.
    pub pipe_width: i32,

    /// Register address width in bits.
    pub register_address_width: i32,

    /// Register data width in bits.
    pub register_data_width: i32,

    /// The FPGA vendor (Xilinx, Intel, etc.).
    pub fpga_vendor: FpgaVendor,

    /// The data port type (Classic or AXI).
    pub dataport: DataportType,

    /// The device ID string (user-settable, max 32 characters).
    pub device_id: String,
}

// ============================================================================
// Bitfile Record
// ============================================================================

/// Metadata about a bitfile that has been loaded onto an FPGA.
///
/// When a user calls `ConfigureFPGA` (REST or gRPC), the server creates a
/// `BitfileRecord` to track what bitfile is currently loaded and who loaded it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitfileRecord {
    /// The filename of the bitfile (e.g., "design_top.bit").
    pub filename: String,

    /// Optional human-readable description.
    pub description: Option<String>,

    /// When the bitfile was programmed.
    pub programmed_at: DateTime<Utc>,

    /// Which user programmed this bitfile (user ID string).
    pub programmed_by: String,
}

// ============================================================================
// Device State
// ============================================================================

/// The current state of a device in the server's device registry.
///
/// ## State Transitions (ADR-002)
///
/// ```text
/// Connected (no bitfile)
///     │
///     ▼ ConfigureFPGA(bitfile)
/// Programmed (bitfile loaded, resources can be created)
///     │
///     ▼ Device disconnected
/// [Removed from registry]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceState {
    /// The device is connected but no bitfile has been loaded yet.
    /// The device appears in the device list but cannot have resources.
    Connected,

    /// A bitfile has been loaded onto the FPGA. Resources can now be created
    /// and acquired by users.
    Programmed {
        /// Metadata about the loaded bitfile.
        bitfile: BitfileRecord,
    },
}

impl DeviceState {
    /// Returns `true` if the device is in the `Programmed` state.
    pub fn is_programmed(&self) -> bool {
        matches!(self, DeviceState::Programmed { .. })
    }

    /// Returns the bitfile record if the device is programmed, or `None`.
    pub fn bitfile(&self) -> Option<&BitfileRecord> {
        match self {
            DeviceState::Programmed { bitfile } => Some(bitfile),
            _ => None,
        }
    }
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceState::Connected => write!(f, "Connected (not programmed)"),
            DeviceState::Programmed { bitfile } => {
                write!(f, "Programmed ({})", bitfile.filename)
            }
        }
    }
}

// ============================================================================
// FpgaDevice — The Main Device Struct
// ============================================================================

/// Represents a connected and recognized FPGA device.
///
/// This is the central domain type in the core layer. Each physical FPGA
/// that passes board recognition (ADR-009) and USB 2.0 filtering (ADR-002)
/// gets an `FpgaDevice` instance in the device registry.
///
/// ## Lifecycle
///
/// 1. Created when `OnDeviceAdded` fires and the device passes filtering
/// 2. State transitions from `Connected` → `Programmed` when a bitfile is loaded
/// 3. Removed from the registry when `OnDeviceRemoved` fires
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpgaDevice {
    /// Complete device information from `okTDeviceInfo`.
    pub info: DeviceInfo,

    /// Current device state (connected or programmed).
    pub state: DeviceState,
}

impl FpgaDevice {
    /// Create a new `FpgaDevice` in the `Connected` (not yet programmed) state.
    pub fn new(info: DeviceInfo) -> Self {
        FpgaDevice {
            info,
            state: DeviceState::Connected,
        }
    }

    /// Returns the device serial number (the primary key for device lookup).
    pub fn serial(&self) -> &str {
        &self.info.serial
    }

    /// Returns the product name (e.g., "XEM8320-AU25P").
    pub fn product_name(&self) -> &str {
        &self.info.product_name
    }

    /// Returns `true` if the device is currently programmed with a bitfile.
    pub fn is_programmed(&self) -> bool {
        self.state.is_programmed()
    }
}

impl fmt::Display for FpgaDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}) [{}] — {}",
            self.info.product_name, self.info.serial, self.info.device_interface, self.state
        )
    }
}

// ============================================================================
// Board Recognition Functions
// ============================================================================

/// Check if a device with the given product ID and interface type is supported.
///
/// A device is supported if:
/// 1. Its interface type is USB3, PCIe, or Gen3 (NOT USB2)
/// 2. Its product ID is in the `SUPPORTED_PRODUCTS` allow-list (ADR-009)
///
/// ## Example
///
/// ```
/// use ok_fpga_core::device::{is_supported_device, DeviceInterface};
///
/// // XEM8320AU25P on USB3 — supported
/// assert!(is_supported_device(56, DeviceInterface::Usb3));
///
/// // XEM8320AU25P on USB2 — NOT supported (wrong interface)
/// assert!(!is_supported_device(56, DeviceInterface::Usb2));
///
/// // Unknown product on USB3 — NOT supported (wrong product)
/// assert!(!is_supported_device(999, DeviceInterface::Usb3));
/// ```
pub fn is_supported_device(product_id: i32, interface: DeviceInterface) -> bool {
    interface.is_supported() && SUPPORTED_PRODUCTS.contains(&product_id)
}

/// Returns a human-readable board name for a product ID, or `None` if unknown.
///
/// This is used for logging and display purposes. The mapping is derived from
/// the `OK_PRODUCT_*` constants in `okFrontPanel.h`.
pub fn board_name_for_product(product_id: i32) -> Option<&'static str> {
    match product_id {
        45 => Some("XEM7320-A75T"),
        46 => Some("XEM7320-A200T"),
        49 => Some("XEM8350-KU060"),
        50 => Some("XEM8350-KU085"),
        51 => Some("XEM8350-KU115"),
        56 => Some("XEM8320-AU25P"),
        57 => Some("XEM8310-AU25P"),
        59 => Some("XEM8370-KU11P"),
        60 => Some("XEM8305-AU15P"),
        61 => Some("SZG-HUB1450-AU10P"),
        62 => Some("SZG-HUB1450-AU25P"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_usb3_board_is_accepted() {
        assert!(is_supported_device(56, DeviceInterface::Usb3));
    }

    #[test]
    fn supported_gen3_board_is_accepted() {
        assert!(is_supported_device(56, DeviceInterface::Gen3));
    }

    #[test]
    fn supported_pcie_board_is_accepted() {
        assert!(is_supported_device(49, DeviceInterface::Pcie));
    }

    #[test]
    fn usb2_board_is_rejected() {
        assert!(!is_supported_device(56, DeviceInterface::Usb2));
    }

    #[test]
    fn unknown_product_is_rejected() {
        assert!(!is_supported_device(999, DeviceInterface::Usb3));
    }

    #[test]
    fn all_supported_products_are_recognized() {
        for &pid in SUPPORTED_PRODUCTS {
            assert!(
                board_name_for_product(pid).is_some(),
                "Product ID {} should have a board name",
                pid
            );
        }
    }

    #[test]
    fn device_state_transitions() {
        let info = DeviceInfo {
            serial: "ABC1234567".to_string(),
            product_name: "XEM8320-AU25P".to_string(),
            product_id: 56,
            device_interface: DeviceInterface::Usb3,
            usb_speed: UsbSpeed::Super,
            device_major_version: 1,
            device_minor_version: 0,
            wire_width: 32,
            trigger_width: 32,
            pipe_width: 32,
            register_address_width: 32,
            register_data_width: 32,
            fpga_vendor: FpgaVendor::Xilinx,
            dataport: DataportType::Legacy,
            device_id: "MyDevice".to_string(),
        };

        let mut device = FpgaDevice::new(info);
        assert!(!device.is_programmed());
        assert_eq!(device.serial(), "ABC1234567");

        // Simulate programming
        device.state = DeviceState::Programmed {
            bitfile: BitfileRecord {
                filename: "design.bit".to_string(),
                description: Some("Test bitfile".to_string()),
                programmed_at: Utc::now(),
                programmed_by: "alice".to_string(),
            },
        };
        assert!(device.is_programmed());
        assert_eq!(device.state.bitfile().unwrap().filename, "design.bit");
    }

    #[test]
    fn device_display_formatting() {
        let info = DeviceInfo {
            serial: "ABC123".to_string(),
            product_name: "XEM8320-AU25P".to_string(),
            product_id: 56,
            device_interface: DeviceInterface::Usb3,
            usb_speed: UsbSpeed::Super,
            device_major_version: 1,
            device_minor_version: 0,
            wire_width: 32,
            trigger_width: 32,
            pipe_width: 32,
            register_address_width: 32,
            register_data_width: 32,
            fpga_vendor: FpgaVendor::Xilinx,
            dataport: DataportType::Legacy,
            device_id: "".to_string(),
        };

        let device = FpgaDevice::new(info);
        let display = format!("{}", device);
        assert!(display.contains("XEM8320"));
        assert!(display.contains("ABC123"));
        assert!(display.contains("USB 3.0"));
    }
}
