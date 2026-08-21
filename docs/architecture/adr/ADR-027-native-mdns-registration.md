# ADR-027: OS-Native mDNS Service Registration

**Date:** 2026-08-21  
**Status:** Accepted (Phase 31)  
**Supersedes:** [ADR-012](ADR-012-mdns-discovery.md) §"Rust Implementation" (library choice only)  
**Extends:** [ADR-012](ADR-012-mdns-discovery.md) (service type, TXT records, feature flags unchanged)

---

## Context

ADR-012 established mDNS/DNS-SD zero-configuration discovery using the
[`mdns-sd`](https://crates.io/crates/mdns-sd) crate — a pure-Rust mDNS
implementation that runs its own embedded mDNS responder on UDP port 5353.

This worked for **same-process discovery** (the server registers, and a client
in the same process or machine can browse). However, **cross-machine discovery
is broken** on both target platforms:

### Windows 11

The **DNS Client service (dnscache)** already owns UDP port 5353 as the system's
mDNS responder. When `mdns-sd` creates a second responder on the same port:

1. **Outbound multicast** — announcements compete with `dnscache` and often
   never leave the machine.
2. **Inbound queries** — remote mDNS queries are delivered to `dnscache` (which
   knows nothing about `_okfpga._tcp`), not to the `mdns-sd` socket.
3. **False positive** — the library reports registration success (internal state
   change), but no packets reach the network.

### Ubuntu 22.04 / 24.04 / 26.04

**`avahi-daemon`** is the system's mDNS responder and similarly owns port 5353.
The `mdns-sd` embedded responder creates the same dual-responder conflict,
leading to unreliable or invisible service announcements.

### Observed Symptom

The server logs "mDNS service registered" on startup, but no mDNS browser on
any other machine on the same LAN (including standard tools like `dns-sd -B`
on Windows or `avahi-browse` on Linux) can discover the `_okfpga._tcp` service.

---

## Decision

### 1. Replace Server-Side Registration Library

Replace `mdns-sd` with [`dns-sd-native`](https://docs.rs/dns-sd-native) for
**server-side service registration only**. This crate delegates to the OS's
native mDNS daemon:

| OS | Underlying Daemon | API Used |
|---|---|---|
| **Windows 11** | dnscache (DNS Client service) | Win32 `DnsServiceRegister` via `dnsapi.dll` |
| **Ubuntu 22.04+** | avahi-daemon | D-Bus (`org.freedesktop.Avahi`) |

By registering through the OS daemon that already owns port 5353:
- No port conflict — the single canonical responder handles all multicast
- Announcements go out on all correct network interfaces
- The OS handles probe/announce/goodbye per RFC 6762
- Remote browsers see the service immediately

### 2. Client-Side Browsing Unchanged

The 8 GUI demos continue using their existing mDNS browsing libraries:

| Client | Library | Rationale |
|--------|---------|-----------|
| Rust GUI demos | `mdns-sd` | Passive multicast listening works fine as a second listener |
| Python GUI demos | `zeroconf` | Same — passive browsing is not affected by the dual-responder issue |

Client-side browsing does not compete with the OS daemon for port ownership —
it joins the multicast group as a listener, which both Windows and Linux handle
correctly even with multiple listeners.

### 3. Failure Modes — Tiered Strategy

Two distinct failure cases with **different handling**:

#### Case 1: OS mDNS daemon not running → fall back to `mdns-sd`

The daemon may be absent or stopped (e.g., minimal Docker image, `avahi-daemon`
not installed, Windows DNS Client service disabled). In this case:

- **Fall back to the embedded `mdns-sd` responder** — because the OS daemon is
  absent, there is no dual-responder conflict; `mdns-sd` is the sole mDNS stack
  and will function correctly.
- Log at `WARN` level: "No host OS mDNS daemon detected, starting stand-alone
  mDNS server"
- The `ManagedService` reports `ServiceStatus::Running` — mDNS is operational,
  just via the fallback path.

This is safe precisely because the dual-responder problem only exists when both
the OS daemon **and** `mdns-sd` run simultaneously on port 5353. If the OS
daemon isn't there, `mdns-sd` is the only listener and works fine.

#### Case 2: OS daemon IS running but registration fails → error, no fallback

This indicates a bug in the registration code or an unexpected OS configuration
problem. The server should:

- Log the error at `ERROR` level with full context
- Report `ServiceStatus::Failed` with the error details
- **Do NOT fall back to `mdns-sd`** — the OS daemon IS running and owns
  port 5353, so `mdns-sd` would recreate the dual-responder conflict. Falling
  back would mask the bug.

### 4. Cargo Dependency Changes

Both `dns-sd-native` (primary) and `mdns-sd` (fallback) are required:

```toml
# Server-side (ok-fpga-server):
# Before:
mdns-sd = { version = "0.13", optional = true }
# After:
dns-sd-native = { version = "0.1", optional = true }
mdns-sd = { version = "0.13", optional = true }
```

Feature flag mapping:
```toml
mdns = ["dep:dns-sd-native", "dep:mdns-sd"]  # was: mdns = ["dep:mdns-sd"]
```

Client-side dependencies (`rust-rest-gui-common`, `rust-grpc-gui-common`) keep
`mdns-sd = "0.13"` for browsing (unchanged).

### 5. Clean Shutdown

The current implementation leaks the `ServiceDaemon` with a comment:
> "We intentionally leak it here — it will be cleaned up when the process exits."

`dns-sd-native` provides a registration handle that can be stored and dropped
for clean deregistration. The `MdnsDiscoveryService` will store this handle and
properly deregister the service in `stop()`.

### 6. What Does NOT Change

Everything defined in ADR-012 **except** the Rust library choice remains valid:

- Service type: `_okfpga._tcp.local.`
- TXT record format and conventions
- Instance naming from TOML config
- Feature flag: `mdns` (optional, not default)
- `ManagedService` lifecycle integration (ADR-010)
- Client discovery pattern

---

## Addendum: A/AAAA Address Records (2026-08-21)

Cross-machine testing revealed that **avahi-browse** on remote Linux clients
could not discover the service, even though the mDNS browsers in the GUI
examples (`mdns-sd`, Python `zeroconf`) could. Root cause: **missing A/AAAA
address records** in the DNS-SD response.

### RFC 6763 §12 — Required Record Chain

A compliant DNS-SD service response must include (or make resolvable):

```
PTR  _okfpga._tcp.local.  →  okfpga-server._okfpga._tcp.local.
SRV  okfpga-server._okfpga._tcp.local.  →  THE-EXPANSE.local. port 9090
TXT  okfpga-server._okfpga._tcp.local.  →  version=0.1.0, ...
A    THE-EXPANSE.local.  →  192.168.x.x        ← WAS MISSING
```

### Deficiency in dns-sd-native (Windows path)

The `dns-sd-native` crate passes `None` for both `pIp4` and `pIp6` when calling
`DnsServiceConstructInstance`. Per Microsoft documentation, `NULL` IP parameters
mean the `DNS_SERVICE_INSTANCE` has no address records. Windows' dnscache
advertises PTR + SRV + TXT but does **not** include A/AAAA records in the
response — and may not reliably respond to separate A-record queries for the
machine's hostname from remote mDNS clients.

**Fix**: Bypass the `dns-sd-native` builder on Windows and call the Win32
`DnsServiceConstructInstance` API directly from `mdns.rs`, passing the resolved
local IPv4 address as `pIp4`. The `dns-sd-native` crate is still used on Linux
(Avahi handles A/AAAA records automatically via its D-Bus API).

### Deficiency in mdns-sd (fallback path)

The fallback registration passed `""` (empty string) for the host address
without calling `.enable_addr_auto()` on the `ServiceInfo`. This meant the
embedded responder also omitted A/AAAA records.

**Fix**: Chain `.enable_addr_auto()` on the `ServiceInfo` so `mdns-sd`
auto-detects local addresses and includes them in responses.

### Client-Side IPv6 Issue

When A/AAAA records were present, clients sometimes picked an IPv6 link-local
address (`fe80::…`) from the non-deterministic `HashSet<IpAddr>` iteration
order, then failed to connect (server binds `0.0.0.0`, IPv4 only).

**Fix**: Client browsers prefer IPv4 addresses via `.find(|a| a.is_ipv4())`.

### Known Limitation: Service Type Enumeration (RFC 6763 §9)

Windows' `DnsServiceRegister` does **not** register the service type
enumeration meta-query PTR record:

```
_services._dns-sd._udp.local.  →  _okfpga._tcp.local.
```

This means tools that enumerate *all* service types on the network (e.g.,
the **Avahi Discovery** GUI app, `avahi-browse _services._dns-sd._udp`) will
not list `_okfpga._tcp` among their results. This is a known limitation of
Windows' native mDNS stack — Avahi and Bonjour register this meta-query
automatically, but Windows dnscache does not.

**Impact**: None for production use. All ok-FPGA client browsers query for
`_okfpga._tcp` directly (not via `_services._dns-sd._udp`), and this works
correctly. The `avahi-browse -r _okfpga._tcp` command successfully discovers
the server.

---

## Files Changed

| File | Change |
|------|--------|
| `crates/ok-fpga-server/Cargo.toml` | `mdns-sd` → `dns-sd-native` + `mdns-sd` |
| `crates/ok-fpga-server/src/mdns.rs` | Tiered registration + A/AAAA record fix |
| `client-examples/rust-rest-gui-common/src/mdns_browser.rs` | Prefer IPv4 addresses |
| `client-examples/python_rest_gui_common/mdns_browser.py` | Prefer IPv4 addresses |
| `docs/architecture/adr/ADR-012-mdns-discovery.md` | Addendum: §"Rust Implementation" superseded by ADR-027 |
| `docs/architecture/adr/ADR-027-native-mdns-registration.md` | This ADR |
| `docs/architecture/overview.md` | Add ADR-027 to index |
| `docs/development-roadmap.md` | Phase 31 added as Now |

---

## Future Work

- **Dynamic TXT record updates** — if the `dns-sd-native` update mechanism is
  trivial, consider also announcing available resources dynamically (in addition
  to the existing `devices` count).
- **mDNS browser unit tests** — deferred from Phase 28A, still applicable.
- **MQTT-based cross-subnet discovery** — for environments where mDNS
  (link-local only) is insufficient.

---

## References

- [ADR-012 — mDNS/DNS-SD Service Discovery](ADR-012-mdns-discovery.md) (partially superseded)
- [RFC 6762 — Multicast DNS](https://datatracker.ietf.org/doc/html/rfc6762)
- [RFC 6763 — DNS-Based Service Discovery](https://datatracker.ietf.org/doc/html/rfc6763)
- [`dns-sd-native` crate](https://docs.rs/dns-sd-native)
- [Windows DnsServiceRegister](https://learn.microsoft.com/en-us/windows/win32/api/windns/nf-windns-dnsserviceregister)
- [Windows DnsServiceConstructInstance](https://learn.microsoft.com/en-us/windows/win32/api/windns/nf-windns-dnsserviceconstructinstance) — `pIp4`/`pIp6` parameters
- [Avahi D-Bus API](https://www.freedesktop.org/wiki/Software/Avahi/)

