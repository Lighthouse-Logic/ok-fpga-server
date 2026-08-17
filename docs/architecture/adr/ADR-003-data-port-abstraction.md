# ADR-003: Data Port Abstraction — Classic vs AXI

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

FrontPanel 6.1.0 provides two distinct HDL endpoint families for FPGA data
operations:

1. **Classic Endpoints** (`FPGADataPortClassic`) — traditional FrontPanel wire/trigger/pipe/register operations using discrete `okWireIn`, `okWireOut`, `okTriggerIn`, `okTriggerOut`, `okPipeIn`, `okPipeOut` HDL modules.
2. **AXI Endpoints** (`FPGADataPortAXI`) — memory-mapped and streaming operations via `okRegisterBridge` with three sub-interfaces:
   - `AXILite` — single/bulk register read/write
   - `AXIFull` — bulk memory-mapped read/write
   - `AXIStream` — streaming data transfers

Device-level operations (configure FPGA, get device info, get sensors/settings, PLL, flash) remain on the `FrontPanel` device handle. Data operations (wires, triggers, pipes, registers) are accessed through a data port obtained *after* FPGA configuration via `GetFPGADataPortClassic()` or `GetFPGADataPortAXI()`.

### Important: `okTDeviceInfo.dataport` vs HDL Endpoint Availability

The field `okTDeviceInfo.dataport` (`okEDataport`) describes the **physical host
interface** (transport layer), NOT which HDL endpoint families are available:

| Value | Name | Meaning |
|-------|------|---------|
| 0 | `OK_DATAPORT_UNKNOWN` | Unknown or not yet determined |
| 1 | `OK_DATAPORT_LEGACY` | FrontPanel Gen1/Gen2 USB host interface |
| 2 | `OK_DATAPORT_AXI` | FrontPanel Gen3 USB host interface |

This value does **NOT** determine whether Classic or AXI API calls can be used.
A single bitfile can implement **both** Classic endpoints and AXI-mapped endpoints
simultaneously. For example, Opal Kelly's `Counters.bit` uses:

- **Classic path**: `okWireIn` (0x00), `okTriggerIn` (0x40), `okWireOut` (0x20-0x22),
  `okTriggerOut` (0x60-0x61) wired directly to counter control/readback logic.
- **AXI path**: `okRegisterBridge` decodes 32-bit addresses (0x0000, 0x0020, 0x0040,
  0x0060) to read/write the same counter registers via memory-mapped semantics.

Both paths run in parallel inside the FPGA fabric on the shared `okHost` bus,
manipulating the same underlying counter signals.

## Decision

### Model the data port split as Rust traits with concrete implementations

```
                  ┌─────────────────┐
                  │   FpgaDevice    │
                  │ (device handle) │
                  └────────┬────────┘
                           │ GetFPGADataPort*()
              ┌────────────┴────────────┐
              ▼                         ▼
   ┌──────────────────┐    ┌──────────────────┐
   │ ClassicDataPort  │    │   AXIDataPort    │
   │                  │    │                  │
   │ - Wires          │    │ - AXILite        │
   │ - Triggers       │    │ - AXIFull        │
   │ - Pipes          │    │ - AXIStream      │
   │ - Registers      │    │                  │
   │ - ResetFPGA      │    │                  │
   └──────────────────┘    └──────────────────┘
```

### Rust type design

```rust
// Marker trait — data port identity
pub trait DataPort: Send {}

pub struct ClassicDataPort { /* wraps okFPGADataPortClassic_HANDLE */ }
pub struct AXIDataPort { /* wraps okFPGADataPortAXI_HANDLE */ }

impl DataPort for ClassicDataPort {}
impl DataPort for AXIDataPort {}

// The device tracks which data port handles have been obtained.
// Note: A device can have BOTH Classic and AXI ports active simultaneously
// if the loaded bitfile implements both endpoint families.
pub enum ActiveDataPort {
    Classic(ClassicDataPort),
    AXI(AXIDataPort),
    Both(ClassicDataPort, AXIDataPort), // Bitfile implements both
    None, // FPGA not yet configured
}
```

### Ownership model

Per the C++ API (`okCFrontPanel` owns `m_classicPort` and `m_axiPort` as value members), the data port handle is **owned by the device handle** — it must not outlive the device, and the caller must not free it. In Rust:

- `ClassicDataPort` holds a raw handle and a `PhantomData<&'device FpgaDevice>` to enforce the lifetime
- Alternatively, `ClassicDataPort` can hold an `Arc` reference to the device to prevent premature drop
- The data port is obtained lazily after `ConfigureFPGA` and cached in the `FpgaDevice` struct

### Method mapping

#### ClassicDataPort methods (from `okFPGADataPortClassic_*` C functions)

| Rust Method | C Function |
|------------|-----------|
| `reset_fpga()` | `okFPGADataPortClassic_ResetFPGA` |
| `set_wire_in_value(ep, val, mask)` | `okFPGADataPortClassic_SetWireInValue` |
| `get_wire_in_value(ep) -> u32` | `okFPGADataPortClassic_GetWireInValue` |
| `update_wire_ins()` | `okFPGADataPortClassic_UpdateWireIns` |
| `get_wire_out_value(ep) -> u32` | `okFPGADataPortClassic_GetWireOutValue` |
| `update_wire_outs()` | `okFPGADataPortClassic_UpdateWireOuts` |
| `activate_trigger_in(ep, bit)` | `okFPGADataPortClassic_ActivateTriggerIn` |
| `update_trigger_outs()` | `okFPGADataPortClassic_UpdateTriggerOuts` |
| `is_triggered(ep, mask) -> bool` | `okFPGADataPortClassic_IsTriggered` |
| `get_trigger_out_vector(ep) -> u32` | `okFPGADataPortClassic_GetTriggerOutVector` |
| `write_to_pipe_in(ep, data) -> Result<i64>` | `okFPGADataPortClassic_WriteToPipeIn` |
| `read_from_pipe_out(ep, buf) -> Result<i64>` | `okFPGADataPortClassic_ReadFromPipeOut` |
| `write_to_block_pipe_in(ep, block_size, data)` | `okFPGADataPortClassic_WriteToBlockPipeIn` |
| `read_from_block_pipe_out(ep, block_size, buf)` | `okFPGADataPortClassic_ReadFromBlockPipeOut` |
| `set_bt_pipe_polling_interval(interval)` | `okFPGADataPortClassic_SetBTPipePollingInterval` |
| `read_register(addr) -> Result<u32>` | `okFPGADataPortClassic_ReadRegister` |
| `write_register(addr, data)` | `okFPGADataPortClassic_WriteRegister` |
| `read_registers(entries) -> Result<()>` | `okFPGADataPortClassic_ReadRegisters` |
| `write_registers(entries)` | `okFPGADataPortClassic_WriteRegisters` |

#### AXIDataPort sub-interfaces

| Sub-Interface | C Handle | Key Methods |
|--------------|----------|-------------|
| `AXILite` | `okFrontPanelAXILite_HANDLE` | `Read`, `Write`, `ReadBulk`, `WriteBulk`, `Reset` (each with default and explicit timeout variants) |
| `AXIFull` | `okFrontPanelAXIFull_HANDLE` | `GetDatapathWidthByteCount`, `Read`, `Write`, `Reset` (each with default and explicit burst/timeout variants) |
| `AXIStream` | `okFrontPanelAXIStream_HANDLE` | `GetWriteDatapathWidthByteCount`, `GetReadDatapathWidthByteCount`, `Read`, `Write`, `Reset` |

Obtained via:
- `okFPGADataPortAXI_GetAXILite(handle)`
- `okFPGADataPortAXI_GetAXIFull(handle)`
- `okFPGADataPortAXI_GetAXIStream(handle)`

### API exposure

Both REST and gRPC APIs expose operations for **both** Classic and AXI endpoint
families unconditionally. The server does **not** gate API routes based on the
`okTDeviceInfo.dataport` field (which describes the physical host interface, not
HDL endpoint availability).

The server attempts the requested FFI call regardless of the reported dataport type.
If the loaded bitfile does not implement the requested endpoint family, the
FrontPanel API returns an error code (e.g., `ok_UnsupportedFeature`), which the
server maps to an appropriate HTTP/gRPC error response.

## Consequences

### Positive
- **No artificial restrictions** — clients can use both Classic and AXI endpoints
  when the loaded bitfile supports both (e.g., Counters.bit)
- **Clean API** — clients see a unified device with all data port operations
  available; the FrontPanel API returns clear errors for unsupported operations
- **Correct ownership** — data port lifetime tied to device lifetime

### Negative
- **Dual API surface** — both Classic and AXI methods are always exposed in REST
  and gRPC, roughly doubling the data-path endpoint count
- **Runtime errors** — unsupported operations fail at runtime (FFI error) rather
  than at compile time or route-registration time

## Addendum A: Dataport Field Clarification (2026-08-17)

The original ADR incorrectly stated that `okTDeviceInfo.dataport` determines whether
Classic or AXI API calls can be used. This has been corrected above.

**Key clarification**: `okTDeviceInfo.dataport` (`okEDataport`) describes the
**physical host interface** type:
- `OK_DATAPORT_LEGACY (1)` = FP-Gen1/Gen2-USB transport
- `OK_DATAPORT_AXI (2)` = FP-Gen3-USB transport

This is a **transport layer** property, independent of which HDL endpoint families
(Classic wires/triggers vs AXI register bridge) are instantiated in the loaded
bitfile. Inside the FPGA:

1. The **okHost** bus connects to the physical interface
2. Classic endpoints (`okWireIn`, `okTriggerIn`, etc.) and `okRegisterBridge`
   (AXI path) attach to the **same** okHost bus in parallel
3. Both endpoint families can coexist in a single bitfile

The server therefore does not restrict API access based on this field.

## Addendum B: AXI Sub-Interface Independence (2026-08-17)

See [ADR-019 — AXI Interface Architecture](ADR-019-axi-interface-architecture.md)
for the full decision.

**Key clarification**: The three AXI sub-interfaces (Lite, Full, Stream) are
**independent, distinct protocols** that can coexist simultaneously in a single
FPGA design. They are NOT interchangeable or polymorphic:

- **AXI Lite**: 32-bit register access (addressed)
- **AXI Full**: Burst-capable byte-level memory-mapped access (addressed)
- **AXI Stream**: Continuous addressless data streaming (FIFO)

REST and gRPC endpoints for each sub-interface must use **only** their own
sub-handle. There must be no fallback from one sub-interface to another.

The AXI data port itself is only available on FP-Gen3-USB devices. Devices
reporting `okDATAPORT_LEGACY` (e.g., XEM7320) do not support any AXI sub-interface.

## References

- [FPGADataPortClassic C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 1080-1131
- [FPGADataPortAXI / AXILite / AXIFull / AXIStream C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 939-1077
- [C++ data port split](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 1696-1822
- [okTDeviceInfo.dataport field](../../FrontPanel-6.1.0/okFrontPanel.h) — line 642
- [ADR-019 — AXI Interface Architecture](ADR-019-axi-interface-architecture.md)

