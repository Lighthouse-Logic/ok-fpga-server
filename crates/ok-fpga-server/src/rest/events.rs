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
                let sse_event = Event::default()
                    .event(event.name())
                    .data(event_data_json(&event));
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

/// The JSON for an SSE `data:` line: the variant's own fields.
///
/// `DeviceEvent` uses serde's default *externally tagged* enum layout, so
/// `serde_json` writes `{"ResourceChanged":{"resource_id":...}}`. The
/// OpenAPI spec says `data` is the inner object (the event name already
/// travels in the SSE `event:` line), so we unwrap the single-key wrapper.
fn event_data_json(event: &ok_fpga_core::manager::DeviceEvent) -> String {
    let value = serde_json::to_value(event).unwrap_or(serde_json::Value::Null);
    let inner = match value {
        serde_json::Value::Object(map) if map.len() == 1 => {
            map.into_iter().next().map(|(_, v)| v).unwrap_or_default()
        }
        other => other,
    };
    inner.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ok_fpga_core::manager::{DeleteReason, DeviceEvent};

    #[test]
    fn test_event_data_json_resource_changed_is_unwrapped() {
        let event = DeviceEvent::ResourceChanged {
            resource_id: "S:r".to_string(),
            new_state: "in_use_locked".to_string(),
            timestamp: chrono::Utc::now(),
        };
        let json: serde_json::Value = serde_json::from_str(&event_data_json(&event)).unwrap();
        assert_eq!(json["resource_id"], "S:r");
        assert_eq!(json["new_state"], "in_use_locked");
        assert!(json.get("ResourceChanged").is_none());
        assert_eq!(event.name(), "ResourceChanged");
    }

    #[test]
    fn test_event_data_json_resource_deleted_includes_reason() {
        let event = DeviceEvent::ResourceDeleted {
            resource_id: "S:r".to_string(),
            reason: DeleteReason::Client,
            timestamp: chrono::Utc::now(),
        };
        let json: serde_json::Value = serde_json::from_str(&event_data_json(&event)).unwrap();
        assert_eq!(json["reason"], "client");
    }
}
