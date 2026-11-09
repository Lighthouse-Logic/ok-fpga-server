// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Build Script for ok-fpga-ffi
//!
//! This build script uses `rust-bindgen` to parse the FrontPanel 6.1.0
//! C header file (`okFrontPanel.h`) and auto-generate Rust FFI type
//! declarations at compile time.
//!
//! ## What It Does
//!
//! 1. Tells bindgen to parse `FrontPanel-6.1.0/okFrontPanel.h`
//! 2. Defines preprocessor macros to exclude unwanted APIs:
//!    - `okNO_LUA` — excludes the Lua scripting engine (okBuffer, okScriptValue, etc.)
//!    - `OPALKELLY_NO_CXX_TYPES` — excludes C++ class wrappers (we only use the C API)
//! 3. Blocklists specific C functions we don't want bindings for:
//!    - `okFrontPanel_WriteI2C` / `ReadI2C` — USB 2.0 only
//!    - `okFrontPanel_ConstructFromFD` — Android only
//! 4. Writes generated bindings to `$OUT_DIR/okfrontpanel_bindings.rs`
//!
//! ## Why We Use Bindgen
//!
//! Bindgen reads the C header and creates Rust `type`, `struct`, `enum`, and
//! `extern "C" fn` declarations that exactly match the C ABI. This saves us
//! from manually writing hundreds of FFI declarations and keeps them in sync
//! with the actual header file.

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell Cargo to re-run this build script if the header or SDK env var changes.
    // This ensures that if Opal Kelly releases a new header, our bindings
    // are automatically regenerated on the next build.
    println!("cargo:rerun-if-env-changed=OK_FRONTPANEL_SDK_DIR");
    println!("cargo:rerun-if-changed=../../FrontPanel-6.1.0/okFrontPanel.h");
    println!("cargo:rerun-if-changed=build.rs");

    // Resolve the path to `okFrontPanel.h`.
    // Priority:
    // 1. `OK_FRONTPANEL_SDK_DIR` environment variable (`<dir>/okFrontPanel.h` or `<dir>/API/include/okFrontPanel.h`)
    // 2. `<workspace_root>/FrontPanel-6.1.0/okFrontPanel.h`
    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by Cargo");
    let header_path = if let Ok(sdk_dir) = env::var("OK_FRONTPANEL_SDK_DIR") {
        let base = PathBuf::from(&sdk_dir);
        let direct = base.join("okFrontPanel.h");
        let nested = base.join("API").join("include").join("okFrontPanel.h");
        if direct.exists() {
            println!("cargo:rerun-if-changed={}", direct.display());
            direct
        } else if nested.exists() {
            println!("cargo:rerun-if-changed={}", nested.display());
            nested
        } else {
            direct
        }
    } else {
        PathBuf::from(&manifest_dir).join("../../FrontPanel-6.1.0/okFrontPanel.h")
    };

    // Verify the header file exists — give a clear error message if not.
    if !header_path.exists() {
        panic!(
            "FrontPanel header not found at: {}\n\
             Copy okFrontPanel.h from the Opal Kelly FrontPanel 6.1.0 SDK into FrontPanel-6.1.0/\n\
             or set OK_FRONTPANEL_SDK_DIR=/path/to/FrontPanel-6.1.0 (see FrontPanel-6.1.0/README.md).",
            header_path.display()
        );
    }

    // Configure bindgen to parse the FrontPanel C header.
    //
    // Key decisions:
    // - We define `okNO_LUA` so the Lua scripting types (okBuffer_HANDLE,
    //   okScriptValue_HANDLE, etc.) are excluded at the preprocessor level.
    // - We define `OPALKELLY_NO_CXX_TYPES` so the C++ class wrappers
    //   (okCFrontPanel, okCPLL22150, etc.) are excluded. We only want the
    //   C API functions (okFrontPanel_Construct, okFrontPanel_Destruct, etc.)
    // - We use `default_enum_style(EnumVariation::Rust)` to generate proper
    //   Rust enums instead of integer constants, which gives us exhaustive
    //   pattern matching.
    let bindings = bindgen::Builder::default()
        // The header file to parse.
        .header(
            header_path
                .to_str()
                .expect("Header path must be valid UTF-8"),
        )
        // Preprocessor defines to exclude unwanted sections of the header.
        //
        // okNO_LUA: Prevents the Lua scripting API from being parsed.
        // This excludes okBuffer_HANDLE, okScriptValue_HANDLE,
        // okScriptValues_HANDLE, okScriptEngine_HANDLE, and all
        // associated functions.
        .clang_arg("-DokNO_LUA")
        //
        // OPALKELLY_NO_CXX_TYPES: Prevents C++ class wrappers from being
        // parsed. Lines 1328-3132 of the header define C++ classes
        // (okCFrontPanel, okCPLL22150, etc.) that we don't need — we use
        // the C function API directly.
        .clang_arg("-DOPALKELLY_NO_CXX_TYPES")
        // --- Blocklisted functions ---
        //
        // These functions exist in the C API but are excluded from our
        // bindings because they are either USB 2.0-only, Android-only,
        // or not needed for the server.
        // USB 2.0 only — not supported by this project (ADR-001).
        .blocklist_function("okFrontPanel_WriteI2C")
        .blocklist_function("okFrontPanel_ReadI2C")
        // Android only — constructs from a file descriptor.
        .blocklist_function("okFrontPanel_ConstructFromFD")
        // --- Bindgen output configuration ---
        //
        // Generate Rust enums with proper variants instead of integer
        // constants. This lets us use `match` exhaustively.
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: true,
        })
        // Derive common traits on generated types so we can debug-print
        // them, compare them, and copy them around easily.
        .derive_debug(true)
        .derive_default(true)
        .derive_copy(true)
        // Use `core` types instead of `std` types where possible.
        // This makes the generated code work in no_std environments,
        // though we don't strictly need it — it's just good practice.
        .use_core()
        // Parse comments from the C header and include them as Rust
        // doc comments on the generated types. This makes the generated
        // code self-documenting when viewed with `cargo doc`.
        .generate_comments(true)
        // Allow only the types and functions with the "ok" prefix to
        // be included. This filters out any system types pulled in by
        // included system headers (like windows.h).
        .allowlist_function("ok.*")
        .allowlist_type("ok.*")
        .allowlist_var("ok.*")
        .allowlist_var("OK_.*")
        .allowlist_var("MAX_.*")
        .allowlist_var("TRUE")
        .allowlist_var("FALSE")
        .allowlist_type("Bool")
        // Generate the bindings. This invokes libclang to parse the
        // header and produce Rust source code.
        .generate()
        .expect("Failed to generate bindings from okFrontPanel.h");

    // Write the generated bindings to $OUT_DIR/okfrontpanel_bindings.rs.
    //
    // $OUT_DIR is a directory that Cargo creates for build script output.
    // It is unique per crate and per build profile (debug/release).
    // The generated file will be included in lib.rs via the include! macro.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("okfrontpanel_bindings.rs"))
        .expect("Failed to write bindings to $OUT_DIR/okfrontpanel_bindings.rs");

    println!("cargo:warning=Generated FrontPanel 6.1.0 bindings successfully");
}
