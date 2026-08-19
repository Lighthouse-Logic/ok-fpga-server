# Archived Roadmap Phases — Phase 28

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
