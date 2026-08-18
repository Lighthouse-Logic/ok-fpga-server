//! # Dashboard Log Broadcast Layer (ADR-022)
//!
//! This module provides a custom `tracing_subscriber::Layer` that captures
//! tracing events and broadcasts them to WebSocket clients via a
//! `tokio::sync::broadcast` channel.
//!
//! ## Architecture
//!
//! ```text
//! tracing events ──► WebSocketBroadcastLayer ──► broadcast::Sender<LogEvent>
//!                                                        │
//!                                         ┌──────────────┼──────────────┐
//!                                         ▼              ▼              ▼
//!                                    WS Client 1    WS Client 2    WS Client N
//!                                    (filter:INFO)  (filter:DEBUG) (filter:TRACE)
//! ```
//!
//! The layer captures events at **all** levels (TRACE and above). Per-client
//! filtering is done at send time in the WebSocket handler, not here. This
//! avoids needing to reconfigure the global tracing subscriber when a
//! dashboard user changes their desired log level.
//!
//! ## Ring Buffer
//!
//! A fixed-capacity ring buffer stores the last `RING_BUFFER_CAPACITY`
//! events. When a new WebSocket client connects, it receives the buffered
//! entries (filtered by its requested level) as backfill before switching
//! to live streaming.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::Level;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

// ============================================================================
// Constants
// ============================================================================

/// Broadcast channel capacity — how many unread events a slow subscriber
/// can fall behind before it receives a `Lagged` error.
const BROADCAST_CHANNEL_CAPACITY: usize = 512;

/// Ring buffer capacity — how many recent log events are kept for backfill
/// when a new WebSocket client connects.
const RING_BUFFER_CAPACITY: usize = 200;

// ============================================================================
// LogEvent
// ============================================================================

/// A structured log event captured from the `tracing` subscriber.
///
/// This is the unit of data sent over the WebSocket to dashboard clients.
/// It is serialized to JSON matching the format specified in ADR-022 §1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// ISO 8601 timestamp of the event.
    pub timestamp: String,

    /// Log level as a string: "TRACE", "DEBUG", "INFO", "WARN", "ERROR".
    pub level: String,

    /// The tracing target (e.g., `ok_fpga_server::handle_pool`).
    pub target: String,

    /// Human-readable log message.
    pub message: String,
}

impl LogEvent {
    /// Convert this event to the WebSocket JSON format expected by the
    /// dashboard's `handleWsEvent()` function.
    pub fn to_ws_json(&self) -> String {
        serde_json::json!({
            "type": "log",
            "timestamp": self.timestamp,
            "level": self.level,
            "target": self.target,
            "message": self.message,
        })
        .to_string()
    }

    /// Parse a tracing `Level` from the string representation.
    /// Returns the numeric priority for comparison (lower = more verbose).
    pub fn level_priority(&self) -> u8 {
        match self.level.as_str() {
            "TRACE" => 0,
            "DEBUG" => 1,
            "INFO" => 2,
            "WARN" => 3,
            "ERROR" => 4,
            _ => 2, // default to INFO
        }
    }
}

/// Convert a log level string to a numeric priority for filtering.
/// Lower = more verbose. Returns None for invalid level strings.
pub fn level_priority_from_str(level: &str) -> Option<u8> {
    match level.to_uppercase().as_str() {
        "TRACE" => Some(0),
        "DEBUG" => Some(1),
        "INFO" => Some(2),
        "WARN" => Some(3),
        "ERROR" => Some(4),
        _ => None,
    }
}

// ============================================================================
// Field Visitor
// ============================================================================

/// Visitor that extracts the `message` field from a tracing event.
struct MessageVisitor {
    message: String,
}

impl MessageVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
            // Remove surrounding quotes if present.
            if self.message.starts_with('"') && self.message.ends_with('"') {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        } else if self.message.is_empty() {
            // If no "message" field, use the first field as the message.
            self.message = format!("{} = {:?}", field.name(), value);
        } else {
            // Append additional fields.
            self.message
                .push_str(&format!(", {} = {:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else if self.message.is_empty() {
            self.message = format!("{} = {}", field.name(), value);
        } else {
            self.message
                .push_str(&format!(", {} = {}", field.name(), value));
        }
    }
}

// ============================================================================
// Ring Buffer
// ============================================================================

/// Thread-safe ring buffer for recent log events.
///
/// Stores the last `RING_BUFFER_CAPACITY` events for backfill when new
/// WebSocket clients connect.
#[derive(Debug)]
pub struct LogRingBuffer {
    buffer: RwLock<VecDeque<LogEvent>>,
}

impl LogRingBuffer {
    /// Create a new empty ring buffer.
    pub fn new() -> Self {
        Self {
            buffer: RwLock::new(VecDeque::with_capacity(RING_BUFFER_CAPACITY)),
        }
    }

    /// Push a log event into the ring buffer.
    ///
    /// If the buffer is full, the oldest event is discarded.
    pub fn push(&self, event: LogEvent) {
        if let Ok(mut buf) = self.buffer.write() {
            if buf.len() >= RING_BUFFER_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(event);
        }
    }

    /// Get all events in the buffer that match the given minimum level priority.
    ///
    /// Events are returned in chronological order (oldest first).
    pub fn get_filtered(&self, min_priority: u8) -> Vec<LogEvent> {
        if let Ok(buf) = self.buffer.read() {
            buf.iter()
                .filter(|e| e.level_priority() >= min_priority)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for LogRingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// WebSocketBroadcastLayer
// ============================================================================

/// A `tracing_subscriber::Layer` that broadcasts log events to WebSocket
/// clients via a `tokio::sync::broadcast` channel.
///
/// This layer captures **all** events (no level filtering). Per-client
/// filtering is done downstream in the WebSocket handler.
pub struct WebSocketBroadcastLayer {
    /// Broadcast sender for log events.
    tx: broadcast::Sender<LogEvent>,

    /// Ring buffer for backfill.
    ring_buffer: Arc<LogRingBuffer>,
}

impl WebSocketBroadcastLayer {
    /// Create a new broadcast layer and return the layer plus handles
    /// needed by the WebSocket handler.
    ///
    /// Returns `(layer, sender, ring_buffer)`:
    /// - `layer` — install this in the tracing subscriber stack
    /// - `sender` — pass to `MgmtState` for WebSocket handlers to subscribe
    /// - `ring_buffer` — pass to `MgmtState` for backfill on new connections
    pub fn new() -> (Self, broadcast::Sender<LogEvent>, Arc<LogRingBuffer>) {
        let (tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        let ring_buffer = Arc::new(LogRingBuffer::new());

        let layer = Self {
            tx: tx.clone(),
            ring_buffer: ring_buffer.clone(),
        };

        (layer, tx, ring_buffer)
    }
}

impl<S> Layer<S> for WebSocketBroadcastLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = *metadata.level();
        let target = metadata.target().to_string();

        // Extract the message from the event fields.
        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        let log_event = LogEvent {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: level_str(level).to_string(),
            target,
            message: visitor.message,
        };

        // Push to ring buffer (for backfill).
        self.ring_buffer.push(log_event.clone());

        // Broadcast to all subscribers (non-blocking, ignores errors
        // when no subscribers are listening).
        let _ = self.tx.send(log_event);
    }
}

/// Convert a tracing `Level` to its string representation.
fn level_str(level: Level) -> &'static str {
    match level {
        Level::TRACE => "TRACE",
        Level::DEBUG => "DEBUG",
        Level::INFO => "INFO",
        Level::WARN => "WARN",
        Level::ERROR => "ERROR",
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_event_to_ws_json_has_correct_type() {
        let event = LogEvent {
            timestamp: "2026-08-18T07:00:00.000Z".to_string(),
            level: "INFO".to_string(),
            target: "test::module".to_string(),
            message: "hello world".to_string(),
        };
        let json = event.to_ws_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "log");
        assert_eq!(parsed["level"], "INFO");
        assert_eq!(parsed["message"], "hello world");
    }

    #[test]
    fn level_priority_ordering() {
        let trace = LogEvent {
            timestamp: String::new(),
            level: "TRACE".into(),
            target: String::new(),
            message: String::new(),
        };
        let info = LogEvent {
            timestamp: String::new(),
            level: "INFO".into(),
            target: String::new(),
            message: String::new(),
        };
        let error = LogEvent {
            timestamp: String::new(),
            level: "ERROR".into(),
            target: String::new(),
            message: String::new(),
        };
        assert!(trace.level_priority() < info.level_priority());
        assert!(info.level_priority() < error.level_priority());
    }

    #[test]
    fn level_priority_from_str_valid() {
        assert_eq!(level_priority_from_str("trace"), Some(0));
        assert_eq!(level_priority_from_str("INFO"), Some(2));
        assert_eq!(level_priority_from_str("Error"), Some(4));
    }

    #[test]
    fn level_priority_from_str_invalid() {
        assert_eq!(level_priority_from_str("verbose"), None);
        assert_eq!(level_priority_from_str(""), None);
    }

    #[test]
    fn ring_buffer_caps_at_capacity() {
        let buf = LogRingBuffer::new();
        for i in 0..250 {
            buf.push(LogEvent {
                timestamp: String::new(),
                level: "INFO".into(),
                target: String::new(),
                message: format!("msg {}", i),
            });
        }
        let all = buf.get_filtered(0);
        assert_eq!(all.len(), RING_BUFFER_CAPACITY);
        // Oldest should be msg 50 (first 50 were evicted).
        assert_eq!(all[0].message, "msg 50");
    }

    #[test]
    fn ring_buffer_filters_by_level() {
        let buf = LogRingBuffer::new();
        buf.push(LogEvent {
            timestamp: String::new(),
            level: "TRACE".into(),
            target: String::new(),
            message: "trace msg".into(),
        });
        buf.push(LogEvent {
            timestamp: String::new(),
            level: "INFO".into(),
            target: String::new(),
            message: "info msg".into(),
        });
        buf.push(LogEvent {
            timestamp: String::new(),
            level: "ERROR".into(),
            target: String::new(),
            message: "error msg".into(),
        });

        // Filter at INFO level (priority 2) — should get INFO + ERROR.
        let filtered = buf.get_filtered(2);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].message, "info msg");
        assert_eq!(filtered[1].message, "error msg");
    }
}
