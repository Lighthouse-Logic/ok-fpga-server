# Phase 35 — Client Demo Bug Fixes & README Update

**Status:** ✅ Completed (2026-08-21)
**Triggered by:** Non-GUI client demos failing at runtime
**Scope:** 2 bugs across 4 non-GUI demo clients + README overhaul
**Amends:** No ADRs (client-side only)

---

## Phase 35-A: IPv6 URL Formatting in REST Discovery

**Root cause:** `rest_url()` / `grpc_url()` in `discovery.rs` formatted
URLs as `http://{addr}:{port}`, but when mDNS resolved an IPv6 link-local
address, the URL was malformed (`http://fe80::1:8088` — no brackets).

**Fix:**
- `preferred_addr()` — prefer IPv4 from the address list
- `format_addr()` — wrap IPv6 in brackets per RFC 2732
- Applied to both `rust-rest-client` and `rust-grpc-client`

## Phase 35-B: Python Unicode Crash on Windows

**Root cause:** Box-drawing characters (`━`, U+2501) crash on Windows
cp1252 console with `UnicodeEncodeError`.

**Fix:** `sys.stdout.reconfigure(encoding='utf-8', errors='replace')`
at the top of each demo script. Applied to both Python REST and gRPC.

## Phase 35-C: README Overhaul

Complete rewrite of `client-examples/README.md` covering:
- 4 non-GUI endpoint exerciser clients
- 8 interactive GUI demos (iced, egui, tkinter, DearPyGui)
- 4 common libraries
- Quick start commands for all frameworks
- Architecture dependency graph
- Hardware test guide (`--test-threads=1`, env vars, why serialization)
