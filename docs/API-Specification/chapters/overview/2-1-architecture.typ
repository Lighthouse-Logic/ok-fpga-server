// chapters/overview/2-1-architecture.typ — Section 2.1: Architecture.
// ref: ADR-001 (Rust/tokio), ADR-004 (REST/axum), ADR-005 (gRPC/tonic),
// ADR-007 (web dashboard), ADR-010 (ServiceManager), ADR-011/031/032 (MQTT),
// ADR-012/027 (mDNS), ADR-017/018/021 (per-device FFI serialisation),
// ADR-022 (dashboard log streaming), ADR-033 (singleton instance guard).
#import "../../lib.typ": *

== Architecture <ov-architecture>

=== Process, network interfaces and ports <ov-arch-ports>
// ref: ADR-001, ADR-004, ADR-005, ADR-007, ADR-010, ADR-011, ADR-012.
ok-fpga-server runs as a single multi-threaded process built on the `tokio`
asynchronous runtime. At startup it loads the Opal Kelly FrontPanel 6.1.0
shared library (`okFrontPanel.dll` on Windows, `libokFrontPanel.so` on Linux),
initialises the device scanner and resource manager, and serves its network
interfaces from that one process:

#table(
  columns: (auto, auto, auto, 1fr),
  [Interface], [Default port], [Config table], [Purpose],
  [REST (HTTP/JSON + SSE)], [`8088` (`8080` built-in fallback)], [`[rest]`], [Base URL `/api/v1`. Defined by `api/openapi.yaml`. Serves REST endpoints and Server-Sent Events (`/events`, `/server/logs`).],
  [gRPC (HTTP/2 + Protobuf)], [`50051`], [`[grpc]`], [Package `okFPGAService.v1`, defined by `proto/okfpga/v1/okfpga.proto`. Exposes `DeviceService`, `ClassicDataPortService`, `AXIDataPortService` and `ServerService`, plus gRPC server reflection.],
  [Management & Web Dashboard], [`9090`], [`[server]` / `[dashboard]`], [Always-on operator interface serving the browser dashboard, WebSocket live updates and `/mgmt/*` service control routes.],
  [MQTT v5 publisher], [`1883` (broker)], [`[mqtt]`], [Optional outbound MQTT v5 client that publishes server announcements, LWT offline notices and report-by-exception state snapshots (@ov-events-mqtt).],
  [mDNS / DNS-SD], [`5353` (UDP)], [`[mdns]`], [Optional zero-configuration local-network advertisement of `_okfpga._tcp.local.` (@ov-events-mdns).],
)

The Management interface (`management_port`, default `9090`) is always active
while the server process is running so an operator can still inspect status or
restart the REST or gRPC listeners if either transport has been stopped
(@ov-mgmt-services).

=== Singleton instance guard and startup port pre-flight <ov-arch-singleton>
// ref: ADR-033 (singleton guard & pre-flight port check).
FrontPanel allows only one process on a host to manage connected FPGA modules
reliably. Before opening the FrontPanel library or starting any network
service, the server MUST acquire an OS-level singleton guard and verify that
its configured TCP ports are free:

+ *Singleton instance lock.* On Windows the server creates the global named
  mutex `Global\ok-fpga-server-singleton`; on Linux and macOS it acquires an
  exclusive non-blocking `flock(LOCK_EX | LOCK_NB)` on
  `/tmp/ok-fpga-server.lock` and writes its PID into the lock file. If another
  instance already holds the lock, the new process MUST log an error and exit
  immediately with a non-zero exit code without touching any hardware handle.
  Because the lock is owned by an OS kernel handle, the operating system
  releases it automatically even if the server process terminates abnormally.
+ *Startup TCP port pre-flight check.* Immediately after acquiring the
  singleton lock, the server tests binding `server.management_port`,
  `rest.port` (when `rest.enabled = true`) and `grpc.port` (when
  `grpc.enabled = true`). If any required port is already in use by another
  process, startup MUST abort with a clear diagnostic naming the conflicting
  bind address and port before any subsystem starts.

=== Per-device FrontPanel FFI serialisation <ov-arch-ffi>
// ref: ADR-001, ADR-017, ADR-018, ADR-021 (DeviceHandlePool).
For each connected and supported FPGA, the server maintains a single open
`okCFrontPanel` handle in its internal `DeviceHandlePool`, protected by a
per-device mutex:

- Every FrontPanel C API call for a given `<serial>` acquires that device's
  mutex and runs on a blocking worker thread so USB/PCIe I/O never stalls the
  asynchronous network runtime.
- Two operations on *different* devices (`serial` A and `serial` B) run in
  parallel without contending on the same mutex.
- Two operations on the *same* device are strictly serialised: the server
  never issues concurrent FrontPanel C calls on the same handle.

#caution[
  Per-device FFI serialisation protects each *individual* API call (or an
  explicit `update=true` refresh-and-read pair, @enf-update-flag). It does
  *not* prevent another client's call from running between two separate HTTP
  or gRPC requests. Multi-step hardware sequences MUST be protected by
  locking the covering shared resource (@res-lock-when) or by using an
  exclusive resource.
]

=== Web dashboard <ov-arch-dashboard>
// ref: ADR-007 (dashboard), ADR-022 (log streaming), Phase 28C/28D (settings & catalog).
When compiled with the `dashboard` feature and enabled via
`dashboard.enabled = true`, the server hosts a single-page operator dashboard
 showing connected devices, loaded bitfiles, SYZYGY peripherals, active
resources, connected users, service states and live server logs:

- *Real-time updates:* The dashboard subscribes to the event bus and log ring
  buffer so device hot-plug, resource state changes and log lines appear
  immediately without polling.
- *Settings lockdown (`dashboard.expose_settings`):* When `true` (default),
  operators can adjust the runtime `endpoint_enforcement` level and server
  `description` from the dashboard Settings page (persisted back to
  `config.toml`). When `false`, the access-control selector is hidden and
  attempts to modify `endpoint_enforcement` through the management settings
  route are ignored.
- *Hardware catalog (`[catalog]`):* Optional `[[catalog.syzygy_boards]]`,
  `[[catalog.devices]]` and `[[catalog.bitfiles]]` entries in `config.toml`
  supply human-readable descriptions and documentation links (`about_url`)
  for the dashboard detail panels.
