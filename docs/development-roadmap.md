# ok-FPGA-Server — Development Roadmap

> **Status:** Living Document
> **Last Updated:** 2026-08-21
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
| **Phase 33** | Read-Aware Lock Enforcement | ADR-024, 026 | ✅ Completed |
| **Phase 34 (Now)** | Client UX & Device Metadata Improvements | ADR-005 (addendum) | 🚧 In Progress |
| **Phase 35+ (Later)** | TLS, CI, persistent storage, packaging | — | 🔵 Exploratory |

---

## 🟢 Now

### Phase 34 — Client UX & Device Metadata Improvements

**Status:** 🚧 In Progress  
**Triggered by:** Live testing with multiple GUI clients  
**Scope:** 2 bugs + 3 features across server + all 8 GUI demos  
**Amends:** ADR-005 (addendum), OpenAPI spec, proto spec

#### Bug 34-A: gRPC lock button does not toggle (gRPC clients only)

**Root cause:** The proto `Resource` message has no `locked_by` field. The
gRPC client libraries hardcode `locked_by: None` in `resource_from_response()`
because there is nothing to read from the proto response. The lock button UI
reads `locked_by` to decide its label — since it is always `None` on the
gRPC side, the button never toggles.

The REST clients are **not affected** — the REST JSON response includes
`locked_by` and the button works correctly.

**Affected clients:** All 4 gRPC GUI demos (Rust iced, Rust egui, Python
tkinter, Python DearPyGui) and their 2 common libraries.

**Fix:**
- Add `string locked_by = 8` to the proto `Resource` message
- Server: populate `locked_by` in all gRPC `Resource` response builders
- Rust gRPC common: read `r.locked_by` instead of hardcoding `None`
- Python gRPC common: read `r.locked_by` instead of hardcoding `None`

#### Feature 34-B: SYZYGY peripherals in device dropdown (all 8 GUI demos)

The "Select Device" dropdown currently shows `{serial} — {product_name}`.
When a device has SYZYGY peripherals attached, the dropdown should also show
the populated SYZYGY ports in order:

```
22520013CC — XEM7320-A75T | 0: SZG-SENSOR1, 2: SZG-ADC (SZG-ADC-LTC2324)
```

Format per port: `{port_index}: {product_name}` — if `product_model` differs
from `product_name`, append `({product_model})`. Skip non-existent/unpopulated
ports.

**Approach:** Option A — fetch SYZYGY at device-list time. After
`list_devices()` returns, the common library fetches SYZYGY ports for each
device and stores them in `DeviceInfo`. A shared `device_display_label()`
helper formats the dropdown label.

**Files:** 4 common libraries (data model + SYZYGY fetch + label formatter),
8 GUI front-ends (use formatter in device picker).

#### Feature 34-C: Bitfile path, filename, and MD5 hash in device info

When the device panel shows a programmed device (expanded), it should display:
- The filename of the programmed bitfile
- The full path (for server-path mode)
- An MD5 hash of the bitfile content

**Core struct:** `BitfileRecord` gains `file_path: Option<String>` and
`md5_hash: Option<String>`.

**Server-path mode** (JSON `{"bitfile_path": "..."}`):
- `filename` = basename of the path
- `file_path` = full path as provided
- `md5_hash` = compute from reading the file

**Binary upload mode** (client-side bitfile):
- Add a `filename` field to the REST multipart form and to the gRPC
  `ConfigureFPGAChunk` message so clients can supply the original filename
  alongside the binary data. If omitted, fall back to `"uploaded"`.
- `file_path` = `None`
- `md5_hash` = compute from received bytes

**Spec changes:**
- OpenAPI: add `filename` to configure multipart form, add `file_path` and
  `md5_hash` to `BitfileInfo` schema
- Proto: add `filename` to `ConfigureFPGAChunk`, add `file_path` and
  `md5_hash` to `BitfileInfo` message

**Dashboard:** `web/app.js` — display `file_path` and `md5_hash` in the
expanded device bitfile panel.

#### Bug 34-D: "Programmed by: api" instead of actual user (REST only)

The REST `configure_fpga()` handler hardcodes `programmed_by: "api"`. It
should extract `X-User-Id` from the request headers, falling back to
`"Unknown"` if absent. The gRPC side already works correctly.

Also: `filename` is hardcoded to `"uploaded"` even for server-path mode where
the actual path is known. Fixed as part of 34-C.

**Files:** `ok-fpga-server/src/rest/devices.rs` (configure_fpga handler)

#### Feature 34-E: Register as Shared or Exclusive (all 8 GUI demos)

All 8 GUI demos hardcode `resource_type: "shared"` in `create_resource()`.
The single "3. Register" button should become two buttons:
- "3. Register (Shared)"
- "Register (Exclusive)"

**Files:** 4 common libraries (`create_resource` gains `resource_type` param),
8 GUI front-ends (split button).

---

#### ADR Impact Assessment

| ADR | Type | Reason |
|-----|------|--------|
| ADR-004 | Addendum | OpenAPI spec gains `filename` in configure form, `file_path`/`md5_hash` in `BitfileInfo` |
| ADR-005 | Addendum | `locked_by` added to `Resource`, `filename` to `ConfigureFPGAChunk`, `file_path`/`md5_hash` to `BitfileInfo` |
| OpenAPI spec | Schema changes | `filename` in configure form, `file_path`/`md5_hash` in `BitfileInfo` |
| Proto spec | Message changes | Same as above |

> **Note:** Bug 34-A and Bug 34-D are straightforward fixes. Features 34-B
> and 34-E are client-only UI changes. Feature 34-C extends the API schema
> and is documented via an addendum to ADR-005.


---

## 🟡 Next

*No phase queued. Promote a "Later" item to begin new work.*

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

* **Native TLS termination** — embedded `rustls` for secure lab deployments
  without a reverse proxy.

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
