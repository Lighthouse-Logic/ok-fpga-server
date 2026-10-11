// chapters/overview.typ — Section 2: server overview (to be expanded).
// ref: docs/architecture/overview.md; ADR-001..ADR-033 general architecture;
// config.toml; roadmap 🔵 Later "expand API specification overview chapters".
#import "../lib.typ": *

= Server Overview <overview>

#note[
  This chapter is an overview only. Each section will be expanded into a full
  specification (every operation, parameter, error and rationale) in a later
  revision of this document. Section numbers in this chapter may therefore
  gain sub-sections, but the existing numbers will not change.
]

== Architecture <ov-architecture>
// ref: ADR-001 (Rust/tokio), ADR-017/018/021 (per-device FFI serialisation),
// ADR-007 (device manager), ADR-016 (event bus).
ok-fpga-server is a single Rust process. It loads the Opal Kelly FrontPanel
6.1.0 shared library, discovers connected devices, opens each one, and serves
several network interfaces from that one process:

#table(
  columns: (auto, auto, 1fr),
  [Interface], [Default port], [Use],
  [REST (HTTP/JSON)], [`8088`], [Base URL `/api/v1`. Defined by `api/openapi.yaml`. Also serves the Server-Sent Events stream and the web dashboard.],
  [gRPC (HTTP/2, protobuf)], [`50051`], [Defined by `proto/okfpga/v1/okfpga.proto`. Same contract as REST; preferred for high-rate pipe and stream transfers.],
  [Management], [`9090`], [Health and service control for operators.],
  [MQTT broker], [`1883`], [Optional publish/subscribe delivery of device and resource events.],
  [mDNS], [—], [Optional service announcement so clients can discover servers on the local network.],
)

All FrontPanel calls for one device are serialised: the server never issues
two FrontPanel calls to the same device at the same time. This protects each
*single* call. It does not protect a *sequence* of calls (for example "set
address, then read data"); sequences are protected by the resource model
(@resource-model).

== Identity, sessions and heartbeat <ov-identity>
// ref: ADR-020 §5 (heartbeat 24 s / 60 s), ADR-029 (identity, Proposed),
// ADR-008 (auth providers; NoAuth default).
Clients identify themselves with a user id: the `X-User-Id` header (REST),
the `x-user-id` metadata key (gRPC), or a `user_id` field in the request.
Authentication is pluggable; the default is no authentication, with local and
LDAP providers and HTTP Basic defined for later use.

A client that holds resources MUST show it is alive. Any identified call
counts; an idle client sends `POST /heartbeat` (gRPC `DeviceService.Ping`) at
least every 24 seconds. After 60 seconds of silence the server releases
everything the user holds. `POST /disconnect` (gRPC
`DeviceService.Disconnect`) releases everything at once. @res-heartbeat gives
the full rules.

== Devices <ov-devices>
// ref: ADR-007, ADR-011 (USB 2.0 filtered).
`GET /devices` lists connected devices; `GET /devices/<serial>` returns one
device's details (model, serial, firmware, interface speed, programmed
state). Further routes expose sensors (`/sensors`), device settings
(`/settings`, `/settings/{key}`), SYZYGY port information (`/syzygy`), PLL
configuration, flash memory access, device id and timeouts. The gRPC
equivalents are in `DeviceService`.

== FPGA configuration (programming) <ov-configure>
// ref: ADR-023 Fix-F Decision 2 (reprogram clears all resources).
A bitfile is loaded with `POST /devices/<serial>/configure` (upload) or
`POST /devices/<serial>/configure_from_server_path` (a file already on the
server). gRPC offers `ConfigureFPGA` and the streaming
`ConfigureFPGAFromMemory`. `GET /devices/<serial>/bitfile` reports what is
loaded, and `GET /devices/<serial>/dataport` reports whether the design uses
the Classic or the AXI data port.

#caution[
  Programming an FPGA replaces its design, so every resource registered on
  that device is deleted (@res-lifecycle-device).
]

== Classic data port <ov-classic>
// ref: ADR-025 (address ranges), D75–D78 (snapshot reads, mask default 0).
Routes under `/devices/<serial>/classic/` (gRPC `ClassicDataPortService`)
map one-to-one onto the FrontPanel Classic API: wires, triggers, pipes,
block-throttled pipes, the register bridge and `ResetFPGA`. The access rules
for every one of these operations are given in @enforcement; the full
per-operation reference will be added later.

== AXI data port <ov-axi>
Routes under `/devices/<serial>/axi/lite`, `/axi/full` and `/axi/stream`
(gRPC `AXIDataPortService`) perform AXI4-Lite, AXI4 and AXI4-Stream
transfers, report datapath widths and reset the interface. Access is
controlled by the address ranges of the caller's resources
(@enf-address-ranges).

#note[
  In this release the gRPC AXI methods are defined but not yet implemented;
  use REST for AXI access.
]

== Events <ov-events>
// ref: ADR-016 (event bus), ADR-034 resource events.
The server publishes device and resource events (device connected or
disconnected, FPGA configured, resource created, acquired, locked, unlocked,
released, deleted). Clients receive them through:

- REST: `GET /events`, a Server-Sent Events stream;
- gRPC: `DeviceService.WatchDeviceEvents`, a server stream;
- MQTT: topics on the optional broker.

== Server management <ov-management>
`/server/status`, `/server/logs`, `/server/restart` and `/server/services/*`
(gRPC `ServerService`) report health and start, stop or restart the
individual services (REST, gRPC, MQTT, mDNS and so on).

== Configuration file <ov-config>
// ref: config.toml; ADR-024 §9 (hot reload); D77 (warn_readonly_side_effects).
The server reads a TOML configuration file. The settings that affect the
behaviour in this document are in the `[access_control]` table and are
described where they apply (@enf-levels, @enf-readonly-warning).
