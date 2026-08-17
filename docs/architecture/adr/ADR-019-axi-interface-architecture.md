# ADR-019: AXI Interface Architecture — Lite, Full, and Stream

**Status**: Accepted  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Depends on**: [ADR-003](ADR-003-data-port-abstraction.md) (Data Port Abstraction),
[ADR-004](ADR-004-rest-api-design.md) (REST API Design),
[ADR-005](ADR-005-grpc-api-design.md) (gRPC API Design),
[ADR-017](ADR-017-ffi-endpoint-wiring.md) (FFI Endpoint Wiring)

## Context

The FrontPanel 6.1.0 `FPGADataPortAXI` class aggregates **three independent AXI
sub-interfaces**. These are not interchangeable variants of the same thing — they
are fundamentally different protocols with different addressing models, transfer
semantics, and use cases:

| Sub-Interface | Protocol | Addressing | Transfer Unit | Primary Use Case |
|---------------|----------|-----------|---------------|------------------|
| **AXI Lite** | AXI4-Lite | Explicit 32/64-bit addresses | Single 32-bit register R/W (+ bulk) | Control & status registers (CSRs), peripheral configuration |
| **AXI Full** | AXI4 | Explicit 32/64-bit addresses | Burst-capable byte-level memory-mapped | Accessing BRAM, DDR4, bulk block transfers |
| **AXI Stream** | AXI4-Stream | **Addressless** (FIFO/point-to-point) | Continuous ordered byte stream | DSP pipelines, ADC/DAC sample streams, packet processing |

A single FPGA design can instantiate **any combination** of these simultaneously.
For example, a DSP design might use:
- AXI Lite for configuration registers
- AXI Full for DMA buffer access
- AXI Stream for real-time sample data

### AXI Availability — Device Generation

The AXI data port is only available on **FP-Gen3-USB** devices. Such devices report
`okDATAPORT_AXI` in `okTDeviceInfo.dataport`. On all other devices (Gen1/Gen2-USB),
`GetFPGADataPortAXI()` returns `ok_UnsupportedFeature (-15)`.

**Known device classification:**

| Device | Generation | AXI Support |
|--------|-----------|-------------|
| XEM7320 | Gen2-USB | ❌ `okDATAPORT_LEGACY` — AXI unsupported |
| XEM8320 | Gen3-USB | ✅ `okDATAPORT_AXI` |
| XEM8350 | Gen3-USB | ✅ `okDATAPORT_AXI` |
| XEM8305 | Gen3-USB | ✅ `okDATAPORT_AXI` |
| XEM8310 | Gen3-USB | ✅ `okDATAPORT_AXI` |
| XEM8370 | Gen3-USB | ✅ `okDATAPORT_AXI` |

### Problem: Incorrect Polymorphic Fallback

An earlier implementation (commit `704f37f`) incorrectly treated the AXI Lite
endpoints as "polymorphic" — the `axi_lite_read/write` REST handlers would fall
back to AXI Full when AXI Lite was unavailable. **This is architecturally wrong**
because:

1. AXI Lite and AXI Full have different transfer semantics (32-bit registers vs
   byte buffers with burst)
2. They are independent sub-interfaces — a bitfile can have one without the other
3. A client requesting "AXI Lite read" should not silently receive an "AXI Full
   read" result with different byte-level semantics
4. If a bitfile has both AXI Lite and AXI Full, they access **different** address
   spaces in the FPGA fabric

## Decision

### 1. Three Distinct API Endpoint Groups

Each AXI sub-interface gets its own, independent set of REST endpoints and gRPC
RPCs. There is **no fallback** between them. If the requested sub-interface is
not available on the device or bitfile, the server returns an error.

```
/devices/{serial}/axi/
├── lite/                    ← AXI Lite (register access)
│   ├── POST /read           ← Read 32-bit register(s)
│   ├── PUT  /write          ← Write 32-bit register(s) (idempotent)
│   └── POST /reset          ← Reset AXI Lite interface
│
├── full/                    ← AXI Full (memory-mapped bulk)
│   ├── POST /read           ← Read byte buffer from address
│   ├── POST /write          ← Write byte buffer to address
│   ├── GET  /datapath-width ← GetDatapathWidthByteCount
│   └── POST /reset          ← Reset AXI Full interface
│
└── stream/                  ← AXI Stream (addressless streaming)
    ├── POST /read           ← Read from FPGA FIFO
    ├── POST /write          ← Write to FPGA FIFO
    ├── GET  /datapath-width ← Read/write datapath widths
    └── POST /reset          ← Reset AXI Stream interface
```

### 2. Handle Acquisition — Independent Sub-Handles

After `ConfigureFPGA`, the server:

1. Calls `GetFPGADataPortAXI()`.
   - If it returns `ok_UnsupportedFeature`: **AXI is not supported** on this device.
     All AXI endpoints return `409 Conflict` with a clear message.
   - If it returns `ok_NoError`: proceed to step 2.

2. Independently probes each sub-interface:
   - `GetAXILite()` → stores `axi_lite` handle (or `None` if null)
   - `GetAXIFull()` → stores `axi_full` handle (or `None` if null)
   - `GetAXIStream()` → stores `axi_stream` handle (or `None` if null)

3. Each endpoint checks **only its own sub-handle**:
   - `POST /axi/lite/read` checks `axi_lite` — returns `409` if `None`
   - `POST /axi/full/read` checks `axi_full` — returns `409` if `None`
   - `POST /axi/stream/read` checks `axi_stream` — returns `409` if `None`

No cross-interface fallback. No polymorphism.

### 3. REST Endpoint Design Per Sub-Interface

#### AXI Lite — Register Access

Designed for low-latency, discrete register operations. Maps naturally to
request-response HTTP:

- **Read**: `POST /axi/lite/read` with `{"address": 0x0000}` → `{"address": 0x0000, "value": 42}`
- **Write**: `PUT /axi/lite/write` with `{"address": 0x0000, "value": 42}` → `{"status": "ok"}`
- **Bulk read**: `POST /axi/lite/read` with `{"address": 0x0000, "count": 16}` → `{"values": [...]}`
- **Bulk write**: `PUT /axi/lite/write` with `{"address": 0x0000, "values": [...]}` → `{"status": "ok"}`

SSE can be used for periodic register polling (dashboard status registers).

#### AXI Full — Memory-Mapped Bulk

Designed for high-throughput, burst-capable memory transfers. Binary payloads:

- **Read**: `POST /axi/full/read` with `{"address": 0x0000, "length": 4096}` →
  `application/octet-stream` or base64-encoded JSON
- **Write**: `POST /axi/full/write` with binary body (address in query param or JSON envelope)
- **Datapath width**: `GET /axi/full/datapath-width` → `{"datapath_width_bytes": 8}`

Large transfers should be chunked to prevent HTTP timeout or memory exhaustion.

#### AXI Stream — Addressless Streaming

Designed for continuous, unidirectional data flow. No addresses:

- **Read (FPGA → Client)**: `POST /axi/stream/read` with `{"length": 4096}` →
  binary data. For continuous streaming, use **SSE** or chunked HTTP streaming.
- **Write (Client → FPGA)**: `POST /axi/stream/write` with binary body →
  `{"transferred_bytes": 4096}`
- **Datapath widths**: `GET /axi/stream/datapath-width` →
  `{"read_width_bytes": 4, "write_width_bytes": 4}`

For full-duplex interactive streaming, **gRPC bidirectional streaming** is preferred
over REST/SSE (SSE is server-to-client only).

### 4. gRPC RPC Design Per Sub-Interface

The `AXIDataPortService` in `okfpga.proto` already defines correct, distinct RPCs:

```protobuf
service AXIDataPortService {
  // AXI Lite — register-level access
  rpc AXILiteRead(AXILiteReadRequest) returns (AXILiteReadResponse);
  rpc AXILiteWrite(AXILiteWriteRequest) returns (StatusResponse);
  rpc AXILiteReadBulk(AXILiteReadBulkRequest) returns (AXILiteReadBulkResponse);
  rpc AXILiteWriteBulk(AXILiteWriteBulkRequest) returns (AXIOperationStats);
  rpc AXILiteReset(DeviceRequest) returns (StatusResponse);

  // AXI Full — bulk memory-mapped
  rpc AXIFullGetDatapathWidth(DeviceRequest) returns (DatapathWidthResponse);
  rpc AXIFullRead(AXIFullReadRequest) returns (AXIFullReadResponse);
  rpc AXIFullWrite(AXIFullWriteRequest) returns (AXIOperationStats);
  rpc AXIFullReset(DeviceRequest) returns (StatusResponse);

  // AXI Stream — continuous streaming
  rpc AXIStreamGetDatapathWidths(DeviceRequest) returns (AXIStreamDatapathWidthsResponse);
  rpc AXIStreamRead(AXIStreamReadRequest) returns (AXIStreamReadResponse);
  rpc AXIStreamWrite(AXIStreamWriteRequest) returns (AXIStreamWriteResponse);
  rpc AXIStreamReset(DeviceRequest) returns (StatusResponse);
}
```

**Future enhancement**: For high-throughput AXI Full transfers, consider adding
server-streaming/client-streaming RPCs. For AXI Stream, consider bidirectional
streaming RPCs to match the continuous nature of the protocol.

### 5. Error Responses

| Scenario | HTTP | gRPC | Message |
|----------|------|------|---------|
| Device doesn't support AXI (`okDATAPORT_LEGACY`) | 409 Conflict | FAILED_PRECONDITION | "Device does not support AXI data port (okDATAPORT_LEGACY)" |
| AXI supported but sub-interface not in bitfile | 409 Conflict | FAILED_PRECONDITION | "FPGA programmed but AXI {Lite\|Full\|Stream} not available in this bitfile" |
| FPGA not programmed | 409 Conflict | FAILED_PRECONDITION | "FPGA not configured" |
| AXI Slave/Decode error | 400 Bad Request | INVALID_ARGUMENT | Error message from FrontPanel |
| Timeout | 504 Gateway Timeout | DEADLINE_EXCEEDED | "AXI transfer timed out" |

### 6. Concurrency — Hardware Serialization

Per [ADR-018](ADR-018-device-handle-pool.md), all FFI calls (including AXI) are
serialized per-device via `tokio::sync::Mutex`. The USB controller cannot handle
overlapping requests across sub-interfaces on the same device.

Different devices can be accessed concurrently.

## Consequences

### Positive

- **Correct semantics** — each sub-interface has its own endpoint, matching the
  hardware architecture
- **No silent data corruption** — a client requesting AXI Lite read never
  accidentally gets an AXI Full read with different byte semantics
- **Future-proof** — AXI Stream can evolve to use SSE or gRPC bidirectional
  streaming without affecting Lite or Full endpoints
- **Clear error messages** — clients know exactly which sub-interface is unavailable
  and why

### Negative

- **Larger API surface** — 12 AXI endpoints (4 per sub-interface) instead of 3
  "polymorphic" ones. This is the correct trade-off for correctness.
- **Cannot test AXI on XEM7320** — the currently connected test hardware does not
  support AXI. Testing requires a Gen3-USB device (e.g., XEM8320, XEM8350).

## Files to Change

| File | Change |
|------|--------|
| `crates/ok-fpga-server/src/rest/axi.rs` | Remove Lite→Full fallback; each handler uses only its own sub-handle |
| `crates/ok-fpga-server/src/handle_pool.rs` | Already correct (acquires all 3 sub-handles independently) |
| `client-examples/python-rest-client/tests/test_hardware.py` | No change needed (tests correctly skip on 409) |

## References

- [notes-on-AXI.md](../../notes-on-AXI.md) — detailed AXI sub-interface comparison
- [ADR-003 — Data Port Abstraction](ADR-003-data-port-abstraction.md) — Classic vs AXI split
- [ADR-004 — REST API Design](ADR-004-rest-api-design.md) — URL hierarchy
- [ADR-005 — gRPC API Design](ADR-005-grpc-api-design.md) — AXIDataPortService definition
- [ADR-017 — FFI Endpoint Wiring](ADR-017-ffi-endpoint-wiring.md) — wiring strategy
- [ADR-018 — Device Handle Pool](ADR-018-device-handle-pool.md) — per-device mutex
- [Opal Kelly FPGADataPortAXI documentation](https://library.opalkelly.com/library/FrontPanelAPI/classOpalKelly_1_1FPGADataPortAXI.html)
