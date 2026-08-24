# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-08-23 (Phase 39 defined)
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
| **Phase 34** | Client UX & Device Metadata Improvements | ADR-004 (F), ADR-005 (C) | ✅ Completed |
| **Phase 35** | Client Demo Bug Fixes & README Update | — | ✅ Completed |
| **Phase 36** | Configure FPGA API Alignment | ADR-028 | ✅ [Archived](archive/roadmap-phases-36.md) |
| **Phase 37** | API Response Compliance Audit | — | ✅ [Archived](archive/roadmap-phases-37.md) |
| **Phase 38** | User Identification Consistency | ADR-029 | ✅ [Archived](archive/roadmap-phases-38.md) |
| **Phase 39** | SSE Event Expansion | ADR-030 | 🟢 Now |
| **Phase 40+ (Later)** | TLS, CI, persistent storage, packaging | — | 🔵 Exploratory |

---

## 🟢 Now

### Phase 39 — SSE Event Expansion

**Goal:** Expand the `DeviceEvent` enum and SSE broadcast to include user
session lifecycle, resource lifecycle, and server shutdown events — bringing
the SSE endpoint to parity with the dashboard WebSocket.

**ADR:** [ADR-030](architecture/adr/ADR-030-sse-event-expansion.md)

**New `DeviceEvent` variants:**

| Variant | Trigger | Broadcast Location |
|---------|---------|--------------------|
| `UserConnected` | First API call from a user (new session) | `heartbeat_middleware.rs` |
| `UserDisconnected` (explicit) | User calls disconnect | `server.rs` |
| `UserDisconnected` (timeout) | Sweep detects expired heartbeat | `sweep.rs` |
| `ResourceCreated` | `POST /resources` | `resources.rs` |
| `ResourceDeleted` | `DELETE /resources/{id}` | `resources.rs` |
| `ServerShutdown` | SIGINT/SIGTERM received | `main.rs` |

**Key Decisions (→ ADR-030):**

- `HeartbeatTimeout` folded into `UserDisconnected { reason: "timeout" }`
- `UserSessionRegistry::touch()` returns `bool` (true = new session) — purely
  internal, no network effect
- SSE handler match is exhaustive (compiler-enforced)
- Also fixes client-example SSE path bug (`/events/stream` → `/events`)

**Exit Criteria:**

- `cargo test` — all pass (new enum variants + serialization tests)
- `cargo build` — exhaustive match enforced (no warnings)
- Manual curl test: all 5 new event types appear on SSE stream
- `openapi.yaml` documents all 9 event types
- Rust REST client-example uses correct `/events` path

---

## 🟡 Next

*(Empty — no phase is currently planned for Next.)*

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

* **Read-only resource acquisition** — **Completed in Phase 30.**
  See [ADR-026](architecture/adr/ADR-026-readonly-acquisition.md) and
  [archive](archive/roadmap-phases-30.md).

* **Native TLS termination & certificate-based identity** — embedded `rustls` for
  secure lab deployments without a reverse proxy. When mutual TLS (mTLS) is enabled,
  user identity can be extracted from the client certificate's CN/SAN fields,
  providing cryptographically verified identity without requiring a login flow.
  The `resolve_user_id()` chain (ADR-029) is designed to be extensible: a future
  TLS phase would add certificate identity as the highest-priority source
  (cert CN → body `user_id` → `X-User-Id` header → "Unknown"). This applies to
  both REST (HTTPS) and gRPC (gRPC-TLS channel credentials). See ADR-029 §Future
  Considerations for details.

* **Persistent resource definitions** — save resource registrations to disk so
  they survive server restarts. This would have to do a quick state save of 
  Users, Resources, connected devices and all info about the device (is it
  programmed, with what bitfile and description, by what user) and this would 
  have to be reloaded and cleared after a "restart" (add that option to the 
  Service Management page on the Web Dashboard?)  - There's no benefit to 
  keeping a persistent save file of the "last bitfile programed, last users
  connected" when the server shutsdown and then reload that on next startup, as
  that could be hours or days apart, the connected FPGA could be reprogrammed, etc.
  This is only useful if the server needs to restart (gracefully and quickly)
  Should this info be always written in case a crash/restart occurs. Is that
  likely with RUST? 


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
| 28 | mDNS-Powered GUI Discovery + Dashboard Settings | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |
| 28C | Settings Lockdown | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |
| 28D | Dashboard Reliability, Expanded Views & SYZYGY Board Catalog | 2026-08-19 | — | [phase 28](archive/roadmap-phases-28.md) |
| 29 | Developer-Defined Classic Address Mapping | 2026-08-20 | ADR-025 | [phase 29](archive/roadmap-phases-29.md) |
| 30 | Read-Only Resource Acquisition & Private Data | 2026-08-20 | ADR-026 | [phase 30](archive/roadmap-phases-30.md) |
| 31 | OS-Native mDNS Service Registration | 2026-08-21 | ADR-027 | [phase 31](archive/roadmap-phases-31.md) |
| 32 | Bug Fixes: Users Page, Settings, gRPC Identity | 2026-08-21 | — | [phase 32](archive/roadmap-phases-32.md) |
| 33 | Read-Aware Lock Enforcement | 2026-08-21 | ADR-024, 026 | [phase 33](archive/roadmap-phases-33.md) |
| 34 | Client UX & Device Metadata Improvements | 2026-08-21 | ADR-004 (F), ADR-005 (C) | [phase 34](archive/roadmap-phases-34.md) |
| 35 | Client Demo Bug Fixes & README Update | 2026-08-21 | — | [phase 35](archive/roadmap-phases-35.md) |
| 36 | Configure FPGA API Alignment | 2026-08-23 | ADR-028 | [phase 36](archive/roadmap-phases-36.md) |
| 37 | API Response Compliance Audit | 2026-08-23 | — | [phase 37](archive/roadmap-phases-37.md) |

---

## Feature Flag Summary

| Feature | Default | Dependencies Added | Phase |
|---------|---------|-------------------|-------|
| `rest` | ✅ Yes | axum, tower-http | 6 |
| `dashboard` | ✅ Yes | (static files only) | 8 |
| `grpc` | ❌ No | tonic, prost, tonic-build | 5 |
| `mqtt` | ❌ No | rumqttc | 10 |
| `mdns` | ❌ No | dns-sd-native + mdns-sd (fallback) | 11, 31 |
| `full` | ❌ No | all of the above | — |
