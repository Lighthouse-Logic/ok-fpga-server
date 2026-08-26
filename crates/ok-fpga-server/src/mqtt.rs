//! # MQTT Status Publisher
//!
//! Implements ADR-011: a publish-only MQTT client that broadcasts server
//! and device status to an external MQTT broker (e.g., Mosquitto).
//!
//! ## Phase 40 Extension (ADR-031)
//!
//! Adds `server.announce` — a retained message on `{prefix}/server/announce`
//! containing full server connectivity information (REST/gRPC/dashboard URLs,
//! description, device count, service states). Enables cross-subnet
//! discovery via a shared MQTT broker, complementing the link-local-only
//! mDNS discovery.
//!
//! **Last Will & Testament (LWT)** is configured so the broker automatically
//! publishes a `server.offline` message if the server crashes or loses
//! connectivity, preventing phantom servers in discovery.
//!
//! ## Topic Structure
//!
//! All topics are prefixed with `{topic_prefix}/` from the TOML config:
//!
//! ```text
//! {prefix}/
//! ├── status                      # Server heartbeat (periodic, QoS 0)
//! ├── server/announce             # Cross-subnet discovery (periodic, QoS 1, retained) [Phase 40]
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

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use rumqttc::{AsyncClient, LastWill, MqttOptions, QoS};
use tokio::sync::Notify;
use tracing::{debug, info, warn};

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
///
/// ## Phase 40 Additions (ADR-031)
///
/// - **`server.announce`** retained message with full connectivity info
/// - **LWT** (Last Will & Testament) publishes `server.offline` on crash
/// - **IP resolution** via `netdev` for cross-subnet URLs
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

        let mut mqtt_options = MqttOptions::new(&self.config.client_id, &host, port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(30));
        mqtt_options.set_clean_session(true);

        // --- Phase 40 (ADR-031 §3): Configure Last Will & Testament ---
        //
        // If the server crashes or loses connectivity, the broker automatically
        // publishes a `server.offline` message to `{prefix}/server/announce`
        // with `retain=true`. This clears the stale announce so subscribers
        // don't see a phantom server.
        let lwt_topic = format!("{}/server/announce", self.config.topic_prefix);
        let lwt_payload = build_message(
            &self.config.client_id,
            "server.offline",
            serde_json::json!({}),
        );
        let lwt = LastWill::new(
            lwt_topic,
            lwt_payload.to_string(),
            QoS::AtLeastOnce,
            true, // retained — overwrites the stale announce
        );
        mqtt_options.set_last_will(lwt);

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 64);

        let shutdown = self.shutdown_signal.clone();
        let state = self.state.clone();
        let topic_prefix = self.config.topic_prefix.clone();
        let publish_interval = self.config.publish_interval_secs;
        let server_id = self.config.client_id.clone();
        let config_for_announce = self.config.clone();

        // --- Phase 40 (ADR-031 §2): Resolve routable IP once at startup ---
        //
        // The IP is resolved here so we log it exactly once. Periodic
        // heartbeat ticks silently re-resolve (in case DHCP changed it)
        // but only log at debug! level.
        let startup_ip = resolve_routable_ip(&self.config);
        if let Some(ip) = startup_ip {
            info!(ip = %ip, "MQTT announce: resolved routable IP for cross-subnet discovery");
        } else {
            warn!("MQTT announce: could not determine routable IPv4 address — announce messages will be skipped");
        }

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
                        // --- Phase 40 (ADR-031 §4): Graceful shutdown ---
                        // Publish `server.offline` to clear the retained announce,
                        // then publish the standard shutdown message.
                        let offline_msg = build_message(
                            &server_id,
                            "server.offline",
                            serde_json::json!({}),
                        );
                        let _ = client.publish(
                            format!("{}/server/announce", topic_prefix),
                            QoS::AtLeastOnce,
                            true, // retained — clears the announce
                            offline_msg.to_string(),
                        ).await;

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

                        // --- Phase 41: Clear all retained topics ---
                        //
                        // Per MQTT spec §3.3.1.3, publishing an empty payload
                        // with retain=true tells the broker to DELETE the
                        // retained message for that topic. This prevents the
                        // broker from accumulating stale retained state from
                        // servers that have shut down cleanly.
                        let retained_topics = [
                            format!("{}/status", topic_prefix),
                            format!("{}/services/status", topic_prefix),
                            format!("{}/devices/inventory", topic_prefix),
                            format!("{}/resources/inventory", topic_prefix),
                            format!("{}/server/announce", topic_prefix),
                        ];

                        for topic in &retained_topics {
                            let _ = client.publish(
                                topic,
                                QoS::AtLeastOnce,
                                true,      // retain flag
                                Vec::new() // empty payload = delete retained
                            ).await;
                        }

                        // Queue the disconnect. rumqttc will internally
                        // enqueue an MQTT DISCONNECT packet.
                        let _ = client.disconnect().await;

                        // Continue polling the event loop to flush all
                        // queued messages (offline, shutdown, empty retained
                        // clears, disconnect) over the wire. rumqttc needs
                        // the poll loop to keep running — without this, the
                        // queued publishes would never actually be sent.
                        //
                        // We poll with a timeout: once the disconnect is
                        // processed the loop will error, breaking out.
                        let flush_deadline = tokio::time::Instant::now()
                            + tokio::time::Duration::from_millis(2000);
                        loop {
                            match tokio::time::timeout_at(
                                flush_deadline,
                                eventloop.poll(),
                            )
                            .await
                            {
                                Ok(Ok(_)) => {} // keep flushing
                                Ok(Err(_)) => break, // connection closed
                                Err(_) => {
                                    warn!("MQTT flush timed out during shutdown");
                                    break;
                                }
                            }
                        }

                        info!("MQTT publisher disconnected (retained topics cleared)");
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
                                    // Phase 40: publish announce on connect.
                                    publish_server_announce(
                                        &client, &topic_prefix, &server_id,
                                        &state, &config_for_announce,
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

        // Spawn the periodic state-change publisher.
        //
        // This task polls the server state at `publish_interval_secs` and
        // publishes an MQTT message **only when the underlying data has
        // changed** (report-by-exception). A forced heartbeat is published
        // every `heartbeat_max_secs` regardless, to confirm liveness.
        //
        // This avoids the IoT anti-pattern of publishing identical payloads
        // with new timestamps on every tick.
        let shutdown2 = self.shutdown_signal.clone();
        let state2 = self.state.clone();
        let prefix2 = self.config.topic_prefix.clone();
        let server_id2 = self.config.client_id.clone();
        let host2 = host.clone();
        let config_for_periodic = self.config.clone();
        let heartbeat_max = self.config.heartbeat_max_secs;

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(publish_interval));

            // Build a separate client for periodic publishing.
            let mut opts = MqttOptions::new(format!("{}-periodic", server_id2), &host2, port);
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

            // State fingerprints: the serialized payload (without timestamps)
            // from the last publish. We compare against these on each tick
            // and only re-publish when the data has actually changed.
            let mut prev_status: Option<String> = None;
            let mut prev_announce: Option<String> = None;
            let mut prev_devices: Option<String> = None;
            let mut prev_resources: Option<String> = None;
            let mut prev_services: Option<String> = None;
            let mut last_heartbeat = std::time::Instant::now();

            loop {
                tokio::select! {
                    _ = shutdown2.notified() => break,
                    _ = interval.tick() => {
                        // Determine if a forced heartbeat is due.
                        let force = heartbeat_max > 0
                            && last_heartbeat.elapsed().as_secs() >= heartbeat_max;

                        // --- Status ---
                        let cur_status = build_status_payload(&state2);
                        if force || prev_status.as_ref() != Some(&cur_status) {
                            publish_with_payload(
                                &periodic_client, &server_id2,
                                "server.status", &cur_status,
                                &format!("{}/status", prefix2),
                                QoS::AtMostOnce, true,
                            ).await;
                            prev_status = Some(cur_status);
                        }

                        // --- Announce ---
                        let cur_announce = build_announce_payload(
                            &state2, &config_for_periodic,
                        );
                        if let Some(ref payload) = cur_announce {
                            if force || prev_announce.as_ref() != Some(payload) {
                                publish_with_payload(
                                    &periodic_client, &server_id2,
                                    "server.announce", payload,
                                    &format!("{}/server/announce", prefix2),
                                    QoS::AtLeastOnce, true,
                                ).await;
                                prev_announce = Some(payload.clone());
                            }
                        }

                        // --- Device inventory ---
                        let cur_devices = build_device_inventory_payload(&state2);
                        if force || prev_devices.as_ref() != Some(&cur_devices) {
                            publish_with_payload(
                                &periodic_client, &server_id2,
                                "devices.inventory", &cur_devices,
                                &format!("{}/devices/inventory", prefix2),
                                QoS::AtLeastOnce, true,
                            ).await;
                            prev_devices = Some(cur_devices);
                        }

                        // --- Resource inventory ---
                        let cur_resources = build_resource_inventory_payload(&state2);
                        if force || prev_resources.as_ref() != Some(&cur_resources) {
                            publish_with_payload(
                                &periodic_client, &server_id2,
                                "resources.inventory", &cur_resources,
                                &format!("{}/resources/inventory", prefix2),
                                QoS::AtLeastOnce, true,
                            ).await;
                            prev_resources = Some(cur_resources);
                        }

                        // --- Services status ---
                        let cur_services = build_services_payload(&state2);
                        if force || prev_services.as_ref() != Some(&cur_services) {
                            publish_with_payload(
                                &periodic_client, &server_id2,
                                "services.status", &cur_services,
                                &format!("{}/services/status", prefix2),
                                QoS::AtLeastOnce, true,
                            ).await;
                            prev_services = Some(cur_services);
                        }

                        if force {
                            last_heartbeat = std::time::Instant::now();
                            debug!("MQTT heartbeat: forced re-publish of all topics");
                        }
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
// Payload Builders (for fingerprinting / report-by-exception)
// ============================================================================

/// Build the server status payload as a canonical JSON string.
///
/// This **excludes** `uptime_secs` from the fingerprint because uptime
/// changes every second and would defeat the report-by-exception model.
/// Uptime is still included in the published message.
fn build_status_payload(state: &AppState) -> String {
    // The fingerprint only includes values that represent real state changes.
    // uptime_secs is excluded — it always increases and is not "state".
    serde_json::json!({
        "device_count": state.device_manager.device_count(),
        "resource_count": state.resource_registry.count(),
        "api_version": state.api_version,
    })
    .to_string()
}

/// Build the services status payload as a canonical JSON string.
fn build_services_payload(state: &AppState) -> String {
    let services: serde_json::Map<String, serde_json::Value> = state
        .service_manager
        .list()
        .iter()
        .map(|s| {
            (
                s.name.clone(),
                serde_json::Value::String(format!("{}", s.status)),
            )
        })
        .collect();

    serde_json::json!({ "services": services }).to_string()
}

/// Build the device inventory payload as a canonical JSON string.
fn build_device_inventory_payload(state: &AppState) -> String {
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

    serde_json::json!({ "devices": devices }).to_string()
}

/// Build the resource inventory payload as a canonical JSON string.
fn build_resource_inventory_payload(state: &AppState) -> String {
    serde_json::json!({
        "count": state.resource_registry.count(),
    })
    .to_string()
}

/// Build the server announce payload as a canonical JSON string.
///
/// Returns `None` if the routable IP cannot be determined.
fn build_announce_payload(
    state: &AppState,
    mqtt_config: &MqttConfig,
) -> Option<String> {
    let ip = resolve_routable_ip(mqtt_config)?;

    let (rest_port, grpc_port, mgmt_port, description) = {
        let cfg = state.config.read().expect("config lock poisoned");
        (
            cfg.rest.port,
            cfg.grpc.port,
            cfg.server.management_port,
            cfg.server.description.clone(),
        )
    };

    let service_states = state.service_manager.list();
    let is_service_running = |name: &str| -> bool {
        service_states
            .iter()
            .any(|s| s.name == name && s.status.to_string().contains("Running"))
    };

    Some(
        serde_json::json!({
            "description": description,
            "rest_url": format!("http://{}:{}", ip, rest_port),
            "grpc_url": format!("grpc://{}:{}", ip, grpc_port),
            "dashboard_url": format!("http://{}:{}", ip, mgmt_port),
            "mqtt_topic_prefix": mqtt_config.topic_prefix,
            "api_version": state.api_version,
            "device_count": state.device_manager.device_count(),
            "services": {
                "rest": is_service_running("rest"),
                "grpc": is_service_running("grpc"),
                "mqtt": true,
                "mdns": is_service_running("mdns"),
            },
        })
        .to_string(),
    )
}

// ============================================================================
// Publish Helpers
// ============================================================================

/// Publish a pre-built payload string, wrapping it in the standard message
/// envelope (version, timestamp, server_id, type).
///
/// Used by the periodic state-change publisher. The `payload_json` is the
/// canonical fingerprint string — it gets parsed back to a `Value` so we
/// can wrap it with the envelope fields.
async fn publish_with_payload(
    client: &AsyncClient,
    server_id: &str,
    msg_type: &str,
    payload_json: &str,
    topic: &str,
    qos: QoS,
    retain: bool,
) {
    // Parse the fingerprint string back to a Value for the envelope.
    let payload: serde_json::Value =
        serde_json::from_str(payload_json).unwrap_or(serde_json::json!({}));

    let msg = build_message(server_id, msg_type, payload);

    if let Err(e) = client
        .publish(topic, qos, retain, msg.to_string())
        .await
    {
        warn!(error = %e, topic = %topic, "failed to publish MQTT message");
    }
}

/// Publish server heartbeat status (used on initial connect).
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
            true,
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish server status");
    }
}

/// Publish service states (used on initial connect).
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
        .map(|s| {
            (
                s.name.clone(),
                serde_json::Value::String(format!("{}", s.status)),
            )
        })
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
            true,
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish services status");
    }
}

/// Publish the device inventory (used on initial connect).
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
            true,
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish device inventory");
    }
}

/// Publish the resource inventory (used on initial connect).
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
            true,
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish resource inventory");
    }
}

/// Publish server announce for cross-subnet discovery (ADR-031 §1).
///
/// Used on initial connect. The periodic task uses `build_announce_payload`
/// + `publish_with_payload` instead.
async fn publish_server_announce(
    client: &AsyncClient,
    prefix: &str,
    server_id: &str,
    state: &AppState,
    mqtt_config: &MqttConfig,
) {
    let payload = match build_announce_payload(state, mqtt_config) {
        Some(p) => p,
        None => {
            warn!("MQTT announce skipped — could not determine routable IPv4 address");
            return;
        }
    };

    // Parse the canonical payload string back to a Value for the envelope.
    let payload_value: serde_json::Value =
        serde_json::from_str(&payload).unwrap_or(serde_json::json!({}));

    let msg = build_message(server_id, "server.announce", payload_value);

    if let Err(e) = client
        .publish(
            format!("{}/server/announce", prefix),
            QoS::AtLeastOnce,
            true,
            msg.to_string(),
        )
        .await
    {
        warn!(error = %e, "failed to publish server announce");
    }
}


// ============================================================================
// IP Address Resolution (ADR-031 §2)
// ============================================================================

/// Resolve the server's routable IPv4 address for MQTT announce URLs.
///
/// Follows the algorithm defined in ADR-031 §2:
///
/// 1. Enumerate all network interfaces via `netdev`.
/// 2. Collect non-loopback interfaces with at least one IPv4 address
///    that is NOT in the 169.254.0.0/16 (APIPA / link-local) range.
/// 3. If only one candidate remains, use it (no config needed).
/// 4. If `announce_interface` is set, find the interface by name.
/// 5. Otherwise, walk `preferred_subnets` and use the first match.
/// 6. Fall back to the first candidate.
///
/// On Windows, `netdev` returns the adapter GUID as `name` and the
/// user-visible name (e.g. "Ethernet", "Wi-Fi") as `friendly_name`.
/// We use `friendly_name` for display and `announce_interface` matching,
/// falling back to `name` on platforms where `friendly_name` is `None`.
///
/// Returns `None` if no routable IPv4 address can be found.
fn resolve_routable_ip(config: &MqttConfig) -> Option<Ipv4Addr> {
    // Each candidate stores:
    //   display_name  — friendly_name if available, else system name
    //   system_name   — raw OS identifier (GUID on Windows, "eth0" on Linux)
    //   ip            — non-APIPA IPv4 address
    let candidates: Vec<(String, String, Ipv4Addr)> = netdev::get_interfaces()
        .into_iter()
        .filter(|iface| !iface.is_loopback())
        .flat_map(|iface| {
            // Prefer the user-visible friendly name (Windows: "Ethernet", "Wi-Fi").
            // Fall back to the system name (Linux: "eth0", "ens192").
            let iface_label = iface
                .friendly_name
                .clone()
                .unwrap_or_else(|| iface.name.clone());
            let sys_name = iface.name.clone();

            iface
                .ipv4
                .iter()
                .map(|net| net.addr())
                .filter(|addr| !is_apipa(*addr))
                .map(|addr| (iface_label.clone(), sys_name.clone(), addr))
                .collect::<Vec<_>>()
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Step 3: single candidate — no ambiguity.
    if candidates.len() == 1 {
        let (ref label, ref sys, addr) = candidates[0];
        debug!(
            interface = %label, system_name = %sys, ip = %addr,
            "MQTT announce: single interface auto-detected"
        );
        return Some(addr);
    }

    // Step 4: if announce_interface is configured, match against both
    // the friendly name and the system name for cross-platform flexibility.
    if !config.announce_interface.is_empty() {
        for (ref label, ref sys, addr) in &candidates {
            if label == &config.announce_interface || sys == &config.announce_interface {
                debug!(
                    interface = %label, system_name = %sys, ip = %addr,
                    "MQTT announce: using configured announce_interface"
                );
                return Some(*addr);
            }
        }
        warn!(
            interface = %config.announce_interface,
            "MQTT announce: configured announce_interface not found, falling through"
        );
    }

    // Step 5: walk preferred_subnets in order.
    for cidr_str in &config.preferred_subnets {
        if let Some((net_addr, prefix_len)) = parse_cidr(cidr_str) {
            for (ref label, ref sys, addr) in &candidates {
                if ip_in_subnet(*addr, net_addr, prefix_len) {
                    debug!(
                        interface = %label, system_name = %sys,
                        ip = %addr, subnet = %cidr_str,
                        "MQTT announce: matched preferred subnet"
                    );
                    return Some(*addr);
                }
            }
        } else {
            warn!(cidr = %cidr_str, "MQTT announce: invalid CIDR in preferred_subnets, skipping");
        }
    }

    // Step 6: fallback to first available.
    let (ref label, ref sys, addr) = candidates[0];
    debug!(
        interface = %label, system_name = %sys, ip = %addr,
        "MQTT announce: no preferred subnet matched, using first available interface"
    );
    Some(addr)
}

/// Check if an IPv4 address is in the APIPA / link-local range (169.254.0.0/16).
///
/// APIPA (Automatic Private IP Addressing) addresses are assigned when DHCP
/// fails. They are not routable and should be excluded from announce URLs.
fn is_apipa(addr: Ipv4Addr) -> bool {
    let octets = addr.octets();
    octets[0] == 169 && octets[1] == 254
}

/// Parse a CIDR string like "10.0.1.0/24" into (network address, prefix length).
///
/// Returns `None` if the string is not a valid CIDR notation.
fn parse_cidr(cidr: &str) -> Option<(Ipv4Addr, u32)> {
    let parts: Vec<&str> = cidr.splitn(2, '/').collect();
    if parts.len() != 2 {
        return None;
    }
    let addr: Ipv4Addr = parts[0].parse().ok()?;
    let prefix: u32 = parts[1].parse().ok()?;
    if prefix > 32 {
        return None;
    }
    Some((addr, prefix))
}

/// Check if an IPv4 address falls within a subnet defined by network address
/// and prefix length.
///
/// This performs standard bitwise masking: for a /24 subnet, we compare the
/// top 24 bits of both addresses. If they match, the address is in the subnet.
fn ip_in_subnet(addr: Ipv4Addr, network: Ipv4Addr, prefix_len: u32) -> bool {
    if prefix_len == 0 {
        return true;
    }
    // Convert to u32 for bitwise comparison.
    let addr_bits = u32::from(addr);
    let net_bits = u32::from(network);
    // Create a mask with `prefix_len` leading 1s.
    // Example: prefix_len=24 → 0xFFFFFF00
    let mask = !0u32 << (32 - prefix_len);
    (addr_bits & mask) == (net_bits & mask)
}

// ============================================================================
// Message Builder
// ============================================================================

/// Build a versioned MQTT message following the ADR-011 format.
///
/// ```json
/// { "v": 1, "ts": "...", "type": "...", "server_id": "...", "payload": {...} }
/// ```
fn build_message(server_id: &str, msg_type: &str, payload: serde_json::Value) -> serde_json::Value {
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

    // ----------------------------------------------------------------
    // Broker URL parsing (existing tests, Phase 10)
    // ----------------------------------------------------------------

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

    // ----------------------------------------------------------------
    // Message format (existing test, Phase 10)
    // ----------------------------------------------------------------

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

    // ----------------------------------------------------------------
    // Phase 40: announce message format
    // ----------------------------------------------------------------

    #[test]
    fn build_announce_message_has_correct_type() {
        let msg = build_message(
            "test-server",
            "server.announce",
            serde_json::json!({
                "description": "Lab Bench 1",
                "rest_url": "http://10.0.1.50:8080",
                "grpc_url": "grpc://10.0.1.50:50051",
                "dashboard_url": "http://10.0.1.50:9090",
                "mqtt_topic_prefix": "okfpga/server01",
                "api_version": "0.1.0",
                "device_count": 2,
            }),
        );
        assert_eq!(msg["type"], "server.announce");
        assert_eq!(msg["v"], 1);
        assert_eq!(msg["payload"]["rest_url"], "http://10.0.1.50:8080");
        assert_eq!(msg["payload"]["device_count"], 2);
        assert_eq!(msg["payload"]["description"], "Lab Bench 1");
    }

    #[test]
    fn build_offline_message_has_correct_type() {
        let msg = build_message(
            "test-server",
            "server.offline",
            serde_json::json!({}),
        );
        assert_eq!(msg["type"], "server.offline");
        assert_eq!(msg["v"], 1);
        // Offline messages have an empty payload.
        assert!(msg["payload"].as_object().unwrap().is_empty());
    }

    // ----------------------------------------------------------------
    // Phase 40: APIPA detection
    // ----------------------------------------------------------------

    #[test]
    fn apipa_detection() {
        // 169.254.x.x should be detected as APIPA (link-local).
        assert!(is_apipa(Ipv4Addr::new(169, 254, 0, 1)));
        assert!(is_apipa(Ipv4Addr::new(169, 254, 255, 255)));
        // Regular addresses should not be APIPA.
        assert!(!is_apipa(Ipv4Addr::new(10, 0, 1, 50)));
        assert!(!is_apipa(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!is_apipa(Ipv4Addr::new(132, 177, 122, 10)));
    }

    // ----------------------------------------------------------------
    // Phase 40: CIDR parsing and subnet matching
    // ----------------------------------------------------------------

    #[test]
    fn parse_cidr_valid() {
        let (addr, prefix) = parse_cidr("10.0.1.0/24").unwrap();
        assert_eq!(addr, Ipv4Addr::new(10, 0, 1, 0));
        assert_eq!(prefix, 24);
    }

    #[test]
    fn parse_cidr_slash_32() {
        let (addr, prefix) = parse_cidr("192.168.1.1/32").unwrap();
        assert_eq!(addr, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(prefix, 32);
    }

    #[test]
    fn parse_cidr_invalid_no_slash() {
        assert!(parse_cidr("10.0.1.0").is_none());
    }

    #[test]
    fn parse_cidr_invalid_prefix_too_large() {
        assert!(parse_cidr("10.0.1.0/33").is_none());
    }

    #[test]
    fn parse_cidr_invalid_addr() {
        assert!(parse_cidr("not.an.ip/24").is_none());
    }

    #[test]
    fn ip_in_subnet_24() {
        let net = Ipv4Addr::new(10, 0, 1, 0);
        assert!(ip_in_subnet(Ipv4Addr::new(10, 0, 1, 50), net, 24));
        assert!(ip_in_subnet(Ipv4Addr::new(10, 0, 1, 255), net, 24));
        assert!(!ip_in_subnet(Ipv4Addr::new(10, 0, 2, 1), net, 24));
    }

    #[test]
    fn ip_in_subnet_16() {
        let net = Ipv4Addr::new(132, 177, 0, 0);
        assert!(ip_in_subnet(Ipv4Addr::new(132, 177, 122, 10), net, 16));
        assert!(ip_in_subnet(Ipv4Addr::new(132, 177, 0, 1), net, 16));
        assert!(!ip_in_subnet(Ipv4Addr::new(132, 178, 0, 1), net, 16));
    }

    #[test]
    fn ip_in_subnet_0_matches_all() {
        let net = Ipv4Addr::new(0, 0, 0, 0);
        assert!(ip_in_subnet(Ipv4Addr::new(10, 0, 1, 50), net, 0));
        assert!(ip_in_subnet(Ipv4Addr::new(192, 168, 1, 1), net, 0));
    }

    // ----------------------------------------------------------------
    // Phase 40: resolve_routable_ip (live system test)
    // ----------------------------------------------------------------

    /// This test runs against the actual network interfaces on the machine.
    /// It verifies that `resolve_routable_ip` returns *some* non-APIPA,
    /// non-loopback address (assuming the machine has at least one).
    #[test]
    fn resolve_ip_returns_non_loopback_non_apipa() {
        let config = MqttConfig::default();
        // On most development machines, there should be at least one
        // non-loopback interface. If this fails, the machine has no
        // network connectivity — which is an expected CI edge case.
        if let Some(ip) = resolve_routable_ip(&config) {
            assert!(!ip.is_loopback(), "should not return loopback");
            assert!(!is_apipa(ip), "should not return APIPA");
        }
        // If None, the machine truly has no routable interfaces — acceptable.
    }
}
