# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-08-19
> **Architecture Decisions:** See [docs/architecture/overview.md](architecture/overview.md)
> **Archive:** Completed phases are in [docs/archive/](archive/)

---

## 📜 Roadmap Rules

1. **Three horizons** — The roadmap always maintains three sections: **Now**
   (active work), **Next** (planned milestones), and **Later** (future
   horizons). None of these sections may be dropped, even if empty.
2. **One "Now" at a time** — Only one phase should be in "Now" at any given
   time. When it completes, the first item from "Next" is promoted.
3. **Living document** — Update this file whenever plans change, phases
   complete, or new features are identified.
4. **Archive completed phases** — When a phase is completed, its detailed
   description must be moved to a file in `docs/archive/`. Follow these
   guidelines:
   - **Append** to the most recent archive file if the last 3 phases in
     that file are related to the newly completed phase (same feature area,
     same ADR family, or part of the same initiative).
   - **Create a new archive file** if the newly completed phase is
     substantially different from the phases in the most recent archive
     (e.g., infrastructure vs feature work, different ADR family).
   - **Consider upcoming work** — if the Now and Next phases are related
     to the newly completed phase, prefer creating a new archive that can
     later absorb those phases too, avoiding frequent file splits.
   - Archive files are named `roadmap-phases-NN-MM.md` where NN–MM is the
     range of phases contained.
   - Update the completed phases table in this file to link to the archive.

---

## 🧭 Horizon Overview

| Horizon | Focus Area | ADR | Status |
|---------|-----------|-----|--------|
| **Phase 28 (Now)** | mDNS-Powered GUI Discovery + Dashboard Settings | ADR-025 (TBD) | 🟡 Planned |
| **Phase 29 (Next)** | Address Range Enforcement for Classic/okRegisterBridge | ADR-024 | 🟡 Planned |
| **Phase 30+ (Later)** | TLS, CI, persistent storage, packaging | — | 🔵 Exploratory |

---

## 🟢 Now: Phase 28 — mDNS-Powered GUI Discovery + Dashboard Settings

*Focus: All GUI example clients discover servers via mDNS before connecting.
The server gains a runtime-editable Settings panel in the Web Dashboard.*

### 28A — GUI mDNS Server Discovery

All four GUI demos (Rust REST/iced, Rust REST/egui, Python REST/DearPyGui,
Python gRPC/DearPyGui) currently require the user to manually enter a server
URL. This phase adds an mDNS listener that discovers `_okfpga._tcp` services
on the local subnet and presents them in a dropdown **before** the manual
"Connect to Board" flow.

* [ ] **mDNS browser in GUI demos** — each demo listens for `_okfpga._tcp`
      announcements and populates a "Discovered Servers (mDNS)" dropdown.
      Selecting a server auto-fills the URL/port fields from the TXT records.
* [ ] **Server `description` TXT field** — add a `description` key to the
      mDNS TXT record so the dropdown can show a human-readable label
      (e.g., "Lab Bench 1 — XEM8320"). The description is configured via
      `[server] description = "..."` in `okfpga-server.toml` and defaults to
      the `instance_name`.
* [ ] **Server config: `[server] description`** — add a `description` field
      to `ServerConfig` (TOML), defaulting to `instance_name`. Propagate to
      mDNS TXT records and the `GET /server/info` REST response.

### 28B — Web Dashboard Settings Panel

The SPA Web Dashboard gains a new "Settings" panel exposing runtime-
configurable server settings.

* [ ] **Settings panel UI** — new tab/page in the dashboard exposing:
  - **Server description** (`server.description`) — editable text field
  - **Endpoint enforcement level** (`access_control.endpoint_enforcement`) —
    dropdown: off / warn / enforce
  - **Other runtime settings** as they are added in future phases
* [ ] **REST API for settings** — `GET /server/settings` and
      `PUT /server/settings` endpoints for reading and updating runtime config.
* [ ] **Unauthenticated access warning** — when `endpoint_enforcement` is
      shown in the dashboard, display a prominent warning:
      > ⚠️ Any user with dashboard access can change this setting.
      > For production environments, disable dashboard settings exposure
      > via `[dashboard] expose_settings = false` in `okfpga-server.toml`.
* [ ] **`expose_settings` config switch** — `[dashboard] expose_settings`
      (boolean, default `true`). When `false`, the Settings panel is hidden
      and the `PUT /server/settings` endpoint returns `403 Forbidden`.

---

## 🟡 Next: Phase 29 — Address Range Enforcement for Classic/okRegisterBridge

*Focus: Extend ADR-024 endpoint enforcement to cover memory-mapped address
ranges used by Classic `okRegisterBridge` designs.*

ADR-024 currently enforces access on Classic **endpoint numbers** (wire, trigger,
pipe addresses 0x00–0xFF) and AXI **address ranges**. However, Classic designs
that use the `okRegisterBridge` HDL module expose a 32-bit register address
space accessed via `ReadRegister` / `WriteRegister` REST and gRPC operations.
These register addresses are **not** Classic endpoint numbers — they are
memory-mapped addresses that pass through the `okRegisterBridge` on the FPGA.

The `address_ranges` field already exists on `Resource` (used for AXI today).
This phase extends enforcement so that `ReadRegister` / `WriteRegister`
operations are also checked against `address_ranges` when a resource declares
them.

* [ ] **`check_address_access()` for register operations** — the existing
      `check_address_access()` in `resource.rs` already validates AXI addresses
      against `address_ranges`. Wire it into the REST `registers/read` and
      `registers/write` handlers and the gRPC `ReadRegister` / `WriteRegister` /
      `ReadRegisters` / `WriteRegisters` RPCs.
* [ ] **Developer-defined Classic endpoint → address mapping hook** — add an
      optional `ResourceAccessPolicy` trait/stub that lets developers specify
      how their Classic FrontPanel endpoints map to address ranges (e.g.,
      "WireIn 0x01 selects the address, PipeIn 0x80 writes data"). The server
      calls this before Classic wire/pipe operations, passing the endpoint and
      the resource's declared address ranges. Default impl: pass-through
      (no mapping, current behavior preserved).
* [ ] **Unit tests** — test register address enforcement on/off/warn modes,
      test address-in-range vs out-of-range, test legacy resources (no
      `address_ranges` = covers all), test lock exclusivity for address ranges.

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


* **Native TLS termination** — embedded `rustls` for secure lab deployments
  without a reverse proxy.

* **Persistent resource definitions** — save resource registrations to disk so
  they survive server restarts.

* **Role-based access control** — Admin vs Normal user authority levels;
  restrict `ConfigureFPGA` to admins.

* **Cross-platform CI** — GitHub Actions matrix for Windows + Ubuntu.

* **Cross-platform packaging** — Windows service installer, systemd unit file,
  Docker image.

* **FPGA dynamic partial reconfiguration** — safe remote flashing with hardware
  fallback recovery.

* **Rust egui demo unit tests** — egui does not provide a built-in test harness
  like iced's `iced_test`; implementing headless UI tests will require
  significant effort.

* **MQTT server discovery messages** — extend the MQTT heartbeat/status
  payload to include full server connectivity information (REST host:port,
  gRPC host:port, dashboard URL, server description, device count). This
  would allow any MQTT subscriber to discover and connect to the server even
  across subnets — provided both the server and the subscriber share the same
  MQTT broker. Unlike mDNS (which is link-local only), MQTT-based discovery
  works across routed networks. The MQTT topic should be
  `{prefix}/server/announce` with a retained message so new subscribers
  receive the server info immediately upon connecting to the broker.

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
| 26 | Endpoint Access Enforcement | 2026-08-19 | ADR-024 | [phases 26–27](archive/roadmap-phases-26-27.md) |
| 27 | OpenAPI Spec Gaps | 2026-08-19 | — | [phases 26–27](archive/roadmap-phases-26-27.md) |

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
