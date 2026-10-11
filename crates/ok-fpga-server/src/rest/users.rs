//! # User Routes
//!
//! REST endpoints for connected user sessions.
//! Implements the `/users` path from ADR-004.
//!
//! Now backed by the [`UserSessionRegistry`](ok_fpga_core::user::UserSessionRegistry)
//! which tracks active users via heartbeat / implicit middleware.
//!
//! Since ADR-034 (P5) each entry also lists `holdings`: every resource the
//! user holds, the mode it was acquired with, and whether the user holds
//! its lock. Holdings are read from the resource registry (the source of
//! truth), not from the session record.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::state::AppState;
use crate::views::HoldingView;

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
    /// Resources held, with mode and lock flag (ADR-034 P5).
    holdings: Vec<HoldingView>,
}

/// `GET /users` — List currently connected users.
///
/// Returns all active user sessions from the `UserSessionRegistry`.
/// Each session includes the user ID, last-seen timestamp, the list of
/// held resource IDs and the richer `holdings` list.
async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Json<UserListBody> {
    let sessions = state.user_sessions.list();
    let users: Vec<UserEntry> = sessions
        .into_iter()
        .map(|s| {
            let holdings = state
                .resource_registry
                .holdings_for(&s.user_id)
                .iter()
                .map(HoldingView::from)
                .collect();
            UserEntry {
                user_id: s.user_id,
                last_seen_at: s.last_seen_at.to_rfc3339(),
                held_resource_ids: s.held_resource_ids,
                holdings,
            }
        })
        .collect();
    let count = users.len();
    Json(UserListBody { count, users })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::{AccessMode, Resource, ResourceRegistry};
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_users_holdings_include_mode_and_locked() {
        let registry = Arc::new(ResourceRegistry::new());
        registry.register(Resource::new(
            "S:r".to_string(),
            "r".to_string(),
            None,
            "S".to_string(),
            AccessMode::Shared,
        ));
        registry
            .acquire("S:r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("S:r", &"alice".to_string()).unwrap();

        let sessions = Arc::new(UserSessionRegistry::new());
        sessions.touch("alice");

        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        let state = Arc::new(AppState::new(
            Arc::new(DeviceManager::new()),
            registry,
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            Arc::new(ok_fpga_core::auth::SessionManager::new(3600)),
            sessions,
            crate::state::EndpointEnforcement::Off,
            crate::config::ServerConfig::default(),
            None,
        ));

        let response = routes()
            .with_state(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/users")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json["users"][0]["holdings"],
            serde_json::json!([{"resource_id": "S:r", "mode": "shared", "locked": true}])
        );
    }
}
