# ADR-033: Singleton Instance Guard & Startup Safety

**Status**: Implemented ✅
**Date**: 2026-08-26
**Deciders**: Architecture Team
**Phase**: 41

## Context

The ok-FPGA-Server binds several TCP ports (management, REST, gRPC) and
exclusively opens the FrontPanel DLL to communicate with FPGA hardware.
Only one server instance should run per host. Prior to this ADR, if a
user launched a second instance, it would:

- Log `ERROR` messages for port bind failures but **continue running**
- Load the FrontPanel DLL (causing device contention)
- Register duplicate mDNS services
- Connect to the MQTT broker with the same client ID (forcing the
  first instance to disconnect)
- Leave REST/gRPC ports in a failed state

The second instance appeared to start but was non-functional, causing
operator confusion and resource conflicts.

## Decision

### §1 — Early Port Pre-Check

At the very top of `main()`, **before** initializing tracing, loading
the FrontPanel DLL, or registering any services, the server validates
all required port bindings using synchronous `std::net::TcpListener::bind()`.

**Ports checked** (only if the service is enabled in config):

| Port | Condition |
|------|-----------|
| Management | Always (management is always-on) |
| REST | `[rest] enabled = true` |
| gRPC | `[grpc] enabled = true` |

If **any** enabled port is already in use, the server prints a clear
error message and exits with code 1:

```
ERROR: Cannot start ok-FPGA-Server — port conflict detected:
  - port 9090 (management) is already in use
  - port 8088 (REST) is already in use
Is another ok-FPGA-Server instance already running?
Only one instance is allowed per host.
```

### §2 — Startup Ordering

```
main()
  ├─ 1. load_config()
  ├─ 2. check_port_availability(&cfg)    ← NEW: exits if any port in use
  ├─ 3. resolve_hostname_fields()
  ├─ 4. init_tracing()
  ├─ 5. load FrontPanel DLL
  ├─ 6. initial device scan
  ├─ 7. register & start services
  ├─ 8. start management server
  └─ 9. wait for shutdown signal
```

The port check uses synchronous `TcpListener::bind()` (not tokio) because
it runs before the async runtime is initialized. The sockets are
immediately dropped after validation — they are re-bound by each
service's actual startup logic. This creates a small TOCTOU window
(another process could bind the port between check and service start),
but this is acceptable for the use case.

### §3 — Design Rationale

**Port pre-check was chosen over alternatives:**

| Approach | Pros | Cons |
|----------|------|------|
| **Port pre-check** (chosen) | Simple, cross-platform, no external deps | Small TOCTOU window |
| Named mutex / `CreateMutexW` | No TOCTOU | Platform-specific, needs unsafe |
| PID file + `flock()` | Standard Unix pattern | Not robust on crash, Windows needs different API |

The TOCTOU window is acceptable because:
- The check and bind happen within ~100ms
- The only realistic conflict is another ok-FPGA-Server, which would
  also fail its own port check
- Management port binding failure still produces a clear error

### §4 — Graceful Shutdown via API

`POST /mgmt/shutdown` triggers the same graceful shutdown flow as
Ctrl+C. A shared `Arc<Notify>` is passed from `main()` to the
management router. In `main()`, a `tokio::select!` waits for either
`ctrl_c()` or the API shutdown signal — both follow the identical
graceful shutdown path:

1. Broadcast `ServerShutdown` SSE event
2. `service_manager.stop_all()` (includes MQTT retained cleanup)
3. Exit with code 0

Previously, the `mgmt_shutdown` handler called `std::process::exit(0)`,
which bypassed the entire graceful shutdown flow.

## Consequences

### Positive
- **No silent failures** — second instance exits immediately with clear message
- **No resource contention** — DLL, mDNS, MQTT are never touched
- **Simple and cross-platform** — works on Windows 11 and Ubuntu
- **Early fail** — failure detected before any initialization cost

### Negative
- **Small TOCTOU window** — another process could theoretically bind the
  port between the check and the actual service start (~100ms gap)
- **Does not detect non-okfpga processes** — if port 9090 is used by
  an unrelated service, the error message mentions "another ok-FPGA-Server"
  which may be misleading

## Test Coverage

| Test | Description |
|------|-------------|
| `port_check_detects_in_use` | Binds a port, verifies guard detects the conflict |
| `port_check_skips_disabled_services` | Verifies disabled services' ports aren't checked |
| `port_check_detects_multiple_conflicts` | Verifies all enabled ports are checked together |
| `port_check_passes_when_free` | Verifies free ports pass (port 0 = auto-assign) |

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/main.rs` | Add `check_port_availability()`, call at top of `main()`, 4 unit tests |
| `crates/ok-fpga-server/src/management.rs` | Add `shutdown_tx` to `MgmtState`, replace `process::exit(0)` with `Notify` |
| `docs/architecture/adr/ADR-033-singleton-guard.md` | This ADR |
| `docs/architecture/overview.md` | Index entry for ADR-033 |

## References

- [ADR-010: Service Manager](ADR-010-service-manager.md) — service lifecycle
- [ADR-011: MQTT Publishing](ADR-011-mqtt-publishing.md) — broker resilience (Addendum C)
- [ADR-031: MQTT Discovery](ADR-031-mqtt-discovery.md) — retained cleanup on shutdown (Addendum B)
