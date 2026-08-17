# Contributing to ok-FPGA-Server

## Development Setup

### Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | 1.70+ | Language toolchain |
| LLVM/libclang | 14+ | `bindgen` needs libclang to parse C headers |
| protoc | 3.x | Proto compilation (only for `--features grpc`) |
| FrontPanel SDK | 6.1.0 | `okFrontPanel.dll` / `.so` for runtime |

### Windows Setup

```powershell
# Install Rust
winget install Rustlang.Rustup

# Install LLVM (for libclang)
winget install LLVM.LLVM

# Install protoc (for gRPC)
winget install Google.Protobuf

# Set environment variables
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:PROTOC = (Get-Command protoc).Source  # or set manually
```

### Ubuntu Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install build dependencies
sudo apt install build-essential libclang-dev protobuf-compiler

# Verify
rustc --version
clang --version
protoc --version
```

### FrontPanel SDK

Place the FrontPanel 6.1.0 SDK at the repository root:

```
ok-fpga-server/
├── FrontPanel-6.1.0/
│   ├── okFrontPanel.h          # C header (used by bindgen at build time)
│   ├── okFrontPanel.dll        # Windows runtime library
│   └── ExampleBitfiles/        # Optional: for hardware tests
└── ...
```

> **Note**: The SDK files are not committed to the public repository.

## Building

```bash
# Default build (REST + dashboard)
cargo build

# Full build (all features)
cargo build --features full

# Minimal build (management server only)
cargo build --no-default-features

# Release build
cargo build --release --features full
```

## Testing

```bash
# Run all unit and integration tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run a specific test
cargo test test_name

# Run hardware tests (requires connected FPGA)
cargo test -p ok-fpga-ffi --test hardware -- --ignored --test-threads=1 --nocapture
```

### Test Categories

| Category | Location | Requires |
|----------|----------|----------|
| Unit tests | `src/**/*.rs` (inline `#[cfg(test)]`) | Nothing |
| Core integration | `crates/ok-fpga-core/tests/` | Nothing |
| Server integration | `crates/ok-fpga-server/tests/` | Nothing |
| Hardware tests | `crates/ok-fpga-ffi/tests/hardware.rs` | Connected FPGA board |

Hardware tests use `#[ignore]` and are skipped by default. They require:
- A connected Opal Kelly USB3/Gen3 FPGA board
- `okFrontPanel.dll` accessible
- `--test-threads=1` (FrontPanel C API is not thread-safe)

## Code Quality

```bash
# Format code
cargo fmt --all

# Lint
cargo clippy --features full --all-targets

# Generate docs
cargo doc --no-deps --open
```

## Project Structure

```
ok-fpga-server/
├── crates/
│   ├── ok-fpga-ffi/        # Layer 1: FFI bindings (all unsafe confined here)
│   │   ├── src/
│   │   │   ├── api.rs       # FrontPanelDll dispatch table (~160 function pointers)
│   │   │   ├── error.rs     # FFI error types
│   │   │   ├── handles.rs   # RAII handle wrappers
│   │   │   └── lib.rs       # Module root + bindgen include
│   │   ├── tests/
│   │   │   └── hardware.rs  # Real hardware integration tests
│   │   └── build.rs         # bindgen configuration
│   │
│   ├── ok-fpga-core/        # Layer 2: Safe domain layer
│   │   └── src/
│   │       ├── auth.rs       # Auth providers + sessions
│   │       ├── device.rs     # DeviceInfo, board enums
│   │       ├── manager.rs    # DeviceManager (hot-plug)
│   │       ├── resource.rs   # ResourceRegistry (acquire/release)
│   │       ├── sensors.rs    # DeviceSensor types
│   │       ├── service.rs    # ManagedService trait
│   │       └── service_manager.rs  # ServiceManager
│   │
│   └── ok-fpga-server/      # Layer 3: Binary crate
│       └── src/
│           ├── main.rs        # Entry point
│           ├── config.rs      # TOML config loading
│           ├── state.rs       # AppState (shared across handlers)
│           ├── management.rs  # Management server (port 9090)
│           ├── rest/          # REST API handlers
│           ├── grpc/          # gRPC service implementations
│           ├── mqtt.rs        # MQTT publisher
│           └── mdns.rs        # mDNS advertiser
│
├── proto/okfpga/v1/          # gRPC proto definitions
├── api/                       # OpenAPI spec
├── dashboard/                 # Web dashboard assets
├── docs/                      # Architecture docs & ADRs
│   └── architecture/
│       ├── overview.md
│       └── adr/               # Architecture Decision Records
└── FrontPanel-6.1.0/          # SDK (not committed)
```

## Architecture Decision Records

When making significant design decisions, document them as ADRs in `docs/architecture/adr/`. Use the existing ADRs as templates. ADRs are numbered sequentially (ADR-001, ADR-002, etc.).

## Commit Style

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add SYZYGY port discovery
fix: handle device disconnect during configuration
docs: update ADR-009 with SYZYGY key naming
test: add hardware integration tests for XEM7320
refactor: extract sensor type conversion to module
```
