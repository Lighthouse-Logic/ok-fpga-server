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
   time.
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
5. **No auto-promotion** — When a "Now" phase completes, do **not**
   automatically promote the first "Next" item to "Now". The user must
   explicitly request promotion.

---

## 🧭 Horizon Overview

| Horizon | Focus Area | ADR | Status |
|---------|-----------|-----|--------|
| **Phase 28D (Now)** | Dashboard Reliability, Expanded Views & SYZYGY Board Catalog | — | 🟢 Active |
| **Phase 29 (Next)** | Address Range Enforcement for Classic/okRegisterBridge | ADR-024 | 🟡 Planned |
| **Phase 30+ (Later)** | TLS, CI, persistent storage, packaging | — | 🔵 Exploratory |

---

## 🟢 Now: Phase 28D — Dashboard Reliability, Expanded Views & SYZYGY Board Catalog

*Focus: Fix WebSocket auto-refresh bugs (REST + gRPC), add expandable
device/resource rows, server identity on Overview, SYZYGY board catalog
with About links, font readability, and upgrade mdns-sd to 0.13.*

### 1. WebSocket Event Audit & Fixes

Fix missing `broadcast_event()` calls that prevent the dashboard from
auto-refreshing on state changes:

- **REST**: `lock_resource` and `unlock_resource` missing WS broadcasts;
  `configure_fpga` missing `device_programmed` event.
- **gRPC**: **Zero** `broadcast_event()` calls across all gRPC handlers.
  Root cause of "second gRPC user not appearing on Users page."
  Fix: add broadcasts to gRPC heartbeat interceptor (`user_connected`),
  `Disconnect` (`user_disconnected`), `ConfigureFpga`
  (`device_programmed`), and all resource RPCs.

### 2. Overview Page: Description & Hostname

- Add `description` and `hostname` to the `/mgmt/status` response.
- Display description in Server Details table on Overview page.
- Show `Host: <hostname>` under the logo in the sidebar.

### 3. Font Readability

- Brighten `--text-muted` from `#5a6178` → `#8b92a8`.

### 4. Expandable Device Rows

- Enrich `/mgmt/devices` with `is_programmed` and `bitfile` info.
- Add `GET /mgmt/devices/{serial}/syzygy` (management port).
- Click a device row to expand: larger image, bitfile details, SYZYGY
  ports, About links from catalog.

### 5. Expandable Resource Rows

- Enrich `/mgmt/resources` with full resource detail (description,
  access_mode, interfaces, endpoints, address_ranges, metadata).
- Click a resource row to expand and show all fields.

### 6. SYZYGY Board Catalog (TOML)

- New `[catalog]` section in `okfpga-server.toml` with known SYZYGY
  boards, devices, and bitfiles — each with optional About URLs.
- `GET /mgmt/catalog` endpoint for dashboard-only consumption.
- Matching: `product_name` required; `product_model`/`product_version`
  optional (partial match when omitted).

### 7. mDNS: Upgrade mdns-sd to 0.13

- Upgrade from 0.11.5 → 0.13. Newer versions handle IPv6 AAAA on
  Windows better. Falls back to tracing filter if needed.

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

* **mDNS browser unit tests** — deferred from Phase 28A. Test the
  `mdns_browser` module in each of the 4 common libraries (Rust + Python,
  REST + gRPC) with mock service announcements where possible. Also test
  `[server] description` config default/override and TXT record generation
  with the description field.

* **mDNS dynamic bitfile announcements** — update the mDNS TXT records to
  include the currently loaded bitfile description (or "Not Programmed" if the
  device has no bitfile loaded). This would allow GUI clients to see at a
  glance what each discovered server is running before connecting. Requires
  re-announcing the mDNS service whenever the FPGA is programmed or reset.

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
| 28 | mDNS-Powered GUI Discovery + Dashboard Settings | 2026-08-19 | ADR-025 (TBD) | [phase 28](archive/roadmap-phases-28.md) |
| 28C | Settings Lockdown | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |

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
