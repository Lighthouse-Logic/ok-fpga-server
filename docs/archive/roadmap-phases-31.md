# Roadmap Archive — Phase 31

## Phase 31 — OS-Native mDNS Service Registration (ADR-027)

**Status:** ✅ Completed 2026-08-21  
**ADR:** [ADR-027 — OS-Native mDNS Registration](../architecture/adr/ADR-027-native-mdns-registration.md)  
**Commits:** `8e7dbd5` (docs), `502f995` (tiered registration), `3ce6441` (A/AAAA fix + client IPv4)

### Focus

Replace the embedded `mdns-sd` mDNS responder with OS-native service
registration via `dns-sd-native` (Windows: dnscache, Linux: avahi-daemon).
Fix missing A/AAAA address records in DNS-SD responses. Fix client-side
IPv6 address selection bug.

### Context

The embedded `mdns-sd` library created a second mDNS responder on UDP
port 5353, competing with the OS's native mDNS daemon (Windows dnscache,
Linux avahi-daemon). This dual-responder conflict made server announcements
invisible to other devices on the network.

### Key Design Decisions

1. **Tiered registration strategy** — try OS-native first, fall back to
   `mdns-sd` only when no OS daemon is running (safe — no dual-responder
   conflict when the daemon is absent).

2. **Windows-specific Win32 wrapper** (`win32_mdns.rs`) — bypasses the
   `dns-sd-native` builder to call `DnsServiceConstructInstance` directly
   with `pIp4`, ensuring the DNS-SD response includes an A record.
   On Linux, Avahi handles A/AAAA records automatically via D-Bus.

3. **`enable_addr_auto()`** on the fallback `mdns-sd::ServiceInfo` — auto-
   detects local addresses and includes A/AAAA records.

4. **Client IPv4 preference** — all 4 browser libraries (Rust REST, Rust
   gRPC, Python REST, Python gRPC) now prefer IPv4 addresses from the
   resolved address set, since the server binds to `0.0.0.0` (IPv4 only).

5. **Service type enumeration not supported** — Windows' `DnsServiceRegister`
   does not register the RFC 6763 §9 `_services._dns-sd._udp` meta-query
   PTR record. Tools like the Avahi Discovery GUI app won't list the service,
   but direct `_okfpga._tcp` browsing (which all clients use) works correctly.

### Verification

- `cargo build --features full` ✅
- `cargo test -p ok-fpga-server --features full` ✅ (60 tests, 7 mDNS-specific)
- `avahi-browse -r _okfpga._tcp` on remote Ubuntu machine ✅ (service visible)
- A/AAAA records confirmed in Wireshark

### Files Changed

| File | Change |
|------|--------|
| `docs/architecture/adr/ADR-027-native-mdns-registration.md` | New ADR + addendum |
| `docs/architecture/adr/ADR-012-mdns-discovery.md` | Addendum: superseded |
| `crates/ok-fpga-server/Cargo.toml` | `dns-sd-native` + `mdns-sd` + `local-ip-address` + `windows` |
| `crates/ok-fpga-server/src/mdns.rs` | Tiered registration + A/AAAA fix |
| `crates/ok-fpga-server/src/win32_mdns.rs` | New: Win32 DnsServiceConstructInstance with pIp4 |
| `client-examples/rust-rest-gui-common/src/mdns_browser.rs` | Prefer IPv4 |
| `client-examples/rust-grpc-gui-common/src/mdns_browser.rs` | Prefer IPv4 |
| `client-examples/python_rest_gui_common/mdns_browser.py` | Prefer IPv4 + inet_ntop |
| `client-examples/python_grpc_gui_common/mdns_browser.py` | Prefer IPv4 + inet_ntop |
