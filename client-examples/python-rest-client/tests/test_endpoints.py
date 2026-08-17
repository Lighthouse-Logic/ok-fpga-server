"""Tests for the ok-FPGA-Server REST endpoints.

These tests require the server to be running with --features full.
Tests that require an MQTT broker are skipped if unavailable.

Run with: pytest -v
"""

from __future__ import annotations

import pytest
import httpx

from okfpga_rest_client.client import OkFpgaRestClient

# Server base URL — change via OKFPGA_REST_URL env var if needed.
BASE_URL = "http://localhost:8088"


@pytest.fixture
async def client():
    """Create a REST client for tests."""
    async with OkFpgaRestClient(BASE_URL) as c:
        yield c


def is_server_running() -> bool:
    """Check if the server is reachable."""
    try:
        resp = httpx.get(f"{BASE_URL}/server/status", timeout=2.0)
        return resp.status_code == 200
    except Exception:
        return False


# Skip all tests if server is not running.
pytestmark = pytest.mark.skipif(
    not is_server_running(),
    reason=f"ok-FPGA-Server not running at {BASE_URL}",
)


# ============================================================================
# Server Endpoints
# ============================================================================


class TestServerEndpoints:
    """Tests for /server/* endpoints."""

    async def test_get_server_status(self, client: OkFpgaRestClient) -> None:
        """GET /server/status returns expected fields."""
        status = await client.get_server_status()
        assert "api_version" in status
        assert "uptime_secs" in status
        assert "device_count" in status
        assert isinstance(status["uptime_secs"], (int, float))

    async def test_list_services(self, client: OkFpgaRestClient) -> None:
        """GET /server/services returns a list of services."""
        resp = await client.list_services()
        assert "services" in resp
        assert isinstance(resp["services"], list)
        # At least the "rest" service should be present.
        names = [s["name"] for s in resp["services"]]
        assert "rest" in names


# ============================================================================
# Device Endpoints
# ============================================================================


class TestDeviceEndpoints:
    """Tests for /devices/* endpoints."""

    async def test_list_devices(self, client: OkFpgaRestClient) -> None:
        """GET /devices returns a device list."""
        resp = await client.list_devices()
        assert "devices" in resp
        assert isinstance(resp["devices"], list)

    async def test_get_device_info_not_found(self, client: OkFpgaRestClient) -> None:
        """GET /devices/{serial} returns 404 for unknown device."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_device_info("NONEXISTENT")
        assert exc_info.value.response.status_code == 404

    async def test_device_sensors_not_implemented(self, client: OkFpgaRestClient) -> None:
        """GET /devices/{serial}/sensors returns 501 (stub)."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_device_sensors("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_device_settings_not_implemented(self, client: OkFpgaRestClient) -> None:
        """GET /devices/{serial}/settings returns 501 (stub)."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_device_settings("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_device_syzygy_not_implemented(self, client: OkFpgaRestClient) -> None:
        """GET /devices/{serial}/syzygy returns 501 (stub)."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_syzygy_ports("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_configure_fpga_not_implemented(self, client: OkFpgaRestClient) -> None:
        """POST /devices/{serial}/configure returns 501 (stub)."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.configure_fpga("TEST0001", "test.bit")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_bitfile_info_not_implemented(self, client: OkFpgaRestClient) -> None:
        """GET /devices/{serial}/bitfile returns 501 (stub)."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_bitfile_info("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_dataport_type_not_implemented(self, client: OkFpgaRestClient) -> None:
        """GET /devices/{serial}/dataport returns 501 (stub)."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_dataport_type("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)


# ============================================================================
# Classic Data Port Endpoints (all expected to return 501)
# ============================================================================


class TestClassicEndpoints:
    """Tests for /devices/{serial}/classic/* stub endpoints."""

    async def test_set_wire_in(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.set_wire_in("TEST0001", 0x00, 0xFF)
        assert exc_info.value.response.status_code in (404, 501)

    async def test_get_wire_out(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_wire_out("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_activate_trigger_in(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.activate_trigger_in("TEST0001", 0x40, 0)
        assert exc_info.value.response.status_code in (404, 501)

    async def test_get_trigger_out(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_trigger_out("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_write_pipe_in(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.write_pipe_in("TEST0001", 0x80, b"\x00\x01")
        assert exc_info.value.response.status_code in (404, 501)

    async def test_read_pipe_out(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.read_pipe_out("TEST0001", 0xA0, 1024)
        assert exc_info.value.response.status_code in (404, 501)

    async def test_classic_reset(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.classic_reset("TEST0001")
        assert exc_info.value.response.status_code in (404, 501)


# ============================================================================
# AXI Data Port Endpoints (all expected to return 501)
# ============================================================================


class TestAXIEndpoints:
    """Tests for /devices/{serial}/axi/* stub endpoints."""

    async def test_axi_lite_read(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.axi_lite_read("TEST0001", 0x0000)
        assert exc_info.value.response.status_code in (404, 501)

    async def test_axi_lite_write(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.axi_lite_write("TEST0001", 0x0000, 42)
        assert exc_info.value.response.status_code in (404, 501)

    async def test_axi_full_read(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.axi_full_read("TEST0001", 0x0000, 256)
        assert exc_info.value.response.status_code in (404, 501)

    async def test_axi_stream_read(self, client: OkFpgaRestClient) -> None:
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.axi_stream_read("TEST0001", 256)
        assert exc_info.value.response.status_code in (404, 501)


# ============================================================================
# Resource Endpoints
# ============================================================================


class TestResourceEndpoints:
    """Tests for /resources/* endpoints."""

    async def test_list_resources(self, client: OkFpgaRestClient) -> None:
        """GET /resources returns a response (may be empty list or 501)."""
        try:
            resp = await client.list_resources()
            assert isinstance(resp, dict)
        except httpx.HTTPStatusError as e:
            assert e.response.status_code in (501, 404)

    async def test_get_resource_not_found(self, client: OkFpgaRestClient) -> None:
        """GET /resources/{id} returns 404 for unknown resource."""
        with pytest.raises(httpx.HTTPStatusError) as exc_info:
            await client.get_resource("nonexistent-resource")
        assert exc_info.value.response.status_code in (404, 501)


# ============================================================================
# User Endpoints
# ============================================================================


class TestUserEndpoints:
    """Tests for /users/* endpoints."""

    async def test_list_users(self, client: OkFpgaRestClient) -> None:
        """GET /users returns a response."""
        try:
            resp = await client.list_users()
            assert isinstance(resp, dict)
        except httpx.HTTPStatusError as e:
            assert e.response.status_code in (501, 404)
