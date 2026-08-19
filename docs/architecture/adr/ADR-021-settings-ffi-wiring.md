# ADR-021: Device Settings FFI Wiring

**Status**: Implemented  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Depends on**: [ADR-001](ADR-001-ffi-strategy.md) (FFI layer),
[ADR-017](ADR-017-ffi-endpoint-wiring.md) (FFI Endpoint Wiring),
[ADR-018](ADR-018-device-handle-pool.md) (Device Handle Pool)

## Context

Phases 5–6 implemented REST and gRPC API servers with **stub handlers** for
device settings that return `501 Not Implemented` ("Settings require FFI
integration"). The FFI layer (Phase 3) already loads all settings-related
FrontPanel C symbols via `libloading`:

| FFI Field | C Symbol | Purpose |
|-----------|----------|---------|
| `settings_construct` | `okDeviceSettings_Construct` | Allocate settings container |
| `settings_destruct` | `okDeviceSettings_Destruct` | Free settings container |
| `fp_get_device_settings` | `okFrontPanel_GetDeviceSettings` | Populate from device |
| `settings_list` | `okDeviceSettings_List` | Enumerate key names |
| `setting_names_construct` | `okDeviceSettingNames_Construct` | Allocate names container |
| `setting_names_destruct` | `okDeviceSettingNames_Destruct` | Free names container |
| `setting_names_get_count` | `okDeviceSettingNames_GetCount` | Get number of keys |
| `setting_names_get` | `okDeviceSettingNames_Get` | Get key name by index |
| `settings_get_int` | `okDeviceSettings_GetInt` | Read integer setting |
| `settings_set_int` | `okDeviceSettings_SetInt` | Write integer setting |
| `settings_get_string` | `okDeviceSettings_GetString` | Read string setting |
| `settings_set_string` | `okDeviceSettings_SetString` | Write string setting |
| `settings_delete` | `okDeviceSettings_Delete` | Remove a setting |
| `settings_save` | `okDeviceSettings_Save` | Persist changes to device |

Hardware tests (`crates/ok-fpga-ffi/tests/hardware.rs`, test
`enumerate_device_settings`) confirmed that **18 settings keys** are
successfully enumerated and read on the XEM7320-A75T without FPGA
configuration.

## Decision

### 1. Settings Are Device-Level, Not Bitfile-Dependent

Unlike sensors (which may require FPGA configuration on certain boards),
device settings are stored in device flash/EEPROM and are available
**immediately after opening a device handle**. No FPGA programming is
required.

This means the settings endpoints should NOT return 409 ("FPGA not
programmed") — they should work on any connected device.

### 2. FFI Call Sequence

Every settings endpoint follows this lifecycle:

```
1. settings_construct()                  → allocate settings container
2. fp_get_device_settings(fp_handle, s)  → populate from device
3. [enumerate / read / write]            → perform the requested operation
4. [settings_save(s)]                    → persist (write operations only)
5. settings_destruct(s)                  → clean up
```

For list/enumerate operations, an additional names container is needed:

```
3a. setting_names_construct()            → allocate names container
3b. settings_list(s, names)              → populate names from settings
3c. setting_names_get_count(names)       → how many keys
3d. For each key:
    - setting_names_get(names, i)        → get key name (C string ptr)
    - settings_get_int(s, key, &val)     → try as integer
    - settings_get_string(s, key, ...)   → try as string (if int fails)
3e. setting_names_destruct(names)        → clean up names
```

### 3. Request-Path Call Chain

Following the pattern established in ADR-017:

```
HTTP/gRPC Request
      │
      ▼
  Route Handler (axum / tonic)
      │  extracts: serial, [key], [body]
      ▼
  AppState.handle_pool.get(serial)
      │  returns: Arc<Mutex<OpenDevice>>
      ▼
  lock device mutex (tokio::sync::Mutex)
      │  ensures: exclusive FFI access per-device
      ▼
  Construct settings handle + populate via FFI
      │  settings_construct → fp_get_device_settings
      ▼
  Perform operation (list / get / set+save)
      ▼
  Destruct settings handle
      ▼
  Return JSON / protobuf response
```

### 4. Value Type Detection

The FrontPanel settings API stores values as either integers (`UINT32`) or
strings. Since there is no explicit "type" query, the implementation tries
`settings_get_int` first. If it returns `ok_NoError`, the value is an
integer. Otherwise, `settings_get_string` is tried.

This matches the approach used in the hardware tests and the `DeviceSetting`
schema defined in both OpenAPI and proto3:

**OpenAPI** (`DeviceSetting`):
```yaml
properties:
  key: { type: string }
  value_type: { type: string, enum: [string, integer] }
  string_value: { type: string }
  int_value: { type: integer }
```

**Proto3** (`DeviceSetting`):
```protobuf
message DeviceSetting {
  string key = 1;
  oneof value {
    string string_value = 2;
    uint32 int_value = 3;
  }
}
```

### 5. Error Mapping

| Scenario | HTTP Status | gRPC Status |
|----------|-------------|-------------|
| Device not in pool | 404 Not Found | NOT_FOUND |
| DLL not loaded | 501 Not Implemented | UNIMPLEMENTED |
| `fp_get_device_settings` fails | 500 Internal | INTERNAL |
| Key not found (GET single) | 404 Not Found | NOT_FOUND |
| `settings_save` fails (PUT) | 500 Internal | INTERNAL |
| Invalid body (PUT) | 400 Bad Request | INVALID_ARGUMENT |

All REST errors use RFC 7807 Problem Details. All gRPC errors use
`tonic::Status` with a descriptive message.

### 6. Endpoints Affected

**REST** (in `rest/devices.rs`):

| Endpoint | Current | Target |
|----------|---------|--------|
| `GET /devices/{serial}/settings` | 501 stub | List all settings |
| `GET /devices/{serial}/settings/{key}` | 501 stub | Read single setting |
| `PUT /devices/{serial}/settings/{key}` | 501 stub | Write + save setting |

**gRPC** (in `grpc/device_service.rs`):

| RPC | Current | Target |
|-----|---------|--------|
| `GetDeviceSettings` | Returns empty `settings: []` | Enumerate all settings |
| `GetDeviceSetting` | Returns UNIMPLEMENTED | Read single setting |
| `SetDeviceSetting` | Returns UNIMPLEMENTED | Write + save setting |

### 7. Settings Handle Lifetime

Unlike data port handles (which persist in `OpenDevice` after FPGA
configuration), settings handles are **constructed and destructed
per-request**. This is because:

1. Settings are rarely accessed (compared to wire/trigger/pipe operations)
2. Settings may change externally (another client, device firmware)
3. Fresh reads ensure consistency
4. The construct/destruct overhead is negligible (~1ms) compared to USB I/O

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/rest/devices.rs` | Wire 3 settings handlers to FFI |
| `crates/ok-fpga-server/src/grpc/device_service.rs` | Wire 3 settings RPCs to FFI |
| `client-examples/rust-rest-client/src/main.rs` | Move Settings out of "stub" section |
| `docs/architecture/overview.md` | Add ADR-021 to index |
| `docs/development-roadmap.md` | Note settings wiring in Phase 19 |

## Consequences

### Positive

- **Settings endpoints become functional** — the Rust REST client demo will
  show real key/value pairs instead of 501
- **No FPGA programming required** — settings work immediately on any
  connected device
- **Consistent pattern** — follows the same DeviceHandlePool + mutex + FFI
  call chain as all other wired endpoints (ADR-017)
- **Per-request handles** — avoids stale cached data

### Negative

- **Per-request overhead** — constructing and destructing settings handles
  on every request adds ~1ms latency. Acceptable for an infrequently-used
  metadata endpoint.
- **No caching** — if settings are read frequently, a caching layer could
  be added later. For now, fresh reads are preferred.

### Future Work

- **Settings change notification** — broadcast a `DeviceEvent` when a
  setting is modified via the API
- **Settings deletion** — expose `settings_delete` via a
  `DELETE /devices/{serial}/settings/{key}` endpoint
- **Settings caching** — if performance becomes an issue, cache the
  settings snapshot in `OpenDevice` with a TTL
- **Dashboard integration** — show settings in the web dashboard device
  detail view

## References

- [ADR-001 — FFI Strategy](ADR-001-ffi-strategy.md)
- [ADR-017 — FFI Endpoint Wiring Strategy](ADR-017-ffi-endpoint-wiring.md)
- [ADR-018 — Device Handle Pool](ADR-018-device-handle-pool.md)
- [FP6 API Surface Catalog — §9 Device Settings](../fp6-api-surface.md)
- [Hardware test: enumerate_device_settings](../../../crates/ok-fpga-ffi/tests/hardware.rs)
