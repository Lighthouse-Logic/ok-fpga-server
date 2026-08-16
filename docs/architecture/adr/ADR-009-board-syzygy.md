# ADR-009: Supported Board Matrix & SYZYGY Port Discovery

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The project goals specify a fixed set of supported Opal Kelly FPGA boards, each with specific device settings and sensor pages. The server must:

1. Recognize supported boards by their `OK_PRODUCT_*` constant from `okTDeviceInfo.productID`
2. Filter out unsupported boards (log a warning)
3. Auto-discover SYZYGY ports via the `DeviceSettings` API (`SYZYGYn_*` keys)
4. Query board-specific device settings and sensors

## Decision

### Supported Board Matrix

| Board | Product Constant(s) | `productID` Value(s) | Interface | Device Settings Reference | Device Sensors Reference |
|-------|---------------------|---------------------|-----------|--------------------------|------------------------|
| **XEM8305** | `OK_PRODUCT_XEM8305AU15P` | 60 | USB3 / Gen3 | [docs.opalkelly.com/szg-hub1450/device-sensors/](https://docs.opalkelly.com/szg-hub1450/device-sensors/) | Same |
| **XEM8310** | `OK_PRODUCT_XEM8310AU25P` | 57 | USB3 / Gen3 | [docs.opalkelly.com/xem8310/device-settings/](https://docs.opalkelly.com/xem8310/device-settings/) | [docs.opalkelly.com/xem8310/device-sensors/](https://docs.opalkelly.com/xem8310/device-sensors/) |
| **XEM7320** | `OK_PRODUCT_XEM7320A75T`, `OK_PRODUCT_XEM7320A200T` | 45, 46 | USB3 | [docs.opalkelly.com/xem7320/device-settings/](https://docs.opalkelly.com/xem7320/device-settings/) | [docs.opalkelly.com/xem7320/device-sensors/](https://docs.opalkelly.com/xem7320/device-sensors/) |
| **XEM8320** | `OK_PRODUCT_XEM8320AU25P` | 56 | USB3 / Gen3 | [docs.opalkelly.com/xem8320/device-settings/](https://docs.opalkelly.com/xem8320/device-settings/) | [docs.opalkelly.com/xem8320/device-sensors/](https://docs.opalkelly.com/xem8320/device-sensors/) |
| **XEM8350** | `OK_PRODUCT_XEM8350KU060`, `KU085`, `KU115` | 49, 50, 51 | USB3 / PCIe | [docs.opalkelly.com/xem8350/device-settings/](https://docs.opalkelly.com/xem8350/device-settings/) | [docs.opalkelly.com/xem8350/device-sensors/](https://docs.opalkelly.com/xem8350/device-sensors/) |
| **XEM8370** | `OK_PRODUCT_XEM8370KU11P` | 59 | USB3 / Gen3 | [docs.opalkelly.com/xem8370/device-settings/](https://docs.opalkelly.com/xem8370/device-settings/) | [docs.opalkelly.com/xem8370/device-sensors/](https://docs.opalkelly.com/xem8370/device-sensors/) |
| **HUB1450** | `OK_PRODUCT_SZGHUB1450AU10P`, `OK_PRODUCT_SZGHUB1450AU25P` | 61, 62 | USB3 / Gen3 | [docs.opalkelly.com/szg-hub1450/device-settings/](https://docs.opalkelly.com/szg-hub1450/device-settings/) | [docs.opalkelly.com/szg-hub1450/device-sensors/](https://docs.opalkelly.com/szg-hub1450/device-sensors/) |

### Board Recognition in Code

```rust
/// Set of supported product IDs.
const SUPPORTED_PRODUCTS: &[i32] = &[
    60,         // XEM8305AU15P
    57,         // XEM8310AU25P
    45, 46,     // XEM7320 (A75T, A200T)
    56,         // XEM8320AU25P
    49, 50, 51, // XEM8350 (KU060, KU085, KU115)
    59,         // XEM8370KU11P
    61, 62,     // HUB1450 (AU10P, AU25P)
];

fn is_supported_device(info: &okTDeviceInfo) -> bool {
    // Must be USB3, PCIe, or Gen3 — NOT USB2
    let supported_interface = matches!(
        info.deviceInterface,
        OK_INTERFACE_USB3 | OK_INTERFACE_PCIE | OK_INTERFACE_GEN3
    );
    
    supported_interface && SUPPORTED_PRODUCTS.contains(&info.productID)
}
```

Devices that fail this check are logged and skipped (per ADR-002).

### SYZYGY Port Auto-Discovery

SYZYGY ports are discovered by probing the `DeviceSettings` API with the standardized `SYZYGYn_*` keys. The number of ports is auto-detected by iterating `n` from 0 until a query returns an error (indicating no more ports).

#### Discovery Algorithm

```
1. Obtain DeviceSettings handle: okFrontPanel_GetDeviceSettings(dev, settings)
2. For n = 0, 1, 2, ... :
   a. Query: okDeviceSettings_GetInt(settings, "SYZYGY{n}_PORT_STATUS", &value)
   b. If error (key not found) → stop; n ports discovered
   c. If ok_NoError:
      - Read SYZYGYn_PORT_STATUS       (INT32: voltage in 10s of mV, 0 = unpowered)
      - Read SYZYGYn_MANUFACTURER_NAME  (STRING)
      - Read SYZYGYn_PRODUCT_NAME       (STRING)
      - Read SYZYGYn_PRODUCT_MODEL      (STRING)
      - Read SYZYGYn_PRODUCT_VERSION    (STRING)
      - Read SYZYGYn_SERIAL_NUMBER      (STRING)
      - Record port info
3. Return list of SyzygyPort structs
```

#### SYZYGY Port Rust Type

```rust
pub struct SyzygyPort {
    /// Port index (0-based)
    pub index: u32,
    
    /// SmartVIO voltage in millivolts (e.g., 3300 = 3.3V). 0 = unpowered.
    pub port_status_mv: u32,
    
    /// Whether a peripheral is detected (port_status_mv > 0)
    pub is_populated: bool,
    
    /// Peripheral DNA fields (empty strings if no peripheral connected)
    pub manufacturer_name: String,
    pub product_name: String,
    pub product_model: String,
    pub product_version: String,
    pub serial_number: String,
}
```

#### SYZYGY Key Format

The keys use the literal format `SYZYGYn_*` where `n` is the zero-based port index. For example:
- `SYZYGY0_PORT_STATUS`
- `SYZYGY0_MANUFACTURER_NAME`
- `SYZYGY1_PORT_STATUS`
- etc.

### Device Settings API Integration

The `DeviceSettings` C API is accessed via:

```c
okDeviceSettings_HANDLE settings = okDeviceSettings_Construct();
ok_ErrorCode ec = okFrontPanel_GetDeviceSettings(dev_handle, settings);

// Read string setting
char buf[256];
ec = okDeviceSettings_GetString(settings, "SYZYGY0_PRODUCT_NAME", 256, buf);

// Read integer setting
UINT32 value;
ec = okDeviceSettings_GetInt(settings, "SYZYGY0_PORT_STATUS", &value);

// List all available settings
okDeviceSettingNames_HANDLE names = okDeviceSettingNames_Construct();
ec = okDeviceSettings_List(settings, names);
int count = okDeviceSettingNames_GetCount(names);
for (int i = 0; i < count; i++) {
    const char* name = okDeviceSettingNames_Get(names, i);
    // ...
}
okDeviceSettingNames_Destruct(names);
okDeviceSettings_Destruct(settings);
```

The Rust FFI layer wraps this in a safe `DeviceSettings` struct with methods like:
- `list_settings() -> Vec<String>`
- `get_string(key: &str) -> Result<String>`
- `get_int(key: &str) -> Result<u32>`
- `set_string(key: &str, value: &str) -> Result<()>`
- `set_int(key: &str, value: u32) -> Result<()>`

### Device Sensors API Integration

The `DeviceSensors` C API provides read-only sensor data:

```c
okDeviceSensors_HANDLE sensors = okDeviceSensors_Construct();
ok_ErrorCode ec = okFrontPanel_GetDeviceSensors(dev_handle, sensors);
int count = okDeviceSensors_GetSensorCount(sensors);
for (int i = 0; i < count; i++) {
    okTDeviceSensor sensor = okDeviceSensors_GetSensor(sensors, i);
    // sensor.name, sensor.type, sensor.value, sensor.min, sensor.max, ...
}
okDeviceSensors_Destruct(sensors);
```

Sensor types map to the `okEDeviceSensorType` enum:

| Type | Enum Value | Unit |
|------|-----------|------|
| Bool | `okDEVICESENSOR_BOOL` | true/false |
| Integer | `okDEVICESENSOR_INTEGER` | — |
| Float | `okDEVICESENSOR_FLOAT` | — |
| Voltage | `okDEVICESENSOR_VOLTAGE` | Volts |
| Current | `okDEVICESENSOR_CURRENT` | Amps |
| Temperature | `okDEVICESENSOR_TEMPERATURE` | °C |
| Fan RPM | `okDEVICESENSOR_FAN_RPM` | RPM |
| Power | `okDEVICESENSOR_POWER` | Watts |

## Consequences

### Positive
- **Exhaustive board support** — all boards listed in the project goals are covered with exact product ID mappings
- **Auto-discovery** — SYZYGY ports are discovered without hardcoding per-board port counts
- **Complete settings/sensors** — full access to board-specific metadata via the generic DeviceSettings/DeviceSensors API
- **Future-proof** — adding new boards requires only adding a product ID to the allow-list

### Negative
- **Board-specific settings knowledge** — while the API is generic, knowing *which* settings keys exist on each board requires consulting the board-specific documentation pages
- **SYZYGY probe overhead** — iterating settings keys until failure adds a small startup cost per device (negligible in practice)

## References

- [Project Goals — Supported Boards](../../ok-FPGA-Server_Project-goals.txt) — lines 85-93
- [Project Goals — SYZYGY](../../ok-FPGA-Server_Project-goals.txt) — lines 32-81
- [okTDeviceSensor struct](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 670-679
- [DeviceSettings C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 866-883
- [DeviceSensors C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 847-854
- [OK_PRODUCT_* constants](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 376-440
