# ok-fpga-server — Product Requirements Document (PRD)

> **Status:** Living Specification v1.0 (`v0.3.12`, API Specification `v1.0.0`, ADR-034)
> **Last Updated:** 2026-11-08
> **License:** Dual-Licensed: GPL-3.0-or-later OR Commercial (Great Bay Synergy LLC d/b/a Lighthouse Logic)
> **Related Docs:** [Architecture Overview](architecture/overview.md) | [Development Roadmap (NNL)](development-roadmap.md) | [API Specification PDF](API-Specification/ok-fpga-server-api-specification.pdf) | [OpenAPI 3.0 Spec](../api/openapi.yaml) | [Proto3 Spec](../proto/okfpga/v1/okfpga.proto)

---

## 1. Executive Summary & Purpose

**`ok-fpga-server`** is a general-purpose, FPGA-design-agnostic Rust service that wraps the Opal Kelly FrontPanel 6.1.0 C/C++ library (`libokFrontPanel.so` / `okFrontPanel.dll`) via FFI and exposes connected Opal Kelly USB 3.0, Gen3-USB, and PCIe FPGA boards (`XEM8305`, `XEM8310`, `XEM7320`, `XEM8320`, `XEM8350`, `XEM8370`, `HUB1450`) over REST (`:8088`), gRPC (`:50051`), Server-Sent Events (SSE), a real-time Web Dashboard (`:9090`), mDNS (`_okfpga._tcp`), and MQTT v5 (`okfpga/server-<hostname>/...`).

It enables multiple concurrent users and client applications to discover, configure, share, lock, and monitor FPGA hardware resources over a network without requiring direct USB/PCIe attachment on every client workstation.

---

## 2. Core Functional Requirements

1. **Device Discovery & Lifecycle ([ADR-002](architecture/adr/ADR-002-device-lifecycle.md), [ADR-018](architecture/adr/ADR-018-device-handle-pool.md), [ADR-028](architecture/adr/ADR-028-configure-fpga-api-alignment.md)):**
   - Automatic hot-plug detection and persistent per-device FFI handle pooling.
   - Bitfile programming from client upload (`POST /devices/{serial}/configure` / `ConfigureFPGAFromMemory`) or server path (`POST /devices/{serial}/configure_from_server_path` / `ConfigureFPGA`), plus configuration clearing (`DELETE /devices/{serial}/configure` / `ClearFPGAConfiguration`).
   - Support for Opal Kelly USB 3.0, Gen3-USB, and PCIe boards (`XEM8305`, `XEM8310`, `XEM7320`, `XEM8320`, `XEM8350`, `XEM8370`, `HUB1450`; USB 2.0 devices are ignored).
2. **Full FrontPanel 6.1.0 Data Port Coverage ([ADR-003](architecture/adr/ADR-003-data-port-abstraction.md), [ADR-017](architecture/adr/ADR-017-ffi-endpoint-wiring.md), [ADR-019](architecture/adr/ADR-019-axi-interface-architecture.md)):**
   - **Classic Data Port (`FPGADataPortClassic`):** `WireIn`, `WireOut`, `TriggerIn`, `TriggerOut` (including atomic `update=true` snapshot latch + read, `D76`, and `mask` defaulting to `0`, `D78`), `PipeIn`, `PipeOut`, `BlockThrottledPipeIn`, `BlockThrottledPipeOut`, `okRegisterBridge` single/bulk reads and writes, and `ResetFPGA`.
   - **AXI Data Port (`FPGADataPortAXI`):** AXI4-Lite, AXI4-Full, and AXI4-Stream endpoints.
   - **Device Peripherals:** Device settings, thermal/voltage sensors, PLL configuration, Flash read/write/erase, and SYZYGY SmartVIO DNA peripheral discovery ([ADR-009](architecture/adr/ADR-009-board-syzygy.md)).
3. **Multi-User Resource Model & Runtime Enforcement ([ADR-034](architecture/adr/ADR-034-resource-model-consolidation.md)):**
   - Client-defined resource declarations (`exclusive` or `shared`) specifying interfaces (`classic`, `axi_lite`, `axi_full`, `axi_stream`), Classic endpoints (`0x00`–`0xBF`), 32-bit `address_ranges`, `private` flags, and `metadata`.
   - Overlap registration rules (`Rule R1` & `Rule A`), idempotent acquisition (`Rule A1`), multi-step transaction locking and non-transitive overlap blocking (`Rule B1`–`B4`, `Rule L1`), and forced cleanup on reprogram/clear/unplug (`D26`).
   - Extensible `ResourceAccessPolicy` trait ([ADR-025](architecture/adr/ADR-025-classic-address-mapping.md)) with `DefaultAccessPolicy` active by default (`NoMapping`). Downstream builds can plug in custom `ResourceAccessPolicy` implementations for design-specific Classic-to-memory address bridges.
4. **Zero-Config & Cross-Subnet Discovery ([ADR-027](architecture/adr/ADR-027-native-mdns-registration.md), [ADR-031](architecture/adr/ADR-031-mqtt-discovery.md), [ADR-032](architecture/adr/ADR-032-mqtt-report-by-exception.md)):**
   - OS-native mDNS (`_okfpga._tcp`) advertising and MQTT v5 retained discovery / report-by-exception status publishing.

---

## 3. Authoritative Specifications

- [api/openapi.yaml](../api/openapi.yaml) — OpenAPI 3.0 REST specification.
- [proto/okfpga/v1/okfpga.proto](../proto/okfpga/v1/okfpga.proto) — Protocol Buffers v3 gRPC specification.
- [docs/API-Specification/ok-fpga-server-api-specification.pdf](API-Specification/ok-fpga-server-api-specification.pdf) — Complete developer reference (`v1.0.0`).
