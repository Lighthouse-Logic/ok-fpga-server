# ok-FPGA-Server Example Clients

Eighteen example projects demonstrating how to connect to and interact with
the ok-FPGA-Server via REST and gRPC APIs. Includes non-GUI endpoint
exercisers, interactive GUI demos, and shared common libraries.

---

## Project Overview

### Non-GUI Clients (Endpoint Exercisers)

Standalone demos that connect to the server, exercise every endpoint/RPC,
and optionally publish status to MQTT.

| Client | Language | API | Directory |
|--------|----------|-----|-----------|
| Rust REST | Rust | REST (reqwest) | `rust-rest-client/` |
| Rust gRPC | Rust | gRPC (tonic) | `rust-grpc-client/` |
| Python REST | Python 3.12+ | REST (httpx) | `python-rest-client/` |
| Python gRPC | Python 3.12+ | gRPC (grpcio) | `python-grpc-client/` |

### GUI Demos (Interactive Counters.bit Workflow)

Interactive GUI applications that walk through the full Counters.bit demo
workflow: connect → select device → program FPGA → register resource →
read/write endpoints → lock/unlock → release.

| Client | Language | API | GUI Framework | Directory |
|--------|----------|-----|---------------|-----------|
| Rust gRPC iced | Rust | gRPC (tonic) | iced | `rust-grpc-gui-iced/` |
| Rust gRPC egui | Rust | gRPC (tonic) | egui | `rust-grpc-gui-egui/` |
| Rust REST iced | Rust | REST (reqwest) | iced | `rust-rest-gui-iced/` |
| Rust REST egui | Rust | REST (reqwest) | egui | `rust-rest-gui-egui/` |
| Python gRPC tkinter | Python 3.12+ | gRPC (grpcio) | tkinter | `python-grpc-gui-tkinter/` |
| Python gRPC DearPyGui | Python 3.12+ | gRPC (grpcio) | DearPyGui | `python-grpc-gui-dearpygui/` |
| Python REST tkinter | Python 3.12+ | REST (requests) | tkinter | `python-rest-gui-tkinter/` |
| Python REST DearPyGui | Python 3.12+ | REST (requests) | DearPyGui | `python-rest-gui-dearpygui/` |

### Common Libraries

Shared client logic used by the GUI demos. Each common library provides
`DeviceInfo`, `ResourceInfo`, `CounterState` types, API methods, and SYZYGY
label formatting.

| Library | Language | API | Directory |
|---------|----------|-----|-----------|
| Rust gRPC common | Rust | gRPC | `rust-grpc-gui-common/` |
| Rust REST common | Rust | REST | `rust-rest-gui-common/` |
| Python gRPC common | Python | gRPC | `python_grpc_gui_common/` |
| Python REST common | Python | REST | `python_rest_gui_common/` |

---

## Prerequisites

1. **ok-FPGA-Server** running with all features:
   ```bash
   cargo run --features full
   ```

2. **MQTT Broker** (optional): Mosquitto at `127.0.0.1:1883`.
   If unavailable, MQTT is skipped gracefully by the non-GUI clients.

3. **mDNS**: Server must have mDNS enabled (`mdns.enabled = true` in
   `config.toml`) for zero-config discovery. Clients fall back to
   `localhost` if mDNS is unavailable.

---

## Quick Start

### Non-GUI Rust Clients

```bash
# REST client (uses mDNS discovery by default)
cargo run --manifest-path rust-rest-client/Cargo.toml

# REST client with explicit URL (bypasses mDNS)
cargo run --manifest-path rust-rest-client/Cargo.toml -- --url http://localhost:8088

# gRPC client
cargo run --manifest-path rust-grpc-client/Cargo.toml
```

### Non-GUI Python Clients

```bash
# REST client
cd python-rest-client
pip install -e ".[dev]"
python -m okfpga_rest_client.examples.demo
python -m okfpga_rest_client.examples.demo --url http://localhost:8088

# gRPC client
cd python-grpc-client
pip install -e ".[dev]"
python generate_protos.py
python -m okfpga_grpc_client.examples.demo
```

### GUI Demos — Rust (iced / egui)

```bash
# Rust gRPC GUI (iced)
cargo run --manifest-path rust-grpc-gui-iced/Cargo.toml

# Rust REST GUI (iced)
cargo run --manifest-path rust-rest-gui-iced/Cargo.toml

# Rust gRPC GUI (egui)
cargo run --manifest-path rust-grpc-gui-egui/Cargo.toml

# Rust REST GUI (egui)
cargo run --manifest-path rust-rest-gui-egui/Cargo.toml
```

### GUI Demos — Python (tkinter / DearPyGui)

```bash
# Python gRPC GUI (tkinter)
cd python-grpc-gui-tkinter
python main.py

# Python REST GUI (tkinter)
cd python-rest-gui-tkinter
python main.py

# Python gRPC GUI (DearPyGui)
cd python-grpc-gui-dearpygui
pip install dearpygui
python main.py

# Python REST GUI (DearPyGui)
cd python-rest-gui-dearpygui
pip install dearpygui
python main.py
```

---

## Architecture

### Non-GUI Clients

All four non-GUI clients follow the same pattern:

1. **mDNS Discovery** — Browse for `_okfpga._tcp.local.` to find the server
2. **API Client** — Connect via REST or gRPC and exercise all endpoints/RPCs
3. **MQTT Status** — Publish heartbeat and resource state to `okfpga-client/{client_id}/`

See [ADR-014](../docs/architecture/adr/ADR-014-example-clients.md) for full details.

### GUI Demo Dependency Graph

```
rust-grpc-gui-iced  ──┐
rust-grpc-gui-egui  ──┴── rust-grpc-gui-common ── tonic/proto
rust-rest-gui-iced  ──┐
rust-rest-gui-egui  ──┴── rust-rest-gui-common ── reqwest

python-grpc-gui-tkinter    ──┐
python-grpc-gui-dearpygui  ──┴── python_grpc_gui_common ── grpcio
python-rest-gui-tkinter    ──┐
python-rest-gui-dearpygui  ──┴── python_rest_gui_common ── requests
```

Each common library provides:
- `DeviceInfo` with SYZYGY label formatting (`display_label()`)
- `ResourceInfo` with lock state tracking (`locked_by`)
- `CounterState` with endpoint read/write helpers
- `create_resource()` with shared/exclusive registration

---

## Testing

### Unit Tests (always run, no hardware required)

```bash
# Rust common libraries (mock-based)
cargo test --manifest-path rust-grpc-gui-common/Cargo.toml --lib
cargo test --manifest-path rust-rest-gui-common/Cargo.toml --lib

# Python tests
cd python-rest-client && pytest -v && cd ..
cd python-grpc-client && pytest -v && cd ..
```

### Hardware Integration Tests

The Rust non-GUI clients include hardware integration tests that exercise
real FPGA endpoints. These tests are `#[ignore]`d by default and require:

1. The ok-FPGA-Server running with `--features full`
2. A supported FPGA connected via USB (e.g., XEM7320-A75T)
3. The Counters.bit bitfile available on disk

**⚠️ Run with `--test-threads=1`** — Hardware tests share a single physical
FPGA. Running tests in parallel causes race conditions (e.g., one test
reprograms the FPGA while another is reading endpoints). The Rust test
harness defaults to parallel execution, so you must explicitly serialize:

```bash
# Run hardware tests (single-threaded to avoid FPGA race conditions)
cargo test --manifest-path rust-rest-client/Cargo.toml \
    -- --ignored --nocapture --test-threads=1

cargo test --manifest-path rust-grpc-client/Cargo.toml \
    -- --ignored --nocapture --test-threads=1
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OKFPGA_SERVER_URL` | `http://localhost:8088` | REST API base URL for hardware tests |
| `OKFPGA_TEST_BITFILE` | `../../FrontPanel-6.1.0/ExampleBitfiles/.../Counters.bit` | Path to the Counters.bit test bitfile |
