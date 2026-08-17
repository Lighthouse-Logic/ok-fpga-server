//! # MQTT Status Publisher
//!
//! Publishes client connection status to an MQTT broker using the topic
//! structure defined in ADR-014:
//!
//! ```text
//! okfpga-client/{client_id}/
//! ├── status                    # Heartbeat
//! ├── connections/{server}/status  # Per-server connection
//! └── resources/inventory       # Acquired resources
//! ```
//!
//! ## NOTE
//! This MQTT module is identical in `rust-rest-client` and `rust-grpc-client`.
//! If you modify this file, apply the same changes to the other client.

use std::time::Duration;

use rumqttc::{Client, MqttOptions, QoS};
use tracing::{info, warn};

/// MQTT status publisher for the client.
///
/// Publishes heartbeat, connection status, and resource state to an MQTT
/// broker. If the broker is unavailable, all publish operations silently
/// fail with a warning log.
pub struct MqttPublisher {
    /// The rumqttc client handle.
    client: Client,
    /// Topic prefix (e.g., "okfpga-client/rust-rest-01").
    prefix: String,
    /// Client type identifier (e.g., "rust-rest").
    client_type: String,
}

/// Configuration for the MQTT publisher.
#[derive(Debug, Clone)]
pub struct MqttConfig {
    /// Broker host (e.g., "127.0.0.1").
    pub broker_host: String,
    /// Broker port (e.g., 1883).
    pub broker_port: u16,
    /// Client ID — must be unique per broker.
    pub client_id: String,
    /// Topic prefix (e.g., "okfpga-client/rust-rest-01").
    pub topic_prefix: String,
    /// Client type (e.g., "rust-rest" or "rust-grpc").
    pub client_type: String,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker_host: "127.0.0.1".to_string(),
            broker_port: 1883,
            client_id: "okfpga-rust-rest-client".to_string(),
            topic_prefix: "okfpga-client/rust-rest-01".to_string(),
            client_type: "rust-rest".to_string(),
        }
    }
}

impl MqttPublisher {
    /// Try to connect to the MQTT broker. Returns None if the broker is
    /// unavailable (logs a warning but does not fail).
    pub fn try_new(config: MqttConfig) -> Option<Self> {
        let mut opts = MqttOptions::new(
            &config.client_id,
            &config.broker_host,
            config.broker_port,
        );
        opts.set_keep_alive(Duration::from_secs(30));

        // Set a Last Will and Testament (LWT) so the broker publishes
        // an offline status if this client disconnects unexpectedly.
        let lwt_topic = format!("{}/status", config.topic_prefix);
        let lwt_payload = serde_json::json!({ "online": false }).to_string();
        opts.set_last_will(rumqttc::LastWill::new(
            &lwt_topic,
            lwt_payload,
            QoS::AtLeastOnce,
            true, // retained
        ));

        let (client, mut connection) = Client::new(opts, 10);

        // Spawn a background thread to drive the MQTT event loop.
        // rumqttc requires polling the connection to send/receive messages.
        std::thread::spawn(move || {
            for notification in connection.iter() {
                match notification {
                    Ok(_) => {}
                    Err(e) => {
                        // Connection errors are expected when broker is down.
                        // Log at debug level to avoid spamming.
                        tracing::debug!("MQTT connection event: {}", e);
                    }
                }
            }
        });

        // Test the connection by trying to publish a heartbeat.
        // If this fails, the broker is likely unavailable.
        let test_topic = format!("{}/status", config.topic_prefix);
        let test_payload = serde_json::json!({
            "online": true,
            "client_type": config.client_type,
            "version": "0.1.0",
        })
        .to_string();

        match client.publish(&test_topic, QoS::AtLeastOnce, true, test_payload.as_bytes()) {
            Ok(_) => {
                info!("MQTT connected to {}:{}", config.broker_host, config.broker_port);
                Some(Self {
                    client,
                    prefix: config.topic_prefix,
                    client_type: config.client_type,
                })
            }
            Err(e) => {
                warn!(
                    "MQTT broker unavailable at {}:{} — skipping MQTT: {}",
                    config.broker_host, config.broker_port, e
                );
                None
            }
        }
    }

    /// Publish a heartbeat status message.
    pub fn publish_heartbeat(&self, uptime_secs: u64) {
        let topic = format!("{}/status", self.prefix);
        let payload = serde_json::json!({
            "online": true,
            "uptime_secs": uptime_secs,
            "client_type": self.client_type,
            "version": "0.1.0",
        })
        .to_string();

        if let Err(e) = self.client.publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes()) {
            warn!("Failed to publish heartbeat: {}", e);
        }
    }

    /// Publish connection status for a specific server.
    pub fn publish_connection_status(
        &self,
        server_instance: &str,
        connected: bool,
        server_host: &str,
        rest_port: Option<u16>,
    ) {
        let topic = format!("{}/connections/{}/status", self.prefix, server_instance);
        let payload = serde_json::json!({
            "connected": connected,
            "server_host": server_host,
            "rest_port": rest_port,
        })
        .to_string();

        if let Err(e) = self.client.publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes()) {
            warn!("Failed to publish connection status: {}", e);
        }
    }

    /// Publish resource inventory snapshot.
    pub fn publish_resource_inventory(&self, resources: &[serde_json::Value]) {
        let topic = format!("{}/resources/inventory", self.prefix);
        let payload = serde_json::json!({ "resources": resources }).to_string();

        if let Err(e) = self.client.publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes()) {
            warn!("Failed to publish resource inventory: {}", e);
        }
    }

    /// Publish an offline status (called during clean shutdown).
    pub fn publish_offline(&self) {
        let topic = format!("{}/status", self.prefix);
        let payload = serde_json::json!({ "online": false }).to_string();
        let _ = self.client.publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes());
    }
}
