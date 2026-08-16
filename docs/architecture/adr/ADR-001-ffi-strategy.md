# ADR-001: FFI Strategy — libloading + rust-bindgen

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server must call into the Opal Kelly FrontPanel 6.1.0 native library (`okFrontPanel.dll` on Windows, `libokFrontPanel.so` on Linux) from Rust. The project ships:

- `okFrontPanel.h` — 3,137-line C/C++ header with `extern "C"` function declarations, struct definitions, and enum constants
- `okFrontPanel.dll` — the Windows DLL (2.1 MB)
- `okFrontPanel.lib` — the Windows import library (57 KB)

The Rust FFI layer must:
1. Bind to approximately 120+ C functions across 10+ subsystems
2. Reproduce all C struct layouts (`okTDeviceInfo`, `okTDeviceSensor`, `okTRegisterEntry`, etc.)
3. Reproduce all C enums (`ok_ErrorCode`, `ok_BoardModel`, `okEDeviceSensorType`, etc.)
4. Support cross-platform deployment: **Windows 11**, **Ubuntu 22.04**, **Ubuntu 24.04**
5. Gracefully handle the case where the native library is not present
6. Omit USB 2.0-only functions (`okFrontPanel_WriteI2C`, `okFrontPanel_ReadI2C`)

## Decision

### 1. Use `rust-bindgen` to generate FFI type declarations

The `bindgen` crate will parse `okFrontPanel.h` at build time (via `build.rs`) and auto-generate:
- All `extern "C"` function signatures
- All C struct layouts with correct alignment and field types
- All C enum constants

This eliminates manual transcription errors across 3,137 lines of header and ensures struct layout fidelity.

**Configuration**:
- Pass `--no-layout-tests` or selectively enable layout tests for CI
- Use `blocklist_function` to exclude `okFrontPanel_WriteI2C`, `okFrontPanel_ReadI2C`, and any other USB 2.0-only functions
- Use `blocklist_item` to exclude the C++ class declarations (we bind to the C API only)
- Define `okNO_LUA` to exclude Lua scripting API (not needed for server)
- Define `OPALKELLY_NO_CXX_TYPES` to suppress C++ wrapper classes
- Target the `extern "C"` block only

### 2. Use `libloading` for runtime DLL/SO loading

Instead of static linking via `okFrontPanel.lib`, use `libloading` to load the shared library at runtime:

```
// Pseudocode — not actual implementation
let lib = unsafe { Library::new("okFrontPanel") }?;
let get_api_version: Symbol<fn() -> c_int> = unsafe { lib.get(b"okFrontPanel_GetAPIVersionMajor") }?;
```

**Rationale**:
- Aligns with the `okFrontPanel_TryLoadLib()` pattern already in the C API
- Allows the server to start and report "FrontPanel library not found" gracefully via the web dashboard and logs
- Avoids hard build-time dependency on `okFrontPanel.lib` (which is Windows-only)
- Transparently handles platform differences (`.dll` vs `.so`)
- DLL path is configurable via TOML config or environment variable

### 3. Wrap raw FFI in safe Rust RAII types

All C handles (`okFrontPanel_HANDLE`, `okFPGADataPortClassic_HANDLE`, `okDeviceSettings_HANDLE`, etc.) will be wrapped in Rust structs that:
- Call the corresponding `*_Construct()` in `new()` / `from_raw()`
- Call the corresponding `*_Destruct()` in `Drop`
- Confine all `unsafe` to the FFI crate boundary
- Never expose raw handles to higher layers

### 4. Thread safety model

The FrontPanel C API is **not thread-safe** for concurrent access to the same device handle. The FFI layer will:
- Mark handle wrapper types as `!Sync` (safe default — cannot be shared across threads)
- Document that callers must serialize access (the core domain layer will enforce this via `Mutex`)
- The `FrontPanelManager` callbacks (`OnDeviceAdded`/`OnDeviceRemoved`) are called from a DLL-internal thread; the FFI layer will marshal these to a Rust channel

### 5. Function dispatch table pattern

Since we use `libloading` (runtime binding), we cannot use the `rust-bindgen`-generated `extern "C"` declarations directly (those assume link-time resolution). Instead:

1. `bindgen` generates the **type signatures, structs, and enums** (used for type-safe function pointers)
2. A hand-authored `FrontPanelApi` struct holds `libloading::Symbol` references for each function
3. A `load()` constructor resolves all symbols at startup, failing fast if any required symbol is missing
4. Each method on `FrontPanelApi` delegates to the loaded symbol

This is a well-established Rust pattern for optional runtime DLL binding.

## Excluded Functions

The following C functions are excluded from the FFI layer as they are USB 2.0-only:

| C Function | Reason |
|-----------|--------|
| `okFrontPanel_WriteI2C` | USB 2.0 platforms only |
| `okFrontPanel_ReadI2C` | USB 2.0 platforms only |

The corresponding error codes (`ok_I2CRestrictedAddress`, `ok_I2CBitError`, `ok_I2CNack`, `ok_I2CUnknownStatus`) are retained in the enum for completeness but will never be returned.

## Consequences

### Positive
- **No manual FFI transcription** — bindgen auto-generates ~700 lines of type declarations from the header
- **Graceful degradation** — server starts without the DLL and reports the error
- **Cross-platform** — single codebase works on Windows 11 and Ubuntu 22.04/24.04
- **Safety boundary** — all `unsafe` confined to the FFI crate

### Negative
- **Build dependency on clang** — `bindgen` requires libclang for header parsing (acceptable for a build-time dependency)
- **Runtime symbol resolution overhead** — negligible (one-time at startup), but function calls go through an extra pointer indirection
- **Maintenance** — the function dispatch table must be updated when new functions are added to the API (mitigated by CI tests)

### Risks
- **DLL version mismatch** — the server calls `okFrontPanel_GetAPIVersionMajor/Minor/Micro` at startup and refuses to proceed if the version is below 6.1.0
- **Struct layout drift** — mitigated by `bindgen` layout tests in CI; if Opal Kelly changes struct layouts in a patch release, tests will catch it

## References

- [rust-bindgen](https://github.com/rust-lang/rust-bindgen)
- [libloading crate](https://docs.rs/libloading)
- [okFrontPanel.h](../../FrontPanel-6.1.0/okFrontPanel.h)
- [okFrontPanel_TryLoadLib()](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 774-786
