# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""
Unit tests for the Python gRPC tkinter GUI demo.

Tests cover state management and logic without launching a GUI window.
"""

import os
import sys
import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from python_grpc_gui_common.counters_grpc_client import (
    CounterState, WireInState, DeviceInfo, ResourceInfo,
)


class TestGrpcTkinterAppState:
    """Tests for gRPC tkinter app state/logic."""

    @pytest.fixture
    def state(self):
        class AppState:
            def __init__(self):
                self.phase = "disconnected"
                self.client = None
                self.devices = []
                self.selected_serial = ""
                self.resource_id = None
                self.wire_in = WireInState()
                self.counter = CounterState()
                self.resource_info = None
                self.poll_interval_ms = 500
                self.override_lockout = False
                self.grpc_url = "localhost:50051"
                self.bitfile_path = "Counters.bit"
        return AppState()

    def test_initial_phase(self, state):
        assert state.phase == "disconnected"

    def test_initial_grpc_url(self, state):
        assert state.grpc_url == "localhost:50051"

    def test_wire_in_defaults(self, state):
        assert state.wire_in.to_value() == 0

    def test_counter_defaults(self, state):
        assert state.counter.count1 == 0
        assert not state.counter.count1_eq_00

    def test_phase_sequence(self, state):
        phases = ["connected", "programmed", "registered", "acquired"]
        for p in phases:
            state.phase = p
            assert state.phase == p

    def test_wire_in_bitmask(self, state):
        state.wire_in.reset1 = True
        state.wire_in.autocount2 = True
        assert state.wire_in.to_value() == 0x05

    def test_counter_state_update(self, state):
        state.counter = CounterState(count1=42, count2=0xFF, count2_eq_ff=True)
        assert state.counter.count1 == 42
        assert state.counter.count2_eq_ff

    def test_lockout_no_resource(self, state):
        assert not _is_locked_out(state)

    def test_lockout_by_other(self, state):
        state.resource_info = ResourceInfo(locked_by="other")
        assert _is_locked_out(state)

    def test_lockout_by_us(self, state):
        state.resource_info = ResourceInfo(locked_by="Pythos Garpac")
        assert not _is_locked_out(state, user_id="Pythos Garpac")

    def test_lockout_override(self, state):
        state.override_lockout = True
        state.resource_info = ResourceInfo(locked_by="other")
        assert not _is_locked_out(state)

    def test_device_list(self, state):
        state.devices = [DeviceInfo(serial="S1", product_name="P1")]
        assert len(state.devices) == 1

    def test_resource_id(self, state):
        state.resource_id = "res-xyz"
        assert state.resource_id == "res-xyz"


def _is_locked_out(state, user_id="Pythos Garpac"):
    if state.override_lockout:
        return False
    if state.resource_info and state.resource_info.locked_by:
        return state.resource_info.locked_by != user_id
    return False
