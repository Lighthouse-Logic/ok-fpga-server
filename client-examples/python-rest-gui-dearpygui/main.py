#!/usr/bin/env python3
# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
"""
Counters.bit Interactive Demo — Python REST + DearPyGui

User Identity: "Dear Rester"

Usage:
    pip install requests dearpygui
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
from python_rest_gui_common.counters_client import (
    CountersClient, CounterState, WireInState, DeviceInfo, ResourceInfo, ClientError,
)
from python_rest_gui_common.mdns_browser import MdnsBrowser, DiscoveredServer

DEFAULT_REST_URL = "http://localhost:8088"
DEFAULT_BITFILE = (
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit"
)
USER_ID = "Dear Rester"


class CountersApp:
    """Main application class for the DearPyGui GUI."""

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
        self.last_device_refresh = 0.0
        self.rest_url = DEFAULT_REST_URL
        self.bitfile_path = os.environ.get("OKFPGA_TEST_BITFILE", DEFAULT_BITFILE)
        # Phase 30 (ADR-026): True when acquired in read-only mode.
        self.acquired_readonly = False

        # Phase 28A: mDNS discovery.
        self.discovered_servers: list[DiscoveredServer] = []
        self._mdns_browser = MdnsBrowser()
        self._mdns_browser.start()

    def run(self):
        dpg.create_context()
        dpg.create_viewport(title="Counters.bit — Dear Rester (REST + DearPyGui)",
                            width=740, height=580)

        with dpg.window(tag="main_window"):
            dpg.add_text("Counters.bit — Dear Rester (REST)", tag="title")

            # Connection panel.
            with dpg.group(tag="connection_panel"):
                # Phase 28A: mDNS discovered servers.
                import time
                time.sleep(2)  # Brief pause to let mDNS discover.
                self.discovered_servers = self._mdns_browser.get_servers()
                self._mdns_browser.stop()
                if self.discovered_servers:
                    mdns_labels = [
                        f"{s.description} — http://{s.host}:{s.rest_port or s.port}"
                        for s in self.discovered_servers
                    ]
                    dpg.add_combo(mdns_labels, tag="mdns_combo", width=400,
                                  label="Discovered (mDNS)",
                                  default_value="Select a discovered server...",
                                  callback=self._on_mdns_select)

                dpg.add_input_text(label="Server URL", default_value=self.rest_url, tag="url_input", width=400)
                dpg.add_button(label="1. Connect to Server", tag="connect_btn", callback=self._on_connect)
                dpg.add_combo([], tag="device_combo", width=-1, callback=self._on_device_select,
                              default_value="No device selected")
                dpg.add_input_text(label="Bitfile", default_value=self.bitfile_path, tag="bitfile_input", width=400)
                # F5: Bitfile mode toggle.
                with dpg.group(horizontal=True):
                    dpg.add_text("Mode:")
                    dpg.add_radio_button(["Server Path", "Local File"], tag="bitfile_mode",
                                          default_value="Server Path", horizontal=True)
                dpg.add_button(label="2. Program Bitfile", tag="program_btn", callback=self._on_program, enabled=False)
                dpg.add_button(label="3. Register (Shared)", tag="register_btn", callback=lambda: self._on_register("shared"), enabled=False)
                dpg.add_button(label="Register (Exclusive)", tag="register_excl_btn", callback=lambda: self._on_register("exclusive"), enabled=False)
                dpg.add_button(label="4. Acquire (Shared)", tag="acquire_btn", callback=self._on_acquire, enabled=False)
                dpg.add_button(label="Acquire R/O", tag="acquire_ro_btn", callback=self._on_acquire_readonly, enabled=False)

            # Counter panel (hidden initially).
            with dpg.group(tag="counter_panel", show=False):
                with dpg.group(horizontal=True):
                    # Counter #1.
                    with dpg.child_window(width=340, height=200, tag="c1_frame"):
                        dpg.add_text("Counter #1 (Auto-count)", tag="c1_title")
                        dpg.add_text("0x00 (0)", tag="count1_text")
                        dpg.add_progress_bar(default_value=0, tag="count1_bar", width=-1)
                        dpg.add_checkbox(label="RESET1 (hold at 0x00)", tag="reset1_cb", callback=self._on_wire_change)
                        dpg.add_checkbox(label="DISABLE1 (freeze)", tag="disable1_cb", callback=self._on_wire_change)
                        dpg.add_text("Triggers:", tag="trig1_label")
                        dpg.add_text("  ==0x00: ⚫  ==0x80: ⚫", tag="trig1_text")

                    # Counter #2.
                    with dpg.child_window(width=340, height=200, tag="c2_frame"):
                        dpg.add_text("Counter #2 (Manual)", tag="c2_title")
                        dpg.add_text("0x00 (0)", tag="count2_text")
                        dpg.add_progress_bar(default_value=0, tag="count2_bar", width=-1)
                        with dpg.group(horizontal=True):
                            dpg.add_button(label="▲ Up", callback=lambda: self._fire_trigger(0x40, 1))
                            dpg.add_button(label="▼ Down", callback=lambda: self._fire_trigger(0x40, 2))
                            dpg.add_button(label="⟲ Reset", callback=lambda: self._fire_trigger(0x40, 0))
                        dpg.add_checkbox(label="AUTOCOUNT2", tag="autocount2_cb", callback=self._on_wire_change)
                        dpg.add_text("Triggers:", tag="trig2_label")
                        dpg.add_text("  ==0xFF: ⚫", tag="trig2_text")

                # Controls row.
                with dpg.group(horizontal=True):
                    dpg.add_text("Buttons:", tag="buttons_text")
                    dpg.add_text("[1] [2] [3] [4]", tag="buttons_val")
                    dpg.add_text("  Poll:")
                    dpg.add_combo(["100", "250", "500", "1000", "2000"],
                                  default_value="500", tag="poll_combo",
                                  callback=self._on_poll_change, width=80)
                    dpg.add_text("ms")

                with dpg.group(horizontal=True):
                    dpg.add_button(label="🔒 Lock", tag="lock_btn", callback=self._on_lock)
                    dpg.add_checkbox(label="Override lockout", tag="override_cb",
                                    callback=lambda s, d: setattr(self, "override_lockout", d))

            dpg.add_separator()
            dpg.add_text("Activity Log")
            dpg.add_input_text(tag="log_text", multiline=True, readonly=True,
                               width=-1, height=80, default_value="")

        dpg.setup_dearpygui()
        dpg.show_viewport()
        dpg.set_primary_window("main_window", True)

        # Polling loop in background thread.
        self._poll_thread = threading.Thread(target=self._polling_loop, daemon=True)
        self._poll_thread.start()

        dpg.start_dearpygui()

        # Phase 25-Fix-H (F8+F10): Clean disconnect before exit.
        if self.client is not None:
            try:
                self.client.disconnect()
            except Exception:
                pass

        dpg.destroy_context()

    # ========================================================================
    # Event Handlers
    # ========================================================================

    def _on_mdns_select(self, sender, app_data):
        """Handle mDNS server selection — extract URL and fill the URL input."""
        # Extract URL from "description — http://host:port" label.
        parts = app_data.split(" \u2014 ", 1)
        if len(parts) == 2:
            url = parts[1]
            dpg.set_value("url_input", url)
            self.rest_url = url

    def _on_connect(self):
        try:
            self.rest_url = dpg.get_value("url_input")
            self.client = CountersClient(self.rest_url, USER_ID)
            self.client.get_server_status()
            self.devices = self.client.list_devices()
            self._log(f"Connected. {len(self.devices)} device(s)")
            items = [d.display_label() for d in self.devices]  # Phase 34-B
            dpg.configure_item("device_combo", items=items, default_value="No device selected")
            self.selected_serial = ""  # Force user to select
            dpg.configure_item("connect_btn", label="1. Connected \u2713", enabled=False)
            # program_btn stays disabled until device is selected
            self.phase = "connected"
            self.last_device_refresh = time.time()
        except Exception as e:
            self._log_err(f"Connect: {e}")

    def _on_device_select(self, sender, app_data):
        for d in self.devices:
            label = d.display_label()  # Phase 34-B
            if label == app_data:
                self.selected_serial = d.serial
                self._log(f"Selected device: {d.serial}")
                if self.phase == "connected":
                    dpg.configure_item("program_btn", enabled=True)

                # F4: Smart-skip — probe device state on selection.
                try:
                    dev_info = self.client.get_device_info(d.serial)
                    state_str = dev_info.get("state", "") if isinstance(dev_info, dict) else ""
                    is_programmed = "Programmed" in str(state_str)
                    resources = self.client.list_resources()
                    existing = next((r for r in resources if r.device_serial == d.serial), None)
                    if is_programmed and existing:
                        self._log(f"Device already programmed, resource '{existing.name}' available — skipping to Registered")
                        self.resource_id = existing.id
                        self.phase = "registered"
                        dpg.configure_item("program_btn", label="2. Programmed ✓", enabled=False)
                        dpg.configure_item("register_btn", label="3. Registered ✓", enabled=False)
                        dpg.configure_item("register_excl_btn", enabled=False)
                        dpg.configure_item("acquire_btn", enabled=True)
                        dpg.configure_item("acquire_ro_btn", enabled=True)
                    elif is_programmed:
                        self._log("Device already programmed — skipping to Register")
                        self.phase = "programmed"
                        dpg.configure_item("program_btn", label="2. Programmed ✓", enabled=False)
                        dpg.configure_item("register_btn", enabled=True)
                        dpg.configure_item("register_excl_btn", enabled=True)
                except Exception as e:
                    self._log(f"Could not check device state: {e}")
                break

    def _on_program(self):
        try:
            self.bitfile_path = dpg.get_value("bitfile_input")
            mode = dpg.get_value("bitfile_mode")
            # F5: Branch on bitfile mode.
            if mode == "Local File":
                self.client.configure_fpga_from_file(self.selected_serial, self.bitfile_path)
            else:
                self.client.configure_fpga(self.selected_serial, self.bitfile_path)
            self._log(f"FPGA programmed ({mode})")
            dpg.configure_item("program_btn", label="2. Programmed ✓", enabled=False)
            dpg.configure_item("register_btn", enabled=True)
            dpg.configure_item("register_excl_btn", enabled=True)
            self.phase = "programmed"
        except Exception as e:
            self._log_err(f"Program: {e}")

    def _on_register(self, resource_type="shared"):
        try:
            info = self.client.create_resource("Counter Resource", self.selected_serial, resource_type)
            self.resource_id = info.id
            self._log(f"Registered ({resource_type}): {info.id}")
            dpg.configure_item("register_btn", label="3. Registered ✓", enabled=False)
            dpg.configure_item("register_excl_btn", enabled=False)
            dpg.configure_item("acquire_btn", enabled=True)
            dpg.configure_item("acquire_ro_btn", enabled=True)
            self.phase = "registered"
        except Exception as e:
            err_str = str(e)
            # F3: 409 Conflict — resource already exists. Look it up.
            if "409" in err_str or "already exists" in err_str.lower():
                self._log("Resource already exists — looking up...")
                try:
                    resources = self.client.list_resources()
                    existing = next((r for r in resources if r.device_serial == self.selected_serial), None)
                    if existing:
                        self._log(f"Found existing: {existing.id}")
                        self.resource_id = existing.id
                        dpg.configure_item("register_btn", label="3. Registered ✓", enabled=False)
                        dpg.configure_item("register_excl_btn", enabled=False)
                        dpg.configure_item("acquire_btn", enabled=True)
                        dpg.configure_item("acquire_ro_btn", enabled=True)
                        self.phase = "registered"
                    else:
                        self._log_err("No matching resource found")
                except Exception as e2:
                    self._log_err(f"Lookup: {e2}")
            else:
                self._log_err(f"Register: {err_str}")

    def _on_acquire(self):
        try:
            self.acquired_readonly = False
            self.client.acquire_resource(self.resource_id)
            self._log("Acquired (shared) — counter control active")
            self.phase = "acquired"
            dpg.configure_item("connection_panel", show=False)
            dpg.configure_item("counter_panel", show=True)
            self.polling = True
        except Exception as e:
            self._log_err(f"Acquire: {e}")

    def _on_acquire_readonly(self):
        """Acquire the resource in read-only mode (Phase 30, ADR-026)."""
        try:
            self.acquired_readonly = True
            self.client.acquire_resource_readonly(self.resource_id)
            self._log("Acquired (read-only) — counter control active")
            self.phase = "acquired"
            dpg.configure_item("connection_panel", show=False)
            dpg.configure_item("counter_panel", show=True)
            self.polling = True
        except Exception as e:
            self._log_err(f"Acquire R/O: {e}")

    def _on_wire_change(self):
        self.wire_in.reset1 = dpg.get_value("reset1_cb")
        self.wire_in.disable1 = dpg.get_value("disable1_cb")
        self.wire_in.autocount2 = dpg.get_value("autocount2_cb")
        try:
            self.client.send_wire_in(self.selected_serial, self.wire_in)
            self._log(f"WireIn = 0x{self.wire_in.to_value():04X}")
        except Exception as e:
            self._log_err(f"WireIn: {e}")

    def _fire_trigger(self, endpoint, bit):
        try:
            self.client.activate_trigger_in(self.selected_serial, endpoint, bit)
            self._log(f"Trigger 0x{endpoint:02X} bit {bit}")
        except Exception as e:
            self._log_err(f"Trigger: {e}")

    def _on_poll_change(self, sender, app_data):
        self.poll_interval_ms = int(app_data)

    def _on_lock(self):
        try:
            if self.resource_info and self.resource_info.locked_by == USER_ID:
                self.client.unlock_resource(self.resource_id)
                self._log("Unlocked")
            else:
                self.client.lock_resource(self.resource_id)
                self._log("Locked")
        except Exception as e:
            self._log_err(f"Lock: {e}")

    # ========================================================================
    # Polling
    # ========================================================================

    def _polling_loop(self):
        while True:
            if self.polling and self.phase == "acquired":
                try:
                    self.counter = self.client.poll_counter_state(self.selected_serial)
                    if self.resource_id:
                        self.resource_info = self.client.get_resource(self.resource_id)
                    self._update_display()
                except Exception as e:
                    self._log_err(f"Poll: {e}")

            # Device refresh every 5s when connected.
            if self.phase != "disconnected" and self.client is not None:
                if time.time() - self.last_device_refresh >= 5.0:
                    self.last_device_refresh = time.time()
                    try:
                        new_devices = self.client.list_devices()
                        old_serial = self.selected_serial
                        self.devices = new_devices
                        items = [d.display_label() for d in self.devices]  # Phase 34-B
                        dpg.configure_item("device_combo", items=items)
                        # Check if selected device disappeared.
                        if old_serial and not any(d.serial == old_serial for d in new_devices):
                            self.selected_serial = ""
                            dpg.configure_item("device_combo", default_value="No device selected")
                            if self.phase == "acquired":
                                self._log_err(f"Device {old_serial} disconnected from server")
                                self.phase = "connected"
                                self.polling = False
                                self.resource_id = None
                                self.resource_info = None
                                dpg.configure_item("counter_panel", show=False)
                                dpg.configure_item("connection_panel", show=True)
                                dpg.configure_item("program_btn", enabled=False)
                            else:
                                self._log_err(f"Device {old_serial} is no longer available")
                    except Exception as e:
                        self._log_err(f"Device refresh: {e}")

            time.sleep(self.poll_interval_ms / 1000.0)

    def _update_display(self):
        c = self.counter
        dpg.set_value("count1_text", f"0x{c.count1:02X} ({c.count1})")
        dpg.set_value("count1_bar", c.count1 / 255.0)
        dpg.set_value("count2_text", f"0x{c.count2:02X} ({c.count2})")
        dpg.set_value("count2_bar", c.count2 / 255.0)

        eq00 = "🟢" if c.count1_eq_00 else "⚫"
        eq80 = "🟢" if c.count1_eq_80 else "⚫"
        dpg.set_value("trig1_text", f"  ==0x00: {eq00}  ==0x80: {eq80}")

        eqff = "🟢" if c.count2_eq_ff else "⚫"
        dpg.set_value("trig2_text", f"  ==0xFF: {eqff}")

        parts = []
        for i in range(4):
            pressed = (c.buttons >> i) & 1
            parts.append(f"[{i+1}]" if pressed else f" {i+1} ")
        dpg.set_value("buttons_val", " ".join(parts))

        if self.resource_info:
            if self.resource_info.locked_by == USER_ID:
                dpg.configure_item("lock_btn", label="🔓 Unlock")
            else:
                dpg.configure_item("lock_btn", label="🔒 Lock")

    # ========================================================================
    # Logging
    # ========================================================================

    def _log(self, msg):
        ts = datetime.now().strftime("%H:%M:%S")
        current = dpg.get_value("log_text")
        dpg.set_value("log_text", f"{current}[{ts}] {msg}\n")

    def _log_err(self, msg):
        ts = datetime.now().strftime("%H:%M:%S")
        current = dpg.get_value("log_text")
        dpg.set_value("log_text", f"{current}[{ts}] ⚠ {msg}\n")


def main():
    app = CountersApp()
    app.run()


if __name__ == "__main__":
    main()
