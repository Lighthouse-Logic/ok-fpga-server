//! # Device Sensors
//!
//! This module provides Rust-friendly types for reading sensor data from
//! Opal Kelly FPGA devices. Sensors report values like temperature, voltage,
//! current, fan speed, and power consumption.
//!
//! The raw C struct `okTDeviceSensor` is converted to
//! [`DeviceSensor`](crate::sensors::DeviceSensor) via the `from_raw()` method.

use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// Sensor Type Enum
// ============================================================================

/// The type of measurement a sensor reports.
///
/// Corresponds to `okEDeviceSensorType` in the C API. Each type determines
/// the unit of the sensor's `value`, `min`, and `max` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorType {
    /// Invalid or unrecognized sensor type.
    Invalid,
    /// Boolean sensor (0 or 1).
    Bool,
    /// Raw integer value (no unit).
    Integer,
    /// Floating-point value (no unit).
    Float,
    /// Voltage in Volts.
    Voltage,
    /// Current in Amps.
    Current,
    /// Temperature in degrees Celsius.
    Temperature,
    /// Fan speed in RPM.
    FanRpm,
    /// Power in Watts.
    Power,
}

impl SensorType {
    /// Convert from the raw `okEDeviceSensorType` integer constant.
    ///
    /// The enum values are defined in `okFrontPanel.h`:
    /// - 0 = Invalid
    /// - 1 = Bool
    /// - 2 = Integer
    /// - 3 = Float
    /// - 4 = Voltage
    /// - 5 = Current
    /// - 6 = Temperature
    /// - 7 = FanRpm
    /// - 8 = Power
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => SensorType::Invalid,
            1 => SensorType::Bool,
            2 => SensorType::Integer,
            3 => SensorType::Float,
            4 => SensorType::Voltage,
            5 => SensorType::Current,
            6 => SensorType::Temperature,
            7 => SensorType::FanRpm,
            8 => SensorType::Power,
            _ => SensorType::Invalid,
        }
    }

    /// Returns the unit string for this sensor type (e.g., "V", "°C", "W").
    pub fn unit(&self) -> &'static str {
        match self {
            SensorType::Invalid => "",
            SensorType::Bool => "",
            SensorType::Integer => "",
            SensorType::Float => "",
            SensorType::Voltage => "V",
            SensorType::Current => "A",
            SensorType::Temperature => "°C",
            SensorType::FanRpm => "RPM",
            SensorType::Power => "W",
        }
    }
}

impl fmt::Display for SensorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorType::Invalid => write!(f, "Invalid"),
            SensorType::Bool => write!(f, "Boolean"),
            SensorType::Integer => write!(f, "Integer"),
            SensorType::Float => write!(f, "Float"),
            SensorType::Voltage => write!(f, "Voltage"),
            SensorType::Current => write!(f, "Current"),
            SensorType::Temperature => write!(f, "Temperature"),
            SensorType::FanRpm => write!(f, "Fan RPM"),
            SensorType::Power => write!(f, "Power"),
        }
    }
}

// ============================================================================
// DeviceSensor — Rust-friendly Sensor Data
// ============================================================================

/// A single sensor reading from an FPGA device.
///
/// This is the Rust-friendly version of `okTDeviceSensor` from the C API.
/// Each sensor has a name, type, current value, and valid range (min/max).
///
/// ## Example
///
/// ```
/// use ok_fpga_core::sensors::{DeviceSensor, SensorType};
///
/// let sensor = DeviceSensor {
///     id: 0,
///     sensor_type: SensorType::Temperature,
///     name: "FPGA Die Temperature".to_string(),
///     description: "Junction temperature of the FPGA die".to_string(),
///     value: 42.5,
///     min: 0.0,
///     max: 85.0,
///     step: 0.1,
/// };
///
/// assert_eq!(sensor.sensor_type.unit(), "°C");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSensor {
    /// Sensor index (0-based).
    pub id: i32,

    /// The type of measurement this sensor reports.
    pub sensor_type: SensorType,

    /// Human-readable sensor name (e.g., "FPGA Die Temperature").
    pub name: String,

    /// Human-readable description of what this sensor measures.
    pub description: String,

    /// Current sensor reading.
    pub value: f64,

    /// Minimum valid value for this sensor.
    pub min: f64,

    /// Maximum valid value for this sensor.
    pub max: f64,

    /// Measurement step/resolution.
    pub step: f64,
}

impl fmt::Display for DeviceSensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = self.sensor_type.unit();
        if unit.is_empty() {
            write!(f, "{}: {}", self.name, self.value)
        } else {
            write!(f, "{}: {} {}", self.name, self.value, unit)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_type_from_raw_round_trips() {
        for i in 0..=8 {
            let st = SensorType::from_raw(i);
            assert_ne!(format!("{}", st), "", "Type {} should have a display", i);
        }
    }

    #[test]
    fn invalid_sensor_type_for_unknown_value() {
        assert_eq!(SensorType::from_raw(99), SensorType::Invalid);
    }

    #[test]
    fn sensor_units_are_correct() {
        assert_eq!(SensorType::Voltage.unit(), "V");
        assert_eq!(SensorType::Temperature.unit(), "°C");
        assert_eq!(SensorType::Power.unit(), "W");
        assert_eq!(SensorType::FanRpm.unit(), "RPM");
    }

    #[test]
    fn sensor_display_includes_unit() {
        let sensor = DeviceSensor {
            id: 0,
            sensor_type: SensorType::Temperature,
            name: "Die Temp".to_string(),
            description: "".to_string(),
            value: 42.5,
            min: 0.0,
            max: 85.0,
            step: 0.1,
        };
        let display = format!("{}", sensor);
        assert!(display.contains("42.5"));
        assert!(display.contains("°C"));
    }
}
