# ADR-015: HTTP Method Semantics — POST vs PUT

**Status**: Accepted  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Supersedes**: Portions of ADR-004 §URL Hierarchy (method assignments)

## Context

Phase 16 testing revealed a mismatch between the OpenAPI spec (which used `POST`
for `SetWireInValue`) and the server implementation (which correctly used `PUT`).
This prompted a comprehensive audit of every REST endpoint's HTTP method against
RFC 9110 semantics.

The core distinction from RFC 9110:

| Feature | `PUT` | `POST` |
| --- | --- | --- |
| **Idempotency** | **Yes** — repeating N times produces the same server state as 1 time | **No** — repeating N times can trigger side-effects N times |
| **URI Target** | Points to the **exact specific resource** (e.g., `/devices/ABC/settings/gain`) | Points to the **collection or processor** (e.g., `/devices` or `/commands`) |
| **Core Semantic** | **Set/Replace/Upsert**: "Make the resource at this URI look like this payload." | **Process/Append/Action**: "Handle this payload according to your internal logic." |
| **Network Retry** | Safe for clients/proxies to automatically retry on timeout | Unsafe to retry automatically without an idempotency key |

### Key Reasons to Choose One Over the Other

1. **Idempotency and Automated Retries**
   - `PUT` to set register `0x00` to `0xDEADBEEF` is idempotent — replaying after a
     network glitch produces the same hardware state.
   - `POST` to fire a trigger is non-idempotent — replaying sends two pulses instead
     of one.

2. **Resource Creation: Client-Specified vs. Server-Assigned URI**
   - Use `POST` when the server determines the identifier (e.g., `POST /sessions`
     returns `201 Created` with `Location: /sessions/sess_98234`).
   - Use `PUT` when the client specifies the exact target URI during creation or
     replacement (e.g., `PUT /settings/network_mode`).

3. **State Assignment vs. Procedural Action**
   - `PUT` for state definitions: register writes, wire value assignments, config
     updates.
   - `POST` for procedural commands: trigger pulses, bitfile programming, calibration
     triggers, DMA transfers.

## Decision

### Complete HTTP Method Audit

Every REST endpoint was evaluated. Changes from the original ADR-004 assignments
are marked with **CHANGED**.

#### Server Endpoints

| Path | Method | Idempotent? | Rationale |
| --- | --- | --- | --- |
| `/server/status` | GET | Yes | Read-only query |
| `/server/restart` | POST | No | Procedural action — restarts the process |
| `/server/logs` | GET | Yes | SSE stream (read-only) |
| `/server/services` | GET | Yes | Read-only query |
| `/server/services/{name}/start` | POST | No | Procedural — starts a service |
| `/server/services/{name}/stop` | POST | No | Procedural — stops a service |
| `/server/services/{name}/restart` | POST | No | Procedural — restarts a service |

#### Device Endpoints

| Path | Method | Idempotent? | Rationale |
| --- | --- | --- | --- |
| `/devices` | GET | Yes | Read-only query |
| `/devices/{serial}` | GET | Yes | Read-only query |
| `/devices/{serial}/sensors` | GET | Yes | Read-only query |
| `/devices/{serial}/settings` | GET | Yes | Read-only query |
| `/devices/{serial}/settings/{key}` | GET | Yes | Read-only query |
| `/devices/{serial}/settings/{key}` | PUT | Yes | State assignment — set a named config value |
| `/devices/{serial}/syzygy` | GET | Yes | Read-only query |
| `/devices/{serial}/configure` | POST | No | Procedural — uploads and programs a bitfile |
| `/devices/{serial}/configure` | DELETE | Yes | Idempotent — clears FPGA config |
| `/devices/{serial}/bitfile` | GET | Yes | Read-only query |
| `/devices/{serial}/dataport` | GET | Yes | Read-only query |

#### Classic Data Port Endpoints

| Path | Method | Idempotent? | Rationale |
| --- | --- | --- | --- |
| `/classic/wires/in` | **PUT** | Yes | **CHANGED from POST.** State assignment — setting endpoint 0x00 to value 0xFF with mask 0xFF always produces the same wire state. Safe to auto-retry. |
| `/classic/wires/in/update` | POST | No | Procedural — transfers pending wire values to FPGA |
| `/classic/wires/out/update` | POST | No | Procedural — latches wire values from FPGA |
| `/classic/wires/out/{address}` | GET | Yes | Read-only query |
| `/classic/triggers/in` | POST | No | **One-shot event** — each call fires a distinct pulse. NOT safe to auto-retry. |
| `/classic/triggers/out/update` | POST | No | Procedural — latches trigger state from FPGA |
| `/classic/triggers/out/{address}` | GET | Yes | Read-only query |
| `/classic/pipes/in` | POST | No | Sequential data transfer — each call sends new data |
| `/classic/pipes/out` | POST | No | Sequential data transfer — each call reads new data |
| `/classic/pipes/block-in` | POST | No | Bulk data transfer |
| `/classic/pipes/block-out` | GET | Yes | Bulk read (no side effects) |
| `/classic/registers/read` | POST | No | Query with request body (address). POST for body-carrying read. |
| `/classic/registers/write` | **PUT** | Yes | **CHANGED from POST.** State assignment — writing address 0x04 = 0xDEADBEEF is idempotent. |
| `/classic/reset` | POST | No | Procedural — resets the FPGA |

#### AXI Data Port Endpoints

| Path | Method | Idempotent? | Rationale |
| --- | --- | --- | --- |
| `/axi/lite/read` | POST | No | Query with body (address). POST for body-carrying read. |
| `/axi/lite/write` | **PUT** | Yes | **CHANGED from POST.** State assignment — writing address + value to a memory-mapped register is idempotent. |
| `/axi/lite/reset` | POST | No | Procedural — resets AXI Lite interface |
| `/axi/full/read` | POST | No | DMA data transfer |
| `/axi/full/write` | POST | No | DMA data transfer — bulk writes are sequential, not idempotent |
| `/axi/full/datapath-width` | GET | Yes | Read-only query |
| `/axi/full/reset` | POST | No | Procedural |
| `/axi/stream/read` | POST | No | Streaming data transfer |
| `/axi/stream/write` | POST | No | Streaming data transfer |
| `/axi/stream/datapath-width` | GET | Yes | Read-only query |
| `/axi/stream/reset` | POST | No | Procedural |

#### Resource Endpoints

| Path | Method | Idempotent? | Rationale |
| --- | --- | --- | --- |
| `/resources` | GET | Yes | Read-only query |
| `/resources` | POST | No | **NEW.** Creates a resource — server assigns state. See ADR-016. |
| `/resources/{resource_id}` | GET | Yes | Read-only query |
| `/resources/{resource_id}` | DELETE | Yes | **NEW.** Removes a resource definition. Idempotent. |
| `/resources/{resource_id}/acquire` | POST | No | Creates a lock/hold — server-side state change |
| `/resources/{resource_id}/release` | POST | No | Releases a hold — procedural action |

#### User & Event Endpoints

| Path | Method | Idempotent? | Rationale |
| --- | --- | --- | --- |
| `/users` | GET | Yes | Read-only query |
| `/users/{user_id}` | GET | Yes | Read-only query |
| `/events/stream` | GET | Yes | SSE stream (read-only) |

### Summary of Changes from ADR-004

| Endpoint | Old Method | New Method | Reason |
| --- | --- | --- | --- |
| `/classic/wires/in` | POST | **PUT** | Idempotent state assignment |
| `/classic/registers/write` | POST | **PUT** | Idempotent state assignment |
| `/axi/lite/write` | POST | **PUT** | Idempotent state assignment |
| `/resources` | (none) | **POST** | Client-side resource creation (ADR-016) |
| `/resources/{resource_id}` | (none) | **DELETE** | Resource cleanup |

## Consequences

### Positive
- **Network safety** — PUT endpoints can be safely retried by proxies and clients
  on timeout without risk of duplicate side effects
- **Semantic clarity** — each endpoint's method now communicates its behavioral
  contract (idempotent state-set vs. procedural action)
- **RFC 9110 compliance** — methods align with HTTP semantics standard

### Negative
- **Breaking change** — clients using POST for wire-in, register-write, or AXI-lite-write
  will receive 405 Method Not Allowed. All 4 example clients updated in the same commit.

## Files Changed

- `api/openapi.yaml` — method corrections
- `crates/ok-fpga-server/src/rest/classic.rs` — `registers/write` route
- `crates/ok-fpga-server/src/rest/axi.rs` — `axi/lite/write` route
- All 4 example clients — method updates
- `proto/okfpga/v1/okfpga.proto` — no proto changes (gRPC uses RPCs, not HTTP methods)

## References

- [RFC 9110 — HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110#section-9.3)
- [ADR-004 — REST API Design](ADR-004-rest-api-design.md)
