Here is a clean, structured template for `development-roadmap.md` designed to maintain high signal-to-noise ratio, prevent task creep, and tie directly into your Architecture Decision Records (ADRs).

---

```markdown
# Project Antigravity — Development Roadmap

> **Status:** Living Document  
> **Last Updated:** 2026-08-18  
> **Architecture Decisions:** All architectural rationale, structural pivots, and superseded designs are documented in [`docs/adr/`](docs/adr/README.md).

---

## 🧭 Horizon Overview

```text
[ ✅ Completed / Archived ] ──▶ [ 🟢 Phase 3: Active ] ──▶ [ 🟡 Phase 4: Next ] ──▶ [ 🔵 Horizons: Future ]

```

| Horizon | Focus Area | Target Milestone | Status |
| --- | --- | --- | --- |
| **Phase 3 (Now)** | gRPC Streaming & Hardware Telemetry Engine | v0.8.0 | 🟢 In Active Development |
| **Phase 4 (Next)** | Multi-Node Testbench Orchestration & Native TLS | v0.9.0 | 🟡 Planned / Scoped |
| **Phase 5+ (Later)** | Distributed Waveform Capture & Web UI Dashboard | v1.0.0+ | 🔵 Exploratory Backlog |

---

## 🟢 Horizon 1: Now (Active Milestone)

### Phase 3: Telemetry & Transport Pipeline (Target: v0.8.0)

*Focus: High-throughput telemetry capture and zero-copy streaming.*

* [ ] **Dual-Stream Telemetry Ingestion**
* Continuous streaming from FPGA FIFO over high-speed interface.
* Implement dynamic backpressure ring-buffer in daemon runtime.
* *Architecture:* See [ADR-0008: Telemetry Stream Buffering](https://www.google.com/search?q=docs/adr/0008-telemetry-stream-buffering.md)


* [ ] **Unified gRPC Interface Layer**
* Implement bidirectional RPC streaming for runtime register polling.
* Client SDK bindings for automation harness.
* *Architecture:* See [ADR-0009: gRPC vs SSE Transport Evaluation](https://www.google.com/search?q=docs/adr/0009-grpc-transport-selection.md)


* [ ] **CI Test Automation Harness**
* Automated integration run against simulated hardware models.



---

## 🟡 Horizon 2: Next (Planned Milestones)

### Phase 4: Multi-Node Orchestration & Security (Target: v0.9.0)

*Focus: Node identity, lab networking security, and multi-chassis synchronization.*

* [ ] **Native In-Process TLS**
* Embed `rustls` configuration blocks into daemon configuration.
* *Architecture:* See [ADR-0011: Native TLS Termination vs Reverse Proxy](https://www.google.com/search?q=docs/adr/0011-native-tls-termination.md)


* [ ] **Multi-Target Device Session Management**
* Shared lock arbitrator for concurrent testbench scripts.
* *Architecture:* See [ADR-0012: Device Lease and Access Arbitration](https://www.google.com/search?q=docs/adr/0012-device-lease-arbitration.md)


* [ ] **Cross-Platform Distribution Packaging**
* Pre-built static `musl` Linux binaries and Windows service installers.



---

## 🔵 Horizon 3: Later (Future / Exploratory Backlog)

Features in this section represent uncommitted research, future expansion, or long-term capabilities:

* **Real-time Logic Analyzer Waveform Viewer (Iced GUI)**
* Native desktop client for raw trace inspection and visual signal integrity verification.


* **FPGA Dynamic Bitstream Reconfiguration Pipeline**
* Safe remote flashing with hardware fallback recovery.
* *Exploratory ADR Needed:* Safe partial-reconfiguration and fallback image mechanisms.


* **REST & OpenAPI Compatibility Gateway**
* Auto-generated lightweight REST proxy layer mapping onto internal gRPC endpoints.



---

## 📦 Milestone Archive & Historical Context

To maintain scannability, completed phases are summarized here and archived in detail in the [Changelog](CHANGELOG.md) and [Archived Roadmaps](https://www.google.com/search?q=docs/archive/roadmap-history.md).

#### Phase 1: Proof of Concept & Hardware Bring-Up (v0.1.0 – v0.4.0)

* **Outcomes:** Initial register read/write bring-up, base USB communication validation, and basic CLI client.
* **Key Architecture Decisions:**
* [ADR-0001: FrontPanel Classic vs AXI Subsystem Evaluation](https://www.google.com/search?q=docs/adr/0001-host-interface-selection.md)
* [ADR-0002: Core Daemon Language Selection (Rust)](https://www.google.com/search?q=docs/adr/0002-rust-runtime-selection.md)



#### Phase 2: Host Interface Refactoring & Register Map Abstraction (v0.5.0 – v0.7.0)

* **Outcomes:** Replaced ad-hoc memory peek/poke with unified 32-bit register space; established thread-safe hardware mutex abstraction.
* **Key Architecture Decisions:**
* [ADR-0004: Memory-Mapped Register Space via okRegisterBridge](https://www.google.com/search?q=docs/adr/0004-register-bridge-architecture.md)
* [ADR-0006: Hardware Synchronization & Concurrency Model](https://www.google.com/search?q=docs/adr/0006-hardware-concurrency.md)