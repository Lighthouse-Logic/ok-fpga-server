# Roadmap Archive — Phase 32

## Phase 32 — Bug Fixes: Users Page, Settings Persistence, gRPC User Identity

**Status:** ✅ Completed 2026-08-21  
**Triggered by:** Live testing with two simultaneous GUI clients (REST + gRPC)

### Focus

Three bugs discovered during hardware testing with two clients (Icy Rester via
REST, Icy Garpac via gRPC) connected simultaneously.

### Bug 1: Dashboard Users Page Only Shows First User

**Root Cause:** The gRPC heartbeat interceptor in `grpc/mod.rs` was only attached
to `DeviceServiceServer`. All other gRPC services (`ClassicDataPortServiceServer`,
`AxiDataPortServiceServer`, `ServerServiceServer`) passed requests without
touching the user session. gRPC users making Classic data port calls didn't get
their session refreshed, and the sweep task eventually expired them.

**Fix:** Attached the heartbeat interceptor to all four gRPC services using
`with_interceptor()`.

### Bug 2: Settings Enforcement Level Doesn't Take Effect at Runtime

**Root Cause:** `PUT /mgmt/settings` correctly updated the config string and
persisted to TOML, but the runtime `AppState.enforcement` enum (which middleware
and handlers actually check) was a plain `EndpointEnforcement` value set at
startup and never updated.

**Fix:**
- Changed `enforcement` field to `RwLock<EndpointEnforcement>` in `state.rs`
- Added `enforcement()` and `set_enforcement()` accessor methods
- Implemented manual `Clone` for `AppState` (since `RwLock` doesn't derive `Clone`)
- Added `set_enforcement()` call in the PUT settings handler after updating the
  config string
- Updated all 3 consumer files (`access_middleware.rs`, `rest/classic.rs`,
  `grpc/classic_service.rs`) to use the new `enforcement()` method

### Bug 3: gRPC Clients Get PermissionDenied When Enforcement Active

**Root Cause:** Both Rust and Python gRPC common libraries stored `user_id` but
never attached it as `x-user-id` gRPC metadata on outgoing requests. The server's
enforcement interceptor checks metadata headers, not protobuf body fields.

**Fix (Rust):** Added `req<T>()` helper method on `CountersGrpcClient` that wraps
any protobuf message in a `tonic::Request` with `x-user-id` metadata pre-set.
Applied to all 22 gRPC calls (21 unary + 1 streaming).

**Fix (Python):** Added `_metadata` tuple storing `("x-user-id", user_id)` on
init. Passed `metadata=self._metadata` to all gRPC stub calls. Also fixed a
pre-existing bug where `disconnect()` referenced `self.user_mgmt_stub`
(non-existent) instead of `self.device_stub`.

### Files Changed

**Server:**
- `crates/ok-fpga-server/src/grpc/mod.rs` — heartbeat interceptor on all services
- `crates/ok-fpga-server/src/state.rs` — `RwLock<EndpointEnforcement>`, accessors, manual Clone
- `crates/ok-fpga-server/src/management.rs` — `set_enforcement()` in PUT handler
- `crates/ok-fpga-server/src/rest/access_middleware.rs` — use `enforcement()` method
- `crates/ok-fpga-server/src/rest/classic.rs` — use `enforcement()` method
- `crates/ok-fpga-server/src/grpc/classic_service.rs` — use `enforcement()` method

**Clients:**
- `client-examples/rust-grpc-gui-common/src/lib.rs` — `req()` helper + all calls wrapped
- `client-examples/python_grpc_gui_common/counters_grpc_client.py` — `_metadata` on all calls

### Verification

| Test Suite | Result |
|------------|--------|
| `cargo build --features full` | ✅ Compiled (pre-existing warnings only) |
| `cargo test -p ok-fpga-server --features full` | ✅ 60 tests passed |
| `cargo test --lib` (rust-grpc-gui-common) | ✅ 16 tests passed |
