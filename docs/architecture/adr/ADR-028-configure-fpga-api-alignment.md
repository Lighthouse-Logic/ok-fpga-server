# ADR-028: Configure FPGA API Alignment

**Status:** Proposed
**Date:** 2026-08-23
**Deciders:** Architecture Team
**Roadmap Phase:** 36

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

The `user_id` field has been removed from the `/configure` multipart schema. User
identification continues to use the `X-User-Id` header, which is consistent with
how every other REST endpoint identifies the user. A separate cross-cutting task
will audit the `user_id` field across all 8 endpoints that define it in their
request bodies to determine the canonical approach (see Deferred Items below).

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
(`/configure`, `/resources/{id}/acquire`, `/resources/{id}/release`,
`/heartbeat`, `/disconnect`, etc.) totaling 19 occurrences. The server
implementation reads `X-User-Id` from a header on most of these. The gRPC proto
uses `user_id` as a message field.

This discrepancy needs a dedicated analysis to determine the canonical approach,
considering:
- REST convention (header vs body)
- Consistency with gRPC (message field)
- Access control middleware (header-based is easier for middleware)
- Impact on all existing client libraries

This will be tracked as a separate task, not part of Phase 36.

---

## Files Changed

*(To be populated on implementation)*

---

## References

- [OpenAPI spec — POST /devices/{serial}/configure](../../api/openapi.yaml)
- [gRPC proto — ConfigureFPGA / ConfigureFPGAFromMemory](../../proto/okfpga/v1/okfpga.proto)
- [ADR-017: FFI Endpoint Wiring Strategy](ADR-017-ffi-endpoint-wiring.md)
- [ADR-C-004: FPGA Programming Flow (Client)](../../docs/client/ADR/ADR-C-004-programming-flow.md)
- [Configure FPGA Review](../../docs/client/labview-developer-guide.html)
