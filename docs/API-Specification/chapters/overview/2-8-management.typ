// chapters/overview/2-8-management.typ — Section 2.8: Server management.
// ref: ADR-004, ADR-005, ADR-010 (ServiceManager), ADR-022 (log streaming);
// openapi.yaml /server/*; proto DeviceService.GetServerStatus, ServerService.
#import "../../lib.typ": *

== Server management <ov-management>

=== Server status and logs <ov-mgmt-status-logs>
// ref: ADR-022; openapi.yaml /server/status, /server/logs, ServerStatus, LogEntry;
// proto DeviceService.GetServerStatus, ServerService.GetServerStatus; @appendix-status.

#table(
  columns: (auto, 1.3fr, 1.5fr, 1.5fr),
  [Operation], [REST], [gRPC], [Response],
  [Get server status], [`GET /server/status`], [`DeviceService.GetServerStatus` & `ServerService.GetServerStatus`], [`ServerStatus` / `ServerStatusResponse`],
  [Stream server logs], [`GET /server/logs` (`text/event-stream`)], [— (REST SSE only)], [SSE `event: log` with `LogEntry` JSON],
)

`ServerStatus` fields:
- *REST (`GET /server/status`):* `server_version` (from `Cargo.toml`),
  `openapi_version` (`api/openapi.yaml` `info.version`), `frontpanel_version`
  (nullable string, e.g. `"6.1.0"`), `platform` (e.g. `"linux-x86_64"` or
  `"windows-x86_64"`), `description` (`server.description`), `uptime`
  (human-readable string), `uptime_seconds` (integer), `connected_devices`
  (integer), `active_resources` (integer) and `services` (array of
  `ServiceInfo`).
- *gRPC (`DeviceService.GetServerStatus`):* returns `ServerStatus { version,
  frontpanel_version, platform, uptime, uptime_seconds, connected_devices,
  active_resources }`.
- *gRPC (`ServerService.GetServerStatus`):* returns `ServerStatusResponse {
  error, api_version, uptime_secs, device_count, services }`.

`GET /server/logs` opens a Server-Sent Events stream of structured log records
from the server's tracing subscriber. Each SSE frame has `event: log` and a JSON
`LogEntry` payload containing `timestamp` (ISO 8601), `level` (`TRACE`,
`DEBUG`, `INFO`, `WARN`, `ERROR`), `target` (Rust module path), `message` and
structured `fields`.

=== Server restart and service lifecycle <ov-mgmt-services>
// ref: ADR-010 (ServiceManager & respond-then-stop pattern);
// openapi.yaml /server/restart, /server/services/*; proto ServerService.

#table(
  columns: (auto, 1.5fr, 1.4fr, 1.4fr),
  [Operation], [REST], [gRPC (`ServerService`)], [Response],
  [Restart server], [`POST /server/restart`], [`RestartServer(StatusResponse)`], [`202 Accepted` `StatusResponse`],
  [List services], [`GET /server/services`], [`ListServices(StatusResponse)`], [`{ "services": [ServiceInfo, ...] }` / `ListServicesResponse`],
  [Start service], [`POST /server/services/{name}/start`], [`StartService(ServiceRequest)`], [`200 OK` `ServiceInfo` / `ServiceOperationResponse`],
  [Stop service], [`POST /server/services/{name}/stop`], [`StopService(ServiceRequest)`], [`200 OK` `ServiceInfo` / `ServiceOperationResponse`],
  [Restart service], [`POST /server/services/{name}/restart`], [`RestartService(ServiceRequest)`], [`202 Accepted` `ServiceInfo` / `ServiceOperationResponse`],
)

Manageable service names (`{name}` / `ServiceRequest.name`) are `"rest"`,
`"grpc"`, `"mqtt"` and `"mdns"`. Each `ServiceInfo` record reports:
- `name` (1): `"rest"`, `"grpc"`, `"mqtt"` or `"mdns"`;
- `status` (2): `"running"`, `"stopped"`, `"starting"`, `"stopping"`,
  `"failed"` or `"not_compiled"`;
- `compiled` (3): `true` if the corresponding Cargo feature flag was compiled
  into the binary;
- `enabled` (4): `true` if the service is enabled in `config.toml`.

Error conditions and self-stop / self-restart behaviour:
- Starting or stopping an uncompiled service (`compiled = false`) or passing an
  unknown service name returns `400 Bad Request` / `INVALID_ARGUMENT`.
- Starting a service that is already `"running"`, or stopping a service that is
  already `"stopped"`, returns `409 Conflict` / `FAILED_PRECONDITION`.
- *Respond-then-stop / respond-then-restart pattern:* When a client stops or
  restarts the same transport service through which the request arrived (for
  example calling `POST /server/services/rest/stop` over REST, or
  `ServerService.RestartService("grpc")` over gRPC), or calls
  `POST /server/restart` / `RestartServer`, the server flushes the HTTP response
  (`200 OK` for stop, `202 Accepted` for restart) before shutting down the
  listener. The always-on Management port (`server.management_port`, default
  `9090`) also exposes `/mgmt/services/*` so a stopped REST or gRPC transport
  can always be started again.
