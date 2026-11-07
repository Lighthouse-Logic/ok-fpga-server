// chapters/resource-model.typ — Section 3: Resource Model.
// Drafted by local-strata (job_0a95d459), reviewed and corrected by
// Antigravity 2026-10-11 against ADR-034, docs/resource-management-
// developer-guide.md rev 10, api/openapi.yaml and okfpga.proto (0.3.1).
// ref: ADR-006, ADR-020 (partly superseded), ADR-026, ADR-034 §1–§7;
// decisions D1–D78 (docs/resource-management-recommendations.md).
#import "../lib.typ": *

= Resource Model <resource-model>

// ref: ADR-006 Context 1–3; ADR-020 §1; D9, D21, D20, D46
FrontPanel gives one host one handle to a device (@intro-modules). The server
holds that handle and lets several clients use the device at once. The
*resource model* is how the clients agree, through the server, on who may use
which parts of the FPGA design.

== Concepts <res-concepts>

=== Device, resource and item <res-concept-resource>
// ref: ADR-006 Resource Model; ADR-034 §1; D9, D21
- A *device* is a connected Opal Kelly module, identified by its serial
  number. Resources can exist on a device only while it is programmed.
- A *resource* is a named, client-defined capability of a programmed device:
  "port A", "the DAC", "the capture engine". It is *not* the whole FPGA. One
  device MAY expose many resources.
- A resource is made of *items*: Classic endpoints (@intro-endpoints) and
  32-bit *address ranges*. A range covers okRegisterBridge registers, or
  memory reached through a developer-defined address mapping
  (@enf-address-mapping), or direct AXI addresses.

*Rationale.* The server cannot know what a bitfile does. Only its designers
know which endpoints form a "port" and which registers are dangerous to
share. So the server is deliberately agnostic: the client that programs the
FPGA declares the resources, and the server enforces the declaration. If a
design needs stronger protection than the defaults give, the architect
declares a stricter resource (an exclusive resource, or `private` items); the
server never guesses.

// ref: D20, D46 (server enforces who may write; atomicity is the client's job)
The server enforces *who may write what*. It does not make a sequence of
calls atomic; clients do that with the lock (@res-lock-when).

=== Three different "modes" <res-concept-modes>
// ref: ADR-034 §1; D33, D44, D62
Three separate settings use the words "shared" and "exclusive". Keep them
apart:

#table(
  columns: (auto, auto, auto, 1fr),
  [Setting], [Values], [Chosen at], [Describes],
  [Declared mode (`access_mode`)], [`shared`, `exclusive`], [Registration], [The resource: may it have one or many read/write holders?],
  [Holder mode (`mode`)], [`shared`, `exclusive`, `read_only`], [Acquisition], [How *one user* holds the resource.],
  [Item access (`access`)], [`shared`, `exclusive`], [Registration], [One endpoint or range: may other resources declare it too?],
)

- An *exclusive resource* has exactly one read/write holder at a time (plus
  any number of ReadOnly observers). It contains only exclusive items.
- A *shared resource* may have many holders. Every non-ReadOnly holder may
  write all of its items without a lock.
- An *exclusive item* belongs to one resource only: no other resource may
  declare it. It is *not* a one-writer guarantee. Every non-ReadOnly holder
  of the declaring resource may write it.
- A *shared item* may also be declared by other shared resources.

#caution[
  "Exclusive item" does not mean "one writer". If only one writer may ever
  exist, declare an *exclusive resource*, or have the clients lock
  (@res-lock-when).
]

=== Holders, locks and blocking <res-concept-lock>
// ref: ADR-020 §3; D1, D2, D5, D7, D18, D19, D35
- A user becomes a *holder* by acquiring a resource. Holding is what grants
  the right to write the resource's items.
- A *lock* is a temporary reservation of a whole shared resource by one of
  its holders. While it lasts, no other user may write through that
  resource. The other holders stay holders.
- *Blocked* is a per-user condition, not a resource state. A resource S is
  blocked for user U when another user holds the lock on a resource R that
  declares at least one of the same shared items as S (@res-blocking).

=== Private <res-concept-private>
// ref: ADR-026 §2 (L1, L2, L3); D11, D22, D57, D75
`private` is an access rule, not a visibility rule. Private resources and
items are still listed. `private` restricts *ReadOnly* holders only; it has no
effect on shared or exclusive holders.

#table(
  columns: (1.3fr, 1.3fr, 2fr),
  [Level], [Where], [Effect on ReadOnly holders],
  [Resource], [`private` on the resource], [ReadOnly acquisition is denied.],
  [Endpoint], [`private` on an endpoint entry], [Reading that endpoint is denied.],
  [Address range], [`private` on a range], [Reading that range is denied.],
)

The HDL architect SHOULD mark an item `private` whenever a ReadOnly read of
it would cause an unwanted side effect (@enf-readonly-design-rule).

== Declaring a resource <res-declare>
// ref: ADR-020 §1, §10–§11; ADR-034 §3; D33, D36, D37, D38, D54, D55, D61,
// D62, D68, D69, D71, D77

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [Create], [`POST /resources`], [`DeviceService.CreateResource`],
  [List], [`GET /resources`], [`DeviceService.ListResources`],
  [Get one], [`GET /resources/{resource_id}`], [`DeviceService.GetResource`],
)

The device MUST be connected and programmed. A successful create returns
`201 Created` with a `Location: /api/v1/resources/{resource_id}` header and
the new `Resource`, in state `available`. The resource id is
`{device_serial}:{name}`. `POST` is used because the server forms the id.
When one or more registration warnings fire (@res-registration,
@enf-readonly-warning), the `201 Created` `Resource` object on REST includes a
read-only `warnings` string array (omitted when empty and on `GET`, acquire,
lock and unlock responses), and gRPC `CreateResource` populates
`ResourceResponse.warnings` (proto field 3, empty on other RPCs).

=== Request fields <res-declare-request>
`CreateResourceRequest`:

#table(
  columns: (1.2fr, 1.1fr, 1.5fr, 1fr, 2fr),
  [Field], [REST type], [Proto], [Required], [Meaning],
  [`name`], [string], [1 `string`], [yes], [Unique per device.],
  [`description`], [string], [2 `string`], [no], [Free text.],
  [`device_serial`], [string], [3 `string`], [yes], [Device that provides the resource.],
  [`access_mode`], [`shared` | `exclusive`], [4 `optional ResourceAccessMode`], [yes, unless `endpoints` is omitted], [Declared mode (@res-concept-modes).],
  [`interfaces`], [string array], [5 `repeated string`], [yes], [`classic`, `axi_lite`, `axi_full`, `axi_stream` (@res-interfaces).],
  [`endpoints`], [object], [6 `EndpointDeclaration`], [no], [Classic endpoints; three shapes (@res-endpoint-shapes).],
  [`address_ranges`], [array], [7 `repeated AddressRange`], [no], [Ranges; only declared ranges are claimed.],
  [`metadata`], [string map], [8 `map<string,string>`], [no], [Text values only; stored as-is and shown on the dashboard.],
  [`private`], [boolean], [9 `bool`], [no], [Default `false`. Resource-level private.],
)

// ref: D59 (no UNSPECIFIED enum values); ADR-034 §1
*Rationale for `optional` in proto.* The proto enums have no "unspecified"
value. `access_mode` and `access` are `optional` so the server can tell "not
sent" from the first enum value (`shared`). A missing `access_mode` is
therefore never silently read as `shared`.

`EndpointDeclaration` and its entries:

#table(
  columns: (1.7fr, 1.2fr, 1.5fr, 2fr),
  [Field], [REST type], [Proto], [Meaning],
  [`default_access`], [`shared` | `exclusive`], [1 `optional ResourceAccessMode`], [The `access` used by listed entries that leave it out. Never claims unlisted endpoints.],
  [`entries`], [array], [2 `repeated EndpointEntry`], [The declared endpoints.],
  [`entries[].address`], [hex string, e.g. `"0x05"`], [1 `uint32`], [Endpoint address; must fall in the block of its `type`.],
  [`entries[].type`], [string], [2 `string`], [`wire_in`, `wire_out`, `trigger_in`, `trigger_out`, `pipe_in`, `pipe_out`, `bt_pipe_in`, `bt_pipe_out`.],
  [`entries[].access`], [`shared` | `exclusive`], [3 `optional ResourceAccessMode`], [Item access. Required unless `default_access` is set.],
  [`entries[].private`], [boolean], [4 `bool`], [Endpoint-level private. Default `false`.],
  [`entries[].description`], [string], [5 `string`], [Free text.],
)

`AddressRange`:

#table(
  columns: (1.3fr, 1.2fr, 1.5fr, 2fr),
  [Field], [REST type], [Proto], [Meaning],
  [`name`], [string], [1 `string`], [Optional label, e.g. `"dac_ctrl"`.],
  [`start`], [hex string, e.g. `"0x44A00000"`], [2 `uint32`], [First address.],
  [`end`], [hex string], [3 `uint32`], [Last address, inclusive; `start` ≤ `end`.],
  [`access`], [`shared` | `exclusive`], [4 `optional ResourceAccessMode`], [Item access. Required.],
  [`private`], [boolean], [5 `bool`], [Range-level private. Default `false`.],
  [`description`], [string], [6 `string`], [Free text.],
)

=== The three shapes of `endpoints` <res-endpoint-shapes>
// ref: D15a, D23, D52, D55, D61, D68, D77 (D77 revises D68 coverage)

#table(
  columns: (auto, 1fr, 1fr),
  [Shape], [Meaning], [`access_mode` rule],
  [`endpoints` omitted], [An exclusive resource that claims every Wire and Trigger endpoint, `0x00`–`0x7F`. Pipes are not claimed.], [May be omitted (becomes `exclusive`). `exclusive` accepted. `shared` → 422.],
  [`{"entries": []}`], [No Classic endpoints at all.], [Required.],
  [`{"entries": [...]}`], [Exactly the listed endpoints.], [Required.],
)

- Pipe and block-throttled pipe endpoints (`0x80`–`0xBF`) MUST always be
  declared explicitly.
- `default_access` MUST only fill in a missing `access` on a listed entry. It
  MUST NOT claim unlisted endpoints. An entry with neither `access` nor
  `default_access` MUST be rejected with 422.
- In every shape, a resource claims only the address ranges it declares. A
  resource with no ranges can reach no address.

*Rationale.* The omitted shape is a convenience for the common
single-resource bitfile: one client, everything. It is still an ordinary
exclusive resource with no special name or behaviour. It stops at `0x7F`
because Opal Kelly uses the same addresses for standard pipes and
block-throttled pipes (`okPipeIn` and `okBTPipeIn` in `0x80`–`0x9F`,
`okPipeOut` and `okBTPipeOut` in `0xA0`–`0xBF`)#footnote[Opal Kelly, _FrontPanel HDL:
Endpoints_ (endpoint address ranges per type),
#link("https://docs.opalkelly.com/fpsdk/frontpanel-hdl/")[docs.opalkelly.com/fpsdk/frontpanel-hdl]\.]\.
The server cannot tell which one the bitfile instantiated, so it cannot
choose the right transfer call; the client must say. The empty shape exists
so a resource can claim *only* registers, without accidentally claiming every
endpoint.

=== Interfaces and data ports <res-interfaces>
// ref: ADR-003; ADR-020 §6, §10; D49, D50, D54, D70
FrontPanel devices report a *data port* type. Every device supported today
reports the Classic data port (FrontPanel `OK_DATAPORT_LEGACY`); future
devices may report the AXI data port (`OK_DATAPORT_AXI`). Devices that report
an unknown data port are not listed and are treated as absent (404).

#table(
  columns: (auto, auto, 1fr),
  [Interface], [Allowed on], [Reaches],
  [`classic`], [every device], [Classic endpoints and okRegisterBridge registers.],
  [`axi_lite`, `axi_full`, `axi_stream`], [AXI data port devices only], [Direct AXI addresses.],
)

A resource MAY list both `classic` and AXI interfaces. On a Classic data port
device any AXI interface MUST be rejected with 422, so today every resource is
Classic-only.

=== Examples <res-declare-examples>

#example("Quick single-resource declaration")[
  Exclusive; claims WireIn, WireOut, TriggerIn and TriggerOut `0x00`–`0x7F`.
  ```json
  { "name": "whole_design", "device_serial": "2149000ABC", "interfaces": ["classic"] }
  ```
]

#example("Register-only resource")[
  No endpoints; one exclusive register range.
  ```json
  {
    "name": "dsp_regs",
    "device_serial": "2149000ABC",
    "access_mode": "exclusive",
    "interfaces": ["classic"],
    "endpoints": { "entries": [] },
    "address_ranges": [
      { "name": "dsp_ctrl", "start": "0x00001000", "end": "0x00001FFF", "access": "exclusive" }
    ]
  }
  ```
]

#example("Two shared resources that share a status WireOut")[
  Both may declare WireOut `0x20` because it is shared in a shared resource on
  both sides (@res-registration, row 1).
  ```json
  [
    { "name": "counter_ctrl", "device_serial": "2149000ABC",
      "access_mode": "shared", "interfaces": ["classic"],
      "endpoints": { "default_access": "shared", "entries": [
        { "address": "0x00", "type": "wire_in" },
        { "address": "0x20", "type": "wire_out" } ] } },
    { "name": "counter_monitor", "device_serial": "2149000ABC",
      "access_mode": "shared", "interfaces": ["classic"],
      "endpoints": { "entries": [
        { "address": "0x20", "type": "wire_out", "access": "shared" } ] } }
  ]
  ```
]

== Registration rules <res-registration>
// ref: D6, D8, D22, D23, D33, D38, D48, D54, D55, D61, D62, D68, D72, D77
// ref: ADR-034 §10 (44-B owner decisions Q1, Q3, Q4); implemented in 0.3.3.

Two rules govern what may be registered:

+ *Exclusive resources contain only exclusive items.* A resource with
  `access_mode: exclusive` that declares a shared endpoint, a shared range,
  or `default_access: shared` MUST be rejected with *422*, whatever else
  exists.
+ *First registration wins.* The first resource that declares an endpoint or
  range defines it. A later registration MUST be accepted only if every
  overlap is a shared item in a shared resource *on both sides*, with the same
  endpoint type and the same item-level `private` flag (the resource-level
  `private` flag is not compared). Otherwise it MUST be rejected
  with *409*, and the error MUST name the existing resource and the
  conflicting item (@res-errors).

Inside one declaration:

- The same endpoint address MUST NOT appear twice, even with different
  endpoint types (for example `pipe_in` and `bt_pipe_in` at `0x80`). This is
  a *422* naming the endpoint.
- On REST, endpoint addresses MUST be written as `0x` followed by exactly
  two hex digits (`0x05`); `5`, `0x5`, `0x005` and `0X05` are rejected with
  *422*. gRPC sends addresses as numbers.
- Address ranges MAY overlap each other. The request succeeds (*201*), and
  the server logs a warning and returns it in the `CreateResource` response
  `warnings` list unless the server setting
  `access_control.warn_range_self_overlap` is turned off. Where overlapping
  ranges differ in `private`, the private range applies to the addresses they
  share.
- Likewise, enabled registration warnings for non-private ReadOnly side-effect
  items (`access_control.warn_readonly_side_effects`, @enf-readonly-warning)
  and omitted `endpoints` / Pipe coexistence
  (`access_control.warn_wildcard_endpoints`) are logged and returned in the
  `CreateResource` `warnings` list.

The server uses 409 for a clash with existing state and 422 for a request
that is wrong on its own.

#table(
  columns: (auto, 1fr, 1fr, 1fr),
  [\#], [Registered first (A)], [Registered second (B)], [Outcome for B],
  [1], [shared resource, EP `0x00` shared], [shared resource, EP `0x00` shared], [201: overlap allowed],
  [2], [shared resource, EP `0x00` shared], [shared resource, EP `0x00` exclusive], [409: already declared shared by A],
  [3], [shared resource, EP `0x00` exclusive], [shared resource, EP `0x00` shared], [409: already declared exclusive by A],
  [4], [shared resource, EP `0x00` exclusive], [shared resource, EP `0x00` exclusive], [409],
  [5], [exclusive resource with EP `0x00`], [any resource declaring EP `0x00`], [409: belongs to exclusive resource A],
  [6], [shared resource, EP `0x00` shared], [exclusive resource declaring EP `0x00` shared], [422: exclusive resource with a shared item],
  [7], [—], [exclusive resource with any shared item], [422, whatever exists],
  [8], [shared, range `0x8000_0000`–`0x8FFF_FFFF` shared], [shared, range `0x8800_0000`–`0x9FFF_FFFF` shared], [201: partial overlap of shared ranges allowed],
  [9], [shared, range X shared], [any resource, overlapping range exclusive], [409],
  [10], [any, range X exclusive], [any overlapping range], [409],
  [11], [shared, EP `0x20` shared, not private], [shared, EP `0x20` shared, private], [409: private flag differs],
  [12], [exclusive resource with `endpoints` omitted], [any resource that declares a Classic endpoint], [409: A blocks every later resource that declares Classic endpoints, pipes included],
  [13], [any], [same `name` on the same device], [409 (no conflict detail)],
  [14], [—], [`endpoints` present, `access_mode` missing], [422: `access_mode` is required],
  [15], [—], [`endpoints` and `access_mode` both omitted], [201: exclusive, Wire and Trigger endpoints],
  [16], [—], [`endpoints` omitted, `access_mode: shared`], [422: a resource without endpoints is exclusive],
  [17], [—], [entry with no `access` and no `default_access`], [422],
  [18], [—], [Classic data port device, any AXI interface listed], [422],
  [19], [exclusive resource with `endpoints` omitted], [`{"entries": []}` with ranges that do not overlap A's ranges], [201: B claims no endpoints],
  [20], [—], [one declaration listing EP `0x05` twice], [422: duplicate endpoint address],
  [21], [—], [one declaration with two overlapping ranges], [201: allowed; a warning is logged],
  [22], [shared resource, EP `0x00` shared, resource not private], [shared resource, EP `0x00` shared, resource private], [201: resource-level `private` is not compared],
)

Order matters: swapping rows 2 and 3 changes *which* registration fails,
never *whether* both can coexist. When B clashes with several existing
resources, the error names the one with the lowest resource ID, so the answer
is the same every time.

The server checks a new declaration in this order: the request itself
(*422*), the device (*404*), interfaces against the device's data port
(*422*), whether the device is programmed (*409*), and finally overlap with
existing resources (*409*). A declaration that can never be valid on a device
therefore gets *422* even while that device is not programmed.

*Rationale.* Shared items in shared resources may overlap because several
functions often need the same physical endpoints. In the example of
@example-dual-port, two network ports drive one AXI bridge through the same
WireIns, so both port resources must declare them. Everything else must have
one clear owner, and "first registration wins" gives a deterministic answer
without a central schema. Requiring the same `private` flag on both sides
prevents one resource from silently weakening another's protection.

== Resource states <res-states>
// ref: ADR-006 states; ADR-034 §2 (wire strings); D25

States are sent as lowercase strings on REST, SSE and the dashboard, and as
the `ResourceState` enum on gRPC.

#table(
  columns: (auto, 1fr),
  [State], [Meaning],
  [`unavailable`], [Device not programmed, or resource not usable.],
  [`initializing`], [Being set up.],
  [`available`], [Registered, no holders.],
  [`in_use_shared`], [At least one holder, not locked (including only ReadOnly holders).],
  [`in_use_locked`], [An exclusive resource is held, or a shared resource is locked.],
)

#table(
  columns: (auto, 1fr, auto),
  [From], [Event], [To],
  [`available`], [acquire as shared or ReadOnly], [`in_use_shared`],
  [`available`], [acquire an exclusive resource], [`in_use_locked`],
  [`in_use_shared`], [acquire / release, holders remain], [`in_use_shared`],
  [`in_use_shared`], [lock], [`in_use_locked`],
  [`in_use_shared`], [exclusive acquire while only ReadOnly holders are present], [`in_use_locked`],
  [`in_use_locked`], [unlock, or the locker releases while others remain], [`in_use_shared`],
  [`in_use_shared` / `in_use_locked`], [last holder releases], [`available`],
  [any], [delete, device disconnect, reprogram or clear], [deleted],
)

Blocking (@res-blocking) never changes a resource's own state.

== Acquire and release <res-acquire>
// ref: ADR-006; ADR-020 §3; ADR-026 §4; ADR-034 §11 (44-C, Q1, Q2); D7, D15f, D25, D28, D43, D62; implemented in 0.3.4.

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [Acquire], [`POST /resources/{resource_id}/acquire`], [`DeviceService.AcquireResource`],
  [Release], [`POST /resources/{resource_id}/release`], [`DeviceService.ReleaseResource`],
)

Request fields (both operations carry `resource_id`, proto field 1, as the
path parameter on REST, and `user_id`, proto field 2, in the body):

#table(
  columns: (0.8fr, 1.3fr, 1.3fr, 2fr),
  [Field], [REST type], [Proto], [Meaning],
  [`user_id`], [string], [2 `string`], [Caller identity (@res-identity).],
  [`mode`], [`shared` | `exclusive` | `read_only`], [3 `optional AccessMode`], [Acquire only. Omitted = the declared mode. `read_only` must be asked for explicitly.],
)

Acquire outcomes:

#table(
  columns: (1.4fr, 1.6fr, 1.3fr),
  [Resource state], [Request], [Result],
  [`available`], [`mode` omitted or equal to `access_mode`], [200],
  [any], [`shared` on an exclusive resource, or `exclusive` on a shared resource], [409 (`mode_mismatch`)],
  [`available`], [`read_only`], [200 → `in_use_shared` (or 409 `private` if the resource is private)],
  [`in_use_shared` (shared resource)], [`shared`], [200: holder added],
  [`in_use_shared` / `in_use_locked`], [`read_only`], [200 (or 409 `private` if the resource is private)],
  [`in_use_shared`, only ReadOnly holders (exclusive resource)], [`exclusive`], [200 → `in_use_locked`; ReadOnly holders stay],
  [`in_use_locked` (exclusive resource held)], [`exclusive`], [409 (`locked`)],
  [`in_use_locked` (shared resource locked)], [`shared`], [409 (`locked`)],
  [any], [caller already holds it in the *same* mode], [200, idempotent, no change],
  [any], [caller already holds it in a *different* mode], [409 (`mode_mismatch`): release first],
  [`unavailable` / `initializing`], [any], [409],
)

Acquiring a shared resource that is *blocked* succeeds. The caller becomes a
holder but cannot write until the blocking lock ends.

Release removes the caller from the holders. When no holders remain the
resource returns to `available`. A locker who releases ends her lock, and the
resource returns to `in_use_shared` if others remain.

*Rationale.* The declared mode is a property of the hardware design, fixed at
registration by the architect. Letting a client acquire in a different mode
would let it override the design, so a mismatch is an error rather than a
silent correction. Repeating an acquire in the same mode is harmless and is
accepted so that clients can retry after a network error. Changing mode
requires an explicit release so a client never loses write rights by
accident.

#example("Acquire shared, then read-only by an observer")[
  ```text
  POST /api/v1/resources/2149000ABC:port-a/acquire
  X-User-Id: alice
  {"user_id": "alice"}

  POST /api/v1/resources/2149000ABC:port-a/acquire
  X-User-Id: carol
  {"user_id": "carol", "mode": "read_only"}
  ```
  gRPC: `AcquireResourceRequest { resource_id: "2149000ABC:port-a",
  user_id: "carol", mode: ACCESS_MODE_READ_ONLY }`.
]

== Lock and unlock <res-lock>
// ref: ADR-020 §3; ADR-024 §6; D1, D5, D18, D19, D35, D38, D44, D46
// Implementation status: blocking (B1–B4) and WatchDeviceEvents implemented in 44-D (0.3.5).

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [Lock], [`POST /resources/{resource_id}/lock`], [`DeviceService.LockResource`],
  [Unlock], [`POST /resources/{resource_id}/unlock`], [`DeviceService.UnlockResource`],
)

Both take `resource_id` (proto field 1) and `user_id` (proto field 2) in
`LockResourceRequest`, and return the updated `Resource`.

- The caller MUST hold the resource, and not as `read_only`.
- Lock MUST be denied with 409 if the resource is already locked by another
  user, or is blocked for the caller. The error names the locking resource
  and user (`conflict.code` = `locked` or `blocked`).
- Locking a resource that the caller *already* holds the lock on is an
  idempotent no-op: the server returns the resource (`locked_by` = caller)
  without emitting a duplicate `ResourceLocked` event.
- Lock is try-lock only. The server MUST NOT queue lock requests. A client
  that is refused waits for a `ResourceUnlocked` event and retries.
- An exclusive resource is implicitly locked while held. Lock and unlock on
  it MUST return an error.
- On success the state becomes `in_use_locked` and `locked_by` is the caller.
  Other holders stay holders and receive a `ResourceLocked` event.
- Only the locker may unlock. Unlock returns the state to `in_use_shared`.
  Holders of the resource *and of every resource it blocked* receive
  `ResourceUnlocked` (including when the lock ends implicitly via release,
  disconnect or session timeout).

=== Blocking <res-blocking>
Resource S is *blocked* for user U when another user V (V ≠ U) holds the
lock on a resource R that declares at least one of the same shared items
(endpoint or range) as S. For U, a blocked resource behaves as if it were
locked by another user, *as a whole*:

- no writes through S, to any of S's items (shared or exclusive endpoints,
  ranges);
- no lock on S;
- reads of S's items remain allowed (subject to the ReadOnly rules).

The locker is never blocked by her own lock. Blocking is *not transitive*:

#example("Non-transitive blocking")[
  R = {A, B}, S = {B, C}, T = {C, D}; all shared. Alice locks R.
  #table(
    columns: (auto, auto, 1fr),
    [Resource], [Blocked for others?], [Why],
    [R], [locked by Alice], [—],
    [S], [yes], [shares item B with R],
    [T], [no], [shares nothing with R],
  )
  Bob holds S and T. He can still write C through T, because C is not one of
  R's items. He cannot write B, or anything else through S.
]

*Rationale.* A lock exists to keep other users away from items while one
user runs a multi-step sequence. If another resource that declares the same
shared item stayed writable, the lock would protect nothing; so overlapping
resources must be blocked too. Blocking the overlapping resource as a whole,
rather than item by item, keeps the rule simple to state and to check.
Stopping at direct overlap (not transitive) keeps unrelated parts of the
design usable.

=== When to lock <res-lock-when>
// ref: D20, D24, D46
The server *never requires* a lock for a write. Locking is how a client keeps
other users out while it does something that must not be interleaved.

- Multi-step operations (several calls that must not be interleaved, writes
  *or* reads) on a resource that other users may hold: the client MUST lock
  first, run the steps, then unlock. The server cannot tell that separate
  calls belong together, so this is a client obligation.
- Single-step operations need no lock.
- Single-user setups (a single-resource bitfile, or an exclusive resource)
  need no lock at all.

Each call is checked on its own against the lock state at that moment. If
another user locks between two of your unlocked calls, your next write is
denied (403).

== Heartbeat, disconnect and identity <res-heartbeat>
// ref: ADR-020 §5; ADR-029 (identity); config user_timeout_secs = 60,
// user_sweep_interval_secs = 10. OpenAPI /heartbeat and proto Ping aligned
// to ≤ 24 s on 2026-10-11 (owner permission; previously "30 s").

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [Heartbeat], [`POST /heartbeat`], [`DeviceService.Ping`],
  [Disconnect], [`POST /disconnect`], [`DeviceService.Disconnect`],
)

- Every identified API call refreshes the user's last-seen time. An idle
  client SHOULD send a heartbeat at least every 24 seconds.
- If the server sees nothing from a user for the configured timeout
  (`user_timeout_secs`, default 60 seconds; checked every 10 seconds), it
  MUST release everything that user holds, ending the user's locks.
- `POST /disconnect` releases everything at once and ends the session. The
  response reports `user_id`, `released_resources` (a count) and `message`.
- The heartbeat response carries `server_time` and (REST) the echoed
  `user_id`.

*Rationale.* A client that crashes while holding a lock would otherwise block
every overlapping resource forever. A 24-second heartbeat against a
60-second timeout tolerates one lost heartbeat without losing the session.

=== Identity <res-identity>
- REST: the `user_id` body field where the operation has one, and the
  `X-User-Id` header. Clients SHOULD send both; if both are present and
  differ the server MUST return 400.
- gRPC: the `user_id` field where the message has one, otherwise the
  `x-user-id` metadata key. Data-port messages have no `user_id` field, so
  gRPC clients MUST always send `x-user-id`.

== Deleting resources and device events <res-lifecycle-device>
// ref: ADR-020 §4, §10; ADR-023 Fix-F Decision 2; ADR-024 §10; D15b, D15c,
// D26, D56, D58, D66. Forced cleanup with reasons reprogram/clear is 44-F.

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [Delete resource], [`DELETE /resources/{resource_id}`], [`DeviceService.DeleteResource`],
  [Program FPGA], [`POST /devices/{serial}/configure`, `POST /devices/{serial}/configure_from_server_path`], [`DeviceService.ConfigureFPGA`, `DeviceService.ConfigureFPGAFromMemory`],
  [Clear FPGA], [`DELETE /devices/{serial}/configure`], [`DeviceService.ClearFPGAConfiguration`],
  [Reset FPGA], [`POST /devices/{serial}/classic/reset`], [`ClassicDataPortService.ResetFPGA`],
)

- Delete returns `204 No Content`, or 409 if anyone holds the resource. Any
  connected user MAY delete a resource.
- When a device disconnects (or an orphan sweep finds its device absent), all
  its resources MUST be deleted with reason `device_removed` ( emitting
  `ResourceUnlocked` first for any locked shared resource) and their holders
  notified.
- Programming or clearing a device is a forced override, done atomically
  before the FFI hardware call, at every enforcement level: (1) the device
  transitions to `Connected` (unprogrammed) and bumps its configuration
  generation, (2) all locks end (`ResourceUnlocked` emitted for locked shared
  resources), (3) all holders are released, (4) all resources are deleted with
  reason `reprogram` or `clear`, (5) the bitfile is written or the
  configuration cleared. If a bitfile transfer fails mid-way, the device
  remains in `Connected` (unprogrammed) state (`44-F q3a`). Every holder of
  every resource on the device MUST be informed, whatever its state. The
  programming client then registers new resources.
- `ResetFPGA` is *not* a reprogram: resources stay (@enf-reset).

*Rationale.* A new bitfile is a new design: the old endpoint and register
meanings no longer apply, so keeping the old resources would be dangerous.
Any connected user may program today because the server has no role-based
access control yet; when it does, programming will be restricted.

== Events <res-events>
// ref: ADR-016; ADR-030; ADR-034 §5; D58. Event names verified against
// openapi.yaml /events and proto DeviceEvent (0.3.1).

The server publishes every resource change on the REST Server-Sent Events
stream (`GET /events`) and the gRPC stream
(`DeviceService.WatchDeviceEvents`).

#table(
  columns: (1fr, auto, auto),
  [Event], [SSE `event:` name], [gRPC `DeviceEvent` field],
  [Device connected], [`DeviceAdded`], [`device_added` (2)],
  [Device disconnected], [`DeviceRemoved`], [`device_removed` (3)],
  [FPGA programmed], [`DeviceProgrammed`], [`bitfile_programmed` (4)],
  [Resource state changed (acquired, released, locked, unlocked)], [`ResourceChanged`], [`resource_state_changed` (5)],
  [Resource registered], [`ResourceCreated`], [`resource_created` (6)],
  [Resource deleted, with `reason`], [`ResourceDeleted`], [`resource_deleted` (7)],
  [Shared resource locked], [`ResourceLocked`], [`resource_locked` (8)],
  [Shared resource unlocked], [`ResourceUnlocked`], [`resource_unlocked` (9)],
  [User session created / ended], [`UserConnected` / `UserDisconnected`], [`user_connected` (10) / `user_disconnected` (11)],
  [Server shutting down], [`ServerShutdown`], [`server_shutdown` (12)],
)

The delete `reason` is `client`, `reprogram`, `clear` or `device_removed`
(proto `DeleteReason`). The SSE `data:` field is the event object itself, not
wrapped in the event name. The server sends a `:ping` comment every 15
seconds to keep proxies from closing the stream.

#example("SSE resource state change")[
  ```text
  event: ResourceChanged
  data: {"resource_id":"2149000ABC:port-a","new_state":"in_use_locked","timestamp":"2026-10-11T12:00:00Z"}
  ```
]

== Errors <res-errors>
// ref: ADR-034 §4; D38, D72, D73

Resource operations report failures with `ProblemDetail` (@intro-two-transports).
When another resource or user causes the failure, `conflict` names it:

#table(
  columns: (auto, 1fr),
  [`conflict` field], [Meaning],
  [`code`], [`overlap`, `locked`, `blocked`, `mode_mismatch`, `private` or `validation`.],
  [`resource_id`], [The existing resource involved.],
  [`resource_name`], [Its name.],
  [`item_type`], [`endpoint` or `address_range`.],
  [`item`], [For example `0x05` or `0x44A60000-0x44A6FFFF`.],
  [`items`], [Every item that clashes with the named resource: endpoints in ascending order, then that resource's ranges in declaration order. `item` is the first entry.],
)

When more than one item clashes, `detail` ends with "(N conflicting items in
total)". A resource without `endpoints` registered against another one lists
every endpoint it claims.

A duplicate name is a 409 without `conflict`. Malformed JSON and bad enum
values are 422. gRPC maps 409 to `FAILED_PRECONDITION`, 404 to `NOT_FOUND`
and 422 to `INVALID_ARGUMENT`, and carries the same fields as a
`ResourceConflictDetail` (type URL
`type.googleapis.com/okFPGAService.v1.ResourceConflictDetail`) in the
standard `google.rpc.Status` details.

#example("Overlap error")[
  ```json
  {
    "type": "about:blank",
    "title": "Resource conflict",
    "status": 409,
    "detail": "Endpoint 0x05 is already declared exclusive by resource counter-ctrl.",
    "conflict": {
      "code": "overlap",
      "resource_id": "2149000ABC:counter-ctrl",
      "resource_name": "counter-ctrl",
      "item_type": "endpoint",
      "item": "0x05",
      "items": ["0x05"]
    }
  }
  ```
]

*Rationale.* A client that is refused needs to know *who* is in the way to
tell its user something useful ("endpoint 0x05 is held by resource
counter-ctrl") without parsing free text.

== Users and holdings <res-users>
// ref: ADR-034 §6; D58. Discrepancies (ADR-034 open items): REST /users
// returns last_seen_at/count, spec says connected_at; GET /users/{id} and
// gRPC GetUser not implemented.

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [List users], [`GET /users`], [`DeviceService.ListUsers`],
  [Get one user], [`GET /users/{user_id}`], [`DeviceService.GetUser`],
)

#table(
  columns: (1.3fr, 1fr, 0.5fr, 2.2fr),
  [`UserSession` field], [Type], [Proto], [Meaning],
  [`user_id`], [string], [1], [User identity.],
  [`connected_at`], [ISO 8601 string], [2], [Session start.],
  [`held_resource_ids`], [string array], [3], [Ids of held resources.],
  [`holdings`], [array of `Holding`], [4], [Per resource: `resource_id` (1), `mode` (2), `locked` (3, true if this user holds the lock).],
)
