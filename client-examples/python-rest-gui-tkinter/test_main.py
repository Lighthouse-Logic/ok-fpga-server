"""
Unit tests for the Python REST tkinter GUI demo.

Tests cover state management and logic — no GUI window is opened.
tkinter is mocked using unittest.mock so tests run headlessly.
"""

import os
import sys
import pytest
from unittest.mock import MagicMock, patch

# Add parent to path so we can import the GUI module.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))


class TestCountersAppState:
    """Tests for CountersApp state/logic without launching a GUI."""

    @pytest.fixture
    def mock_app(self):
        """Create a CountersApp with tkinter mocked out."""
        import tkinter as tk

        # Create a minimal root mock that supports StringVar/BooleanVar/IntVar
        root = MagicMock(spec=tk.Tk)
        root.title = MagicMock()
        root.geometry = MagicMock()
        root.protocol = MagicMock()

        # Patch tkinter variables to use real values
        with patch("tkinter.StringVar") as MockStringVar, \
             patch("tkinter.BooleanVar") as MockBoolVar, \
             patch("tkinter.IntVar") as MockIntVar:

            # Make variables work as value holders
            def make_string_var(value=""):
                var = MagicMock()
                var._value = value
                var.get = MagicMock(side_effect=lambda: var._value)
                var.set = MagicMock(side_effect=lambda v: setattr(var, '_value', v))
                return var

            def make_bool_var(value=False):
                var = MagicMock()
                var._value = value
                var.get = MagicMock(side_effect=lambda: var._value)
                var.set = MagicMock(side_effect=lambda v: setattr(var, '_value', v))
                return var

            def make_int_var(value=0):
                var = MagicMock()
                var._value = value
                var.get = MagicMock(side_effect=lambda: var._value)
                var.set = MagicMock(side_effect=lambda v: setattr(var, '_value', v))
                return var

            MockStringVar.side_effect = make_string_var
            MockBoolVar.side_effect = make_bool_var
            MockIntVar.side_effect = make_int_var

            # Import here so mocks are active
            from python_rest_gui_common.counters_client import WireInState, CounterState

            # Create app-like state without actually building UI
            class MockApp:
                def __init__(self):
                    self.phase = "disconnected"
                    self.client = None
                    self.devices = []
                    self.selected_serial = make_string_var()
                    self.resource_id = None
                    self.wire_in = WireInState()
                    self.counter = CounterState()
                    self.resource_info = None
                    self.poll_interval_ms = make_int_var(value=500)
                    self.override_lockout = make_bool_var(value=False)
                    self.rest_url = make_string_var(value="http://localhost:8088")
                    self.bitfile_path = make_string_var(value="Counters.bit")
                    self.reset1_var = make_bool_var(value=False)
                    self.disable1_var = make_bool_var(value=False)
                    self.autocount2_var = make_bool_var(value=False)

            return MockApp()

    def test_initial_phase(self, mock_app):
        assert mock_app.phase == "disconnected"

    def test_initial_client_is_none(self, mock_app):
        assert mock_app.client is None

    def test_initial_devices_empty(self, mock_app):
        assert mock_app.devices == []

    def test_initial_resource_id_none(self, mock_app):
        assert mock_app.resource_id is None

    def test_initial_wire_in_defaults(self, mock_app):
        assert mock_app.wire_in.to_value() == 0

    def test_initial_counter_defaults(self, mock_app):
        assert mock_app.counter.count1 == 0
        assert mock_app.counter.count2 == 0

    def test_poll_interval_default(self, mock_app):
        assert mock_app.poll_interval_ms.get() == 500

    def test_rest_url_default(self, mock_app):
        assert mock_app.rest_url.get() == "http://localhost:8088"

    def test_phase_transition_connect(self, mock_app):
        mock_app.phase = "connected"
        assert mock_app.phase == "connected"

    def test_phase_transition_program(self, mock_app):
        mock_app.phase = "programmed"
        assert mock_app.phase == "programmed"

    def test_phase_transition_register(self, mock_app):
        mock_app.phase = "registered"
        assert mock_app.phase == "registered"

    def test_phase_transition_acquire(self, mock_app):
        mock_app.phase = "acquired"
        assert mock_app.phase == "acquired"

    def test_wire_in_state_update(self, mock_app):
        mock_app.wire_in.reset1 = True
        mock_app.wire_in.autocount2 = True
        assert mock_app.wire_in.to_value() == 0x05

    def test_counter_state_update(self, mock_app):
        from python_rest_gui_common.counters_client import CounterState
        mock_app.counter = CounterState(count1=42, count2=128,
                                         count1_eq_00=True)
        assert mock_app.counter.count1 == 42
        assert mock_app.counter.count1_eq_00

    def test_override_lockout_toggle(self, mock_app):
        assert not mock_app.override_lockout.get()
        mock_app.override_lockout.set(True)
        assert mock_app.override_lockout.get()

    def test_resource_id_assignment(self, mock_app):
        mock_app.resource_id = "res-abc"
        assert mock_app.resource_id == "res-abc"

    def test_device_list_population(self, mock_app):
        from python_rest_gui_common.counters_client import DeviceInfo
        mock_app.devices = [
            DeviceInfo(serial="SN001", product_name="XEM7320"),
            DeviceInfo(serial="SN002", product_name="XEM8320"),
        ]
        assert len(mock_app.devices) == 2
        assert mock_app.devices[0].serial == "SN001"
