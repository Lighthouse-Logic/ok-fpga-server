// chapters/overview/2-9-config.typ — Section 2.9: Configuration file.
// ref: config.toml; crates/ok-fpga-server/src/config.rs; ADR-008 (auth),
// ADR-024 (access_control), ADR-031/032 (mqtt), Phase 28D (catalog), Phase 45 (warn_wildcard_endpoints).
#import "../../lib.typ": *

== Configuration file <ov-config>

=== Server, transport and discovery tables <ov-config-tables>
// ref: config.toml; crates/ok-fpga-server/src/config.rs.
The server loads configuration in three layers (highest precedence last):
+ built-in defaults;
+ a TOML file selected by `--config <path>` (or `-c <path>`), the
  `OKFPGA_CONFIG` environment variable, or `./config.toml` in the working
  directory;
+ `OKFPGA_*` environment variable overrides (@ov-config-auth-access).

#table(
  columns: (1.5fr, 0.8fr, 1.1fr, 2fr),
  [Key], [Type], [Built-in default], [Meaning],
  [`server.management_port`], [u16], [`9090`], [Always-on management and dashboard TCP port.],
  [`server.management_bind`], [string], [`"0.0.0.0"`], [Bind address for the management listener.],
  [`server.log_level`], [string], [`"info"`], [`tracing` filter string (`trace`, `debug`, `info`, `warn`, `error` or per-module filter).],
  [`server.log_file`], [string], [`"logs/okfpga-server.log"`], [Append-mode log file path (`""` disables file logging).],
  [`server.user_timeout_secs`], [u64], [`60`], [Seconds of silence before an idle user session expires and its resources are released (`0` disables expiration, @ov-id-sessions).],
  [`server.user_sweep_interval_secs`], [u64], [`10`], [Interval in seconds between expired-session sweep checks.],
  [`server.description`], [string], [`""`], [Human-readable server description exposed in `/server/status`, mDNS and MQTT announce.],
  [`rest.enabled`], [bool], [`true`], [Start the REST server on launch.],
  [`rest.bind`], [string], [`"0.0.0.0"`], [Bind address for the REST listener.],
  [`rest.port`], [u16], [`8080` (`8088` in shipped `config.toml`)], [TCP port for the REST API (`/api/v1`).],
  [`grpc.enabled`], [bool], [`true`], [Start the gRPC server on launch.],
  [`grpc.bind`], [string], [`"0.0.0.0"`], [Bind address for the gRPC listener.],
  [`grpc.port`], [u16], [`50051`], [TCP port for the gRPC API.],
  [`mqtt.enabled`], [bool], [`false` (`true` in `config.toml`)], [Start the MQTT v5 status publisher (@ov-events-mqtt).],
  [`mqtt.broker_url`], [string], [`"mqtt://localhost:1883"`], [MQTT broker URL.],
  [`mqtt.publish_interval_secs`], [u64], [`5`], [State poll interval in seconds for report-by-exception publishing.],
  [`mqtt.heartbeat_max_secs`], [u64], [`300`], [Maximum seconds between forced heartbeat publishes (`0` = rely on LWT only).],
  [`mqtt.announce_interface`], [string], [`""`], [Network interface name (e.g. `"eth0"`, `"Ethernet"`) used to resolve the server IP in `server/announce`.],
  [`mqtt.preferred_subnets`], [string array], [`[]`], [Ordered CIDR subnets (e.g. `["10.40.40.0/24"]`) used to pick a LAN interface when `announce_interface` is empty and multiple NICs exist.],
  [`mdns.enabled`], [bool], [`false` (`true` in `config.toml`)], [Advertise `_okfpga._tcp.local.` via mDNS (@ov-events-mdns).],
  [`mdns.instance_name`], [string], [`"okfpga-server-<hostname>"`], [DNS-SD instance name (auto-derived from sanitised hostname unless overridden).],
  [`dashboard.enabled`], [bool], [`true`], [Serve the web dashboard (@ov-arch-dashboard).],
  [`dashboard.default_log_level`], [string], [`"info"`], [Initial log filter level in the dashboard log viewer.],
  [`dashboard.expose_settings`], [bool], [`true`], [Allow runtime edits to `endpoint_enforcement` on the dashboard Settings page.],
  [`catalog.syzygy_boards`], [table array], [`[]`], [Known SYZYGY pods (`product_name`, `product_model`, `product_version`, `about_url`).],
  [`catalog.devices`], [table array], [`[]`], [Known FPGA models (`product_name`, `about_url`).],
  [`catalog.bitfiles`], [table array], [`[]`], [Known bitfiles (`filename`, `description`, `about_url`).],
)

=== Authentication and access-control tables and environment overrides <ov-config-auth-access>
// ref: ADR-008; ADR-024; ADR-034 §8, §10, §17; config.rs apply_env_overrides;
// @enf-levels; @enf-readonly-warning.

#table(
  columns: (1.6fr, 0.8fr, 0.9fr, 2.1fr),
  [Key], [Type], [Default], [Meaning],
  [`auth.provider`], [string], [`"none"`], [`"none"`, `"local"` or `"ldap"` (@ov-id-auth).],
  [`auth.session_expiry_secs`], [u64], [`28800`], [Authenticated session lifetime in seconds (8 hours).],
  [`auth.local.users`], [table array], [`[]`], [Local user credentials when `auth.provider = "local"`.],
  [`auth.ldap.*`], [table], [—], [LDAP settings: `url`, `use_ssl`, `domain_components`, `organizational_units`, `uid_attribute`.],
  [`access_control.endpoint_enforcement`], [string], [`"enforce"`], [`"off"`, `"warn"` or `"enforce"` (@enf-levels).],
  [`access_control.warn_range_self_overlap`], [bool], [`true`], [Log and return a registration warning when two `address_ranges` in the same declaration overlap (@res-registration, @enf-readonly-warning).],
  [`access_control.warn_readonly_side_effects`], [bool], [`true`], [Log and return a registration warning when `ReadOnly` holders could reach a non-private `TriggerOut`, `PipeOut`, `BTPipeOut` or `AddressRange` (@enf-readonly-warning).],
  [`access_control.warn_wildcard_endpoints`], [bool], [`true`], [Log and return a registration warning when a Classic resource omits `endpoints` (`0x00`–`0x7F` assumed) or coexists with a Pipe-declaring resource (@res-endpoint-shapes, @enf-readonly-warning).],
)

Supported environment variable overrides:

#table(
  columns: (1.8fr, 1.8fr, 1.4fr),
  [Environment variable], [Overrides config key], [Accepted values],
  [`OKFPGA_CONFIG`], [Config file path], [Filesystem path to `.toml` file],
  [`OKFPGA_LOG_LEVEL`], [`server.log_level`], [`trace`, `debug`, `info`, `warn`, `error`, or filter],
  [`OKFPGA_MANAGEMENT_PORT`], [`server.management_port`], [`u16` port number],
  [`OKFPGA_REST_PORT`], [`rest.port`], [`u16` port number],
  [`OKFPGA_REST_ENABLED`], [`rest.enabled`], [`true`/`false`, `1`/`0`, `yes`/`no`, `on`/`off`],
  [`OKFPGA_GRPC_PORT`], [`grpc.port`], [`u16` port number],
  [`OKFPGA_GRPC_ENABLED`], [`grpc.enabled`], [boolean],
  [`OKFPGA_MQTT_BROKER_URL`], [`mqtt.broker_url`], [Broker URL string],
  [`OKFPGA_MQTT_ENABLED`], [`mqtt.enabled`], [boolean],
  [`OKFPGA_MDNS_ENABLED`], [`mdns.enabled`], [boolean],
  [`OKFPGA_ENDPOINT_ENFORCEMENT`], [`access_control.endpoint_enforcement`], [`off`, `warn`, `enforce`],
  [`OKFPGA_WARN_RANGE_SELF_OVERLAP`], [`access_control.warn_range_self_overlap`], [boolean],
  [`OKFPGA_WARN_READONLY_SIDE_EFFECTS`], [`access_control.warn_readonly_side_effects`], [boolean],
  [`OKFPGA_WARN_WILDCARD_ENDPOINTS`], [`access_control.warn_wildcard_endpoints`], [boolean],
)
