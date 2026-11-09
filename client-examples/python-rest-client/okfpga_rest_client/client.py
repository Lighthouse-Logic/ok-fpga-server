# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""ok-FPGA-Server REST Client.

Async REST client for every endpoint defined in the OpenAPI 3.0 spec
(/api/openapi.yaml). Uses httpx for async HTTP and returns parsed JSON.

Usage:
    async with OkFpgaRestClient("http://localhost:8088") as client:
        status = await client.get_server_status()
        devices = await client.list_devices()
"""

from __future__ import annotations

from typing import Any, Optional

import httpx


class OkFpgaRestClient:
    """Async REST client for the ok-FPGA-Server.

    Methods are grouped by API category matching the OpenAPI spec:
    - server_* — Server status, services, restart
    - list_devices, get_device_* — Device discovery and info
    - configure_fpga, get_bitfile_info, get_dataport_type — FPGA config
    - classic_* — Classic data port (wires, triggers, pipes, registers)
    - axi_* — AXI data port (lite, full, stream)
    - resource_* — Resource management
    - user_* — User management
    - watch_events — SSE event stream
    """

    def __init__(
        self,
        base_url: str,
        timeout: float = 10.0,
        user_id: str = "python-rest-client",
    ) -> None:
        """Create a new REST client.

        The ``/api/v1`` prefix is appended automatically — pass the bare
        server URL (e.g., ``"http://localhost:8088"``).

        Args:
            base_url: Server REST API base URL (e.g., "http://localhost:8088").
            timeout: Default request timeout in seconds.
            user_id: Caller identity sent via ``X-User-Id`` header (S-18, ADR-029).
        """
        self.base_url = base_url.rstrip("/")
        self.user_id = user_id
        self._client = httpx.AsyncClient(
            base_url=f"{self.base_url}/api/v1",
            timeout=timeout,
            headers={"X-User-Id": self.user_id},
        )

    async def __aenter__(self) -> "OkFpgaRestClient":
        return self

    async def __aexit__(self, *args: Any) -> None:
        await self._client.aclose()

    async def close(self) -> None:
        """Close the underlying HTTP client."""
        await self._client.aclose()

    async def _get(self, path: str, **kwargs: Any) -> Any:
        """GET request, return parsed JSON."""
        resp = await self._client.get(path, **kwargs)
        resp.raise_for_status()
        return resp.json()

    async def _get_bytes(self, path: str, **kwargs: Any) -> bytes:
        """GET request returning raw binary bytes (S-05)."""
        resp = await self._client.get(path, **kwargs)
        resp.raise_for_status()
        return resp.content

    async def _post(self, path: str, json: Any = None, **kwargs: Any) -> Any:
        """POST request with optional JSON body, return parsed JSON."""
        resp = await self._client.post(path, json=json, **kwargs)
        resp.raise_for_status()
        # Some endpoints return 202 with no body.
        if resp.status_code == 202 or not resp.content:
            return {"status": "accepted"}
        return resp.json()

    async def _post_with_user(
        self, path: str, user_id: str, json: dict | None = None
    ) -> Any:
        """POST with user_id in both JSON body and X-User-Id header (ADR-029).

        Ensures the server receives user identity via both mechanisms:
        the request body ``user_id`` field (primary) and the ``X-User-Id``
        header (fallback for middleware).
        """
        body = json or {}
        body["user_id"] = user_id
        return await self._post(
            path, json=body, headers={"X-User-Id": user_id}
        )

    async def _put(self, path: str, json: Any = None) -> Any:
        """PUT request with JSON body, return parsed JSON."""
        resp = await self._client.put(path, json=json)
        resp.raise_for_status()
        return resp.json()

    async def _delete(self, path: str) -> None:
        """DELETE request, returns None on success (204)."""
        resp = await self._client.delete(path)
        resp.raise_for_status()

    # ========================================================================
    # Server Endpoints (/server/*)
    # ========================================================================

    async def get_server_status(self) -> dict:
        """GET /server/status — Server status, uptime, services."""
        return await self._get("/server/status")

    async def restart_server(self) -> dict:
        """POST /server/restart — Restart the server."""
        return await self._post("/server/restart")

    async def list_services(self) -> dict:
        """GET /server/services — List manageable services."""
        return await self._get("/server/services")

    async def start_service(self, name: str) -> dict:
        """POST /server/services/{name}/start — Start a service."""
        return await self._post(f"/server/services/{name}/start")

    async def stop_service(self, name: str) -> dict:
        """POST /server/services/{name}/stop — Stop a service."""
        return await self._post(f"/server/services/{name}/stop")

    async def restart_service(self, name: str) -> dict:
        """POST /server/services/{name}/restart — Restart a service."""
        return await self._post(f"/server/services/{name}/restart")

    # ========================================================================
    # Device Endpoints (/devices/*)
    # ========================================================================

    async def list_devices(self) -> dict:
        """GET /devices — List all connected FPGA devices."""
        return await self._get("/devices")

    async def get_device_info(self, serial: str) -> dict:
        """GET /devices/{serial} — Get detailed device info."""
        return await self._get(f"/devices/{serial}")

    async def get_device_sensors(self, serial: str) -> dict:
        """GET /devices/{serial}/sensors — Read sensor values."""
        return await self._get(f"/devices/{serial}/sensors")

    async def get_device_settings(self, serial: str) -> dict:
        """GET /devices/{serial}/settings — List all settings."""
        return await self._get(f"/devices/{serial}/settings")

    async def get_device_setting(self, serial: str, key: str) -> dict:
        """GET /devices/{serial}/settings/{key} — Get a single setting."""
        return await self._get(f"/devices/{serial}/settings/{key}")

    async def set_device_setting(self, serial: str, key: str, value: Any) -> dict:
        """PUT /devices/{serial}/settings/{key} — Set a setting value."""
        return await self._put(f"/devices/{serial}/settings/{key}", json={"value": value})

    async def get_syzygy_ports(self, serial: str) -> dict:
        """GET /devices/{serial}/syzygy — Get SYZYGY port info."""
        return await self._get(f"/devices/{serial}/syzygy")

    # ========================================================================
    # FPGA Configuration
    # ========================================================================

    async def configure_fpga(self, serial: str, bitfile_path: str) -> dict:
        """POST /devices/{serial}/configure_from_server_path -- Program the FPGA (ADR-028).

        Sends a JSON body with the server-side bitfile path. The bitfile
        must exist on the server's filesystem.
        """
        return await self._post(
            f"/devices/{serial}/configure_from_server_path",
            json={"bitfile_path": bitfile_path},
        )

    async def get_bitfile_info(self, serial: str) -> dict:
        """GET /devices/{serial}/bitfile — Get loaded bitfile info."""
        return await self._get(f"/devices/{serial}/bitfile")

    async def get_dataport_type(self, serial: str) -> dict:
        """GET /devices/{serial}/dataport — Get data port type."""
        return await self._get(f"/devices/{serial}/dataport")

    # ========================================================================
    # Classic Data Port (/devices/{serial}/classic/*)
    # ========================================================================

    async def set_wire_in(self, serial: str, endpoint: int, value: int,
                          mask: Optional[int] = None,
                          auto_update: bool = True) -> dict:
        """PUT /devices/{serial}/classic/wires/in — Set WireIn value (S-07)."""
        body: dict[str, Any] = {
            "endpoint": endpoint,
            "value": value,
            "auto_update": auto_update,
        }
        if mask is not None:
            body["mask"] = mask
        return await self._put(f"/devices/{serial}/classic/wires/in", json=body)

    async def get_wire_out(
        self, serial: str, endpoint: int = 0x20, update: bool = True
    ) -> dict:
        """GET /devices/{serial}/classic/wires/out?endpoint=N — Read single WireOut value."""
        return await self._get(
            f"/devices/{serial}/classic/wires/out",
            params={"endpoint": endpoint, "update": str(update).lower()},
        )

    async def activate_trigger_in(self, serial: str, endpoint: int, bit: int) -> dict:
        """POST /devices/{serial}/classic/triggers/in — Activate TriggerIn."""
        return await self._post(
            f"/devices/{serial}/classic/triggers/in",
            json={"endpoint": endpoint, "bit": bit},
        )

    async def get_trigger_out(
        self, serial: str, endpoint: int = 0x60, mask: int = 0, update: bool = True
    ) -> dict:
        """GET /devices/{serial}/classic/triggers/out?endpoint=N — Read single TriggerOut."""
        return await self._get(
            f"/devices/{serial}/classic/triggers/out",
            params={"endpoint": endpoint, "mask": mask, "update": str(update).lower()},
        )

    async def write_pipe_in(self, serial: str, endpoint: int, data: bytes) -> dict:
        """POST /devices/{serial}/classic/pipes/in?endpoint=N — Write raw binary bytes to PipeIn (S-05)."""
        return await self._post(
            f"/devices/{serial}/classic/pipes/in",
            params={"endpoint": endpoint},
            content=data,
            headers={"Content-Type": "application/octet-stream"},
        )

    async def read_pipe_out(self, serial: str, endpoint: int, length: int) -> bytes:
        """GET /devices/{serial}/classic/pipes/out?endpoint=N&length=L — Read raw binary bytes from PipeOut (S-05)."""
        return await self._get_bytes(
            f"/devices/{serial}/classic/pipes/out",
            params={"endpoint": endpoint, "length": length},
        )

    async def write_block_pipe_in(self, serial: str, endpoint: int,
                                   data: bytes, block_size: int) -> dict:
        """POST /devices/{serial}/classic/pipes/block-in?endpoint=N&block_size=B — Write to BlockPipeIn (S-05)."""
        return await self._post(
            f"/devices/{serial}/classic/pipes/block-in",
            params={"endpoint": endpoint, "block_size": block_size},
            content=data,
            headers={"Content-Type": "application/octet-stream"},
        )

    async def read_block_pipe_out(self, serial: str, endpoint: int,
                                   length: int, block_size: int) -> bytes:
        """GET /devices/{serial}/classic/pipes/block-out?endpoint=N&block_size=B&length=L — Read from BlockPipeOut (S-05)."""
        return await self._get_bytes(
            f"/devices/{serial}/classic/pipes/block-out",
            params={"endpoint": endpoint, "block_size": block_size, "length": length},
        )

    async def read_register(self, serial: str, address: int) -> dict:
        """POST /devices/{serial}/classic/registers/read — Read register (delegates to bulk endpoint, S-06)."""
        return await self.read_registers(serial, [address])

    async def read_registers(self, serial: str, addresses: list[int]) -> dict:
        """POST /devices/{serial}/classic/registers/read — Bulk read registers (S-06)."""
        return await self._post(
            f"/devices/{serial}/classic/registers/read",
            json={"addresses": addresses},
        )

    async def write_register(self, serial: str, address: int, value: int) -> dict:
        """PUT /devices/{serial}/classic/registers/write — Write register (delegates to bulk endpoint, S-06)."""
        return await self.write_registers(serial, [{"address": address, "data": value}])

    async def write_registers(self, serial: str, entries: list[dict[str, int]]) -> dict:
        """PUT /devices/{serial}/classic/registers/write — Bulk write registers (S-06)."""
        return await self._put(
            f"/devices/{serial}/classic/registers/write",
            json={"entries": entries},
        )

    async def classic_reset(self, serial: str) -> dict:
        """POST /devices/{serial}/classic/reset — Reset FPGA."""
        return await self._post(f"/devices/{serial}/classic/reset")

    # ========================================================================
    # AXI Data Port (/devices/{serial}/axi/*)
    # ========================================================================

    async def axi_lite_read(self, serial: str, address: int) -> dict:
        """POST /devices/{serial}/axi/lite/read."""
        return await self._post(f"/devices/{serial}/axi/lite/read", json={"address": address})

    async def axi_lite_write(self, serial: str, address: int, value: int) -> dict:
        """PUT /devices/{serial}/axi/lite/write.

        This is a PUT (idempotent state assignment per ADR-015).
        """
        return await self._put(
            f"/devices/{serial}/axi/lite/write",
            json={"address": address, "value": value},
        )

    async def axi_lite_reset(self, serial: str) -> dict:
        """POST /devices/{serial}/axi/lite/reset."""
        return await self._post(f"/devices/{serial}/axi/lite/reset")

    async def axi_full_read(self, serial: str, address: int, length: int) -> dict:
        """POST /devices/{serial}/axi/full/read."""
        return await self._post(
            f"/devices/{serial}/axi/full/read",
            json={"address": address, "length": length},
        )

    async def axi_full_write(self, serial: str, address: int, data: bytes) -> dict:
        """POST /devices/{serial}/axi/full/write."""
        return await self._post(
            f"/devices/{serial}/axi/full/write",
            json={"address": address, "data": list(data)},
        )

    async def axi_full_datapath_width(self, serial: str) -> dict:
        """GET /devices/{serial}/axi/full/datapath-width."""
        return await self._get(f"/devices/{serial}/axi/full/datapath-width")

    async def axi_full_reset(self, serial: str) -> dict:
        """POST /devices/{serial}/axi/full/reset."""
        return await self._post(f"/devices/{serial}/axi/full/reset")

    async def axi_stream_read(self, serial: str, length: int) -> dict:
        """POST /devices/{serial}/axi/stream/read."""
        return await self._post(f"/devices/{serial}/axi/stream/read", json={"length": length})

    async def axi_stream_write(self, serial: str, data: bytes) -> dict:
        """POST /devices/{serial}/axi/stream/write."""
        return await self._post(f"/devices/{serial}/axi/stream/write", json={"data": list(data)})

    async def axi_stream_datapath_width(self, serial: str) -> dict:
        """GET /devices/{serial}/axi/stream/datapath-width."""
        return await self._get(f"/devices/{serial}/axi/stream/datapath-width")

    async def axi_stream_reset(self, serial: str) -> dict:
        """POST /devices/{serial}/axi/stream/reset."""
        return await self._post(f"/devices/{serial}/axi/stream/reset")

    # ========================================================================
    # Resources (/resources/*)
    # ========================================================================

    async def list_resources(self) -> dict:
        """GET /resources — List all resources."""
        return await self._get("/resources")

    async def get_resource(self, resource_id: str) -> dict:
        """GET /resources/{resource_id} — Get resource details."""
        return await self._get(f"/resources/{resource_id}")

    async def acquire_resource(
        self, resource_id: str, mode: Optional[str], user_id: str
    ) -> dict:
        """POST /resources/{resource_id}/acquire — Acquire a resource (S-03)."""
        body: dict[str, Any] = {}
        if mode is not None:
            body["mode"] = mode
        return await self._post_with_user(
            f"/resources/{resource_id}/acquire",
            user_id,
            json=body,
        )

    async def release_resource(self, resource_id: str, user_id: str) -> dict:
        """POST /resources/{resource_id}/release — Release a resource."""
        return await self._post_with_user(
            f"/resources/{resource_id}/release", user_id
        )

    async def create_resource(
        self,
        name: str,
        device_serial: str,
        access_mode: str = "shared",
        description: Optional[str] = None,
        interfaces: Optional[list[str]] = None,
        endpoints: Optional[dict[str, Any]] = None,
        address_ranges: Optional[list[dict[str, Any]]] = None,
    ) -> dict:
        """POST /resources — Create a new resource (ADR-020 / ADR-034).

        When ``access_mode == "shared"`` and ``endpoints`` is omitted,
        populates the standard ``Counters.bit`` endpoint and register
        declarations so ``ADR-034 D68`` validation succeeds.
        """
        body: dict[str, Any] = {
            "name": name,
            "device_serial": device_serial,
            "access_mode": access_mode,
            "interfaces": interfaces if interfaces is not None else ["classic"],
        }
        if endpoints is not None:
            body["endpoints"] = endpoints
        elif access_mode == "shared":
            body["endpoints"] = {
                "default_access": "shared",
                "entries": [
                    {"address": "0x00", "type": "wire_in", "access": "shared"},
                    {"address": "0x20", "type": "wire_out", "access": "shared"},
                    {"address": "0x21", "type": "wire_out", "access": "shared"},
                    {"address": "0x22", "type": "wire_out", "access": "shared"},
                    {"address": "0x40", "type": "trigger_in", "access": "shared"},
                    {"address": "0x60", "type": "trigger_out", "access": "shared"},
                    {"address": "0x61", "type": "trigger_out", "access": "shared"},
                ],
            }
        if address_ranges is not None:
            body["address_ranges"] = address_ranges
        elif access_mode == "shared":
            body["address_ranges"] = [
                {
                    "name": "classic_registers",
                    "start": "0x00000000",
                    "end": "0x000000FF",
                    "access": "shared",
                }
            ]
        if description is not None:
            body["description"] = description
        return await self._post("/resources", json=body)

    async def delete_resource(self, resource_id: str) -> None:
        """DELETE /resources/{resource_id} — Delete a resource (ADR-020).

        Returns 409 if the resource is currently held by any user.
        """
        await self._delete(f"/resources/{resource_id}")

    async def lock_resource(self, resource_id: str, user_id: str) -> dict:
        """POST /resources/{resource_id}/lock — Lock a shared resource (ADR-020 §3)."""
        return await self._post_with_user(
            f"/resources/{resource_id}/lock", user_id
        )

    async def unlock_resource(self, resource_id: str, user_id: str) -> dict:
        """POST /resources/{resource_id}/unlock — Unlock a locked resource (ADR-020 §3)."""
        return await self._post_with_user(
            f"/resources/{resource_id}/unlock", user_id
        )

    async def heartbeat(self, user_id: str) -> dict:
        """POST /heartbeat — User liveness heartbeat (ADR-020 §5)."""
        return await self._post_with_user("/heartbeat", user_id)

    async def disconnect(self, user_id: str) -> dict:
        """POST /disconnect — Disconnect user and release all resources."""
        return await self._post_with_user("/disconnect", user_id)

    # ========================================================================
    # Users (/users/*)
    # ========================================================================

    async def list_users(self) -> dict:
        """GET /users — List connected users."""
        return await self._get("/users")

    async def get_user(self, user_id: str) -> dict:
        """GET /users/{user_id} — Get user details."""
        return await self._get(f"/users/{user_id}")
