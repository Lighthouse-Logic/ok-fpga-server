# Archive — Phases 40–42: MQTT Infrastructure, Singleton Guard & mDNS Hostname

> **Archived:** 2026-08-27
> **Source:** [development-roadmap.md](../development-roadmap.md)

---

## Phase 40 — MQTT Discovery, Report-by-Exception, Hostname Identity

**Goal:** Implement MQTT-based server discovery, report-by-exception
publishing, and hostname-derived server identity for cross-subnet
environments.

**ADRs:**
- [ADR-011 (Addendum B)](../architecture/adr/ADR-011-mqtt.md) — MQTT configuration updates
- [ADR-031](../architecture/adr/ADR-031-mqtt-discovery.md) — MQTT server announce/discovery
- [ADR-032](../architecture/adr/ADR-032-mqtt-report-by-exception.md) — MQTT report-by-exception

**Key Changes:**
- Server publishes retained `server.announce` message on startup to
  `{prefix}/server/announce` with REST URL, gRPC URL, services, etc.
- LWT configured for `server.offline` message on unexpected disconnect
- Report-by-exception: status, services, devices, resources published
  to dedicated MQTT topics on state change
- Server identity derived from hostname (e.g., `okfpga-server-lab-bench-1`)
- MQTT topic prefix: `okfpga/server-{hostname}/`

**Verification:** `cargo test` all passing, MQTT messages verified via
MQTT Explorer desktop app.

---

## Phase 41 — Singleton Instance Guard & Startup Safety

**Goal:** Prevent a second ok-FPGA-Server instance from starting on the
same host, and ensure the server validates all required port bindings
before loading the FrontPanel DLL or registering any services.

**ADR:** [ADR-033](../architecture/adr/ADR-033-singleton-guard.md) — Singleton Instance Guard

**Key Changes:**
- `check_port_availability()` in `main.rs` — synchronous TCP bind at
  the very start of `main()`, before the async runtime
- Only checks enabled services (management always, REST if enabled,
  gRPC if enabled)
- Hard exit with clear error message if any port is in use
- Graceful shutdown publishes empty retained payloads to clear all
  MQTT topics (ADR-031 Addendum B)
- MQTT integration tests verify broker cleanup and broker-unreachable
  resilience

**Deliverables:**

| File | Changes |
|------|---------|
| `main.rs` | `check_port_availability()` guard, reordered initialization |
| `mqtt.rs` | Empty retained payloads on shutdown, broker resilience |
| `main.rs` tests | Port pre-check unit tests (AddrInUse, disabled skip) |
| `mqtt.rs` tests | Retained cleanup, broker-unreachable integration tests |
| `ADR-033` | Singleton guard design document |
| `overview.md` | ADR-033 added to index |

**Verification:** `cargo test` — all passing (port pre-check, MQTT
cleanup, broker resilience). MQTT Explorer confirms clean broker state
after server shutdown.

---

## Phase 42 — mDNS Hostname Identity Alignment

**Goal:** Align the mDNS `instance_name` default with the MQTT `client_id`
pattern so both produce `"okfpga-server-{hostname}"`. Previously mDNS
defaulted to `"okfpga-server"` (no hostname), while MQTT already used
`"okfpga-server-{hostname}"` (Phase 40). This prevented multihomed servers
from being identified as the same server across discovery methods.

**Required By:** Client Phase C-15 (Discovery JSON Normalization).

**Key Changes:**
- Added `MdnsConfig::populate_from_hostname()` to `config.rs`, matching
  the existing `MqttConfig::populate_from_hostname()` pattern
- Sets `instance_name = "okfpga-server-{hostname}"` when the config
  value is at its default (`"okfpga-server"`)
- Called from `main.rs` alongside `cfg.mqtt.resolve_hostname_fields()`

**Deliverables:**

| File | Changes |
|------|---------|
| `config.rs` | `MdnsConfig::populate_from_hostname()` method |
| `main.rs` | Call `cfg.mdns.populate_from_hostname()` at startup |

**Verification:** `cargo check --features full` — passed.

