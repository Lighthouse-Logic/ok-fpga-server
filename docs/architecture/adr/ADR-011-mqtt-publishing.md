# ADR-011: MQTT Status Publishing

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

Lab environments often have many monitoring systems (dashboards, alerting tools, data loggers) that need to observe FPGA device status without maintaining persistent connections to the ok-FPGA-Server. MQTT is the standard protocol for this pattern — lightweight, decoupled, and widely supported.

## Decision

### MQTT is Publish-Only

The ok-FPGA-Server acts as an **MQTT publisher only**. It publishes status and event messages to an external MQTT broker (e.g., Mosquitto). It does NOT subscribe to any command topics.

**Rationale**: MQTT is a poor fit for command/control (no request-reply, no error codes, no backpressure for binary data). Commands belong in REST or gRPC.

### External Broker Required

The server always connects to an external MQTT broker. It does not embed a broker. When MQTT is enabled but the broker is unavailable, the server logs warnings and retries with exponential backoff. It does not fail to start.

### Feature Flag

MQTT support is behind the `mqtt` Cargo feature flag and is NOT a default feature. It requires the `rumqttc` dependency.

```toml
[mqtt]
enabled = false
broker_url = "mqtt://localhost:1883"
client_id = "okfpga-server-01"
topic_prefix = "okfpga/server01"
publish_interval_secs = 5
```

### Topic Structure

All topics are prefixed with `{topic_prefix}/` from the TOML config. The `{topic_prefix}` is typically `okfpga/{server_id}`.

```
{prefix}/
├── status                          # Server heartbeat (periodic)
├── services/status                 # Which services are running (on change)
├── devices/
│   ├── inventory                   # Full device list (on change)
│   └── {serial}/
│       ├── state                   # Connected/Programmed (on change)
│       ├── sensors                 # Sensor readings (periodic)
│       ├── syzygy                  # SYZYGY port status (on change)
│       └── events                  # Add/remove/program events
└── resources/
    ├── inventory                   # Full resource list (on change)
    └── {resource_id}/state         # Resource state changes (on change)
```

### Message Format

All messages are JSON with a mandatory version field for forward compatibility:

```json
{
  "v": 1,
  "ts": "2026-08-16T20:00:00Z",
  "type": "device.sensors",
  "server_id": "okfpga-lab-bench-1",
  "payload": { ... }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `v` | integer | Schema version. Starts at 1. Incremented on breaking changes. |
| `ts` | string | ISO 8601 timestamp (UTC). |
| `type` | string | Message type (dotted notation, e.g., `server.status`, `device.state`). |
| `server_id` | string | Server instance name from TOML config. |
| `payload` | object | Type-specific data. New fields may be added without bumping `v`. |

### Message Types

#### `server.status` (periodic heartbeat)
```json
{
  "v": 1, "ts": "...", "type": "server.status",
  "server_id": "lab-bench-1",
  "payload": {
    "uptime_secs": 86400,
    "device_count": 3,
    "api_version": "1.0.0"
  }
}
```

#### `services.status` (on change)
```json
{
  "v": 1, "ts": "...", "type": "services.status",
  "server_id": "lab-bench-1",
  "payload": {
    "services": {
      "rest": "running",
      "grpc": "running",
      "mqtt": "running",
      "mdns": "stopped"
    }
  }
}
```

#### `device.state` (on change)
```json
{
  "v": 1, "ts": "...", "type": "device.state",
  "server_id": "lab-bench-1",
  "payload": {
    "serial": "ABC1234567",
    "product_name": "XEM8320-AU25P",
    "state": "programmed",
    "bitfile": "design_v2.bit"
  }
}
```

#### `device.sensors` (periodic)
```json
{
  "v": 1, "ts": "...", "type": "device.sensors",
  "server_id": "lab-bench-1",
  "payload": {
    "serial": "ABC1234567",
    "sensors": [
      { "name": "FPGA Die Temp", "type": "temperature", "value": 42.5, "unit": "°C" },
      { "name": "VCCINT", "type": "voltage", "value": 0.85, "unit": "V" }
    ]
  }
}
```

#### `device.event` (on occurrence)
```json
{
  "v": 1, "ts": "...", "type": "device.event",
  "server_id": "lab-bench-1",
  "payload": {
    "event": "added",
    "serial": "ABC1234567",
    "product_name": "XEM8320-AU25P",
    "interface": "USB 3.0"
  }
}
```

#### `resource.state` (on change)
```json
{
  "v": 1, "ts": "...", "type": "resource.state",
  "server_id": "lab-bench-1",
  "payload": {
    "resource_id": "ABC123:dac_ch1",
    "name": "dac_ch1",
    "state": "in_use_shared",
    "holders": ["alice", "bob"]
  }
}
```

### QoS Levels

| Topic | QoS | Rationale |
|-------|-----|-----------|
| `status` (heartbeat) | 0 | Transient — next heartbeat replaces it |
| `devices/{serial}/sensors` | 0 | Periodic — loss of one reading is acceptable |
| `devices/{serial}/state` | 1 | State changes should be delivered reliably |
| `devices/{serial}/events` | 1 | Device connect/disconnect should not be lost |
| `devices/inventory` | 1 | Inventory should be delivered reliably |
| `resources/*/state` | 1 | Resource state changes should be delivered |
| `services/status` | 1 | Service state changes should be delivered |

### Retained Messages

The following topics use MQTT retained messages so new subscribers immediately get the current state:

- `{prefix}/status` — current server status
- `{prefix}/services/status` — current service states
- `{prefix}/devices/inventory` — current device list
- `{prefix}/devices/{serial}/state` — current device state
- `{prefix}/resources/inventory` — current resource list

### Security Considerations (Private Network)

| Measure | Details |
|---------|---------|
| No command topics | Prevents unauthorized control via MQTT |
| Topic prefix isolation | Each server uses a unique prefix, avoiding collision |
| No credentials in payloads | Sensor data and events contain no secrets |
| Broker-level ACLs | Mosquitto supports topic-level access control |
| Optional TLS | Broker can be configured with TLS independently |
| Client ID uniqueness | TOML `client_id` prevents session conflicts |

### Rust Implementation

The MQTT publisher runs as a `ManagedService` (ADR-010):

```rust
pub struct MqttPublisher {
    config: MqttConfig,
    client: AsyncClient,
    status: AtomicServiceStatus,
}

impl ManagedService for MqttPublisher {
    fn name(&self) -> &str { "mqtt" }
    // ...
}
```

It subscribes to the `DeviceManager` event channel and publishes corresponding MQTT messages.

## Consequences

### Positive
- **Decoupled monitoring** — clients don't need persistent server connections
- **Extensible** — new message types can be added without breaking existing consumers
- **Standard protocol** — any MQTT client (Python, Node.js, C, etc.) can subscribe
- **Low overhead** — JSON messages are small, QoS 0 for periodic data

### Negative
- **External dependency** — requires a running MQTT broker
- **Eventual consistency** — subscribers may miss messages during broker downtime
- **No guaranteed ordering** — MQTT does not guarantee message ordering across topics

## References

- [MQTT v5.0 Specification](https://docs.oasis-open.org/mqtt/mqtt/v5.0/mqtt-v5.0.html)
- [rumqttc Crate](https://crates.io/crates/rumqttc)
- [Mosquitto Broker](https://mosquitto.org/)
- [ADR-010 Service Manager](ADR-010-service-manager.md) — feature flags and lifecycle
