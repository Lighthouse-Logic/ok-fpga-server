# Archived Roadmap Phases — Phase 28 / 28C / 28D

> Archived from the active roadmap after completion.

---

## Phase 28: mDNS-Powered GUI Discovery + Dashboard Settings

*Completed: 2026-08-19*

Focus: All GUI example clients discover servers via mDNS before connecting.
The server gains a runtime-editable Settings panel in the Web Dashboard.

### 28A — GUI mDNS Server Discovery

All **eight** GUI demos were updated to discover ok-FPGA-Server instances
on the local network via mDNS before requiring manual URL entry.

**Implementation summary:**

| Component | Change |
|-----------|--------|
| `rust-rest-gui-common` | New `mdns_browser` module using `mdns-sd 0.13` |
| `rust-grpc-gui-common` | New `mdns_browser` module using `mdns-sd 0.13` |
| `python_rest_gui_common` | New `mdns_browser.py` module using `zeroconf >=0.131` |
| `python_grpc_gui_common` | New `mdns_browser.py` module using `zeroconf >=0.131` |
| All 8 GUI demos | "Discovered (mDNS)" dropdown added before URL input |
| Server `config.rs` | Added `[server] description` field |
| Server `mdns.rs` | Added `description` key to TXT records |
| Server `rest/server.rs` | Added `description` to `GET /server/status` |

**Architecture:** mDNS browsing logic lives in the 4 common libraries
(not duplicated in each demo). Each demo's UI layer calls the common
library's discovery API and renders a dropdown of discovered servers.
The `DiscoveredServer` struct exposes: instance_name, description, host,
port, rest_port, grpc_port, dashboard_port, api_version, device_count.

### 28B — Web Dashboard Settings Panel

The SPA Web Dashboard gained a new **Settings** page (7th page) exposing
runtime-configurable server settings with TOML write-back persistence.

**Implementation summary:**

| Component | Change |
|-----------|--------|
| Server `config.rs` | `save_config()` for TOML write-back, `expose_settings` in `DashboardConfig` |
| Server `state.rs` | `AppState` gained `config` (RwLock) and `config_path` fields |
| Server `rest/server.rs` | `GET/PUT /server/settings` endpoints |
| Server `management.rs` | `GET/PUT /mgmt/settings` endpoints (management port) |
| Server `rest/errors.rs` | Added `ProblemDetail::forbidden()` helper |
| Dashboard `index.html` | Settings page: description field, enforcement dropdown, warning banner |
| Dashboard `style.css` | Settings form styles (warning, inputs, status indicators) |
| Dashboard `app.js` | fetchSettings, saveSettings, expose_settings visibility guard |
| `api/openapi.yaml` | `GET/PUT /server/settings` + `ServerSettings` schema |

**Key design decisions:**
- Settings are **persistent** — PUT writes updated values back to the TOML file
- `[dashboard] expose_settings = false` hides the Settings nav item and returns 403
- Unauthenticated access warning displayed prominently on the Settings page
- `settings_updated` WebSocket event triggers dashboard auto-refresh

---

## Phase 28C: Settings Lockdown

*Completed: 2026-08-19*

Phase 28B incorrectly exposed `GET/PUT /server/settings` on the client-facing
REST API (port 8088), allowing any network client to change server settings.
Phase 28C removes those endpoints and refines the `expose_settings` behavior.

### Changes

| Component | Change |
|-----------|--------|
| Server `rest/server.rs` | Removed `GET/PUT /server/settings` routes and handlers |
| `api/openapi.yaml` | Removed `/server/settings` path and `ServerSettings` schema |
| Server `management.rs` | Refined: `SettingsBody.endpoint_enforcement` is now `Option<String>`, only included when `expose_settings = true` |
| Server `config.rs` | Updated `expose_settings` docstring — controls enforcement visibility only, not description |
| Dashboard `index.html` | Wrapped warning banner + Access Control panel in `enforcement-section` div |
| Dashboard `app.js` | Settings nav always visible; enforcement section conditionally shown based on API response |

### Key design decisions

- **Description always accessible** — server description can be read/changed from the dashboard regardless of `expose_settings`
- **`expose_settings` = enforcement visibility** — when `false`, the enforcement dropdown is hidden and enforcement changes in PUT are silently ignored
- **No REST settings API** — settings are management-only (`/mgmt/settings` on port 9090), used exclusively by the dashboard
- **`ServerSettings` removed from OpenAPI** — management API is internal and not part of the public API spec

---

## Phase 28D: Dashboard Reliability, Expanded Views & SYZYGY Board Catalog

*Completed: 2026-08-19*

Fixes systemic dashboard auto-refresh gaps (especially for gRPC), adds
expandable detail rows to Devices and Resources pages, introduces a
TOML-based SYZYGY board catalog, and makes UI improvements.

### 1. WebSocket Event Audit & Fixes

| Component | Change |
|-----------|--------|
| REST `resources.rs` | Added `resource_locked` and `resource_unlocked` broadcasts |
| REST `devices.rs` | Added `device_programmed` broadcast to `configure_fpga` |
| gRPC `mod.rs` | Heartbeat interceptor broadcasts `user_connected` for new users |
| gRPC `device_service.rs` | Added broadcasts for `user_disconnected`, `device_programmed`, `resources_cleared`, `resource_acquired`, `resource_released`, `resource_locked`, `resource_unlocked` |
| Dashboard `app.js` | New event types + cross-page refresh (resource events → Users page, user events → Resources page) |

### 2. Overview Page: Description & Hostname

| Component | Change |
|-----------|--------|
| `management.rs` | Added `description` and `hostname` to `StatusResponse` |
| `index.html` | Description row in Server Details; hostname in sidebar |
| `app.js` | Populates description and hostname from status response |

### 3. Font Readability

- `--text-muted` brightened from `#5a6178` → `#8b92a8` for better contrast on dark backgrounds.

### 4. Expandable Device Rows

| Component | Change |
|-----------|--------|
| `management.rs` | `DeviceInfoResponse` enriched with `is_programmed`, `bitfile` (filename, description, programmed_at, programmed_by) |
| `handle_pool.rs` | New `DeviceHandlePool::discover_syzygy_ports()` shared FFI method |
| `management.rs` | New `GET /mgmt/devices/{serial}/syzygy` endpoint |
| `app.js` | Click-to-expand device rows: larger image, device ID, firmware, bitfile info, lazy-loaded SYZYGY ports |
| `style.css` | Detail panel grid, slideDown animation, board-img-large |

### 5. Expandable Resource Rows

| Component | Change |
|-----------|--------|
| `management.rs` | `ResourceInfoResponse` enriched with `description`, `access_mode`, `metadata` |
| `app.js` | Click-to-expand resource rows: full ID, description, access mode, locked_by, metadata table |

### 6. SYZYGY Board Catalog (TOML)

| Component | Change |
|-----------|--------|
| `config.rs` | New `CatalogConfig` with `SyzygyBoardEntry`, `DeviceCatalogEntry`, `BitfileCatalogEntry` |
| `management.rs` | New `GET /mgmt/catalog` endpoint |
| `app.js` | Fetches catalog once at init; `catalogDeviceUrl()`, `catalogSyzygyUrl()`, `catalogBitfile()`, `aboutLink()` helpers; "About ↗" links in device + SYZYGY detail rows |
| `style.css` | `.about-link` styling |

### 7. mDNS: Upgrade mdns-sd to 0.13

- Upgraded `mdns-sd` from `0.11` → `0.13` in `Cargo.toml`. Newer version handles IPv6 AAAA link-local addresses better on Windows, reducing error log spam. No API changes required.

### Key design decisions

- **Catalog is dashboard-only** — catalog data is served only through `GET /mgmt/catalog` on the management port, never inline with device/SYZYGY REST responses
- **SYZYGY discovery extracted to pool** — `discover_syzygy_ports()` lives on `DeviceHandlePool` so both REST and management endpoints share the same FFI logic
- **Lazy-load SYZYGY** — SYZYGY ports are fetched on first expand click, not on page load (avoids blocking device list rendering)

