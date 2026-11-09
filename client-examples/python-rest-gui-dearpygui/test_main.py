# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""
Unit tests for the Python REST DearPyGui GUI demo.

Tests cover state management and logic — no GUI window is opened.
DearPyGui is not imported; only the common library data types are tested.
"""

import os
import sys
import pytest

# Add parent to path so we can import the common library.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from python_rest_gui_common.counters_client import (
    CounterState, WireInState, DeviceInfo, ResourceInfo,
)


class TestDearPyGuiAppState:
    """Tests for DearPyGui app state/logic without launching a window."""

    @pytest.fixture
    def app_state(self):
        """Create minimal app state matching DearPyGui demo defaults."""
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
                self.rest_url = "http://localhost:8088"
                self.bitfile_path = "Counters.bit"
        return AppState()

    def test_initial_phase(self, app_state):
        assert app_state.phase == "disconnected"

    def test_initial_wire_in(self, app_state):
        assert app_state.wire_in.to_value() == 0

    def test_initial_counter(self, app_state):
        assert app_state.counter.count1 == 0
        assert app_state.counter.count2 == 0

    def test_phase_transitions(self, app_state):
        for phase in ["connected", "programmed", "registered", "acquired"]:
            app_state.phase = phase
            assert app_state.phase == phase

    def test_wire_in_all_bits(self, app_state):
        app_state.wire_in.reset1 = True
        app_state.wire_in.disable1 = True
        app_state.wire_in.autocount2 = True
        assert app_state.wire_in.to_value() == 0x07

    def test_counter_update(self, app_state):
        app_state.counter = CounterState(count1=0xFF, count2=0x42,
                                          buttons=0x0F, count2_eq_ff=True)
        assert app_state.counter.count1 == 0xFF
        assert app_state.counter.count2_eq_ff

    def test_override_lockout(self, app_state):
        assert not app_state.override_lockout
        app_state.override_lockout = True
        assert app_state.override_lockout

    def test_lockout_logic_no_resource(self, app_state):
        """Not locked out when no resource info."""
        assert not _is_locked_out(app_state)

    def test_lockout_logic_locked_by_other(self, app_state):
        """Locked out when another user holds the lock."""
        app_state.resource_info = ResourceInfo(locked_by="other_user")
        assert _is_locked_out(app_state)

    def test_lockout_logic_locked_by_us(self, app_state):
        """Not locked out when we hold the lock."""
        app_state.resource_info = ResourceInfo(locked_by="Pythos Rester")
        assert not _is_locked_out(app_state, user_id="Pythos Rester")

    def test_lockout_override(self, app_state):
        """Override bypasses lock."""
        app_state.override_lockout = True
        app_state.resource_info = ResourceInfo(locked_by="other_user")
        assert not _is_locked_out(app_state)

    def test_device_selection(self, app_state):
        app_state.devices = [
            DeviceInfo(serial="S1", product_name="XEM7320"),
            DeviceInfo(serial="S2", product_name="XEM8320"),
        ]
        app_state.selected_serial = "S1"
        assert app_state.selected_serial == "S1"

    def test_resource_id_lifecycle(self, app_state):
        assert app_state.resource_id is None
        app_state.resource_id = "res-123"
        assert app_state.resource_id == "res-123"


def _is_locked_out(app_state, user_id="Pythos Rester"):
    """Helper implementing the lockout check logic from the GUI."""
    if app_state.override_lockout:
        return False
    if app_state.resource_info and app_state.resource_info.locked_by:
        return app_state.resource_info.locked_by != user_id
    return False
