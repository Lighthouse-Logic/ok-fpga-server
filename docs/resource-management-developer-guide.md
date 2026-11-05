# Resource Management — Developer & User Guide (Consolidated into ADR-034)

> **Status:** Consolidated into [ADR-034 — Resource Model, Registration, Acquisition, Locking and Enforcement](architecture/adr/ADR-034-resource-model-consolidation.md) and the [API Specification PDF](API-Specification/ok-fpga-server-api-specification.pdf) (Phase 44-J, 2026-11-05).
> **Decision Log:** [resource-management-recommendations.md](resource-management-recommendations.md) (`D1–D78`).

To prevent documentation drift, the normative resource management model, registration rules, acquisition state machine, lock blocking semantics, and runtime enforcement pipeline now live in **[ADR-034](architecture/adr/ADR-034-resource-model-consolidation.md)** and **[docs/API-Specification/](API-Specification/main.typ)** (`ok-fpga-server-api-specification.pdf`).

## Quick Section Mapping

| Former Guide Section | Consolidated Location |
|---|---|
| **§1 Core concepts & terminology (`T1`, `E1`, `D49`, `D50`, `D64`)** | [ADR-034 §1](architecture/adr/ADR-034-resource-model-consolidation.md) · API Specification §3.1 |
| **§2 Declaring & registering resources (`R1`, `Rule A`, `D55`, `D61`, `D68`, `D69`, `D77`)** | [ADR-034 §2, §4, §10](architecture/adr/ADR-034-resource-model-consolidation.md) · [HDL-to-Resource Mapping Guide](client/hdl-resource-mapping-guide.md) · API Specification §3.2–§3.4 |
| **§3 Acquiring & releasing resources (`A1`, `D25`, `D62`)** | [ADR-034 §3, §5, §11](architecture/adr/ADR-034-resource-model-consolidation.md) · API Specification §3.5 |
| **§4 Locking, blocking & multi-step operations (`B1`–`B4`, `L1`, `D18`, `D35`, `D46`)** | [ADR-034 §6, §12](architecture/adr/ADR-034-resource-model-consolidation.md) · API Specification §3.6 |
| **§5 Runtime enforcement, Read/Write matrices, `ReadOnly` & Opal Kelly snapshot semantics (`B2`, `D75`, `D76`, `D78`, `SD-03`, `SD-04`)** | [ADR-034 §8, §9, §13, §16](architecture/adr/ADR-034-resource-model-consolidation.md) · API Specification §4 |
| **§6 DualPort-Transceiver dual-port `okRegisterBridge` walkthrough (`D4`, `D14`, `D30`)** | API Specification §5 ([`chapters/example-DualPort.typ`](API-Specification/chapters/example-DualPort.typ)) |
| **§7 Lifecycle: reprogram, clear, disconnect & `ResetFPGA` (`D26`, `D56`, `D66`)** | [ADR-034 §7, §14](architecture/adr/ADR-034-resource-model-consolidation.md) · API Specification §3.7 |
| **§8 Dataport normalisation (`legacy` vs `axi`, `D63`, `D70`)** | [ADR-034 §15](architecture/adr/ADR-034-resource-model-consolidation.md) · API Specification §2 |
| **Client usage examples (Python REST, LabVIEW, Rust/Python GUI & CLI)** | [Python REST Client Guide](guides/python-rest-client-guide.md) · [HDL-to-Resource Mapping Guide](client/hdl-resource-mapping-guide.md) |
