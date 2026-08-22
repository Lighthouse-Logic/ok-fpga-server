# Phase 34 — Client UX & Device Metadata Improvements

**Status:** ✅ Completed (2026-08-21)
**Triggered by:** Live testing with multiple GUI clients
**Scope:** 2 bugs + 3 features across server + all 8 GUI demos
**Amends:** ADR-004 (Addendum F), ADR-005 (Addendum C), OpenAPI spec, proto spec

---

## Bug 34-A: gRPC Lock Button Toggle

**Root cause:** The proto `Resource` message had no `locked_by` field. gRPC
client libraries hardcoded `locked_by: None` in `resource_from_response()`
so the lock button never toggled.

**Fix:**
- Added `string locked_by = 8` to proto `Resource` message
- Server: populated `locked_by` in all gRPC `Resource` response builders
- Rust gRPC common: read `r.locked_by` instead of hardcoding `None`
- Python gRPC common: read `r.locked_by` from proto response

## Feature 34-B: SYZYGY Peripherals in Device Dropdown

All 8 GUI demos now show SYZYGY peripheral information in the device picker.
Format: `ProductName (Serial) | 0:DAC3283 | 2:ADC3241`

**Implementation:**
- All 4 common libs: `DeviceInfo` gains `syzygy_label` field, fetched during
  `list_devices()` via the existing REST/gRPC SYZYGY endpoints
- `display_label()` method formats the picker label
- All 8 GUI front-ends use `display_label()` in device pickers

## Feature 34-C: Bitfile Path, Filename, and MD5 Hash

**Core:** `BitfileRecord` gains `file_path: Option<String>` and `md5_hash: Option<String>`.

**Server:**
- REST configure: extracts filename from `Content-Disposition`, `X-Filename`,
  or path basename. Computes MD5 hash. Populates `file_path`.
- gRPC configure: same, using new `filename` field in `ConfigureFPGAChunk`.

**Spec changes:**
- OpenAPI: `filename` form field, `file_path`/`md5_hash` in `BitfileInfo`
- Proto: `filename` in `ConfigureFPGAChunk`, `file_path`/`md5_hash` in `BitfileInfo`

**Dashboard:** `web/app.js` displays `file_path` and `md5_hash` in expanded device panel.

## Bug 34-D: "Programmed by: api" Fix (REST)

REST `configure_fpga()` handler now extracts `X-User-Id` from request headers
instead of hardcoding `"api"`. Also fixed filename to use actual path basename
in server-path mode.

## Feature 34-E: Register as Shared or Exclusive

All 8 GUI demos now offer two registration buttons:
- "3. Register (Shared)" — registers with `access_mode: "shared"`
- "Register (Exclusive)" — registers with `access_mode: "exclusive"`

**Implementation:**
- All 4 common libs: `create_resource()` gains `resource_type` parameter
- All 8 GUI front-ends: split single "Register" button into two

## ADR Impact

| ADR | Type | Detail |
|-----|------|--------|
| ADR-004 | Addendum F | OpenAPI spec: `filename` form field, `file_path`/`md5_hash` in `BitfileInfo`, `access_mode` in create resource |
| ADR-005 | Addendum C | Proto: `locked_by` in `Resource`, `filename` in `ConfigureFPGAChunk`, `file_path`/`md5_hash` in `BitfileInfo`, `access_mode` in `CreateResourceRequest` |
