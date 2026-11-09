# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""
Unit tests for python_grpc_gui_common.counters_grpc_client.

Tests cover data models only (WireInState, CounterState, DeviceInfo,
ResourceInfo). gRPC client methods require a running server and are
tested via hardware integration tests (Phase 18).
"""

import pytest
from python_grpc_gui_common.counters_grpc_client import (
    CounterState,
    WireInState,
    DeviceInfo,
    ResourceInfo,
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

    def test_individual_flags_independent(self):
        """Verify each flag only affects its own bit."""
        for flag, expected in [("reset1", 0x01), ("disable1", 0x02), ("autocount2", 0x04)]:
            state = WireInState(**{flag: True})
            assert state.to_value() == expected, f"Flag {flag} should produce {expected}"

    def test_combinations(self):
        # reset1 + autocount2 = 0x05
        state = WireInState(reset1=True, autocount2=True)
        assert state.to_value() == 0x05
        # disable1 + autocount2 = 0x06
        state = WireInState(disable1=True, autocount2=True)
        assert state.to_value() == 0x06


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

    def test_trigger_booleans(self):
        state = CounterState(count1_eq_00=True, count2_eq_ff=True)
        assert state.count1_eq_00
        assert not state.count1_eq_80
        assert state.count2_eq_ff


# ============================================================================
# DeviceInfo construction tests
# ============================================================================


class TestDeviceInfo:
    def test_construction(self):
        info = DeviceInfo(serial="SN001", product_name="XEM8320-AU25P")
        assert info.serial == "SN001"
        assert info.product_name == "XEM8320-AU25P"

    def test_defaults(self):
        info = DeviceInfo()
        assert info.serial == ""
        assert info.product_name == ""


# ============================================================================
# ResourceInfo tests
# ============================================================================


class TestResourceInfo:
    def test_locked_by_none(self):
        info = ResourceInfo(id="res-1", name="Test", holders=["user1"])
        assert info.locked_by is None
        assert len(info.holders) == 1

    def test_locked_by_present(self):
        info = ResourceInfo(locked_by="Pythos Garpac")
        assert info.locked_by == "Pythos Garpac"

    def test_empty_holders_default(self):
        info = ResourceInfo()
        assert info.holders == []
        assert info.warnings == []

    def test_warnings_populated(self):
        info = ResourceInfo(warnings=["resource 'test' omits 'endpoints'"])
        assert info.warnings == ["resource 'test' omits 'endpoints'"]

    def test_multiple_holders(self):
        info = ResourceInfo(holders=["user1", "user2", "user3"])
        assert len(info.holders) == 3
        assert "user2" in info.holders
