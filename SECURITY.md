# Security Policy

Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic.

## Supported Versions

Security updates are applied to the latest release on the `main` branch:

| Version | Supported |
| ------- | --------- |
| `0.3.x` (`main`) | :white_check_mark: |
| `< 0.3.0` | :x: |

## Deployment Security Posture (Important)

Please review the current security boundaries before deploying **`ok-fpga-server`**:

1. **Trusted Network / Reverse Proxy Required**:
   - By default, `ok-fpga-server` serves plaintext HTTP (`:8088` REST, `:9090` Management/Dashboard) and plaintext HTTP/2 (`:50051` gRPC).
   - Built-in `[auth]` providers (`local` and `ldap` in `crates/ok-fpga-core/src/auth.rs`) are currently **architectural stubs** (`ADR-008`) that do not verify password hashes or bind to an external LDAP server, and user identity (`X-User-Id`) is self-asserted for cooperative lab arbitration.
   - **Do NOT expose ports `8088`, `9090`, or `50051` directly to the public internet or untrusted networks.** Deploy on an isolated lab LAN/VPN, or place the server behind an authenticating TLS reverse proxy or zero-trust tunnel (e.g., Caddy/Nginx/Envoy with mTLS/OIDC, SSH/WireGuard, or Cloudflare Zero Trust Access).

2. **Physical Hardware & Bitstream Access**:
   - Clients with network access to `ok-fpga-server` can load FPGA bitstreams, toggle resets, and read/write hardware endpoints. Ensure only authorized testbench hosts and engineers can reach the daemon.

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub Issues, Discussions, or Pull Requests.**

You can report a vulnerability privately using either of the following channels:

1. **GitHub Private Vulnerability Reporting (Preferred)**:
   - Navigate to the [**Security → Advisories**](https://github.com/Lighthouse-Logic/ok-fpga-server/security/advisories/new) tab of this repository and click **"Report a vulnerability"** to open a private security advisory directly with the maintainers.
2. **Email**:
   - Send a report to **[admin@lighthouselogic.dev](mailto:admin@lighthouselogic.dev)** with the subject line: `[SECURITY] ok-fpga-server vulnerability report`.

### What to Include
- Affected crate, endpoint, or feature flag (`rest`, `grpc`, `dashboard`, `mqtt`, `mdns`, `ok-fpga-ffi`, `ok-fpga-core`)
- Reproduction steps or proof-of-concept request/configuration
- Potential impact and any suggested remediation

### Response Timeline
- **Acknowledgement:** Within **3 business days**
- **Triage & Status Update:** Within **7 business days**
- **Coordinated Disclosure:** Once a fix is released on `main`, we will publish a GitHub Security Advisory crediting the reporter (unless anonymity is requested).
