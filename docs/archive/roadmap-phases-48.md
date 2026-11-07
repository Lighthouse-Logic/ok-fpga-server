# Archived Roadmap Phase: 48 (Configurable MQTT Test Broker Host)

## Phase 48 — Configurable MQTT Test Broker Host ✅ (Server `0.3.10`)

- **Date:** 2026-11-06
- **Scope:**
  - Updated [`crates/ok-fpga-server/tests/mqtt_integration.rs`](../../crates/ok-fpga-server/tests/mqtt_integration.rs) so MQTT integration tests read the target broker hostname and TCP port from environment variables instead of hardcoding `127.0.0.1:1883`:
    - `OKFPGA_TEST_MQTT_HOST` (default: `127.0.0.1`)
    - `OKFPGA_TEST_MQTT_PORT` (default: `1883`)
    - `OKFPGA_TEST_MQTT_STRICT` (when `1` or `true`, unreachable brokers fail immediately instead of skipping gracefully)
  - Documented the environment variables, defaults, and collision-avoidance scheme in the `//!` module doc header of `tests/mqtt_integration.rs`.
  - Added per-process run IDs (`<pid>-<epoch_ms>` via `OnceLock`) and atomic sequence counters (`AtomicU64`) so every test run uses isolated topic prefixes (`okfpga/test/<run_id>/<label>/...`) and unique MQTT client IDs (`okfpga-test-<label>-<run_id>-<seq>`), preventing collisions with live `okfpga/server-<hostname>/...` topics or concurrent test runs on a shared broker.
  - Replaced brittle fixed `sleep(300ms..500ms)` waits with an event-forwarding helper (`spawn_connected`) and deterministic `CONNACK`, `SUBACK`, and `PUBACK` waits over real network RTT.
  - Marked the corresponding open item in [`docs/architecture/adr/ADR-034-resource-model-consolidation.md`](../architecture/adr/ADR-034-resource-model-consolidation.md) (§Open Items) as resolved.
