#!/usr/bin/env python3
"""
Counters.bit Interactive Demo — Python gRPC + DearPyGui

User Identity: "Dear Garpac"

Usage:
    pip install grpcio grpcio-tools dearpygui
    cd ../python-grpc-gui-common && python generate_stubs.py
    set OKFPGA_TEST_BITFILE=path/to/Counters.bit  (optional)
    python main.py
"""

import os
import sys
import threading
import time
from datetime import datetime

import dearpygui.dearpygui as dpg

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from python_grpc_gui_common.counters_grpc_client import (
    CountersGrpcClient, CounterState, WireInState, DeviceInfo, ResourceInfo,
)

DEFAULT_GRPC_ADDR = "localhost:50051"
DEFAULT_BITFILE = (
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit"
)
USER_ID = "Dear Garpac"


class CountersApp:
    """Main application class for the gRPC DearPyGui GUI."""

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
        self.polling = False
        self.grpc_addr = DEFAULT_GRPC_ADDR
        self.bitfile_path = os.environ.get("OKFPGA_TEST_BITFILE", DEFAULT_BITFILE)

    def run(self):
        dpg.create_context()
        dpg.create_viewport(title="Counters.bit — Dear Garpac (gRPC + DearPyGui)",
                            width=740, height=580)

        with dpg.window(tag="main_window"):
            dpg.add_text("Counters.bit — Dear Garpac (gRPC)")

            with dpg.group(tag="connection_panel"):
                dpg.add_input_text(label="gRPC Address", default_value=self.grpc_addr, tag="addr_input", width=400)
                dpg.add_button(label="1. Connect", tag="connect_btn", callback=self._on_connect)
                dpg.add_listbox([], tag="device_list", width=400, num_items=3, callback=self._on_device_select)
                dpg.add_input_text(label="Bitfile", default_value=self.bitfile_path, tag="bitfile_input", width=400)
                dpg.add_button(label="2. Program", tag="program_btn", callback=self._on_program, enabled=False)
                dpg.add_button(label="3. Register", tag="register_btn", callback=self._on_register, enabled=False)
                dpg.add_button(label="4. Acquire", tag="acquire_btn", callback=self._on_acquire, enabled=False)

            with dpg.group(tag="counter_panel", show=False):
                with dpg.group(horizontal=True):
                    with dpg.child_window(width=340, height=200):
                        dpg.add_text("Counter #1 (Auto-count)")
                        dpg.add_text("0x00", tag="c1_text")
                        dpg.add_progress_bar(default_value=0, tag="c1_bar", width=-1)
                        dpg.add_checkbox(label="RESET1", tag="reset1_cb", callback=self._on_wire)
                        dpg.add_checkbox(label="DISABLE1", tag="disable1_cb", callback=self._on_wire)
                        dpg.add_text("==0x00: ⚫  ==0x80: ⚫", tag="t1_text")

                    with dpg.child_window(width=340, height=200):
                        dpg.add_text("Counter #2 (Manual)")
                        dpg.add_text("0x00", tag="c2_text")
                        dpg.add_progress_bar(default_value=0, tag="c2_bar", width=-1)
                        with dpg.group(horizontal=True):
                            dpg.add_button(label="▲", callback=lambda: self._fire(0x40, 1))
                            dpg.add_button(label="▼", callback=lambda: self._fire(0x40, 2))
                            dpg.add_button(label="⟲", callback=lambda: self._fire(0x40, 0))
                        dpg.add_checkbox(label="AUTOCOUNT2", tag="auto_cb", callback=self._on_wire)
                        dpg.add_text("==0xFF: ⚫", tag="t2_text")

                with dpg.group(horizontal=True):
                    dpg.add_text("Buttons:", tag="btn_text")
                    dpg.add_text("[1] [2] [3] [4]", tag="btn_val")
                    dpg.add_text("  Poll:")
                    dpg.add_combo(["100", "250", "500", "1000", "2000"], default_value="500",
                                  tag="poll_combo", callback=self._on_poll, width=80)
                    dpg.add_text("ms")

                with dpg.group(horizontal=True):
                    dpg.add_button(label="🔒 Lock", tag="lock_btn", callback=self._on_lock)
                    dpg.add_checkbox(label="Override", tag="override_cb",
                                    callback=lambda s, d: setattr(self, "override_lockout", d))

            dpg.add_separator()
            dpg.add_text("Activity Log")
            dpg.add_input_text(tag="log_text", multiline=True, readonly=True, width=-1, height=80)

        dpg.setup_dearpygui()
        dpg.show_viewport()
        dpg.set_primary_window("main_window", True)

        self._poll_thread = threading.Thread(target=self._polling_loop, daemon=True)
        self._poll_thread.start()

        dpg.start_dearpygui()
        dpg.destroy_context()

    def _on_connect(self):
        try:
            self.grpc_addr = dpg.get_value("addr_input")
            self.client = CountersGrpcClient(self.grpc_addr, USER_ID)
            self.client.get_server_status()
            self.devices = self.client.list_devices()
            self._log(f"Connected. {len(self.devices)} device(s)")
            items = [f"{d.serial} — {d.product_name}" for d in self.devices]
            dpg.configure_item("device_list", items=items)
            if self.devices: self.selected_serial = self.devices[0].serial
            dpg.configure_item("connect_btn", label="1. ✓", enabled=False)
            dpg.configure_item("program_btn", enabled=True)
            self.phase = "connected"
        except Exception as e:
            self._log_err(f"Connect: {e}")

    def _on_device_select(self, s, d):
        for dev in self.devices:
            if f"{dev.serial} — {dev.product_name}" == d:
                self.selected_serial = dev.serial; break

    def _on_program(self):
        try:
            self.bitfile_path = dpg.get_value("bitfile_input")
            self.client.configure_fpga(self.selected_serial, self.bitfile_path)
            self._log("Programmed")
            dpg.configure_item("program_btn", label="2. ✓", enabled=False)
            dpg.configure_item("register_btn", enabled=True); self.phase = "programmed"
        except Exception as e:
            self._log_err(f"Program: {e}")

    def _on_register(self):
        try:
            info = self.client.create_resource("Counter Resource", self.selected_serial)
            self.resource_id = info.id; self._log(f"Registered: {info.id}")
            dpg.configure_item("register_btn", label="3. ✓", enabled=False)
            dpg.configure_item("acquire_btn", enabled=True); self.phase = "registered"
        except Exception as e:
            self._log_err(f"Register: {e}")

    def _on_acquire(self):
        try:
            self.client.acquire_resource(self.resource_id)
            self._log("Acquired"); self.phase = "acquired"
            dpg.configure_item("connection_panel", show=False)
            dpg.configure_item("counter_panel", show=True)
            self.polling = True
        except Exception as e:
            self._log_err(f"Acquire: {e}")

    def _on_wire(self):
        self.wire_in.reset1 = dpg.get_value("reset1_cb")
        self.wire_in.disable1 = dpg.get_value("disable1_cb")
        self.wire_in.autocount2 = dpg.get_value("auto_cb")
        try:
            self.client.send_wire_in(self.selected_serial, self.wire_in)
        except Exception as e:
            self._log_err(f"WireIn: {e}")

    def _fire(self, ep, bit):
        try:
            self.client.activate_trigger_in(self.selected_serial, ep, bit)
        except Exception as e:
            self._log_err(f"Trigger: {e}")

    def _on_poll(self, s, d):
        self.poll_interval_ms = int(d)

    def _on_lock(self):
        try:
            if self.resource_info and self.resource_info.locked_by == USER_ID:
                self.client.unlock_resource(self.resource_id); self._log("Unlocked")
            else:
                self.client.lock_resource(self.resource_id); self._log("Locked")
        except Exception as e:
            self._log_err(f"Lock: {e}")

    def _polling_loop(self):
        while True:
            if self.polling and self.phase == "acquired":
                try:
                    self.counter = self.client.poll_counter_state(self.selected_serial)
                    if self.resource_id:
                        self.resource_info = self.client.get_resource(self.resource_id)
                    self._update()
                except Exception as e:
                    self._log_err(f"Poll: {e}")
            time.sleep(self.poll_interval_ms / 1000.0)

    def _update(self):
        c = self.counter
        dpg.set_value("c1_text", f"0x{c.count1:02X} ({c.count1})")
        dpg.set_value("c1_bar", c.count1 / 255.0)
        dpg.set_value("c2_text", f"0x{c.count2:02X} ({c.count2})")
        dpg.set_value("c2_bar", c.count2 / 255.0)
        dpg.set_value("t1_text", f"  ==0x00: {'🟢' if c.count1_eq_00 else '⚫'}  ==0x80: {'🟢' if c.count1_eq_80 else '⚫'}")
        dpg.set_value("t2_text", f"  ==0xFF: {'🟢' if c.count2_eq_ff else '⚫'}")
        parts = [f"[{i+1}]" if (c.buttons >> i) & 1 else f" {i+1} " for i in range(4)]
        dpg.set_value("btn_val", " ".join(parts))
        if self.resource_info:
            dpg.configure_item("lock_btn", label="🔓 Unlock" if self.resource_info.locked_by == USER_ID else "🔒 Lock")

    def _log(self, msg):
        ts = datetime.now().strftime("%H:%M:%S")
        dpg.set_value("log_text", dpg.get_value("log_text") + f"[{ts}] {msg}\n")

    def _log_err(self, msg):
        ts = datetime.now().strftime("%H:%M:%S")
        dpg.set_value("log_text", dpg.get_value("log_text") + f"[{ts}] ⚠ {msg}\n")


if __name__ == "__main__":
    CountersApp().run()
