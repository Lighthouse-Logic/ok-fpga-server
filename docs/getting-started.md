# ok-FPGA-Server — Getting Started Guide

A step-by-step guide for setting up the ok-FPGA-Server on a **server machine**
(with an FPGA attached) and controlling it remotely from a **client machine**
using the example clients and the web dashboard.

---

## Architecture Overview

```
        Client Machine                           Server Machine
  ┌─────────────────────────┐            ┌───────────────────────────────┐
  │  Web Browser            │───:9090──▶ │  Web Dashboard (SPA)          │
  │  Python / Rust Client   │───:8088──▶ │  REST API (Axum)              │
  │  Python / Rust Client   │──:50051──▶ │  gRPC API (Tonic)             │
  │  MQTT Explorer          │──:1883───▶ │  MQTT Publisher → Broker      │
  └─────────────────────────┘            │                               │
                                         │        ok-fpga-core           │
                                         │        ok-fpga-ffi            │
                                         │            │                  │
                                         │     okFrontPanel.dll/.so      │
                                         │            │  USB3/PCIe       │
                                         │     ┌──────┴───────┐         │
                                         │     │  Opal Kelly  │         │
                                         │     │  XEM7320 etc │         │
                                         │     └──────────────┘         │
                                         └───────────────────────────────┘
```

**Ports summary:**

| Port  | Protocol | Service | Required |
|-------|----------|---------|----------|
| 9090  | HTTP | Management server + Web Dashboard | Always |
| 8088  | HTTP | REST API | Yes (default feature) |
| 50051 | HTTP/2 | gRPC API | Only with `--features grpc` |
| 1883  | MQTT | MQTT broker (external) | Optional |

---

## Part 1 — Server Machine Setup

### 1.1 Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| **Rust** | 1.70+ (2021 edition) | `rustup` recommended |
| **libclang** | — | Required by `bindgen` for C header parsing |
| **protoc** | 3.x+ | Required only for gRPC (`--features grpc`) |
| **FrontPanel SDK** | 6.1.0 | `okFrontPanel.dll` (Windows) or `libokFrontPanel.so` (Linux) |
| **Opal Kelly FPGA** | USB3/PCIe | XEM7320, XEM8305, XEM8310, XEM8320, XEM8350, XEM8370, HUB1450 |

#### Windows Setup

```powershell
# 1. Install Rust
winget install Rust.Rustup

# 2. Install LLVM (provides libclang for bindgen)
winget install LLVM.LLVM
# Then set:
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"

# 3. Install protoc (needed for gRPC feature)
winget install Google.Protobuf
# Or download manually and set:
$env:PROTOC = "C:\tools\protoc\bin\protoc.exe"

# 4. Connect your Opal Kelly FPGA via USB3 or PCIe
# 5. Install the FrontPanel USB driver if needed (from the Opal Kelly website)
```

#### Ubuntu Setup

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Install dependencies
sudo apt update
sudo apt install libclang-dev protobuf-compiler libusb-1.0-0-dev

# 3. Install the FrontPanel 6.1.0 shared library
# Copy libokFrontPanel.so to /usr/local/lib or set LD_LIBRARY_PATH
```

### 1.2 Clone and Build

```bash
git clone <repository-url> ok-fpga-server
cd ok-fpga-server

# Build with all features (REST + gRPC + dashboard + MQTT + mDNS):
cargo build --features full

# Or build with defaults only (REST + dashboard):
cargo build
```

### 1.3 Configure for Network Access

The server binds to `0.0.0.0` by default, so it is accessible from any machine
on the network. The default configuration file is [`config.toml`](config.toml):

```toml
# === config.toml ===

[server]
management_port = 9090      # Web dashboard + management API
management_bind = "0.0.0.0" # Listen on all interfaces
log_level = "info"

[rest]
enabled = true
bind = "0.0.0.0"            # Listen on all interfaces
port = 8088

[grpc]
enabled = true
bind = "0.0.0.0"            # Listen on all interfaces
port = 50051

[dashboard]
enabled = true               # Serve the web UI at :9090

[mqtt]
enabled = false               # Enable if you have a Mosquitto broker

[mdns]
enabled = false               # Enable for zero-config LAN discovery

[auth]
provider = "none"             # No authentication by default
```

> [!IMPORTANT]
> If you have a firewall, open ports **9090** (dashboard), **8088** (REST),
> and **50051** (gRPC) for inbound TCP connections from the client machine.

### 1.4 Start the Server

```bash
# Run with full features:
cargo run --features full

# Or with environment overrides:
OKFPGA_LOG_LEVEL=debug cargo run --features full

# Or specify a custom config file:
cargo run --features full -- --config /path/to/my-config.toml
```

You should see output like:

```
  INFO  ok_fpga_server: configuration loaded from config.toml
  INFO  ok_fpga_server: management server started on 0.0.0.0:9090
  INFO  ok_fpga_server: REST API started on 0.0.0.0:8088
  INFO  ok_fpga_server: gRPC API started on 0.0.0.0:50051
  INFO  ok_fpga_server::device_monitor: discovered device: XEM7320-A75T (serial: 2237000ETR)
```

### 1.5 Verify the Server is Running

From the server machine itself, test with `curl`:

```bash
# Server status (management API):
curl http://localhost:9090/mgmt/status

# List connected FPGA devices (REST API):
curl http://localhost:8088/api/v1/devices

# Server services (management API):
curl http://localhost:9090/mgmt/services
```

---

## Part 2 — Web Dashboard

The web dashboard is a single-page application (SPA) served on the management
port (default **9090**). It requires no installation — just open a browser.

### 2.1 Open the Dashboard

From any machine on the same network, navigate to:

```
http://<server-ip>:9090
```

For example, if the server machine's IP is `192.168.1.100`:

```
http://192.168.1.100:9090
```

### 2.2 Dashboard Pages

| Page | Description |
|------|-------------|
| **Overview** | Server uptime, connected device count, active resources, service status |
| **Devices** | List of all connected FPGAs with serial, board model, firmware version, and sensor data (temperature, voltage) |
| **Resources** | Registered FPGA resources, their access mode (shared/exclusive), current state, and holders |
| **Users** | Connected users and which resources they hold |
| **Services** | Start, stop, and restart individual services (REST, gRPC, MQTT, mDNS) |
| **Logs** | Real-time structured log stream via WebSocket |

### 2.3 Real-Time Updates

The dashboard connects via **WebSocket** (`ws://<server-ip>:9090/ws/events`) for
real-time event streaming. Events include:

- Device connected / disconnected
- Resource acquired / released / locked / unlocked
- Service started / stopped / failed
- Sensor readings updated

The connection status indicator in the sidebar footer shows the WebSocket state.

---

## Part 3 — Client Machine Setup

The client machine does **not** need an FPGA attached. It only needs network
access to the server machine.

### 3.1 Environment Variables

All clients use environment variables to locate the server:

| Variable | Default | Used By |
|----------|---------|---------|
| `OKFPGA_SERVER_URL` | `http://localhost:8088` | REST clients (Rust + Python) |
| `OKFPGA_GRPC_URL` | `http://localhost:50051` | Rust gRPC client |
| `OKFPGA_GRPC_TARGET` | `localhost:50051` | Python gRPC client |
| `OKFPGA_TEST_BITFILE` | `../../bitfiles/Counters.bit` | Hardware integration tests |

Set these to point at the **server machine**:

```bash
# Replace with your server's IP address:
export OKFPGA_SERVER_URL="http://192.168.1.100:8088"
export OKFPGA_GRPC_URL="http://192.168.1.100:50051"
export OKFPGA_GRPC_TARGET="192.168.1.100:50051"
```

**Windows (PowerShell):**

```powershell
$env:OKFPGA_SERVER_URL = "http://192.168.1.100:8088"
$env:OKFPGA_GRPC_URL   = "http://192.168.1.100:50051"
$env:OKFPGA_GRPC_TARGET = "192.168.1.100:50051"
```

### 3.2 Option A — Rust REST Client

```bash
cd client-examples/rust-rest-client

# Interactive demo (discovers and lists devices, reads sensors):
cargo run -- --url http://192.168.1.100:8088

# Run automated endpoint tests:
OKFPGA_SERVER_URL="http://192.168.1.100:8088" cargo test -- --ignored --test-threads=1 --nocapture
```

**What the demo does:**
1. Connects to the server via mDNS discovery (or the provided `--url`)
2. Lists all connected FPGA devices
3. Reads device info, sensors, and settings
4. If MQTT is available, publishes connection status

### 3.3 Option B — Rust gRPC Client

```bash
cd client-examples/rust-grpc-client

# Interactive demo:
cargo run -- --url http://192.168.1.100:50051

# Run automated RPC tests:
OKFPGA_GRPC_URL="http://192.168.1.100:50051" \
OKFPGA_SERVER_URL="http://192.168.1.100:8088" \
cargo test -- --ignored --test-threads=1 --nocapture
```

> [!NOTE]
> The Rust gRPC client requires `protoc` to be installed for compilation
> (the proto stubs are generated at build time via `tonic-build`).

### 3.4 Option C — Python REST Client

```bash
cd client-examples/python-rest-client

# Install the client library:
pip install -e ".[dev]"

# Interactive demo:
python -m okfpga_rest_client.examples.demo --url http://192.168.1.100:8088

# Run automated tests:
OKFPGA_SERVER_URL="http://192.168.1.100:8088" pytest tests/ -v --tb=short
```

### 3.5 Option D — Python gRPC Client

```bash
cd client-examples/python-grpc-client

# Install the client library:
pip install -e ".[dev]"

# Generate the gRPC proto stubs (one-time):
python generate_protos.py

# Interactive demo:
python -m okfpga_grpc_client.examples.demo --target 192.168.1.100:50051

# Run automated tests:
OKFPGA_GRPC_TARGET="192.168.1.100:50051" \
OKFPGA_SERVER_URL="http://192.168.1.100:8088" \
pytest tests/ -v --tb=short
```

---

## Part 4 — Common Workflows

### 4.1 Discover Devices

**REST (curl):**

```bash
curl http://192.168.1.100:8088/api/v1/devices | python -m json.tool
```

**Python:**

```python
from okfpga_rest_client import OkFpgaRestClient

async with OkFpgaRestClient("http://192.168.1.100:8088") as client:
    devices = await client.list_devices()
    for d in devices["devices"]:
        print(f"{d['serial']}  {d['board_model']}  {d['product_name']}")
```

### 4.2 Program an FPGA

**REST (curl):**

```bash
# Upload and program a bitfile onto a device:
curl -X POST http://192.168.1.100:8088/api/v1/devices/2237000ETR/configure \
  -F "bitfile=@Counters.bit"
```

**Python:**

```python
await client.configure_fpga("2237000ETR", "/path/to/Counters.bit")
```

### 4.3 Read Sensors

**REST (curl):**

```bash
curl http://192.168.1.100:8088/api/v1/devices/2237000ETR/sensors | python -m json.tool
```

Example response:

```json
{
  "sensors": [
    { "name": "FPGA Core Voltage", "value": 1.0, "unit": "V" },
    { "name": "FPGA Temperature", "value": 42.5, "unit": "°C" }
  ]
}
```

### 4.4 Classic Wire I/O

```bash
# Set WireIn endpoint 0x00 to value 0xFF (with mask 0xFF):
curl -X PUT http://192.168.1.100:8088/api/v1/devices/2237000ETR/classic/wires/in \
  -H "Content-Type: application/json" \
  -d '{"endpoint": 0, "value": 255, "mask": 255}'

# Update all WireIn values to the FPGA:
curl -X POST http://192.168.1.100:8088/api/v1/devices/2237000ETR/classic/wires/in/update

# Read WireOut endpoint 0x20:
curl -X POST http://192.168.1.100:8088/api/v1/devices/2237000ETR/classic/wires/out/update
curl http://192.168.1.100:8088/api/v1/devices/2237000ETR/classic/wires/out?endpoint=32
```

### 4.5 Register and Acquire a Resource

```bash
# Create a resource definition:
curl -X POST http://192.168.1.100:8088/api/v1/resources \
  -H "Content-Type: application/json" \
  -d '{
    "name": "counters",
    "device_serial": "2237000ETR",
    "access_mode": "shared",
    "description": "Classic counter endpoints from Counters.bit"
  }'

# Acquire the resource:
curl -X POST http://192.168.1.100:8088/api/v1/resources/2237000ETR:counters/acquire \
  -H "Content-Type: application/json" \
  -d '{"mode": "shared", "user_id": "alice"}'

# Lock the resource (exclusive use while locked):
curl -X POST http://192.168.1.100:8088/api/v1/resources/2237000ETR:counters/lock \
  -H "Content-Type: application/json" \
  -d '{"user_id": "alice"}'

# Unlock it:
curl -X POST http://192.168.1.100:8088/api/v1/resources/2237000ETR:counters/unlock \
  -H "Content-Type: application/json" \
  -d '{"user_id": "alice"}'

# Release the resource:
curl -X POST http://192.168.1.100:8088/api/v1/resources/2237000ETR:counters/release \
  -H "Content-Type: application/json" \
  -d '{"user_id": "alice"}'
```

### 4.6 Server-Sent Events (Real-Time Monitoring)

Subscribe to the SSE stream for live event updates:

```bash
curl -N http://192.168.1.100:8088/api/v1/events
```

Events are newline-delimited JSON:

```
data: {"type":"DeviceAdded","serial":"2237000ETR","timestamp":"2026-08-17T12:00:00Z"}
data: {"type":"ResourceAcquired","resource_id":"2237000ETR:counters","user_id":"alice"}
```

### 4.7 Management API

The management API runs on port **9090** and is always available:

```bash
# Server status:
curl http://192.168.1.100:9090/mgmt/status

# Full device info:
curl http://192.168.1.100:9090/mgmt/devices

# List services:
curl http://192.168.1.100:9090/mgmt/services

# Stop a service:
curl -X POST http://192.168.1.100:9090/mgmt/services/rest/stop

# Restart a service:
curl -X POST http://192.168.1.100:9090/mgmt/services/rest/restart

# View server config (sanitized):
curl http://192.168.1.100:9090/mgmt/config

# Graceful shutdown:
curl -X POST http://192.168.1.100:9090/mgmt/shutdown
```

---

## Part 5 — Optional Features

### 5.1 mDNS / DNS-SD Discovery

When `mdns.enabled = true`, the server advertises itself on the local network
as `_okfpga._tcp.local.`. Clients can discover the server automatically without
knowing its IP address.

**Config:**

```toml
[mdns]
enabled = true
instance_name = "okfpga-lab-bench-1"
```

**Client discovery (Python):**

```python
from okfpga_rest_client.discovery import discover_servers
servers = await discover_servers(timeout=3.0)
for s in servers:
    print(f"Found: {s.name} at {s.host}:{s.rest_port}")
```

### 5.2 MQTT Status Publishing

The server can publish device status, service events, and heartbeats to an
external MQTT broker (e.g., Mosquitto).

**Config:**

```toml
[mqtt]
enabled = true
broker_url = "mqtt://192.168.1.50:1883"
client_id = "okfpga-server-01"
topic_prefix = "okfpga/server01"
publish_interval_secs = 5
```

**Topics published:**

| Topic | Description |
|-------|-------------|
| `okfpga/server01/status` | Periodic server heartbeat |
| `okfpga/server01/devices` | Device connect/disconnect events |
| `okfpga/server01/services` | Service state changes |

### 5.3 Authentication

For multi-user environments, enable authentication:

```toml
[auth]
provider = "local"
session_expiry_secs = 28800  # 8 hours

[[auth.local.users]]
username = "admin"
password_hash = "$2b$12$..."  # bcrypt hash
display_name = "Administrator"
groups = ["admin"]

[[auth.local.users]]
username = "student1"
password_hash = "$2b$12$..."
display_name = "Student 1"
groups = ["users"]
```

---

## Part 6 — Troubleshooting

### Connection Refused

**Symptom:** `curl: (7) Failed to connect to 192.168.1.100 port 8088`

**Fix:**
1. Verify the server is running: check the console output
2. Verify the bind address is `0.0.0.0` (not `127.0.0.1`)
3. Open firewall ports: `9090`, `8088`, `50051`
4. Test from the server machine first: `curl http://localhost:8088/api/v1/devices`

### No Devices Found

**Symptom:** `{"devices": []}` even though an FPGA is connected.

**Fix:**
1. The FPGA must be connected to the **server** machine, not the client
2. Check USB3 cable is properly seated
3. Verify FrontPanel driver is installed: the FPGA should appear in Device Manager (Windows) or `lsusb` (Linux)
4. Check server logs for `discovered device:` messages
5. USB 2.0 devices are intentionally skipped — only USB3/PCIe boards are supported

### Dashboard Not Loading

**Symptom:** Browser shows a blank page or 404 at `http://<ip>:9090`.

**Fix:**
1. Verify the server was built with the `dashboard` feature: `cargo build --features dashboard`
2. The `web/` directory must exist at the server's working directory
3. Check the console for `management server started` message

### gRPC Client Can't Connect

**Symptom:** `transport error` or `failed to connect`

**Fix:**
1. Verify gRPC is enabled: `grpc.enabled = true` in config.toml
2. Verify the server was built with `--features grpc`
3. Use `http://` prefix (not `https://`): `http://192.168.1.100:50051`
4. Python gRPC target does NOT use `http://` prefix: `192.168.1.100:50051`

### Proto Compilation Fails

**Symptom:** `error: could not find protoc` during `cargo build`

**Fix:**
```bash
# Windows:
$env:PROTOC = "C:\tools\protoc\bin\protoc.exe"

# Linux:
export PROTOC=$(which protoc)

# Or install it:
# Windows: winget install Google.Protobuf
# Ubuntu:  sudo apt install protobuf-compiler
```

---

## API Quick Reference

### REST API (port 8088)

All REST endpoints are prefixed with `/api/v1/`.

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/devices` | List connected FPGAs |
| GET | `/api/v1/devices/{serial}` | Device info |
| GET | `/api/v1/devices/{serial}/sensors` | Temperature, voltage |
| GET | `/api/v1/devices/{serial}/settings` | Device settings |
| POST | `/api/v1/devices/{serial}/configure` | Program bitfile |
| GET | `/api/v1/devices/{serial}/bitfile` | Current bitfile info |
| GET | `/api/v1/devices/{serial}/dataport` | Data port type |
| GET | `/api/v1/devices/{serial}/syzygy` | SYZYGY peripherals |
| PUT | `/api/v1/devices/{serial}/classic/wires/in` | Set WireIn |
| POST | `/api/v1/devices/{serial}/classic/wires/in/update` | Push WireIns |
| GET | `/api/v1/devices/{serial}/classic/wires/out` | Read WireOut |
| POST | `/api/v1/devices/{serial}/classic/wires/out/update` | Pull WireOuts |
| POST | `/api/v1/devices/{serial}/classic/triggers/in` | Fire TriggerIn |
| GET | `/api/v1/devices/{serial}/classic/triggers/out` | Read TriggerOuts |
| POST | `/api/v1/devices/{serial}/classic/pipes/in` | Write Pipe |
| POST | `/api/v1/devices/{serial}/classic/pipes/out` | Read Pipe |
| GET | `/api/v1/devices/{serial}/classic/registers/{addr}` | Read register |
| PUT | `/api/v1/devices/{serial}/classic/registers/{addr}` | Write register |
| POST | `/api/v1/devices/{serial}/classic/reset` | Reset FPGA |
| POST | `/api/v1/devices/{serial}/axi/lite/read` | AXI Lite read |
| PUT | `/api/v1/devices/{serial}/axi/lite/write` | AXI Lite write |
| GET | `/api/v1/resources` | List resources |
| POST | `/api/v1/resources` | Create resource |
| GET | `/api/v1/resources/{id}` | Get resource |
| DELETE | `/api/v1/resources/{id}` | Delete resource |
| POST | `/api/v1/resources/{id}/acquire` | Acquire resource |
| POST | `/api/v1/resources/{id}/release` | Release resource |
| POST | `/api/v1/resources/{id}/lock` | Lock resource |
| POST | `/api/v1/resources/{id}/unlock` | Unlock resource |
| POST | `/api/v1/heartbeat` | User heartbeat |
| GET | `/api/v1/events` | SSE event stream |
| GET | `/api/v1/server/status` | Server status |
| GET | `/api/v1/server/services` | List services |
| GET | `/api/v1/users` | List users |

### Management API (port 9090)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/mgmt/status` | Server status |
| GET | `/mgmt/devices` | Connected devices |
| GET | `/mgmt/services` | Service states |
| POST | `/mgmt/services/{name}/start` | Start service |
| POST | `/mgmt/services/{name}/stop` | Stop service |
| POST | `/mgmt/services/{name}/restart` | Restart service |
| GET | `/mgmt/config` | Config (sanitized) |
| POST | `/mgmt/shutdown` | Graceful shutdown |
| WS | `/ws/events` | WebSocket events |
