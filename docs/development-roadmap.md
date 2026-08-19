# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-08-18
> **Architecture Decisions:** See [docs/architecture/overview.md](architecture/overview.md)
> **Archive:** Completed phases are in [docs/archive/](archive/)

---

## 🧭 Horizon Overview

| Horizon | Focus Area | ADR | Status |
|---------|-----------|-----|--------|
| **Phase 26 (Now)** | Endpoint Access Enforcement middleware | [ADR-024](architecture/adr/ADR-024-endpoint-access-enforcement.md) | 🟢 Ready to start |
| **Phase 27 (Next)** | OpenAPI spec gaps + hardening | — | 🟡 Planned |
| **Phase 28+ (Later)** | Read-only resources, TLS, persistent storage, packaging | — | 🔵 Exploratory |

---

## 🟢 Now: Phase 26 — Endpoint Access Enforcement (ADR-024)

*Focus: Implement runtime enforcement so that Classic and AXI data port operations
require the caller to hold a resource covering the target endpoint.*

* [ ] **REST middleware** — Axum layer that checks `X-User-Id` against
      `ResourceRegistry` before dispatching Classic/AXI handlers.
      Returns `403 Forbidden` (RFC 7807) when enforcement is active and the
      user doesn't hold a matching resource.
      *Architecture:* See [ADR-024 §Decision](architecture/adr/ADR-024-endpoint-access-enforcement.md)

* [ ] **gRPC interceptor** — Tonic interceptor with equivalent logic using
      gRPC metadata for user identity. Returns `PERMISSION_DENIED`.
      *Architecture:* See [ADR-024 §Decision](architecture/adr/ADR-024-endpoint-access-enforcement.md)

* [ ] **Configuration** — `endpoint_enforcement` config key with values
      `off` (default), `warn`, `enforce`.
      *Architecture:* See [ADR-024 §Configuration](architecture/adr/ADR-024-endpoint-access-enforcement.md)

* [ ] **Unit tests** — test enforcement on/off/warn modes, lock exclusivity,
      and resource-not-held rejection.

---

## 🟡 Next: Phase 27 — OpenAPI Spec Gaps & Hardening

*Focus: Address the spec gaps identified during ADR-004/005 alignment.*

* [ ] **Add `POST /heartbeat` to `openapi.yaml`** — explicit liveness endpoint
      (currently only in ADR-020 and gRPC proto `Ping` RPC)

* [ ] **Add `POST /resources/{id}/lock` and `/unlock` to `openapi.yaml`** —
      currently only in ADR-020 and gRPC proto `LockResource`/`UnlockResource`

* [ ] **Python gRPC demo unit tests** — requires proto generation step;
      deferred from Phase 25-Fix

* [ ] **Cross-platform CI** — GitHub Actions matrix for Windows + Ubuntu

---

## 🔵 Later: Future Horizons

Features in this section represent uncommitted research or long-term capabilities:

* **gRPC AXI FFI wiring** — all 14 AXI RPCs in `AXIDataPortService` (Lite, Full,
  Stream) are `UNIMPLEMENTED` stubs. REST AXI is fully wired, but gRPC was not
  connected because no hardware with `FPGADataPortAXI` (Gen3-USB /
  `OK_DATAPORT_AXI`) is available for testing. Implement and test when a
  Gen3-USB device is accessible.

* **okREALM_TEST simulation mode** — explore FrontPanel's `okREALM_TEST`
  capability for testing without a physical device. Would enable CI test
  coverage in environments with no hardware access.

* **Hardware integration verification pass** — verify all server endpoints
  against a live server with connected hardware. Includes gRPC reflection,
  REST status/services endpoints, mDNS discovery, and running all 4 example
  client libraries (Rust REST, Rust gRPC, Python REST, Python gRPC) end-to-end.

* **Server-side static resources (bitfile-aware auto-registration)** — imbue
  the server with knowledge of specific bitfiles' capabilities so that when a
  known bitfile is programmed, the server automatically creates the appropriate
  resources without client involvement.

* **In-use protection on re-program** — prevent FPGA re-programming when
  resources are held (shared or exclusive) by any user. Requires user privilege
  levels (admin can override).

* **Read-only resource acquisition** — `AccessMode::ReadOnly` for observe-without-modify
  access. See [ADR-024 Future Work](architecture/adr/ADR-024-endpoint-access-enforcement.md).

* **Address range enforcement for Classic/okRegisterBridge** — developer-defined
  stubs for HDL-specific memory-map enforcement.
  See [ADR-024 Future Work](architecture/adr/ADR-024-endpoint-access-enforcement.md).

* **Native TLS termination** — embedded `rustls` for secure lab deployments
  without a reverse proxy.

* **Persistent resource definitions** — save resource registrations to disk so
  they survive server restarts.

* **Role-based access control** — Admin vs Normal user authority levels;
  restrict `ConfigureFPGA` to admins.

* **Cross-platform packaging** — Windows service installer, systemd unit file,
  Docker image.

* **FPGA dynamic partial reconfiguration** — safe remote flashing with hardware
  fallback recovery.

* **Rust egui demo unit tests** — egui does not provide a built-in test harness
  like iced's `iced_test`; implementing headless UI tests will require
  significant effort.

---

## 📦 Completed Phases

All completed phases are archived in [`docs/archive/`](archive/) with full
implementation details, verification steps, and acceptance criteria.

| Phase | Title | Date | Key ADRs | Archive |
|-------|-------|------|----------|---------|
| 0 | Project Scaffolding | 2026-08-16 | — | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 1 | OpenAPI 3.0 Specification | 2026-08-16 | ADR-004 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 2 | Proto3 gRPC Specification | 2026-08-16 | ADR-005 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 3 | FFI Layer (`ok-fpga-ffi`) | 2026-08-16 | ADR-001 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 4 | Core Domain (`ok-fpga-core`) | 2026-08-16 | ADR-002, 003, 006 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 4E | Service Manager | 2026-08-16 | ADR-010 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 5 | gRPC Server | 2026-08-16 | ADR-005 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 6 | REST Server | 2026-08-16 | ADR-004 | [phases 0–6](archive/roadmap-phases-00-06.md) |
| 7 | Configuration & Logging | 2026-08-16 | — | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 8 | Web Dashboard | 2026-08-16 | ADR-007 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 9 | Authentication Stubs | 2026-08-16 | ADR-008 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 10 | MQTT Publisher | 2026-08-16 | ADR-011 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 11 | mDNS Discovery | 2026-08-16 | ADR-012 | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 12 | Integration Testing | 2026-08-16 | — | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 13 | Documentation & Polish | 2026-08-16 | — | [phases 7–13](archive/roadmap-phases-07-13.md) |
| 14 | Device Scanner | 2026-08-17 | ADR-013→018 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 15 | Hot-Plug Detection | 2026-08-17 | ADR-002 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 16 | Example Clients & E2E | 2026-08-17 | ADR-014 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 17 | HTTP Method Audit | 2026-08-17 | ADR-015 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 18 | Hardware Integration Tests | 2026-08-17 | ADR-016 | [phases 14–18](archive/roadmap-phases-14-18.md) |
| 19 | FFI Endpoint Wiring | 2026-08-17 | ADR-017, 018, 019 | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 20 | Enhanced Resource Registration | 2026-08-17 | ADR-020 | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 21 | User Session Tracking | 2026-08-17 | ADR-020 | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 22 | Event System & Policy Stubs | 2026-08-17 | — | [phases 19–22](archive/roadmap-phases-19-22.md) |
| 23 | Disconnect Detection Fix | 2026-08-18 | — | [phases 23–25](archive/roadmap-phases-23-25.md) |
| 24 | Dashboard Log Streaming | 2026-08-18 | ADR-022 | [phases 23–25](archive/roadmap-phases-23-25.md) |
| 25 | GUI Example Clients | 2026-08-18 | ADR-023 | [phases 23–25](archive/roadmap-phases-23-25.md) |

---

## Feature Flag Summary

| Feature | Default | Dependencies Added | Phase |
|---------|---------|-------------------|-------|
| `rest` | ✅ Yes | axum, tower-http | 6 |
| `dashboard` | ✅ Yes | (static files only) | 8 |
| `grpc` | ❌ No | tonic, prost, tonic-build | 5 |
| `mqtt` | ❌ No | rumqttc | 10 |
| `mdns` | ❌ No | mdns-sd | 11 |
| `full` | ❌ No | all of the above | — |
