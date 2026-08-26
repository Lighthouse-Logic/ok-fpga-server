# ADR-032: MQTT Report-by-Exception Publishing

**Status**: Implemented ✅
**Date**: 2026-08-26
**Deciders**: Architecture Team
**Supersedes**: ADR-011 §"periodic heartbeat" semantics

## Context

ADR-011 defined the MQTT publisher as a constant-interval system: every
`publish_interval_secs` (default 5s), the server published *all* topics
(status, announce, device inventory, resource inventory) regardless of
whether any underlying data had changed.

This is a well-documented IoT anti-pattern. When the server state is
stable (the common case), every 5-second publish sends an identical
payload with only a new timestamp. This causes:

- **Broker churn**: retained messages are replaced 12×/minute, waking
  all subscribers to process redundant data
- **Database bloat**: time-series consumers (InfluxDB, TimescaleDB)
  record identical values at high frequency
- **Bandwidth waste**: network traffic with no informational value
- **False change signals**: subscribers cannot distinguish "something
  changed" from "nothing changed but the clock advanced"

Including timestamps in payloads is correct and necessary (for latency
measurement, offline buffering, and time-series accuracy), but timestamps
should **not drive publishing frequency**.

## Decision

### §1 — Report-by-Exception Model

The periodic publisher now operates as a **report-by-exception** system:

1. Every `publish_interval_secs` (default 5s), the server **polls** its
   own state by building a canonical JSON fingerprint of each topic's
   payload, **excluding fields that always change** (timestamps,
   `uptime_secs`).
2. The fingerprint is compared against the previously published value.
3. A message is published **only if the fingerprint differs** — meaning
   the underlying data has actually changed.
4. When published, the message still includes the current timestamp for
   data integrity.

### §2 — Fingerprint Fields (What Triggers a Publish)

| Topic | Fingerprint includes | Excluded from fingerprint |
|-------|---------------------|--------------------------|
| `server.status` | `device_count`, `resource_count`, `api_version` | `uptime_secs`, `ts` |
| `server.announce` | `description`, `rest_url`, `grpc_url`, `dashboard_url`, `mqtt_topic_prefix`, `api_version`, `device_count`, `services` | `ts` |
| `devices.inventory` | `devices[]` (serial, product_name) | `ts` |
| `resources.inventory` | `count` | `ts` |
| `services.status` | `services` (name → status map) | `ts` |

### §3 — Maximum Heartbeat Interval

A new config field `heartbeat_max_secs` (default 300 = 5 minutes)
forces a re-publish of **all** topics at a configurable maximum interval,
even when no state has changed. This serves as a liveness confirmation —
subscribers can treat the absence of any message within this window as a
potential problem.

```toml
[mqtt]
# How often to check for state changes (seconds).
# Messages are only published when underlying data has changed.
publish_interval_secs = 5

# Maximum seconds between forced heartbeat publishes.
# Confirms the server is alive. Set to 0 to disable.
heartbeat_max_secs = 300
```

Setting `heartbeat_max_secs = 0` disables the forced heartbeat entirely,
relying solely on MQTT LWT (ADR-031 §3) for crash detection.

### §4 — Implementation Architecture

The periodic task maintains a set of `Option<String>` fingerprint
variables, one per topic. On each tick:

```
for each topic:
    current_fingerprint = build_*_payload(state)  # canonical JSON, no ts
    if force_heartbeat OR current != previous:
        publish(topic, wrap_in_envelope(current_fingerprint))
        previous = current
```

The `build_*_payload()` functions return the payload portion as a
canonical JSON string. This string serves double duty:
1. **Fingerprint** for change detection (string equality comparison)
2. **Payload** for the published message (parsed back to JSON and wrapped
   in the standard ADR-011 message envelope with `v`, `ts`, `type`,
   `server_id`)

### §5 — Windows-Friendly Interface Names

The `netdev` crate returns the Windows adapter GUID (e.g.,
`{952CCB50-2796-4D4F-B599-6ABDC12B3DD6}`) as `Interface.name` on
Windows. This is not human-readable.

`netdev` 0.31 provides `Interface.friendly_name: Option<String>` which
contains the user-visible name (e.g., "Ethernet", "Wi-Fi") on Windows.
On Linux, `friendly_name` is `None` and `name` already contains the
readable name (e.g., `eth0`, `ens192`).

**Resolution:**
- Display names use `friendly_name.unwrap_or(name)` — human-readable on
  all platforms.
- `announce_interface` config matching checks **both** `friendly_name`
  and `name`, so users can specify either the Windows-friendly name
  (`"Ethernet"`) or the system identifier.
- Debug-level logs show both `interface` (display) and `system_name`
  (raw OS identifier) for troubleshooting.

## Consequences

### Positive
- **Eliminates broker churn** — no messages when state is stable
- **Accurate change detection** — subscribers know every message
  represents a real state transition
- **Configurable liveness** — `heartbeat_max_secs` balances
  responsiveness vs. traffic
- **Backward-compatible** — message format, topics, QoS, and retain
  behavior are unchanged; only the *frequency* changes

### Negative
- **Higher latency** — state changes are detected at the poll interval
  (up to `publish_interval_secs` delay). For the current 5s default,
  this is acceptable for lab monitoring but would not suit real-time
  control (which uses REST/gRPC, not MQTT).
- **Fingerprint fragility** — if `serde_json` serialization order
  changes across Rust versions, fingerprints may differ even when data
  is identical, causing a single spurious re-publish (self-correcting
  on the next tick).

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/mqtt.rs` | Add `build_*_payload()` fingerprint builders, `publish_with_payload()` helper, refactor periodic task to compare-and-publish model |
| `crates/ok-fpga-server/src/config.rs` | Add `heartbeat_max_secs` to `MqttConfig`, update `publish_interval_secs` doc |
| `config.toml` | Add `heartbeat_max_secs = 300` with explanatory comments |
| `crates/ok-fpga-server/tests/mqtt_integration.rs` | Rewrite: all messages non-retained, dedicated `okfpga/integration-test/` prefix, add `server.announce` coverage |

## References

- [ADR-011: MQTT Status Publishing](ADR-011-mqtt-publishing.md) — original MQTT architecture (superseded §periodic)
- [ADR-031: MQTT Cross-Subnet Discovery](ADR-031-mqtt-discovery.md) — `server.announce`, LWT, IP resolution
- MQTT Best Practices: [Report by Exception](https://www.hivemq.com/mqtt-essentials/) — publish only on state change
- IoT Anti-Patterns: [Timestamp-Driven Publishing](https://www.hivemq.com/blog/mqtt-essentials-part-6-mqtt-quality-of-service-levels/) — avoid constant publishes just to update timestamps
