//! # Server-Sent Events (SSE) Routes
//!
//! REST endpoint for streaming device and service events.
//! Implements the `/events` path from ADR-004.
//!
//! ## Event Types
//!
//! The SSE stream publishes all `DeviceEvent` variants from the
//! `DeviceManager`'s broadcast channel:
//!
//! - `DeviceAdded` — A new FPGA was connected
//! - `DeviceRemoved` — An FPGA was disconnected
//! - `DeviceProgrammed` — An FPGA was programmed with a bitfile
//! - `ResourceChanged` — A resource's state changed (acquired, released, locked, etc.)
//!
//! ## Usage
//!
//! ```bash
//! curl -N http://localhost:8088/api/v1/events
//! # Produces:
//! # event: DeviceAdded
//! # data: {"serial":"ABC123","product_name":"XEM7320-A75T",...}
//! ```

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::routing::get;
use axum::Router;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::AppState;

/// Build the `/events` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/events", get(subscribe_events))
}

/// `GET /events` — Subscribe to server-sent events (SSE).
///
/// This streams device connect/disconnect events, programming events,
/// and resource state changes as SSE events. Each event includes a
/// JSON payload in the `data` field and a named `event` type.
///
/// The stream stays open until the client disconnects. Events are
/// broadcast from the `DeviceManager`'s `tokio::sync::broadcast` channel.
async fn subscribe_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    // Subscribe to the device manager's broadcast channel.
    let rx = state.device_manager.subscribe();

    // Convert the broadcast receiver into an SSE-compatible stream.
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                // Serialize the event to JSON.
                let json = serde_json::to_string(&event).unwrap_or_default();

                // Determine the SSE event type from the enum variant name.
                let event_type = match &event {
                    ok_fpga_core::manager::DeviceEvent::DeviceAdded { .. } => "DeviceAdded",
                    ok_fpga_core::manager::DeviceEvent::DeviceRemoved { .. } => "DeviceRemoved",
                    ok_fpga_core::manager::DeviceEvent::DeviceProgrammed { .. } => {
                        "DeviceProgrammed"
                    }
                    ok_fpga_core::manager::DeviceEvent::ResourceChanged { .. } => {
                        "ResourceChanged"
                    }
                };

                let sse_event = Event::default().event(event_type).data(json);
                Some(Ok(sse_event))
            }
            Err(_) => {
                // BroadcastStream can produce lagged errors if the receiver
                // falls behind. We silently skip those.
                None
            }
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}
