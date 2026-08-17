"""Interactive demo for the ok-FPGA-Server gRPC client.

Usage:
    python -m okfpga_grpc_client.examples.demo
    python -m okfpga_grpc_client.examples.demo --target localhost:50051
"""

from __future__ import annotations

import argparse
import asyncio
import logging

from okfpga_grpc_client.client import OkFpgaGrpcClient
from okfpga_grpc_client.discovery import discover_servers
from okfpga_grpc_client.mqtt_status import MqttConfig, MqttPublisher

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
        # --- List Devices ---
        print("--- Devices ---")
        try:
            resp = await client.list_devices()
            if not resp.devices:
                print("  No devices connected")
            for dev in resp.devices:
                print(f"  {dev.serial} - {dev.product_name} ({dev.product_id})")
        except Exception as e:
            print(f"  Error: {e}")

        # --- Server Status ---
        print("\n--- Server Status ---")
        try:
            status = await client.get_server_status()
            print(f"  Response: {status}")
        except Exception as e:
            print(f"  Error: {e}")

    # Step 4: MQTT.
    if mqtt:
        mqtt.publish_heartbeat(0)
        mqtt.publish_connection_status("localhost", True, target)
        print("\n--- MQTT ---")
        print("  Published heartbeat and connection status")
        mqtt.publish_offline()
    else:
        print("\n--- MQTT ---")
        print("  Skipped (broker unavailable)")

    print("\nOK: Demo complete.")


if __name__ == "__main__":
    asyncio.run(main())
