// chapters/overview/2-6-axi.typ — Section 2.6: AXI data port.
// ref: ADR-003, ADR-004, ADR-005, ADR-015, ADR-025 (address ranges),
// ADR-034 D50, D64, P9, P14; @enf-address-ranges; @enf-reset; @appendix-status.
#import "../../lib.typ": *

== AXI data port <ov-axi>

=== Direct AXI vs Classic bridges and address widths <ov-axi-model>
// ref: ADR-003; ADR-025; ADR-034 D50, D64, P9, P14; @res-interfaces; @enf-address-ranges.
Routes under `/devices/{serial}/axi/*` and RPCs in `AXIDataPortService` map onto
Opal Kelly's `OpalKelly::FPGADataPortAXI` interface (available on FP-Gen3-USB
devices that report `dataport = "axi"` / `OK_DATAPORT_AXI`,
@ov-cfg-bitfile-dataport).

- *Direct AXI vs Classic-to-AXI bridges:* Do not confuse direct FrontPanel AXI
  (`FPGADataPortAXI`, used on Gen3-USB devices via `/axi/*`) with designs on
  Classic devices (such as XEM8320 or XEM7320) that tunnel AXI accesses through
  Classic `WireIn`/`TriggerIn`/`WireOut` endpoints. Classic bridge designs use
  `/classic/*` routes with `interfaces: ["classic"]` and optional
  developer-defined address mappings (@enf-address-mapping, @example-DualPort).
- *64-bit request `address` vs 32-bit `AddressRange`:* To match the FrontPanel
  6.1.0 C API signatures (`uint64_t address`), AXI Lite and AXI Full request
  schemas accept a 64-bit integer `address`. Resource `AddressRange`
  declarations (`start`, `end`) are 32-bit unsigned integers
  (`0x0000_0000`–`0xFFFF_FFFF`). Under `endpoint_enforcement = "enforce"`, the
  server checks the entire byte span `[address, address + byte_length - 1]` of
  an AXI Lite or AXI Full transfer against the caller's held resources:
  - the holding resource MUST declare the matching AXI interface (`axi_lite`,
    `axi_full` or `axi_stream`);
  - the entire accessed byte span MUST fit within `0x0000_0000`–`0xFFFF_FFFF`
    and lie inside a declared `AddressRange` of that usable resource (not
    `private` for `ReadOnly` reads). Any request whose span overflows 32 bits
    cannot match a 32-bit `AddressRange` and MUST be denied (`403` /
    `PERMISSION_DENIED`).

#note[
  In server version `0.3.10`, the REST AXI routes are wired, whereas the gRPC
  `AXIDataPortService` methods return `UNIMPLEMENTED` until an FP-Gen3-USB
  (`OK_DATAPORT_AXI`) hardware test target is available (@appendix-status).
]

=== AXI4-Lite operations <ov-axi-lite>
// ref: openapi.yaml /devices/{serial}/axi/lite/*; proto AXIDataPortService AXILite*.

#table(
  columns: (auto, 1.5fr, 1.5fr, 1.3fr),
  [Operation], [REST], [gRPC (`AXIDataPortService`)], [Class & response],
  [Read 32-bit word(s)], [`POST /devices/{serial}/axi/lite/read`], [`AXILiteRead(AXILiteReadRequest)` & `AXILiteReadBulk(AXILiteReadBulkRequest)`], [Read → `AXILiteReadResponse` / `AXILiteReadBulkResponse`],
  [Write 32-bit word(s)], [`PUT /devices/{serial}/axi/lite/write`], [`AXILiteWrite(AXILiteWriteRequest)` & `AXILiteWriteBulk(AXILiteWriteBulkRequest)`], [Write → `AXIOperationStats` / `StatusResponse`],
  [Reset AXI4-Lite], [`POST /devices/{serial}/axi/lite/reset`], [`AXILiteReset(DeviceRequest)`], [Write → `StatusResponse` (@enf-reset)],
)

Request and response details:
- *REST `AXILiteReadRequest`:* `address` (64-bit integer, required), `count`
  (number of 32-bit words, default `1`), optional `timeout_ms`. Returns
  `AXILiteReadResponse { values: [uint32, ...], bytes_transferred }`.
- *REST `AXILiteWriteRequest`:* `address` (required), either `value` (single
  32-bit word) or `values` (array of 32-bit words for bulk write), and optional
  `timeout_ms`. Returns `AXIOperationStats { transfer_byte_count,
  transaction_count }`.
- Requires `axi_lite` in the holding resource's `interfaces` list and coverage
  of `[address, address + 4 * count - 1]` in `address_ranges`.

=== AXI4 Full operations <ov-axi-full>
// ref: openapi.yaml /devices/{serial}/axi/full/*; proto AXIDataPortService AXIFull*.

#table(
  columns: (auto, 1.6fr, 1.4fr, 1.3fr),
  [Operation], [REST], [gRPC (`AXIDataPortService`)], [Class & response],
  [Bulk read], [`POST /devices/{serial}/axi/full/read`], [`AXIFullRead(AXIFullReadRequest)`], [Read → raw bytes (`application/octet-stream`) / `AXIFullReadResponse`],
  [Bulk write], [`POST /devices/{serial}/axi/full/write?address=A&burst=B&timeout_ms=T` (`application/octet-stream`)], [`AXIFullWrite(AXIFullWriteRequest)`], [Write → `AXIOperationStats`],
  [Get datapath width], [`GET /devices/{serial}/axi/full/datapath-width`], [`AXIFullGetDatapathWidth(DeviceRequest)`], [Read → `{ "width_bytes": N }` / `DatapathWidthResponse`],
  [Reset AXI4 Full], [`POST /devices/{serial}/axi/full/reset`], [`AXIFullReset(DeviceRequest)`], [Write → `StatusResponse` (@enf-reset)],
)

Request and response details:
- *Read (`AXIFullReadRequest`):* `address` (required), `length` in bytes
  (required), optional `burst` length and `timeout_ms`.
- *Write:* on REST, `address` (required), `burst` (optional) and `timeout_ms`
  (optional) are query parameters and the payload is sent as raw
  `application/octet-stream` bytes; on gRPC, `AXIFullWriteRequest` carries
  `serial`, `address`, `data`, `burst` and `timeout_ms`.
- Requires `axi_full` in the holding resource's `interfaces` list and coverage
  of `[address, address + length - 1]` in `address_ranges`.

=== AXI4-Stream operations <ov-axi-stream>
// ref: openapi.yaml /devices/{serial}/axi/stream/*; proto AXIDataPortService AXIStream*.

#table(
  columns: (auto, 1.6fr, 1.4fr, 1.3fr),
  [Operation], [REST], [gRPC (`AXIDataPortService`)], [Class & response],
  [Stream read], [`POST /devices/{serial}/axi/stream/read`], [`AXIStreamRead(AXIStreamReadRequest)`], [Read → raw bytes (`application/octet-stream`) / `AXIStreamReadResponse`],
  [Stream write], [`POST /devices/{serial}/axi/stream/write?timeout_ms=T` (`application/octet-stream`)], [`AXIStreamWrite(AXIStreamWriteRequest)`], [Write → `AXIStreamWriteResponse` (`bytes_transferred`)],
  [Get datapath widths], [`GET /devices/{serial}/axi/stream/datapath-width`], [`AXIStreamGetDatapathWidths(DeviceRequest)`], [Read → `{ "write_width_bytes", "read_width_bytes" }` / `AXIStreamDatapathWidthsResponse`],
  [Reset AXI4-Stream], [`POST /devices/{serial}/axi/stream/reset`], [`AXIStreamReset(DeviceRequest)`], [Write → `StatusResponse` (@enf-reset)],
)

Unlike AXI4-Lite and AXI4 Full, AXI4-Stream transfers are unaddressed byte
streams (`length` and optional `timeout_ms`). Under `enforce`, the caller MUST
hold a usable resource on `<serial>` that declares `"axi_stream"` in its
`interfaces` list.
