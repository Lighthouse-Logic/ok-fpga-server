// chapters/enforcement.typ — Section 4: Access Enforcement.
// Drafted by local-strata (job_0a95d459), reviewed and corrected by
// Antigravity 2026-10-11 (Opal Kelly quotes re-verified against the live
// FPGADataPortClassic page and okFrontPanel.h 1093–1103).
// ref: ADR-024 (levels, tiers, pipeline), ADR-025 (address mapping, §3 lock
// rule superseded by D44), ADR-026 (ReadOnly/private), ADR-034 §8–§9;
// D5, D9, D12, D15e, D16, D19, D21, D27, D29, D42, D44, D46, D51, D56,
// D66, D67, D75, D76, D77, D78.
#import "../lib.typ": *

= Access Enforcement <enforcement>

This chapter specifies how the server decides, for every data-port command,
whether the caller may perform it. It builds on the resource model
(@resource-model).

== Enforcement levels <enf-levels>
// ref: ADR-024 §1, §9 (hot reload); D15e, D27
// Implementation status: repo config.toml and the code default are still
// "off"; default "enforce" lands in 44-E.

```toml
[access_control]
endpoint_enforcement = "enforce"    # "off" | "warn" | "enforce"
warn_readonly_side_effects = true   # see the ReadOnly section
```

#table(
  columns: (auto, 1fr, auto),
  [Level], [Behaviour], [User id required],
  [`enforce`], [Default. Denied commands fail.], [yes],
  [`warn`], [Debugging aid: violations (including a missing user id) are logged at warning level, but the command runs.], [expected],
  [`off`], [Debugging aid: no checks.], [no],
)

All clients MUST work correctly under `enforce`. `off` and `warn` exist only
to find clients that do not follow the rules. Denials are `403` (REST) /
`PERMISSION_DENIED` (gRPC). Conflicts with existing state are `409` /
`FAILED_PRECONDITION`; requests that are invalid on their own are `422` /
`INVALID_ARGUMENT`.

== Route tiers <enf-tiers>
// ref: ADR-024 §4

#table(
  columns: (auto, 1fr, 1fr),
  [Tier], [Routes], [Checks under `warn` / `enforce`],
  [Management], [`/server/*`, `/devices`, `/resources/*`, `/users/*`, `/events`, `/disconnect`, `/heartbeat`], [User id.],
  [Device], [`/devices/{serial}`, `/configure`, `/settings/*`, `/sensors`, `/syzygy`], [User id.],
  [Data port], [`/devices/{serial}/classic/**`, `/devices/{serial}/axi/**`], [User id; caller holds a resource on the device; the permission rules below.],
)

gRPC applies the same checks inside each data-port method.

== Reads and writes <enf-read-write>
// ref: D16, D21, D42, D75 (F1)

Every data-port operation is either a *read* or a *write*. The server does
not try to judge whether an operation changes hardware state.

#table(
  columns: (auto, 1fr),
  [Class], [Operations],
  [Read], [`UpdateWireOuts`, `GetWireOutValue`, `UpdateTriggerOuts`, `IsTriggered`, `GetTriggerOutVector`, PipeOut and block-throttled PipeOut reads, `ReadRegister(s)`, AXI reads.],
  [Write], [`SetWireInValue`, `UpdateWireIns`, `ActivateTriggerIn`, PipeIn and block-throttled PipeIn writes, `WriteRegister(s)`, AXI writes, `ResetFPGA`, AXI Lite / Full / Stream resets.],
)

*Rationale.* `UpdateWireOuts` and `UpdateTriggerOuts` only move data from the
FPGA to the host, so they are reads, even though `UpdateTriggerOuts` clears
latches (@enf-opalkelly). Resets change user-logic state, so they are writes.
Classifying by direction keeps the rules predictable; designs that need more
protection use exclusive resources and `private` items (@enf-read-harm).

== Opal Kelly read semantics <enf-opalkelly>
// ref: ADR-034 §9; D42, D76, D78; okFrontPanel.h 1093–1103

=== The shared snapshot <enf-snapshot>
The server opens one FrontPanel handle per device, so all clients share one
WireOut snapshot and one TriggerOut snapshot. Reads behave exactly as the
FrontPanel API defines them:

- `UpdateWireOuts` and `UpdateTriggerOuts` are the only reads that talk to
  the FPGA. Opal Kelly: `UpdateWireOuts` "is called to request the current
  state of all WireOut values from the XEM. All wire outs are captured and
  read at the same time"; `UpdateTriggerOuts` "is called to query the XEM to
  determine if any TriggerOuts have been activated since the last
  call"#footnote[Opal Kelly, _FrontPanel API reference_, class
  `OpalKelly::FPGADataPortClassic`, methods `UpdateWireOuts()` and
  `UpdateTriggerOuts()`; #ok-api\.]\.
- `GetWireOutValue`, `IsTriggered` and `GetTriggerOutVector` only read the
  snapshot left by the most recent Update call, *whoever made it*. Opal Kelly
  describes `GetWireOutValue` as getting "the value of a particular Wire Out
  from the internal wire data structure", and `IsTriggered` as finding out "if
  a particular bit (or bits) on a particular TriggerOut endpoint has triggered
  since the last call to UpdateTriggerOuts()"#footnote[Ibid., methods
  `GetWireOutValue()`, `IsTriggered()` and `GetTriggerOutVector()`.].
- `UpdateTriggerOuts` takes only the device handle (C API
  `okFPGADataPortClassic_UpdateTriggerOuts(hnd)`)#footnote[`okFrontPanel.h`
  (FrontPanel 6.1.0), declarations of `okFPGADataPortClassic_UpdateWireOuts`,
  `_UpdateTriggerOuts`, `_IsTriggered` and `_GetTriggerOutVector`.]. It
  therefore reads and clears the latches of *every* TriggerOut on the device
  at once, and the result lands in the one shared snapshot.

=== The `update` flag <enf-update-flag>

#table(
  columns: (1fr, 1.4fr, 1.4fr),
  [Read], [REST], [gRPC],
  [`GetWireOutValue`], [`GET …/classic/wires/out?endpoint=…&update=true`], [`GetWireValueRequest.update` (field 3; ignored by `GetWireInValue`)],
  [`IsTriggered` + `GetTriggerOutVector`], [`GET …/classic/triggers/out?endpoint=…&mask=…&update=true`], [`IsTriggeredRequest.update` (field 4), `TriggerOutVectorRequest.update` (field 3)],
)

- `update` defaults to `false`: read the current snapshot only.
- With `update=true` the server MUST run the Update call and the read
  atomically under the device lock, so no other client's Update can come in
  between.
- The standalone update operations remain: `POST …/classic/wires/in/update`,
  `…/wires/out/update`, `…/triggers/out/update` (gRPC `UpdateWireIns`,
  `UpdateWireOuts`, `UpdateTriggerOuts`).

*Rationale.* Earlier server versions silently ran an Update before every
read. That cleared trigger latches behind the client's back and made the
server behave differently from every FrontPanel program. Following the
FrontPanel model exactly means existing FrontPanel knowledge applies
unchanged; the `update` flag adds the one thing a network server must add:
an Update and a read that no other client can split.

=== One endpoint per read <enf-one-endpoint>
Every read names exactly one endpoint. There are no whole-bank reads; a
missing `endpoint` MUST be rejected with 422. In REST the `endpoint` and
`mask` query parameters are integers (`endpoint=32` for `0x20`).

*Rationale.* Access rules are per endpoint. A whole-bank read would return
values of endpoints the caller may not be entitled to see (for example
private items).

=== The `IsTriggered` mask <enf-mask>
The mask defaults to 0. With no mask, `triggered` is always `false`; the
client MUST set exactly the bits it cares about. The full 32-bit vector is
always returned as well (REST `vector`, gRPC `GetTriggerOutVector`).

*Rationale.* An all-ones default would make a careless client look correct
while also reporting triggers on bits it does not own. Opal Kelly's own
example passes an explicit one-bit mask: `IsTriggered(0x71, 0x80)` checks
bit 7 of endpoint `0x71`.

#example("Read one WireOut with refresh")[
  ```text
  GET /api/v1/devices/2149000ABC/classic/wires/out?endpoint=32&update=true
  X-User-Id: alice

  200 {"endpoint": 32, "value": 4294901760}
  ```
  gRPC: `ClassicDataPortService.GetWireOutValue(GetWireValueRequest {
  serial: "2149000ABC", endpoint: 0x20, update: true })`.
]

#example("Test bit 1 of TriggerOut 0x61 with refresh")[
  ```text
  GET /api/v1/devices/2149000ABC/classic/triggers/out?endpoint=97&mask=2&update=true
  X-User-Id: bob

  200 {"endpoint": 97, "triggered": true, "vector": 3}
  ```
  `vector` = 3 shows bits 0 and 1 set; `triggered` tests only bit 1.
]

=== Lost triggers between clients <enf-lost-triggers>
// ref: D67 (reopened, Later), D76 Q-G

#caution[
  A client cannot tell whether other clients ran `UpdateTriggerOuts` (or
  `UpdateWireOuts`) since its own last read, or how many times. One client's
  `UpdateTriggerOuts` can consume a trigger another client was waiting for.
  This is standard FrontPanel behaviour with one shared handle.
]

#example("Alice consumes Bob's trigger")[
  An FPGA has two network ports. Alice uses `port-a` (TriggerOut `0x60`), Bob
  uses `port-b` (TriggerOut `0x61`). Bob's packet-arrived trigger fires on
  `0x61`. Before Bob polls, Alice calls `UpdateTriggerOuts` to check `0x60`.
  The FPGA latch for `0x61` is now cleared, and the snapshot holds Bob's
  trigger only until the next Update by anyone. If Bob now reads `0x61` with
  `update=true`, he sees `false`: his trigger is lost. With `update=false` he
  sees `true` only if nobody else updated since Alice did, and he cannot know
  whether that is the case.
]

Until a server-side trigger event log exists (planned), designs that cannot
accept this SHOULD do one of the following:

- put the TriggerOuts in an exclusive resource and mark them `private`;
- have all trigger users read triggers only under the lock
  (@res-lock-when);
- signal events through WireOut status bits that the user logic clears on a
  WireIn write, instead of TriggerOuts.

The server MUST NOT keep a per-holder copy of trigger state.

== Write permissions <enf-write-matrix>
// ref: D5, D19, D44, D46, D51, D75

Let U be the caller and R a resource U holds that covers the item. R is
*usable* if it is not locked by another user and not blocked for U
(@res-blocking). A write is allowed if *at least one* such R allows it.

#table(
  columns: (auto, 1fr, 1fr, 1fr, 1fr, 1fr),
  [U's hold on R], [Shared EP], [Exclusive EP], [Shared range], [Exclusive range], [Item not declared by R],
  [none], [deny], [deny], [deny], [deny], [deny],
  [shared holder, R usable], [allow], [allow], [allow], [allow], [deny],
  [shared holder, R locked by another user], [deny], [deny], [deny], [deny], [deny],
  [shared holder, R blocked], [deny], [deny], [deny], [deny], [deny],
  [exclusive holder], [—], [allow], [—], [allow], [deny],
  [ReadOnly holder], [deny], [deny], [deny], [deny], [deny],
)

("—": an exclusive resource cannot contain shared items.)

- The only runtime difference between shared and exclusive items: a shared
  item may also be writable through another usable resource that declares
  it; an exclusive item only through its one resource.
- No lock is needed to write; the server never asks "does U hold the lock?".
- `private` has no effect on writes.

== Read permissions and ReadOnly holders <enf-read-matrix>
// ref: ADR-026; ADR-034 §8; D12, D16, D21, D29, D42, D75 (F1, F2)
// Implementation status: F1/F2 enforcement is 44-E.

#table(
  columns: (auto, 1fr, 1fr, 1fr),
  [U's hold on R], [Read, non-private], [Read, private], [Access through an address mapping],
  [none], [deny], [deny], [deny],
  [shared or exclusive holder, any lock state], [allow], [allow], [follows the write rules],
  [ReadOnly holder], [allow (trigger rule below)], [deny], [deny],
)

=== What a ReadOnly holder may do <enf-readonly>
A ReadOnly holder MAY read every covered item that is not private:
WireOuts and `UpdateWireOuts`; TriggerOuts (`IsTriggered`,
`GetTriggerOutVector`, `UpdateTriggerOuts`); PipeOut and block-throttled
PipeOut; `ReadRegister(s)`; AXI reads.

A ReadOnly holder MUST NOT: write anything (WireIn / `UpdateWireIns`,
`ActivateTriggerIn`, PipeIn, register or AXI writes); call `ResetFPGA` or an
AXI reset; lock; read private items; or use a developer-defined address
mapping (reaching a mapped address needs WireIn / TriggerIn writes).

*Trigger rule.* `UpdateTriggerOuts`, and any trigger read with
`update=true`, MUST be denied to a ReadOnly caller when *any* resource on the
device declares a private TriggerOut. Snapshot reads (`update=false`) are
checked per endpoint: a private endpoint is denied.

*Rationale.* `UpdateTriggerOuts` cannot be limited to one endpoint
(@enf-snapshot): it clears every latch on the device, including private ones.
Allowing it would let an observer destroy data the architect explicitly
protected.

=== Design rule for architects <enf-readonly-design-rule>
// ref: D75 design rule (documented, not enforced); D21

A ReadOnly action SHOULD NOT change user-logic state or consume data other
holders rely on. The server allows ReadOnly reads of every non-private item,
so the HDL architect SHOULD mark `private` every item where a read has a side
effect. In Opal Kelly HDL#footnote[Opal Kelly, _FrontPanel HDL: Endpoints_
(`okWireOut`, `okTriggerOut`, `okPipeOut`, `okBTPipeOut`,
`okRegisterBridge` port descriptions),
#link("https://docs.opalkelly.com/fpsdk/frontpanel-hdl/")[docs.opalkelly.com/fpsdk/frontpanel-hdl]\.]\:

#table(
  columns: (auto, 1fr, auto),
  [Endpoint], [Effect of a host read on the FPGA], [Recommendation],
  [`okWireOut`], [None: there is no read strobe; user logic cannot see the read.], [Safe to leave non-private.],
  [`okTriggerOut`], [`UpdateTriggerOuts` clears the latched trigger bits of the whole device.], [Mark private if triggers matter.],
  [`okPipeOut`, `okBTPipeOut`], [Each word read pulses `ep_read`, typically draining a FIFO.], [Mark private unless loss of data is acceptable.],
  [`okRegisterBridge`], [A read pulses `ep_read`; some designs use it for clear-on-read registers.], [Mark the range private if any register is clear-on-read.],
)

*Rationale.* The client is not an end user but a developer who knows the HDL.
The server does not forbid risky observer reads outright, because some
designs want them (for example a monitor that drains a diagnostics FIFO). It
recommends, warns and lets the architect decide.

=== The side-effect warning <enf-readonly-warning>
// ref: D75 F4; ADR-034 §8. Implementation status: setting lands in 44-E.

#table(
  columns: (1.4fr, 0.7fr, 0.6fr, 2fr),
  [Setting], [Type], [Default], [Effect],
  [`warn_readonly_side_effects`], [boolean], [`true`], [At registration, log a warning when ReadOnly holders could reach a non-private PipeOut, block-throttled PipeOut or TriggerOut, or a non-private address range.],
  [env `OKFPGA_WARN_READONLY_SIDE_EFFECTS`], [boolean], [—], [Overrides the file value.],
)

== When other users' reads could hurt you <enf-read-harm>
// ref: D21

The server allows reads by every holder, including reads that consume data
(a PipeOut read drains a FIFO) and reads during someone else's lock. If a
design would be harmed by another holder's read, the architect MUST declare
it accordingly: put the item in an exclusive resource, and also mark it
`private` if ReadOnly observers must not read it either.

#example("Capture FIFO")[
  A capture engine streams results into a FIFO read through PipeOut `0xA3`. A
  second holder reading `0xA3` would steal samples.
  ```json
  {
    "name": "capture",
    "device_serial": "2149000ABC",
    "access_mode": "exclusive",
    "interfaces": ["classic"],
    "endpoints": { "entries": [
      { "address": "0xA3", "type": "pipe_out", "access": "exclusive", "private": true }
    ] }
  }
  ```
  A separate shared `monitor` resource can still expose the status WireOuts
  to everyone.
]

== Address ranges and address mapping <enf-address-ranges>
// ref: ADR-025 §1–§3, §6 (hook trait ResourceAccessPolicy, compiled Rust);
// D4, D9, D12, D29, D44, D51, D52

Address ranges protect three kinds of access:

#table(
  columns: (auto, 1fr),
  [Access], [Address checked],
  [okRegisterBridge (`ReadRegister(s)`, `WriteRegister(s)`)], [The register address.],
  [Direct AXI (`/axi/lite`, `/axi/full`, `/axi/stream`)], [The AXI address.],
  [Developer-defined mapping (Classic writes)], [The address the mapping computes.],
)

The address MUST lie in a range of *the same usable resource* that allowed
the command. A resource with no ranges can reach no address.

=== Developer-defined address mapping <enf-address-mapping>
Some designs use Classic endpoints as a bridge into a memory space; for
example a WireIn holds an AXI address and a TriggerIn starts the access. For
such designs the server can be built with a mapping hook that turns a
Classic write into the address it will reach. The hook is compiled into the
server; changing it requires rebuilding the server.

#table(
  columns: (auto, 1fr, 1fr),
  [Hook result], [Meaning], [Check],
  [`NoMapping`], [The write does not reach an address.], [None beyond the endpoint rules.],
  [`AllowAlways`], [The write may reach any address.], [Skips only the address check.],
  [`MapsToAddress(a)`], [The write reaches address `a`.], [`a` must be in a range of the same usable resource.],
)

`AllowAlways` MUST NOT skip the identity, holding, ReadOnly, lock or
blocking checks.

== Enforcement pipeline <enf-pipeline>
// ref: ADR-024 §2, §5; D5, D9, D16, D19, D21, D42, D44, D46, D51, D56,
// D66, D75

For one data-port command the server performs:

+ The caller is identified.
+ The caller holds at least one resource on the device.
+ Candidates = held resources that cover the endpoint or address. None →
  deny.
+ *Read:* allow, unless the only candidates are ReadOnly holds and the item
  is private. `UpdateTriggerOuts` or a trigger read with `update=true` by a
  caller whose candidates are all ReadOnly: deny if any resource on the
  device declares a private TriggerOut.\
  *Write:* drop candidates held as ReadOnly, locked by another user, or
  blocked. None left → deny.
+ Classic write with an address mapping: `NoMapping` / `AllowAlways` →
  allow; `MapsToAddress(a)` → allow only if `a` is in a range of a remaining
  candidate.
+ okRegisterBridge or direct AXI access: allow only if the address is in a
  range of a remaining candidate.
+ `ResetFPGA` and AXI resets have no item: steps 1–2, and the caller must
  hold at least one resource on the device not as ReadOnly.

The check runs once per command; a pipe or AXI transfer then runs without
per-byte checks. The server serialises FrontPanel calls per device, which
protects single calls only. Multi-step sequences need the client's lock
(@res-lock-when).

== ResetFPGA and AXI resets <enf-reset>
// ref: D56, D66, D75 F1

#table(
  columns: (auto, 1fr, 1fr),
  [Operation], [REST], [gRPC],
  [Reset FPGA], [`POST /devices/{serial}/classic/reset`], [`ClassicDataPortService.ResetFPGA`],
  [Reset AXI Lite], [`POST /devices/{serial}/axi/lite/reset`], [`AXIDataPortService.AXILiteReset`],
  [Reset AXI Full], [`POST /devices/{serial}/axi/full/reset`], [`AXIDataPortService.AXIFullReset`],
  [Reset AXI Stream], [`POST /devices/{serial}/axi/stream/reset`], [`AXIDataPortService.AXIStreamReset`],
)

All return `StatusResponse` (`success`, `message`). A reset is not a
reprogram: resources stay registered. The caller MUST be identified and MUST
hold at least one resource on the device other than as ReadOnly.

*Rationale.* A reset changes the state of all user logic, which affects every
holder, so it is a write. It is not tied to one item, so it cannot be checked
against an endpoint; requiring a non-ReadOnly hold on the device is the
closest meaningful rule until role-based access control exists.
