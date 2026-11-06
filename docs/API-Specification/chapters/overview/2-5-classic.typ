// chapters/overview/2-5-classic.typ — Section 2.5: Classic data port.
// ref: ADR-004, ADR-005, ADR-015 (HTTP methods), ADR-019 (FFI endpoint wiring),
// ADR-024 (enforcement), ADR-025 (address mapping), ADR-034 §8–§9 (D42, D66, D75, D76, D78);
// Phase 44-H (REST octet-stream pipes & bulk registers, WireInRequest.auto_update).
#import "../../lib.typ": *

== Classic data port <ov-classic>

All Classic data-port operations map onto Opal Kelly's
`OpalKelly::FPGADataPortClassic` class (`okFPGADataPortClassic_*` in
`okFrontPanel.h`)#footnote[#ok-api\.]\. Every operation is subject to the
access-enforcement pipeline in @enforcement: under `endpoint_enforcement =
"enforce"`, the caller MUST be identified and MUST hold a resource on `<serial>`
that covers the target endpoint or register address with sufficient read or
write permission (@enf-write-matrix, @enf-read-matrix).

=== WireIn operations <ov-classic-wirein>
// ref: ADR-015 (PUT idempotency); ADR-034 D42; openapi.yaml /classic/wires/in*;
// proto SetWireInValue, GetWireInValue, UpdateWireIns.

#table(
  columns: (auto, 1.5fr, 1.4fr, 1.4fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Set WireIn], [`PUT /devices/{serial}/classic/wires/in`], [`SetWireInValue(SetWireInValueRequest)`], [Write → `StatusResponse`],
  [Update WireIns], [`POST /devices/{serial}/classic/wires/in/update`], [`UpdateWireIns(DeviceRequest)`], [Write → `StatusResponse`],
  [Get staged WireIn], [— (gRPC only)], [`GetWireInValue(GetWireValueRequest)`], [Read (host cache) → `WireValueResponse`],
)

Request fields for `WireInRequest` / `SetWireInValueRequest`:
- `endpoint` (integer / proto `int32` 2, required): WireIn address in
  `0x00`–`0x1F` (`0`–`31`). Addresses outside `0x00`–`0x1F` return `400` /
  `INVALID_ARGUMENT`.
- `value` (integer / proto `uint32` 3, required): 32-bit value to stage.
- `mask` (integer / proto `uint32` 4, optional): bitmask selecting which bits of
   the staged WireIn word are modified. On REST `mask` defaults to
  `0xFFFF_FFFF` (`4294967295`); on gRPC callers SHOULD pass `0xFFFF_FFFF` when
  updating all 32 bits (a proto `0` mask stages through FrontPanel with mask
  `0xFFFF_FFFF` when omitted by convention, or the explicit mask supplied).
- `auto_update` (boolean / proto `bool` 5, optional): on REST defaults to
  `true`. When `true`, the server calls `okFrontPanel_SetWireInValue` followed
  immediately by `okFrontPanel_UpdateWireIns` under the same per-device FFI
  lock. When `false`, the value is staged in the host buffer only and sent to
  the FPGA by a later `UpdateWireIns` call.

*Note on `GetWireInValue` (gRPC).* FrontPanel's `okWireIn` HDL module has no
read-back path from the FPGA fabric; `GetWireInValue` returns the value most
recently staged in the host-side `okCFrontPanel` buffer
(`GetWireValueRequest.update` is ignored).

=== WireOut operations <ov-classic-wireout>
// ref: ADR-034 §9; D42, D75, D76; @enf-opalkelly.

#table(
  columns: (auto, 1.6fr, 1.4fr, 1.3fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Read one WireOut], [`GET /devices/{serial}/classic/wires/out?endpoint=N&update=B`], [`GetWireOutValue(GetWireValueRequest)`], [Read → `WireOutResponse` / `WireValueResponse`],
  [Refresh WireOuts], [`POST /devices/{serial}/classic/wires/out/update`], [`UpdateWireOuts(DeviceRequest)`], [Read → `StatusResponse`],
)

Parameters and rules (@enf-opalkelly):
- `endpoint` (required, `0x20`–`0x3F` / `32`–`63`): single WireOut address to
  read. Omitting `endpoint` on REST returns `422 Unprocessable Entity` (there
  are no whole-bank reads, @enf-one-endpoint). An out-of-range address returns
  `400` / `INVALID_ARGUMENT`.
- `update` (optional boolean, default `false`): when `false`, reads the value
  from the shared device snapshot captured by the most recent `UpdateWireOuts`
  call (by any client). When `true`, the server executes `UpdateWireOuts` and
  `GetWireOutValue` atomically under the per-device FFI lock.
- Both operations are classified as *reads*: `ReadOnly` holders MAY call
  `UpdateWireOuts` and MAY read any covered WireOut that is not marked
  `private` (@enf-read-matrix). Sampling `okWireOut` has no read strobe and is
  invisible to FPGA user logic.

=== TriggerIn operations <ov-classic-triggerin>
// ref: ADR-034 §13 (bit 0–31 validation); openapi.yaml /classic/triggers/in; proto ActivateTriggerIn.

#table(
  columns: (auto, 1.5fr, 1.5fr, 1.3fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Fire TriggerIn bit], [`POST /devices/{serial}/classic/triggers/in`], [`ActivateTriggerIn(ActivateTriggerInRequest)`], [Write → `StatusResponse`],
)

Request fields (`TriggerInRequest` / `ActivateTriggerInRequest`):
- `endpoint` (required, `0x40`–`0x5F` / `64`–`95`): TriggerIn endpoint address;
- `bit` (required, `0`–`31`): bit index to pulse for one `ep_clk` cycle in the
  FPGA. A `bit` outside `0..=31` or an `endpoint` outside `0x40..=0x5F` MUST be
  rejected with `400 Bad Request` / `INVALID_ARGUMENT`.

=== TriggerOut operations <ov-classic-triggerout>
// ref: ADR-034 §9; D42, D75 (F2), D76, D78; @enf-opalkelly; @enf-readonly.

#table(
  columns: (auto, 1.6fr, 1.4fr, 1.3fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Read one TriggerOut], [`GET /devices/{serial}/classic/triggers/out?endpoint=N&mask=M&update=B`], [`IsTriggered(IsTriggeredRequest)` & `GetTriggerOutVector(TriggerOutVectorRequest)`], [Read → `TriggerOutResponse` / `IsTriggeredResponse` / `TriggerOutVectorResponse`],
  [Refresh TriggerOuts], [`POST /devices/{serial}/classic/triggers/out/update`], [`UpdateTriggerOuts(DeviceRequest)`], [Read → `StatusResponse`],
)

Parameters and rules (@enf-opalkelly, @enf-readonly):
- `endpoint` (required, `0x60`–`0x7F` / `96`–`127`): single TriggerOut address.
  Omitting `endpoint` on REST returns `422` (@enf-one-endpoint); an address
  outside `0x60`–`0x7F` returns `400` / `INVALID_ARGUMENT`.
- `mask` (optional 32-bit unsigned integer, *default `0`*): bitmask passed to
  `IsTriggered(endpoint, mask)`. Because the default is `0`, `triggered` is
  always `false` unless the caller specifies the bits to test (@enf-mask). On
  REST, `TriggerOutResponse` always returns both `triggered` (boolean) and the
  full 32-bit `vector` from the snapshot; on gRPC, `IsTriggered` returns
  `triggered` and `GetTriggerOutVector` returns `vector`.
- `update` (optional boolean, default `false`): when `true`, runs
  `UpdateTriggerOuts` immediately before reading the snapshot under the same
  per-device FFI lock.
- *Whole-device latch clearing and `ReadOnly` restriction:*
  `UpdateTriggerOuts` reads and *clears* every TriggerOut latch on the FPGA at
  once (@enf-snapshot, @enf-lost-triggers). A `ReadOnly` holder MAY read a
  non-private TriggerOut from the snapshot (`update=false`), but
  `UpdateTriggerOuts` and any trigger read with `update=true` MUST be denied
  (`403` / `PERMISSION_DENIED`) to a `ReadOnly` caller whenever *any* resource
  on the device declares a `private` TriggerOut (@enf-readonly).

=== PipeIn and PipeOut operations <ov-classic-pipes>
// ref: ADR-034 Phase 44-H (REST application/octet-stream); openapi.yaml /classic/pipes/in, /out;
// proto WriteToPipeIn, ReadFromPipeOut.

#table(
  columns: (auto, 1.6fr, 1.4fr, 1.3fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Write PipeIn], [`POST /devices/{serial}/classic/pipes/in?endpoint=N` (`application/octet-stream`)], [`WriteToPipeIn(WriteToPipeInRequest)`], [Write → `PipeTransferResponse` (`bytes_transferred`)],
  [Read PipeOut], [`GET /devices/{serial}/classic/pipes/out?endpoint=N&length=L`], [`ReadFromPipeOut(ReadFromPipeOutRequest)`], [Read → raw bytes (`application/octet-stream`) / `PipeDataResponse`],
)

Parameters and rules:
- `endpoint`: `0x80`–`0x9F` (`128`–`159`) for PipeIn (`type: "pipe_in"`), or
  `0xA0`–`0xBF` (`160`–`191`) for PipeOut (`type: "pipe_out"`). Pipe endpoints
  are never claimed by omitting `endpoints` in `CreateResourceRequest`; they
  MUST be declared explicitly (@res-endpoint-shapes).
- `length` (PipeOut read, required): number of bytes to read (`length >= 0`).
- On REST, `POST .../pipes/in` accepts raw binary bytes in the request body
  (`Content-Type: application/octet-stream`) and returns JSON
  `{ "bytes_transferred": N }`; `GET .../pipes/out` returns raw binary bytes
  (`Content-Type: application/octet-stream`).

=== Block-throttled pipe operations <ov-classic-blockpipes>
// ref: openapi.yaml /classic/pipes/block-in, /block-out;
// proto WriteToBlockPipeIn, ReadFromBlockPipeOut, SetBTPipePollingInterval.

#table(
  columns: (auto, 1.6fr, 1.4fr, 1.3fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Write BTPipeIn], [`POST /devices/{serial}/classic/pipes/block-in?endpoint=N&block_size=B` (`application/octet-stream`)], [`WriteToBlockPipeIn(WriteToBlockPipeInRequest)`], [Write → `PipeTransferResponse`],
  [Read BTPipeOut], [`GET /devices/{serial}/classic/pipes/block-out?endpoint=N&block_size=B&length=L`], [`ReadFromBlockPipeOut(ReadFromBlockPipeOutRequest)`], [Read → raw bytes (`application/octet-stream`) / `PipeDataResponse`],
  [Set polling interval], [— (gRPC only)], [`SetBTPipePollingInterval(SetBTPipePollingIntervalRequest)`], [Device setting → `StatusResponse`],
)

Parameters and rules:
- `endpoint`: `0x80`–`0x9F` (`type: "bt_pipe_in"`) for BlockPipeIn, or
  `0xA0`–`0xBF` (`type: "bt_pipe_out"`) for BlockPipeOut. A resource MUST
  declare the endpoint with `bt_pipe_in` / `bt_pipe_out` matching the HDL
  module instantiated in the bitfile (`okBTPipeIn` / `okBTPipeOut`).
- `block_size` (required): block size in bytes (`block_size > 0`), typically a
  power of two matching the FPGA FIFO block handshake.
- `length` (BTPipeOut read, required): total transfer length in bytes
  (`length >= 0`, typically a multiple of `block_size`).

=== Register bridge operations <ov-classic-registers>
// ref: ADR-015 (PUT for write); ADR-025 (address ranges); Phase 44-H (bulk REST registers);
// openapi.yaml /classic/registers/read, /write; proto ReadRegister(s), WriteRegister(s).

#table(
  columns: (auto, 1.5fr, 1.5fr, 1.3fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Read register(s)], [`POST /devices/{serial}/classic/registers/read`], [`ReadRegister(ReadRegisterRequest)` & `ReadRegisters(ReadRegistersRequest)`], [Read → `{ "entries": [RegisterEntry, ...] }` / `RegisterValueResponse` / `RegisterEntriesResponse`],
  [Write register(s)], [`PUT /devices/{serial}/classic/registers/write`], [`WriteRegister(WriteRegisterRequest)` & `WriteRegisters(WriteRegistersRequest)`], [Write → `StatusResponse`],
)

Parameters and rules:
- Each `RegisterEntry` carries a 32-bit `address` (`uint32`) and a 32-bit
  `data` (`uint32`) word, mapping to `okTRegisterEntry` on the FrontPanel
  `okRegisterBridge` bus.
- On REST, `POST .../registers/read` takes `{ "addresses": [uint32, ...] }`
  (supporting both single and bulk reads) and returns
  `{ "entries": [{ "address": ..., "data": ... }, ...] }`.
  `PUT .../registers/write` takes `{ "entries": [RegisterEntry, ...] }`.
- *Pre-FFI batch enforcement:* Under `endpoint_enforcement = "enforce"`, every
  register `address` in a single or bulk read/write request MUST fall within a
  declared `AddressRange` of a usable resource held by the caller (with
  `classic` in `interfaces`, and not marked `private` when the caller is
  `ReadOnly`, @enf-address-ranges). For bulk requests, all addresses in the
  batch are verified *before* the FrontPanel FFI call is made; if any address
  is unauthorised, the entire call is rejected with `403 Forbidden` /
  `PERMISSION_DENIED` without touching the hardware.

=== Classic FPGA reset <ov-classic-reset>
// ref: ADR-034 D56, D66, D75 F1; @enf-reset.

#table(
  columns: (auto, 1.5fr, 1.4fr, 1.4fr),
  [Operation], [REST], [gRPC (`ClassicDataPortService`)], [Class & response],
  [Reset FPGA], [`POST /devices/{serial}/classic/reset`], [`ResetFPGA(DeviceRequest)`], [Write → `StatusResponse`],
)

Calls `okFrontPanel_ResetFPGA` on the device. Unlike reprogramming or clearing
the FPGA (@ov-cfg-clear), `ResetFPGA` does *not* delete or unlock registered
resources. Because it resets user logic across the design, it is classified as
a *write*: under `enforce`, the caller MUST hold at least one resource on the
device in `shared` or `exclusive` mode (`ReadOnly` holders are denied with
`403` / `PERMISSION_DENIED`, @enf-reset).
