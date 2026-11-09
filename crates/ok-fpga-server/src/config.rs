// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Server Configuration
//!
//! This module loads and validates the server configuration from a TOML file.
//! Configuration is loaded in this priority order:
//!
//! 1. Built-in defaults (always present)
//! 2. TOML file (path from `--config` CLI arg or `OKFPGA_CONFIG` env var,
//!    default: `config.toml` in the working directory)
//! 3. Environment variable overrides (`OKFPGA_*` prefix)
//!
//! ## Feature Flag Validation
//!
//! The config loader checks each service's `enabled` flag against the
//! compiled feature set. If a service is `enabled = true` but its Cargo
//! feature is not compiled, a warning is logged (the server does NOT panic).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ============================================================================
// Configuration Structs
// ============================================================================

/// Top-level server configuration.
///
/// Each section corresponds to a server component or optional service.
/// Sections for uncompiled features are still parsed (for validation warnings).
///
/// ## Example
///
/// ```toml
/// [server]
/// management_port = 9090
/// log_level = "info"
///
/// [rest]
/// enabled = true
/// bind = "0.0.0.0"
/// port = 8080
///
/// [grpc]
/// enabled = false
/// bind = "0.0.0.0"
/// port = 50051
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Core server settings (management port, logging).
    pub server: CoreConfig,

    /// REST API server configuration.
    pub rest: RestConfig,

    /// gRPC API server configuration.
    pub grpc: GrpcConfig,

    /// MQTT status publisher configuration.
    pub mqtt: MqttConfig,

    /// mDNS/DNS-SD service discovery configuration.
    pub mdns: MdnsConfig,

    /// Web dashboard configuration.
    pub dashboard: DashboardConfig,

    /// Authentication configuration (ADR-008).
    pub auth: AuthConfig,

    /// Access control configuration (ADR-024).
    pub access_control: AccessControlConfig,

    /// Catalog of known boards, devices, and bitfiles (Phase 28D).
    /// Dashboard-only beautification — adds "About" links to detail panels.
    pub catalog: CatalogConfig,
}

/// Core server settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreConfig {
    /// Port for the management server (always-on, serves dashboard + mgmt API).
    pub management_port: u16,

    /// Log level filter. Supports `tracing` filter syntax:
    /// `trace`, `debug`, `info`, `warn`, `error`, or per-module like
    /// `ok_fpga_server=debug,ok_fpga_core=info`.
    pub log_level: String,

    /// Bind address for the management server.
    pub management_bind: String,

    /// Seconds of inactivity before a user session is considered expired.
    /// The server sweep task will auto-release resources held by expired users.
    /// Set to 0 to disable the sweep task entirely.
    pub user_timeout_secs: u64,

    /// How often (in seconds) the sweep task checks for expired users.
    /// Only relevant when `user_timeout_secs > 0`.
    pub user_sweep_interval_secs: u64,

    /// Path to the log file. Logs are appended (not truncated) on each server
    /// start. Set to empty string to disable file logging.
    ///
    /// Default: `"logs/okfpga-server.log"`.
    pub log_file: String,

    /// Human-readable description of this server instance.
    ///
    /// Shown in mDNS discovery results and the `GET /server/status` response.
    /// Defaults to the mDNS `instance_name` if empty. Can be changed at
    /// runtime via the dashboard Settings panel (persisted to TOML).
    ///
    /// Example: `"Lab Bench 1 — XEM8320"`.
    pub description: String,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            management_port: 9090,
            log_level: "info".to_string(),
            management_bind: "0.0.0.0".to_string(),
            user_timeout_secs: 60,
            user_sweep_interval_secs: 10,
            log_file: "logs/okfpga-server.log".to_string(),
            description: String::new(),
        }
    }
}

/// REST API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RestConfig {
    /// Whether the REST server should start on launch.
    pub enabled: bool,

    /// Bind address for the REST server.
    pub bind: String,

    /// Port for the REST API.
    pub port: u16,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

/// gRPC API server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcConfig {
    /// Whether the gRPC server should start on launch.
    pub enabled: bool,

    /// Bind address for the gRPC server.
    pub bind: String,

    /// Port for the gRPC API.
    pub port: u16,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "0.0.0.0".to_string(),
            port: 50051,
        }
    }
}

/// MQTT status publisher configuration (Phase 10, extended Phase 40).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MqttConfig {
    /// Whether the MQTT publisher should start on launch.
    pub enabled: bool,

    /// MQTT broker URL (e.g., `mqtt://localhost:1883`).
    pub broker_url: String,

    /// MQTT client ID. Automatically derived as `okfpga-server-<hostname>`.
    ///
    /// There should only ever be one ok-FPGA-Server per host, so the
    /// hostname provides natural uniqueness. This field is not
    /// user-configurable — it is computed at startup.
    #[serde(skip)]
    pub client_id: String,

    /// Topic prefix for all published messages. Automatically derived
    /// as `okfpga/server-<hostname>`.
    ///
    /// Like `client_id`, this is computed from the hostname at startup
    /// and is not set in config.toml.
    #[serde(skip)]
    pub topic_prefix: String,

    /// How often to check for state changes (seconds).
    ///
    /// The server polls its own state at this interval and publishes an
    /// MQTT message **only if the underlying data has changed**. This avoids
    /// the IoT anti-pattern of publishing identical payloads with new
    /// timestamps. Set lower for faster change detection (e.g. 5s), higher
    /// to reduce CPU usage on the poll loop.
    pub publish_interval_secs: u64,

    /// Maximum seconds between forced heartbeat publishes, even if no state
    /// has changed.
    ///
    /// Confirms the server is still alive and reachable. Subscribers can
    /// treat the absence of a heartbeat within this window as a potential
    /// problem. Set to 0 to disable the heartbeat and rely solely on
    /// LWT for crash detection.
    pub heartbeat_max_secs: u64,

    /// Name of the network interface to use for announce URLs (ADR-031 §2).
    ///
    /// Only needed when the machine has multiple non-loopback interfaces.
    /// When empty and only one non-loopback, non-APIPA interface exists,
    /// that interface is used automatically. When empty and multiple
    /// interfaces exist, `preferred_subnets` is consulted.
    ///
    /// Example: `"Ethernet"`, `"eth0"`, `"ens192"`.
    pub announce_interface: String,

    /// Ordered list of preferred subnets (CIDR notation) for IP auto-detection
    /// when `announce_interface` is not set and multiple interfaces exist
    /// (ADR-031 §2).
    ///
    /// The first interface whose IPv4 address matches a listed subnet is used.
    /// When empty and no `announce_interface` is set, the first available
    /// non-loopback, non-APIPA interface is used.
    ///
    /// Example: `["10.0.1.0/24", "10.40.40.0/24"]`
    pub preferred_subnets: Vec<String>,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            broker_url: "mqtt://localhost:1883".to_string(),
            // Computed at startup from hostname — see MqttConfig::resolve_hostname_fields()
            client_id: String::new(),
            topic_prefix: String::new(),
            publish_interval_secs: 5,
            heartbeat_max_secs: 300,
            announce_interface: String::new(),
            preferred_subnets: Vec::new(),
        }
    }
}

// ============================================================================
// Phase 43 — Hostname sanitization
// ============================================================================

/// Normalize a raw system hostname into a DNS/MQTT-safe identifier.
///
/// Transformations:
/// - Lowercase the entire string
/// - Replace spaces, underscores, and dots with hyphens
/// - Strip any characters that are not ASCII alphanumeric or hyphen
/// - Collapse consecutive hyphens into one
/// - Trim leading/trailing hyphens
/// - Fall back to `"unknown"` if the result is empty
pub(crate) fn sanitize_hostname(raw: &str) -> String {
    let s: String = raw
        .to_lowercase()
        .chars()
        .map(|c| match c {
            ' ' | '_' | '.' => '-',
            c if c.is_ascii_alphanumeric() || c == '-' => c,
            _ => '-',
        })
        .collect();

    // Collapse consecutive hyphens and trim leading/trailing.
    let s = s
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if s.is_empty() {
        "unknown".to_string()
    } else {
        s
    }
}

impl MqttConfig {
    /// Populate `client_id` and `topic_prefix` from the system hostname.
    ///
    /// Called once at startup, after config is loaded from TOML. The TOML
    /// file does not contain these fields — they are always derived as:
    /// - `client_id`    = `"okfpga-server-<hostname>"`
    /// - `topic_prefix` = `"okfpga/server-<hostname>"`
    ///
    /// This ensures uniqueness per host without manual configuration.
    pub fn resolve_hostname_fields(&mut self) {
        let host = hostname::get()
            .map(|h| sanitize_hostname(&h.to_string_lossy()))
            .unwrap_or_else(|_| "unknown".to_string());

        self.client_id = format!("okfpga-server-{}", host);
        self.topic_prefix = format!("okfpga/server-{}", host);
    }
}

/// mDNS/DNS-SD service discovery configuration (Phase 11).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MdnsConfig {
    /// Whether mDNS service advertising should be enabled.
    pub enabled: bool,

    /// Human-readable instance name (e.g., `"okfpga-lab-bench-1"`).
    pub instance_name: String,
}

impl Default for MdnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            instance_name: "okfpga-server".to_string(),
        }
    }
}

impl MdnsConfig {
    /// Populate `instance_name` from the system hostname when still at the
    /// default value.
    ///
    /// Called once at startup, after config is loaded from TOML.  If the user
    /// has not overridden `instance_name` in the TOML, it is set to
    /// `"okfpga-server-<hostname>"` — matching the MQTT `client_id` pattern
    /// so that mDNS and MQTT discovery produce the same server identity.
    ///
    /// If the user has set a custom `instance_name`, it is left unchanged.
    pub fn populate_from_hostname(&mut self) {
        if self.instance_name == "okfpga-server" {
            let host = hostname::get()
                .map(|h| sanitize_hostname(&h.to_string_lossy()))
                .unwrap_or_else(|_| "unknown".to_string());
            self.instance_name = format!("okfpga-server-{}", host);
        }
    }
}

/// Web dashboard configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DashboardConfig {
    /// Whether the web dashboard should be served.
    pub enabled: bool,

    /// Default log level displayed in the dashboard log viewer.
    /// Users can change this at runtime via the dashboard UI.
    ///
    /// Valid values: `"trace"`, `"debug"`, `"info"`, `"warn"`, `"error"`.
    /// Default: `"info"`.
    pub default_log_level: String,

    /// Whether the Access Control section is visible on the dashboard Settings page.
    ///
    /// When `false`, the endpoint enforcement dropdown is hidden from the
    /// dashboard and enforcement changes via `PUT /mgmt/settings` are silently
    /// ignored. The server description is always editable regardless of this flag.
    /// Default: `true`.
    pub expose_settings: bool,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_log_level: "info".to_string(),
            expose_settings: true,
        }
    }
}

/// Authentication configuration (ADR-008).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Authentication provider: `"none"`, `"local"`, or `"ldap"`.
    pub provider: String,

    /// Session expiry in seconds (default: 8 hours = 28800).
    pub session_expiry_secs: u64,

    /// Local auth provider settings.
    pub local: LocalAuthConfig,

    /// LDAP auth provider settings.
    pub ldap: ok_fpga_core::auth::LdapConfig,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            session_expiry_secs: 28800, // 8 hours
            local: LocalAuthConfig::default(),
            ldap: ok_fpga_core::auth::LdapConfig::default(),
        }
    }
}

/// Local auth provider configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalAuthConfig {
    /// List of local user accounts.
    pub users: Vec<ok_fpga_core::auth::LocalUser>,
}

// ============================================================================
// Access Control Configuration (ADR-024)
// ============================================================================

/// Access control configuration (ADR-024).
///
/// Controls whether FPGA data port operations require the calling user to
/// hold a resource covering the target endpoint.
///
/// ## Example
///
/// ```toml
/// [access_control]
/// endpoint_enforcement = "off"  # "off", "warn", or "enforce"
/// warn_range_self_overlap = true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessControlConfig {
    /// Enforcement level for endpoint access checks.
    ///
    /// - `"off"`: No checks. Debugging/troubleshooting aid only.
    /// - `"warn"`: Log violations at WARN level but allow operations.
    ///   User ID is required on all requests.
    /// - `"enforce"`: Reject unauthorized operations with 403 / PERMISSION_DENIED.
    ///   User ID is required on all requests.
    ///
    /// Default: `"enforce"` (S-12, 44-E).
    pub endpoint_enforcement: String,

    /// Log a warning when one resource declaration lists address ranges
    /// that overlap each other (44-B, owner decision Q1).
    ///
    /// Such a declaration is *allowed*: the overlap is inside one resource,
    /// so it cannot clash with anyone else. It is still worth a warning,
    /// because it is usually a typo, and because overlapping ranges with
    /// different `private` flags need a tie-break (the private one wins).
    ///
    /// Default: `true`. Env override: `OKFPGA_WARN_RANGE_SELF_OVERLAP`.
    pub warn_range_self_overlap: bool,

    /// Log a warning at registration when `ReadOnly` holders could reach a
    /// non-private `PipeOut`, `BTPipeOut`, `TriggerOut`, or `AddressRange`
    /// where a read may cause hardware side effects (D75 F4, 44-E).
    ///
    /// Default: `true`. Env override: `OKFPGA_WARN_READONLY_SIDE_EFFECTS`.
    pub warn_readonly_side_effects: bool,

    /// Log and return a warning at registration when a Classic resource omits
    /// `endpoints` (claiming `0x00–0x7F` Wire/Trigger endpoints only) or when
    /// an omitted-`endpoints` resource coexists on the same device with a
    /// Pipe/BTPipe-declaring resource (D77, 44-E Q2, Phase 45 Q4).
    ///
    /// Default: `true`. Env override: `OKFPGA_WARN_WILDCARD_ENDPOINTS`.
    pub warn_wildcard_endpoints: bool,
}

impl Default for AccessControlConfig {
    fn default() -> Self {
        Self {
            endpoint_enforcement: "enforce".to_string(),
            warn_range_self_overlap: true,
            warn_readonly_side_effects: true,
            warn_wildcard_endpoints: true,
        }
    }
}

// ============================================================================
// Catalog Configuration (Phase 28D)
// ============================================================================

/// Catalog of known SYZYGY boards, FPGA devices, and bitfiles.
///
/// This is purely for dashboard beautification — it adds "About" links
/// and human-readable descriptions to device/SYZYGY/bitfile detail rows.
/// Catalog data is served ONLY through `GET /mgmt/catalog`.
///
/// ## Example
///
/// ```toml
/// [[catalog.syzygy_boards]]
/// product_name = "SZG-DAC-AD9116"
/// about_url = "https://docs.opalkelly.com/syzygy/szg-dac-ad9116/"
///
/// [[catalog.devices]]
/// product_name = "XEM8320"
/// about_url = "https://docs.opalkelly.com/xem8320/"
///
/// [[catalog.bitfiles]]
/// filename = "counters.bit"
/// description = "Simple counter design for testing"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CatalogConfig {
    /// Known SYZYGY peripheral boards.
    #[serde(default)]
    pub syzygy_boards: Vec<SyzygyBoardEntry>,

    /// Known FPGA devices (for "About" links in the device detail panel).
    #[serde(default)]
    pub devices: Vec<DeviceCatalogEntry>,

    /// Known bitfiles (for descriptions and "About" links).
    #[serde(default)]
    pub bitfiles: Vec<BitfileCatalogEntry>,
}

/// A known SYZYGY peripheral board in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyzygyBoardEntry {
    /// Product name to match against (e.g., "SZG-DAC-AD9116").
    pub product_name: String,

    /// Optional model qualifier for finer matching (e.g., "A").
    #[serde(default)]
    pub product_model: Option<String>,

    /// Optional version qualifier.
    #[serde(default)]
    pub product_version: Option<String>,

    /// URL for more information about this board.
    #[serde(default)]
    pub about_url: Option<String>,
}

/// A known FPGA device in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCatalogEntry {
    /// Product name to match against (e.g., "XEM8320").
    pub product_name: String,

    /// URL for more information about this device.
    #[serde(default)]
    pub about_url: Option<String>,
}

/// A known bitfile in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitfileCatalogEntry {
    /// Filename to match against (e.g., "counters.bit").
    pub filename: String,

    /// Human-readable description of the bitfile.
    #[serde(default)]
    pub description: Option<String>,

    /// URL for more information about this bitfile.
    #[serde(default)]
    pub about_url: Option<String>,
}

// ============================================================================
// Config Loading
// ============================================================================

/// The default config file name (searched in the working directory).
const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// Environment variable for overriding the config file path.
const CONFIG_ENV_VAR: &str = "OKFPGA_CONFIG";

/// Load the server configuration.
///
/// Resolution order:
/// 1. Check `--config <path>` CLI argument
/// 2. Check `OKFPGA_CONFIG` environment variable
/// 3. Fall back to `./config.toml`
/// 4. If no file found, use built-in defaults
///
/// After loading, the config is validated against compiled features.
/// Returns the parsed config and the path it was loaded from (if any).
pub fn load_config() -> (ServerConfig, Option<PathBuf>) {
    let config_path = resolve_config_path();

    let config = if let Some(path) = &config_path {
        if path.exists() {
            info!(path = %path.display(), "loading configuration");
            match load_from_file(path) {
                Ok(cfg) => cfg,
                Err(e) => {
                    warn!(
                        error = %e,
                        path = %path.display(),
                        "failed to parse config file — using defaults"
                    );
                    ServerConfig::default()
                }
            }
        } else {
            info!(
                path = %path.display(),
                "config file not found — using defaults"
            );
            ServerConfig::default()
        }
    } else {
        info!("no config file specified — using defaults");
        ServerConfig::default()
    };

    // Apply environment variable overrides.
    let config = apply_env_overrides(config);

    // Validate against compiled features.
    validate_feature_flags(&config);

    (config, config_path)
}

/// Save the server configuration back to its TOML file.
///
/// This is used by the dashboard Settings panel to persist runtime changes
/// (e.g., server description, endpoint enforcement level). The entire
/// `ServerConfig` is serialized and written to the file, preserving all
/// fields — not just the ones that were changed.
///
/// Returns `Ok(())` on success, or an error if the path is unknown or the
/// write fails.
pub fn save_config(config: &ServerConfig, path: &Path) -> anyhow::Result<()> {
    let toml_str = toml::to_string_pretty(config)?;
    std::fs::write(path, toml_str)?;
    info!(path = %path.display(), "configuration saved to disk");
    Ok(())
}

/// Resolve the config file path from CLI args or environment.
fn resolve_config_path() -> Option<PathBuf> {
    // Check CLI args: `--config <path>` or `-c <path>`.
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len().saturating_sub(1) {
        if args[i] == "--config" || args[i] == "-c" {
            return Some(PathBuf::from(&args[i + 1]));
        }
    }

    // Check environment variable.
    if let Ok(path) = std::env::var(CONFIG_ENV_VAR) {
        return Some(PathBuf::from(path));
    }

    // Fall back to default path.
    let default_path = PathBuf::from(DEFAULT_CONFIG_FILE);
    Some(default_path)
}

/// Load and parse a TOML config file.
fn load_from_file(path: &Path) -> anyhow::Result<ServerConfig> {
    let content = std::fs::read_to_string(path)?;
    let config: ServerConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Apply environment variable overrides to the configuration.
///
/// Supported overrides:
/// - `OKFPGA_LOG_LEVEL` → `server.log_level`
/// - `OKFPGA_MANAGEMENT_PORT` → `server.management_port`
/// - `OKFPGA_REST_PORT` → `rest.port`
/// - `OKFPGA_REST_ENABLED` → `rest.enabled`
/// - `OKFPGA_GRPC_PORT` → `grpc.port`
/// - `OKFPGA_GRPC_ENABLED` → `grpc.enabled`
/// - `OKFPGA_MQTT_BROKER_URL` → `mqtt.broker_url`
/// - `OKFPGA_MQTT_ENABLED` → `mqtt.enabled`
/// - `OKFPGA_MDNS_ENABLED` → `mdns.enabled`
/// - `OKFPGA_WARN_RANGE_SELF_OVERLAP` → `access_control.warn_range_self_overlap`
fn apply_env_overrides(mut config: ServerConfig) -> ServerConfig {
    if let Ok(val) = std::env::var("OKFPGA_LOG_LEVEL") {
        config.server.log_level = val;
    }
    if let Ok(val) = std::env::var("OKFPGA_MANAGEMENT_PORT") {
        if let Ok(port) = val.parse::<u16>() {
            config.server.management_port = port;
        }
    }
    if let Ok(val) = std::env::var("OKFPGA_REST_PORT") {
        if let Ok(port) = val.parse::<u16>() {
            config.rest.port = port;
        }
    }
    if let Ok(val) = std::env::var("OKFPGA_REST_ENABLED") {
        config.rest.enabled = parse_bool(&val, config.rest.enabled);
    }
    if let Ok(val) = std::env::var("OKFPGA_GRPC_PORT") {
        if let Ok(port) = val.parse::<u16>() {
            config.grpc.port = port;
        }
    }
    if let Ok(val) = std::env::var("OKFPGA_GRPC_ENABLED") {
        config.grpc.enabled = parse_bool(&val, config.grpc.enabled);
    }
    if let Ok(val) = std::env::var("OKFPGA_MQTT_BROKER_URL") {
        config.mqtt.broker_url = val;
    }
    if let Ok(val) = std::env::var("OKFPGA_MQTT_ENABLED") {
        config.mqtt.enabled = parse_bool(&val, config.mqtt.enabled);
    }
    if let Ok(val) = std::env::var("OKFPGA_MDNS_ENABLED") {
        config.mdns.enabled = parse_bool(&val, config.mdns.enabled);
    }
    if let Ok(val) = std::env::var("OKFPGA_WARN_RANGE_SELF_OVERLAP") {
        config.access_control.warn_range_self_overlap =
            parse_bool(&val, config.access_control.warn_range_self_overlap);
    }
    if let Ok(val) = std::env::var("OKFPGA_WARN_READONLY_SIDE_EFFECTS") {
        config.access_control.warn_readonly_side_effects =
            parse_bool(&val, config.access_control.warn_readonly_side_effects);
    }
    if let Ok(val) = std::env::var("OKFPGA_WARN_WILDCARD_ENDPOINTS") {
        config.access_control.warn_wildcard_endpoints =
            parse_bool(&val, config.access_control.warn_wildcard_endpoints);
    }
    if let Ok(val) = std::env::var("OKFPGA_ENDPOINT_ENFORCEMENT") {
        config.access_control.endpoint_enforcement = val;
    }
    config
}

/// Parse a boolean from a string (supports `true/false`, `1/0`, `yes/no`).
fn parse_bool(s: &str, default: bool) -> bool {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => {
            warn!(value = %s, "unrecognized boolean value — using default");
            default
        }
    }
}

/// Validate that enabled services have their Cargo features compiled.
///
/// Logs a warning for any service that is `enabled = true` in config but
/// whose feature flag was not compiled. The server continues — it just
/// won't start that service.
fn validate_feature_flags(config: &ServerConfig) {
    // REST
    #[cfg(not(feature = "rest"))]
    if config.rest.enabled {
        warn!(
            "REST server is enabled in config but the 'rest' feature is not compiled. \
             Rebuild with `--features rest` or set `rest.enabled = false` in config."
        );
    }

    // gRPC
    #[cfg(not(feature = "grpc"))]
    if config.grpc.enabled {
        warn!(
            "gRPC server is enabled in config but the 'grpc' feature is not compiled. \
             Rebuild with `--features grpc` or set `grpc.enabled = false` in config."
        );
    }

    // MQTT
    #[cfg(not(feature = "mqtt"))]
    if config.mqtt.enabled {
        warn!(
            "MQTT publisher is enabled in config but the 'mqtt' feature is not compiled. \
             Rebuild with `--features mqtt` or set `mqtt.enabled = false` in config."
        );
    }

    // mDNS
    #[cfg(not(feature = "mdns"))]
    if config.mdns.enabled {
        warn!(
            "mDNS discovery is enabled in config but the 'mdns' feature is not compiled. \
             Rebuild with `--features mdns` or set `mdns.enabled = false` in config."
        );
    }

    // Dashboard
    #[cfg(not(feature = "dashboard"))]
    if config.dashboard.enabled {
        warn!(
            "Web dashboard is enabled in config but the 'dashboard' feature is not compiled. \
             Rebuild with `--features dashboard` or set `dashboard.enabled = false` in config."
        );
    }

    // Suppress unused variable warnings when all features are compiled.
    let _ = config;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_ports() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.server.management_port, 9090);
        assert_eq!(cfg.rest.port, 8080);
        assert_eq!(cfg.grpc.port, 50051);
    }

    #[test]
    fn test_access_control_warn_range_self_overlap_omitted_defaults_true() {
        assert!(ServerConfig::default().access_control.warn_range_self_overlap);
        let cfg: ServerConfig =
            toml::from_str("[access_control]\nendpoint_enforcement = \"enforce\"\n").unwrap();
        assert!(cfg.access_control.warn_range_self_overlap);
        let cfg: ServerConfig =
            toml::from_str("[access_control]\nwarn_range_self_overlap = false\n").unwrap();
        assert!(!cfg.access_control.warn_range_self_overlap);
    }

    #[test]
    fn test_access_control_default_endpoint_enforcement_is_enforce() {
        // S-12 (44-E): Shipped default is now "enforce".
        assert_eq!(
            ServerConfig::default().access_control.endpoint_enforcement,
            "enforce"
        );
    }

    #[test]
    fn test_access_control_warn_readonly_side_effects_omitted_defaults_true() {
        // D75 F4 (44-E): Default is true when omitted.
        assert!(ServerConfig::default().access_control.warn_readonly_side_effects);
        let cfg: ServerConfig =
            toml::from_str("[access_control]\nendpoint_enforcement = \"enforce\"\n").unwrap();
        assert!(cfg.access_control.warn_readonly_side_effects);
        let cfg: ServerConfig =
            toml::from_str("[access_control]\nwarn_readonly_side_effects = false\n").unwrap();
        assert!(!cfg.access_control.warn_readonly_side_effects);
    }

    #[test]
    fn test_access_control_warn_wildcard_endpoints_omitted_defaults_true() {
        // Phase 45 Q4: Default is true when omitted.
        assert!(ServerConfig::default().access_control.warn_wildcard_endpoints);
        let cfg: ServerConfig =
            toml::from_str("[access_control]\nendpoint_enforcement = \"enforce\"\n").unwrap();
        assert!(cfg.access_control.warn_wildcard_endpoints);
        let cfg: ServerConfig =
            toml::from_str("[access_control]\nwarn_wildcard_endpoints = false\n").unwrap();
        assert!(!cfg.access_control.warn_wildcard_endpoints);
    }

    #[test]
    fn default_rest_is_enabled() {
        let cfg = ServerConfig::default();
        assert!(cfg.rest.enabled);
    }

    #[test]
    fn default_grpc_is_enabled() {
        let cfg = ServerConfig::default();
        assert!(cfg.grpc.enabled);
    }

    #[test]
    fn default_mqtt_is_disabled() {
        let cfg = ServerConfig::default();
        assert!(!cfg.mqtt.enabled);
    }

    #[test]
    fn default_mdns_is_disabled() {
        let cfg = ServerConfig::default();
        assert!(!cfg.mdns.enabled);
    }

    #[test]
    fn default_log_level_is_info() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.server.log_level, "info");
    }

    #[test]
    fn parse_bool_accepts_various_truthy_values() {
        assert!(parse_bool("true", false));
        assert!(parse_bool("1", false));
        assert!(parse_bool("yes", false));
        assert!(parse_bool("on", false));
        assert!(parse_bool("TRUE", false));
        assert!(parse_bool("Yes", false));
    }

    #[test]
    fn parse_bool_accepts_various_falsy_values() {
        assert!(!parse_bool("false", true));
        assert!(!parse_bool("0", true));
        assert!(!parse_bool("no", true));
        assert!(!parse_bool("off", true));
    }

    #[test]
    fn parse_bool_returns_default_for_unknown() {
        assert!(parse_bool("maybe", true));
        assert!(!parse_bool("dunno", false));
    }

    #[test]
    fn toml_round_trip() {
        let cfg = ServerConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: ServerConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.server.management_port, cfg.server.management_port);
        assert_eq!(parsed.rest.port, cfg.rest.port);
        assert_eq!(parsed.grpc.port, cfg.grpc.port);
        assert_eq!(parsed.mqtt.broker_url, cfg.mqtt.broker_url);
    }

    #[test]
    fn partial_toml_uses_defaults_for_missing_sections() {
        let toml_str = r#"
            [server]
            management_port = 9999
        "#;
        let cfg: ServerConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.server.management_port, 9999);
        // Missing sections should use defaults.
        assert_eq!(cfg.rest.port, 8080);
        assert!(cfg.rest.enabled);
        assert_eq!(cfg.grpc.port, 50051);
    }

    // ── Phase 43: sanitize_hostname tests ──────────────────────────────

    #[test]
    fn sanitize_hostname_lowercases() {
        assert_eq!(sanitize_hostname("THE-EXPANSE"), "the-expanse");
    }

    #[test]
    fn sanitize_hostname_replaces_spaces() {
        assert_eq!(sanitize_hostname("My Lab Server"), "my-lab-server");
    }

    #[test]
    fn sanitize_hostname_replaces_underscores() {
        assert_eq!(sanitize_hostname("lab_server_1"), "lab-server-1");
    }

    #[test]
    fn sanitize_hostname_replaces_dots() {
        assert_eq!(sanitize_hostname("server..name"), "server-name");
    }

    #[test]
    fn sanitize_hostname_strips_special_chars() {
        assert_eq!(sanitize_hostname("server@#$name!"), "server-name");
    }

    #[test]
    fn sanitize_hostname_collapses_consecutive_hyphens() {
        assert_eq!(sanitize_hostname("a---b"), "a-b");
    }

    #[test]
    fn sanitize_hostname_trims_leading_trailing_hyphens() {
        assert_eq!(sanitize_hostname("-server-"), "server");
    }

    #[test]
    fn sanitize_hostname_all_invalid_returns_unknown() {
        assert_eq!(sanitize_hostname("___"), "unknown");
        assert_eq!(sanitize_hostname(""), "unknown");
        assert_eq!(sanitize_hostname("---"), "unknown");
    }

    #[test]
    fn sanitize_hostname_already_clean() {
        assert_eq!(sanitize_hostname("already-clean"), "already-clean");
    }

    #[test]
    fn sanitize_hostname_complex() {
        assert_eq!(
            sanitize_hostname("  My Server (Lab #2)  "),
            "my-server-lab-2"
        );
    }
}
