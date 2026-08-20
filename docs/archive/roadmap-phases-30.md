# Roadmap Archive — Phase 30

## Phase 30 — Read-Only Resource Acquisition & Private Data (ADR-026)

**Status:** ✅ Completed 2026-08-20  
**ADR:** [ADR-026 — Read-Only Acquisition & Private Data](../architecture/adr/ADR-026-readonly-acquisition.md)  
**Commits:** `0a1e2c6` (docs), `cd0ea2f` (code + specs + GUI demos)

### Focus

Introduce `AccessMode::ReadOnly` for observe-without-modify access. Add "Private"
marking at three levels (Resource, Endpoint, Address Range) that restricts what
read-only holders can see. Enforce write denial and Private data hiding for
read-only users across all Classic, AXI, and okRegisterBridge operations. Update
all 8 GUI demos with an "Acquire Read-Only" button.

### Context

Previously, the only acquisition modes were `Shared` (multiple holders, read+write)
and `Exclusive` (sole holder, read+write). There was no way for a user to observe
a resource's state without having write access — a common need in monitoring and
dashboard scenarios where one operator is actively controlling a device while
colleagues need to observe the output.

### Key Design Decisions

1. **ReadOnly holders cannot call `UpdateWireOuts` / `UpdateTriggerOuts`** — these
   disturb the FPGA system state. ReadOnly users see cached values only.

2. **ReadOnly holders CAN acquire even an Exclusive resource** (they don't
   interfere with the exclusive holder's access) — unless the resource itself is
   marked Private.

3. **`private: bool` at three levels:**
   - **Resource**: Denies ReadOnly acquisition entirely
   - **Endpoint**: Denies ReadOnly access to that specific endpoint
   - **AddressRange**: Denies ReadOnly reads of that specific address range

4. **ReadOnly holders cannot Lock** — only Shared holders can acquire a lock.

5. **Private applies uniformly** to Classic, AXI, and okRegisterBridge operations.

### Implementation

#### Core (`ok-fpga-core/src/resource.rs`)
- Added `AccessMode::ReadOnly` variant + Display impl
- Added `HolderEntry` struct replacing `Vec<UserId>` with `Vec<HolderEntry>` to
  track per-holder access mode
- Added `private: bool` to `Resource`, `EndpointEntry`, and `AddressRange`
- Added `EndpointType::is_input()` helper
- Added `AcquisitionDenied::PrivateResource`, `AccessDenied::ReadOnlyWriteAttempt`,
  and `AccessDenied::PrivateDataDenied` error variants
- Updated `can_acquire()`, `check_endpoint_access()`, `check_address_access()`,
  and `lock()` with ReadOnly + Private enforcement
- Added 14 new unit tests covering all ReadOnly and Private scenarios
- Total: 158 core tests passing

#### Server (`ok-fpga-server`)
- REST handler: accepts `"read_only"` mode string in `parse_access_mode()`
- gRPC handler: accepts `ACCESS_MODE_READ_ONLY = 2`
- Updated `management.rs`, `resources.rs`, `sweep.rs`, `device_service.rs` for
  `HolderEntry` serialization
- Total: 45 server tests passing

#### API Specifications
- **OpenAPI** (`api/openapi.yaml`): Added `read_only` to mode enum, `private` and
  `access_mode` fields to `Resource` schema, updated descriptions
- **Proto3** (`proto/okfpga/v1/okfpga.proto`): Added `ACCESS_MODE_READ_ONLY = 2`,
  `bool private = 7` to `Resource` message, updated descriptions

#### Common Libraries (4)
- Rust REST, Rust gRPC, Python REST, Python gRPC — all gained
  `acquire_resource_readonly()` method

#### GUI Demos (8)
- All 8 demos gained an "Acquire R/O" button alongside existing "Acquire (Shared)"
- Rust demos: ReadOnly acquisition forces lockout (write controls disabled) with
  "📖 Read-Only Mode" banner
- Python demos: ReadOnly acquisition shown with banner where supported
- All 4 Rust GUI demos compile clean; 33 iced unit tests passing

### Checklist

* [x] ADR-026 — new ADR documenting ReadOnly semantics, Private data, enforcement
* [x] `AccessMode::ReadOnly` and `HolderEntry` tracking
* [x] `private` field on `Resource`, `EndpointEntry`, and `AddressRange`
* [x] `is_input()` helper on `EndpointType`
* [x] `ReadOnlyWriteAttempt`, `PrivateDataDenied`, `PrivateResource` error variants
* [x] `can_acquire()` policy — ReadOnly rules, Private resource check
* [x] `check_endpoint_access()` / `check_address_access()` — ReadOnly + Private
* [x] OpenAPI spec — add `read_only` to mode enum, `private` fields
* [x] Proto3 spec — add `ACCESS_MODE_READ_ONLY = 2`, `private` fields
* [x] REST/gRPC acquire handlers — accept `read_only` mode
* [x] GUI demos (all 8) — "Acquire Read-Only" button
* [x] Unit tests — 14+ new tests for ReadOnly and Private enforcement
