// chapters/example-DualPort.typ — Section 5: worked example.
// Drafted by local-strata (job_0a95d459), reviewed and corrected by
// Antigravity 2026-10-11. Source: developer guide §6 (DualPort-Transceiver,
// axibridge-access-guide-v2.md). Address ranges are ILLUSTRATIVE: Strata
// invented the base addresses; only Core 0 0x44A0_0014 appears in the guide.
// ref: ADR-025 (DualPort mapping stub); D4, D5, D9, D14, D19, D20, D21,
// D24, D29, D30, D35, D44, D46, D75
#import "../lib.typ": *

= Worked Example: Dual-Port DualPort on an XEM8320 <example-DualPort>

This chapter applies the rules of @resource-model and @enforcement to a real
design: a DualPort-Transceiver bitfile on an XEM8320 with two network ports. Each
port has its own cores (Core, Trigger, Gearbox, I²C) and half of the DDR4
memory. Both ports reach their cores through *one* AXI bridge built from
Classic endpoints, and share one Authenticator.

#note[
  The address ranges in this chapter are illustrative. The real DualPort
  address map is defined by the bitfile's documentation.
]

== Setup <DualPort-setup>
// ref: D9, D14, D30, D75 (0xA0 private), D26 (reprogram clears)

+ Program the XEM8320 with the DualPort bitfile. Any user may do this; it
  deletes every existing resource on the device (@res-lifecycle-device).
+ Register two *shared* resources, `port-a` and `port-b`. Order does not
  matter: every overlap is a shared item in a shared resource
  (@res-registration, row 1).

#table(
  columns: (auto, 1fr, 1fr),
  [Resource], [Shared endpoints (declared by both)], [Exclusive ranges (own only)],
  [`port-a`], [AXI bridge: WireIn `0x00`–`0x08`, TriggerIn `0x40`/`0x44`/`0x45`, WireOut `0x20`–`0x23`/`0x31`, PipeIn `0x80`, PipeOut `0xA0`. Authenticator: WireIn `0x17`, TriggerIn `0x4C`, WireOut `0x2A`–`0x2C`, TriggerOut `0x60`–`0x63`.], [Core 0, Trigger 0, Gearbox 0, I²C 0, DDR4 lower half.],
  [`port-b`], [the same], [Core 1, Trigger 1, Gearbox 1, I²C 1, DDR4 upper half.],
)

PipeOut `0xA0` is marked `private` in both resources: a ReadOnly read would
drain bridge data that a holder is waiting for (@enf-readonly-design-rule).
The Authenticator TriggerOuts are also private, so a ReadOnly observer cannot
clear them with `UpdateTriggerOuts` (@enf-readonly). The `private` flag must
be identical in both declarations (@res-registration, row 11).

#example("Declaration of port-a")[
  `port-b` is identical except for its name, `description`, ranges and
  metadata.
  ```json
  {
    "name": "port-a",
    "description": "DualPort port A",
    "device_serial": "2149000ABC",
    "access_mode": "shared",
    "interfaces": ["classic"],
    "endpoints": {
      "default_access": "shared",
      "entries": [
        {"address": "0x00", "type": "wire_in"},  {"address": "0x01", "type": "wire_in"},
        {"address": "0x02", "type": "wire_in"},  {"address": "0x03", "type": "wire_in"},
        {"address": "0x04", "type": "wire_in"},  {"address": "0x05", "type": "wire_in"},
        {"address": "0x06", "type": "wire_in"},  {"address": "0x07", "type": "wire_in"},
        {"address": "0x08", "type": "wire_in"},  {"address": "0x17", "type": "wire_in"},
        {"address": "0x20", "type": "wire_out"}, {"address": "0x21", "type": "wire_out"},
        {"address": "0x22", "type": "wire_out"}, {"address": "0x23", "type": "wire_out"},
        {"address": "0x2A", "type": "wire_out"}, {"address": "0x2B", "type": "wire_out"},
        {"address": "0x2C", "type": "wire_out"}, {"address": "0x31", "type": "wire_out"},
        {"address": "0x40", "type": "trigger_in"}, {"address": "0x44", "type": "trigger_in"},
        {"address": "0x45", "type": "trigger_in"}, {"address": "0x4C", "type": "trigger_in"},
        {"address": "0x60", "type": "trigger_out", "private": true},
        {"address": "0x61", "type": "trigger_out", "private": true},
        {"address": "0x62", "type": "trigger_out", "private": true},
        {"address": "0x63", "type": "trigger_out", "private": true},
        {"address": "0x80", "type": "pipe_in", "description": "bridge burst write data"},
        {"address": "0xA0", "type": "pipe_out", "private": true,
         "description": "bridge burst read data"}
      ]
    },
    "address_ranges": [
      {"name": "core-0",     "start": "0x44A00000", "end": "0x44A0FFFF", "access": "exclusive"},
      {"name": "trigger-0",  "start": "0x44A10000", "end": "0x44A1FFFF", "access": "exclusive"},
      {"name": "gearbox-0",  "start": "0x44A20000", "end": "0x44A2FFFF", "access": "exclusive"},
      {"name": "i2c-0",      "start": "0x44A30000", "end": "0x44A3FFFF", "access": "exclusive"},
      {"name": "ddr4-lower", "start": "0x80000000", "end": "0x8FFFFFFF", "access": "exclusive"}
    ],
    "metadata": { "associated_cores": "Core_0,Trigger_0,Gearbox_0,axi_iic_0" }
  }
  ```
]

== Bridge transactions <DualPort-transactions>
// ref: D30 (only three transactions), D4 (ADR-025 hook maps WI 0x00/0x01)

DualPort uses only three bridge transactions. Each is several calls on
shared endpoints, so a compliant client locks its port before the first call
and unlocks after the last (@res-lock-when).

#table(
  columns: (auto, 1fr),
  [Transaction], [Calls (gRPC names; REST equivalents under `/classic/`)],
  [One-shot write], [`SetWireInValue` `0x00` = address, `SetWireInValue` `0x08` = data, `UpdateWireIns`, `ActivateTriggerIn` `0x45`, `UpdateWireOuts`, check `GetWireOutValue` `0x20` (bresp).],
  [Burst write], [`SetWireInValue` `0x00` = address, `0x02` = control, `UpdateWireIns`, `WriteToPipeIn` `0x80`, `ActivateTriggerIn` `0x40`, `UpdateWireOuts`, check `0x20`.],
  [Burst read], [`SetWireInValue` `0x01` = address, `0x03` = control, `UpdateWireIns`, `ActivateTriggerIn` `0x44` bit 0, `UpdateWireOuts`, poll `0x21` bit 0, `ReadFromPipeOut` `0xA0`.],
)

The server's DualPort address mapping (@enf-address-mapping) resolves the
address written to WireIn `0x00` or `0x01`, and checks it against the ranges
of the caller's resource.

Module status is read from AXI memory, not from WireOuts: *Status Register
2* at module base + `0x08`, through a burst read.

#table(
  columns: (auto, 1fr, 1fr),
  [Bits], [Core module], [Trigger module],
  [7], [Authenticated], [Authenticated],
  [4:1], [current state], [current state],
  [0], [txTriggerArmed], [txTriggerArmed],
  [8], [Core Ready], [—],
  [9 / 10], [—], [Trigger Armed / Triggered],
  [14:12], [txControlState], [—],
  [31:16], [last non-idle address], [—],
)

== Usage scenario <DualPort-scenarios>
// ref: D5, D9, D16, D17, D19, D20, D21, D24, D29, D35, D44, D46, D75

#table(
  columns: (auto, 1fr, 1fr),
  [Step], [Action], [Result and reason],
  [1], [Alice acquires `port-a`.], [200. She is a holder and may write `port-a`'s items.],
  [2], [Alice locks `port-a` and burst-reads Core 0 Status Register 2: bit 7 = 0 (not authenticated).], [Allowed. The burst read is multi-step, so she locks. The address is in `port-a`'s Core 0 range.],
  [3], [Still holding the lock, Alice authenticates (WireIn `0x17`, TriggerIn `0x4C`, …), re-reads bit 7 = 1, then unlocks.], [Allowed. While she holds the lock, `port-b` is blocked for everyone else.],
  [4], [Alice locks `port-a`, does a one-shot write to Core 0 (`0x44A0_0014`), unlocks.], [Allowed. The server would accept the writes without the lock, but the transaction is multi-step, so a compliant client locks.],
  [5], [Bob acquires `port-b`.], [200, even while `port-a` is locked: acquiring is never blocked.],
  [6], [Bob locks `port-b`.], [200 if `port-a` is unlocked. If Alice holds her lock: 409 (`blocked`). Bob waits for `ResourceUnlocked` and retries.],
  [7], [Bob burst-reads Core 1 Status Register 2: bit 7 = 1, Core Ready = 1.], [Allowed. The FPGA is already authenticated, so Bob skips authentication, uses his own ranges, then unlocks.],
  [8], [Alice (holding only `port-a`) writes a Core 1 address to WireIn `0x00`.], [403: the mapped address is not in any of `port-a`'s ranges, locked or not.],
  [9], [Bob writes WireIn `0x00` while Alice holds `port-a`'s lock.], [403: `port-b` is blocked for Bob.],
  [10], [Bob reads WireOut `0x21` or PipeOut `0xA0` while Alice holds the lock.], [Allowed by the server (shared holders may read, private or not). A compliant client never does this outside its own locked transaction: reading `0xA0` would steal Alice's burst-read data.],
  [11], [Carol acquires `port-a` as ReadOnly.], [200 (the resource is not private). She may read the non-private WireOuts. She cannot use the bridge: she may not write its WireIns or TriggerIns, and mapped access is denied to ReadOnly. She cannot read PipeOut `0xA0` or the private TriggerOuts, and `UpdateTriggerOuts` is denied to her because the device declares private TriggerOuts.],
  [12], [Bob writes WireIn `0x07` (FIFO reset) while nobody holds a lock.], [Allowed: Bob holds a usable resource that declares it. Not disturbing other users is the clients' job.],
  [13], [Dave also acquires `port-a` (shared). Alice and Dave both write Core 0 without locks.], [Allowed: both are holders. Their clients must coordinate with the lock, since bridge transactions are multi-step.],
  [14], [Someone reprograms the FPGA.], [All locks end, all holders are released, all resources are deleted (reason `reprogram`) and every holder is informed. Everyone starts again at setup.],
)

#note[
  *One user, both ports.* Alice may hold both `port-a` and `port-b`. Her own
  lock never blocks her. For a bridge transaction into either port's ranges,
  locking *either* port keeps other users out, because a lock on one port
  blocks the other for everyone else. Interleaving her *own* transactions is
  her client's concern; the server does not protect a user from herself.
]

== Why a lock blocks the other port <DualPort-lock-rationale>

Both ports drive one physical bridge. Each transaction is five to seven calls
on shared endpoints, and interleaving two transactions corrupts both.
Blocking `port-b` while `port-a` is locked serialises the bridge, which is
exactly what the hardware needs. The lock covers only one transaction, so the
other port waits only briefly. Future server-side transaction support could
make each bridge transaction a single call.
