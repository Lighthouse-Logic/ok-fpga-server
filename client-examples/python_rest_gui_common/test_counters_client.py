# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""
Unit tests for python_rest_gui_common.counters_client.

Tests cover:
- WireInState bitmask computation
- CounterState defaults
- DeviceInfo / ResourceInfo dataclass construction
- ClientError formatting
- CountersClient URL construction
- HTTP request/response handling (using responses mock)
"""

import pytest
import responses
from python_rest_gui_common.counters_client import (
    CounterState,
    WireInState,
    DeviceInfo,
    ResourceInfo,
    ClientError,
    CountersClient,
)


# ============================================================================
# WireInState bitmask tests
# ============================================================================


class TestWireInState:
    def test_default_is_zero(self):
        state = WireInState()
        assert state.to_value() == 0

    def test_reset1_bit(self):
        state = WireInState(reset1=True)
        assert state.to_value() == 0x01

    def test_disable1_bit(self):
        state = WireInState(disable1=True)
        assert state.to_value() == 0x02

    def test_autocount2_bit(self):
        state = WireInState(autocount2=True)
        assert state.to_value() == 0x04

    def test_all_bits_set(self):
        state = WireInState(reset1=True, disable1=True, autocount2=True)
        assert state.to_value() == 0x07

    def test_reset1_disable1_combination(self):
        state = WireInState(reset1=True, disable1=True)
        assert state.to_value() == 0x03

    def test_reset1_autocount2_combination(self):
        state = WireInState(reset1=True, autocount2=True)
        assert state.to_value() == 0x05


# ============================================================================
# CounterState default tests
# ============================================================================


class TestCounterState:
    def test_defaults(self):
        state = CounterState()
        assert state.count1 == 0
        assert state.count2 == 0
        assert state.buttons == 0
        assert not state.count1_eq_00
        assert not state.count1_eq_80
        assert not state.count2_eq_ff

    def test_construction_with_values(self):
        state = CounterState(count1=42, count2=0xFF, buttons=0x0F,
                             count1_eq_00=True, count1_eq_80=False,
                             count2_eq_ff=True)
        assert state.count1 == 42
        assert state.count2 == 0xFF
        assert state.buttons == 0x0F
        assert state.count1_eq_00
        assert not state.count1_eq_80
        assert state.count2_eq_ff


# ============================================================================
# DeviceInfo construction tests
# ============================================================================


class TestDeviceInfo:
    def test_construction(self):
        info = DeviceInfo(serial="ABC123", product_name="XEM7320-A75T",
                          device_interface="usb3", usb_speed="super")
        assert info.serial == "ABC123"
        assert info.product_name == "XEM7320-A75T"
        assert info.device_interface == "usb3"

    def test_defaults(self):
        info = DeviceInfo()
        assert info.serial == ""
        assert info.product_name == ""


# ============================================================================
# ResourceInfo tests
# ============================================================================


class TestResourceInfo:
    def test_locked_by_none(self):
        info = ResourceInfo(id="res-1", name="Test", device_serial="S1",
                            access_mode="shared", holders=["user1"])
        assert info.locked_by is None
        assert len(info.holders) == 1

    def test_locked_by_present(self):
        info = ResourceInfo(id="res-1", name="Test", device_serial="S1",
                            access_mode="shared", locked_by="Icy Rester")
        assert info.locked_by == "Icy Rester"

    def test_empty_holders_default(self):
        info = ResourceInfo()
        assert info.holders == []
        assert info.locked_by is None


# ============================================================================
# ClientError tests
# ============================================================================


class TestClientError:
    def test_error_message_format(self):
        err = ClientError(404, "Not Found")
        assert "404" in str(err)
        assert "Not Found" in str(err)
        assert err.status == 404
        assert err.body == "Not Found"


# ============================================================================
# CountersClient URL construction tests
# ============================================================================


class TestCountersClientURL:
    def test_base_url_no_trailing_slash(self):
        client = CountersClient("http://localhost:8088", "test-user")
        assert client.base_url == "http://localhost:8088/api/v1"

    def test_base_url_with_trailing_slash(self):
        client = CountersClient("http://localhost:8088/", "test-user")
        assert client.base_url == "http://localhost:8088/api/v1"

    def test_user_id_stored(self):
        client = CountersClient("http://localhost:8088", "Pythos Rester")
        assert client.user_id == "Pythos Rester"

    def test_user_id_header_set(self):
        client = CountersClient("http://localhost:8088", "test-user")
        assert client.session.headers["X-User-Id"] == "test-user"


# ============================================================================
# Mock HTTP integration tests (using responses)
# ============================================================================


class TestCountersClientHTTP:
    @responses.activate
    def test_list_devices_parses_response(self):
        # Mock response matches the OpenAPI spec: {"devices": [...]} wrapper
        # with full DeviceInfo fields (per ai-collaboration.md §6).
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/devices",
            json={"devices": [{"serial": "SN001", "product_name": "XEM7320",
                               "device_interface": "usb3", "usb_speed": "super",
                               "is_front_panel_enabled": False}]},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        devices = client.list_devices()
        assert len(devices) == 1
        assert devices[0].serial == "SN001"
        assert devices[0].product_name == "XEM7320"
        assert devices[0].device_interface == "usb3"

    @responses.activate
    def test_list_devices_handles_empty_response(self):
        """Verify that an empty devices list returns an empty list."""
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/devices",
            json={"devices": []}, status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        devices = client.list_devices()
        assert len(devices) == 0

    @responses.activate
    def test_get_server_status_success(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/server/status",
            json={"status": "ok"}, status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        result = client.get_server_status()
        assert result["status"] == "ok"

    @responses.activate
    def test_configure_fpga_sends_bitfile_path_to_server_path_endpoint(self):
        # ADR-028: server-side path configuration uses the dedicated endpoint.
        responses.add(
            responses.POST,
            "http://localhost:8088/api/v1/devices/SN001/configure_from_server_path",
            json={"filename": "Counters.bit", "programmed_by": "test-user"}, status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        client.configure_fpga("SN001", "Counters.bit")
        assert responses.calls[0].request.body is not None
        import json
        body = json.loads(responses.calls[0].request.body)
        assert body["bitfile_path"] == "Counters.bit"

    @responses.activate
    def test_server_error_raises_client_error(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/server/status",
            body="Internal Server Error", status=500,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        with pytest.raises(ClientError) as exc_info:
            client.get_server_status()
        assert exc_info.value.status == 500
        assert "Internal Server Error" in exc_info.value.body

    @responses.activate
    def test_get_wire_out_value_parses_value(self):
        responses.add(
            responses.GET,
            "http://localhost:8088/api/v1/devices/SN001/classic/wires/out",
            json={"value": 42}, status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        value = client.get_wire_out_value("SN001", 0x20)
        assert value == 42

    @responses.activate
    def test_activate_trigger_sends_endpoint_and_bit(self):
        responses.add(
            responses.POST,
            "http://localhost:8088/api/v1/devices/SN001/classic/triggers/in",
            json={}, status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        client.activate_trigger_in("SN001", 0x40, 1)
        import json
        body = json.loads(responses.calls[0].request.body)
        assert body["endpoint"] == 0x40
        assert body["bit"] == 1

    @responses.activate
    def test_create_resource_parses_response(self):
        responses.add(
            responses.POST, "http://localhost:8088/api/v1/resources",
            json={"id": "res-abc", "name": "Counter Resource",
                  "device_serial": "SN001", "access_mode": "shared",
                  "interfaces": ["classic"],
                  "holders": [{"user_id": "test-user", "mode": "shared"}],
                  "locked_by": None,
                  "warnings": ["resource 'Counter Resource' omits 'endpoints'"]},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        info = client.create_resource("Counter Resource", "SN001")
        assert info.id == "res-abc"
        assert info.name == "Counter Resource"
        assert info.access_mode == "shared"
        assert info.holders == ["test-user"]
        assert info.locked_by is None
        assert info.warnings == ["resource 'Counter Resource' omits 'endpoints'"]
        import json
        body = json.loads(responses.calls[0].request.body)
        assert body["interfaces"] == ["classic"]


# ============================================================================
# Phase 25-Fix-F: Tests for new methods (F3/F4/F5)
# ============================================================================


class TestGetDeviceInfo:
    """Tests for get_device_info() — used by F4 smart-skip."""

    @responses.activate
    def test_get_device_info_returns_dict(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/devices/SN001",
            json={"serial": "SN001", "product_name": "XEM7320", "state": "Programmed"},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        info = client.get_device_info("SN001")
        assert isinstance(info, dict)
        assert info["serial"] == "SN001"
        assert info["state"] == "Programmed"

    @responses.activate
    def test_get_device_info_not_programmed(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/devices/SN001",
            json={"serial": "SN001", "product_name": "XEM7320", "state": "NotProgrammed"},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        info = client.get_device_info("SN001")
        assert info.get("state", "") != "Programmed"

    @responses.activate
    def test_get_device_info_404_raises(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/devices/UNKNOWN",
            body="Not found", status=404,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        with pytest.raises(ClientError) as exc_info:
            client.get_device_info("UNKNOWN")
        assert exc_info.value.status == 404


class TestListResources:
    """Tests for list_resources() — used by F3 (409 conflict) and F4 (smart-skip)."""

    @responses.activate
    def test_list_resources_returns_list(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/resources",
            json={"resources": [
                {"id": "res-1", "name": "Counter Resource", "device_serial": "SN001",
                 "access_mode": "shared", "interfaces": ["classic"],
                 "holders": [{"user_id": "user1", "mode": "shared"}], "locked_by": None},
                {"id": "res-2", "name": "Other Resource", "device_serial": "SN002",
                 "access_mode": "exclusive", "interfaces": ["classic"],
                 "holders": [], "locked_by": None},
            ]},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        resources = client.list_resources()
        assert len(resources) == 2
        assert resources[0].id == "res-1"
        assert resources[0].device_serial == "SN001"
        assert resources[0].holders == ["user1"]
        assert resources[1].id == "res-2"

    @responses.activate
    def test_list_resources_empty(self):
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/resources",
            json={"resources": []},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        resources = client.list_resources()
        assert len(resources) == 0

    @responses.activate
    def test_list_resources_can_find_by_device_serial(self):
        """Verify the F3/F4 lookup pattern: find resource by device serial."""
        responses.add(
            responses.GET, "http://localhost:8088/api/v1/resources",
            json={"resources": [
                {"id": "res-1", "name": "Counter", "device_serial": "SN001",
                 "access_mode": "shared", "holders": [], "locked_by": None},
                {"id": "res-2", "name": "Other", "device_serial": "SN002",
                 "access_mode": "shared", "holders": [], "locked_by": None},
            ]},
            status=200,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        resources = client.list_resources()
        # Simulate the F3/F4 lookup pattern used in all demos.
        existing = next((r for r in resources if r.device_serial == "SN001"), None)
        assert existing is not None
        assert existing.id == "res-1"
        # Non-existent serial returns None.
        missing = next((r for r in resources if r.device_serial == "SN999"), None)
        assert missing is None


class TestConfigureFpgaFromFile:
    """Tests for configure_fpga_from_file() -- multipart upload mode (ADR-028)."""

    @responses.activate
    def test_configure_fpga_from_file_sends_multipart(self, tmp_path):
        """ADR-028: Local File mode reads file and uploads as multipart/form-data."""
        responses.add(
            responses.POST,
            "http://localhost:8088/api/v1/devices/SN001/configure",
            json={"filename": "test.bit", "programmed_by": "test-user"}, status=200,
        )
        # Create a small temp bitfile.
        bitfile = tmp_path / "test.bit"
        bitfile.write_bytes(b"\x00\x09\x0f\x01\x00\x00\x01\x0d")

        client = CountersClient("http://localhost:8088", "test-user")
        client.configure_fpga_from_file("SN001", str(bitfile))

        # Verify multipart content type was used.
        content_type = responses.calls[0].request.headers["Content-Type"]
        assert "multipart/form-data" in content_type

    def test_configure_fpga_from_file_missing_file_raises(self):
        """F5: Non-existent local file raises FileNotFoundError."""
        client = CountersClient("http://localhost:8088", "test-user")
        with pytest.raises(FileNotFoundError):
            client.configure_fpga_from_file("SN001", "/no/such/file.bit")


class TestCreateResource409:
    """Tests for F3: 409 Conflict → list_resources fallback."""

    @responses.activate
    def test_create_resource_409_raises_client_error(self):
        """F3: Verify that a 409 response raises ClientError with '409' in message."""
        responses.add(
            responses.POST, "http://localhost:8088/api/v1/resources",
            body='{"error":"resource already exists for device SN001"}',
            status=409,
        )
        client = CountersClient("http://localhost:8088", "test-user")
        with pytest.raises(ClientError) as exc_info:
            client.create_resource("Counter", "SN001")
        assert exc_info.value.status == 409
        assert "409" in str(exc_info.value)
