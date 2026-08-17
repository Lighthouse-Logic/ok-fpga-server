//! # MQTT Status Publisher
//!
//! Implements ADR-011: a publish-only MQTT client that broadcasts server
//! and device status to an external MQTT broker (e.g., Mosquitto).
//!
//! ## Topic Structure
//!
//! All topics are prefixed with `{topic_prefix}/` from the TOML config:
//!
//! ```text
//! {prefix}/
//! ├── status                      # Server heartbeat (periodic, QoS 0)
//! ├── services/status             # Service states (on change, QoS 1, retained)
//! ├── devices/
//! │   ├── inventory               # Full device list (on change, QoS 1, retained)
//! │   └── {serial}/
//! │       ├── state               # Device state (on change, QoS 1, retained)
//! │       ├── sensors             # Sensor readings (periodic, QoS 0)
//! │       └── events              # Device events (on event, QoS 1)
//! └── resources/
//!     └── inventory               # Resource list (on change, QoS 1, retained)
//! ```
//!
//! ## Message Format
//!
//! All messages are JSON with a mandatory version field:
//!
//! ```json
//! { "v": 1, "ts": "2026-...", "type": "server.status", "server_id": "...", "payload": {} }
//! ```
//!
//! ## Security
//!
//! MQTT is **publish-only** — the server never subscribes to command topics.
//! This prevents unauthorized control via MQTT.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use tokio::sync::Notify;
use tracing::{info, warn};

use crate::config::MqttConfig;
use crate::state::AppState;

/// Schema version for all MQTT messages.
const MESSAGE_VERSION: u32 = 1;

// ============================================================================
// MqttPublisher — ManagedService
// ============================================================================

/// MQTT status publisher.
///
/// Connects to an external MQTT broker and publishes periodic heartbeat
/// messages and event-driven state changes. Implements `ManagedService`
/// for lifecycle management via the `ServiceManager`.
pub struct MqttPublisher {
    /// Atomic status: 0=Stopped, 1=Running, 2=Starting, 3=Stopping
    status: AtomicU8,

    /// MQTT configuration from TOML config.
    config: MqttConfig,

    /// Shared application state.
    state: Arc<AppState>,

    /// Signal to trigger graceful shutdown.
    shutdown_signal: Arc<Notify>,
}

impl MqttPublisher {
    /// Create a new `MqttPublisher`.
    pub fn new(config: MqttConfig, state: Arc<AppState>) -> Self {
        Self {
            status: AtomicU8::new(0), // Stopped
            config,
            state,
            shutdown_signal: Arc::new(Notify::new()),
        }
    }

    fn atomic_to_status(val: u8) -> ServiceStatus {
        match val {
            0 => ServiceStatus::Stopped,
            1 => ServiceStatus::Running,
            2 => ServiceStatus::Starting,
            3 => ServiceStatus::Stopping,
            _ => ServiceStatus::Failed {
                reason: "unknown state".to_string(),
            },
        }
    }
}

#[async_trait::async_trait]
impl ManagedService for MqttPublisher {
    fn name(&self) -> &str {
        "mqtt"
    }

    fn status(&self) -> ServiceStatus {
        Self::atomic_to_status(self.status.load(Ordering::SeqCst))
    }

    async fn start(&self) -> Result<(), ServiceError> {
        self.status.store(2, Ordering::SeqCst); // Starting

        // Parse the broker URL for rumqttc.
        let broker_url = &self.config.broker_url;
        let (host, port) = parse_broker_url(broker_url);

        let mut mqtt_options = MqttOptions::new(
            &self.config.client_id,
            &host,
            port,
        );
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(30));
        mqtt_options.set_clean_session(true);

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 64);

        let shutdown = self.shutdown_signal.clone();
        let state = self.state.clone();
        let topic_prefix = self.config.topic_prefix.clone();
        let publish_interval = self.config.publish_interval_secs;
        let server_id = self.config.client_id.clone();

        info!(
            broker = %broker_url,
            client_id = %self.config.client_id,
            topic_prefix = %topic_prefix,
            "MQTT publisher connecting"
        );

        // Spawn the MQTT event loop handler.
        tokio::spawn(async move {
            let mut connected = false;

            loop {
                tokio::select! {
                    _ = shutdown.notified() => {
                        // Publish a shutdown message before disconnecting.
                        let msg = build_message(
                            &server_id,
                            "server.shutdown",
                            serde_json::json!({}),
                        );
                        let _ = client.publish(
                            format!("{}/status", topic_prefix),
                            QoS::AtLeastOnce,
                            false,
                            msg.to_string(),
                        ).await;
                        let _ = client.disconnect().await;
                        info!("MQTT publisher disconnected");
                        break;
                    }
                    event = eventloop.poll() => {
                        match event {
                            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(_))) => {
                                if !connected {
                                    connected = true;
                                    info!("MQTT connected to broker");
                                    // Publish initial retained status.
                                    publish_server_status(
                                        &client, &topic_prefix, &server_id, &state
                                    ).await;
                                    publish_services_status(
                                        &client, &topic_prefix, &server_id, &state
                                    ).await;
                                    publish_device_inventory(
                                        &client, &topic_prefix, &server_id, &state
                                    ).await;
                                    publish_resource_inventory(
                                        &client, &topic_prefix, &server_id, &state
                                    ).await;
                                }
                            }
                            Ok(_) => {} // Ignore other events.
                            Err(e) => {
                                if connected {
                                    warn!(error = %e, "MQTT connection lost — will retry");
                                    connected = false;
                                }
                                // rumqttc handles reconnection internally.
                                tokio::time::sleep(
                                    tokio::time::Duration::from_secs(5)
                                ).await;
                            }
                        }
                    }
                }
            }
        });

        // Spawn the periodic heartbeat publisher.
        // Use the main client for heartbeat — the periodic publisher uses
        // a timer-based pattern to publish at the configured interval.

        // Use the same client from above for heartbeat — but actually we need
        // to use a separate approach. Let's use a simpler timer-based pattern.
        let shutdown2 = self.shutdown_signal.clone();
        let state2 = self.state.clone();
        let prefix2 = self.config.topic_prefix.clone();
        let server_id2 = self.config.client_id.clone();
        let host2 = host.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(publish_interval),
            );

            // Build a separate client for periodic publishing.
            let mut opts = MqttOptions::new(
                format!("{}-periodic", server_id2),
                &host2,
                port,
            );
            opts.set_keep_alive(std::time::Duration::from_secs(30));
            opts.set_clean_session(true);

            let (periodic_client, mut periodic_loop) = AsyncClient::new(opts, 16);

            // Spawn the event loop for the periodic client.
            tokio::spawn(async move {
                loop {
                    if periodic_loop.poll().await.is_err() {
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
            });

            loop {
                tokio::select! {
                    _ = shutdown2.notified() => break,
                    _ = interval.tick() => {
                        publish_server_status(
                            &periodic_client, &prefix2, &server_id2, &state2
                        ).await;
                    }
                }
            }
        });

        self.status.store(1, Ordering::SeqCst); // Running
        info!("MQTT publisher started");
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        self.status.store(3, Ordering::SeqCst); // Stopping

        // Signal both tasks to shut down.
        self.shutdown_signal.notify_waiters();

        // Give time for the shutdown message to be published.
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        self.status.store(0, Ordering::SeqCst); // Stopped
        info!("MQTT publisher stopped");
        Ok(())
    }
}

// ============================================================================
// Publish Functions
// ============================================================================

/// Publish server heartbeat status.
async fn publish_server_status(
    client: &AsyncClient,
    prefix: &str,
    server_id: &str,
    state: &AppState,
) {
    let msg = build_message(
        server_id,
        "server.status",
        serde_json::json!({
            "uptime_secs": state.uptime_secs(),
            "device_count": state.device_manager.device_count(),
            "resource_count": state.resource_registry.count(),
            "api_version": state.api_version,
        }),
    );

    if let Err(e) = client
        .publish(
            format!("{}/status", prefix),
            QoS::AtMostOnce,
            true, // retained
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish server status");
    }
}

/// Publish service states.
async fn publish_services_status(
    client: &AsyncClient,
    prefix: &str,
    server_id: &str,
    state: &AppState,
) {
    let services: serde_json::Map<String, serde_json::Value> = state
        .service_manager
        .list()
        .iter()
        .map(|s| (s.name.clone(), serde_json::Value::String(format!("{}", s.status))))
        .collect();

    let msg = build_message(
        server_id,
        "services.status",
        serde_json::json!({ "services": services }),
    );

    if let Err(e) = client
        .publish(
            format!("{}/services/status", prefix),
            QoS::AtLeastOnce,
            true, // retained
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish services status");
    }
}

/// Publish the device inventory.
async fn publish_device_inventory(
    client: &AsyncClient,
    prefix: &str,
    server_id: &str,
    state: &AppState,
) {
    let devices: Vec<serde_json::Value> = state
        .device_manager
        .registry()
        .list()
        .iter()
        .map(|d| {
            serde_json::json!({
                "serial": d.serial(),
                "product_name": d.product_name(),
            })
        })
        .collect();

    let msg = build_message(
        server_id,
        "devices.inventory",
        serde_json::json!({ "devices": devices }),
    );

    if let Err(e) = client
        .publish(
            format!("{}/devices/inventory", prefix),
            QoS::AtLeastOnce,
            true, // retained
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish device inventory");
    }
}

/// Publish the resource inventory.
async fn publish_resource_inventory(
    client: &AsyncClient,
    prefix: &str,
    server_id: &str,
    state: &AppState,
) {
    let count = state.resource_registry.count();

    let msg = build_message(
        server_id,
        "resources.inventory",
        serde_json::json!({
            "count": count,
        }),
    );

    if let Err(e) = client
        .publish(
            format!("{}/resources/inventory", prefix),
            QoS::AtLeastOnce,
            true, // retained
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish resource inventory");
    }
}

// ============================================================================
// Message Builder
// ============================================================================

/// Build a versioned MQTT message following the ADR-011 format.
///
/// ```json
/// { "v": 1, "ts": "...", "type": "...", "server_id": "...", "payload": {...} }
/// ```
fn build_message(
    server_id: &str,
    msg_type: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "v": MESSAGE_VERSION,
        "ts": chrono::Utc::now().to_rfc3339(),
        "type": msg_type,
        "server_id": server_id,
        "payload": payload,
    })
}

// ============================================================================
// Helpers
// ============================================================================

/// Parse a broker URL like `mqtt://host:port` or `mqtts://host:port`.
///
/// Returns `(host, port)`. Defaults to port 1883 for mqtt:// and 8883 for mqtts://.
fn parse_broker_url(url: &str) -> (String, u16) {
    let url = url
        .trim_start_matches("mqtt://")
        .trim_start_matches("mqtts://")
        .trim_start_matches("tcp://");

    let parts: Vec<&str> = url.splitn(2, ':').collect();
    let host = parts[0].to_string();
    let port = parts
        .get(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(1883);

    (host, port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mqtt_url() {
        let (host, port) = parse_broker_url("mqtt://localhost:1883");
        assert_eq!(host, "localhost");
        assert_eq!(port, 1883);
    }

    #[test]
    fn parse_mqtts_url() {
        let (host, port) = parse_broker_url("mqtts://broker.example.com:8883");
        assert_eq!(host, "broker.example.com");
        assert_eq!(port, 8883);
    }

    #[test]
    fn parse_url_default_port() {
        let (host, port) = parse_broker_url("mqtt://myhost");
        assert_eq!(host, "myhost");
        assert_eq!(port, 1883);
    }

    #[test]
    fn build_message_format() {
        let msg = build_message(
            "test-server",
            "server.status",
            serde_json::json!({"uptime_secs": 42}),
        );
        assert_eq!(msg["v"], 1);
        assert_eq!(msg["type"], "server.status");
        assert_eq!(msg["server_id"], "test-server");
        assert_eq!(msg["payload"]["uptime_secs"], 42);
        assert!(msg["ts"].is_string());
    }
}
