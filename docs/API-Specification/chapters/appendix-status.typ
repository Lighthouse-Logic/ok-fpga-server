// chapters/appendix-status.typ — Appendix A: implementation status.
// ref: ADR-034 header and Open items; roadmap Phase 44 sub-phases:
//   overlap rules / Rule A / interface check / private storage → 44-B
//   acquire mode rules (S-03, A1)                                → 44-C
//   blocking B1–B4, gRPC WatchDeviceEvents fan-out               → 44-D
//   coverage D61/D77, enforcement on all routes, default enforce,
//   ReadOnly F1/F2, warn_readonly_side_effects (F4)              → 44-E
//   forced cleanup on configure/clear, reasons reprogram/clear   → 44-F
//   example clients                                              → 44-G
// Update this appendix with every sub-phase commit.
#import "../lib.typ": *

= Implementation Status (server 0.3.8) <appendix-status>

This document specifies the target behaviour. Server version 0.3.8
implements part of it. Clients SHOULD be written against the specification;
the items below show where today's server still differs.

#table(
  columns: (1fr, auto),
  [Behaviour], [Status in 0.3.8],
  [Declaration schema, holder objects, state strings, structured conflicts (@res-declare, @res-errors)], [implemented],
  [Opal Kelly snapshot reads, `update` flag, one endpoint per read, mask default 0 (@enf-opalkelly)], [implemented],
  [`ResourceLocked` / `ResourceUnlocked` events; `holdings` in user listings], [implemented],
  [Full registration rules: shared-overlap rule, exclusive-only rule, interface / data port check, storage of `private` flags (@res-registration)], [implemented],
  [Strict endpoint address format (`0x` + two hex digits) and rejection of duplicate entries], [implemented],
  [Conflict `items` list (@res-errors)], [implemented],
  [Acquire mode rules: holder-mode tracking, idempotent repeat acquire, `mode_mismatch` and `private` conflicts, ReadOnly + Exclusive transitions (@res-acquire)], [implemented],
  [Blocking of overlapping resources (@res-blocking)], [implemented],
  [gRPC event stream fan-out (`WatchDeviceEvents`)], [implemented],
  [Coverage of omitted `endpoints` limited to `0x00`–`0x7F` (@res-endpoint-shapes)], [implemented],
  [Enforcement on every data-port operation, `enforce` as default (@enf-levels)], [implemented],
  [ReadOnly denial of resets and the private-TriggerOut rule (@enf-readonly)], [implemented],
  [`warn_readonly_side_effects` setting (@enf-readonly-warning)], [implemented],
  [Validation of `ActivateTriggerIn` bit numbers (0–31)], [implemented],
  [Forced cleanup when programming, clearing or unplugging, with delete reasons `reprogram` / `clear` / `device_removed` (@res-lifecycle-device)], [implemented],
  [Classic data-path REST alignment: `application/octet-stream` pipes and block pipes, bulk register reads/writes, `WireInRequest.auto_update` (@ov-classic)], [implemented],
  [`GET /users/{user_id}`, gRPC `GetUser`; REST `connected_at` field (@res-users)], [planned; REST returns `last_seen_at`],
  [gRPC AXI methods (@ov-axi)], [planned; use REST],
)
