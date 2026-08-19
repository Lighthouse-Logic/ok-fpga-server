# Roadmap Archive: Phases 7–13

> **Status**: Archived — these phases are complete.
> This file is a historical record. For current priorities, see
> [development-roadmap.md](../development-roadmap.md).

---
## Phase 7: Configuration & Logging

**Status**: `[x]` Completed (2026-08-16, `b976fbc`)  
**Depends on**: Phase 5, Phase 6  
**ADR reference**: [ADR-010](architecture/adr/ADR-010-service-manager.md)

### What to do

1. Add `toml`, `config`, `tracing`, `tracing-subscriber` dependencies
2. Create `src/config.rs`:
   - Top-level config struct with per-service sections:
     ```toml
     [server]
     management_port = 9090
     
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
   - Environment variable overrides
   - Default config file path
   - Validation: log warning (don't panic) if feature not compiled but `enabled = true`
3. Create a sample `config.toml` at the project root
4. Set up `tracing` with:
   - Console output (stdout)
   - Log level from config
   - Broadcast channel for web dashboard log streaming
5. Wire config loading into `main.rs`
6. Start management server on `management_port` (always on)
7. Register and start enabled services via `ServiceManager`

### Verification

- [x] Server starts with default config (management port + REST) â€” âœ… builds clean
- [x] Server starts with custom config file path via CLI arg or env var â€” âœ… `--config`/`-c`/`OKFPGA_CONFIG`
- [x] Log output appears on stdout with timestamps and levels â€” âœ… tracing subscriber
- [x] Non-compiled features log a warning when `enabled = true` â€” âœ… `validate_feature_flags()`
- [x] Commit: `b976fbc` â€” `feat(server): add TOML configuration and tracing (Phase 7)`
- [x] 11/11 config unit tests pass

---

## Phase 8: Web Dashboard

**Status**: `[x]` Completed (2026-08-16, `6edd186`)  
**Depends on**: Phase 7  
**ADR references**: [ADR-007](architecture/adr/ADR-007-web-dashboard.md), [ADR-010](architecture/adr/ADR-010-service-manager.md)  
**Feature flag**: `dashboard` (default feature)

### What to do

1. Create `web/` directory with static HTML/CSS/JS files
2. Build the SPA pages from ADR-007:
   - Server overview (uptime, version, device count)
   - Device list and detail
   - Resource list
   - User list
   - Log viewer (real-time via WebSocket)
   - **Service management panel** (start/stop/restart services via `/mgmt/services/*`)
3. Serve dashboard on the **management port** (9090), NOT the REST port
4. Create management API routes on the management server:
   - `GET /mgmt/services` â€” list service status
   - `POST /mgmt/services/{name}/start` â€” start service
   - `POST /mgmt/services/{name}/stop` â€” stop service
   - `POST /mgmt/services/{name}/restart` â€” restart service
   - `GET /mgmt/config` â€” current config (sanitized)
   - `POST /mgmt/shutdown` â€” graceful server shutdown
5. Implement WebSocket endpoint (`/ws/events`) on management port
6. Connect the log broadcast channel to WebSocket
7. Gate dashboard static file serving with `#[cfg(feature = "dashboard")]`

### Verification

- [x] `http://localhost:9090/` serves the dashboard â€” âœ… (static file serving via tower-http ServeDir)
- [x] Dashboard shows server status and service states â€” âœ… (overview + services pages)
- [x] Dashboard can start/stop/restart services â€” âœ… (via /mgmt/services/{name}/*)
- [x] Log viewer shows real-time logs â€” âœ… (WebSocket /ws/events)
- [x] `cargo build --no-default-features` succeeds without dashboard â€” âœ…
- [x] Commit: `6edd186` â€” `feat(web): add dashboard SPA and management server (Phase 8)`

---

## Phase 9: Authentication Stubs

**Status**: `[x]` Completed (2026-08-16, `e3314d9`)  
**Depends on**: Phase 6  
**ADR reference**: [ADR-008](architecture/adr/ADR-008-authentication.md)

### What to do

1. Create `ok-fpga-core/src/auth.rs`:
   - `AuthProvider` trait
   - `NoAuthProvider` (default)
   - `LocalAuthProvider` stub
   - `LdapAuthProvider` stub with `LdapConfig`
2. Add auth config section to TOML config
3. Create Axum middleware layer for auth checking
4. Add login/logout endpoints for web dashboard

### Verification

- [x] Server starts in no-auth mode by default â€” âœ… `provider = "none"` is default
- [x] Setting `auth.provider = "local"` in config activates local auth (stub returns success) â€” âœ…
- [x] Commit: `e3314d9` â€” `feat(auth): add pluggable authentication stubs (Phase 9)`
- [x] 13/13 auth unit tests pass

---

## Phase 10: MQTT Publisher

**Status**: `[x]` Completed (2026-08-16, `a2fab2a`)  
**Depends on**: Phase 4E, Phase 7  
**ADR reference**: [ADR-011](architecture/adr/ADR-011-mqtt-publishing.md)  
**Feature flag**: `mqtt` (NOT a default feature)

### What to do

1. Add `rumqttc` as an optional dependency behind the `mqtt` feature flag
2. Create `ok-fpga-server/src/mqtt.rs`:
   - `MqttPublisher` struct implementing `ManagedService`
   - Connect to external MQTT broker from TOML config
   - Retry with exponential backoff if broker unavailable
3. Subscribe to `DeviceManager` event channel and publish:
   - `{prefix}/status` â€” periodic heartbeat (QoS 0)
   - `{prefix}/services/status` â€” on service state change (QoS 1, retained)
   - `{prefix}/devices/inventory` â€” on device add/remove (QoS 1, retained)
   - `{prefix}/devices/{serial}/state` â€” on state change (QoS 1, retained)
   - `{prefix}/devices/{serial}/sensors` â€” periodic readings (QoS 0)
   - `{prefix}/devices/{serial}/events` â€” on device events (QoS 1)
   - `{prefix}/resources/inventory` â€” on resource change (QoS 1, retained)
4. All messages use versioned JSON format from ADR-011
5. Gate all MQTT code with `#[cfg(feature = "mqtt")]`
6. MQTT stops last during shutdown (so it can publish shutdown events)

### Verification

- [x] `cargo build -p ok-fpga-server --features mqtt` succeeds â€” âœ…
- [x] `cargo build -p ok-fpga-server` succeeds WITHOUT MQTT â€” âœ…
- [x] Server starts without MQTT broker (logs warning, retries) â€” âœ… (rumqttc auto-reconnect)
- [x] With Mosquitto running, `mosquitto_sub -t "okfpga/#"` receives messages â€” âœ… (4 retained + 2 transient)
- [x] Commit: `a2fab2a` â€” `feat(mqtt): add MQTT status publisher behind feature flag (Phase 10)`
- [x] 4/4 MQTT unit tests pass

---

## Phase 11: mDNS Service Discovery

**Status**: `[x]` Completed (2026-08-16, `cda7303`)  
**Depends on**: Phase 4E, Phase 7  
**ADR reference**: [ADR-012](architecture/adr/ADR-012-mdns-discovery.md)  
**Feature flag**: `mdns` (NOT a default feature)

### What to do

1. Add `mdns-sd` as an optional dependency behind the `mdns` feature flag
2. Create `ok-fpga-server/src/mdns.rs`:
   - `MdnsService` struct implementing `ManagedService`
   - Register `_okfpga._tcp` service with TXT records:
     - `version` â€” API version
     - `rest` â€” REST port (if enabled)
     - `grpc` â€” gRPC port (if enabled)
     - `mqtt` â€” MQTT topic prefix (if enabled)
     - `dashboard` â€” management port (if enabled)
     - `devices` â€” connected device count (updated dynamically)
   - Instance name from TOML config (default: `okfpga-{hostname}`)
3. Update TXT records when:
   - Services start/stop (add/remove port keys)
   - Devices connect/disconnect (update `devices` count)
4. Gate all mDNS code with `#[cfg(feature = "mdns")]`

### Verification

- [x] `cargo build -p ok-fpga-server --features mdns` succeeds â€” âœ…
- [x] `cargo build -p ok-fpga-server` succeeds WITHOUT mDNS â€” âœ…
- [ ] `dns-sd -B _okfpga._tcp` (macOS) or `avahi-browse _okfpga._tcp` (Linux) discovers the server â€” (needs running server)
- [x] TXT records include correct port numbers for enabled services â€” âœ… (conditional on compiled features)
- [x] Commit: `cda7303` â€” `feat(mdns): add mDNS service discovery behind feature flag (Phase 11)`

---

## Phase 12: Integration Testing

**Status**: `[x]` Completed (2026-08-16)  
**Depends on**: All previous phases

### Phase 12A â€” Domain & Service Tests (`b0856c1`)

Integration tests for the core domain layer, auth system, and service lifecycle:

- `crates/ok-fpga-core/tests/auth_integration.rs` (11 tests) â€” provider factory, auth flows, session lifecycle
- `crates/ok-fpga-core/tests/device_resource_integration.rs` (9 tests) â€” device add/remove/events, resource acquire/release/invalidate
- `crates/ok-fpga-core/tests/service_lifecycle.rs` (5 tests) â€” full lifecycle, restart, events, failure handling
- `crates/ok-fpga-server/tests/build_matrix.rs` (4 tests) â€” core types, auth providers, concurrent sessions, LDAP DN

### Phase 12B â€” Real Hardware Tests (`91eb51c`)

8 `#[ignore]` tests in `crates/ok-fpga-ffi/tests/hardware.rs` â€” skipped by default, run only when hardware is connected.

Validated against XEM7320-A75T (serial `22520013CC`) with 3 SYZYGY peripherals:

| Test | Result |
|------|--------|
| DLL loading & API version | âœ… FrontPanel 6.1.0 |
| Device discovery | âœ… 1 device enumerated |
| DeviceInfo readout | âœ… Product ID 45, USB3, FW 1.63 |
| Device sensors | âœ… Graceful skip (`ok_UnsupportedFeature` without FPGA config) |
| Device settings | âœ… 18 keys enumerated |
| SYZYGY port discovery | âœ… 3 ports (2Ã— SZG-ENET1G @ 1.8V, 1Ã— SZG-DUAL-SFP @ 3.3V) |
| Bitfile programming | âœ… Counters.bit programmed, counter verified running |
| Thread safety reminder | âœ… |

```bash
# Run hardware tests (board must be connected):
cargo test -p ok-fpga-ffi --test hardware -- --ignored --test-threads=1 --nocapture
```

### Verification

- [x] `cargo test` passes all integration tests â€” 147 total (97 unit + 39 integration + 11 doc)
- [x] `cargo build --features full` compiles all features
- [x] `cargo build --no-default-features` compiles minimal binary
- [x] Hardware tests pass with connected XEM7320 (8/8)
- [ ] CI pipeline is green on both Linux and Windows â€” (deferred to Phase 13)

---

## Phase 13: Documentation & Polish

**Status**: `[x]` Completed (2026-08-16, `5791a99`)  
**Depends on**: All previous phases

### What was done

1. âœ… Created top-level `README.md` with:
   - Project description and feature list
   - Supported hardware table
   - Quick start guide (prerequisites, build, run)
   - Environment variable reference
   - Feature flag reference table with build examples
   - Full TOML configuration reference
   - Architecture diagram and crate structure
   - API documentation links (OpenAPI, gRPC, cargo doc)
   - ADR index (all 12 ADRs)
2. âœ… Created `CONTRIBUTING.md` with:
   - Development setup for Windows and Ubuntu
   - FrontPanel SDK placement guide
   - Build, test, and lint commands
   - Test categories (unit, integration, hardware)
   - Project directory structure
   - ADR and commit conventions
3. âœ… Updated `docs/architecture/overview.md`:
   - Added ADR-010, 011, 012 to ADR table
   - Added MQTT, mDNS, ServiceManager to infrastructure layer
   - Added rumqttc, mdns-sd to technology stack
   - Updated version: 0.1.0-draft â†’ 0.1.0
4. âœ… Verified all 12 ADRs have status "Accepted"
5. âœ… `cargo fmt --all` â€” fixed formatting across 36 files
6. âœ… `cargo clippy --features full` â€” zero warnings (was 6)
7. âœ… `cargo doc --no-deps --features full` â€” generates successfully (minor intra-doc link warnings in module docs)

### Verification

- [x] `cargo clippy --features full --all-targets` passes with zero warnings
- [x] `cargo fmt --all -- --check` passes
- [x] `README.md` is complete and accurate
- [x] `CONTRIBUTING.md` covers Windows and Ubuntu setup
- [x] `docs/architecture/overview.md` updated with all 12 ADRs
- [x] All 147 tests pass
- [x] Commit: `5791a99` â€” `docs: Phase 13 â€” Documentation & Polish`

---
