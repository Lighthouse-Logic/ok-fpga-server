//! # User Routes
//!
//! REST endpoints for connected user sessions.
//! Implements the `/users` path from ADR-004.

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;

/// Build the `/users` route group.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/users", get(list_users))
}

/// User list response.
#[derive(Serialize)]
struct UserListBody {
    users: Vec<String>,
}

/// `GET /users` — List currently connected users.
///
/// User session tracking is implemented in Phase 9 (Authentication).
/// For now, returns an empty list.
async fn list_users() -> Json<UserListBody> {
    Json(UserListBody {
        users: vec![],
    })
}
