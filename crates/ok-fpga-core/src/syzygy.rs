//! # SYZYGY Port Discovery
//!
//! This module implements the SYZYGY peripheral port auto-discovery algorithm
//! from ADR-009. SYZYGY is a standardized I/O connector used on Opal Kelly
//! FPGA boards to attach peripheral modules (ADCs, DACs, sensors, etc.).
//!
//! ## Discovery Algorithm
//!
//! SYZYGY ports are discovered by probing the device's settings API:
//!
//! 1. For port index `n` = 0, 1, 2, ...:
//!    - Query `SYZYGYn_PORT_STATUS` (integer setting)
//!    - If the key doesn't exist → stop; `n` ports were found
//!    - If it exists, read the port's DNA fields (manufacturer, product, etc.)
//! 2. Return a list of [`SyzygyPort`](crate::syzygy::SyzygyPort) structs

use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// SYZYGY Port Struct
// ============================================================================

/// A SYZYGY peripheral port discovered on an FPGA device.
///
/// Each port may optionally have a peripheral module plugged in. The
/// `is_populated` flag indicates whether a module is detected (based on
/// whether the port is powered, i.e., `port_status_mv > 0`).
///
/// ## DNA Fields
///
/// When a peripheral is plugged in, its "DNA" (identification data) is
/// available through the manufacturer name, product name, model, version,
/// and serial number fields. These are read from the peripheral's EEPROM
/// via the SYZYGY standard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyzygyPort {
    /// Port index (0-based).
    pub index: u32,

    /// SmartVIO voltage in millivolts (e.g., 3300 = 3.3V).
    /// A value of 0 means the port is unpowered (no peripheral detected).
    pub port_status_mv: u32,

    /// Whether a peripheral module is detected on this port.
    /// This is `true` when `port_status_mv > 0`.
    pub is_populated: bool,

    /// Peripheral manufacturer name (empty if no peripheral connected).
    pub manufacturer_name: String,

    /// Peripheral product name (empty if no peripheral connected).
    pub product_name: String,

    /// Peripheral product model (empty if no peripheral connected).
    pub product_model: String,

    /// Peripheral product version (empty if no peripheral connected).
    pub product_version: String,

    /// Peripheral serial number (empty if no peripheral connected).
    pub serial_number: String,
}

impl SyzygyPort {
    /// Create a new SyzygyPort from discovered data.
    ///
    /// The `is_populated` flag is automatically set based on `port_status_mv`.
    pub fn new(index: u32, port_status_mv: u32) -> Self {
        SyzygyPort {
            index,
            port_status_mv,
            is_populated: port_status_mv > 0,
            manufacturer_name: String::new(),
            product_name: String::new(),
            product_model: String::new(),
            product_version: String::new(),
            serial_number: String::new(),
        }
    }

    /// Returns the SmartVIO voltage in Volts (e.g., 3.3).
    /// Returns 0.0 if the port is unpowered.
    pub fn voltage(&self) -> f64 {
        self.port_status_mv as f64 / 1000.0
    }

    /// Returns the device settings key for a given port index and field.
    ///
    /// The key format is `SYZYGYn_FIELD_NAME` where `n` is the port index.
    ///
    /// ## Example
    ///
    /// ```
    /// use ok_fpga_core::syzygy::SyzygyPort;
    ///
    /// assert_eq!(SyzygyPort::settings_key(0, "PORT_STATUS"), "SYZYGY0_PORT_STATUS");
    /// assert_eq!(SyzygyPort::settings_key(1, "MANUFACTURER_NAME"), "SYZYGY1_MANUFACTURER_NAME");
    /// ```
    pub fn settings_key(index: u32, field: &str) -> String {
        format!("SYZYGY{}_{}", index, field)
    }
}

impl fmt::Display for SyzygyPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_populated {
            write!(
                f,
                "SYZYGY[{}]: {} {} ({}V) — SN: {}",
                self.index,
                self.manufacturer_name,
                self.product_name,
                self.voltage(),
                self.serial_number
            )
        } else {
            write!(f, "SYZYGY[{}]: empty (unpowered)", self.index)
        }
    }
}

/// The standard device settings key names used for SYZYGY port discovery.
///
/// These are the field suffixes appended to `SYZYGYn_` to form the full key.
/// For example, port 0's manufacturer name is `SYZYGY0_MANUFACTURER_NAME`.
pub const SYZYGY_FIELD_PORT_STATUS: &str = "PORT_STATUS";
pub const SYZYGY_FIELD_MANUFACTURER_NAME: &str = "MANUFACTURER_NAME";
pub const SYZYGY_FIELD_PRODUCT_NAME: &str = "PRODUCT_NAME";
pub const SYZYGY_FIELD_PRODUCT_MODEL: &str = "PRODUCT_MODEL";
pub const SYZYGY_FIELD_PRODUCT_VERSION: &str = "PRODUCT_VERSION";
pub const SYZYGY_FIELD_SERIAL_NUMBER: &str = "SERIAL_NUMBER";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_key_format() {
        assert_eq!(
            SyzygyPort::settings_key(0, SYZYGY_FIELD_PORT_STATUS),
            "SYZYGY0_PORT_STATUS"
        );
        assert_eq!(
            SyzygyPort::settings_key(2, SYZYGY_FIELD_MANUFACTURER_NAME),
            "SYZYGY2_MANUFACTURER_NAME"
        );
    }

    #[test]
    fn unpowered_port_is_not_populated() {
        let port = SyzygyPort::new(0, 0);
        assert!(!port.is_populated);
        assert_eq!(port.voltage(), 0.0);
    }

    #[test]
    fn powered_port_is_populated() {
        let mut port = SyzygyPort::new(0, 3300);
        port.manufacturer_name = "Opal Kelly".to_string();
        port.product_name = "SZG-DAC-AD9116".to_string();

        assert!(port.is_populated);
        assert!((port.voltage() - 3.3).abs() < 0.001);
    }

    #[test]
    fn display_populated_port() {
        let mut port = SyzygyPort::new(1, 1800);
        port.manufacturer_name = "ACME".to_string();
        port.product_name = "ADC-16".to_string();
        port.serial_number = "SN001".to_string();

        let display = format!("{}", port);
        assert!(display.contains("SYZYGY[1]"));
        assert!(display.contains("ACME"));
        assert!(display.contains("SN001"));
    }

    #[test]
    fn display_empty_port() {
        let port = SyzygyPort::new(0, 0);
        let display = format!("{}", port);
        assert!(display.contains("empty"));
    }
}
