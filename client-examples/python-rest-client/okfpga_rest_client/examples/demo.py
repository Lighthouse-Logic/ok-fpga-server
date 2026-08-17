"""Interactive demo for the ok-FPGA-Server REST client.

Usage:
    python -m okfpga_rest_client.examples.demo
    python -m okfpga_rest_client.examples.demo --url http://localhost:8088
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import sys

from okfpga_rest_client.client import OkFpgaRestClient
from okfpga_rest_client.discovery import discover_servers
from okfpga_rest_client.mqtt_status import MqttConfig, MqttPublisher

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
logger = logging.getLogger(__name__)


async def main() -> None:
    parser = argparse.ArgumentParser(description="ok-FPGA-Server REST client demo")
    parser.add_argument("--url", help="REST API base URL (overrides mDNS)")
    parser.add_argument("--mqtt-host", default="127.0.0.1", help="MQTT broker host")
    parser.add_argument("--mqtt-port", type=int, default=1883, help="MQTT broker port")
    parser.add_argument("--no-mdns", action="store_true", help="Skip mDNS discovery")
    args = parser.parse_args()

    # Step 1: Discover server or use provided URL.
    if args.url:
        base_url = args.url
    elif args.no_mdns:
        base_url = "http://localhost:8088"
    else:
        servers = discover_servers(timeout=3.0)
        if servers and servers[0].rest_url:
            base_url = servers[0].rest_url
            logger.info("Discovered: %s → %s", servers[0].instance_name, base_url)
        else:
            base_url = "http://localhost:8088"
            logger.info("No servers found via mDNS, using %s", base_url)

    # Step 2: MQTT (optional).
    mqtt = MqttPublisher.try_new(MqttConfig(
        broker_host=args.mqtt_host,
        broker_port=args.mqtt_port,
    ))

    # Step 3: Exercise REST endpoints.
    print()
    print("╔══════════════════════════════════════════════════╗")
    print("║     ok-FPGA-Server Python REST Client Demo      ║")
    print(f"║  Connecting to: {base_url:<33}║")
    print("╚══════════════════════════════════════════════════╝")
    print()

    async with OkFpgaRestClient(base_url) as client:
        # --- Server Status ---
        print("━━━ Server Status ━━━")
        try:
            status = await client.get_server_status()
            print(f"  API Version:  {status.get('api_version', 'N/A')}")
            print(f"  Uptime:       {status.get('uptime_secs', 'N/A')}s")
            print(f"  Devices:      {status.get('device_count', 'N/A')}")
        except Exception as e:
            print(f"  Error: {e}")

        # --- Services ---
        print("\n━━━ Services ━━━")
        try:
            svc_resp = await client.list_services()
            for svc in svc_resp.get("services", []):
                print(f"  {svc['name']} — {svc['status']} (compiled={svc['compiled']})")
        except Exception as e:
            print(f"  Error: {e}")

        # --- Devices ---
        print("\n━━━ Devices ━━━")
        devices = []
        try:
            dev_resp = await client.list_devices()
            devices = dev_resp.get("devices", [])
            if not devices:
                print("  No devices connected")
            for dev in devices:
                print(f"  {dev['serial']} — {dev['product_name']} "
                      f"(programmed: {dev['is_programmed']})")
        except Exception as e:
            print(f"  Error: {e}")

        # --- Device Detail ---
        if devices:
            serial = devices[0]["serial"]
            print(f"\n━━━ Device Detail: {serial} ━━━")
            try:
                info = await client.get_device_info(serial)
                print(f"  Info: {info}")
            except Exception as e:
                print(f"  Device info: {e}")

            # Stub endpoints (expect 501)
            print("\n━━━ Stub Endpoints (expect 501) ━━━")
            for name, coro in [
                ("Sensors", client.get_device_sensors(serial)),
                ("Settings", client.get_device_settings(serial)),
                ("SYZYGY", client.get_syzygy_ports(serial)),
                ("Bitfile", client.get_bitfile_info(serial)),
                ("Dataport", client.get_dataport_type(serial)),
            ]:
                try:
                    result = await coro
                    print(f"  {name}: {result}")
                except Exception as e:
                    print(f"  {name}: {e} ✓ (stub)")

        # --- Resources ---
        print("\n━━━ Resources ━━━")
        try:
            res = await client.list_resources()
            print(f"  {res}")
        except Exception as e:
            print(f"  Resources: {e}")

        # --- Users ---
        print("\n━━━ Users ━━━")
        try:
            users = await client.list_users()
            print(f"  {users}")
        except Exception as e:
            print(f"  Users: {e}")

    # Step 4: MQTT status.
    if mqtt:
        mqtt.publish_heartbeat(0)
        mqtt.publish_connection_status("localhost", True, base_url, 8088)
        print("\n━━━ MQTT ━━━")
        print("  Published heartbeat and connection status")
        mqtt.publish_offline()
    else:
        print("\n━━━ MQTT ━━━")
        print("  Skipped (broker unavailable)")

    print("\n✅ Demo complete.")


if __name__ == "__main__":
    asyncio.run(main())
