//! # Build Script for ok-fpga-server
//!
//! This build script will use `tonic-build` to compile the proto3 gRPC
//! service definition into Rust server/client code. For now it is a
//! placeholder that will be implemented in Phase 5 of the development roadmap.
//!
//! When fully implemented, it will:
//! 1. Compile `proto/okfpga/v1/okfpga.proto`
//! 2. Generate Rust types for all proto messages
//! 3. Generate Rust trait implementations for all gRPC services

fn main() {
    // Phase 5 will add the tonic-build invocation here.
    // For now, the build script is a no-op so the crate compiles.
    println!("cargo:rerun-if-changed=build.rs");
}
