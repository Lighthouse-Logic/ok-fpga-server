# Getting Started — Rust gRPC GUI (iced)

**Demo name**: Icy Garpac  
**Protocol**: gRPC (HTTP/2 + Protocol Buffers)  
**GUI framework**: [iced](https://iced.rs) 0.14

## Prerequisites

- Rust toolchain (1.75+): https://rustup.rs
- `protoc` (Protocol Buffer compiler): https://grpc.io/docs/protoc-installation/
- ok-FPGA-Server running with gRPC enabled (default port 50051)
- A Counters.bit bitfile for your board

## Build & Run

```bash
cd client-examples/rust-grpc-gui-iced
cargo run
```

> **Note**: The build step auto-compiles the `.proto` file via `tonic-build`.
> Ensure `protoc` is in your PATH.

## Default Configuration

| Setting | Default | Override |
|---------|---------|---------|
| gRPC address | `http://localhost:50051` | Edit in GUI |
| Bitfile path | `FrontPanel-6.1.0/ExampleBitfiles/...` | Edit in GUI |
| User ID | `Icy Garpac` | Hardcoded in `main.rs` |

## Workflow

1. Click **Connect to Server** — discovers devices via `ListDevices` RPC
2. Select a device from the dropdown
3. Enter bitfile path and click **Program Bitfile**
4. Use the Counter Control Panel to interact with the FPGA

## Troubleshooting

- **"Connect failed"**: Verify gRPC is enabled in server config and port is correct
- **"Unimplemented" on Program**: See Phase 25-Fix-E — gRPC ConfigureFPGA wiring
