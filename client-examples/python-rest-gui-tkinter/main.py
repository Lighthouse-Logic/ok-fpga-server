#!/usr/bin/env python3
"""
Counters.bit Interactive Demo — Python REST + tkinter

User Identity: "Pythos Rester"

Usage:
    pip install requests
    set OKFPGA_TEST_BITFILE=path/to/Counters.bit  (optional)
    python main.py
"""

import os
import sys
import threading
import tkinter as tk
from tkinter import ttk, messagebox
from datetime import datetime

# Add common library to path.
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from python_rest_gui_common.counters_client import (
    CountersClient, CounterState, WireInState, DeviceInfo, ResourceInfo, ClientError,
)
from python_rest_gui_common.mdns_browser import MdnsBrowser, DiscoveredServer

DEFAULT_REST_URL = "http://localhost:8088"
DEFAULT_BITFILE = (
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit"
)
USER_ID = "Pythos Rester"


class CountersApp:
    """Main application class for the tkinter GUI."""

    def __init__(self, root: tk.Tk):
        self.root = root
        self.root.title("Counters.bit — Pythos Rester (REST + tkinter)")
        self.root.geometry("720x560")
        self.root.protocol("WM_DELETE_WINDOW", self.on_close)

        # State.
        self.phase = "disconnected"
        self.client = None
        self.devices = []
        self.selected_serial = tk.StringVar()
        self.resource_id = None
        self.wire_in = WireInState()
        self.counter = CounterState()
        self.resource_info = None
        self.poll_interval_ms = tk.IntVar(value=500)
        self.override_lockout = tk.BooleanVar(value=False)
        self.rest_url = tk.StringVar(value=DEFAULT_REST_URL)
        self.bitfile_path = tk.StringVar(
            value=os.environ.get("OKFPGA_TEST_BITFILE", DEFAULT_BITFILE)
        )
        # F5: Bitfile mode — "server_path" or "local_file".
        self.bitfile_mode = tk.StringVar(value="server_path")
        # Phase 30 (ADR-026): True when acquired in read-only mode.
        self.acquired_readonly = False

        # Tkinter variables for checkboxes.
        self.reset1_var = tk.BooleanVar(value=False)
        self.disable1_var = tk.BooleanVar(value=False)
        self.autocount2_var = tk.BooleanVar(value=False)

        self._build_ui()
        self._polling_id = None
        self._device_refresh_id = None

        # Phase 28A: mDNS discovery.
        self.discovered_servers: list[DiscoveredServer] = []
        self._mdns_browser = MdnsBrowser()
        self._mdns_browser.start()
        # Schedule mDNS results after 2s.
        self.root.after(2000, self._populate_mdns)

    def _build_ui(self):
        """Build the tkinter UI."""
        # Main frame.
        main = ttk.Frame(self.root, padding=8)
        main.pack(fill=tk.BOTH, expand=True)

        # Title.
        ttk.Label(main, text="Counters.bit — Pythos Rester (REST)", font=("", 14, "bold")).pack(anchor=tk.W)
        ttk.Separator(main, orient=tk.HORIZONTAL).pack(fill=tk.X, pady=4)

        # Content area — switches between connection and counter panels.
        self.content_frame = ttk.Frame(main)
        self.content_frame.pack(fill=tk.BOTH, expand=True)

        ttk.Separator(main, orient=tk.HORIZONTAL).pack(fill=tk.X, pady=4)

        # Log area.
        log_frame = ttk.LabelFrame(main, text="Activity Log", padding=4)
        log_frame.pack(fill=tk.X)
        self.log_text = tk.Text(log_frame, height=5, state=tk.DISABLED, bg="#1e1e1e", fg="#b0b0b0", font=("Consolas", 9))
        self.log_text.pack(fill=tk.X)

        self._show_connection_panel()

    def _clear_content(self):
        for w in self.content_frame.winfo_children():
            w.destroy()

    def _show_connection_panel(self):
        self._clear_content()
        f = self.content_frame

        # Phase 28A: mDNS discovered servers.
        self.mdns_row = ttk.Frame(f); self.mdns_row.pack(fill=tk.X, pady=2)
        ttk.Label(self.mdns_row, text="Discovered (mDNS):").pack(side=tk.LEFT)
        self.mdns_combo = ttk.Combobox(self.mdns_row, state="readonly", width=50)
        self.mdns_combo.set("Scanning...")
        self.mdns_combo.pack(side=tk.LEFT, padx=4)
        self.mdns_combo.bind("<<ComboboxSelected>>", self._on_mdns_select)

        # URL.
        row = ttk.Frame(f); row.pack(fill=tk.X, pady=2)
        ttk.Label(row, text="Server URL:").pack(side=tk.LEFT)
        ttk.Entry(row, textvariable=self.rest_url, width=40).pack(side=tk.LEFT, padx=4)

        # Connect button.
        self.connect_btn = ttk.Button(f, text="1. Connect to Server", command=self._on_connect)
        self.connect_btn.pack(anchor=tk.W, pady=2)

        # Device selection dropdown.
        dev_row = ttk.Frame(f); dev_row.pack(fill=tk.X, pady=2)
        ttk.Label(dev_row, text="Select Device:").pack(side=tk.LEFT)
        self.device_combo = ttk.Combobox(dev_row, state="readonly", width=100)
        self.device_combo.set("No device selected")
        self.device_combo.pack(side=tk.LEFT, padx=4)
        self.device_combo.bind("<<ComboboxSelected>>", self._on_device_select)

        # Bitfile.
        row2 = ttk.Frame(f); row2.pack(fill=tk.X, pady=2)
        ttk.Label(row2, text="Bitfile:").pack(side=tk.LEFT)
        ttk.Entry(row2, textvariable=self.bitfile_path, width=60).pack(side=tk.LEFT, padx=4)

        # F5: Bitfile mode toggle.
        mode_row = ttk.Frame(f); mode_row.pack(fill=tk.X, pady=2)
        ttk.Label(mode_row, text="Mode:").pack(side=tk.LEFT)
        ttk.Radiobutton(mode_row, text="Server Path", variable=self.bitfile_mode, value="server_path").pack(side=tk.LEFT, padx=4)
        ttk.Radiobutton(mode_row, text="Local File", variable=self.bitfile_mode, value="local_file").pack(side=tk.LEFT, padx=4)

        self.program_btn = ttk.Button(f, text="2. Program Bitfile", command=self._on_program, state=tk.DISABLED)
        self.program_btn.pack(anchor=tk.W, pady=2)
        self.register_btn = ttk.Button(f, text="3. Register (Shared)", command=lambda: self._on_register("shared"), state=tk.DISABLED)
        self.register_btn.pack(anchor=tk.W, pady=2, side=tk.LEFT)
        self.register_excl_btn = ttk.Button(f, text="Register (Exclusive)", command=lambda: self._on_register("exclusive"), state=tk.DISABLED)
        self.register_excl_btn.pack(anchor=tk.W, pady=2, side=tk.LEFT, padx=4)
        self.acquire_btn = ttk.Button(f, text="4. Acquire (Shared)", command=self._on_acquire, state=tk.DISABLED)
        self.acquire_btn.pack(anchor=tk.W, pady=2, side=tk.LEFT)
        self.acquire_ro_btn = ttk.Button(f, text="Acquire R/O", command=self._on_acquire_readonly, state=tk.DISABLED)
        self.acquire_ro_btn.pack(anchor=tk.W, pady=2, side=tk.LEFT, padx=4)

    def _show_counter_panel(self):
        self._clear_content()
        f = self.content_frame

        # Phase 30: ReadOnly banner.
        if self.acquired_readonly:
            ro_label = ttk.Label(f, text="\U0001f4d6 Read-Only Mode \u2014 write controls disabled",
                                foreground="goldenrod")
            ro_label.pack(anchor=tk.W, pady=(0, 4))

        # Two columns for counters.
        cols = ttk.Frame(f)
        cols.pack(fill=tk.BOTH, expand=True)

        # Counter #1.
        c1 = ttk.LabelFrame(cols, text="Counter #1 (Auto-count)", padding=4)
        c1.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=(0, 4))
        self.count1_label = ttk.Label(c1, text="0x00 (0)", font=("Consolas", 16))
        self.count1_label.pack()
        self.count1_bar = ttk.Progressbar(c1, maximum=255)
        self.count1_bar.pack(fill=tk.X, pady=4)
        ttk.Checkbutton(c1, text="RESET1 (hold at 0x00)", variable=self.reset1_var, command=self._on_wire_change).pack(anchor=tk.W)
        ttk.Checkbutton(c1, text="DISABLE1 (freeze)", variable=self.disable1_var, command=self._on_wire_change).pack(anchor=tk.W)
        self.trig1_label = ttk.Label(c1, text="==0x00: ⚫  ==0x80: ⚫")
        self.trig1_label.pack(anchor=tk.W, pady=4)

        # Counter #2.
        c2 = ttk.LabelFrame(cols, text="Counter #2 (Manual)", padding=4)
        c2.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=(4, 0))
        self.count2_label = ttk.Label(c2, text="0x00 (0)", font=("Consolas", 16))
        self.count2_label.pack()
        self.count2_bar = ttk.Progressbar(c2, maximum=255)
        self.count2_bar.pack(fill=tk.X, pady=4)
        btn_row = ttk.Frame(c2); btn_row.pack()
        ttk.Button(btn_row, text="▲ Up", command=lambda: self._fire_trigger(0x40, 1)).pack(side=tk.LEFT, padx=2)
        ttk.Button(btn_row, text="▼ Down", command=lambda: self._fire_trigger(0x40, 2)).pack(side=tk.LEFT, padx=2)
        ttk.Button(btn_row, text="⟲ Reset", command=lambda: self._fire_trigger(0x40, 0)).pack(side=tk.LEFT, padx=2)
        ttk.Checkbutton(c2, text="AUTOCOUNT2", variable=self.autocount2_var, command=self._on_wire_change).pack(anchor=tk.W)
        self.trig2_label = ttk.Label(c2, text="==0xFF: ⚫")
        self.trig2_label.pack(anchor=tk.W, pady=4)

        # Bottom bar.
        bar = ttk.Frame(f)
        bar.pack(fill=tk.X, pady=4)
        self.buttons_label = ttk.Label(bar, text="Buttons: [1] [2] [3] [4]")
        self.buttons_label.pack(side=tk.LEFT)
        ttk.Label(bar, text="  Poll:").pack(side=tk.LEFT)
        poll_combo = ttk.Combobox(bar, textvariable=self.poll_interval_ms, values=[100, 250, 500, 1000, 2000], width=6)
        poll_combo.pack(side=tk.LEFT)
        ttk.Label(bar, text="ms").pack(side=tk.LEFT)

        lock_frame = ttk.Frame(f)
        lock_frame.pack(fill=tk.X)
        self.lock_btn = ttk.Button(lock_frame, text="🔒 Lock", command=self._on_lock)
        self.lock_btn.pack(side=tk.LEFT)
        ttk.Checkbutton(lock_frame, text="Override lockout", variable=self.override_lockout).pack(side=tk.LEFT, padx=8)

        # Start polling.
        self._start_polling()

    # ========================================================================
    # Event Handlers
    # ========================================================================

    def _populate_mdns(self):
        """Populate the mDNS combo after discovery period."""
        self.discovered_servers = self._mdns_browser.get_servers()
        self._mdns_browser.stop()
        if self.discovered_servers:
            labels = [
                f"{s.description} \u2014 http://{s.host}:{s.rest_port or s.port}"
                for s in self.discovered_servers
            ]
            self.mdns_combo["values"] = labels
            self.mdns_combo.set("Select a discovered server...")
            self._log(f"mDNS: discovered {len(self.discovered_servers)} server(s)")
        else:
            self.mdns_combo.set("No servers found")

    def _on_mdns_select(self, event):
        """Handle mDNS server selection — extract URL and fill the URL entry."""
        selected = self.mdns_combo.get()
        parts = selected.split(" \u2014 ", 1)
        if len(parts) == 2:
            self.rest_url.set(parts[1])

    def _on_connect(self):
        try:
            self.client = CountersClient(self.rest_url.get(), USER_ID)
            self.client.get_server_status()
            self.devices = self.client.list_devices()
            self._log(f"Connected. {len(self.devices)} device(s)")
            self._update_device_combo()
            self.selected_serial.set("")  # Force user to select
            self.connect_btn.config(text="1. Connected ✓", state=tk.DISABLED)
            # program_btn stays DISABLED until device is selected
            self.phase = "connected"
            self._start_device_refresh()
        except Exception as e:
            self._log_err(f"Connect failed: {e}")

    def _update_device_combo(self):
        """Refresh the device dropdown values."""
        items = [d.display_label() for d in self.devices]  # Phase 34-B
        self.device_combo["values"] = items
        # If current selection is no longer in the list, reset.
        current = self.selected_serial.get()
        if current and not any(d.serial == current for d in self.devices):
            self.device_combo.set("No device selected")
            self.selected_serial.set("")

    def _on_device_select(self, event=None):
        idx = self.device_combo.current()
        if 0 <= idx < len(self.devices):
            serial = self.devices[idx].serial
            self.selected_serial.set(serial)
            self._log(f"Selected device: {serial}")
            if self.phase == "connected":
                self.program_btn.config(state=tk.NORMAL)

            # F4: Smart-skip — probe device state on selection.
            try:
                dev_info = self.client.get_device_info(serial)
                state_str = dev_info.get("state", "") if isinstance(dev_info, dict) else ""
                is_programmed = "Programmed" in str(state_str)
                resources = self.client.list_resources()
                existing = next((r for r in resources if r.device_serial == serial), None)
                if is_programmed and existing:
                    self._log(f"Device already programmed, resource '{existing.name}' available — skipping to Registered")
                    self.resource_id = existing.id
                    self.phase = "registered"
                    self.program_btn.config(text="2. Programmed ✓", state=tk.DISABLED)
                    self.register_btn.config(text="3. Registered ✓", state=tk.DISABLED)
                    self.register_excl_btn.config(state=tk.DISABLED)
                    self.acquire_btn.config(state=tk.NORMAL)
                    self.acquire_ro_btn.config(state=tk.NORMAL)
                elif is_programmed:
                    self._log("Device already programmed — skipping to Register")
                    self.phase = "programmed"
                    self.program_btn.config(text="2. Programmed ✓", state=tk.DISABLED)
                    self.register_btn.config(state=tk.NORMAL)
                    self.register_excl_btn.config(state=tk.NORMAL)
            except Exception as e:
                self._log(f"Could not check device state: {e}")

    def _start_device_refresh(self):
        """Refresh device list every 5 seconds."""
        self._refresh_devices()

    def _refresh_devices(self):
        if self.phase == "disconnected" or self.client is None:
            return
        try:
            new_devices = self.client.list_devices()
            old_serial = self.selected_serial.get()
            self.devices = new_devices
            self._update_device_combo()
            # Check if selected device disappeared.
            if old_serial and not any(d.serial == old_serial for d in new_devices):
                self.selected_serial.set("")
                if self.phase == "acquired":
                    # Device disconnected during active use.
                    self._log_err(f"Device {old_serial} disconnected from server")
                    self.phase = "connected"
                    self.resource_id = None
                    self.resource_info = None
                    if self._polling_id:
                        self.root.after_cancel(self._polling_id)
                        self._polling_id = None
                    self._show_connection_panel()
                    messagebox.showwarning(
                        "Device Disconnected",
                        f"⚠ Device {old_serial} has been disconnected from the server.\n"
                        "The Counter Control Panel has been closed."
                    )
                    return
                else:
                    self._log_err(f"Device {old_serial} is no longer available")
        except Exception as e:
            self._log_err(f"Device refresh failed: {e}")
        self._device_refresh_id = self.root.after(5000, self._refresh_devices)

    def _on_program(self):
        try:
            serial = self.selected_serial.get()
            bitfile = self.bitfile_path.get()
            mode = self.bitfile_mode.get()
            # F5: Branch on bitfile mode.
            if mode == "local_file":
                self.client.configure_fpga_from_file(serial, bitfile)
            else:
                self.client.configure_fpga(serial, bitfile)
            self._log(f"FPGA programmed ({mode})")
            self.program_btn.config(text="2. Programmed ✓", state=tk.DISABLED)
            self.register_btn.config(state=tk.NORMAL)
            self.register_excl_btn.config(state=tk.NORMAL)
            self.phase = "programmed"
        except Exception as e:
            self._log_err(f"Program failed: {e}")

    def _on_register(self, resource_type="shared"):
        try:
            info = self.client.create_resource("Counter Resource", self.selected_serial.get(), resource_type)
            self.resource_id = info.id
            self._log(f"Resource registered ({resource_type}): {info.id}")
            self.register_btn.config(text="3. Registered ✓", state=tk.DISABLED)
            self.register_excl_btn.config(state=tk.DISABLED)
            self.acquire_btn.config(state=tk.NORMAL)
            self.acquire_ro_btn.config(state=tk.NORMAL)
            self.phase = "registered"
        except Exception as e:
            err_str = str(e)
            # F3: 409 Conflict — resource already exists. Look it up.
            if "409" in err_str or "already exists" in err_str.lower():
                self._log("Resource already exists — looking up...")
                try:
                    resources = self.client.list_resources()
                    serial = self.selected_serial.get()
                    existing = next((r for r in resources if r.device_serial == serial), None)
                    if existing:
                        self._log(f"Found existing resource: {existing.id}")
                        self.resource_id = existing.id
                        self.register_btn.config(text="3. Registered ✓", state=tk.DISABLED)
                        self.register_excl_btn.config(state=tk.DISABLED)
                        self.acquire_btn.config(state=tk.NORMAL)
                        self.acquire_ro_btn.config(state=tk.NORMAL)
                        self.phase = "registered"
                    else:
                        self._log_err("No matching resource found")
                except Exception as e2:
                    self._log_err(f"Lookup failed: {e2}")
            else:
                self._log_err(f"Register failed: {err_str}")

    def _on_acquire(self):
        try:
            self.acquired_readonly = False
            self.client.acquire_resource(self.resource_id)
            self._log("Resource acquired (shared) \u2014 counter control active")
            self.phase = "acquired"
            self._show_counter_panel()
        except Exception as e:
            self._log_err(f"Acquire failed: {e}")

    def _on_acquire_readonly(self):
        """Acquire the resource in read-only mode (Phase 30, ADR-026)."""
        try:
            self.acquired_readonly = True
            self.client.acquire_resource_readonly(self.resource_id)
            self._log("Resource acquired (read-only) \u2014 counter control active")
            self.phase = "acquired"
            self._show_counter_panel()
        except Exception as e:
            self._log_err(f"Acquire R/O failed: {e}")

    def _on_wire_change(self):
        self.wire_in.reset1 = self.reset1_var.get()
        self.wire_in.disable1 = self.disable1_var.get()
        self.wire_in.autocount2 = self.autocount2_var.get()
        try:
            self.client.send_wire_in(self.selected_serial.get(), self.wire_in)
            self._log(f"WireIn 0x00 = 0x{self.wire_in.to_value():04X}")
        except Exception as e:
            self._log_err(f"WireIn failed: {e}")

    def _fire_trigger(self, endpoint, bit):
        try:
            self.client.activate_trigger_in(self.selected_serial.get(), endpoint, bit)
            self._log(f"TriggerIn 0x{endpoint:02X} bit {bit}")
        except Exception as e:
            self._log_err(f"Trigger failed: {e}")

    def _on_lock(self):
        try:
            locked_by_me = (
                self.resource_info
                and self.resource_info.locked_by == USER_ID
            )
            if locked_by_me:
                self.client.unlock_resource(self.resource_id)
                self._log("Resource unlocked")
            else:
                self.client.lock_resource(self.resource_id)
                self._log("Resource locked")
        except Exception as e:
            self._log_err(f"Lock/unlock failed: {e}")

    # ========================================================================
    # Polling
    # ========================================================================

    def _start_polling(self):
        self._poll()

    def _poll(self):
        if self.phase != "acquired":
            return
        try:
            self.counter = self.client.poll_counter_state(self.selected_serial.get())
            if self.resource_id:
                self.resource_info = self.client.get_resource(self.resource_id)
            self._update_counter_display()
        except Exception as e:
            self._log_err(f"Poll error: {e}")

        self._polling_id = self.root.after(self.poll_interval_ms.get(), self._poll)

    def _update_counter_display(self):
        c = self.counter
        self.count1_label.config(text=f"0x{c.count1:02X} ({c.count1})")
        self.count1_bar["value"] = c.count1
        self.count2_label.config(text=f"0x{c.count2:02X} ({c.count2})")
        self.count2_bar["value"] = c.count2

        eq00 = "🟢" if c.count1_eq_00 else "⚫"
        eq80 = "🟢" if c.count1_eq_80 else "⚫"
        self.trig1_label.config(text=f"==0x00: {eq00}  ==0x80: {eq80}")

        eqff = "🟢" if c.count2_eq_ff else "⚫"
        self.trig2_label.config(text=f"==0xFF: {eqff}")

        btn_parts = []
        for i in range(4):
            pressed = (c.buttons >> i) & 1
            btn_parts.append(f"[{i+1}]" if pressed else f" {i+1} ")
        self.buttons_label.config(text="Buttons: " + " ".join(btn_parts))

        # Update lock button.
        if self.resource_info:
            if self.resource_info.locked_by == USER_ID:
                self.lock_btn.config(text="🔓 Unlock")
            else:
                self.lock_btn.config(text="🔒 Lock")

    # ========================================================================
    # Quit
    # ========================================================================

    def on_close(self):
        if self.phase == "acquired":
            result = messagebox.askyesnocancel(
                "Release Resource?",
                "Release the Counter Resource before quitting?\n"
                "Yes = Release and quit\n"
                "No = Leave held and quit\n"
                "Cancel = Stay"
            )
            if result is None:  # Cancel
                return
            if result:  # Yes
                # Phase 25-Fix-H (F8+F10): Use disconnect() for clean
                # session + resource cleanup.
                try:
                    self.client.disconnect()
                except Exception:
                    pass
        if self._polling_id:
            self.root.after_cancel(self._polling_id)
        if self._device_refresh_id:
            self.root.after_cancel(self._device_refresh_id)
        self.root.destroy()

    # ========================================================================
    # Logging
    # ========================================================================

    def _log(self, msg: str):
        ts = datetime.now().strftime("%H:%M:%S")
        self.log_text.config(state=tk.NORMAL)
        self.log_text.insert(tk.END, f"[{ts}] {msg}\n")
        self.log_text.see(tk.END)
        self.log_text.config(state=tk.DISABLED)

    def _log_err(self, msg: str):
        ts = datetime.now().strftime("%H:%M:%S")
        self.log_text.config(state=tk.NORMAL)
        self.log_text.insert(tk.END, f"[{ts}] ⚠ {msg}\n", "error")
        self.log_text.tag_config("error", foreground="#ff6464")
        self.log_text.see(tk.END)
        self.log_text.config(state=tk.DISABLED)


def main():
    root = tk.Tk()
    app = CountersApp(root)
    root.mainloop()


if __name__ == "__main__":
    main()
