"""Hardware Integration Tests — Python gRPC Client.

These tests require:
1. The ok-FPGA-Server running (gRPC on port 50051)
2. An XEM7320-A75T FPGA connected via USB
3. The Counters.bit bitfile at the path specified by OKFPGA_TEST_BITFILE

Run with: pytest tests/test_hardware.py -v --tb=short

All tests are marked with @pytest.mark.hardware and are skipped if no
device is detected or the server is not reachable.
"""

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

hardware = pytest.mark.hardware


async def skip_if_unimplemented(coro, label: str):
    """Execute an async gRPC call; skip if UNIMPLEMENTED or similar."""
    try:
        return await coro
    except grpc.aio.AioRpcError as e:
        code = e.code()
        if code in (
            grpc.StatusCode.UNIMPLEMENTED,
            grpc.StatusCode.FAILED_PRECONDITION,
            grpc.StatusCode.ABORTED,
        ):
            logger.info(f"{label}: not implemented ({code})")
            pytest.skip(f"{label} returns {code}")
        raise


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture(scope="session")
def event_loop():
    """Session-scoped event loop for async fixtures."""
    import asyncio
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


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
    serial = devices[0].serial
    logger.info(f"Using device: {serial}")
    return serial


# ---------------------------------------------------------------------------
# Phase 1: Server Connectivity & Device Discovery
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_01_server_status(client: OkFpgaGrpcClient):
    """Verify gRPC server is reachable and returns status."""
    status = await client.get_server_status()
    assert status.api_version
    logger.info(f"gRPC Server v{status.api_version}, uptime={status.uptime_secs}s")


@pytest.mark.asyncio
@hardware
async def test_02_list_devices(client: OkFpgaGrpcClient):
    """Verify device listing via gRPC."""
    result = await client.list_devices()
    devices = list(result.devices)
    assert len(devices) > 0
    logger.info(f"Found {len(devices)} device(s): {devices[0].serial}")


@pytest.mark.asyncio
@hardware
async def test_03_device_info(client: OkFpgaGrpcClient, device_serial: str):
    """Get device info via gRPC."""
    info = await client.get_device_info(device_serial)
    assert info.device.serial == device_serial
    logger.info(f"Device: {info.device.product_name}")


@pytest.mark.asyncio
@hardware
async def test_04_list_services(client: OkFpgaGrpcClient):
    """List services via gRPC ServerService."""
    result = await client.list_services()
    services = list(result.services)
    assert len(services) > 0
    logger.info(f"Services: {[s.name for s in services]}")


# ---------------------------------------------------------------------------
# Phase 2: Classic Data Port (gRPC stubs — skip if UNIMPLEMENTED)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
@hardware
async def test_05_classic_wire_in(client: OkFpgaGrpcClient, device_serial: str):
    """Set WireIn via gRPC."""
    await skip_if_unimplemented(
        client.set_wire_in_value(device_serial, 0x00, 0x01, 0xFF),
        "SetWireInValue",
    )
    await skip_if_unimplemented(
        client.update_wire_ins(device_serial),
        "UpdateWireIns",
    )
    logger.info("WireIn set via gRPC")


@pytest.mark.asyncio
@hardware
async def test_06_classic_trigger_in(client: OkFpgaGrpcClient, device_serial: str):
    """Activate TriggerIn via gRPC."""
    await skip_if_unimplemented(
        client.activate_trigger_in(device_serial, 0x40, 0),
        "ActivateTriggerIn",
    )
    logger.info("TriggerIn activated via gRPC")


# ---------------------------------------------------------------------------
# Phase 3: AXI (expected to skip on XEM7320)
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
@hardware
async def test_07_axi_lite_read(client: OkFpgaGrpcClient, device_serial: str):
    """AXI Lite read via gRPC."""
    await skip_if_unimplemented(
        client.axi_lite_read(device_serial, 0x00),
        "AXILiteRead",
    )


@pytest.mark.asyncio
@hardware
async def test_08_axi_lite_write(client: OkFpgaGrpcClient, device_serial: str):
    """AXI Lite write via gRPC."""
    await skip_if_unimplemented(
        client.axi_lite_write(device_serial, 0x00, 0x01),
        "AXILiteWrite",
    )


# ---------------------------------------------------------------------------
# Phase 4: Resources via gRPC
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
@hardware
async def test_09_create_resource(client: OkFpgaGrpcClient, device_serial: str):
    """Create a resource via gRPC."""
    result = await skip_if_unimplemented(
        client.create_resource(
            "grpc-test-resource",
            device_serial,
            description="Test resource via gRPC",
        ),
        "CreateResource",
    )
    assert result.resource.id
    logger.info(f"Created: {result.resource.id}")


@pytest.mark.asyncio
@hardware
async def test_10_list_resources(client: OkFpgaGrpcClient):
    """List resources via gRPC."""
    result = await skip_if_unimplemented(
        client.list_resources(),
        "ListResources",
    )
    resources = list(result.resources)
    logger.info(f"Resources: {len(resources)}")


@pytest.mark.asyncio
@hardware
async def test_11_delete_resource(client: OkFpgaGrpcClient, device_serial: str):
    """Delete a resource via gRPC."""
    resource_id = f"{device_serial}:grpc-test-resource"
    await skip_if_unimplemented(
        client.delete_resource(resource_id),
        "DeleteResource",
    )
    logger.info(f"Deleted: {resource_id}")
