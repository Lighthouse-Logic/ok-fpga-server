// chapters/overview/2-4-configure.typ — Section 2.4: FPGA configuration (programming).
// ref: ADR-023 Fix-F Decision 2; ADR-028 (ConfigureFPGA alignment); ADR-029 (identity);
// ADR-034 §7, §14 (44-F forced cleanup, D15b, D26, P11, P12); Phase 34-C (file_path, md5_hash);
// D63, D70, SD-05, P16 (dataport normalisation).
#import "../../lib.typ": *

== FPGA configuration (programming) <ov-configure>

=== Client bitfile upload <ov-cfg-upload>
// ref: ADR-028; ADR-029; ADR-034 D26; openapi.yaml POST /devices/{serial}/configure;
// proto DeviceService.ConfigureFPGAFromMemory.

#table(
  columns: (auto, 1.3fr, 1.5fr, 1.5fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [Program from client upload], [`POST /devices/{serial}/configure` (`multipart/form-data`)], [`ConfigureFPGAFromMemory(stream ConfigureFPGAChunk)`], [`BitfileInfo` / `ConfigureFPGAResponse`],
)

Uploads a `.bit` or `.rbf` bitfile from the client machine and writes it to the
FPGA via FrontPanel's `ConfigureFPGAFromMemory`:

- *REST (`multipart/form-data`):*
  - `bitfile` (binary, required): raw bitfile bytes;
  - `filename` (string, optional): original filename (defaults to `"uploaded"`
    if omitted);
  - `description` (string, optional): human-readable description of the loaded
    design;
  - `user_id` (string, optional): caller identity, subject to the mismatch rule
    against `X-User-Id` (@ov-id-resolution).
- *gRPC (`stream ConfigureFPGAChunk`):*
  - `serial` (1), `user_id` (4) and optional `filename` (5) are supplied in the
    first chunk;
  - `data` (2) carries each binary chunk;
  - `is_last` (3) is set to `true` on the final chunk.

=== Server-path bitfile configuration <ov-cfg-server-path>
// ref: ADR-028; openapi.yaml POST /devices/{serial}/configure_from_server_path;
// proto DeviceService.ConfigureFPGA.

#table(
  columns: (auto, 1.5fr, 1.3fr, 1.5fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [Program from server path], [`POST /devices/{serial}/configure_from_server_path`], [`ConfigureFPGA(ConfigureFPGARequest)`], [`BitfileInfo` / `ConfigureFPGAResponse`],
)

Programs the FPGA from a bitfile that already resides on the server's local
filesystem:
- `bitfile_path` (required): path to the `.bit` or `.rbf` file on the server;
- `description` (REST, optional): human-readable description (if omitted, the
  server may populate it from a matching `[[catalog.bitfiles]]` entry in
  `config.toml`);
- `user_id` (optional on REST, field 3 on gRPC): caller identity
  (@ov-id-resolution).

Returns `400 Bad Request` / `INVALID_ARGUMENT` if the path cannot be read or
FrontPanel rejects the bitfile, or `404 Not Found` / `NOT_FOUND` if `<serial>`
is unknown.

=== Clearing FPGA configuration and forced cleanup <ov-cfg-clear>
// ref: ADR-034 §7, §14 (44-F, D15b, D26, P11, P12); @res-lifecycle-device.

#table(
  columns: (auto, 1.3fr, 1.5fr, 1.5fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [Clear configuration], [`DELETE /devices/{serial}/configure`], [`ClearFPGAConfiguration(DeviceRequest)`], [`StatusResponse`],
)

#caution[
  Programming (`POST /configure`, `POST /configure_from_server_path`,
  `ConfigureFPGA`, `ConfigureFPGAFromMemory`) and clearing
  (`DELETE /configure`, `ClearFPGAConfiguration`) are *forced overrides* at
  every enforcement level (`off`, `warn` and `enforce`).
]

Before invoking the FrontPanel hardware configuration or clear call, the server
atomically executes the forced cleanup sequence (@res-lifecycle-device):
+ The device state transitions to `Connected` (unprogrammed) and its internal
  `config_generation` counter increments, preventing concurrent resource
  registrations against the outgoing design.
+ Every locked shared resource on the device is unlocked and emits
  `ResourceUnlocked`.
+ Every holder of every resource on the device is released.
+ Every resource on the device is deleted and emits `ResourceDeleted` with
  `reason = "reprogram"` (for configure) or `reason = "clear"` (for clear).
+ The FrontPanel `ConfigureFPGA*` or `ClearFPGAConfiguration` call runs under
  the per-device FFI lock. If the hardware configuration fails mid-way, the
  device remains in the `Connected` (unprogrammed) state (`bitfile = null`).
  When programming succeeds, the device transitions to `Programmed`, records the
  new `BitfileInfo` (computing its hex-encoded `md5_hash`), and emits
  `DeviceProgrammed`.

=== Bitfile metadata and dataport discovery <ov-cfg-bitfile-dataport>
// ref: Phase 34-C (file_path, md5_hash); ADR-034 D63, D70, SD-05, P16; @res-interfaces.

#table(
  columns: (auto, 1.3fr, 1.4fr, 1.6fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [Get bitfile info], [`GET /devices/{serial}/bitfile`], [`GetBitfileInfo(DeviceRequest)`], [`BitfileInfo` / `BitfileInfoResponse` (`404` if unprogrammed)],
  [Get dataport type], [`GET /devices/{serial}/dataport`], [`GetDataportType(DeviceRequest)`], [`{ "dataport": "legacy" | "axi" }` / `DataportTypeResponse`],
)

`BitfileInfo` fields:
- `filename` (1): basename of the programmed bitfile (or `"uploaded"` when an
  upload omitted `filename`);
- `description` (2): optional description string;
- `programmed_at` (3): ISO 8601 UTC timestamp when programming completed;
- `programmed_by` (4): `user_id` that programmed the device;
- `file_path` (5): full server filesystem path when programmed via
  `configure_from_server_path` / `ConfigureFPGA`, or `null` (REST) / `""`
  (gRPC) for client uploads;
- `md5_hash` (6): lowercase hex-encoded MD5 digest of the bitfile binary.

`GET /devices/{serial}/dataport` (`GetDataportType`) reports the physical host
transport layer from `okTDeviceInfo.dataport`:
- `"legacy"` (`DATAPORT_LEGACY`, FrontPanel `OK_DATAPORT_LEGACY`): FP-Gen1/Gen2
  USB or PCIe host interface (for example XEM7320, XEM8320). Supports Classic
  endpoints (`interfaces: ["classic"]`) only; AXI interfaces are rejected at
  resource registration with `422` / `INVALID_ARGUMENT` (@res-interfaces).
- `"axi"` (`DATAPORT_AXI`, FrontPanel `OK_DATAPORT_AXI`): FP-Gen3-USB host
  interface (for example XEM8305, XEM8310, XEM8350, XEM8370). Supports both
  Classic endpoints and direct AXI interfaces (`axi_lite`, `axi_full`,
  `axi_stream`).

*Important distinction.* `dataport` describes the *host transport*, not whether
the loaded bitfile instantiates Classic HDL endpoints. Classic endpoints work
on both `legacy` and `axi` devices whenever the bitfile instantiates `okHost`
and Classic endpoint modules.
