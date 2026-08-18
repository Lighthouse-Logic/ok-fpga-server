# Getting Started — Rust REST GUI (egui)

**Demo name**: Rusty Rester  
**Protocol**: REST (HTTP/JSON)  
**GUI framework**: [egui](https://github.com/emilk/egui) 0.30

## Prerequisites

- Rust toolchain (1.75+): https://rustup.rs
- ok-FPGA-Server running with at least one connected FPGA
- A Counters.bit bitfile for your board

## Build & Run

```bash
cd client-examples/rust-rest-gui-egui
cargo run
```

## Default Configuration

| Setting | Default | Override |
|---------|---------|---------|
| Server URL | `http://localhost:8088` | Edit in GUI |
| Bitfile path | `FrontPanel-6.1.0/ExampleBitfiles/...` | Edit in GUI |
| User ID | `Rusty Rester` | Hardcoded in `main.rs` |

## Workflow

1. Click **Connect to Server** — discovers devices via `GET /api/v1/devices`
2. Select a device from the dropdown
3. Enter bitfile path and click **Program Bitfile**
4. Use the Counter Control Panel to interact with the FPGA

## Troubleshooting

- **"Connect failed"**: Verify the server is running at the specified URL
- **No devices in dropdown**: Check server console for connected devices
- **"Deserialization error"**: Server API may have changed — check OpenAPI spec
