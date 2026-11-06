// chapters/overview/2-7-events.typ — Section 2.7: Events.
// ref: ADR-011, ADR-012, ADR-016 (event bus), ADR-027 (OS-native mDNS),
// ADR-030 (SSE expansion), ADR-031/032 (MQTT announce, LWT, report-by-exception),
// ADR-034 §5, §12 (44-D gRPC WatchDeviceEvents fan-out, D58); Phase 42/43 (hostname identity).
#import "../../lib.typ": *

== Events <ov-events>

=== REST Server-Sent Events (`GET /events`) and gRPC `WatchDeviceEvents` <ov-events-streams>
// ref: ADR-016; ADR-030; ADR-034 §5, §12 (44-D); openapi.yaml GET /events, DeviceEvent;
// proto DeviceService.WatchDeviceEvents, DeviceEvent; @res-events.

#table(
  columns: (auto, 1.3fr, 1.5fr, 1.5fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Stream item],
  [Subscribe to events], [`GET /events` (`text/event-stream`)], [`WatchDeviceEvents(WatchDeviceEventsRequest)`], [SSE `event:` + JSON `data:` / `stream DeviceEvent`],
)

The server fans out every device, resource, user and server lifecycle event to
all connected SSE and gRPC stream subscribers (@res-events):

#table(
  columns: (1.2fr, 1.2fr, 1.8fr, 1.6fr),
  [SSE `event:` name], [gRPC `DeviceEvent` field], [Payload fields (REST JSON / Proto)], [When emitted],
  [`DeviceAdded`], [`device_added` (2)], [`serial`, `product_name`, `interface` (proto `device_interface`), `timestamp`], [A supported FPGA is connected.],
  [`DeviceRemoved`], [`device_removed` (3)], [`serial`, `timestamp`], [A connected FPGA is unplugged.],
  [`DeviceProgrammed`], [`bitfile_programmed` (4)], [`serial`, `bitfile` (proto `filename`), `programmed_by`, `timestamp`], [An FPGA is programmed with a bitfile.],
  [`ResourceChanged`], [`resource_state_changed` (5)], [`resource_id`, `new_state` (proto adds `previous_state`, `device_serial`), `timestamp`], [A resource is acquired, released, locked or unlocked.],
  [`ResourceCreated`], [`resource_created` (6)], [`resource_id`, `device_serial`, `name`, `timestamp`], [A resource definition is registered (`POST /resources` / `CreateResource`).],
  [`ResourceDeleted`], [`resource_deleted` (7)], [`resource_id`, `reason` (`client`, `reprogram`, `clear`, `device_removed` / `DeleteReason`), `timestamp`], [A resource is deleted by a client or by forced cleanup.],
  [`ResourceLocked`], [`resource_locked` (8)], [`resource_id`, `user_id`, `timestamp`], [A holder locks a shared resource.],
  [`ResourceUnlocked`], [`resource_unlocked` (9)], [`resource_id`, `user_id`, `timestamp`], [A shared resource lock ends (explicit unlock, release, timeout, reprogram, clear or unplug).],
  [`UserConnected`], [`user_connected` (10)], [`user_id`, `timestamp`], [A user session is created on first identified API call.],
  [`UserDisconnected`], [`user_disconnected` (11)], [`user_id`, `reason` (`explicit` or `timeout`), `timestamp`], [A user disconnects or expires via heartbeat sweep.],
  [`ServerShutdown`], [`server_shutdown` (12)], [`reason` (`signal`), `timestamp`], [The server process begins graceful shutdown.],
)

On REST SSE, the `data:` line contains the event JSON object directly (not
wrapped in an outer variant key), and the server sends a `:ping` comment every
`15` seconds to prevent idle proxy timeouts. On gRPC, `DeviceEvent.timestamp`
(field 1) carries the ISO 8601 UTC timestamp and `oneof event` carries the
typed payload message.

=== MQTT publisher and cross-subnet discovery <ov-events-mqtt>
// ref: ADR-011, ADR-031, ADR-032; crates/ok-fpga-server/src/mqtt.rs.
When compiled with the `mqtt` feature and enabled (`mqtt.enabled = true`), the
server connects as a publish-only client to `mqtt.broker_url` (default
`"mqtt://localhost:1883"`). It never subscribes to command topics.

- *Hostname-derived identity:* The MQTT client ID and topic prefix are derived
  automatically from the sanitised system hostname as
  `client_id = "okfpga-server-<hostname>"` and
  `topic_prefix = "okfpga/server-<hostname>"`.
- *JSON message envelope:* Every published message uses the versioned envelope
  `{ "v": 1, "ts": "<ISO-8601>", "type": "<message-type>", "server_id":
  "okfpga-server-<hostname>", "payload": { ... } }`.
- *Report-by-exception and heartbeat:* The publisher checks state every
  `mqtt.publish_interval_secs` (default `5` s) and publishes inventory/sensor
  topics *only when the underlying data has changed*, or when
  `mqtt.heartbeat_max_secs` (default `300` s; `0` disables forced heartbeats)
  elapses without a publish.
- *Last Will and Testament (LWT):* The MQTT connection registers an LWT on
  `{topic_prefix}/server/announce` (QoS 1, retained) with `type =
  "server.offline"`, so the broker automatically marks the server offline if it
  crashes or loses network connectivity.

#table(
  columns: (1.5fr, auto, auto, 2fr),
  [Topic (`{topic_prefix}/...`)], [QoS], [Retained], [Message `type` and purpose],
  [`status`], [0], [no], [`server.status` — periodic server liveness and uptime.],
  [`server/announce`], [1], [yes], [`server.announce` (online) / `server.offline` (LWT or shutdown) — cross-subnet discovery record carrying REST, gRPC and dashboard URLs, description, device count and service states. IP selection follows `mqtt.announce_interface` and `mqtt.preferred_subnets` (@ov-config-tables).],
  [`services/status`], [1], [yes], [`services.status` — state of `rest`, `grpc`, `mqtt` and `mdns` services (published on change).],
  [`devices/inventory`], [1], [yes], [`devices.inventory` — list of connected devices (published on change).],
  [`devices/{serial}/state`], [1], [yes], [`device.state` — per-device connection and bitfile state (published on change).],
  [`devices/{serial}/sensors`], [0], [no], [`device.sensors` — sensor readings (published when readings change beyond threshold or on heartbeat).],
  [`devices/{serial}/events`], [1], [no], [`device.event` — real-time device lifecycle events.],
  [`resources/inventory`], [1], [yes], [`resources.inventory` — registered resources and states (published on change).],
)

=== mDNS service advertisement <ov-events-mdns>
// ref: ADR-012, ADR-027; Phase 42, Phase 43; crates/ok-fpga-server/src/mdns.rs.
When compiled with the `mdns` feature and enabled (`mdns.enabled = true`), the
server advertises itself on the local link via Multicast DNS / DNS-SD (RFC 6762
and RFC 6763):

- *Service type and instance name:* Registers service type `_okfpga._tcp.local.`
   targeting `server.management_port` (default `9090`). Unless overridden in
  `config.toml`, `mdns.instance_name` defaults to `"okfpga-server-<hostname>"`
  (matching the MQTT `client_id`).
- *Tiered OS-native registration:* The server first registers through the host
  operating system's native mDNS daemon (Windows `DnsServiceRegister` /
  `dnscache` with explicit IPv4 `A` record; Linux Avahi via D-Bus) to avoid UDP
  port 5353 dual-responder conflicts, and cleans up stale registrations from
  prior runs before registering. If no OS daemon is running, it falls back to
  an embedded `mdns-sd` responder with automatic address advertisement.
- *DNS-SD TXT records:* Keys for disabled or uncompiled services are omitted:
  - `version`: server version string (for example `"0.3.10"`);
  - `description`: `server.description` (or `mdns.instance_name` if empty);
  - `rest`: REST TCP port (for example `"8088"`), when REST is enabled;
  - `grpc`: gRPC TCP port (for example `"50051"`), when gRPC is enabled;
  - `mqtt`: MQTT topic prefix (`"okfpga/server-<hostname>"`), when MQTT is
    enabled;
  - `dashboard`: management/dashboard TCP port (`"9090"`), when dashboard is
    enabled;
  - `devices`: current connected FPGA count as a decimal string (updated
    dynamically on hot-plug).
