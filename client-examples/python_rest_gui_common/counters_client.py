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
    """Device info matching the OpenAPI DeviceInfo schema.

    Fields match api/openapi.yaml components/schemas/DeviceInfo.
    Per ai-collaboration.md §6, clients must parse according to the spec.
    """
    serial: str = ""
    device_id: str = ""
    product_name: str = ""
    product_id: int = 0
    device_interface: str = ""
    usb_speed: str = ""
    device_major_version: int = 0
    device_minor_version: int = 0
    wire_width: int = 0
    trigger_width: int = 0
    pipe_width: int = 0
    register_address_width: int = 0
    register_data_width: int = 0
    fpga_vendor: str = ""
    dataport: str = ""
    is_front_panel_enabled: bool = False
    syzygy_label: str = ""  # Phase 34-B

    def display_label(self) -> str:
        """Build a user-friendly label: 'XEM7320-A75T (SERIAL) | 0:DAC | 2:ADC'"""
        base = f"{self.product_name} ({self.serial})"
        if self.syzygy_label:
            return f"{base} | {self.syzygy_label}"
        return base


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
        return self._get("/server/status")

    def list_devices(self) -> List[DeviceInfo]:
        """List all connected FPGA devices.

        The server returns ``{"devices": [...]}`` per the OpenAPI spec.
        Each device dict is mapped to a :class:`DeviceInfo` dataclass.
        """
        data = self._get("/devices")
        # Unwrap the {"devices": [...]} wrapper per OpenAPI spec.
        device_list = (data or {}).get("devices", [])
        devices = [DeviceInfo(
            serial=d.get("serial", ""),
            device_id=d.get("device_id", ""),
            product_name=d.get("product_name", ""),
            product_id=d.get("product_id", 0),
            device_interface=d.get("device_interface", ""),
            usb_speed=d.get("usb_speed", ""),
            device_major_version=d.get("device_major_version", 0),
            device_minor_version=d.get("device_minor_version", 0),
            wire_width=d.get("wire_width", 0),
            trigger_width=d.get("trigger_width", 0),
            pipe_width=d.get("pipe_width", 0),
            register_address_width=d.get("register_address_width", 0),
            register_data_width=d.get("register_data_width", 0),
            fpga_vendor=d.get("fpga_vendor", ""),
            dataport=d.get("dataport", ""),
            is_front_panel_enabled=d.get("is_front_panel_enabled", False),
        ) for d in device_list]
        # Phase 34-B: Populate SYZYGY labels.
        for dev in devices:
            try:
                dev.syzygy_label = self._get_syzygy_label(dev.serial)
            except Exception:
                pass
        return devices

    def _get_syzygy_label(self, serial: str) -> str:
        """Phase 34-B: Fetch SYZYGY ports and build a compact label string."""
        data = self._get(f"/devices/{serial}/syzygy")
        ports = (data or {}).get("ports", [])
        parts = []
        for p in ports:
            if p.get("is_populated"):
                idx = p.get("index", 0)
                name = p.get("product_name", "")
                model = p.get("product_model", "")
                if not model or model == name:
                    parts.append(f"{idx}:{name}")
                else:
                    parts.append(f"{idx}:{name} ({model})")
        return " | ".join(parts)

    # --- FPGA Configuration ---

    def configure_fpga(self, serial: str, bitfile_path: str):
        """Program FPGA from a server-side path.

        Sends ``{"bitfile_path": "..."}`` as JSON. The bitfile must exist
        on the server's filesystem.
        """
        return self._post(f"/devices/{serial}/configure",
                          {"bitfile_path": bitfile_path})

    def configure_fpga_from_file(self, serial: str, local_path: str):
        """Program FPGA from a local file.

        Reads the bitfile from the client's local filesystem and uploads
        it to the server as ``application/octet-stream``. The server calls
        ``ConfigureFPGAFromMemory`` with the received bytes.
        """
        with open(local_path, "rb") as f:
            data = f.read()
        url = f"{self.base_url}/devices/{serial}/configure"
        resp = self.session.post(
            url, data=data,
            headers={"Content-Type": "application/octet-stream"},
        )
        if resp.status_code >= 400:
            raise ClientError(resp.status_code, resp.text)
        return resp.json() if resp.text else None

    # --- Classic Data Port: WireIn/WireOut ---

    def set_wire_in(self, serial: str, endpoint: int, value: int, mask: int):
        """PUT /devices/{serial}/classic/wires/in — server calls UpdateWireIns implicitly."""
        return self._put(f"/devices/{serial}/classic/wires/in",
                         {"endpoint": endpoint, "value": value, "mask": mask})

    def get_wire_out_value(self, serial: str, endpoint: int) -> int:
        """GET /devices/{serial}/classic/wires/out?endpoint={ep} — server calls UpdateWireOuts implicitly."""
        data = self._get(f"/devices/{serial}/classic/wires/out?endpoint={endpoint}")
        return (data or {}).get("value", 0)

    # --- Classic Data Port: TriggerIn/TriggerOut ---

    def activate_trigger_in(self, serial: str, endpoint: int, bit: int):
        return self._post(f"/devices/{serial}/classic/triggers/in",
                          {"endpoint": endpoint, "bit": bit})

    def get_trigger_out_vector(self, serial: str, endpoint: int) -> int:
        """GET /devices/{serial}/classic/triggers/out?endpoint={ep} — server calls UpdateTriggerOuts implicitly."""
        data = self._get(f"/devices/{serial}/classic/triggers/out?endpoint={endpoint}")
        return (data or {}).get("value", 0)

    # --- Combined Poll ---

    def send_wire_in(self, serial: str, state: WireInState):
        """Set WireIn 0x00. Server calls UpdateWireIns implicitly."""
        self.set_wire_in(serial, 0x00, state.to_value(), 0x07)

    def poll_counter_state(self, serial: str) -> CounterState:
        """Read all counter outputs in one batch."""
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

    def get_device_info(self, serial: str) -> dict:
        """``GET /devices/{serial}`` — get full device info for a single device.

        Used by F4 smart-skip to check if a device is already programmed.
        Returns the raw JSON dict.
        """
        return self._get(f"/devices/{serial}")

    def list_resources(self) -> list:
        """``GET /resources`` — list all resources across all devices.

        Used by F3 (409 conflict recovery) and F4 (smart-skip) to look up
        existing resources on a device.
        """
        data = self._get("/resources")
        resources_list = data.get("resources", []) if isinstance(data, dict) else []
        return [ResourceInfo(
            id=r.get("id", ""),
            name=r.get("name", ""),
            device_serial=r.get("device_serial", ""),
            access_mode=r.get("access_mode", ""),
            holders=r.get("holders", []),
            locked_by=r.get("locked_by"),
        ) for r in resources_list]


    def create_resource(self, name: str, device_serial: str, resource_type: str = "shared") -> ResourceInfo:
        """Phase 34-E: resource_type can be 'shared' or 'exclusive'."""
        data = self._post("/resources", {
            "name": name,
            "device_serial": device_serial,
            "access_mode": resource_type,
            "description": "Counters.bit hardware test resource",
        })
        return ResourceInfo(
            id=data.get("id", ""),
            name=data.get("name", name),
            device_serial=data.get("device_serial", device_serial),
            access_mode=data.get("access_mode", resource_type),
            holders=data.get("holders", []),
            locked_by=data.get("locked_by"),
        )

    def acquire_resource(self, resource_id: str):
        return self._post(f"/resources/{resource_id}/acquire",
                          {"mode": "shared", "user_id": self.user_id})

    def acquire_resource_readonly(self, resource_id: str):
        """Acquire the resource in read-only mode (ADR-026).

        ReadOnly holders can observe output endpoints and address ranges but
        cannot perform write operations or state-disturbing calls.
        """
        return self._post(f"/resources/{resource_id}/acquire",
                          {"mode": "read_only", "user_id": self.user_id})

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

    def disconnect(self):
        """Cleanly disconnect this user from the server (Phase 25-Fix-H, F10).

        Releases all resources held by this user on any device, removes
        the user session, and stops heartbeat tracking. This is the
        recommended way to end a client session.
        """
        return self._post("/disconnect", {"user_id": self.user_id})
