//! # Implicit Heartbeat Middleware
//!
//! Axum middleware that automatically updates a user's `last_seen_at`
//! timestamp on every REST API request, providing implicit liveness
//! tracking without requiring explicit heartbeat calls.
//!
//! ## How User Identity is Determined
//!
//! - When `auth.provider = "none"`: Reads the `X-User-Id` HTTP header.
//!   **It is highly recommended** to always send this header even when
//!   authentication is disabled, or liveness tracking will not function.
//! - When `auth.provider = "local"` or `"ldap"`: User identity would come
//!   from the authenticated session (future work).
//!
//! ## Behavior
//!
//! If an `X-User-Id` header is present, the middleware calls
//! `user_sessions.touch(user_id)` before passing the request to the
//! next handler. If the header is absent, the request passes through
//! unchanged (backward compatible — no error, no 401).

use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::state::AppState;

/// Header name used to identify users when `auth.provider = "none"`.
///
/// **Highly recommended** to always send this header, even without
/// authentication, so liveness tracking and auto-release can function.
pub const USER_ID_HEADER: &str = "X-User-Id";

/// Axum middleware that touches the user session on every request.
///
/// This is the "implicit heartbeat" — every REST API call that includes
/// an `X-User-Id` header automatically extends the user's liveness window.
///
/// ## Usage
///
/// ```rust,ignore
/// use axum::middleware;
///
/// let router = Router::new()
///     .nest("/api/v1", api_v1_routes())
///     .layer(middleware::from_fn_with_state(
///         state.clone(),
///         heartbeat_middleware,
///     ));
/// ```
pub async fn heartbeat_middleware(
    state: axum::extract::State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    // Extract user_id from the X-User-Id header, if present.
    // We do this before passing the request to the handler.
    if let Some(user_id) = request
        .headers()
        .get(USER_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        if !user_id.is_empty() {
            // touch() returns true if this is a brand-new session.
            let is_new = state.user_sessions.touch(user_id);

            if is_new {
                // Broadcast DeviceEvent for SSE subscribers (Phase 39, ADR-030).
                let _ = state.device_manager.broadcast(
                    ok_fpga_core::manager::DeviceEvent::UserConnected {
                        user_id: user_id.to_string(),
                        timestamp: chrono::Utc::now(),
                    },
                );

                // Broadcast user_connected event for dashboard (Phase 25-Fix-H, F9).
                state.broadcast_event(&format!(
                    r#"{{"type":"user_connected","user_id":"{}"}}"#,
                    user_id
                ));
            }
        }
    }

    // Pass the request through to the actual handler.
    next.run(request).await
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::ResourceRegistry;
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tower::ServiceExt;

    /// Build a minimal test app with the heartbeat middleware.
    fn test_app() -> (Router, Arc<AppState>) {
        let user_sessions = Arc::new(UserSessionRegistry::new());
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        let session_manager = Arc::new(ok_fpga_core::auth::SessionManager::new(3600));

        let state = Arc::new(AppState::new(
            Arc::new(DeviceManager::new()),
            Arc::new(ResourceRegistry::new()),
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            session_manager,
            user_sessions,
            crate::state::EndpointEnforcement::Off,
            crate::config::ServerConfig::default(),
            None,
        ));

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                heartbeat_middleware,
            ))
            .with_state(state.clone());

        (app, state)
    }

    #[tokio::test]
    async fn test_middleware_touches_session_with_header() {
        let (app, state) = test_app();

        assert_eq!(state.user_sessions.count(), 0);

        let request = axum::http::Request::builder()
            .uri("/test")
            .header("X-User-Id", "alice")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Alice's session should now exist
        assert_eq!(state.user_sessions.count(), 1);
        let session = state.user_sessions.get("alice").unwrap();
        assert_eq!(session.user_id, "alice");
    }

    #[tokio::test]
    async fn test_middleware_skips_without_header() {
        let (app, state) = test_app();

        let request = axum::http::Request::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // No session should be created
        assert_eq!(state.user_sessions.count(), 0);
    }

    #[tokio::test]
    async fn test_middleware_skips_empty_header() {
        let (app, state) = test_app();

        let request = axum::http::Request::builder()
            .uri("/test")
            .header("X-User-Id", "")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Empty user ID should not create a session
        assert_eq!(state.user_sessions.count(), 0);
    }
}
