// chapters/overview/2-3-devices.typ — Section 2.3: Devices.
// ref: ADR-001, ADR-002 (hot-plug), ADR-004, ADR-005, ADR-013..018 (device scanner),
// ADR-034 D63/D70/SD-05/P16 (dataport normalisation).
#import "../../lib.typ": *

== Devices <ov-devices>

=== Supported models, filtering and hot-plug <ov-dev-discovery>
// ref: rules §3 (supported 7 models, no USB 2.0); ADR-002; ADR-034 D63, D70.
ok-fpga-server supports seven Opal Kelly board families over USB 3.0, PCIe and
FP-Gen3-USB using the FrontPanel 6.1.0 C API:
*XEM8305*, *XEM8310*, *XEM7320*, *XEM8320*, *XEM8350*, *XEM8370* and
*HUB1450*.

- *USB 2.0 exclusion:* Devices reporting `DEVICE_INTERFACE_USB2` are unsupported
  and MUST be omitted from device listings.
- *Unknown data port exclusion:* Devices reporting `OK_DATAPORT_UNKNOWN` (`0`)
  in `okTDeviceInfo.dataport` are unsupported, MUST be omitted from
  `GET /devices` / `ListDevices`, and MUST return `404 Not Found` / `NOT_FOUND`
  on any `/devices/{serial}/*` route or RPC (@res-interfaces).
- *Hot-plug detection:* The server monitors physical device connections and
  disconnections. When a supported FPGA is plugged in, the server opens its
  handle and emits `DeviceAdded`. When a device is unplugged, the server
  performs forced resource cleanup for that serial (`ResourceUnlocked` for any
  locked shared resources, then `ResourceDeleted` with
  `reason = "device_removed"`, @res-lifecycle-device) and emits `DeviceRemoved`.

=== Device listing and device information <ov-dev-info>
// ref: openapi.yaml /devices, /devices/{serial}, DeviceInfo; proto ListDevices, GetDeviceInfo.

#table(
  columns: (auto, 1.2fr, 1.3fr, 1.8fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [List devices], [`GET /devices`], [`ListDevices(ListDevicesRequest)`], [`{ "devices": [DeviceInfo, ...] }` / `ListDevicesResponse`],
  [Get one device], [`GET /devices/{serial}`], [`GetDeviceInfo(DeviceRequest)`], [`DeviceInfo` / `DeviceInfoResponse` (`404` / `NOT_FOUND` if absent)],
)

The `serial` path parameter (or `DeviceRequest.serial`) is the 10-character
Opal Kelly serial number (for example `"2149000ABC"`). `DeviceInfo` maps the
fields of FrontPanel's `okTDeviceInfo` structure#footnote[Opal Kelly,
_FrontPanel SDK: `okTDeviceInfo` structure_, `okFrontPanel.h` (FrontPanel
6.1.0).] plus the server's current bitfile record:

#table(
  columns: (1.5fr, 1.2fr, 1.4fr, 2fr),
  [Field], [REST type], [Proto (`DeviceInfo`)], [Meaning],
  [`serial`], [string], [1 `string`], [10-character factory serial number.],
  [`device_id`], [string], [2 `string`], [User-configurable device ID string (up to 32 bytes).],
  [`product_name`], [string], [3 `string`], [Model name, e.g. `"XEM8320-AU25P"`.],
  [`product_id`], [integer], [4 `int32`], [`OK_PRODUCT_*` numeric constant.],
  [`device_interface`], [`unknown` | `usb2` | `pcie` | `usb3` | `gen3`], [5 `DeviceInterface`], [Host physical interface (`usb2` is filtered out).],
  [`usb_speed`], [`unknown` | `full` | `high` | `super` | `ss_gen1` | `ss_gen2` | `ss_gen2x2`], [6 `USBSpeed`], [Negotiated USB link speed.],
  [`device_major_version`], [integer], [7 `int32`], [Device firmware major version.],
  [`device_minor_version`], [integer], [8 `int32`], [Device firmware minor version.],
  [`wire_width`], [integer], [9 `int32`], [Classic wire endpoint width in bits (`32`).],
  [`trigger_width`], [integer], [10 `int32`], [Classic trigger endpoint width in bits (`32`).],
  [`pipe_width`], [integer], [11 `int32`], [Classic pipe endpoint width in bits (`32`).],
  [`register_address_width`], [integer], [12 `int32`], [`okRegisterBridge` address width in bits (`32`).],
  [`register_data_width`], [integer], [13 `int32`], [`okRegisterBridge` data width in bits (`32`).],
  [`fpga_vendor`], [`unknown` | `xilinx` | `intel`], [14 `FPGAVendor`], [FPGA silicon vendor.],
  [`dataport`], [`legacy` | `axi`], [15 `DataportType`], [Host transport type (`OK_DATAPORT_LEGACY` vs `OK_DATAPORT_AXI`, @ov-cfg-bitfile-dataport).],
  [`is_front_panel_enabled`], [boolean], [16 `bool`], [`true` when the loaded design enables the FrontPanel host interface.],
  [`bitfile`], [`BitfileInfo` (optional)], [17 `optional BitfileInfo`], [Present when the FPGA is currently programmed (@ov-cfg-bitfile-dataport).],
)

=== Device sensors <ov-dev-sensors>
// ref: openapi.yaml /devices/{serial}/sensors, DeviceSensor; proto GetDeviceSensors.

#table(
  columns: (auto, 1.3fr, 1.3fr, 1.7fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [Read sensors], [`GET /devices/{serial}/sensors`], [`GetDeviceSensors(DeviceRequest)`], [`{ "sensors": [DeviceSensor, ...] }` / `DeviceSensorsResponse`],
)

Reads all hardware monitoring sensors exposed by `okCDeviceSensors` on the
module (`404` / `NOT_FOUND` if `<serial>` is unknown). Each `DeviceSensor`
entry contains:
- `id` (integer / proto `int32` 1): numeric sensor ID;
- `sensor_type` (string / proto `SensorType` 2): `invalid`, `bool`, `integer`,
  `float`, `voltage`, `current`, `temperature`, `fan_rpm` or `power`;
- `name` (3) and `description` (4): sensor label and units/description;
- `min` (5), `max` (6), `step` (7) and current `value` (8) as floating-point
  numbers (`double`).

=== Device settings <ov-dev-settings>
// ref: openapi.yaml /devices/{serial}/settings, /settings/{key}; proto GetDeviceSettings, GetDeviceSetting, SetDeviceSetting.

#table(
  columns: (auto, 1.4fr, 1.4fr, 1.5fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [List settings], [`GET /devices/{serial}/settings`], [`GetDeviceSettings(DeviceRequest)`], [`{ "settings": [DeviceSetting, ...] }` / `DeviceSettingsResponse`],
  [Get setting], [`GET /devices/{serial}/settings/{key}`], [`GetDeviceSetting(GetDeviceSettingRequest)`], [`DeviceSetting` / `DeviceSettingResponse`],
  [Set setting], [`PUT /devices/{serial}/settings/{key}`], [`SetDeviceSetting(SetDeviceSettingRequest)`], [`StatusResponse`],
)

Reads or updates non-volatile and runtime settings stored via FrontPanel's
`okCDeviceSettings` API (`404` / `NOT_FOUND` if the device or key does not
exist). On REST, `DeviceSetting` and `DeviceSettingValue` carry `value_type`
(`"string"` or `"integer"`), `string_value` and `int_value`. On gRPC,
`DeviceSetting` uses `oneof value { string string_value = 2; uint32 int_value = 3; }`.

=== SYZYGY peripheral discovery and DNA <ov-dev-syzygy>
// ref: openapi.yaml /devices/{serial}/syzygy, SyzygyPort; proto GetSyzygyPorts.

#table(
  columns: (auto, 1.3fr, 1.3fr, 1.7fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [Discover SYZYGY ports], [`GET /devices/{serial}/syzygy`], [`GetSyzygyPorts(DeviceRequest)`], [`{ "ports": [SyzygyPort, ...] }` / `SyzygyPortsResponse`],
)

Probes the device's `SYZYGYn_*` device settings keys (`SYZYGYn_PORT_STATUS`,
`SYZYGYn_MANUFACTURER_NAME`, `SYZYGYn_PRODUCT_NAME`, etc.) to report every
SYZYGY peripheral port on the carrier board#footnote[Opal Kelly, _SYZYGY
Standard and SmartVIO DNA Specification_,
#link("https://syzygyfpga.io/")[syzygyfpga.io]\.]\. Each `SyzygyPort` entry
reports:

#table(
  columns: (1.4fr, 1fr, 1fr, 2.2fr),
  [Field], [REST type], [Proto], [Meaning],
  [`index`], [integer], [1 `uint32`], [0-based SYZYGY port index (`0`, `1`, ...).],
  [`port_status_mv`], [integer], [2 `uint32`], [Negotiated SmartVIO rail voltage in millivolts (`0` when unpowered or empty).],
  [`is_populated`], [boolean], [3 `bool`], [`true` when a peripheral pod is detected on the port.],
  [`manufacturer_name`], [string], [4 `string`], [Peripheral manufacturer from SYZYGY DNA EEPROM.],
  [`product_name`], [string], [5 `string`], [Peripheral product name (e.g. `"SZG-DAC-AD9116"`).],
  [`product_model`], [string], [6 `string`], [Peripheral model qualifier.],
  [`product_version`], [string], [7 `string`], [Peripheral revision string.],
  [`serial_number`], [string], [8 `string`], [Peripheral serial number.],
)

=== PLL configuration, flash memory, device ID and timeout <ov-dev-hardware-mgmt>
// ref: proto DeviceService GetPLLConfiguration, SetPLLConfiguration, LoadDefaultPLLConfiguration,
// FlashRead, FlashWrite, FlashEraseSector, SetDeviceID, SetTimeout; @appendix-status.
`DeviceService` in `proto/okfpga/v1/okfpga.proto` also defines board-level
hardware management RPCs:

#table(
  columns: (1.5fr, 1.6fr, 1.9fr),
  [gRPC Method (`DeviceService`)], [Request message], [Purpose],
  [`GetPLLConfiguration`], [`GetPLLRequest { serial, pll_index }`], [Reads `reference_frequency` (Hz) and `outputs[]` (`index`, `frequency`, `enabled`) for PLL index `0` (PLL22150) or `1` (PLL22393).],
  [`SetPLLConfiguration`], [`SetPLLRequest { serial, pll_index, reference_frequency, outputs[] }`], [Writes a new PLL configuration to the device.],
  [`LoadDefaultPLLConfiguration`], [`DeviceRequest { serial }`], [Restores the PLL to its EEPROM default configuration.],
  [`FlashRead`], [`FlashReadRequest { serial, address, length }`], [Reads `length` bytes from on-board non-volatile flash memory starting at `address` into `FlashDataResponse.data`.],
  [`FlashWrite`], [`FlashWriteRequest { serial, address, data }`], [Writes `data` bytes to on-board flash memory at `address`.],
  [`FlashEraseSector`], [`FlashEraseSectorRequest { serial, address }`], [Erases the flash sector containing `address`.],
  [`SetDeviceID`], [`SetDeviceIDRequest { serial, device_id }`], [Writes the user-definable 32-character device ID string.],
  [`SetTimeout`], [`SetTimeoutRequest { serial, timeout_ms }`], [Sets the FrontPanel communication timeout in milliseconds.],
)

#note[
  These eight board-level operations are currently defined on gRPC
  `DeviceService` only and have no REST equivalents in `api/openapi.yaml`
  (@appendix-status).
]
