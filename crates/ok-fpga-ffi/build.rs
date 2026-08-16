//! # Build Script for ok-fpga-ffi
//!
//! This build script will use `rust-bindgen` to parse the FrontPanel 6.1.0
//! C header file and generate Rust type declarations. For now it is a
//! placeholder that will be implemented in Phase 3A of the development roadmap.
//!
//! When fully implemented, it will:
//! 1. Point bindgen at `FrontPanel-6.1.0/okFrontPanel.h`
//! 2. Define `okNO_LUA` and `OPALKELLY_NO_CXX_TYPES` to exclude unwanted APIs
//! 3. Blocklist USB 2.0-only functions (WriteI2C, ReadI2C)
//! 4. Generate output to `$OUT_DIR/okfrontpanel_bindings.rs`

fn main() {
    // Phase 3A will add the bindgen invocation here.
    // For now, the build script is a no-op so the crate compiles.
    println!("cargo:rerun-if-changed=build.rs");
}
