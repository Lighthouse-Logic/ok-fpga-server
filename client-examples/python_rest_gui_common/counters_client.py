"""
ok-FPGA-Server REST GUI Common Library

Shared API client for the Python REST GUI demos (tkinter and dearpygui).
Provides CountersClient with typed methods for Classic Data Port I/O,
resource management, and a combined poll_counter_state() method.

This module is imported by python-rest-gui-tkinter and python-rest-gui-dearpygui.
"""

import requests
from dataclasses import dataclass, field
from typing import Optional, List


@dataclass
class CounterState:
    """Counter state read from the FPGA via WireOut and TriggerOut endpoints."""
    count1: int = 0
    count2: int = 0
    buttons: int = 0
    count1_eq_00: bool = False
    count1_eq_80: bool = False
    count2_eq_ff: bool = False


@dataclass
class WireInState:
    """WireIn control state for endpoint 0x00."""
    reset1: bool = False
    disable1: bool = False
    autocount2: bool = False

    def to_value(self) -> int:
        val = 0
        if self.reset1: val |= 1
        if self.disable1: val |= 2
        if self.autocount2: val |= 4
        return val


@dataclass
class DeviceInfo:
    serial: str = ""
    product_name: str = ""
    interface: str = ""
    usb_speed: str = ""
    state: str = ""


@dataclass
class ResourceInfo:
    id: str = ""
    name: str = ""
    device_serial: str = ""
    access_mode: str = ""
    holders: List[str] = field(default_factory=list)
    locked_by: Optional[str] = None


class ClientError(Exception):
    """REST client error with status code and body."""
    def __init__(self, status: int, body: str):
        super().__init__(f"Server error {status}: {body}")
        self.status = status
        self.body = body


class CountersClient:
    """REST API client for the Counters.bit interactive demo.

    Provides methods for every API call needed by the GUI:
    server status, device listing, FPGA configuration, Classic Data Port I/O,
    and resource management.

    Args:
        rest_url: Server REST API base URL (e.g., "http://localhost:8088")
        user_id: User identity for session tracking (e.g., "Pythos Rester")
    """

    def __init__(self, rest_url: str, user_id: str):
        self.base_url = f"{rest_url.rstrip('/')}/api/v1"
        self.user_id = user_id
        self.session = requests.Session()
        self.session.headers["X-User-Id"] = user_id

    def _get(self, path: str):
        resp = self.session.get(f"{self.base_url}{path}")
        if resp.status_code >= 400:
            raise ClientError(resp.status_code, resp.text)
        return resp.json() if resp.text else None

    def _post(self, path: str, body=None):
        resp = self.session.post(f"{self.base_url}{path}", json=body)
        if resp.status_code >= 400:
            raise ClientError(resp.status_code, resp.text)
        return resp.json() if resp.text else None

    def _put(self, path: str, body):
        resp = self.session.put(f"{self.base_url}{path}", json=body)
        if resp.status_code >= 400:
            raise ClientError(resp.status_code, resp.text)
        return resp.json() if resp.text else None

    # --- Server & Device ---

    def get_server_status(self):
        return self._get("/status")

    def list_devices(self) -> List[DeviceInfo]:
        data = self._get("/devices")
        return [DeviceInfo(
            serial=d.get("serial", ""),
            product_name=d.get("product_name", ""),
            interface=d.get("interface", ""),
            usb_speed=d.get("usb_speed", ""),
            state=d.get("state", ""),
        ) for d in (data or [])]

    # --- FPGA Configuration ---

    def configure_fpga(self, serial: str, bitfile_path: str):
        return self._post(f"/devices/{serial}/configure",
                          {"bitfile_path": bitfile_path})

    # --- Classic Data Port: WireIn/WireOut ---

    def set_wire_in(self, serial: str, endpoint: int, value: int, mask: int):
        return self._put(f"/devices/{serial}/classic/wires/in",
                         {"endpoint": endpoint, "value": value, "mask": mask})

    def update_wire_ins(self, serial: str):
        return self._post(f"/devices/{serial}/classic/wires/in/update")

    def update_wire_outs(self, serial: str):
        return self._post(f"/devices/{serial}/classic/wires/out/update")

    def get_wire_out_value(self, serial: str, endpoint: int) -> int:
        data = self._get(f"/devices/{serial}/classic/wires/out/0x{endpoint:02x}")
        return (data or {}).get("value", 0)

    # --- Classic Data Port: TriggerIn/TriggerOut ---

    def activate_trigger_in(self, serial: str, endpoint: int, bit: int):
        return self._post(f"/devices/{serial}/classic/triggers/in",
                          {"endpoint": endpoint, "bit": bit})

    def get_trigger_out_vector(self, serial: str, endpoint: int) -> int:
        data = self._get(f"/devices/{serial}/classic/triggers/out/0x{endpoint:02x}")
        return (data or {}).get("value", 0)

    # --- Combined Poll ---

    def send_wire_in(self, serial: str, state: WireInState):
        """Set WireIn 0x00 and commit."""
        self.set_wire_in(serial, 0x00, state.to_value(), 0x07)
        self.update_wire_ins(serial)

    def poll_counter_state(self, serial: str) -> CounterState:
        """Read all counter outputs in one batch."""
        self.update_wire_outs(serial)
        count1 = self.get_wire_out_value(serial, 0x20) & 0xFF
        count2 = self.get_wire_out_value(serial, 0x21) & 0xFF
        buttons = self.get_wire_out_value(serial, 0x22) & 0x0F
        trig_60 = self.get_trigger_out_vector(serial, 0x60)
        trig_61 = self.get_trigger_out_vector(serial, 0x61)
        return CounterState(
            count1=count1,
            count2=count2,
            buttons=buttons,
            count1_eq_00=bool(trig_60 & 0x01),
            count1_eq_80=bool(trig_60 & 0x02),
            count2_eq_ff=bool(trig_61 & 0x01),
        )

    # --- Resource Management ---

    def create_resource(self, name: str, device_serial: str) -> ResourceInfo:
        data = self._post("/resources", {
            "name": name,
            "device_serial": device_serial,
            "access_mode": "shared",
            "description": "Counters.bit hardware test resource",
        })
        return ResourceInfo(
            id=data.get("id", ""),
            name=data.get("name", name),
            device_serial=data.get("device_serial", device_serial),
            access_mode=data.get("access_mode", "shared"),
            holders=data.get("holders", []),
            locked_by=data.get("locked_by"),
        )

    def acquire_resource(self, resource_id: str):
        return self._post(f"/resources/{resource_id}/acquire",
                          {"mode": "shared", "user_id": self.user_id})

    def release_resource(self, resource_id: str):
        return self._post(f"/resources/{resource_id}/release",
                          {"user_id": self.user_id})

    def get_resource(self, resource_id: str) -> ResourceInfo:
        data = self._get(f"/resources/{resource_id}")
        return ResourceInfo(
            id=data.get("id", ""),
            name=data.get("name", ""),
            device_serial=data.get("device_serial", ""),
            access_mode=data.get("access_mode", ""),
            holders=data.get("holders", []),
            locked_by=data.get("locked_by"),
        )

    def lock_resource(self, resource_id: str):
        return self._post(f"/resources/{resource_id}/lock",
                          {"user_id": self.user_id})

    def unlock_resource(self, resource_id: str):
        return self._post(f"/resources/{resource_id}/unlock",
                          {"user_id": self.user_id})

    def heartbeat(self):
        return self._post("/heartbeat", {"user_id": self.user_id})
