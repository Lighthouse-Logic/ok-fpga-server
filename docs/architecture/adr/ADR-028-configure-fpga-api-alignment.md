# ADR-028: Configure FPGA API Alignment

**Status:** Implemented ✅
**Date:** 2026-08-23
**Deciders:** Architecture Team
**Roadmap Phase:** 36
**Commit:** `fc0d77d`

---

## Context

The `POST /devices/{serial}/configure` endpoint supports two distinct use cases:

1. **Server-side path** — the bitfile already exists on the server's filesystem;
   the client sends only a path string.
2. **Client-side upload** — the client sends the raw bitfile bytes to the server,
   which programs the FPGA from memory.

The current implementation has several discrepancies with the OpenAPI spec and the
gRPC proto definition, discovered during the first LabVIEW integration attempt:

- The OpenAPI spec defines **only** `multipart/form-data` for
  `/devices/{serial}/configure`. There is no `application/json` content type
  defined, even though the server has a JSON-path code path.
- The server handler does **not parse true multipart** — it sniffs `Content-Type`
  and treats the entire body as raw bytes if it contains `"multipart"` or
  `"octet-stream"`, which would corrupt the bitfile with boundary markers if a
  client sent actual `multipart/form-data`.
- The server's `ConfigureResponse` returns `{status, serial, message}`, but the
  spec says the response should be `BitfileInfo`.
- The Rust GUI clients send `application/json` with `{"bitfile_path": "..."}` for
  server-path mode and `application/octet-stream` for client-upload mode — neither
  matches the OpenAPI spec.
- The Python LabVIEW client sends `application/x-www-form-urlencoded` with
  `{filename: path}` — wrong encoding, wrong field name, causes HTTP 400.
- The gRPC proto already has the correct two-RPC split: `ConfigureFPGA` (path) and
  `ConfigureFPGAFromMemory` (streaming binary). The REST API should parallel this.

### Problem Statement

How should the REST API be structured to support both server-side path programming
and client-side binary upload, while aligning with the OpenAPI spec, the gRPC proto,
and the needs of all client libraries (Rust GUI, Python LabVIEW)?

---

## Decision

### Split into two REST endpoints

| REST Endpoint | gRPC Equivalent | Content-Type | Purpose |
|---|---|---|---|
| `POST /devices/{serial}/configure` | `ConfigureFPGAFromMemory` | `multipart/form-data` | Client uploads bitfile binary |
| `POST /devices/{serial}/configure_from_server_path` | `ConfigureFPGA` | `application/json` | Server reads bitfile from its own disk |

### Endpoint 1: `/devices/{serial}/configure` — Client Upload (multipart)

Keeps the existing endpoint for client-side binary upload but **fixes the server to
parse true `multipart/form-data`** using `axum::extract::Multipart` (or `multer`).

```yaml
requestBody:
  content:
    multipart/form-data:
      schema:
        type: object
        required: [bitfile]
        properties:
          bitfile:
            type: string
            format: binary
            description: The .bit or .rbf bitfile to program
          filename:
            type: string
            description: Original filename of the bitfile
          description:
            type: string
            description: Optional description of the bitfile
          user_id:
            type: string
            description: User performing the programming (should match X-User-Id header)
```

**Why `multipart/form-data` instead of `application/octet-stream`:**

- `multipart/form-data` is the HTTP standard for mixed binary+text uploads (RFC 7578).
- It allows metadata fields (filename, description) to travel alongside the binary
  in a single request, fully described by the OpenAPI schema.
- `application/octet-stream` requires non-standard custom headers (`X-Filename`,
  `X-Description`) that are invisible in OpenAPI `requestBody` schemas and not
  self-documenting.
- All major HTTP client libraries (`requests`, `reqwest`, `FormData`) have native
  multipart support.

### Endpoint 2: `/devices/{serial}/configure_from_server_path` — Server Path (JSON)

New endpoint for server-side path programming.

```yaml
requestBody:
  content:
    application/json:
      schema:
        type: object
        required: [bitfile_path]
        properties:
          bitfile_path:
            type: string
            description: Path to the bitfile on the server's filesystem
          description:
            type: string
            description: Optional description of the bitfile
          user_id:
            type: string
            description: User performing the programming (should match X-User-Id header)
```

### Response: Both endpoints return `BitfileInfo`

Both endpoints return the `BitfileInfo` schema on success (HTTP 200), matching the
existing OpenAPI definition:

```yaml
BitfileInfo:
  filename:       string
  description:    string
  programmed_at:  string (date-time)
  programmed_by:  string
  file_path:      string | null    # null for client uploads
  md5_hash:       string | null    # hex-encoded
```

The current `ConfigureResponse {status, serial, message}` struct must be replaced.

### User identification

Both endpoints include a `user_id` field in their request schema. This makes the
API self-documenting — OpenAPI code generators and Swagger UI consumers can
discover the field without reading external documentation.

The `X-User-Id` header must also be set to the same value. The header is used by
the server's access control middleware (which inspects headers on all requests),
while the body field provides the value for the `BitfileRecord.programmed_by`
audit trail.

A future cross-cutting task will audit all 8 endpoints that define `user_id` in
their request bodies and align the server implementation to read from both the
body field and the header consistently (see Deferred Items below).

### gRPC proto

The gRPC proto already has the correct split:

- `ConfigureFPGA(ConfigureFPGARequest)` — `{serial, bitfile_path, user_id}`
- `ConfigureFPGAFromMemory(stream ConfigureFPGAChunk)` — `{serial, data, is_last, user_id, filename}`

Both return `ConfigureFPGAResponse { BitfileInfo bitfile }`.

No proto changes are needed for this ADR.

---

## Consequences

### Positive

- REST API structure parallels the gRPC service definition.
- Each endpoint has a single, clear content type — no content-type sniffing.
- The server will correctly parse multipart form data instead of treating boundary
  markers as bitfile bytes.
- The `BitfileInfo` response provides useful metadata to clients without a second GET.
- The `description` field is finally usable (currently hard-coded to `None`).

### Negative

- Breaking change: existing Rust GUI clients that call `/configure` with
  `application/json` must switch to `/configure_from_server_path`. Mitigation:
  both clients are in-tree and can be updated in the same commit.
- The `application/octet-stream` shortcut used by the Rust GUI
  `configure_fpga_from_file()` must be rewritten to use true multipart.

---

## Deferred Items

### User Identification Consistency (`user_id` vs `X-User-Id`)

The OpenAPI spec defines `user_id` as a request body field in 8 endpoints
(`/configure`, `/configure_from_server_path`, `/resources/{id}/acquire`,
`/resources/{id}/release`, `/heartbeat`, `/disconnect`, etc.) totaling ~19
occurrences. The server implementation currently reads `X-User-Id` from a
header on most of these and ignores the body field.

The preferred direction is to **preserve `user_id` in the OpenAPI request
schemas** so the API remains self-documenting for code generators and Swagger
UI. The server should be updated to read `user_id` from the request body
(falling back to `X-User-Id` header if absent), aligning with the gRPC proto
which uses `user_id` as a message field.

This analysis should consider:
- Reading `user_id` from body first, `X-User-Id` header as fallback
- Consistency with gRPC (proto uses `user_id` message field)
- Access control middleware implications
- Impact on all existing client libraries

This will be tracked as a separate task, not part of Phase 36.

---

## Files Changed

| File | Changes |
|------|---------|
| `crates/ok-fpga-server/Cargo.toml` | Added `multipart` feature to axum |
| `crates/ok-fpga-server/src/rest/devices.rs` | Split `configure_fpga` into two handlers, added `Multipart` parsing, `resolve_user_id`, `finalize_configure`, returns `BitfileRecord` |
| `crates/ok-fpga-server/src/rest/resources.rs` | Updated error message with new endpoint name |
| `crates/ok-fpga-server/src/rest/access_middleware.rs` | Added test for new endpoint classification |
| `client-examples/rust-rest-gui-common/Cargo.toml` | Added `multipart` feature to reqwest |
| `client-examples/rust-rest-gui-common/src/lib.rs` | Updated `configure_fpga()` URL, rewrote `configure_fpga_from_file()` with multipart, updated test |
| `api/openapi.yaml` | Added `/configure_from_server_path` endpoint, cleaned Phase 34-C annotations |

**Commit:** `fc0d77d`

---

## References

> *Note ([ADR-034](ADR-034-resource-model-consolidation.md) §14, `D26`, Phase 44-F): Reprogramming (`POST /devices/{serial}/configure`, `POST /devices/{serial}/configure_from_server_path`, gRPC `ConfigureFPGA*`) and clearing (`DELETE /devices/{serial}/configure`, `ClearFPGAConfiguration`) perform unconditional forced cleanup of existing resources on the device (`reprogram` / `clear`) and never return `409 Conflict` for resources in use.*

- [OpenAPI spec — POST /devices/{serial}/configure](../../../api/openapi.yaml)
- [gRPC proto — ConfigureFPGA / ConfigureFPGAFromMemory](../../../proto/okfpga/v1/okfpga.proto)
- [ADR-017: FFI Endpoint Wiring Strategy](ADR-017-ffi-endpoint-wiring.md)
- [ADR-034: Resource Model Consolidation](ADR-034-resource-model-consolidation.md)
- [ADR-C-004: FPGA Programming Flow (Client)](../../client/ADR/ADR-C-004-programming-flow.md)
- [Configure FPGA Review](../../client/labview-developer-guide.html)
