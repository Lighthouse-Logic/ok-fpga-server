# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""mDNS Discovery Module.

Discovers ok-FPGA-Server instances on the local network using mDNS/DNS-SD.
Browses for _okfpga._tcp.local. and extracts connection details from TXT records.

NOTE: This discovery module is identical in python-rest-client and python-grpc-client.
If you modify this file, apply the same changes to the other client.
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass, field
from typing import Optional

from zeroconf import ServiceBrowser, ServiceInfo, Zeroconf

logger = logging.getLogger(__name__)

SERVICE_TYPE = "_okfpga._tcp.local."


@dataclass
class DiscoveredServer:
    """Information about a discovered ok-FPGA-Server instance."""

    instance_name: str
    hostname: str
    management_port: int
    addresses: list[str] = field(default_factory=list)
    rest_port: Optional[int] = None
    grpc_port: Optional[int] = None
    mqtt_topic: Optional[str] = None
    device_count: Optional[int] = None
    api_version: Optional[str] = None

    @property
    def rest_url(self) -> Optional[str]:
        """REST API base URL, or None if REST is disabled."""
        if self.rest_port and self.addresses:
            return f"http://{self.addresses[0]}:{self.rest_port}"
        return None

    @property
    def grpc_url(self) -> Optional[str]:
        """gRPC endpoint URL, or None if gRPC is disabled."""
        if self.grpc_port and self.addresses:
            return f"{self.addresses[0]}:{self.grpc_port}"
        return None


class _Listener:
    """Internal mDNS service listener that collects discovered servers."""

    def __init__(self) -> None:
        self.servers: list[DiscoveredServer] = []

    def add_service(self, zc: Zeroconf, type_: str, name: str) -> None:
        info = zc.get_service_info(type_, name)
        if info is None:
            return

        props = {
            k.decode(): v.decode() if v else ""
            for k, v in info.properties.items()
        }

        server = DiscoveredServer(
            instance_name=name,
            hostname=info.server or "",
            management_port=info.port or 0,
            addresses=[str(addr) for addr in info.parsed_scoped_addresses()],
            rest_port=int(props["rest"]) if "rest" in props else None,
            grpc_port=int(props["grpc"]) if "grpc" in props else None,
            mqtt_topic=props.get("mqtt"),
            device_count=int(props["devices"]) if "devices" in props else None,
            api_version=props.get("version"),
        )

        logger.info("Found server: %s at %s:%d", name, server.hostname, server.management_port)
        self.servers.append(server)

    def remove_service(self, zc: Zeroconf, type_: str, name: str) -> None:
        pass  # Not needed for one-shot discovery.

    def update_service(self, zc: Zeroconf, type_: str, name: str) -> None:
        pass  # Not needed for one-shot discovery.


def _discover_sync(timeout: float) -> list[DiscoveredServer]:
    zc = Zeroconf()
    listener = _Listener()
    browser = ServiceBrowser(zc, SERVICE_TYPE, listener)

    time.sleep(timeout)

    browser.cancel()
    zc.close()
    return listener.servers


def discover_servers(timeout: float = 3.0) -> list[DiscoveredServer]:
    """Browse the local network for ok-FPGA-Server instances.

    Safe to call from both synchronous code and inside an active ``asyncio``
    event loop (offloads blocking ``Zeroconf`` I/O to a worker thread when an
    event loop is running on the current thread).

    Args:
        timeout: How long to wait for mDNS responses (seconds).

    Returns:
        List of discovered servers (may be empty).
    """
    import asyncio
    from concurrent.futures import ThreadPoolExecutor

    logger.info("Browsing for %s (timeout: %.1fs)...", SERVICE_TYPE, timeout)

    try:
        asyncio.get_running_loop()
        in_loop = True
    except RuntimeError:
        in_loop = False

    if in_loop:
        with ThreadPoolExecutor(max_workers=1) as pool:
            servers = pool.submit(_discover_sync, timeout).result()
    else:
        servers = _discover_sync(timeout)

    if not servers:
        logger.warning("No ok-FPGA-Server instances found via mDNS")

    return servers
