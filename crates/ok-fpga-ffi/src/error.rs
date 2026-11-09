// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # FFI Error Types
//!
//! This module defines the error types used throughout the FFI layer.
//! It wraps the raw `ok_ErrorCode` C enum into Rust-friendly error types
//! with human-readable messages.
//!
//! ## Error Hierarchy
//!
//! - [`FfiError`](crate::error::FfiError) — The main error type for FFI operations. It wraps either
//!   a FrontPanel error code or a DLL loading error.
//! - [`LoadError`](crate::error::LoadError) — Errors that can occur when loading the FrontPanel DLL.

use std::fmt;
use std::path::PathBuf;

use crate::ok_ErrorCode;

// ============================================================================
// FrontPanel Error Code Wrapper
// ============================================================================

/// A Rust-friendly representation of an `ok_ErrorCode` returned by FrontPanel
/// C API functions.
///
/// ## How to Use
///
/// Most FrontPanel C functions return an `ok_ErrorCode`. The [`FfiError::from_code`]
/// method converts these into a `Result`:
///
/// ```ignore
/// let code = unsafe { (api.some_function)(handle) };
/// FfiError::from_code(code)?;
/// ```
///
/// If the code is `ok_NoError` (0), the result is `Ok(())`.
/// Otherwise, a descriptive `Err(FfiError)` is returned.
#[derive(Debug, Clone)]
pub enum FfiError {
    /// The FrontPanel DLL returned an error code.
    /// The inner value is the raw `ok_ErrorCode` integer and a human-readable
    /// description of what that error means.
    FrontPanelError {
        /// The raw error code from the C API (negative integer).
        code: i32,
        /// Human-readable description of the error.
        message: String,
    },

    /// The FrontPanel DLL could not be loaded.
    LoadError(LoadError),

    /// A C string returned by the DLL was not valid UTF-8.
    /// This should never happen with Opal Kelly hardware but we handle
    /// it defensively.
    InvalidUtf8 {
        /// Which field or function produced the invalid string.
        context: String,
    },

    /// A null pointer was returned where a valid handle was expected.
    NullHandle {
        /// Which function returned the null handle.
        function: String,
    },
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FfiError::FrontPanelError { code, message } => {
                write!(f, "FrontPanel error {}: {}", code, message)
            }
            FfiError::LoadError(e) => write!(f, "DLL load error: {}", e),
            FfiError::InvalidUtf8 { context } => {
                write!(f, "Invalid UTF-8 in {}", context)
            }
            FfiError::NullHandle { function } => {
                write!(f, "Null handle returned by {}", function)
            }
        }
    }
}

impl std::error::Error for FfiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FfiError::LoadError(e) => Some(e),
            _ => None,
        }
    }
}

impl FfiError {
    /// Convert a raw `ok_ErrorCode` into a `Result`.
    ///
    /// Returns `Ok(())` if the code is `ok_NoError` (value 0).
    /// Returns `Err(FfiError::FrontPanelError { ... })` for any other value.
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let code = ok_ErrorCode::ok_NoError;
    /// assert!(FfiError::from_code(code).is_ok());
    ///
    /// let code = ok_ErrorCode::ok_DeviceNotOpen;
    /// assert!(FfiError::from_code(code).is_err());
    /// ```
    pub fn from_code(code: ok_ErrorCode) -> Result<(), Self> {
        if matches!(code, ok_ErrorCode::ok_NoError) {
            Ok(())
        } else {
            Err(FfiError::FrontPanelError {
                code: code as i32,
                message: error_code_to_string(code),
            })
        }
    }

    /// Convert a raw `ok_ErrorCode` into a `Result<T>`, returning the
    /// provided value on success.
    ///
    /// This is a convenience method for functions that return both an
    /// error code and a value.
    pub fn from_code_with<T>(code: ok_ErrorCode, value: T) -> Result<T, Self> {
        FfiError::from_code(code).map(|_| value)
    }
}

impl From<LoadError> for FfiError {
    fn from(e: LoadError) -> Self {
        FfiError::LoadError(e)
    }
}

// ============================================================================
// DLL Loading Error
// ============================================================================

/// Errors that occur when loading the FrontPanel shared library at runtime.
///
/// These errors are non-recoverable — if the DLL can't be loaded, the server
/// cannot communicate with any FPGA hardware. However, the server can still
/// start and report a "DLL not found" status via the REST/gRPC API.
#[derive(Debug, Clone)]
pub enum LoadError {
    /// The shared library file was not found at the expected path.
    LibraryNotFound {
        /// The path that was attempted.
        path: PathBuf,
        /// The OS error message.
        message: String,
    },

    /// The shared library was found but a required symbol could not be resolved.
    /// This usually means the DLL version is too old and doesn't export a
    /// function we need.
    SymbolNotFound {
        /// The name of the missing symbol (e.g., "okFrontPanel_Construct").
        symbol: String,
        /// The OS error message.
        message: String,
    },

    /// The DLL was loaded but reports an API version older than 6.1.0.
    /// We only support FrontPanel 6.1.0 and later.
    VersionMismatch {
        /// The version reported by the loaded DLL.
        actual: String,
        /// The minimum required version.
        required: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::LibraryNotFound { path, message } => {
                write!(f, "Library not found at '{}': {}", path.display(), message)
            }
            LoadError::SymbolNotFound { symbol, message } => {
                write!(f, "Symbol '{}' not found: {}", symbol, message)
            }
            LoadError::VersionMismatch { actual, required } => {
                write!(
                    f,
                    "API version mismatch: found {}, required >= {}",
                    actual, required
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

// ============================================================================
// Error Code Description Mapping
// ============================================================================

/// Convert an `ok_ErrorCode` enum variant into a human-readable description.
///
/// These descriptions are derived from the FrontPanel 6.1.0 documentation
/// and the C header comments.
fn error_code_to_string(code: ok_ErrorCode) -> String {
    // Allow unreachable_patterns: the generated enum is #[non_exhaustive]
    // so the catch-all arm is needed for forward compatibility, even though
    // the compiler currently knows all variants.
    #[allow(unreachable_patterns)]
    match code {
        ok_ErrorCode::ok_NoError => "No error".to_string(),
        ok_ErrorCode::ok_Failed => "Operation failed".to_string(),
        ok_ErrorCode::ok_Timeout => "Operation timed out".to_string(),
        ok_ErrorCode::ok_DoneNotHigh => "FPGA DONE signal not high after configuration".to_string(),
        ok_ErrorCode::ok_TransferError => "Data transfer error".to_string(),
        ok_ErrorCode::ok_CommunicationError => "Communication error with device".to_string(),
        ok_ErrorCode::ok_InvalidBitstream => "Invalid bitstream file".to_string(),
        ok_ErrorCode::ok_FileError => "File I/O error".to_string(),
        ok_ErrorCode::ok_DeviceNotOpen => "Device is not open — call Open() first".to_string(),
        ok_ErrorCode::ok_InvalidEndpoint => "Invalid endpoint address".to_string(),
        ok_ErrorCode::ok_InvalidBlockSize => {
            "Invalid block size for block pipe transfer".to_string()
        }
        ok_ErrorCode::ok_I2CRestrictedAddress => {
            "I2C address is restricted (USB 2.0 only)".to_string()
        }
        ok_ErrorCode::ok_I2CBitError => "I2C bit error".to_string(),
        ok_ErrorCode::ok_I2CNack => "I2C NACK received".to_string(),
        ok_ErrorCode::ok_I2CUnknownStatus => "I2C unknown status".to_string(),
        ok_ErrorCode::ok_UnsupportedFeature => "Feature not supported by this device".to_string(),
        ok_ErrorCode::ok_FIFOUnderflow => "FIFO underflow".to_string(),
        ok_ErrorCode::ok_FIFOOverflow => "FIFO overflow".to_string(),
        ok_ErrorCode::ok_DataAlignmentError => {
            "Data alignment error — check buffer size and alignment".to_string()
        }
        ok_ErrorCode::ok_InvalidResetProfile => "Invalid FPGA reset profile".to_string(),
        ok_ErrorCode::ok_InvalidParameter => "Invalid parameter passed to function".to_string(),
        ok_ErrorCode::ok_AXISlaveError => "AXI slave error".to_string(),
        ok_ErrorCode::ok_AXIDecodeError => "AXI decode error — invalid address".to_string(),
        ok_ErrorCode::ok_GatewareTimeout => "Gateware timeout — FPGA did not respond".to_string(),
        ok_ErrorCode::ok_EntitlementFailed => "Entitlement check failed".to_string(),
        ok_ErrorCode::ok_FrontPanelNotEnabled => {
            "FrontPanel is not enabled in the FPGA bitfile".to_string()
        }
        // Catch-all for any future error codes added to the API.
        // The #[non_exhaustive] attribute on the enum requires this.
        _ => format!("Unknown error code: {}", code as i32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that ok_NoError produces Ok(()).
    #[test]
    fn from_code_no_error_is_ok() {
        let result = FfiError::from_code(ok_ErrorCode::ok_NoError);
        assert!(result.is_ok());
    }

    /// Verify that error codes produce Err with correct code.
    #[test]
    fn from_code_error_produces_err() {
        let result = FfiError::from_code(ok_ErrorCode::ok_DeviceNotOpen);
        assert!(result.is_err());

        if let Err(FfiError::FrontPanelError { code, message }) = result {
            assert!(code < 0, "Error codes should be negative");
            assert!(
                message.contains("not open"),
                "Message should describe the error: {}",
                message
            );
        } else {
            panic!("Expected FrontPanelError variant");
        }
    }

    /// Verify that all known error codes have descriptive messages
    /// (not the "Unknown error code" fallback).
    #[test]
    fn all_known_codes_have_descriptions() {
        let codes = [
            ok_ErrorCode::ok_Failed,
            ok_ErrorCode::ok_Timeout,
            ok_ErrorCode::ok_DeviceNotOpen,
            ok_ErrorCode::ok_InvalidEndpoint,
            ok_ErrorCode::ok_UnsupportedFeature,
            ok_ErrorCode::ok_FrontPanelNotEnabled,
        ];

        for code in &codes {
            let desc = error_code_to_string(*code);
            assert!(
                !desc.starts_with("Unknown"),
                "Error code {:?} should have a description, got: {}",
                code,
                desc
            );
        }
    }

    /// Verify Display trait formatting.
    #[test]
    fn error_display_formatting() {
        let err = FfiError::FrontPanelError {
            code: -8,
            message: "Device is not open".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("-8"));
        assert!(display.contains("not open"));
    }
}
