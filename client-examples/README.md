# ok-FPGA-Server Example Clients

Four standalone example clients demonstrating how to connect to and interact with
the ok-FPGA-Server via REST and gRPC APIs. Each client also performs mDNS discovery
and publishes its connection status to MQTT.

## Clients

| Client | Language | API | Directory |
|--------|----------|-----|-----------|
| Rust REST | Rust | REST (reqwest) | `rust-rest-client/` |
| Rust gRPC | Rust | gRPC (tonic) | `rust-grpc-client/` |
| Python REST | Python 3.12+ | REST (httpx) | `python-rest-client/` |
| Python gRPC | Python 3.12+ | gRPC (grpcio) | `python-grpc-client/` |

## Prerequisites

1. **ok-FPGA-Server** running with all features:
   ```bash
   cd /path/to/ok-fpga-server
   cargo run --features full
   ```

2. **MQTT Broker** (optional): Mosquitto at the address configured in client config.
   If unavailable, MQTT is skipped gracefully.

3. **mDNS**: Server must have mDNS enabled (`mdns.enabled = true` in `config.toml`)
   for zero-config discovery. Clients fall back to `localhost` if mDNS is unavailable.

## Quick Start

### Rust Clients

```bash
# REST client
cd rust-rest-client
cargo run                # Interactive demo
cargo test               # Automated endpoint tests

# gRPC client
cd rust-grpc-client
cargo run                # Interactive demo
cargo test               # Automated RPC tests
```

### Python Clients

```bash
# REST client
cd python-rest-client
pip install -e ".[dev]"
python -m okfpga_rest_client.examples.demo    # Interactive demo
pytest -v                                      # Automated tests

# gRPC client
cd python-grpc-client
pip install -e ".[dev]"
python generate_protos.py                      # Generate gRPC stubs
python -m okfpga_grpc_client.examples.demo     # Interactive demo
pytest -v                                      # Automated tests
```

## Architecture

All clients follow the same pattern:

1. **mDNS Discovery** — Browse for `_okfpga._tcp.local.` to find the server
2. **API Client** — Connect via REST or gRPC and exercise all endpoints/RPCs
3. **MQTT Status** — Publish heartbeat and resource state to `okfpga-client/{client_id}/`

See [ADR-014](../docs/architecture/adr/ADR-014-example-clients.md) for full details.

## Testing

All tests require the server to be running with `--features full`. Tests that
require an MQTT broker are skipped if the broker is unreachable.

```bash
# Run all client tests (server must be running)
cd rust-rest-client && cargo test && cd ..
cd rust-grpc-client && cargo test && cd ..
cd python-rest-client && pytest -v && cd ..
cd python-grpc-client && pytest -v && cd ..
```
