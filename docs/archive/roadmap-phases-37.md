# Archived Roadmap Phases 37

> Archived from `docs/development-roadmap.md` on 2026-08-23.

---

## Phase 37 — API Response Compliance Audit ✅

**Goal:** Audit every REST endpoint's actual response struct against its OpenAPI schema
and fix any discrepancies. Replace all ad-hoc `serde_json::json!` handler responses
with typed `#[derive(Serialize)]` structs.

**Deliverables:**

| # | Task | Notes |
|---|------|-------|
| 1 | Build comparison table | Every endpoint's OpenAPI response schema vs actual Rust return type |
| 2 | Fix ServerStatusBody | Add `server_version`, `openapi_version`, `frontpanel_version`, `platform`, `uptime`, `uptime_seconds`, `connected_devices`, `active_resources`, `services[]` |
| 3 | Fix SensorReading | Rename `minimum`→`min`, `maximum`→`max`; add `sensor_type`, `step`; change values to `f64` |
| 4 | Fix BitfileFullInfo | Add `file_path`, `md5_hash` fields |
| 5 | Fix get_device_info | Return typed `DeviceFullInfo` instead of raw `serde_json::Value` |
| 6 | Fix ResourceView | Add `private` field from core `Resource.private` |
| 7 | Replace 22 ad-hoc JSON handlers in classic.rs | `StatusResponse`, `PipeTransferResponse`, `PipeReadResponse`, `WireOutResponse`, `TriggerOutResponse` |
| 8 | Replace 9 ad-hoc JSON handlers in axi.rs | `StatusResponse`, `DatapathWidthResponse`, `StreamWidthsResponse`, `AXIStreamReadResponse`, `AXIStreamWriteResponse` |
| 9 | Replace ad-hoc JSON in devices.rs | `DeviceSettingView`, `SettingsListBody`, `SensorsListBody`, `SyzygyPortView`, `SyzygyPortsBody`, `DataportResponse`, `DeviceStatusResponse` |
| 10 | Fix release_resource in resources.rs | Return `StatusResponse {success, message}` |
| 11 | Update OpenAPI ServerStatus schema | Rename `version`→`server_version`, add `openapi_version`, `description`, `services[]` |

**Exit Criteria:**
- Every endpoint's response matches its OpenAPI schema ✅
- All 22 ad-hoc `serde_json::json!` handlers replaced with typed structs ✅
- `cargo test` passes (core: 11, server: 4) ✅
- Hardware verified with XEM7320 (server status, device list, device info, dataport) ✅

**Commits:** `b0c8182`
