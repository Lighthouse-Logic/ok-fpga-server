# Roadmap Archive: Phases 26–27

> **Status**: Archived — these phases are complete.
> This file is a historical record. For current priorities, see
> [development-roadmap.md](../development-roadmap.md).

---
## Phase 26: Endpoint Access Enforcement (ADR-024)

**Status**: `[x]` Completed (2026-08-19, `ca032f7`, `f86ed71`, `478e5f5`)
**Depends on**: Phase 20 (Resource Registration), Phase 21 (User Session Tracking)
**ADR reference**: [ADR-024](../architecture/adr/ADR-024-endpoint-access-enforcement.md)

### Overview

Implemented runtime endpoint access enforcement for ok-FPGA-Server, closing
the gap identified in ADR-020 §7 where data port operations were unrestricted.
The system supports three enforcement modes (`off`, `warn`, `enforce`) with a
hybrid architecture: lightweight middleware for route-level gating + handler-level
checks for endpoint-specific enforcement.

### Architecture

Access control and heartbeat are **independent, parallel middleware layers** —
they do not serialize through each other.

```
Request ──┬──→ Access MW (identity + device-level gate) ──→ Handler (endpoint check) ──→ Response
          │
          └──→ Heartbeat MW (async touch, independent liveness concern)
```

#### Three-Tier Route Classification (REST)

| Tier | Routes | Enforcement |
|------|--------|-------------|
| **Tier 1** — Management | `/server/*`, `/devices` (list), `/resources/*`, `/users/*`, `/events/*` | Identity only (X-User-Id present) |
| **Tier 2** — Device-level | `/devices/{serial}`, `/devices/{serial}/configure` | Identity only |
| **Tier 3** — Data port | `/devices/{serial}/classic/**`, `/devices/{serial}/axi/**` | Identity + device-level gate + endpoint-level check |

#### gRPC Enforcement

gRPC uses **in-handler enforcement** (not an interceptor) because metadata must
be extracted before `request.into_inner()` consumes the tonic `Request`. Each
RPC extracts `x-user-id` from metadata, then calls the same core registry
gatekeepers as REST.

### Changes

| Component | File | Description |
|-----------|------|-------------|
| Config | `state.rs` | `EndpointEnforcement` enum (`Off`, `Warn`, `Enforce`) |
| Config | `config.rs` | `AccessControlConfig` with `endpoint_enforcement` TOML key |
| Core | `resource.rs` | `check_device_access()`, `check_endpoint_access()`, `check_address_access()` |
| REST MW | `rest/access_middleware.rs` | Three-tier route classification middleware |
| REST | `rest/mod.rs` | Wired access MW as independent layer |
| REST | `rest/classic.rs` | `enforce_endpoint()`, `enforce_address()` helpers + all handlers updated |
| gRPC | `grpc/classic_service.rs` | `check_access()`, `check_address()` + all 19 RPCs updated |
| gRPC | `grpc/axi_service.rs` | Documentation: enforcement required when FFI is implemented |
| Docs | `ADR-024` | Status → Implemented |
| Docs | `overview.md` | ADR-024 status updated |
| Docs | `development-roadmap.md` | Phase 26 completed, Phase 27 promoted |

### Verification

| Test Suite | Tests | Status |
|-----------|-------|--------|
| Core `check_*` methods | 15 tests | ✅ All pass |
| Access middleware | 11 tests | ✅ All pass |
| Full suite (`--features full`) | 236 tests | ✅ All pass (0 failed, 17 ignored) |

### Configuration

```toml
# okfpga-server.toml
[access_control]
# "off"     — no enforcement (default, backward compatible)
# "warn"    — log violations but allow operations (User ID required)
# "enforce" — reject unauthorized operations with 403 / PERMISSION_DENIED (User ID required)
endpoint_enforcement = "off"
```

---
## Phase 27: OpenAPI Spec Gaps

**Status**: `[x]` Completed (2026-08-19, `9c2f439`)
**Depends on**: Phase 1 (OpenAPI Spec), Phase 20 (Resource Registration)
**ADR reference**: [ADR-004](../architecture/adr/ADR-004-rest-api-design.md),
[ADR-020](../architecture/adr/ADR-020-client-side-resource-registration.md)

### Overview

Addressed spec gaps where REST endpoints existed in the server implementation
but were missing from the OpenAPI specification. Also fixed Python gRPC demo
unit tests that were blocked by a proto stub import issue.

### Changes

#### OpenAPI Spec Additions (`api/openapi.yaml`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/heartbeat` | POST | User liveness ping (ADR-020 §5). Echoes `server_time` + `user_id`. |
| `/resources/{id}/lock` | POST | Lock a shared resource for exclusive use. Returns `409` if not lockable. |
| `/resources/{id}/unlock` | POST | Unlock a previously locked resource. Returns `409` if not locked by user. |

All three endpoints already had working REST implementations — this phase
documented them in the OpenAPI spec as the single source of truth.

#### Python gRPC Demo Fix (`python_grpc_gui_common/counters_grpc_client.py`)

The `grpc_tools.protoc` code generator produces stubs with **absolute internal
imports** (e.g., `import okfpga_pb2`). This requires the `gen/` directory to be
on `sys.path` before any import attempt. The previous code only added `gen/` to
`sys.path` inside the `except ImportError` fallback, which was too late.

**Fix**: Add `gen/` to `sys.path` unconditionally at module load time, then
try relative import (`.gen`) first (package context), falling back to absolute
import (`gen`) for direct execution.

### Verification

| Test Suite | Tests | Status |
|-----------|-------|--------|
| Rust full suite (`--features full`) | 236 tests | ✅ All pass |
| Python gRPC DearPyGui demo | 12 tests | ✅ All pass |
| Python gRPC common library | 15 tests | ✅ All pass |
| YAML validation | — | ✅ Valid |
