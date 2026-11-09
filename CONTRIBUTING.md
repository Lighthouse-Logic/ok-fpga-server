# Contributing to `ok-fpga-server`

Thank you for your interest in contributing to **`ok-fpga-server`**, maintained by **Lighthouse Logic** (a registered trade name of **Great Bay Synergy LLC**).

---

## 1. Dual-Licensing & Contributor License Agreement (CLA)

### Why We Require a CLA
**`ok-fpga-server`** is published under a **Dual-Licensing Model**:
- The **Community Edition** is free and open-source under the **GNU General Public License v3.0 (`GPLv3`)** (see [`LICENSE`](./LICENSE)).
- **Great Bay Synergy LLC d/b/a Lighthouse Logic** also offers commercial, closed-source licensing options to fund ongoing FPGA/embedded systems R&D and enable commercial hardware/software integration (see [`COMMERCIAL-LICENSE.md`](./COMMERCIAL-LICENSE.md)).

To maintain this dual-licensing model while allowing **you to retain full copyright ownership** of your work, all contributors must agree to our **Contributor License Agreement ([`CLA.md`](./CLA.md))** before a Pull Request can be merged.

### How to Sign the CLA (1-Minute Automated PR Comment)
We use a self-contained GitHub Action (`contributor-assistant/github-action`)—**no third-party OAuth website login is required**:

1. Open your Pull Request on `Lighthouse-Logic/ok-fpga-server` as normal.
2. The automated CLA bot will post a comment on your Pull Request asking you to review [`CLA.md`](./CLA.md).
3. Once you have read and agreed to [`CLA.md`](./CLA.md), post a new comment on the Pull Request with the exact text:
   ```text
   I have read the CLA Document and I hereby sign the CLA
   ```
4. The workflow will automatically record your GitHub username and signature in `.github/cla-signatures.json` and mark the CLA status check as **passed**. You only need to sign once across all Pull Requests in this repository.

---

## 2. Pull Request & Commit Hygiene

1. **Git Author Privacy & Identity**:
   - Everything committed to a public Git repository becomes part of the permanent public commit log. Before committing, verify your local `git config user.name` and `git config user.email` match the public identity or GitHub `noreply` email you wish to publish.
2. **SPDX & Copyright Headers**:
   - Preserve existing SPDX license identifiers (`SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial`) and copyright notices in source files.
3. **Focused Changes**:
   - Keep each Pull Request scoped to a single bug fix, feature, or documentation improvement.
4. **No Third-Party Copyleft Contamination**:
   - Do not copy code from third-party GPL-only or proprietary sources unless you are the sole copyright holder and can grant the rights specified in [`CLA.md`](./CLA.md).
5. **Testing**:
   - Include or update unit/integration tests alongside code changes and ensure all existing tests pass before submitting your PR.

---

## 3. Commercial Licensing or Private Inquiries

If your organization would like to contribute under a corporate agreement, sponsor a feature, or obtain a commercial license for **`ok-fpga-server`**, please contact us at [admin@lighthouselogic.dev](mailto:admin@lighthouselogic.dev) or visit [https://lighthouselogic.dev](https://lighthouselogic.dev).

---

## 4. Development Setup

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

Place the FrontPanel 6.1.0 SDK at the repository root (or set `OK_FRONTPANEL_SDK_DIR`):

```
ok-fpga-server/
├── FrontPanel-6.1.0/
│   ├── okFrontPanel.h          # C header (used by bindgen at build time)
│   ├── okFrontPanel.dll        # Windows runtime library (or libokFrontPanel.so on Linux)
│   └── ExampleBitfiles/        # Optional: for hardware tests
└── ...
```

> **Note**: The proprietary Opal Kelly SDK files are not committed to the public repository. See [`FrontPanel-6.1.0/README.md`](FrontPanel-6.1.0/README.md) for setup instructions.

---

## 5. Building

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

---

## 6. Testing

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
- `okFrontPanel.dll` / `libokFrontPanel.so` accessible
- `--test-threads=1` (FrontPanel C API is not thread-safe)

---

## 7. Code Quality

```bash
# Format code
cargo fmt --all

# Lint
cargo clippy --features full --all-targets

# Generate docs
cargo doc --no-deps --open
```

---

## 8. Project Structure

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
└── FrontPanel-6.1.0/          # SDK instructions (proprietary binaries not committed)
```

---

## 9. Architecture Decision Records & Commit Style

When making significant design decisions, document them as ADRs in `docs/architecture/adr/`. Use the existing ADRs as templates. ADRs are numbered sequentially (`ADR-001`, `ADR-002`, etc.).

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add SYZYGY port discovery
fix: handle device disconnect during configuration
docs: update ADR-009 with SYZYGY key naming
test: add hardware integration tests for XEM7320
refactor: extract sensor type conversion to module
```
