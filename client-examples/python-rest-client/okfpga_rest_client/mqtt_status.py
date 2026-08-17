"""MQTT Status Publisher.

Publishes client connection status to an MQTT broker. Topic structure
follows ADR-014: okfpga-client/{client_id}/...

NOTE: This MQTT module is identical in python-rest-client and python-grpc-client.
If you modify this file, apply the same changes to the other client.
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from typing import Any, Optional

import paho.mqtt.client as mqtt_client

logger = logging.getLogger(__name__)


@dataclass
class MqttConfig:
    """MQTT publisher configuration."""

    broker_host: str = "127.0.0.1"
    broker_port: int = 1883
    client_id: str = "okfpga-python-rest-client"
    topic_prefix: str = "okfpga-client/python-rest-01"
    client_type: str = "python-rest"


class MqttPublisher:
    """MQTT status publisher for the client.

    Publishes heartbeat, connection status, and resource state.
    If the broker is unavailable, all operations silently fail with a warning.
    """

    def __init__(self, config: MqttConfig) -> None:
        self._prefix = config.topic_prefix
        self._client_type = config.client_type
        self._client = mqtt_client.Client(
            callback_api_version=mqtt_client.CallbackAPIVersion.VERSION2,
            client_id=config.client_id,
        )

        # Set Last Will (offline status if we disconnect unexpectedly).
        self._client.will_set(
            f"{self._prefix}/status",
            payload=json.dumps({"online": False}),
            qos=1,
            retain=True,
        )

        try:
            self._client.connect(config.broker_host, config.broker_port, keepalive=30)
            self._client.loop_start()
            self._connected = True
            logger.info("MQTT connected to %s:%d", config.broker_host, config.broker_port)
        except Exception as e:
            self._connected = False
            logger.warning("MQTT broker unavailable — skipping MQTT: %s", e)

    @staticmethod
    def try_new(config: MqttConfig) -> Optional["MqttPublisher"]:
        """Try to create a publisher. Returns None if broker unavailable."""
        pub = MqttPublisher(config)
        return pub if pub._connected else None

    def publish_heartbeat(self, uptime_secs: int = 0) -> None:
        """Publish a heartbeat status message."""
        if not self._connected:
            return
        self._publish(
            f"{self._prefix}/status",
            {
                "online": True,
                "uptime_secs": uptime_secs,
                "client_type": self._client_type,
                "version": "0.1.0",
            },
            retain=True,
        )

    def publish_connection_status(
        self,
        server_instance: str,
        connected: bool,
        server_host: str,
        rest_port: Optional[int] = None,
    ) -> None:
        """Publish per-server connection status."""
        if not self._connected:
            return
        self._publish(
            f"{self._prefix}/connections/{server_instance}/status",
            {
                "connected": connected,
                "server_host": server_host,
                "rest_port": rest_port,
            },
        )

    def publish_resource_inventory(self, resources: list[dict[str, Any]]) -> None:
        """Publish resource inventory snapshot."""
        if not self._connected:
            return
        self._publish(f"{self._prefix}/resources/inventory", {"resources": resources})

    def publish_offline(self) -> None:
        """Publish offline status (clean shutdown)."""
        if not self._connected:
            return
        self._publish(f"{self._prefix}/status", {"online": False}, retain=True)
        self._client.loop_stop()
        self._client.disconnect()

    def _publish(self, topic: str, payload: dict, retain: bool = False) -> None:
        """Internal publish helper."""
        try:
            self._client.publish(topic, json.dumps(payload), qos=1, retain=retain)
        except Exception as e:
            logger.warning("MQTT publish failed: %s", e)
