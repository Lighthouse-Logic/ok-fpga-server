# ADR-031: MQTT Cross-Subnet Server Discovery

**Status**: Proposed (Phase 40)
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
- [netdev crate](https://crates.io/crates/netdev) — cross-platform network interface enumeration
- [MQTT v5.0 — Will Message](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901033)
- [RFC 3927 — Link-Local (169.254.0.0/16)](https://tools.ietf.org/html/rfc3927)
