# Archive — Phase 39: SSE Event Expansion

> **Archived:** 2026-08-23
> **Commits:** `84fbc33` (docs), `5f787a5` (implementation)
> **ADR:** [ADR-030 — SSE Event Expansion](../architecture/adr/ADR-030-sse-event-expansion.md)

---

## Phase 39 — SSE Event Expansion

**Goal:** Expand the `DeviceEvent` enum and SSE broadcast to include user
session lifecycle, resource lifecycle, and server shutdown events — bringing
the SSE endpoint to parity with the dashboard WebSocket.

**New `DeviceEvent` variants:**

| Variant | Trigger | Broadcast Location |
|---------|---------|--------------------|
| `UserConnected` | First API call from a user (new session) | `heartbeat_middleware.rs` |
| `UserDisconnected` (explicit) | User calls disconnect | `server.rs` |
| `UserDisconnected` (timeout) | Sweep detects expired heartbeat | `sweep.rs` |
| `ResourceCreated` | `POST /resources` | `resources.rs` |
| `ResourceDeleted` | `DELETE /resources/{id}` | `resources.rs` |
| `ServerShutdown` | SIGINT/SIGTERM received | `main.rs` |

**Key Decisions (→ ADR-030):**

- `HeartbeatTimeout` folded into `UserDisconnected { reason: "timeout" }`
- `UserSessionRegistry::touch()` returns `bool` (true = new session) — purely
  internal, no network effect
- SSE handler match is exhaustive (compiler-enforced)
- Also fixes client-example SSE path bug (`/events/stream` → `/events`)

**Exit Criteria (all met):**

- `cargo test` — 270 pass, 0 fail (5 new serialization round-trip tests)
- `cargo build` — exhaustive match enforced (no new warnings)
- Integration test: all 4 testable event types verified on live SSE stream
- `openapi.yaml` documents all 9 event types
- Rust REST client-example uses correct `/events` path
