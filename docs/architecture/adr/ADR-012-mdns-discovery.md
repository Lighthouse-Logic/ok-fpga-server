# ADR-012: mDNS Service Discovery

**Status**: Implemented  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

In lab environments, multiple ok-FPGA-Servers may be running on different machines. Clients (CLI tools, GUI applications, scripts) need to discover available servers without hardcoding IP addresses or hostnames. mDNS (multicast DNS, RFC 6762) with DNS-SD (DNS Service Discovery, RFC 6763) provides zero-configuration network discovery.

## Decision

### Single Service Type Registration

Register a single DNS-SD service type `_okfpga._tcp` per server instance. Sub-service ports and capabilities are advertised via TXT records, rather than registering multiple separate service types.

**Rationale**: Registering `_http._tcp`, `_grpc._tcp`, etc. separately would:
- Pollute the mDNS namespace
- Make it harder for clients to correlate "these all belong to the same FPGA server"
- Conflict with other HTTP/gRPC services on the network

### Service Registration Details

```
Instance: "{instance_name}"           # From TOML config (e.g., "okfpga-lab-bench-1")
Service:  _okfpga._tcp.local.
Host:     {hostname}.local.
Port:     {management_port}            # Default 9090 (management/dashboard port)
TXT:
  version={api_version}                # e.g., "1.0.0"
  rest={rest_port}                     # e.g., "8080" (absent if disabled)
  grpc={grpc_port}                     # e.g., "50051" (absent if disabled)
  mqtt={mqtt_topic_prefix}             # e.g., "okfpga/server01" (absent if disabled)
  dashboard={management_port}          # e.g., "9090" (absent if disabled)
  devices={device_count}               # e.g., "3" (updated dynamically)
```

### TXT Record Conventions

- **Key-value pairs**: Standard DNS-SD TXT record format (`key=value`)
- **Absent keys mean disabled**: If a service is not compiled or not enabled, its TXT key is omitted entirely. Clients must handle missing keys.
- **Dynamic updates**: The `devices` count is updated when devices are added/removed. The mDNS library re-registers the service with updated TXT records.
- **All values are strings**: DNS-SD TXT values are always strings, even for numeric values.

### Feature Flag

mDNS is behind the `mdns` Cargo feature flag and is NOT a default feature.

```toml
[mdns]
enabled = false
instance_name = "okfpga-lab-bench-1"
```

### Instance Naming

The `instance_name` is set in the TOML config. If not specified, a default is generated from the hostname (e.g., `okfpga-{hostname}`). Instance names must be unique on the local network — if a collision is detected, the mDNS library handles it per RFC 6762 §9 (append a number).

### Client Discovery Pattern

Clients discover servers by browsing for `_okfpga._tcp.local.`:

```python
# Python client example using zeroconf
from zeroconf import ServiceBrowser, Zeroconf

class Listener:
    def add_service(self, zc, type_, name):
        info = zc.get_service_info(type_, name)
        print(f"Found: {name}")
        print(f"  Host: {info.server}")
        print(f"  Dashboard: http://{info.server}:{info.port}")
        if b"rest" in info.properties:
            print(f"  REST: http://{info.server}:{info.properties[b'rest'].decode()}")
        if b"grpc" in info.properties:
            print(f"  gRPC: {info.server}:{info.properties[b'grpc'].decode()}")

zc = Zeroconf()
browser = ServiceBrowser(zc, "_okfpga._tcp.local.", Listener())
```

### Rust Implementation

The mDNS service runs as a `ManagedService` (ADR-010):

```rust
pub struct MdnsService {
    config: MdnsConfig,
    daemon: ServiceDaemon,
    status: AtomicServiceStatus,
}

impl ManagedService for MdnsService {
    fn name(&self) -> &str { "mdns" }
    // ...
}
```

Library: [`mdns-sd`](https://crates.io/crates/mdns-sd) — pure Rust, async-compatible, well-maintained.

### Network Considerations

| Consideration | Details |
|---------------|---------|
| **Scope** | mDNS is link-local only (not routable). For cross-subnet discovery, use a DNS-SD proxy or static config. |
| **Multicast** | Requires multicast support on the network. Some corporate networks disable multicast. |
| **Firewall** | UDP port 5353 must be open for mDNS. |
| **IPv4/IPv6** | Register on both IPv4 (224.0.0.251) and IPv6 (ff02::fb). |
| **Multiple NICs** | The library handles multi-homed hosts by registering on all interfaces. |

## Consequences

### Positive
- **Zero-configuration** — clients discover servers without any manual setup
- **Standard protocol** — works with any mDNS/DNS-SD client library
- **Single service type** — clean namespace, easy client correlation
- **Dynamic updates** — device count and service availability update in real-time

### Negative
- **Link-local only** — does not work across subnets without additional infrastructure
- **Multicast dependency** — blocked on some corporate networks
- **UDP 5353** — may require firewall configuration

## References

- [RFC 6762 — Multicast DNS](https://datatracker.ietf.org/doc/html/rfc6762)
- [RFC 6763 — DNS-Based Service Discovery](https://datatracker.ietf.org/doc/html/rfc6763)
- [mdns-sd Crate](https://crates.io/crates/mdns-sd)
- [ADR-010 Service Manager](ADR-010-service-manager.md) — feature flags and lifecycle
