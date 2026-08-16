# ADR-003: Data Port Abstraction — Classic vs AXI

**Status**: Accepted  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

FrontPanel 6.1.0 splits FPGA data operations into two distinct "data port" subsystems:

1. **Classic Data Port** (`FPGADataPortClassic`) — traditional FrontPanel wire/trigger/pipe/register operations
2. **AXI Data Port** (`FPGADataPortAXI`) — AXI-based operations with three sub-interfaces:
   - `AXILite` — single/bulk register read/write
   - `AXIFull` — bulk memory-mapped read/write
   - `AXIStream` — streaming data transfers

Device-level operations (configure FPGA, get device info, get sensors/settings, PLL, flash) remain on the `FrontPanel` device handle. Data operations (wires, triggers, pipes, registers) are accessed through a data port obtained *after* FPGA configuration via `GetFPGADataPortClassic()` or `GetFPGADataPortAXI()`.

The data port type depends on the FPGA's gateware and the device family:
- `okTDeviceInfo.dataport == OK_DATAPORT_LEGACY (1)` → Classic
- `okTDeviceInfo.dataport == OK_DATAPORT_AXI (2)` → AXI

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

// The device knows which data port type it has
pub enum ActiveDataPort {
    Classic(ClassicDataPort),
    AXI(AXIDataPort),
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

Both REST and gRPC APIs will provide operations for both Classic and AXI data ports. The API layer will route to the correct data port based on the device's `dataport` field from `okTDeviceInfo`. If a client attempts a Classic operation on an AXI-only device (or vice versa), the server returns a clear error.

## Consequences

### Positive
- **Type safety** — Rust's type system prevents calling Classic operations on an AXI device at compile time within the core layer
- **Clean API** — clients see a unified device with the correct operations for its data port type
- **Correct ownership** — data port lifetime tied to device lifetime

### Negative
- **Dual API surface** — both Classic and AXI methods must be exposed in REST and gRPC, roughly doubling the data-path endpoint count
- **Runtime dispatch** — the `ActiveDataPort` enum requires runtime checks when routing from the API layer

## References

- [FPGADataPortClassic C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 1080-1131
- [FPGADataPortAXI / AXILite / AXIFull / AXIStream C API](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 939-1077
- [C++ data port split](../../FrontPanel-6.1.0/okFrontPanel.h) — lines 1696-1822
- [okTDeviceInfo.dataport field](../../FrontPanel-6.1.0/okFrontPanel.h) — line 642
