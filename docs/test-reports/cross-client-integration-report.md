# Cross-Client Hardware Integration Report (Phase 18F)

**Date:** 2026-08-17
**Hardware:** XEM7320-A75T (serial: 22520013CC)
**Bitfile:** Counters.bit (FP-50, 20190501)
**Dataport:** Classic (okDATAPORT_LEGACY — no AXI support)
**Server:** ok-FPGA-Server v0.1.0

---

## Summary

| Client | Protocol | Tests | Passed | Skipped | Failed | Time |
|--------|----------|-------|--------|---------|--------|------|
| Python REST | HTTP/REST | 23 | 16 | 7 | **0** | 14.93s |
| Rust REST | HTTP/REST | 21 | 21 | 0 | **0** | 18.86s |
| Rust gRPC | gRPC/HTTP2 | 10 | 10 | 0 | **0** | 3.12s |
| Python gRPC | gRPC/HTTP2 | 11 | 6 | 5 | **0** | 0.52s |
| **Total** | | **65** | **53** | **12** | **0** | **37.43s** |

All 12 skips are expected — either AXI not supported on the hardware (409),
sensors unavailable on XEM7320, or gRPC Classic/AXI stubs not yet wired
to FFI (UNIMPLEMENTED).

---

## Coverage Matrix

| Feature | REST (Python) | REST (Rust) | gRPC (Rust) | gRPC (Python) |
|---------|:---:|:---:|:---:|:---:|
| Server Status | PASS | PASS | PASS | PASS |
| List Devices | PASS | PASS | PASS | PASS |
| Device Info | PASS | PASS | PASS | PASS |
| Device Sensors | SKIP | PASS | -- | -- |
| Configure FPGA | PASS | PASS | -- | -- |
| Dataport Type | PASS | PASS | -- | -- |
| Classic Wires | PASS | PASS | SKIP | SKIP |
| Classic Triggers | PASS | PASS | SKIP | SKIP |
| Classic Registers | PASS | PASS | -- | -- |
| AXI Lite | SKIP | PASS | SKIP | SKIP |
| AXI Full | SKIP | -- | -- | -- |
| AXI Stream | SKIP | -- | -- | -- |
| Resources | PASS | PASS | -- | PASS |

Legend: PASS = tested and passing, SKIP = expected skip, -- = not tested yet

---

## Key Observations

1. REST Classic Interface is fully functional — all wire, trigger, and
   register operations work correctly on real hardware.

2. AXI correctly returns 409 on XEM7320 (Legacy dataport). Full AXI
   testing requires SZG-HUB1450 hardware (Gen3-USB).

3. gRPC Classic stubs not yet wired — SetWireInValue, UpdateWireIns,
   ActivateTriggerIn return UNIMPLEMENTED. REST Classic is fully wired.

4. Session-scoped configure caching reduced Python REST test time from
   ~25s to ~15s.

5. Zero failures across all 65 tests — all skips are expected.
