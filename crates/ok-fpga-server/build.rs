//! Build script for ok-fpga-server.
//!
//! When the `grpc` feature is enabled, this script compiles the proto3
//! service definition (`proto/okfpga/v1/okfpga.proto`) into Rust types
//! using `tonic-build`. The generated code is placed in `$OUT_DIR` and
//! included via `tonic::include_proto!()` in the gRPC module.
//!
//! When `grpc` is NOT enabled, this script does nothing — the proto file
//! is not compiled and tonic/prost are not required.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only compile protos when the gRPC feature is enabled.
    // The `CARGO_FEATURE_GRPC` env var is set by Cargo when `grpc` is active.
    #[cfg(feature = "grpc")]
    {
        // The proto file path is relative to the workspace root, not this crate.
        // We use `../..` to navigate from `crates/ok-fpga-server/` up to the
        // workspace root where `proto/` lives.
        let proto_path = "../../proto/okfpga/v1/okfpga.proto";
        let proto_dir = "../../proto";

        println!("cargo:rerun-if-changed={}", proto_path);

        tonic_build::configure()
            // Generate server-side code (service trait implementations).
            .build_server(true)
            // Don't generate client code — we're a server, not a client.
            .build_client(false)
            // Compile the proto file.
            .compile_protos(&[proto_path], &[proto_dir])?;

        println!("cargo:warning=Compiled gRPC proto definitions successfully");
    }

    // When grpc feature is not enabled, do nothing.
    #[cfg(not(feature = "grpc"))]
    {
        // Suppress unused variable warnings in the build script.
        let _ = "proto compilation skipped — grpc feature not enabled";
    }

    Ok(())
}
