# ADR-010: Service Manager, Feature Flags & Runtime Configuration

**Status**: Implemented  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server exposes multiple network-facing services (REST API, gRPC API, MQTT publisher, mDNS discovery, Web Dashboard). Different deployment scenarios require different combinations:

- A headless lab controller may need only REST + MQTT
- A developer workstation may need REST + Dashboard + gRPC
- A minimal embedded deployment may need only REST
- CI/CD environments need none of the above (just the core library for testing)

Additionally, operators need to start/stop individual services at runtime without restarting the entire process.

## Decision

### 1. Cargo Feature Flags (Compile-time)

Each optional service is gated behind a Cargo feature flag. This allows binary size reduction and eliminates unused dependencies.

```toml
[features]
default = ["rest", "dashboard"]
rest      = ["dep:axum", "dep:tower-http"]
grpc      = ["dep:tonic", "dep:prost"]
mqtt      = ["dep:rumqttc"]
mdns      = ["dep:mdns-sd"]
dashboard = []
full      = ["rest", "grpc", "mqtt", "mdns", "dashboard"]
```

**Default features**: `rest` + `dashboard` — the most common deployment scenario. gRPC, MQTT, and mDNS are opt-in because they add significant dependencies or require external infrastructure.

**Code pattern**:
```rust
#[cfg(feature = "rest")]
mod rest_service;

#[cfg(feature = "grpc")]
mod grpc_service;
```

### 2. TOML Runtime Configuration

Each service has an `enabled` flag in the TOML config. A service can only be enabled at runtime if the corresponding feature flag was compiled in. If a feature is not compiled but `enabled = true` is set in TOML, the server logs a warning and ignores the setting.

```toml
[server]
management_port = 9090    # Always-on management + dashboard port

[rest]
enabled = true
bind = "0.0.0.0"
port = 8080

[grpc]
enabled = true
bind = "0.0.0.0"
port = 50051

[mqtt]
enabled = false
broker_url = "mqtt://localhost:1883"
client_id = "okfpga-server-01"
topic_prefix = "okfpga/server01"
publish_interval_secs = 5

[mdns]
enabled = false
instance_name = "okfpga-lab-bench-1"

[dashboard]
enabled = true
```

### 3. Management Server (Always-On)

A lightweight HTTP server on a dedicated management port (default 9090) that:
- **Always runs** — cannot be stopped except by killing the process
- Serves the Web Dashboard (if the `dashboard` feature is compiled in)
- Exposes a management API for service lifecycle control

This solves the circular dependency problem: the Dashboard cannot stop the REST server if they share the same HTTP listener, because they are on separate ports.

```
┌─────────────────────────────────────────────────────────┐
│                    ok-fpga-server                        │
│                                                          │
│  ┌──────────────────────────┐  Always on, non-stoppable │
│  │  Management Server :9090 │                            │
│  │  ├─ Web Dashboard (SPA)  │                            │
│  │  └─ /mgmt/services/*    │  Service lifecycle API      │
│  └──────────────────────────┘                            │
│                                                          │
│  ┌──────────┐ ┌──────────┐ ┌──────┐ ┌──────┐  Optional │
│  │REST :8080│ │gRPC:50051│ │ MQTT │ │ mDNS │  services │
│  └──────────┘ └──────────┘ └──────┘ └──────┘           │
│                                                          │
│  ┌──────────────────────────────────────────┐           │
│  │         ServiceManager (core)             │           │
│  └──────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────┘
```

### 4. ServiceManager Abstraction

A centralized `ServiceManager` in `ok-fpga-core` owns the lifecycle of all optional services:

```rust
/// Trait that all manageable services implement.
pub trait ManagedService: Send + Sync {
    /// Human-readable service name (e.g., "rest", "grpc", "mqtt", "mdns").
    fn name(&self) -> &str;

    /// Current status of the service.
    fn status(&self) -> ServiceStatus;

    /// Start the service. Returns error if already running.
    async fn start(&self) -> Result<(), ServiceError>;

    /// Gracefully stop the service. Returns error if not running.
    async fn stop(&self) -> Result<(), ServiceError>;

    /// Stop then start. Default implementation calls stop() then start().
    async fn restart(&self) -> Result<(), ServiceError> {
        self.stop().await?;
        self.start().await
    }
}

pub enum ServiceStatus {
    Running,
    Stopped,
    Starting,
    Stopping,
    Failed { error: String },
    NotCompiled,
}

pub struct ServiceManager {
    services: HashMap<String, Arc<dyn ManagedService>>,
    event_tx: broadcast::Sender<ServiceEvent>,
}
```

### 5. Management API

Exposed on the management port:

| Method | Path | Description |
|--------|------|-------------|
| GET | `/mgmt/services` | List all services and their status |
| POST | `/mgmt/services/{name}/start` | Start a stopped service |
| POST | `/mgmt/services/{name}/stop` | Stop a running service |
| POST | `/mgmt/services/{name}/restart` | Restart a service |
| GET | `/mgmt/config` | Current config (sanitized, no secrets) |
| POST | `/mgmt/shutdown` | Graceful server shutdown |

### 6. Service Lifecycle via REST and gRPC

Both the REST API and gRPC API expose service lifecycle endpoints that delegate to the `ServiceManager`:

- A service restarting itself uses the "respond then restart" pattern: send a `202 Accepted` response, then schedule the restart via `tokio::spawn` with a brief delay.
- MQTT and mDNS are not command/control channels and cannot manage services.

### 7. Shutdown Ordering

Services are independent and can be stopped in any order, with two exceptions:
1. **MQTT stops last** among optional services — so it can publish "service X stopped" events for other services
2. **Management server stops truly last** — only on process exit

Startup order: Management server first, then all enabled services in parallel.

## Consequences

### Positive
- **Minimal binaries** — developers can exclude unused services entirely
- **Runtime flexibility** — operators can enable/disable services without recompilation
- **No circular dependencies** — management port is always available
- **Centralized lifecycle** — `ServiceManager` provides a single API for all consumers
- **Testable** — services can be tested independently

### Negative
- **Two HTTP listeners** — management port + REST port adds slight complexity
- **Feature flag combinatorics** — must test that all feature combinations compile
- **Delayed self-restart** — the 202 + spawn pattern is slightly less intuitive than synchronous restart

## References

- [Cargo Features](https://doc.rust-lang.org/cargo/reference/features.html)
- [ADR-004 REST API Design](ADR-004-rest-api-design.md) — service management endpoints
- [ADR-005 gRPC API Design](ADR-005-grpc-api-design.md) — service lifecycle RPCs
