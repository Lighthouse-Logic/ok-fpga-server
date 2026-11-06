// chapters/introduction.typ — Section 1.
// ref: ADR-001 (Rust server), ADR-004 (REST), ADR-005 (gRPC), ADR-015 (REST
// conventions), ADR-025 (endpoint address ranges), fp6-api-surface.md.
#import "../lib.typ": *

= Introduction <intro>

== Purpose and audience <intro-purpose>
// ref: user request 2026-10-11 (developer-facing API spec).

This document specifies the public interfaces of *ok-fpga-server*: a network
server that gives remote clients controlled access to Opal Kelly FrontPanel
FPGA modules. It explains every operation and parameter of the REST
(OpenAPI) and gRPC (Protocol Buffers) interfaces. It also explains *why* each
one behaves as it does, with examples and references to the Opal Kelly
documentation.

The intended readers are:

- client developers writing software (Python, C++, LabVIEW, web) that talks to
  the server;
- HDL architects deciding how a bitfile's endpoints are divided into
  resources that several users can share safely;
- server maintainers who must keep the implementation consistent with this
  specification.

The machine-readable definitions are `api/openapi.yaml` (REST) and
`proto/okfpga/v1/okfpga.proto` (gRPC). This document is their companion. It
holds the behavioural rules and rationale that are too long for inline
comments. Where the definitions and this document disagree, report the
discrepancy; it is a defect in one of them.

== Conventions <intro-conventions>

=== Requirement keywords <intro-keywords>
The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are to be read as
described in RFC 2119#footnote[S. Bradner, _Key words for use in RFCs to
Indicate Requirement Levels_, RFC 2119,
#link("https://www.rfc-editor.org/rfc/rfc2119")[rfc-editor.org/rfc/rfc2119]\.]\.
A requirement on "the server" applies to both the REST and gRPC interfaces
unless the text says otherwise.

=== Notation <intro-notation>
- Endpoint addresses and memory addresses are written in hexadecimal:
  `0x20`, `0x44A6_0000`. Underscores are for readability only. In JSON they
  are strings (`"0x20"`); in protobuf they are unsigned integers.
- REST paths are relative to the base URL `http://<host>:8088/api/v1`.
- gRPC methods are written `Service.Method`, for example
  `DeviceService.AcquireResource`. The protobuf package is
  `okFPGAService.v1`.
- `<serial>` is the device serial number reported by FrontPanel (for example
  `2149000ABC`).

=== One contract, two transports <intro-two-transports>
// ref: rules §4 (specs are the single source of truth); ADR-004/ADR-005
// parity; "REST and gRPC always change together" (project rule).
REST and gRPC are two encodings of *one* contract. Every operation that
exists on one MUST exist on the other with the same meaning, the same
parameters (subject to naming conventions) and the same error conditions.
Errors map as follows:

#table(
  columns: (auto, auto, 1fr),
  [REST status], [gRPC status], [Typical meaning],
  [`400`], [`INVALID_ARGUMENT`], [Malformed request (bad JSON, bad hex string).],
  [`401`], [`UNAUTHENTICATED`], [No or invalid credentials.],
  [`403`], [`PERMISSION_DENIED`], [The caller is identified but the access rules deny the operation.],
  [`404`], [`NOT_FOUND`], [Unknown device, resource or user.],
  [`409`], [`FAILED_PRECONDITION`], [The request conflicts with the current state (name in use, resource held, overlapping exclusive declaration).],
  [`422`], [`INVALID_ARGUMENT`], [Well-formed request that violates a semantic rule (for example a missing endpoint address).],
  [`500`], [`INTERNAL`], [FrontPanel or server failure.],
  [`503`], [`UNAVAILABLE`], [Device busy, disconnected or not programmed.],
)

// ref: openapi.yaml components.schemas.ProblemDetail / ResourceConflict;
// proto ResourceConflictDetail; ADR-034 D73.
REST error bodies use the `ProblemDetail` schema (RFC 7807 problem details:
`type`, `title`, `status`, `detail`, `instance`). Failed resource operations
add a machine-readable `conflict` object; gRPC carries the same fields as a
`ResourceConflictDetail` in the status details.

== Opal Kelly FrontPanel background <intro-frontpanel>
// ref: fp6-api-surface.md; ADR-025 §1 (address ranges); okFrontPanel.h.

=== Modules and the host interface <intro-modules>
Opal Kelly modules (for example the XEM8320) combine an FPGA with a host
interface (USB 3.0 or PCIe). The FrontPanel SDK provides a host API and HDL
modules ("endpoints") that the FPGA designer instantiates. The host talks to
the endpoints through the host interface#footnote[Opal Kelly,
_FrontPanel SDK documentation_,
#link("https://docs.opalkelly.com/fpsdk/")[docs.opalkelly.com/fpsdk]\.]\.

ok-fpga-server supports the XEM8305, XEM8310, XEM7320, XEM8320, XEM8350,
XEM8370 and HUB1450 using the FrontPanel 6.1.0 C API. USB 2.0 devices are not
supported and are filtered out of device lists.

FrontPanel allows *one open handle per device*. The server owns that handle.
All clients share it through the server. This is the root reason the server
needs a resource model: without it, nothing would stop two clients from
writing the same endpoint at the same time.

=== Classic endpoints <intro-endpoints>
The "Classic" data port exposes a fixed 8-bit endpoint address space. The
address alone tells you the endpoint type#footnote[Opal Kelly,
_FrontPanel HDL: Endpoints_,
#link("https://docs.opalkelly.com/fpsdk/frontpanel-hdl/")[docs.opalkelly.com/fpsdk/frontpanel-hdl]; #ok-api\.]\:

#table(
  columns: (auto, auto, auto, 1fr),
  [Endpoint], [Addresses], [Direction], [Behaviour],
  [WireIn], [`0x00`–`0x1F`], [host → FPGA], [32-bit static value. Set in a host buffer, sent to the FPGA by `UpdateWireIns`.],
  [WireOut], [`0x20`–`0x3F`], [FPGA → host], [32-bit static value. All WireOuts are copied into a host buffer by `UpdateWireOuts`; `GetWireOutValue` reads the buffer.],
  [TriggerIn], [`0x40`–`0x5F`], [host → FPGA], [32 one-shot bits, each a single clock pulse in the FPGA, sent by `ActivateTriggerIn`.],
  [TriggerOut], [`0x60`–`0x7F`], [FPGA → host], [32 latched event bits. `UpdateTriggerOuts` fetches and *clears* the latched bits; `IsTriggered` tests the fetched copy.],
  [PipeIn / BTPipeIn], [`0x80`–`0x9F`], [host → FPGA], [Byte stream (block-throttled variant uses a ready signal).],
  [PipeOut / BTPipeOut], [`0xA0`–`0xBF`], [FPGA → host], [Byte stream (block-throttled variant uses a ready signal).],
)

An address in the pipe ranges is either a standard pipe or a block-throttled
pipe, never both. The register bridge (`okRegisterBridge`) adds a separate
32-bit register address space reached by `ReadRegister` / `WriteRegister`.

=== The shared host buffer <intro-host-buffer>
// ref: D75 (snapshot semantics); okFrontPanel.h 1095–1103.
WireOut and TriggerOut values are not read directly from the FPGA. A single
`UpdateWireOuts` (or `UpdateTriggerOuts`) call fetches *all* 32 values of
that type into one buffer inside the open device handle. Later
`GetWireOutValue` / `IsTriggered` calls only read that buffer. Because the
server owns the only handle, *every client sees the same buffer*. @enforcement
explains how this affects read semantics, and why reading a TriggerOut can
destroy information another client was waiting for.

=== AXI data port <intro-axi>
Newer FrontPanel designs may use an AXI data port instead of the Classic
endpoints. AXI4-Lite, AXI4 (full) and AXI4-Stream accesses are addressed by
32-bit memory addresses rather than endpoint numbers. The resource model
protects them with *address ranges* (@resource-model).
