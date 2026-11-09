# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""Tests for the ok-FPGA-Server gRPC services.

These tests require the server to be running with --features full.
Run with: pytest -v

NOTE: Proto stubs must be generated first via `python generate_protos.py`.
"""

from __future__ import annotations

import pytest
import grpc

# Check if protos are generated before importing.
try:
    from okfpga_grpc_client.client import OkFpgaGrpcClient
    PROTOS_AVAILABLE = True
except ImportError:
    PROTOS_AVAILABLE = False

# gRPC server address.
TARGET = "localhost:50051"


def is_server_running() -> bool:
    """Check if the gRPC server is reachable."""
    if not PROTOS_AVAILABLE:
        return False
    try:
        channel = grpc.insecure_channel(TARGET)
        grpc.channel_ready_future(channel).result(timeout=2)
        channel.close()
        return True
    except Exception:
        return False


pytestmark = [
    pytest.mark.skipif(not PROTOS_AVAILABLE, reason="Proto stubs not generated"),
    pytest.mark.skipif(not is_server_running(), reason=f"gRPC server not running at {TARGET}"),
]


@pytest.fixture
async def client():
    """Create a gRPC client for tests."""
    async with OkFpgaGrpcClient(TARGET) as c:
        yield c


# ============================================================================
# DeviceService RPCs
# ============================================================================


class TestDeviceService:
    """Tests for DeviceService RPCs."""

    async def test_list_devices(self, client: OkFpgaGrpcClient) -> None:
        """ListDevices returns a response."""
        resp = await client.list_devices()
        assert hasattr(resp, "devices")

    async def test_get_device_info_not_found(self, client: OkFpgaGrpcClient) -> None:
        """GetDeviceInfo returns NOT_FOUND for unknown serial."""
        with pytest.raises(grpc.aio.AioRpcError) as exc_info:
            await client.get_device_info("NONEXISTENT")
        assert exc_info.value.code() in (grpc.StatusCode.NOT_FOUND, grpc.StatusCode.UNIMPLEMENTED)


# ============================================================================
# ClassicDataPortService RPCs
# ============================================================================


class TestClassicDataPortService:
    """Tests for ClassicDataPortService RPCs."""

    async def test_set_wire_in_unimplemented(self, client: OkFpgaGrpcClient) -> None:
        """SetWireInValue returns UNIMPLEMENTED for stub."""
        with pytest.raises(grpc.aio.AioRpcError) as exc_info:
            await client.set_wire_in_value("TEST0001", 0x00, 0xFF)
        assert exc_info.value.code() in (
            grpc.StatusCode.NOT_FOUND,
            grpc.StatusCode.UNIMPLEMENTED,
        )

    async def test_activate_trigger_unimplemented(self, client: OkFpgaGrpcClient) -> None:
        """ActivateTriggerIn returns UNIMPLEMENTED for stub."""
        with pytest.raises(grpc.aio.AioRpcError) as exc_info:
            await client.activate_trigger_in("TEST0001", 0x40, 0)
        assert exc_info.value.code() in (
            grpc.StatusCode.NOT_FOUND,
            grpc.StatusCode.UNIMPLEMENTED,
        )


# ============================================================================
# AXIDataPortService RPCs
# ============================================================================


class TestAXIDataPortService:
    """Tests for AXIDataPortService RPCs."""

    async def test_axi_lite_read_unimplemented(self, client: OkFpgaGrpcClient) -> None:
        """AXILiteRead returns UNIMPLEMENTED for stub."""
        with pytest.raises(grpc.aio.AioRpcError) as exc_info:
            await client.axi_lite_read("TEST0001", 0x0000)
        assert exc_info.value.code() in (
            grpc.StatusCode.NOT_FOUND,
            grpc.StatusCode.UNIMPLEMENTED,
        )
