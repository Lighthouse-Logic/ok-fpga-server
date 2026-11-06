// chapters/overview/2-2-identity.typ — Section 2.2: Identity, sessions and heartbeat.
// ref: ADR-008 (auth providers; NoAuth default), ADR-020 §5 (sessions & 24 s / 60 s heartbeat),
// ADR-029 (caller identity resolution & mismatch rules), ADR-034 §6–§7.
#import "../../lib.typ": *

== Identity, sessions and heartbeat <ov-identity>

=== Caller identity resolution and mismatch rules <ov-id-resolution>
// ref: ADR-029; ADR-034 P8; @res-identity.
Every request that creates or uses a session, acquires or locks a resource, or
accesses a device under `endpoint_enforcement = "warn"` or `"enforce"` MUST
identify the calling user. The server resolves the caller's `user_id` from two
transport locations:

#table(
  columns: (auto, 1.3fr, 1.3fr, 2fr),
  [Transport], [Request field], [Header / metadata], [Resolution & mismatch rule],
  [REST], [`user_id` JSON body or multipart form field (where defined)], [`X-User-Id` HTTP header], [If both are non-empty, they MUST be identical; if they differ, the server MUST reject the request with `400 Bad Request` (`ProblemDetail`). If only one is provided, that value is used.],
  [gRPC], [`user_id` field on the request message (where defined)], [`x-user-id` gRPC metadata key], [If both are non-empty and differ, the server MUST reject the call with `INVALID_ARGUMENT`. Data-port RPC messages carry no `user_id` field, so gRPC clients MUST always send `x-user-id` metadata.],
)

When neither a body/message field nor a header/metadata key is supplied:
- Under `endpoint_enforcement = "enforce"`, data-port, device and management
  requests that require identity MUST be rejected with `403 Forbidden` (REST)
  or `PERMISSION_DENIED` (gRPC) (@enf-levels, @enf-tiers).
- On operations that accept anonymous calls when enforcement is `"off"`, the
  caller falls back to `"Unknown"`.

*Rationale.* Accepting both a body field and a header makes REST calls easy to
test from interactive OpenAPI/Swagger tools while allowing HTTP/gRPC middleware
to inspect identity uniformly from headers/metadata. Rejecting mismatches
prevents a client from accidentally attributing an operation to one user in
middleware while mutating state under another user in the payload.

=== Pluggable authentication providers <ov-id-auth>
// ref: ADR-008 (pluggable authentication architecture); config.toml [auth].
Authentication is configured in the `[auth]` table of `config.toml`
(@ov-config-auth-access):

#table(
  columns: (auto, 1fr, 2fr),
  [`auth.provider`], [Scheme], [Behaviour],
  [`"none"` (default)], [Self-asserted `X-User-Id` / `x-user-id`], [No password check. Any client on the network may assert any `user_id`. Intended for trusted lab networks.],
  [`"local"`], [HTTP Basic (`basicAuth`)], [Validates credentials against the static user list in `[auth.local]`. Sessions expire after `auth.session_expiry_secs` (default `28800` s = 8 hours).],
  [`"ldap"`], [HTTP Basic (`basicAuth`)], [Validates credentials against the LDAP/LDAPS directory configured in `[auth.ldap]` (`url`, `use_ssl`, `domain_components`, `organizational_units`, `uid_attribute`).],
)

When authentication fails, the server returns `401 Unauthorized` (REST) or
`UNAUTHENTICATED` (gRPC) (@intro-two-transports).

=== Session lifecycle, heartbeat and disconnect <ov-id-sessions>
// ref: ADR-020 §5; ADR-030 (UserConnected/UserDisconnected); ADR-034 §6–§7;
// @res-heartbeat; @res-users.

#table(
  columns: (auto, 1.2fr, 1.3fr, 1.8fr),
  [Operation], [REST], [gRPC (`DeviceService`)], [Response],
  [List users], [`GET /users`], [`ListUsers(ListUsersRequest)`], [`{ "users": [UserSession, ...] }` / `ListUsersResponse` (@res-users)],
  [Get user], [`GET /users/{user_id}`], [`GetUser(UserRequest)`], [`UserSession` / `UserResponse` (`404` / `NOT_FOUND` if unknown, @appendix-status)],
  [Heartbeat], [`POST /heartbeat`], [`Ping(PingRequest)`], [`{ "server_time", "user_id" }` / `PingResponse { server_time, error }`],
  [Disconnect], [`POST /disconnect`], [`Disconnect(DisconnectRequest)`], [`{ "user_id", "released_resources", "message" }` / `DisconnectResponse`],
)

Session lifecycle rules:

+ *Implicit session creation and touch.* The first identified API call from a
  `user_id` creates a `UserSession` and emits a `UserConnected` event
  (@ov-events-streams). Every subsequent identified REST or gRPC request
  updates that session's last-seen timestamp.
+ *Explicit heartbeat (`POST /heartbeat` / `DeviceService.Ping`).* While a
  client holds any resource and is otherwise idle, it SHOULD send a heartbeat
  at least every *24 seconds* (@res-heartbeat). Both `POST /heartbeat` and
  `Ping` require a non-empty `user_id` (`400 Bad Request` / `INVALID_ARGUMENT`
  if missing or mismatched).
+ *Inactivity timeout and sweep.* A background sweep task runs every
  `server.user_sweep_interval_secs` (default `10` s) and expires any session
  that has been silent for more than `server.user_timeout_secs` (default
  `60` s; set to `0` to disable expiration). When a session expires, the
  server atomically unlocks and releases every resource the user holds (emitting
  `ResourceUnlocked` and `ResourceChanged` as appropriate) and emits
  `UserDisconnected` with `reason = "timeout"`.
+ *Clean disconnect (`POST /disconnect` / `DeviceService.Disconnect`).*
  Releases all resources held by `user_id` across all devices, ends any locks
  held by that user, removes the session immediately, emits
  `UserDisconnected` with `reason = "explicit"`, and returns the count of
  released resources in `released_resources`. If `user_id` has no active
  session on REST, `POST /disconnect` returns `404 Not Found`.
