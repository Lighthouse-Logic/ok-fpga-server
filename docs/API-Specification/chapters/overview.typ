// chapters/overview.typ — Section 2: Server Overview and Operations Reference.
// ref: docs/architecture/overview.md; ADR-001..ADR-034; config.toml;
// Phase 46 (expanded overview chapters §2.1–§2.9).
#import "../lib.typ": *

= Server Overview <overview>

This chapter specifies the server architecture, identity and session lifecycle,
device discovery and board-level management, FPGA configuration, Classic and
AXI data-port operations, real-time events and network discovery, server
service management, and configuration file settings. Together with the
*Resource Model* (@resource-model) and *Access Enforcement* (@enforcement), it
covers every operation in `api/openapi.yaml` and `proto/okfpga/v1/okfpga.proto`.

#include "overview/2-1-architecture.typ"
#include "overview/2-2-identity.typ"
#include "overview/2-3-devices.typ"
#include "overview/2-4-configure.typ"
#include "overview/2-5-classic.typ"
#include "overview/2-6-axi.typ"
#include "overview/2-7-events.typ"
#include "overview/2-8-management.typ"
#include "overview/2-9-config.typ"
