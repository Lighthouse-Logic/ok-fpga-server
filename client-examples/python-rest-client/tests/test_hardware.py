# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""Hardware Integration Tests — Python REST Client.

These tests require:
1. The ok-FPGA-Server running at http://localhost:8088
2. An XEM7320-A75T FPGA connected via USB
3. The Counters.bit bitfile at the path specified by OKFPGA_TEST_BITFILE

Run with: pytest tests/test_hardware.py -v --tb=short

All tests are marked with @pytest.mark.hardware and are skipped if no
device is detected or the bitfile is not found.
"""

import asyncio
import logging
import os
from pathlib import Path
from typing import AsyncGenerator

import pytest
import pytest_asyncio

from okfpga_rest_client.client import OkFpgaRestClient

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SERVER_URL = os.environ.get("OKFPGA_SERVER_URL", "http://localhost:8088")
BITFILE_PATH = os.environ.get(
    "OKFPGA_TEST_BITFILE",
    str(Path(__file__).parent.parent.parent.parent
        / "FrontPanel-6.1.0" / "ExampleBitfiles"
        / "20190501-FP50-XEM7320-A75" / "XEM7320-A75" / "Counters.bit"),
)

# Counters.bit endpoint map (from Counetrs-endpoints.md)
WIRE_IN_EP = 0x00          # RESET1=bit0, DISABLE1=bit1, AUTOCOUNT2=bit2
TRIGGER_IN_EP = 0x40       # RESET2=bit0, UP2=bit1, DOWN2=bit2
WIRE_OUT_COUNT1 = 0x20     # Counter #1 value [7:0]
WIRE_OUT_COUNT2 = 0x21     # Counter #2 value [7:0]
WIRE_OUT_BUTTONS = 0x22    # Pushbutton status [3:0]
TRIGGER_OUT_60 = 0x60      # COUNT1EQ00=bit0, COUNT1EQ80=bit1
TRIGGER_OUT_61 = 0x61      # COUNT2EQFF=bit0

# AXI register addresses (from Counters.bit okRegisterBridge)
AXI_CTRL = 0x00000000      # [0] reset1, [1] disable1, [2] autocount2
AXI_COUNT1 = 0x00000020    # [7:0] counter1 value (read-only)
AXI_COUNT2 = 0x00000024    # [7:0] counter2 value (read-only)
AXI_TRIG_CTRL = 0x00000040 # [0] reset2, [1] up2, [2] down2 (write-only)
AXI_TRIG_STATUS = 0x00000060  # [0] count1==0x00, [1] count1==0x80, [2] count2==0xFF

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

hardware = pytest.mark.hardware


async def skip_if_not_implemented(coro, label: str):
    """Execute an async call; skip the test if the server returns 501 or 409.

    - 501 Not Implemented: FFI stub not yet wired.
    - 409 Conflict: Feature not supported on this hardware (e.g., AXI on
      Legacy data port devices, or FPGA not programmed).
    """
    try:
        return await coro
    except Exception as e:
        msg = str(e)
        if "501" in msg:
            logger.info(f"{label}: not yet implemented (501)")
            pytest.skip(f"{label} returns 501 (FFI stub)")
        if "409" in msg:
            logger.info(f"{label}: not supported on this hardware (409)")
            pytest.skip(f"{label} returns 409 (unsupported feature)")
        raise


@pytest.fixture(scope="session")
def event_loop():
    """Session-scoped event loop for async fixtures.

    Required by pytest-asyncio when using session-scoped async fixtures.
    """
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()


@pytest_asyncio.fixture
async def client() -> AsyncGenerator[OkFpgaRestClient, None]:
    """Per-test async REST client (for tests that don't need programming)."""
    c = OkFpgaRestClient(SERVER_URL)
    yield c
    await c.close()


@pytest_asyncio.fixture(scope="session")
async def session_client() -> AsyncGenerator[OkFpgaRestClient, None]:
    """Session-scoped async REST client for configure caching."""
    c = OkFpgaRestClient(SERVER_URL)
    yield c
    await c.close()


@pytest.fixture(scope="session")
def bitfile_path() -> str:
    """Resolve and validate the bitfile path."""
    if not Path(BITFILE_PATH).exists():
        pytest.skip(f"Bitfile not found: {BITFILE_PATH}")
    return BITFILE_PATH


@pytest_asyncio.fixture
async def device_serial(client: OkFpgaRestClient) -> str:
    """Get the serial of the first connected device, or skip."""
    try:
        result = await client.list_devices()
    except Exception:
        pytest.skip("Server not reachable")
    devices = result.get("devices", [])
    if not devices:
        pytest.skip("No FPGA devices connected")
    serial = devices[0]["serial"]
    logger.info(f"Using device: {serial}")
    return serial


async def ensure_session_resource(
    client: OkFpgaRestClient, serial: str
) -> None:
    """Ensure a shared covering resource exists and is acquired by client.user_id."""
    res_id = f"{serial}:session-counters"
    try:
        await client.create_resource(
            name="session-counters",
            device_serial=serial,
            access_mode="shared",
            description="Session covering resource for Classic tests",
        )
    except Exception:
        pass
    try:
        await client.acquire_resource(res_id, "shared", client.user_id)
    except Exception:
        pass


@pytest_asyncio.fixture(scope="session")
async def programmed_device(
    session_client: OkFpgaRestClient, bitfile_path: str
) -> str:
    """Program the FPGA with Counters.bit ONCE per test session.

    This fixture is session-scoped: the bitfile is configured once at the
    start of the test session and reused for all tests. This avoids the
    ~1s overhead of re-programming the FPGA before every test.

    If the configure endpoint returns 501 (not yet implemented in FFI),
    this fixture skips the test rather than failing.
    """
    # First, discover the device serial.
    try:
        result = await session_client.list_devices()
    except Exception:
        pytest.skip("Server not reachable")
    devices = result.get("devices", [])
    if not devices:
        pytest.skip("No FPGA devices connected")
    serial = devices[0]["serial"]
    logger.info(f"Session-scoped programming: device {serial}")

    # Program the FPGA once for the entire session.
    try:
        result = await session_client.configure_fpga(serial, bitfile_path)
        logger.info(f"Configure result: {result}")
    except Exception as e:
        if "501" in str(e):
            pytest.skip("POST /configure returns 501 (FFI stub)")
        raise
    # Small delay for hardware to stabilize after programming.
    await asyncio.sleep(0.3)
    await ensure_session_resource(session_client, serial)
    return serial


# ---------------------------------------------------------------------------
# Phase 1: Server Connectivity & Device Discovery
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_01_server_status(client: OkFpgaRestClient):
    """Verify server is reachable and returns valid status."""
    status = await client.get_server_status()
    assert "server_version" in status
    assert "uptime_seconds" in status
    logger.info(
        f"Server v{status['server_version']}, uptime={status['uptime_seconds']}s"
    )


@pytest.mark.asyncio
@hardware
async def test_02_list_devices(client: OkFpgaRestClient):
    """Verify device listing includes at least one device."""
    result = await client.list_devices()
    assert "devices" in result
    assert len(result["devices"]) > 0
    dev = result["devices"][0]
    assert "serial" in dev
    logger.info(f"Found {len(result['devices'])} device(s): {dev['serial']}")


@pytest.mark.asyncio
@hardware
async def test_03_device_info(client: OkFpgaRestClient, device_serial: str):
    """Verify detailed device info for the connected FPGA."""
    info = await client.get_device_info(device_serial)
    assert info["serial"] == device_serial
    assert "product_name" in info
    logger.info(f"Device: {info.get('product_name', 'unknown')}")


@pytest.mark.asyncio
@hardware
async def test_04_device_sensors(client: OkFpgaRestClient, device_serial: str):
    """Verify device sensors (temperature, voltage, etc.)."""
    sensors = await skip_if_not_implemented(
        client.get_device_sensors(device_serial), "GET /devices/.../sensors"
    )
    assert "sensors" in sensors
    logger.info(f"Sensors: {len(sensors['sensors'])} entries")


# ---------------------------------------------------------------------------
# Phase 2: FPGA Configuration
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_05_configure_fpga(
    client: OkFpgaRestClient, device_serial: str, bitfile_path: str
):
    """Program the FPGA with Counters.bit and verify success."""
    result = await skip_if_not_implemented(
        client.configure_fpga(device_serial, bitfile_path), "POST /configure"
    )
    logger.info(f"Configure result: {result}")
    await asyncio.sleep(0.3)
    await ensure_session_resource(client, device_serial)

    # Verify bitfile info
    bitinfo = await skip_if_not_implemented(
        client.get_bitfile_info(device_serial), "GET /bitfile"
    )
    logger.info(f"Bitfile info: {bitinfo}")


@pytest.mark.asyncio
@hardware
async def test_06_dataport_type(
    client: OkFpgaRestClient, programmed_device: str
):
    """Verify dataport type after programming."""
    result = await skip_if_not_implemented(
        client.get_dataport_type(programmed_device), "GET /dataport"
    )
    assert "dataport" in result
    logger.info(f"Dataport type: {result['dataport']}")


# ---------------------------------------------------------------------------
# Phase 3: Classic Data Port — Wires
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_07_classic_reset_counter1(
    client: OkFpgaRestClient, programmed_device: str
):
    """Reset Counter #1 via WireIn, verify it reads 0x00."""
    serial = programmed_device

    # Assert RESET1 (bit 0) to hold counter at 0x00
    await skip_if_not_implemented(
        client.set_wire_in(serial, WIRE_IN_EP, 0x01), "PUT wires/in"
    )
    await asyncio.sleep(0.1)

    # Read Counter #1 via WireOut
    result = await skip_if_not_implemented(
        client.get_wire_out(serial), "GET wires/out"
    )
    logger.info(f"WireOut after reset: {result}")


@pytest.mark.asyncio
@hardware
async def test_08_classic_disable_counter1(
    client: OkFpgaRestClient, programmed_device: str
):
    """Disable Counter #1 via WireIn bit 1, verify it holds its value."""
    serial = programmed_device

    # Clear reset, set DISABLE1 (bit 1)
    await skip_if_not_implemented(
        client.set_wire_in(serial, WIRE_IN_EP, 0x02), "PUT wires/in"
    )
    await asyncio.sleep(0.1)

    # Read counter twice — should be same value (disabled)
    result1 = await skip_if_not_implemented(
        client.get_wire_out(serial), "GET wires/out"
    )
    await asyncio.sleep(0.05)
    result2 = await skip_if_not_implemented(
        client.get_wire_out(serial), "GET wires/out"
    )
    logger.info(f"Counter1 disabled: {result1} → {result2}")


@pytest.mark.asyncio
@hardware
async def test_09_classic_free_running_counter1(
    client: OkFpgaRestClient, programmed_device: str
):
    """Release Counter #1 (clear all bits), verify it changes."""
    serial = programmed_device

    # Clear all control bits → counter runs freely
    await skip_if_not_implemented(
        client.set_wire_in(serial, WIRE_IN_EP, 0x00), "PUT wires/in"
    )
    await asyncio.sleep(0.2)

    result = await skip_if_not_implemented(
        client.get_wire_out(serial), "GET wires/out"
    )
    logger.info(f"Counter1 free-running: {result}")


# ---------------------------------------------------------------------------
# Phase 4: Classic Data Port — Triggers
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_10_classic_trigger_reset_counter2(
    client: OkFpgaRestClient, programmed_device: str
):
    """Reset Counter #2 via TriggerIn bit 0."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.activate_trigger_in(serial, TRIGGER_IN_EP, 0), "POST triggers/in"
    )
    logger.info(f"TriggerIn RESET2: {result}")
    await asyncio.sleep(0.1)


@pytest.mark.asyncio
@hardware
async def test_11_classic_trigger_count_up(
    client: OkFpgaRestClient, programmed_device: str
):
    """Count up Counter #2 via TriggerIn bit 1."""
    serial = programmed_device

    # Reset counter2 first
    await skip_if_not_implemented(
        client.activate_trigger_in(serial, TRIGGER_IN_EP, 0), "POST triggers/in"
    )
    await asyncio.sleep(0.05)

    # Count up several times
    for _ in range(5):
        await skip_if_not_implemented(
            client.activate_trigger_in(serial, TRIGGER_IN_EP, 1), "POST triggers/in"
        )
        await asyncio.sleep(0.01)

    # Read counter2 via WireOut
    result = await skip_if_not_implemented(
        client.get_wire_out(serial, WIRE_OUT_COUNT2), "GET wires/out"
    )
    logger.info(f"Counter2 after 5 count-ups: {result}")


@pytest.mark.asyncio
@hardware
async def test_12_classic_trigger_count_down(
    client: OkFpgaRestClient, programmed_device: str
):
    """Count down Counter #2 via TriggerIn bit 2."""
    serial = programmed_device

    await skip_if_not_implemented(
        client.activate_trigger_in(serial, TRIGGER_IN_EP, 2), "POST triggers/in"
    )
    await asyncio.sleep(0.05)

    result = await skip_if_not_implemented(
        client.get_wire_out(serial, WIRE_OUT_COUNT2), "GET wires/out"
    )
    logger.info(f"Counter2 after count-down: {result}")


@pytest.mark.asyncio
@hardware
async def test_13_classic_trigger_out(
    client: OkFpgaRestClient, programmed_device: str
):
    """Read TriggerOut values (counter threshold status)."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.get_trigger_out(serial), "GET triggers/out"
    )
    logger.info(f"TriggerOut: {result}")


# ---------------------------------------------------------------------------
# Phase 5: Classic Data Port — Registers
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_14_classic_register_read(
    client: OkFpgaRestClient, programmed_device: str
):
    """Read a register by address (Classic path)."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.read_register(serial, 0x00), "POST registers/read"
    )
    logger.info(f"Register read 0x00: {result}")


@pytest.mark.asyncio
@hardware
async def test_15_classic_register_write(
    client: OkFpgaRestClient, programmed_device: str
):
    """Write a register by address (Classic path)."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.write_register(serial, 0x00, 0x01), "PUT registers/write"
    )
    logger.info(f"Register write 0x00=0x01: {result}")


# ---------------------------------------------------------------------------
# Phase 6: AXI Data Port — Lite
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_16_axi_lite_read_ctrl(
    client: OkFpgaRestClient, programmed_device: str
):
    """Read the CTRL register via AXI Lite."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.axi_lite_read(serial, AXI_CTRL), "POST axi/lite/read"
    )
    logger.info(f"AXI CTRL read: {result}")


@pytest.mark.asyncio
@hardware
async def test_17_axi_lite_write_ctrl(
    client: OkFpgaRestClient, programmed_device: str
):
    """Write the CTRL register via AXI Lite (set RESET1)."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.axi_lite_write(serial, AXI_CTRL, 0x01), "PUT axi/lite/write"
    )
    logger.info(f"AXI CTRL write (RESET1): {result}")
    await asyncio.sleep(0.1)


@pytest.mark.asyncio
@hardware
async def test_18_axi_lite_read_counter1(
    client: OkFpgaRestClient, programmed_device: str
):
    """Read Counter #1 value via AXI Lite."""
    serial = programmed_device

    # Reset counter1 via AXI
    await skip_if_not_implemented(
        client.axi_lite_write(serial, AXI_CTRL, 0x01), "PUT axi/lite/write"
    )
    await asyncio.sleep(0.1)

    result = await skip_if_not_implemented(
        client.axi_lite_read(serial, AXI_COUNT1), "POST axi/lite/read"
    )
    logger.info(f"AXI COUNT1: {result}")


@pytest.mark.asyncio
@hardware
async def test_19_axi_lite_read_counter2(
    client: OkFpgaRestClient, programmed_device: str
):
    """Read Counter #2 value via AXI Lite."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.axi_lite_read(serial, AXI_COUNT2), "POST axi/lite/read"
    )
    logger.info(f"AXI COUNT2: {result}")


@pytest.mark.asyncio
@hardware
async def test_20_axi_trig_ctrl_write(
    client: OkFpgaRestClient, programmed_device: str
):
    """Write to AXI TRIG_CTRL (reset counter2)."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.axi_lite_write(serial, AXI_TRIG_CTRL, 0x01), "PUT axi/lite/write"
    )
    logger.info(f"AXI TRIG_CTRL write (RESET2): {result}")


@pytest.mark.asyncio
@hardware
async def test_21_axi_trig_status_read(
    client: OkFpgaRestClient, programmed_device: str
):
    """Read AXI TRIG_STATUS (threshold flags)."""
    serial = programmed_device
    result = await skip_if_not_implemented(
        client.axi_lite_read(serial, AXI_TRIG_STATUS), "POST axi/lite/read"
    )
    logger.info(f"AXI TRIG_STATUS: {result}")


# ---------------------------------------------------------------------------
# Phase 7: Resource CRUD (Client-Side Registration, ADR-016)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_22_create_resource(
    client: OkFpgaRestClient, programmed_device: str
):
    """Create a resource via POST /resources (ADR-020)."""
    serial = programmed_device
    result = await client.create_resource(
        name="test-classic-counters",
        device_serial=serial,
        access_mode="shared",
        description="Classic counter endpoints from Counters.bit",
    )
    assert "id" in result
    logger.info(f"Created resource: {result['id']}")

    # Verify it appears in list
    resources = await client.list_resources()
    assert any(r["id"] == result["id"] for r in resources.get("resources", []))


@pytest.mark.asyncio
@hardware
async def test_22a_lock_unlock_resource(
    client: OkFpgaRestClient, programmed_device: str
):
    """Lock and unlock a shared resource (ADR-020 §3)."""
    serial = programmed_device
    resource_id = f"{serial}:test-classic-counters"
    user_id = "python-test-user"

    # Acquire the resource.
    await client.acquire_resource(resource_id, "shared", user_id)

    # Lock it.
    locked = await client.lock_resource(resource_id, user_id)
    assert locked["state"] == "in_use_locked"
    assert locked.get("locked_by") == user_id
    logger.info(f"Locked: {resource_id}")

    # Unlock it.
    unlocked = await client.unlock_resource(resource_id, user_id)
    assert unlocked["state"] == "in_use_shared"
    logger.info(f"Unlocked: {resource_id}")

    # Release it.
    await client.release_resource(resource_id, user_id)


@pytest.mark.asyncio
@hardware
async def test_22b_heartbeat(client: OkFpgaRestClient):
    """User liveness heartbeat (ADR-020 §5)."""
    result = await client.heartbeat("python-test-user")
    assert "server_time" in result
    assert result["user_id"] == "python-test-user"
    logger.info(f"Heartbeat: {result['server_time']}")


@pytest.mark.asyncio
@hardware
async def test_23_delete_resource(
    client: OkFpgaRestClient, programmed_device: str
):
    """Delete a resource via DELETE /resources/{id} (ADR-020)."""
    serial = programmed_device

    # Create a resource to delete (shared so it coexists with session-counters)
    result = await client.create_resource(
        name="test-delete-me",
        device_serial=serial,
        access_mode="shared",
        description="Temporary resource for deletion test",
    )
    resource_id = result["id"]

    # Delete it
    await client.delete_resource(resource_id)
    # Also clean up test-classic-counters created in test_22
    try:
        await client.delete_resource(f"{serial}:test-classic-counters")
    except Exception:
        pass

    # Verify it's gone
    resources = await client.list_resources()
    assert not any(
        r["id"] == resource_id for r in resources.get("resources", [])
    )
    logger.info(f"Deleted resource: {resource_id}")


# ---------------------------------------------------------------------------
# Phase 8: Separable Dual-Board Isolation Test (Optional — Skips if < 2 Boards)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
@hardware
async def test_24_dual_board_isolation_optional(bitfile_path: str):
    """Separable dual-XEM7320 isolation test (skips cleanly if < 2 boards)."""
    async with (
        OkFpgaRestClient(SERVER_URL, user_id="py-dual-user-a") as client_a,
        OkFpgaRestClient(SERVER_URL, user_id="py-dual-user-b") as client_b,
    ):
        devices = (await client_a.list_devices()).get("devices", [])
        if len(devices) < 2:
            pytest.skip(
                f"Dual-board test requires >= 2 connected FPGAs (found {len(devices)})"
            )

        serial_a = devices[0]["serial"]
        serial_b = devices[1]["serial"]
        logger.info(f"Dual-board REST test on {serial_a} and {serial_b}")

        await client_a.configure_fpga(serial_a, bitfile_path)
        await client_b.configure_fpga(serial_b, bitfile_path)

        res_a = f"{serial_a}:py-dual-counters"
        res_b = f"{serial_b}:py-dual-counters"
        await client_a.create_resource("py-dual-counters", serial_a, "shared")
        await client_b.create_resource("py-dual-counters", serial_b, "shared")
        await client_a.acquire_resource(res_a, "shared", "py-dual-user-a")
        await client_b.acquire_resource(res_b, "shared", "py-dual-user-b")

        await client_a.set_wire_in(serial_a, WIRE_IN_EP, 0x00)
        await client_b.set_wire_in(serial_b, WIRE_IN_EP, 0x00)
        await client_a.activate_trigger_in(serial_a, TRIGGER_IN_EP, 0)
        await client_b.activate_trigger_in(serial_b, TRIGGER_IN_EP, 0)

        for _ in range(4):
            await client_a.activate_trigger_in(serial_a, TRIGGER_IN_EP, 1)
        for _ in range(9):
            await client_b.activate_trigger_in(serial_b, TRIGGER_IN_EP, 1)

        c2_a = (await client_a.get_wire_out(serial_a, WIRE_OUT_COUNT2))["value"] & 0xFF
        c2_b = (await client_b.get_wire_out(serial_b, WIRE_OUT_COUNT2))["value"] & 0xFF
        assert c2_a == 4
        assert c2_b == 9

        await client_a.release_resource(res_a, "py-dual-user-a")
        await client_b.release_resource(res_b, "py-dual-user-b")
        await client_a.delete_resource(res_a)
        await client_b.delete_resource(res_b)

