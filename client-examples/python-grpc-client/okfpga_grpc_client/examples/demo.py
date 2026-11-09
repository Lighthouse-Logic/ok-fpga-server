# Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
# SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial

"""Interactive demo for the ok-FPGA-Server gRPC client.

Usage:
    python -m okfpga_grpc_client.examples.demo
    python -m okfpga_grpc_client.examples.demo --target localhost:50051
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import sys

from okfpga_grpc_client.client import OkFpgaGrpcClient
from okfpga_grpc_client.discovery import discover_servers
from okfpga_grpc_client.mqtt_status import MqttConfig, MqttPublisher

# Phase 35-B: Reconfigure stdout for UTF-8 on Windows to avoid
# UnicodeEncodeError when printing box-drawing characters (━, ╔, etc.)
# on the default cp1252 console.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
logger = logging.getLogger(__name__)


async def main() -> None:
    parser = argparse.ArgumentParser(description="ok-FPGA-Server gRPC client demo")
    parser.add_argument("--target", help="gRPC server address (overrides mDNS)")
    parser.add_argument("--mqtt-host", default="127.0.0.1", help="MQTT broker host")
    parser.add_argument("--mqtt-port", type=int, default=1883, help="MQTT broker port")
    parser.add_argument("--no-mdns", action="store_true", help="Skip mDNS discovery")
    args = parser.parse_args()

    # Step 1: Discover or use provided target.
    if args.target:
        target = args.target
    elif args.no_mdns:
        target = "localhost:50051"
    else:
        servers = discover_servers(timeout=3.0)
        if servers and servers[0].grpc_url:
            target = servers[0].grpc_url
            logger.info("Discovered: %s → %s", servers[0].instance_name, target)
        else:
            target = "localhost:50051"
            logger.info("No servers found via mDNS, using %s", target)

    # Step 2: MQTT (optional).
    mqtt = MqttPublisher.try_new(MqttConfig(
        broker_host=args.mqtt_host,
        broker_port=args.mqtt_port,
        client_id="okfpga-python-grpc-client",
        topic_prefix="okfpga-client/python-grpc-01",
        client_type="python-grpc",
    ))

    # Step 3: Connect and exercise gRPC RPCs.
    print()
    print("+" + "=" * 50 + "+")
    print("|    ok-FPGA-Server Python gRPC Client Demo       |")
    print(f"|  Connecting to: {target:<33}|")
    print("+" + "=" * 50 + "+")
    print()

    async with OkFpgaGrpcClient(target) as client:
        # --- Server Status ---
        print("━━━ Server Status ━━━")
        try:
            status = await client.get_server_status()
            print(f"  API Version:  {status.api_version}")
            print(f"  Uptime:       {status.uptime_secs}s")
            print(f"  Devices:      {status.device_count}")
        except Exception as e:
            print(f"  Error: {e}")

        # --- List Services ---
        print("\n━━━ Services ━━━")
        try:
            resp = await client.list_services()
            for svc in resp.services:
                print(
                    f"  {svc.name} — status: {svc.status}, "
                    f"compiled: {svc.compiled}, enabled: {svc.enabled}"
                )
        except Exception as e:
            print(f"  Error: {e}")

        # --- List Devices ---
        print("\n━━━ Devices ━━━")
        first_serial = None
        try:
            resp = await client.list_devices()
            if not resp.devices:
                print("  No devices connected")
            for dev in resp.devices:
                print(f"  {dev.serial} — {dev.product_name} ({dev.product_id})")
                if first_serial is None:
                    first_serial = dev.serial
        except Exception as e:
            print(f"  Error: {e}")

        # --- Device Detail (if any device found) ---
        if first_serial:
            # Device Info
            print(f"\n━━━ Device Detail: {first_serial} ━━━")
            try:
                resp = await client.get_device_info(first_serial)
                dev = resp.device
                if dev:
                    print(f"  Product:    {dev.product_name} (ID {dev.product_id})")
                    print(f"  USB Speed:  {dev.usb_speed}")
                    print(f"  Firmware:   {dev.device_major_version}.{dev.device_minor_version}")
                    print(f"  Dataport:   {dev.dataport}")
                    if dev.HasField("bitfile"):
                        print(f"  Bitfile:    {dev.bitfile.filename}")
                        if dev.bitfile.programmed_at:
                            print(f"  Programmed: {dev.bitfile.programmed_at} by {dev.bitfile.programmed_by}")
                    else:
                        print("  Bitfile:    (not programmed)")
            except Exception as e:
                print(f"  Error: {e}")

            # Device Settings
            print("\n━━━ Device Settings ━━━")
            try:
                resp = await client.get_device_settings(first_serial)
                print(f"  {len(resp.settings)} settings found:")
                for s in resp.settings:
                    if s.HasField("value"):
                        which = s.WhichOneof("value")
                        if which == "int_value":
                            print(f"    {s.key} = {s.int_value} (integer)")
                        elif which == "string_value":
                            print(f'    {s.key} = "{s.string_value}"')
                    else:
                        print(f"    {s.key} = <unknown type>")
            except Exception as e:
                print(f"  Error: {e}")

            # SYZYGY Ports
            print("\n━━━ SYZYGY Ports ━━━")
            try:
                resp = await client.get_syzygy_ports(first_serial)
                if not resp.ports:
                    print("  No SYZYGY ports found")
                else:
                    print(f"  {len(resp.ports)} ports discovered:")
                    for port in resp.ports:
                        if port.is_populated:
                            print(
                                f"    Port {port.index}: {port.manufacturer_name} "
                                f"{port.product_name} ({port.port_status_mv}mV) "
                                f"— SN: {port.serial_number}"
                            )
                        else:
                            print(f"    Port {port.index}: empty (unpowered)")
            except Exception as e:
                print(f"  Error: {e}")

            # Sensors
            print("\n━━━ Sensors ━━━")
            try:
                resp = await client.get_device_sensors(first_serial)
                if not resp.sensors:
                    print("  No sensors found")
                else:
                    for s in resp.sensors:
                        print(f"  {s.name} = {s.value} ({s.description})")
            except Exception as e:
                print(f"  {e} (expected on boards without on-board sensors)")

        # --- Resources ---
        print("\n━━━ Resources ━━━")
        try:
            resp = await client.list_resources()
            if not resp.resources:
                print("  No resources registered")
            else:
                for r in resp.resources:
                    print(
                        f"  {r.id} — {r.name} [{r.state}] "
                        f"(device: {r.device_serial})"
                    )
        except Exception as e:
            print(f"  Error: {e}")

        # --- Users ---
        print("\n━━━ Users ━━━")
        try:
            resp = await client.list_users()
            if not resp.users:
                print("  No active user sessions")
            else:
                for u in resp.users:
                    print(f"  {u.user_id} — connected: {u.connected_at} (resources: {list(u.held_resource_ids)})")
        except Exception as e:
            print(f"  Error: {e}")

        # --- Heartbeat ---
        print("\n━━━ Heartbeat ━━━")
        try:
            resp = await client.ping("python-grpc-demo")
            print(f"  Pong — server time: {resp.server_time}")
        except Exception as e:
            print(f"  Error: {e}")

    # Step 4: MQTT.
    if mqtt:
        mqtt.publish_heartbeat(0)
        mqtt.publish_connection_status("localhost", True, target)
        print("\n━━━ MQTT ━━━")
        print("  Published heartbeat and connection status")
        mqtt.publish_offline()
    else:
        print("\n━━━ MQTT ━━━")
        print("  Skipped (broker unavailable)")

    print("\n✅ Demo complete.")


if __name__ == "__main__":
    asyncio.run(main())
