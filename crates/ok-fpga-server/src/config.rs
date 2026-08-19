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

/// MQTT status publisher configuration (Phase 10).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MqttConfig {
    /// Whether the MQTT publisher should start on launch.
    pub enabled: bool,

    /// MQTT broker URL (e.g., `mqtt://localhost:1883`).
    pub broker_url: String,

    /// MQTT client ID. Should be unique per server instance.
    pub client_id: String,

    /// Topic prefix for all published messages.
    pub topic_prefix: String,

    /// How often to publish periodic status messages (seconds).
    pub publish_interval_secs: u64,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            broker_url: "mqtt://localhost:1883".to_string(),
            client_id: "okfpga-server-01".to_string(),
            topic_prefix: "okfpga/server01".to_string(),
            publish_interval_secs: 5,
        }
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
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_log_level: "info".to_string(),
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
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessControlConfig {
    /// Enforcement level for endpoint access checks.
    ///
    /// - `"off"`: No checks. Current v1 behavior. User ID not required.
    /// - `"warn"`: Log violations at WARN level but allow operations.
    ///   User ID is required on all requests.
    /// - `"enforce"`: Reject unauthorized operations with 403 / PERMISSION_DENIED.
    ///   User ID is required on all requests.
    ///
    /// Default: `"off"`.
    pub endpoint_enforcement: String,
}

impl Default for AccessControlConfig {
    fn default() -> Self {
        Self {
            endpoint_enforcement: "off".to_string(),
        }
    }
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
pub fn load_config() -> ServerConfig {
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

    config
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
}
