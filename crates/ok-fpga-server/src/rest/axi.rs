//! # AXI Data Port Routes
//!
//! REST endpoints for AXI data port operations (Lite, Full, Stream).
//! Implements the `/devices/{serial}/axi/*` paths from ADR-004.
//! All endpoints require FFI integration and return 501 until complete.

use std::sync::Arc;

use axum::extract::Path;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/devices/{serial}/axi/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // AXI Lite
        .route("/devices/{serial}/axi/lite/read", post(axi_lite_read))
        .route("/devices/{serial}/axi/lite/write", post(axi_lite_write))
        .route("/devices/{serial}/axi/lite/reset", post(axi_lite_reset))
        // AXI Full
        .route("/devices/{serial}/axi/full/widths", get(axi_full_widths))
        .route("/devices/{serial}/axi/full/read", post(axi_full_read))
        .route("/devices/{serial}/axi/full/write", post(axi_full_write))
        .route("/devices/{serial}/axi/full/reset", post(axi_full_reset))
        // AXI Stream
        .route("/devices/{serial}/axi/stream/widths", get(axi_stream_widths))
        .route("/devices/{serial}/axi/stream/read", post(axi_stream_read))
        .route("/devices/{serial}/axi/stream/write", post(axi_stream_write))
        .route("/devices/{serial}/axi/stream/reset", post(axi_stream_reset))
}

// All AXI data port operations require FFI integration.

async fn axi_lite_read(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXILiteRead requires FFI integration"))
}

async fn axi_lite_write(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXILiteWrite requires FFI integration"))
}

async fn axi_lite_reset(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXILiteReset requires FFI integration"))
}

async fn axi_full_widths(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIFullGetDatapathWidths requires FFI integration"))
}

async fn axi_full_read(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIFullRead requires FFI integration"))
}

async fn axi_full_write(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIFullWrite requires FFI integration"))
}

async fn axi_full_reset(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIFullReset requires FFI integration"))
}

async fn axi_stream_widths(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIStreamGetDatapathWidths requires FFI integration"))
}

async fn axi_stream_read(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIStreamRead requires FFI integration"))
}

async fn axi_stream_write(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIStreamWrite requires FFI integration"))
}

async fn axi_stream_reset(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("AXIStreamReset requires FFI integration"))
}
