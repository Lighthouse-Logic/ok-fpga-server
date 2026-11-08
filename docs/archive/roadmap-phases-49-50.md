# Archived Roadmap Phases: 49–50

## Phase 49 — Hardware Integration Verification Pass ✅ (Server `0.3.11`)

- **Date:** 2026-11-08
- **Scope:**
  - Verified all direct FFI and core hardware integration tests against two physical Opal Kelly `XEM7320-A75T` boards (`22500013BJ` and `22520013CC`) connected via USB 3.0 on the hardware test host:
    - `ok-fpga-ffi` (`tests/hardware.rs`): 8/8 passed (`discover_connected_devices`, `enumerate_device_settings`, `enumerate_syzygy_ports`, `load_frontpanel_dll`, `program_example_bitfile`, `read_device_info`, `read_device_sensors`, `warn_if_multithreaded`).
    - `ok-fpga-core` (`tests/device_resource_integration.rs`): 3/3 passed (`test_hardware_xem7320_counters_enforcement_pipeline`, `test_hardware_xem7320_wire_in_auto_update_semantics`, and the new separable dual-board test `test_hardware_dual_xem7320_independent_resources_and_enforcement`).
  - Added separable / single-board-friendly dual-`XEM7320` hardware isolation tests across `ok-fpga-core` and all 4 example clients (`rust-rest-client`, `rust-grpc-client`, `python-rest-client`, `python-grpc-client`). When fewer than 2 physical FPGA boards are connected, these tests skip gracefully so single-board hardware test runs never fail; when 2 boards are present, they verify independent programming, resource lock isolation, and independent counter/trigger state across both boards.
  - Updated all 4 example client libraries (`client-examples/`) for compatibility with `endpoint_enforcement = "enforce"` ([ADR-029](../architecture/adr/ADR-029-user-identification-consistency.md), [ADR-034](../architecture/adr/ADR-034-resource-model-consolidation.md)) and OpenAPI schema alignment (Phase 37):
    - `rust-rest-client` (`0.1.0` -> `0.1.1`): Added `X-User-Id` header support (`with_user_id`, `post_with_user`), `clear_fpga_configuration`, `Counters.bit` `shared` resource declaration (`endpoints` + `address_ranges` hex strings), `ServerStatus` field alignment (`server_version`, `openapi_version`, `uptime_seconds`, `connected_devices`), and `test_22_dual_board_isolation_optional` (24/24 hardware tests passing).
    - `rust-grpc-client` (`0.1.0` -> `0.1.1`): Added `x-user-id` gRPC metadata injection (`with_user_id`), `configure_fpga`, `clear_fpga_configuration`, `get_dataport_type`, `acquire_resource`, `release_resource`, `list_users`, `Counters.bit` `shared` resource declaration, and `test_21_dual_board_isolation_optional` (21/21 hardware tests passing).
    - `python-rest-client` (`0.1.2` -> `0.1.3`): Added default `X-User-Id` client header (`user_id` constructor parameter), `Counters.bit` `shared` resource declaration, offloaded blocking `Zeroconf` mDNS discovery to a worker thread when called inside a running `asyncio` event loop, fixed Counter #2 WireOut endpoint (`0x21`) in trigger tests, and added `test_24_dual_board_isolation_optional` (19 passed, 7 expected hardware skips).
    - `python-grpc-client` (`0.1.3` -> `0.1.4`): Added `x-user-id` gRPC metadata on all RPCs, `configure_fpga`, `clear_fpga_configuration`, `get_dataport_type`, `update=True` on `get_wire_out_value` / `get_trigger_out_vector` (`S-07`), worker-thread mDNS discovery inside `asyncio`, and `test_19_dual_board_isolation_optional` (19 passed, 2 expected AXI skips).

---

## Phase 50 — Purge Tracked Proprietary Opal Kelly FrontPanel SDK/IP Files from Git ✅ (Server `0.3.12`)

- **Date:** 2026-11-08
- **Scope:**
  - Removed all 17 tracked proprietary Opal Kelly FrontPanel 6.1.0 SDK files (`okFrontPanel.h`, `okFrontPanelDLL.h`, `okFrontPanel.dll`, `okFrontPanel.lib`, sample `.bit` files, and `Counters-endpoints.md`) from git tracking and scrubbed them from historical commits via `git filter-repo`.
  - Updated [`.gitignore`](../../.gitignore) so `FrontPanel-6.1.0/*` is gitignored except for repo-authored setup documentation (`FrontPanel-6.1.0/README.md`, `FrontPanel-6.1.0/ExampleBitfiles/README.md`) and Linux USB udev rules (`FrontPanel-6.1.0/60-opalkelly.rules`).
  - Added `OK_FRONTPANEL_SDK_DIR` support in `crates/ok-fpga-ffi/build.rs`, `crates/ok-fpga-server/src/scanner.rs`, `crates/ok-fpga-server/src/device_monitor.rs`, and `crates/ok-fpga-ffi/tests/hardware.rs`, documented in [`FrontPanel-6.1.0/README.md`](../../FrontPanel-6.1.0/README.md) and [`README.md`](../../README.md).
