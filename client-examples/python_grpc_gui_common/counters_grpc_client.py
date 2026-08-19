"""
ok-FPGA-Server gRPC GUI Common Library

Shared gRPC API client for the Python gRPC GUI demos (tkinter and dearpygui).
Uses grpcio + protobuf stubs generated from the proto file.

Note: This module expects the proto stubs to be generated into a `gen/` directory.
Run `python -m grpc_tools.protoc` to generate them (see generate_stubs.py).
"""

import os
import sys
from dataclasses import dataclass, field
from typing import Optional, List

import grpc

# Add the generated proto directory to the path.
_gen_dir = os.path.join(os.path.dirname(__file__), "gen")
if _gen_dir not in sys.path:
    sys.path.insert(0, _gen_dir)

# These imports require the generated proto stubs in gen/.
# If missing, run: python generate_stubs.py
try:
    from gen import okfpga_pb2
    from gen import okfpga_pb2_grpc
except ImportError as e:
    raise ImportError(
        "gRPC proto stubs not found. Run 'python generate_stubs.py' in "
        "the python_grpc_gui_common/ directory to generate them. "
        f"Original error: {e}"
    ) from e


@dataclass
class CounterState:
    count1: int = 0
    count2: int = 0
    buttons: int = 0
    count1_eq_00: bool = False
    count1_eq_80: bool = False
    count2_eq_ff: bool = False


@dataclass
class WireInState:
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


@dataclass
class ResourceInfo:
    id: str = ""
    name: str = ""
    device_serial: str = ""
    access_mode: str = ""
    holders: List[str] = field(default_factory=list)
    locked_by: Optional[str] = None


class CountersGrpcClient:
    """gRPC API client for the Counters.bit interactive demo.

    Args:
        grpc_addr: gRPC server address (e.g., "localhost:50051")
        user_id: User identity for session tracking
    """

    def __init__(self, grpc_addr: str, user_id: str):
        self.channel = grpc.insecure_channel(grpc_addr)
        self.device_stub = okfpga_pb2_grpc.DeviceServiceStub(self.channel)
        self.classic_stub = okfpga_pb2_grpc.ClassicDataPortServiceStub(self.channel)
        self.server_stub = okfpga_pb2_grpc.ServerServiceStub(self.channel)
        self.user_id = user_id

    def get_server_status(self):
        self.device_stub.GetServerStatus(okfpga_pb2.ServerStatusRequest())

    def list_devices(self) -> List[DeviceInfo]:
        resp = self.device_stub.ListDevices(okfpga_pb2.ListDevicesRequest())
        return [DeviceInfo(serial=d.serial, product_name=d.product_name)
                for d in resp.devices]

    def get_device_info(self, serial: str) -> dict:
        """``GetDeviceInfo`` RPC — get full device info for a single device.

        Returns a dict with at least ``state`` derived from whether a bitfile
        is loaded. Used by F4 smart-skip.
        """
        resp = self.device_stub.GetDeviceInfo(
            okfpga_pb2.DeviceRequest(serial=serial))
        device = resp.device
        state = "Programmed" if device.HasField("bitfile") else "NotProgrammed"
        return {
            "serial": device.serial,
            "product_name": device.product_name,
            "state": state,
        }

    def list_resources(self) -> list:
        """``ListResources`` RPC — list all resources across all devices.

        Used by F3 (409 conflict recovery) and F4 (smart-skip).
        """
        from python_rest_gui_common.counters_client import ResourceInfo
        resp = self.device_stub.ListResources(
            okfpga_pb2.ListResourcesRequest())
        return [ResourceInfo(
            id=r.id,
            name=r.name,
            device_serial=r.device_serial,
            access_mode="shared",
            holders=list(r.holders),
            locked_by=None,
        ) for r in resp.resources]

    def configure_fpga(self, serial: str, bitfile_path: str):
        """Configure FPGA from a server-side bitfile path."""
        self.device_stub.ConfigureFPGA(okfpga_pb2.ConfigureFPGARequest(
            serial=serial, bitfile_path=bitfile_path, user_id=self.user_id))

    def configure_fpga_from_file(self, serial: str, local_path: str):
        """Configure FPGA from a local file (client-side streaming).

        Reads the bitfile from the client's local filesystem and streams it
        to the server using ``ConfigureFPGAFromMemory`` RPC as a single chunk.
        """
        with open(local_path, "rb") as f:
            data = f.read()

        def chunk_iterator():
            yield okfpga_pb2.ConfigureFPGAChunk(
                serial=serial, data=data, is_last=True, user_id=self.user_id)

        self.device_stub.ConfigureFPGAFromMemory(chunk_iterator())

    # --- Classic Data Port ---

    def set_wire_in(self, serial: str, endpoint: int, value: int, mask: int):
        self.classic_stub.SetWireInValue(okfpga_pb2.SetWireInValueRequest(
            serial=serial, endpoint=endpoint, value=value, mask=mask, auto_update=False))

    def update_wire_ins(self, serial: str):
        self.classic_stub.UpdateWireIns(okfpga_pb2.DeviceRequest(serial=serial))

    def update_wire_outs(self, serial: str):
        self.classic_stub.UpdateWireOuts(okfpga_pb2.DeviceRequest(serial=serial))

    def get_wire_out_value(self, serial: str, endpoint: int) -> int:
        resp = self.classic_stub.GetWireOutValue(
            okfpga_pb2.GetWireValueRequest(serial=serial, endpoint=endpoint))
        return resp.value

    def activate_trigger_in(self, serial: str, endpoint: int, bit: int):
        self.classic_stub.ActivateTriggerIn(
            okfpga_pb2.ActivateTriggerInRequest(serial=serial, endpoint=endpoint, bit=bit))

    def get_trigger_out_vector(self, serial: str, endpoint: int) -> int:
        resp = self.classic_stub.GetTriggerOutVector(
            okfpga_pb2.TriggerOutVectorRequest(serial=serial, endpoint=endpoint))
        return resp.vector

    # --- Combined Operations ---

    def send_wire_in(self, serial: str, state: WireInState):
        self.set_wire_in(serial, 0x00, state.to_value(), 0x07)
        self.update_wire_ins(serial)

    def poll_counter_state(self, serial: str) -> CounterState:
        self.update_wire_outs(serial)
        count1 = self.get_wire_out_value(serial, 0x20) & 0xFF
        count2 = self.get_wire_out_value(serial, 0x21) & 0xFF
        buttons = self.get_wire_out_value(serial, 0x22) & 0x0F
        trig_60 = self.get_trigger_out_vector(serial, 0x60)
        trig_61 = self.get_trigger_out_vector(serial, 0x61)
        return CounterState(
            count1=count1, count2=count2, buttons=buttons,
            count1_eq_00=bool(trig_60 & 0x01),
            count1_eq_80=bool(trig_60 & 0x02),
            count2_eq_ff=bool(trig_61 & 0x01),
        )

    # --- Resource Management ---

    def _resource_from_response(self, resp) -> ResourceInfo:
        r = resp.resource if hasattr(resp, 'resource') else resp
        return ResourceInfo(
            id=r.id, name=r.name, device_serial=r.device_serial,
            access_mode="shared", holders=list(r.holders), locked_by=None,
        )

    def create_resource(self, name: str, device_serial: str) -> ResourceInfo:
        resp = self.device_stub.CreateResource(okfpga_pb2.CreateResourceRequest(
            name=name, device_serial=device_serial,
            description="Counters.bit hardware test resource",
            resource_type="shared", capabilities=[]))
        return self._resource_from_response(resp)

    def acquire_resource(self, resource_id: str):
        self.device_stub.AcquireResource(okfpga_pb2.AcquireResourceRequest(
            resource_id=resource_id, user_id=self.user_id, mode=0))  # ACCESS_MODE_SHARED

    def release_resource(self, resource_id: str):
        self.device_stub.ReleaseResource(okfpga_pb2.ReleaseResourceRequest(
            resource_id=resource_id, user_id=self.user_id))

    def get_resource(self, resource_id: str) -> ResourceInfo:
        resp = self.device_stub.GetResource(
            okfpga_pb2.ResourceRequest(resource_id=resource_id))
        return self._resource_from_response(resp)

    def lock_resource(self, resource_id: str):
        self.device_stub.LockResource(okfpga_pb2.LockResourceRequest(
            resource_id=resource_id, user_id=self.user_id))

    def unlock_resource(self, resource_id: str):
        self.device_stub.UnlockResource(okfpga_pb2.LockResourceRequest(
            resource_id=resource_id, user_id=self.user_id))

    def heartbeat(self):
        self.device_stub.Ping(okfpga_pb2.PingRequest(user_id=self.user_id))

    def disconnect(self):
        """Cleanly disconnect this user from the server (Phase 25-Fix-H, F10).

        Releases all resources held by this user on any device, removes
        the user session, and stops heartbeat tracking. This is the
        recommended way to end a client session.
        """
        self.user_mgmt_stub.Disconnect(
            okfpga_pb2.DisconnectRequest(user_id=self.user_id))
