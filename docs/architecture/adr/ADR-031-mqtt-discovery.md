# ADR-031: MQTT Cross-Subnet Server Discovery

**Status**: Implemented ✅
**Date**: 2026-08-26
**Deciders**: Architecture Team
**Supersedes**: — (Extends [ADR-011](ADR-011-mqtt-publishing.md))

## Context

mDNS/DNS-SD discovery (`_okfpga._tcp`, ADR-012, ADR-027) is **link-local
only** — it cannot cross IP routers. Lab environments with servers on
different subnets (e.g. `10.0.1.x` bench network and `10.0.2.x` instrument
rack) cannot discover each other via mDNS.

The ok-FPGA-Server already has a fully implemented MQTT publisher (ADR-011,
Phase 10) that publishes heartbeat and status messages to an external broker.
By adding a `server.announce` message with full connectivity information,
any MQTT subscriber on any subnet can discover the server — provided both
share the same MQTT broker.

The lab environment uses **DHCP**, so IP addresses change routinely. The
announce message must resolve the server's current routable IP address at
publish time, not rely on a static configuration.

## Decision

### §1 — New Topic: `{prefix}/server/announce`

A **retained** message published to `{prefix}/server/announce` on connect
and on every periodic heartbeat tick. QoS 1 (at-least-once). Retained so
new subscribers receive the announce immediately upon connecting to the
broker.

#### Message Format

```json
{
  "v": 1,
  "ts": "2026-08-26T10:00:00Z",
  "type": "server.announce",
  "server_id": "okfpga-server-01",
  "payload": {
    "description": "Lab Bench 1 - XEM7320",
    "rest_url": "http://10.0.1.50:8080",
    "grpc_url": "grpc://10.0.1.50:50051",
    "dashboard_url": "http://10.0.1.50:9090",
    "mqtt_topic_prefix": "okfpga/server01",
    "api_version": "0.1.0",
    "device_count": 1,
    "services": {
      "rest": true,
      "grpc": true,
      "mqtt": true,
      "mdns": true
    }
  }
}
```

### §2 — IP Address Resolution

The `rest_url`, `grpc_url`, and `dashboard_url` fields require the server's
routable IPv4 address. The server resolves this using the following algorithm:

1. **Enumerate** all non-loopback network interfaces using the `netdev`
   crate (`netdev::get_interfaces()`), which provides interface names and
   grouped IPv4/IPv6 addresses per interface — essential for matching by
   interface name on Windows (friendly names) and Linux (`eth0`, `ens192`).
2. **Discard** any interface whose IPv4 address is in the `169.254.0.0/16`
   (APIPA / link-local) range.
3. **Single interface**: if only one non-loopback, non-APIPA interface
   remains, use its IPv4 address. No configuration needed.
4. **Multiple interfaces**: use the interface whose **name** matches the
   `[mqtt] announce_interface` config value.
5. **No `announce_interface` set + multiple interfaces**: walk the
   `[mqtt] preferred_subnets` list in order and use the first interface
   whose IPv4 address matches a listed subnet (CIDR notation).
6. **No match**: log a warning and fall back to the first non-loopback,
   non-APIPA interface found.

#### Config Fields (additions to `[mqtt]`)

```toml
[mqtt]
# ... existing fields ...

# Name of the network interface to use for announce URLs.
# Only needed when the machine has multiple non-loopback interfaces.
# Example: "Ethernet", "eth0", "ens192"
# Default: "" (auto-detect)
announce_interface = ""

# Ordered list of preferred subnets (CIDR) for auto-detection when
# announce_interface is not set and multiple interfaces exist.
# The first matching interface wins.
# Default: [] (no preference — use first available)
preferred_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.40.40.0/24"]
```

### §3 — MQTT Last Will & Testament (LWT)

The MQTT client registers a **Last Will & Testament** message when
connecting to the broker:

- **Topic**: `{prefix}/server/announce`
- **Payload**: `{"v": 1, "type": "server.offline", "server_id": "...", "ts": "..."}`
- **QoS**: 1
- **Retain**: true

**Behavior:**
- **Graceful shutdown**: the server explicitly publishes the offline message
  and disconnects. The broker stores the retained offline message.
- **Crash / network failure**: the broker detects the TCP connection loss
  (via keepalive timeout) and publishes the LWT message automatically. This
  clears the stale announce and prevents phantom servers in discovery.

Subscribers can distinguish online/offline servers by checking the `type`
field: `"server.announce"` = online, `"server.offline"` = offline.

### §4 — Graceful Shutdown

On `stop()`, the MQTT publisher:
1. Publishes the offline message to `{prefix}/server/announce` (retained).
2. Publishes the existing `server.shutdown` message to `{prefix}/status`.
3. Disconnects from the broker.

### §5 — AppState Extensions

`AppState` gains immutable fields set during construction:

| Field | Type | Source |
|-------|------|--------|
| `rest_port` | `u16` | `config.rest.port` |
| `grpc_port` | `u16` | `config.grpc.port` |
| `management_port` | `u16` | `config.server.management_port` |
| `description` | `String` | `config.server.description` |

These are needed by `publish_server_announce()` to build URLs.

## Consequences

### Positive
- **Cross-subnet discovery** — subscribers on any subnet can find the server
  via a shared MQTT broker
- **DHCP-friendly** — resolves the current IP at publish time, not at config time
- **Crash-resilient** — LWT ensures stale announces are cleared automatically
- **Backward-compatible** — new topic, no changes to existing topics

### Negative
- **Requires MQTT broker** — cross-subnet discovery only works when MQTT is
  enabled and a shared broker is reachable
- **IP resolution complexity** — multi-homed hosts require interface config

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/mqtt.rs` | Add `publish_server_announce()`, LWT setup, shutdown clear, `resolve_routable_ip()` |
| `crates/ok-fpga-server/src/config.rs` | Add `announce_interface`, `preferred_subnets` to `MqttConfig` |
| `crates/ok-fpga-server/Cargo.toml` | Add `netdev` dependency to `mqtt` feature |
| `config.toml` | Add `announce_interface`, `preferred_subnets` with explanatory comments |
| `docs/architecture/adr/ADR-031-mqtt-discovery.md` | This ADR |
| `docs/architecture/overview.md` | Index entry for ADR-031 |

## References

- [ADR-011: MQTT Status Publishing](ADR-011-mqtt-publishing.md) — base MQTT architecture
- [ADR-012: mDNS Service Discovery](ADR-012-mdns-service-discovery.md) — link-local discovery
- [ADR-032: Report-by-Exception Publishing](ADR-032-mqtt-report-by-exception.md) — refines periodic publishing to change-driven model
- [netdev crate](https://crates.io/crates/netdev) — cross-platform network interface enumeration
- [MQTT v5.0 — Will Message](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901033)
- [RFC 3927 — Link-Local (169.254.0.0/16)](https://tools.ietf.org/html/rfc3927)

---

## Addendum A — Windows Friendly Names & Heartbeat (2026-08-26)

### Interface Name Resolution

The `netdev` crate returns the Windows adapter **GUID** (e.g.,
`{952CCB50-2796-...}`) as `Interface.name` on Windows. This is not
human-readable. `netdev` 0.31 provides `Interface.friendly_name:
Option<String>` which contains the user-visible name ("Ethernet",
"Wi-Fi") on Windows.

The IP resolution algorithm (§2) now uses
`friendly_name.unwrap_or(name)` for display and `announce_interface`
matching, checking both fields for cross-platform flexibility.

### Heartbeat Integration

The `server.announce` message is no longer re-published on every
periodic tick (ADR-032). It is published:
1. On initial MQTT connection (unchanged)
2. When the announce payload data changes (device added/removed,
   service started/stopped, IP change in DHCP environments)
3. On the forced heartbeat interval (`heartbeat_max_secs`, default 300s)

See [ADR-032](ADR-032-mqtt-report-by-exception.md) for the full
report-by-exception publishing model.

### Hostname-Derived Identity

The `client_id` and `topic_prefix` fields referenced in §1 are no longer
user-configurable. They are derived from the system hostname at startup
as `okfpga-server-<hostname>` and `okfpga/server-<hostname>` respectively.
The `mqtt_topic_prefix` field in the announce payload (§1) reflects this.

See [ADR-011 Addendum B](ADR-011-mqtt-publishing.md) for details.

---

## Addendum B — Graceful Shutdown Retained Cleanup (2026-08-26)

### Retained Topic Cleanup (Phase 41)

The graceful shutdown sequence in §4 has been extended. On `stop()`, the
MQTT publisher now:

1. Publishes `server.offline` to `{prefix}/server/announce` (retained).
2. Publishes `server.shutdown` to `{prefix}/status` (non-retained).
3. **Publishes empty payloads** (retain=true) to all 5 retained topics:
   - `{prefix}/status`
   - `{prefix}/services/status`
   - `{prefix}/devices/inventory`
   - `{prefix}/resources/inventory`
   - `{prefix}/server/announce`
4. Queues an MQTT DISCONNECT.
5. **Flushes the rumqttc event loop** (up to 2s deadline) to ensure all
   queued messages are actually sent over the wire before the TCP
   connection closes.
6. Logs `"MQTT publisher disconnected (retained topics cleared)"`.

Per MQTT spec §3.3.1.3, publishing an empty payload with `retain=true`
instructs the broker to **delete** the retained message for that topic.

### Event Loop Flush

The original implementation used a fixed 200ms sleep before disconnect,
which did **not** work because rumqttc requires the event loop to keep
polling for queued messages to be sent. The fix continues polling
`eventloop.poll()` after queueing the cleanup messages and disconnect,
with a 2-second timeout. This ensures all messages are flushed before
the connection is torn down.

### API Shutdown Integration

`POST /mgmt/shutdown` now triggers the same graceful shutdown flow as
Ctrl+C. A shared `Arc<Notify>` is passed from `main()` to the
management router. The handler notifies it after a 500ms delay (to
allow the HTTP response to be sent). `main()` uses `tokio::select!` to
wait on either `ctrl_c()` or the API shutdown signal — both follow the
identical graceful shutdown path (SSE broadcast → stop_all → exit).

### Broker Unreachable Logging

When the MQTT broker is unreachable on initial connection, the event
loop now logs a single WARN:

```
WARN MQTT broker unreachable — will retry in background
     broker=localhost:1883
     error=I/O: Connection refused (os error 10061)
```

Subsequent retry failures are silent to avoid log spam. If a previously
established connection is lost, the existing `"MQTT connection lost"`
warning fires. This log-once behavior is controlled by an
`initial_connect_warned` flag alongside the existing `connected` flag.

