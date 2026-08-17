//! # User Routes
//!
//! REST endpoints for connected user sessions.
//! Implements the `/users` path from ADR-004.
//!
//! Now backed by the [`UserSessionRegistry`](ok_fpga_core::user::UserSessionRegistry)
//! which tracks active users via heartbeat / implicit middleware.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

/// Build the `/users` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/users", get(list_users))
}

/// User list response.
#[derive(Serialize)]
struct UserListBody {
    /// Total number of active user sessions.
    count: usize,
    /// Active user sessions.
    users: Vec<UserEntry>,
}

/// A single user session entry returned by `GET /users`.
#[derive(Serialize)]
struct UserEntry {
    /// User identifier.
    user_id: String,
    /// ISO 8601 timestamp of last activity.
    last_seen_at: String,
    /// Resource IDs currently held by this user.
    held_resource_ids: Vec<String>,
}

/// `GET /users` — List currently connected users.
///
/// Returns all active user sessions from the `UserSessionRegistry`.
/// Each session includes the user ID, last-seen timestamp, and
/// a list of held resource IDs.
async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Json<UserListBody> {
    let sessions = state.user_sessions.list();
    let users: Vec<UserEntry> = sessions
        .into_iter()
        .map(|s| UserEntry {
            user_id: s.user_id,
            last_seen_at: s.last_seen_at.to_rfc3339(),
            held_resource_ids: s.held_resource_ids,
        })
        .collect();
    let count = users.len();
    Json(UserListBody { count, users })
}
