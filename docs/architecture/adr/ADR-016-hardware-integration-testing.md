# ADR-016: Hardware Integration Testing

**Status**: Implemented  
**Date**: 2026-08-17  
**Deciders**: Architecture Team  
**Related**: [ADR-014](ADR-014-example-clients.md) (Example Client Architecture),
[ADR-017](ADR-017-ffi-endpoint-wiring.md) (FFI Endpoint Wiring)

## Context

Phase 16 delivered four example clients with protocol-level tests, but lacked
**real hardware tests** — no tests exercised actual FPGA operations (wire, trigger,
AXI) through the server's REST and gRPC APIs.

The user requires exhaustive integration tests using the `Counters.bit` bitfile on
a real XEM7320-A75T FPGA, covering both Classic and AXI data paths.

## Decision

### 1. Hardware Integration Test Matrix

#### Test Bitfile: Counters.bit

Path configured via environment variable:
- Rust: `OKFPGA_TEST_BITFILE` env var (default: `../../FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit`)
- Python: `OKFPGA_TEST_BITFILE` env var or `pyproject.toml` `[tool.okfpga-test]` section

The bitfile MUST NOT be committed to git (per `FrontPanel-6.1.0/ExampleBitfiles/README.md`).

#### Counters.bit Endpoint Reference

From `Counetrs-endpoints.md` (note: filename has typo):

**Classic Endpoints:**

| Endpoint | Type | Address/Bits | Description |
|----------|------|-------------|-------------|
| WireIn | Input | 0x00: bit0=RESET1, bit1=DISABLE1, bit2=AUTOCOUNT2 | Counter control signals |
| TriggerIn | Input | 0x40: bit0=RESET2, bit1=UP2, bit2=DOWN2 | Counter2 one-shot controls |
| WireOut | Output | 0x20=COUNT1[7:0], 0x21=COUNT2[7:0], 0x22=BUTTON[3:0] | Counter/button readback |
| TriggerOut | Output | 0x60: bit0=COUNT1==0x00, bit1=COUNT1==0x80 | Counter1 comparators |
| TriggerOut | Output | 0x61: bit0=COUNT2==0xFF | Counter2 comparator |

**AXI Endpoints (via okRegisterBridge):**

| Address | Name | Access | Description |
|---------|------|--------|-------------|
| 0x0000 | CTRL | R/W | [0] reset1, [1] disable1, [2] autocount2 |
| 0x0020 | COUNT1 | R | [7:0] counter1 value |
| 0x0024 | COUNT2 | R | [7:0] counter2 value |
| 0x0040 | TRIG_CTRL | W | [0] reset2, [1] up2, [2] down2 |
| 0x0060 | TRIG_STATUS | R | [0] count1==0x00, [1] count1==0x80, [2] count2==0xFF |

#### Test Matrix (Per Client)

All hardware tests are marked `#[ignore]` (Rust) or `@pytest.mark.skip` with
auto-detect (Python). They run only when the server is running and an FPGA is
connected.

**Server & Device Tests:**

| # | Test Name | Verifies |
|---|-----------|----------|
| 1 | `test_server_status` | Server status endpoint returns valid response |
| 2 | `test_list_services` | Service enumeration returns all compiled services |
| 3 | `test_list_devices_finds_xem7320` | Device discovery finds the connected FPGA |
| 4 | `test_device_info_matches_hardware` | DeviceInfo fields match real XEM7320 board |

**Configuration Tests:**

| # | Test Name | Verifies |
|---|-----------|----------|
| 5 | `test_configure_fpga_with_counters_bit` | Bitfile programming via API works |
| 6 | `test_dataport_type` | Dataport type is reported correctly |

**Classic Data Port Tests:**

| # | Test Name | Verifies |
|---|-----------|----------|
| 7 | `test_classic_wire_in_reset_counter1` | SetWireIn(0x00, bit0=1) resets counter1 to 0 |
| 8 | `test_classic_wire_in_disable` | SetWireIn(0x00, bit1=1) freezes counter |
| 9 | `test_classic_wire_in_free_run` | Clear control bits → counter advances |
| 10 | `test_classic_trigger_in_reset_counter2` | ActivateTrigger(0x40, bit0) resets counter2 |
| 11 | `test_classic_trigger_in_count_up` | ActivateTrigger(0x40, bit1) × 5 → counter2 == 5 |
| 12 | `test_classic_trigger_in_count_down` | 10 up + 3 down → counter2 == 7 |
| 13 | `test_classic_trigger_out_vector` | TriggerOut(0x60) returns comparator bits |
| 14 | `test_classic_register_read` | ReadRegister(0x00) returns value |
| 15 | `test_classic_register_write` | WriteRegister(0x00, 0x42) round-trip |

**AXI Data Port Tests (expected to skip on XEM7320):**

| # | Test Name | Verifies |
|---|-----------|----------|
| 16 | `test_axi_lite_read` | AXI Lite read (skips gracefully on Legacy devices) |
| 17 | `test_axi_lite_write` | AXI Lite write (skips gracefully on Legacy devices) |

**Test Execution Order:**

Tests must run sequentially (`--test-threads=1` for Rust, `-p no:randomly` for
pytest) in the numbered order above. The FPGA is a shared resource — parallel
execution would cause undefined hardware state.

### 2. Data Port Architecture & Dual-Endpoint Bitfiles

The `okTDeviceInfo.dataport` field (`okEDataport`) describes the **physical host
interface** (transport layer), NOT which HDL endpoint families are available:

| Value | Name | Meaning |
|-------|------|---------|
| 0 | `OK_DATAPORT_UNKNOWN` | Unknown or not yet determined |
| 1 | `OK_DATAPORT_LEGACY` | FP-Gen1/Gen2 USB host interface |
| 2 | `OK_DATAPORT_AXI` | FP-Gen3 USB host interface |

The XEM7320-A75T reports `dataportType = Legacy` (value 1) because it uses the
FP-Gen1/Gen2 USB host interface. This has **no bearing** on which HDL endpoint
families are available in the loaded bitfile.

#### How Counters.bit Implements Both Endpoint Families

Inside the HDL top-level, the shared **okHost** bus connects to all endpoint
modules concurrently:

```
                    ┌─────────────────────────────────────────────┐
                    │              FPGA Fabric                    │
                    │                                             │
  USB ──── okHost ──┤                                             │
                    │  ┌─────────────────┐                        │
                    ├──┤ okWireIn (0x00)  ├──┐                    │
                    │  └─────────────────┘  │  ┌──────────────┐  │
                    │  ┌─────────────────┐  ├──┤  Counter     │  │
                    ├──┤ okTriggerIn(0x40)├──┘  │  Logic       │  │
                    │  └─────────────────┘  ┌──┤  (shared)    │  │
                    │  ┌─────────────────┐  │  └──────────────┘  │
                    ├──┤ okWireOut (0x20) ├──┘         │          │
                    │  └─────────────────┘            │          │
                    │  ┌─────────────────┐            │          │
                    ├──┤ okTriggerOut(0x60)├───────────┘          │
                    │  └─────────────────┘                       │
                    │                                             │
                    │  ┌──────────────────────┐                   │
                    ├──┤ okRegisterBridge     ├───── same counter │
                    │  │ (AXI-Lite decode)    │      registers    │
                    │  │  0x0000 = CTRL       │                   │
                    │  │  0x0020 = COUNT1     │                   │
                    │  │  0x0040 = TRIG_CTRL  │                   │
                    │  │  0x0060 = TRIG_STATUS│                   │
                    │  └──────────────────────┘                   │
                    └─────────────────────────────────────────────┘
```

**Classic access**: `SetWireInValue(0x00, ...)` / `UpdateWireIns()` manipulates
counters via traditional wire endpoints.

**AXI / Register access**: `WriteRegister(0x00000000, ...)` /
`ReadRegister(0x00000020, ...)` accesses the exact same counter block using
memory-mapped register semantics via `okRegisterBridge`.

Both paths run in parallel inside the fabric and manipulate shared counter signals.

#### Server Behavior

The server MUST NOT gate API routes based on the `okTDeviceInfo.dataport` field.
All Classic and AXI endpoints are always available. If the loaded bitfile does not
implement a requested endpoint family, the FrontPanel API returns an error code
(e.g., `ok_UnsupportedFeature`), which the server maps to an appropriate error
response.

See [ADR-003 Addendum A](ADR-003-data-port-abstraction.md) for the full
architectural correction.

### 3. Bitfile Path Configuration

| Platform | Config Mechanism | Default |
|----------|-----------------|---------|
| Rust clients | `OKFPGA_TEST_BITFILE` env var | `../../FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit` |
| Python clients | `OKFPGA_TEST_BITFILE` env var | Same relative path |

Tests skip gracefully if the bitfile is not found at the configured path.

## Consequences

### Positive
- **Comprehensive validation** — 17+ tests per client covering the full stack from
  HTTP/gRPC down to real hardware I/O
- **Reproducible** — bitfile path is configurable for different build environments
- **Graceful degradation** — AXI tests skip on Legacy devices rather than failing

### Negative
- **Requires real hardware** — tests cannot run in CI without an FPGA connected
- **Sequential execution** — hardware tests cannot be parallelized

## GUI Integration Tests (ADR-023)

The eight GUI demo clients (ADR-023) serve as **interactive hardware integration
tests**. They exercise the same Classic Data Port endpoints as the automated
test matrix above, but under user-driven control rather than scripted sequences.

GUI demos additionally test:
- **Resource locking enforcement** — user can override client-side lockout to
  verify that the server rejects writes from non-lock-holders
- **Multi-user shared access** — multiple GUI clients connected simultaneously
  to the same board and resource
- **Session timeout** — user can exit without releasing the resource to verify
  the server's sweep mechanism reclaims it

See [ADR-023 — GUI Example Clients](ADR-023-gui-example-clients.md).

## References

- [ADR-014 — Example Client Architecture](ADR-014-example-clients.md)
- [ADR-017 — FFI Endpoint Wiring](ADR-017-ffi-endpoint-wiring.md)
- [ADR-020 — Client-Side Resource Registration](ADR-020-client-side-resource-registration.md)
- [ADR-023 — GUI Example Clients](ADR-023-gui-example-clients.md)
- [Counters.bit Endpoint Reference](../counters-bit-endpoints.md)
- [ai-collaboration.md §9 — Testing Strategy](../../.agents/rules/ai-collaboration.md)
