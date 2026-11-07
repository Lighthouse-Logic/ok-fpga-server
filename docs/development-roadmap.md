# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-11-06 (Phase 48 completed and archived)
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
| **Phase 39** | SSE Event Expansion | ADR-030 | ✅ [Archived](archive/roadmap-phases-39.md) |
| **Phase 40** | MQTT Discovery, Report-by-Exception, Hostname Identity | ADR-011, ADR-031, ADR-032 | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 41** | Singleton Instance Guard & Startup Safety | ADR-033 | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 42** | mDNS Hostname Identity Alignment | — | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 43** | mDNS Stale Registration Cleanup & Hostname Sanitization | — | ✅ [Archived](archive/roadmap-phases-40-43.md) |
| **Phase 44** | Resource Model Consolidation & Enforcement Alignment | ADR-034 (supersedes parts of ADR-003/006/020/024/025/026) | ✅ [Archived](archive/roadmap-phases-44.md) |
| **Phase 45** | Explicit Registration Warnings in CreateResource Responses | ADR-034 (§17) | ✅ [Archived](archive/roadmap-phases-44.md#phase-45--explicit-registration-warnings-in-createresource-responses--0310) |
| **Phase 46** | API Specification: Expand the Overview Chapters (§2.1–§2.9) | — | ✅ [Archived](archive/roadmap-phases-46-47.md#phase-46--api-specification-expand-the-overview-chapters-2129--doc-v100-server-0310) |
| **Phase 47** | OpenAPI / Proto Cite API Specification Section Numbers | — | ✅ [Archived](archive/roadmap-phases-46-47.md#phase-47--openapi--proto-cite-api-specification-section-numbers--doc-v100-server-0310) |
| **Phase 48** | Configurable MQTT Test Broker Host | — | ✅ [Archived](archive/roadmap-phases-48.md) |

---

## 🟢 Now

*(Empty — Phase 48 completed and archived in [docs/archive/roadmap-phases-48.md](archive/roadmap-phases-48.md). Per Roadmap Rule 5, promote the next phase explicitly when ready.)*

---

## 🟡 Next

*(Empty — no phase is planned next.)*

---

## 🔵 Later: Future Horizons

Features in this section represent uncommitted research or long-term capabilities:

* **TriggerOut event log (non-standard Opal Kelly extension)** — D67
  reopened 2026-10-11 (Q-F). Needs a design round and owner approval
  before any work; nothing below is decided except where marked.

  **Why.** With one FrontPanel handle per device (ADR-017/018), every client
  shares one TriggerOut snapshot (D76). `UpdateTriggerOuts` reads **and
  clears** the latches of the whole device, so one client's update can
  consume a trigger another client was waiting for, and no client can tell
  whether, or how many, updates happened since its own last read (the
  known limitation documented for Q-G in the guide §5.4 and ADR-034 §9).

  **Opal Kelly references (what the server must keep matching, D76).**
  - FrontPanel API, `FPGADataPortClassic`:
    https://library.opalkelly.com/library/FrontPanelAPI/classOpalKelly_1_1FPGADataPortClassic.html
  - C header `FrontPanel-6.1.0/.../okFrontPanel.h:1095–1103`
    (`okFrontPanel_UpdateTriggerOuts(okFrontPanel_HANDLE)`,
    `okFrontPanel_IsTriggered(handle, ep, mask)`,
    `okFrontPanel_GetTriggerOutVector(handle, ep)`): `UpdateTriggerOuts`
    takes **only the handle**, no endpoint.
  - `UpdateTriggerOuts`: "query the XEM to determine if any TriggerOuts
    have been activated since the last call."
  - `IsTriggered`: "find out if a particular bit (or bits) on a particular
    TriggerOut endpoint has triggered since the last call to
    UpdateTriggerOuts()" (quote corrected 2026-10-11 against the live page).
  - `GetTriggerOutVector`: "retrieve the full vector of values for a
    particular TriggerOut endpoint" (from the last update).
  - HDL endpoint ports (FrontPanel HDL library): `okWireOut` has only
    `okHE` / `okEH` / `ep_addr` / `ep_datain` — **no read strobe**, so a
    WireOut read is invisible to user logic. `okTriggerOut` has inputs
    `ep_clk` / `ep_trigger`; the latch lives inside the Opal Kelly endpoint
    and is cleared by `UpdateTriggerOuts`. `okPipeOut` / `okBTPipeOut`
    drive an `ep_read` output (FIFO drain). `okRegisterBridge` drives
    `ep_read` (designs may clear on read).

  **Scenario (owner, 2026-10-11).** One FPGA with two network ports. Alice
  holds `port-a` and watches TriggerOut A; Bob holds `port-b` and watches
  TriggerOut B. When Alice calls `UpdateTriggerOuts` she also clears B's
  latch, so Bob may never see his trigger. If Bob could ask "were there
  any B events **newer than my last read**?" before issuing his own
  update, he would know that a trigger happened even though Alice's update
  consumed it.

  **Proposal (best current outline).**
  - Every `UpdateTriggerOuts` the server runs (from any client, the
    `update=true` flag, or a poller) gets a device-wide **sequence number**
    and a **timestamp**.
  - The server appends each **non-zero** TriggerOut EP vector from that
    update to a **bounded, device-wide log**: `{seq, timestamp, ep,
    vector}`.
  - Clients query "events for EP x since seq N" and get the matching
    entries plus the latest seq. Each client **keeps its own N**.
  - **No per-holder server state** (decided; the old per-holder cache S-16
    stays rejected). The server never tracks who has seen what.
  - Clearly labelled **non-standard Opal Kelly**: separate routes / RPCs
    (e.g. `GET …/classic/triggers/out/events?endpoint=0x61&since=N` and a
    `GetTriggerOutEvents` RPC). The standard routes keep exact FrontPanel
    semantics (D76).
  - The log is cleared on reprogram / clear / disconnect, with the
    sequence restarting (or an epoch id) so stale N values are detected.

  **Open points (need owner decisions).**
  - **Log size**: fixed entry count vs time window; per device vs per EP;
    configurable in `config.toml`.
  - **Overflow reporting**: when entries older than N were dropped, the
    reply must say so (e.g. `overflow: true`, `oldest_seq`), so a client
    knows it may have missed events.
  - **`private` / ReadOnly**: a ReadOnly holder must not see events of a
    private TriggerOut EP; whether a ReadOnly holder may read the log at
    all when a private TriggerOut exists (F2 analogy), and whether
    non-holders of the covering resource may read events for that EP.
  - Whether `UpdateWireOuts` gets a matching "last update seq/timestamp"
    so clients can at least detect that someone else refreshed WireOuts.

  **Server-side poller (option).** A per-device, opt-in background task
  calls `UpdateTriggerOuts` every *T* ms and feeds the log, so no event is
  lost between client polls. Trade-offs: latency (≤ *T*) vs USB/PCIe bus
  load and FFI-mutex contention with client traffic; it must take the
  device lock like any other call; it changes what clients see from the
  standard routes (the snapshot is refreshed by the poller), so it must be
  explicitly enabled per device (config or API) and documented. Interaction
  with `update=true` and F2 (a private TriggerOut) must be defined.

  **gRPC streaming (option).** A server-streaming RPC (e.g.
  `WatchTriggerOutEvents(serial, endpoints[], since_seq)`) that replays
  log entries after `since_seq` and then pushes new ones as the poller or
  any update produces them. It maps onto the `WatchDeviceEvents` fan-out
  pattern (44-D, broadcast channel per device); REST could offer the same
  via SSE on `/events`. Slow consumers get the overflow marker instead of
  unbounded buffering.

  **Related design-specific hooks.** This is one of several server-side
  extensions that particular bitfiles need beyond plain FrontPanel calls,
  e.g. Classic-to-AXI address bridges and custom AXI address
  handling (ADR-025 `ResourceAccessPolicy`), and server-side sequences
  (44.5, D12, D40). A common extension mechanism should be considered
  together with this item.

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


* **Role-based access control** — Admin vs Normal user authority levels,
  tied to authenticated identity (mTLS / LDAP, replacing self-asserted
  `X-User-Id`). Scope (resource-management decision D31, see
  [resource-management-recommendations.md](resource-management-recommendations.md)):
  - restrict `ConfigureFPGA` / `ConfigureFPGAFromMemory` / clear
    configuration, which today is a forced override that unlocks, releases
    and deletes every resource on the device (D15b, D26);
  - restrict resource registration and deletion (today: anyone, D15c);
  - optionally restrict writes to reset EPs (today: any holder of an
    unblocked resource, D17);
  - restrict server management (service start/stop/restart, server
    restart).

* **Cross-platform CI** — GitHub Actions matrix for Windows + Ubuntu.

* **Cross-platform packaging** — Windows service installer, systemd unit file,
  Docker image.

* **FPGA dynamic partial reconfiguration** — safe remote flashing with hardware
  fallback recovery.

* **Resource-model follow-ups (moved from Phase 44 draft)** — server-side
  sequences / transactions (embedded engine explored first, D40; replaces
  the old `classic/sequence` endpoint proposal); declarative address carriers for bridge
  designs (old S-21); WireIn shadow read-back (old Q44-6); atomic
  multi-resource acquire (old Q44-12). See
  [resource-management-recommendations.md](resource-management-recommendations.md) §5.

* **Phase 44-E follow-up (owner decision Q3b) — Migrate legacy Tier 3 route unit tests to `EndpointEnforcement::Enforce`:**
  update older route-level validation unit tests (which currently use a
  `test_state()` helper with `EndpointEnforcement::Off` to test query/body
  parsing in isolation) so all unit tests run with
  `EndpointEnforcement::Enforce` enabled.

* **Purge tracked proprietary Opal Kelly FrontPanel SDK/IP files from git (Phase 44-F Q5)** —
  remove tracked proprietary FrontPanel SDK headers, C++ wrappers, sample
  bitfiles, and libraries under `FrontPanel-6.1.0/` from git tracking, keep
  `FrontPanel-6.1.0/` gitignored, and document how developers and CI machines
  install or point `OK_FRONTPANEL_SDK_DIR` / `LD_LIBRARY_PATH` at an external
  Opal Kelly FrontPanel SDK directory.

* **Rust egui demo unit tests** — egui does not provide a built-in test harness
  like iced's `iced_test`; implementing headless UI tests will require
  significant effort.

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
| 38 | User Identification Consistency | 2026-08-23 | ADR-029 | [phase 38](archive/roadmap-phases-38.md) |
| 39 | SSE Event Expansion | 2026-08-24 | ADR-030 | [phase 39](archive/roadmap-phases-39.md) |
| 40 | MQTT Discovery, Report-by-Exception, Hostname Identity | 2026-08-26 | ADR-011(B), ADR-031, ADR-032 | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 41 | Singleton Instance Guard & Startup Safety | 2026-08-26 | ADR-033 | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 42 | mDNS Hostname Identity Alignment | 2026-08-27 | — | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 43 | mDNS Stale Registration Cleanup & Hostname Sanitization | 2026-09-19 | — | [phases 40–43](archive/roadmap-phases-40-43.md) |
| 44 | Resource Model Consolidation & Enforcement Alignment | 2026-11-05 | ADR-034 | [phase 44](archive/roadmap-phases-44.md) |
| 45 | Explicit Registration Warnings in CreateResource Responses | 2026-11-06 | ADR-034 (§17) | [phases 44–45](archive/roadmap-phases-44.md#phase-45--explicit-registration-warnings-in-createresource-responses--0310) |
| 46 | API Specification: Expand the Overview Chapters (§2.1–§2.9) | 2026-11-06 | — | [phases 46–47](archive/roadmap-phases-46-47.md#phase-46--api-specification-expand-the-overview-chapters-2129--doc-v100-server-0310) |
| 47 | OpenAPI / Proto Cite API Specification Section Numbers | 2026-11-06 | — | [phases 46–47](archive/roadmap-phases-46-47.md#phase-47--openapi--proto-cite-api-specification-section-numbers--doc-v100-server-0310) |
| 48 | Configurable MQTT Test Broker Host | 2026-11-06 | — | [phase 48](archive/roadmap-phases-48.md) |

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
