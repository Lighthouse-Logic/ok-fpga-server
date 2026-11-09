# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""Hardware Integration Tests — Python gRPC Client.

These tests require:
1. The ok-FPGA-Server running (gRPC on port 50051, REST on port 8088)
2. An XEM7320-A75T FPGA connected via USB
3. The Counters.bit bitfile at OKFPGA_TEST_BITFILE

Run with: pytest tests/test_hardware.py -v --tb=short
"""

import asyncio
import logging
import os

import pytest
import grpc

from okfpga_grpc_client.client import OkFpgaGrpcClient

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

GRPC_TARGET = os.environ.get("OKFPGA_GRPC_TARGET", "localhost:50051")
REST_URL = os.environ.get("OKFPGA_SERVER_URL", "http://localhost:8088")

WIRE_IN_EP = 0x00
WIRE_OUT_COUNTER1 = 0x20
WIRE_OUT_COUNTER2 = 0x21
TRIGGER_IN_EP = 0x40

BITFILE_PATH = os.environ.get(
    "OKFPGA_TEST_BITFILE",
    os.path.normpath(
        os.path.join(
            os.path.dirname(__file__),
            "..", "..", "..",
            "FrontPanel-6.1.0", "ExampleBitfiles",
            "20190501-FP50-XEM7320-A75", "XEM7320-A75", "Counters.bit",
        )
    ),
)


async def skip_if_unimplemented(coro, label: str):
    """Execute an async gRPC call; skip if UNIMPLEMENTED or FailedPrecondition."""
    try:
        return await coro
    except grpc.aio.AioRpcError as e:
        code = e.code()
        if code in (
            grpc.StatusCode.UNIMPLEMENTED,
            grpc.StatusCode.FAILED_PRECONDITION,
            grpc.StatusCode.ABORTED,
        ):
            pytest.skip(f"{label} returns {code}")
        raise


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture
async def client():
    """Per-test gRPC client."""
    c = OkFpgaGrpcClient(GRPC_TARGET)
    await c.connect()
    yield c
    await c.close()


@pytest.fixture
async def device_serial(client: OkFpgaGrpcClient) -> str:
    """Get serial of first connected device, or skip."""
    try:
        result = await client.list_devices()
    except Exception:
        pytest.skip("gRPC server not reachable")
    devices = list(result.devices)
    if not devices:
        pytest.skip("No FPGA devices connected")
    return devices[0].serial


async def ensure_programmed_and_held(
    client: OkFpgaGrpcClient, serial: str
) -> None:
    """Ensure the FPGA is programmed and client.user_id holds a covering resource."""
    info = await client.get_device_info(serial)
    if not info.device.HasField("bitfile"):
        await client.configure_fpga(serial, BITFILE_PATH)
        await asyncio.sleep(0.3)
    res_id = f"{serial}:session-grpc-counters"
    try:
        await client.create_resource(
            "session-grpc-counters",
            serial,
            description="Session covering resource for gRPC tests",
            access_mode="shared",
        )
    except grpc.aio.AioRpcError:
        pass
    try:
        await client.acquire_resource(res_id, client.user_id, "shared")
    except grpc.aio.AioRpcError:
        pass


# ---------------------------------------------------------------------------
# Phase 1: Server Connectivity & Device Discovery
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_01_server_status(client: OkFpgaGrpcClient):
    """Verify gRPC server is reachable and returns status."""
    status = await client.get_server_status()
    assert status.api_version
    logger.info(f"gRPC Server v{status.api_version}, uptime={status.uptime_secs}s")


@pytest.mark.asyncio
async def test_02_list_devices(client: OkFpgaGrpcClient):
    """Verify device listing via gRPC."""
    result = await client.list_devices()
    devices = list(result.devices)
    assert len(devices) > 0


@pytest.mark.asyncio
async def test_03_device_info(client: OkFpgaGrpcClient, device_serial: str):
    """Get device info via gRPC."""
    info = await client.get_device_info(device_serial)
    assert info.device.serial == device_serial


@pytest.mark.asyncio
async def test_04_list_services(client: OkFpgaGrpcClient):
    """List services via gRPC ServerService."""
    result = await client.list_services()
    services = list(result.services)
    assert len(services) > 0


# ---------------------------------------------------------------------------
# Phase 2: Configure FPGA via gRPC (DeviceService.ConfigureFPGA)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_05_configure_fpga_via_rest(
    client: OkFpgaGrpcClient, device_serial: str
):
    """Configure FPGA via gRPC before Classic gRPC tests."""
    resp = await skip_if_unimplemented(
        client.configure_fpga(device_serial, BITFILE_PATH),
        "ConfigureFPGA",
    )
    logger.info(f"FPGA configured via gRPC: {resp.bitfile.filename}")
    await asyncio.sleep(0.3)
    await ensure_programmed_and_held(client, device_serial)


# ---------------------------------------------------------------------------
# Phase 3: Classic Data Port — Wires
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_06_classic_wire_in_reset(client: OkFpgaGrpcClient, device_serial: str):
    """Set WireIn RESET1."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, WIRE_IN_EP, 0x01, 0xFF),
        "SetWireInValue",
    )
    await client.update_wire_ins(device_serial)
    await asyncio.sleep(0.1)

    resp = await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER1)
    logger.info(f"Counter1 after reset: {resp.value}")


@pytest.mark.asyncio
async def test_07_classic_wire_in_free_run(client: OkFpgaGrpcClient, device_serial: str):
    """Clear controls — counter should free-run."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, WIRE_IN_EP, 0x00, 0xFF),
        "SetWireInValue",
    )
    await client.update_wire_ins(device_serial)
    await asyncio.sleep(0.2)

    val1 = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER1)).value
    await asyncio.sleep(0.2)
    val2 = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER1)).value

    logger.info(f"Free-running: val1={val1}, val2={val2}")
    assert val1 != val2, "counter should be advancing"


@pytest.mark.asyncio
async def test_08_classic_wire_in_disable(client: OkFpgaGrpcClient, device_serial: str):
    """Set DISABLE1 — counter should freeze."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, WIRE_IN_EP, 0x02, 0xFF),
        "SetWireInValue",
    )
    await client.update_wire_ins(device_serial)
    await asyncio.sleep(0.1)

    val1 = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER1)).value
    await asyncio.sleep(0.1)
    val2 = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER1)).value

    logger.info(f"Disabled: val1={val1}, val2={val2}")
    assert val1 == val2, "counter should be frozen when disabled"


# ---------------------------------------------------------------------------
# Phase 4: Classic Data Port — Triggers
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_09_classic_trigger_reset_counter2(client: OkFpgaGrpcClient, device_serial: str):
    """Reset counter2 via TriggerIn."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, WIRE_IN_EP, 0x00, 0xFF),
        "SetWireInValue",
    )
    await client.update_wire_ins(device_serial)
    await client.activate_trigger_in(device_serial, TRIGGER_IN_EP, 0)
    await asyncio.sleep(0.05)

    val = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER2)).value
    logger.info(f"Counter2 after reset: {val}")
    assert val == 0


@pytest.mark.asyncio
async def test_10_classic_trigger_count_up(client: OkFpgaGrpcClient, device_serial: str):
    """Count up 5 times via TriggerIn."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, WIRE_IN_EP, 0x00, 0xFF),
        "SetWireInValue",
    )
    await client.update_wire_ins(device_serial)
    await client.activate_trigger_in(device_serial, TRIGGER_IN_EP, 0)
    await asyncio.sleep(0.05)

    for _ in range(5):
        await client.activate_trigger_in(device_serial, TRIGGER_IN_EP, 1)
    await asyncio.sleep(0.05)

    val = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER2)).value
    logger.info(f"Counter2 after 5 ups: {val}")
    assert val == 5


@pytest.mark.asyncio
async def test_11_classic_trigger_count_down(client: OkFpgaGrpcClient, device_serial: str):
    """Count 10 up + 3 down = 7."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, WIRE_IN_EP, 0x00, 0xFF),
        "SetWireInValue",
    )
    await client.update_wire_ins(device_serial)
    await client.activate_trigger_in(device_serial, TRIGGER_IN_EP, 0)
    await asyncio.sleep(0.05)

    for _ in range(10):
        await client.activate_trigger_in(device_serial, TRIGGER_IN_EP, 1)
    for _ in range(3):
        await client.activate_trigger_in(device_serial, TRIGGER_IN_EP, 2)
    await asyncio.sleep(0.05)

    val = (await client.get_wire_out_value(device_serial, WIRE_OUT_COUNTER2)).value
    logger.info(f"Counter2 after 10 up + 3 down: {val}")
    assert val == 7


@pytest.mark.asyncio
async def test_12_classic_trigger_out_vector(client: OkFpgaGrpcClient, device_serial: str):
    """Read TriggerOut vector."""
    await ensure_programmed_and_held(client, device_serial)
    resp = await skip_if_unimplemented(
        client.get_trigger_out_vector(device_serial, 0x60),
        "GetTriggerOutVector",
    )
    logger.info(f"TriggerOut 0x60: vector={resp.vector}")


# ---------------------------------------------------------------------------
# Phase 5: Classic Data Port — Registers
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_13_classic_register_read(client: OkFpgaGrpcClient, device_serial: str):
    """Read register 0x00."""
    await ensure_programmed_and_held(client, device_serial)
    resp = await skip_if_unimplemented(
        client.read_register(device_serial, 0x00),
        "ReadRegister",
    )
    logger.info(f"Register 0x00: value={resp.value}")


@pytest.mark.asyncio
async def test_14_classic_register_write(client: OkFpgaGrpcClient, device_serial: str):
    """Write register 0x00."""
    await ensure_programmed_and_held(client, device_serial)
    await skip_if_unimplemented(
        client.write_register(device_serial, 0x00, 0x42),
        "WriteRegister",
    )
    resp = await client.read_register(device_serial, 0x00)
    logger.info(f"Register 0x00 after write: {resp.value}")


# ---------------------------------------------------------------------------
# Phase 6: AXI (expected to skip on XEM7320)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_15_axi_lite_read(client: OkFpgaGrpcClient, device_serial: str):
    """AXI Lite read via gRPC."""
    await skip_if_unimplemented(
        client.axi_lite_read(device_serial, 0x00),
        "AXILiteRead",
    )


@pytest.mark.asyncio
async def test_16_axi_lite_write(client: OkFpgaGrpcClient, device_serial: str):
    """AXI Lite write via gRPC."""
    await skip_if_unimplemented(
        client.axi_lite_write(device_serial, 0x00, 0x01),
        "AXILiteWrite",
    )


# ---------------------------------------------------------------------------
# Phase 7: Resources via gRPC
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_17_create_resource(client: OkFpgaGrpcClient, device_serial: str):
    """Create a resource via gRPC (ADR-020)."""
    result = await skip_if_unimplemented(
        client.create_resource(
            "grpc-test-resource",
            device_serial,
            description="Test resource via gRPC",
            access_mode="shared",
        ),
        "CreateResource",
    )
    assert result.resource.id
    logger.info(f"Created: {result.resource.id}")


@pytest.mark.asyncio
async def test_17a_list_resources(client: OkFpgaGrpcClient):
    """List all resources via gRPC."""
    result = await skip_if_unimplemented(
        client.list_resources(),
        "ListResources",
    )
    logger.info(f"Resources: {len(result.resources)}")


@pytest.mark.asyncio
async def test_17b_ping(client: OkFpgaGrpcClient):
    """Heartbeat / liveness ping via gRPC (ADR-020 §5)."""
    result = await skip_if_unimplemented(
        client.ping("python-grpc-test-user"),
        "Ping",
    )
    assert result.server_time
    logger.info(f"Ping: server_time={result.server_time}")


@pytest.mark.asyncio
async def test_18_delete_resource(client: OkFpgaGrpcClient, device_serial: str):
    """Delete a resource via gRPC (ADR-020)."""
    resource_id = f"{device_serial}:grpc-test-resource"
    try:
        await client.delete_resource(resource_id)
        logger.info(f"Deleted: {resource_id}")
    except grpc.aio.AioRpcError as e:
        code = e.code()
        if code in (grpc.StatusCode.NOT_FOUND, grpc.StatusCode.UNIMPLEMENTED):
            pytest.skip(f"DeleteResource: {code}")
        raise


# ---------------------------------------------------------------------------
# Phase 8: Separable Dual-Board Isolation Test (Optional — Skips if < 2 Boards)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
async def test_19_dual_board_isolation_optional():
    """Separable dual-XEM7320 gRPC isolation test (skips if < 2 boards)."""
    async with (
        OkFpgaGrpcClient(GRPC_TARGET, user_id="py-grpc-dual-a") as client_a,
        OkFpgaGrpcClient(GRPC_TARGET, user_id="py-grpc-dual-b") as client_b,
    ):
        devices = list((await client_a.list_devices()).devices)
        if len(devices) < 2:
            pytest.skip(
                f"Dual-board test requires >= 2 connected FPGAs (found {len(devices)})"
            )

        serial_a = devices[0].serial
        serial_b = devices[1].serial
        logger.info(f"Dual-board gRPC test on {serial_a} and {serial_b}")

        await client_a.configure_fpga(serial_a, BITFILE_PATH)
        await client_b.configure_fpga(serial_b, BITFILE_PATH)

        res_a = f"{serial_a}:py-grpc-dual"
        res_b = f"{serial_b}:py-grpc-dual"
        await client_a.create_resource("py-grpc-dual", serial_a, access_mode="shared")
        await client_b.create_resource("py-grpc-dual", serial_b, access_mode="shared")
        await client_a.acquire_resource(res_a, "py-grpc-dual-a", "shared")
        await client_b.acquire_resource(res_b, "py-grpc-dual-b", "shared")

        await client_a.set_wire_in_value(serial_a, WIRE_IN_EP, 0x00, 0xFF)
        await client_a.update_wire_ins(serial_a)
        await client_b.set_wire_in_value(serial_b, WIRE_IN_EP, 0x00, 0xFF)
        await client_b.update_wire_ins(serial_b)

        await client_a.activate_trigger_in(serial_a, TRIGGER_IN_EP, 0)
        await client_b.activate_trigger_in(serial_b, TRIGGER_IN_EP, 0)
        for _ in range(4):
            await client_a.activate_trigger_in(serial_a, TRIGGER_IN_EP, 1)
        for _ in range(9):
            await client_b.activate_trigger_in(serial_b, TRIGGER_IN_EP, 1)

        c2_a = (await client_a.get_wire_out_value(serial_a, WIRE_OUT_COUNTER2)).value & 0xFF
        c2_b = (await client_b.get_wire_out_value(serial_b, WIRE_OUT_COUNTER2)).value & 0xFF
        assert c2_a == 4
        assert c2_b == 9

        await client_a.release_resource(res_a, "py-grpc-dual-a")
        await client_b.release_resource(res_b, "py-grpc-dual-b")
        await client_a.delete_resource(res_a)
        await client_b.delete_resource(res_b)
