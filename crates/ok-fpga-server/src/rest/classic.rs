//! # Classic Data Port Routes
//!
//! REST endpoints for Classic data port operations (wire, trigger, pipe,
//! register). Implements the `/devices/{serial}/classic/*` paths from ADR-004.
//! All endpoints require FFI integration and return 501 until complete.

use std::sync::Arc;

use axum::extract::Path;
use axum::routing::{get, post, put};
use axum::{Json, Router};

use crate::rest::errors::ProblemDetail;
use crate::state::AppState;

/// Build the `/devices/{serial}/classic/*` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        // Wires
        .route("/devices/{serial}/classic/wires/in", put(set_wire_in))
        .route("/devices/{serial}/classic/wires/in/update", post(update_wire_ins))
        .route("/devices/{serial}/classic/wires/out/update", post(update_wire_outs))
        .route("/devices/{serial}/classic/wires/out/{address}", get(get_wire_out))
        // Triggers
        .route("/devices/{serial}/classic/triggers/in", post(activate_trigger_in))
        .route("/devices/{serial}/classic/triggers/out/update", post(update_trigger_outs))
        .route("/devices/{serial}/classic/triggers/out/{address}", get(is_triggered))
        // Pipes
        .route("/devices/{serial}/classic/pipes/in", post(write_to_pipe_in))
        .route("/devices/{serial}/classic/pipes/out", post(read_from_pipe_out))
        .route("/devices/{serial}/classic/pipes/block-in", post(write_to_block_pipe_in))
        .route("/devices/{serial}/classic/pipes/block-out", post(read_from_block_pipe_out))
        // Registers
        .route("/devices/{serial}/classic/registers/read", post(read_register))
        .route("/devices/{serial}/classic/registers/write", post(write_register))
        // Reset
        .route("/devices/{serial}/classic/reset", post(reset_fpga))
}

// All Classic data port operations require FFI integration.
// Each returns 501 Not Implemented with a ProblemDetail body.

async fn set_wire_in(Path((_serial,)): Path<(String,)>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("SetWireIn requires FFI integration"))
}

async fn update_wire_ins(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("UpdateWireIns requires FFI integration"))
}

async fn update_wire_outs(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("UpdateWireOuts requires FFI integration"))
}

async fn get_wire_out(Path((_serial, _address)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("GetWireOutValue requires FFI integration"))
}

async fn activate_trigger_in(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("ActivateTriggerIn requires FFI integration"))
}

async fn update_trigger_outs(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("UpdateTriggerOuts requires FFI integration"))
}

async fn is_triggered(Path((_serial, _address)): Path<(String, String)>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("IsTriggered requires FFI integration"))
}

async fn write_to_pipe_in(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("WriteToPipeIn requires FFI integration"))
}

async fn read_from_pipe_out(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("ReadFromPipeOut requires FFI integration"))
}

async fn write_to_block_pipe_in(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("WriteToBlockPipeIn requires FFI integration"))
}

async fn read_from_block_pipe_out(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("ReadFromBlockPipeOut requires FFI integration"))
}

async fn read_register(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("ReadRegister requires FFI integration"))
}

async fn write_register(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("WriteRegister requires FFI integration"))
}

async fn reset_fpga(Path(_serial): Path<String>) -> Result<Json<serde_json::Value>, ProblemDetail> {
    Err(ProblemDetail::not_implemented("ResetFPGA requires FFI integration"))
}
