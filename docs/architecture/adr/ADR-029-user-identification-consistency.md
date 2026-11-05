# ADR-029: User Identification Consistency

**Date:** 2026-08-23  
**Status:** Accepted — Implemented (Phase 38, commit `ba7f9fa`; amended in [ADR-034](ADR-034-resource-model-consolidation.md) §13 `S-18` to reject missing identity instead of falling back to `"Unknown"`)  
**Phase:** [Phase 38 — User Identification Consistency](../../archive/roadmap-phases-38.md)  

---

## Context

The ok-fpga-server has two mechanisms for identifying which user is making a request:

1. **`X-User-Id` HTTP header** — Read by the access control middleware
   (`access_middleware.rs`) and heartbeat middleware (`heartbeat_middleware.rs`).
   Required when endpoint enforcement is active. Every REST request must include
   this header.

2. **`user_id` request body field** — Defined in the OpenAPI spec for 8 endpoints
   and in the gRPC proto for corresponding message types. Self-documenting for
   Swagger UI and code generators.

Prior to Phase 38, only the two configure endpoints (Phase 36, ADR-028) used the
`resolve_user_id()` function to read identity from body first, then fall back to
the header. All other handlers that accept `user_id` in their body read the body
field directly with no header fallback. This creates two problems:

- **Inconsistency**: Some handlers use the body field, others ignore it.
- **Fragility**: Clients that set only the `X-User-Id` header (e.g., the Python
  REST client library, curl one-liners) get a 422 deserialization error when the
  body's `user_id` is required but absent — even though the header provides the
  same information.

## Decision

### Canonical identity resolution order

All server handlers that need user identity MUST call a shared `resolve_user_id()`
function that applies the following precedence:

1. **Request body `user_id` field** (primary) — matches the gRPC proto convention
   and is self-documenting in OpenAPI/Swagger UI.
2. **`X-User-Id` HTTP header** (fallback) — used by middleware and by clients that
   prefer header-based identity.
3. **`"Unknown"`** — if neither source provides a non-empty value.

### Mismatch rejection

When both the body `user_id` and the `X-User-Id` header are present and non-empty,
they **MUST match**. If they differ, the server returns **400 Bad Request** with a
`ProblemDetail` explaining the mismatch. This prevents subtle bugs where the
middleware (which reads the header) and the handler (which reads the body) operate
on different user identities.

### Rust struct flexibility

Request body structs make `user_id` an `Option<String>` so that a missing body field
does not cause a 422 deserialization error. The handler then resolves identity via
the shared function. The OpenAPI spec retains `required: [user_id]` to signal that
well-behaved clients SHOULD always send it; the server is lenient when the header
provides a fallback.

### Middleware is header-only

The access control middleware and heartbeat middleware continue reading **only** the
`X-User-Id` header. Middleware runs before body parsing and should not parse or
buffer request bodies. This is correct and unchanged by this ADR.

### Clients send both

Client libraries and examples are updated to send `user_id` in **both** the request
body and the `X-User-Id` header, set to the same value. This ensures compatibility
with both the middleware (header) and the handler (body).

## Affected Endpoints

| # | Endpoint | Body field | Header | Handler |
|---|----------|-----------|--------|---------|
| 1 | `POST /devices/{serial}/configure` | `user_id` (optional) | `X-User-Id` | `devices.rs` — already uses `resolve_user_id()` |
| 2 | `POST /devices/{serial}/configure_from_server_path` | `user_id` (optional) | `X-User-Id` | `devices.rs` — already uses `resolve_user_id()` |
| 3 | `POST /resources/{id}/acquire` | `user_id` (required → optional) | `X-User-Id` | `resources.rs` — **updated** |
| 4 | `POST /resources/{id}/release` | `user_id` (required → optional) | `X-User-Id` | `resources.rs` — **updated** |
| 5 | `POST /resources/{id}/lock` | `user_id` (required → optional) | `X-User-Id` | `resources.rs` — **updated** |
| 6 | `POST /resources/{id}/unlock` | `user_id` (required → optional) | `X-User-Id` | `resources.rs` — **updated** |
| 7 | `POST /heartbeat` | `user_id` (required → optional) | `X-User-Id` | `server.rs` — **updated** |
| 8 | `POST /disconnect` | `user_id` (required → optional) | `X-User-Id` | `server.rs` — **updated** |

## Future Considerations: TLS and Certificate-Based Identity

The current `resolve_user_id()` chain (body → header → "Unknown") is designed to
be extensible. A future phase implementing native TLS termination (see Later:
Future Horizons in the development roadmap) should consider adding a third identity
source:

**Proposed future chain:** mTLS client certificate CN/SAN → body `user_id` → `X-User-Id` header → "Unknown"

With mutual TLS (mTLS), the server can extract user identity from the client
certificate's Common Name (CN) or Subject Alternative Name (SAN) fields. This would
provide cryptographically verified identity without requiring users to "log in" — the
certificate itself authenticates the client. Key considerations:

- **Certificate-only deployments:** If mTLS is configured, the certificate identity
  could be sufficient — no `X-User-Id` header or body field needed. The
  `resolve_user_id()` function would simply add certificate identity as the highest-
  priority source.
- **Mixed deployments:** Some clients may use certificates while others use
  header-based identity. The resolution chain handles this naturally.
- **No impact on current design:** The body field and header remain useful even with
  TLS — they are the primary mechanism for non-TLS deployments and for gRPC parity
  (gRPC uses `user_id` as a message field, not a header).
- **gRPC parity:** gRPC uses channel credentials (TLS certificates) at the transport
  level and `user_id` as a message field at the application level. The REST API
  mirrors this with TLS at the transport level and `user_id` in the body/header at
  the application level.

This ADR does not implement TLS — it documents the extensibility point for a future
phase.

## Files Changed

- `crates/ok-fpga-server/src/rest/mod.rs` — shared `resolve_user_id()` function
- `crates/ok-fpga-server/src/rest/devices.rs` — remove local `resolve_user_id()`,
  import from `mod.rs`
- `crates/ok-fpga-server/src/rest/resources.rs` — update 4 handlers + request structs
- `crates/ok-fpga-server/src/rest/server.rs` — update 2 handlers + request structs
- `api/openapi.yaml` — update `user_id` field descriptions in 8 endpoints
- `client-examples/` — update client libraries to send `user_id` in both body and header

## Open Items

- The `client/python-labview-client` will be updated separately in Phase C-9.
