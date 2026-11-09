# Counters.bit — Classic Data Port Endpoint Reference

**Bitfile**: `Counters.bit` (FrontPanel Example Bitfile)  
**Board**: XEM7320-A75T (also available for XEM8320-AU25P)  
**Path**: `FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit`  
**Env Var**: `OKFPGA_TEST_BITFILE`

---

## Overview

This bitfile implements two independent counters controlled by Classic Data Port
endpoints. Counter #1 auto-counts when enabled. Counter #2 is manually controlled
via trigger inputs. Both counters provide comparator outputs via Trigger Out
endpoints.

To reduce the number of endpoints, signals are shared among the counters within
each endpoint type.

---

## Wire In (`0x00`)

Static control signals for both counters. These are wires (not triggers) because
they maintain persistent state.

| Signal | Bit(s) | Description |
|--------|--------|-------------|
| `RESET1` | 0 | When asserted, Counter #1 holds the value `0x00` and does not count. |
| `DISABLE1` | 1 | When asserted, Counter #1 holds its value and does not count. |
| `AUTOCOUNT2` | 2 | When asserted, Counter #2 auto-counts (instead of manual trigger control). |
| *Unused* | 15:3 | |

---

## Trigger In (`0x40`)

One-shot control events for Counter #2. These are triggers (not wires) because
we want single-event behavior — e.g., one count-up per activation.

> **Note:** `RESET2` could have been a wire like `RESET1`, but is implemented as
> a trigger so the user cannot hold it asserted indefinitely.

| Signal | Bit(s) | Description |
|--------|--------|-------------|
| `RESET2` | 0 | When asserted, Counter #2 resets to `0x00`. |
| `UP2` | 1 | When asserted, Counter #2 counts up by one. |
| `DOWN2` | 2 | When asserted, Counter #2 counts down by one. |
| *Unused* | 15:3 | |

---

## Wire Out (`0x20`, `0x21`, `0x22`)

Observable outputs providing counter values and board button status.

| Endpoint | Signal | Description |
|----------|--------|-------------|
| `0x20` | `COUNT1[7:0]` | Counter #1 current value (0x00–0xFF). |
| `0x21` | `COUNT2[7:0]` | Counter #2 current value (0x00–0xFF). |
| `0x22` | `BUTTON[3:0]` | Status of on-board pushbuttons. A pressed button asserts its corresponding bit. |

---

## Trigger Out (`0x60`)

Comparator outputs for Counter #1. These are triggers because they fire as
one-shot events when the counter reaches specific values.

| Signal | Bit(s) | Description |
|--------|--------|-------------|
| `COUNT1EQ00` | 0 | Asserted when Counter #1 value equals `0x00`. |
| `COUNT1EQ80` | 1 | Asserted when Counter #1 value equals `0x80`. |
| *Unused* | 15:2 | |

---

## Trigger Out (`0x61`)

Comparator output for Counter #2.

| Signal | Bit(s) | Description |
|--------|--------|-------------|
| `COUNT2EQFF` | 0 | Asserted when Counter #2 value equals `0xFF`. |
| *Unused* | 15:1 | |

---

## AXI Register Map (via okRegisterBridge)

The same counter logic is also accessible via AXI memory-mapped registers.
Both Classic and AXI paths manipulate the **same** hardware counters.

| Address | Name | Access | Description |
|---------|------|--------|-------------|
| `0x0000` | `CTRL` | R/W | [0] reset1, [1] disable1, [2] autocount2 |
| `0x0020` | `COUNT1` | R | [7:0] Counter #1 value |
| `0x0024` | `COUNT2` | R | [7:0] Counter #2 value |
| `0x0040` | `TRIG_CTRL` | W | [0] reset2, [1] up2, [2] down2 |
| `0x0060` | `TRIG_STATUS` | R | [0] count1==0x00, [1] count1==0x80, [2] count2==0xFF |

---

## References

- [ADR-016 — Hardware Integration Testing](architecture/adr/ADR-016-hardware-integration-testing.md)
- [ADR-023 — GUI Example Clients](architecture/adr/ADR-023-gui-example-clients.md)
- `Original endpoint doc` (local SDK file)
