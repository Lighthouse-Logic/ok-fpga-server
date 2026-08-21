"""mDNS Service Browser for ok-FPGA-Server discovery.

Discovers ok-FPGA-Server instances on the local network via mDNS/DNS-SD.
Browses for ``_okfpga._tcp.local.`` services and returns a list of discovered
servers with their connection details extracted from TXT records.

Usage::

    from python_rest_gui_common.mdns_browser import MdnsBrowser

    browser = MdnsBrowser()
    browser.start()

    # Wait a few seconds for discovery...
    import time
    time.sleep(3)

    servers = browser.get_servers()
    for s in servers:
        print(f"{s.instance_name}: {s.description} ({s.host}:{s.rest_port})")

    browser.stop()
"""

from __future__ import annotations

import socket
import threading
from dataclasses import dataclass, field
from typing import Optional

from zeroconf import ServiceBrowser, ServiceInfo, ServiceStateChange, Zeroconf

# mDNS service type for ok-FPGA-Server instances.
SERVICE_TYPE = "_okfpga._tcp.local."


@dataclass
class DiscoveredServer:
    """A discovered ok-FPGA-Server instance on the local network."""

    #: The mDNS instance name (e.g., "okfpga-lab-bench-1").
    instance_name: str

    #: Human-readable description from the TXT record.
    description: str = ""

    #: Hostname or IP address of the server.
    host: str = ""

    #: Port the mDNS service was registered on (management port).
    port: int = 0

    #: REST API port (from TXT "rest" key), if the server has REST enabled.
    rest_port: Optional[int] = None

    #: gRPC API port (from TXT "grpc" key), if the server has gRPC enabled.
    grpc_port: Optional[int] = None

    #: Dashboard management port (from TXT "dashboard" key).
    dashboard_port: Optional[int] = None

    #: API version string (from TXT "version" key).
    api_version: str = "unknown"

    #: Number of connected devices (from TXT "devices" key).
    device_count: int = 0


class MdnsBrowser:
    """mDNS service browser for discovering ok-FPGA-Server instances.

    Creates a background listener that watches for ``_okfpga._tcp`` service
    announcements. Discovered servers are accumulated in a thread-safe dict
    and can be retrieved at any time via :meth:`get_servers`.
    """

    def __init__(self) -> None:
        self._zeroconf: Optional[Zeroconf] = None
        self._browser: Optional[ServiceBrowser] = None
        self._servers: dict[str, DiscoveredServer] = {}
        self._lock = threading.Lock()

    def start(self) -> None:
        """Start the mDNS browser.

        Creates a Zeroconf instance and begins browsing for
        ``_okfpga._tcp.local.`` services. Discovered servers are
        accumulated in the background.
        """
        if self._zeroconf is not None:
            return  # Already started.

        self._zeroconf = Zeroconf()
        self._browser = ServiceBrowser(
            self._zeroconf,
            SERVICE_TYPE,
            handlers=[self._on_state_change],
        )

    def stop(self) -> None:
        """Stop the mDNS browser and release resources."""
        if self._browser is not None:
            self._browser.cancel()
            self._browser = None
        if self._zeroconf is not None:
            self._zeroconf.close()
            self._zeroconf = None

    def get_servers(self) -> list[DiscoveredServer]:
        """Return the current list of discovered servers."""
        with self._lock:
            return list(self._servers.values())

    def _on_state_change(
        self,
        zeroconf: Zeroconf,
        service_type: str,
        name: str,
        state_change: ServiceStateChange,
    ) -> None:
        """Handle mDNS service state changes."""
        if state_change == ServiceStateChange.Added:
            info = zeroconf.get_service_info(service_type, name)
            if info is not None:
                self._add_server(name, info)
        elif state_change == ServiceStateChange.Removed:
            with self._lock:
                self._servers.pop(name, None)
        elif state_change == ServiceStateChange.Updated:
            info = zeroconf.get_service_info(service_type, name)
            if info is not None:
                self._add_server(name, info)

    def _add_server(self, name: str, info: ServiceInfo) -> None:
        """Extract server details from a ServiceInfo and store them."""

        def _txt(key: str) -> str:
            """Get a TXT record value as a string."""
            val = info.properties.get(key.encode("utf-8"))
            if val is None:
                return ""
            return val.decode("utf-8", errors="replace")

        def _txt_int(key: str) -> Optional[int]:
            """Get a TXT record value as an optional int."""
            val = _txt(key)
            if not val:
                return None
            try:
                return int(val)
            except ValueError:
                return None

        # Resolve the host address.
        # Prefer IPv4 — the server binds to 0.0.0.0 (IPv4 only).
        # inet_ntoa only handles 4-byte (IPv4) addresses; 16-byte IPv6
        # addresses would crash it. We iterate to find IPv4 first, then
        # fall back to IPv6 with inet_ntop (ADR-027 addendum).
        host = ""
        for addr_bytes in info.addresses:
            if len(addr_bytes) == 4:  # IPv4
                host = socket.inet_ntoa(addr_bytes)
                break
        if not host:
            for addr_bytes in info.addresses:
                if len(addr_bytes) == 16:  # IPv6
                    host = socket.inet_ntop(socket.AF_INET6, addr_bytes)
                    break
        if not host and info.server:
            host = info.server.rstrip(".")

        description = _txt("description")
        if not description:
            # Strip the service type suffix to get just the instance name.
            description = name.replace(f".{SERVICE_TYPE}", "")

        server = DiscoveredServer(
            instance_name=name,
            description=description,
            host=host,
            port=info.port or 0,
            rest_port=_txt_int("rest"),
            grpc_port=_txt_int("grpc"),
            dashboard_port=_txt_int("dashboard"),
            api_version=_txt("version") or "unknown",
            device_count=_txt_int("devices") or 0,
        )

        with self._lock:
            self._servers[name] = server
