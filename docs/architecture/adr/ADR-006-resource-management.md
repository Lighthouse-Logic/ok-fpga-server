# ADR-006: Multi-User Resource Management

**Status**: Implemented  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The ok-FPGA-Server must support multiple users accessing one or more connected FPGAs. The project goals establish several key constraints:

1. **Resources are NOT entire FPGAs** — a "resource" is a generic, user-defined capability exposed by a programmed FPGA. One FPGA may expose multiple resources.
2. **Unprogrammed FPGAs cannot have resources** — resources only exist after a bitfile has been successfully loaded.
3. **One FPGA might be usable by more than one user** — the server should not presume exclusivity.
4. **The server provides stub/extensibility code** — developers can customize whether specific HDL functionality can be shared or must be locked.
5. **Resource states**: `Available`, `In Use (Shared)`, `In Use (Locked)`, `Unavailable`, `Initializing`.

## Decision

### Resource Model

A **resource** is a named, addressable unit of functionality exposed by a programmed FPGA. Resources are:
- Defined by the developer/integrator (not auto-discovered from hardware)
- Associated with a specific FPGA serial number
- Only valid while the FPGA is programmed with the expected bitfile
- Individually acquirable by users

```
┌────────────────────────────────────┐
│          FPGA (XEM8320)            │
│  Serial: ABC1234567                │
│  Bitfile: dsp_v2.bit               │
│                                    │
│  ┌───────────┐  ┌──────────────┐   │
│  │ Resource  │  │  Resource    │   │
│  │ "dac_ch1" │  │ "adc_stream" │   │
│  │ Locked    │  │ Shared       │   │
│  │ User: bob │  │ Users:       │   │
│  └───────────┘  │  alice, carol│   │
│                 └──────────────┘   │
└────────────────────────────────────┘
```

### Resource State Model

Resources have five possible states. There is no rigid state machine — transitions are governed by the resource lifecycle and access control logic. The typical flow is:

```
Unavailable ──► Initializing ──► Available ──► In Use (Shared)
     ▲                              │              │
     │                              ▼              │
     │                        In Use (Locked)      │
     │                              │              │
     └──────────────────────────────┴──────────────┘
                   (release / disconnect / reprogram)
```

| State | Description |
|-------|-------------|
| `Unavailable` | Default state. FPGA not programmed, or resource explicitly marked unavailable. |
| `Initializing` | FPGA is being programmed or resource is being set up. Transitional. |
| `Available` | Resource is ready and no user has acquired it. |
| `InUseShared` | One or more users are using the resource concurrently. Other users may also acquire it. |
| `InUseLocked` | Exactly one user has exclusive access. No other users may acquire it until released. |

### Resource Registry

```rust
pub struct Resource {
    /// Unique identifier for this resource
    pub id: ResourceId,
    
    /// Human-readable name (e.g., "dac_channel_1")
    pub name: String,
    
    /// Optional description
    pub description: Option<String>,
    
    /// Serial of the FPGA that provides this resource
    pub device_serial: String,
    
    /// Current state
    pub state: ResourceState,
    
    /// Users currently holding this resource (empty if Available/Unavailable)
    pub holders: Vec<UserId>,
    
    /// Access mode when in use
    pub access_mode: Option<AccessMode>,
}

pub enum ResourceState {
    Unavailable,
    Initializing,
    Available,
    InUseShared,
    InUseLocked,
}

pub enum AccessMode {
    Shared,
    Exclusive,
}
```

### Acquisition Protocol

```
POST /api/v1/resources/{resource_id}/acquire
{
  "user_id": "alice",
  "mode": "shared"    // or "exclusive"
}
```

Decision logic (extensible via trait):

```rust
pub trait ResourceAccessPolicy: Send + Sync {
    /// Determine if the given user can acquire the resource in the given mode.
    fn can_acquire(
        &self,
        resource: &Resource,
        user: &UserId,
        requested_mode: AccessMode,
    ) -> Result<(), AcquisitionDenied>;
    
    /// Called when a resource's FPGA is disconnected or reprogrammed.
    fn on_resource_invalidated(
        &self,
        resource: &Resource,
        holders: &[UserId],
    );
}
```

**Default policy** (stub):
- If `Available` → allow acquire in either mode
- If `InUseShared` and request is `Shared` → allow (add user to holders)
- If `InUseShared` and request is `Exclusive` → deny
- If `InUseLocked` → deny (resource is exclusively held)
- If `Unavailable` or `Initializing` → deny

This policy is a **stub** that developers can replace with HDL-specific logic.

### FPGA ↔ Resource Lifecycle

1. **FPGA connected but not programmed**: No resources. Device appears in device list but resource list is empty for this device.
2. **FPGA programmed with bitfile**: Developer-defined resources transition from `Unavailable` → `Initializing` → `Available`.
3. **User acquires resource**: `Available` → `InUseShared` or `InUseLocked`.
4. **User releases resource**: If no holders remain, → `Available`.
5. **FPGA disconnected**: All resources for this FPGA → `Unavailable`. All holders are notified (SSE event / gRPC stream).
6. **FPGA reprogrammed with different bitfile**: All existing resources → `Unavailable` (old bitfile's resources are invalid). New resources may be created for the new bitfile.
7. **FPGA reprogrammed with same bitfile**: Resources may be re-initialized rather than destroyed (policy-dependent).

### User Tracking

Users are identified by a `UserId` (string). For the initial version (no auth), users are self-identified via a header or parameter. The auth stubs (ADR-008) will provide authenticated user identity.

Each user's current state is tracked:
```rust
pub struct UserSession {
    pub user_id: UserId,
    pub connected_at: DateTime<Utc>,
    pub held_resources: Vec<ResourceId>,
}
```

### Bitfile Tracking

For every FPGA, the server tracks the current bitfile:

```rust
pub struct BitfileRecord {
    pub filename: String,
    pub description: Option<String>,
    pub programmed_at: DateTime<Utc>,
    pub programmed_by: UserId,
}
```

An FPGA can only be reprogrammed if it is not `InUse` (no resources currently held by any user), or if the user has appropriate permissions (extensible via policy).

## Consequences

### Positive
- **Flexible resource model** — resources are user-defined, not tied to hardware topology
- **Extensible access control** — the `ResourceAccessPolicy` trait enables HDL-specific sharing logic
- **Clean lifecycle** — resources are properly invalidated on disconnect/reprogram
- **Multi-user safe** — the state model handles concurrent acquisition correctly

### Negative
- **Resource definition is manual** — developers must define resources; they are not auto-discovered from hardware
- **State complexity** — the resource lifecycle interacts with device lifecycle, creating many edge cases
- **No persistent storage (v1)** — resource definitions are in-memory; a restart resets all state

### Future Considerations
- **Persistent resource definitions** — load from TOML config or database
- **Resource auto-discovery** — if bitfiles include metadata describing their resources
- **Fine-grained locking** — per-endpoint or per-register locking rather than whole-resource

## Addendum A: Client-Side Resource Registration (2026-08-17)

See [ADR-020](ADR-020-client-side-resource-registration.md) for full design.

The original ADR stated resources are "defined by the developer/integrator" but did
not specify the creation mechanism. ADR-020 resolves this:

- **The client creates resources** via `POST /api/v1/resources` after programming a
  bitfile. The server remains agnostic to bitfile capabilities.
- **The client deletes resources** via `DELETE /api/v1/resources/{resource_id}` when
  cleaning up before reprogramming or disconnecting.
- **Server auto-deletes**: On FPGA disconnect, resources are **deleted** (the
  device state is unknown after reconnection — it may have lost power or been
  reprogrammed by another tool).
- **Access mode at registration**: The `access_mode` field is set at registration
  time as a hardware design constraint, replacing the original model where mode was
  chosen at acquisition time.
- **Lock/unlock semantics**: Shared resources can be temporarily locked by any
  holder for uninterrupted access, then unlocked to resume shared access.
- **Heartbeat/liveness**: Users must maintain heartbeats (implicit via any API call,
  or explicit via `POST /heartbeat`) to retain resource holds. Auto-release occurs
  after 60 seconds of inactivity.
- **Interface declarations**: Resources declare Classic, AXI-Lite, AXI-Full, and/or
  AXI-Stream support, validated against the device at registration time.
- **Classic endpoint declarations**: Resources declare which EPs they use and
  whether each is shared or exclusive. Exclusive EP overlaps are rejected.
- **Address range declarations**: Resources declare memory-mapped address ranges
  with per-range sharing semantics. Exclusive overlaps are rejected.

## References

- [Project Goals — Resource Model](../../ok-FPGA-Server_Project-goals.txt) — lines 3-4
- [ADR-020 — Client-Side Resource Registration](ADR-020-client-side-resource-registration.md)

