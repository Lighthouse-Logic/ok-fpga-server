//! # Access Enforcement Middleware (ADR-024)
//!
//! Lightweight Axum middleware that enforces user identity and device-level
//! access control on ALL `/api/v1/*` routes. This middleware is **independent**
//! from the heartbeat middleware — they are separate, parallel concerns.
//!
//! ## Three Enforcement Tiers
//!
//! Routes are classified into three tiers based on their path:
//!
//! - **Tier 1 — Management routes** (no `{serial}`):
//!   `/server/*`, `/devices` (list), `/resources/*`, `/users/*`, `/events/*`,
//!   `/disconnect`, `/heartbeat`.
//!   When enforcement is active (`warn` or `enforce`), only **user identity**
//!   is enforced (`X-User-Id` must be present). No restrictions on which
//!   management operations the user may perform.
//!
//! - **Tier 2 — Device-level routes** (have `{serial}`, but not data port):
//!   `/devices/{serial}` (info), `/devices/{serial}/configure`, etc.
//!   Same as Tier 1: user identity only, no resource check.
//!
//! - **Tier 3 — Data port routes** (have `{serial}` + Classic or AXI path):
//!   `/devices/{serial}/classic/**`, `/devices/{serial}/axi/**`.
//!   User identity + **device-level gating** via `check_device_access()`.
//!   Detailed endpoint-level checks are done inside each handler.
//!
//! ## Design Rationale
//!
//! This middleware does NOT parse request bodies or query params — that is the
//! handler's job. The middleware only does identity validation and device-level
//! gating (a fast, cheap check). This keeps the critical path between Request
//! and Handler minimal (ADR-024 §4, Design Principle 1).

use std::sync::Arc;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::warn;

use crate::rest::errors::ProblemDetail;
use crate::rest::heartbeat_middleware::USER_ID_HEADER;
use crate::state::{AppState, EndpointEnforcement};

/// Axum middleware that enforces user identity and device-level access control.
///
/// Applied to ALL `/api/v1/*` routes. See module-level docs for tier details.
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
///         access_enforcement_middleware,
///     ));
/// ```
pub async fn access_enforcement_middleware(
    state: axum::extract::State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    // Fast path: if enforcement is off, pass through immediately.
    // No checks of any kind — current v1 behavior.
    if !state.enforcement.is_active() {
        return next.run(request).await;
    }

    // Extract user identity from the X-User-Id header.
    let user_id = request
        .headers()
        .get(USER_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Determine the route tier from the URL path.
    let path = request.uri().path().to_string();
    let tier = classify_route(&path);

    match tier {
        RouteTier::Management | RouteTier::DeviceLevel => {
            // Tier 1 + 2: "Warn" and "Enforce" enable user identity enforcement
            // only — no restrictions on which management operations the user
            // may perform. Any authenticated user may call any management or
            // device-level endpoint.
            if user_id.is_none() {
                match state.enforcement {
                    EndpointEnforcement::Warn => {
                        warn!(
                            path = %path,
                            "Access enforcement (warn): X-User-Id header missing"
                        );
                        // Warn mode: log but allow the request through.
                    }
                    EndpointEnforcement::Enforce => {
                        return ProblemDetail::new(
                            StatusCode::FORBIDDEN,
                            "User identity required: X-User-Id header must be present when endpoint enforcement is active",
                        )
                        .into_response();
                    }
                    EndpointEnforcement::Off => unreachable!(),
                }
            }
        }
        RouteTier::DataPort { serial } => {
            // Tier 3: User identity + device-level gating.
            if user_id.is_none() {
                match state.enforcement {
                    EndpointEnforcement::Warn => {
                        warn!(
                            path = %path,
                            "Access enforcement (warn): X-User-Id header missing on data port route"
                        );
                        // Warn mode: log but allow the request through.
                    }
                    EndpointEnforcement::Enforce => {
                        return ProblemDetail::new(
                            StatusCode::FORBIDDEN,
                            "User identity required: X-User-Id header must be present when endpoint enforcement is active",
                        )
                        .into_response();
                    }
                    EndpointEnforcement::Off => unreachable!(),
                }
            } else {
                let uid = user_id.as_ref().unwrap();

                // Device-level check: does the user hold ANY resource on this device?
                if let Err(denied) =
                    state.resource_registry.check_device_access(uid, &serial)
                {
                    match state.enforcement {
                        EndpointEnforcement::Warn => {
                            warn!(
                                user_id = %uid,
                                serial = %serial,
                                path = %path,
                                "Access enforcement (warn): {}",
                                denied
                            );
                            // Warn mode: log but allow the request through.
                        }
                        EndpointEnforcement::Enforce => {
                            return ProblemDetail::new(
                                StatusCode::FORBIDDEN,
                                denied.to_string(),
                            )
                            .into_response();
                        }
                        EndpointEnforcement::Off => unreachable!(),
                    }
                }
            }
        }
    }

    // All checks passed (or warn mode allowed through) — proceed to handler.
    next.run(request).await
}

// ============================================================================
// Route Classification
// ============================================================================

/// Which enforcement tier a route belongs to.
#[derive(Debug)]
enum RouteTier {
    /// Tier 1: Management routes — no `{serial}` in the path.
    Management,
    /// Tier 2: Device-level routes — have `{serial}` but not Classic/AXI.
    DeviceLevel,
    /// Tier 3: Data port routes — have `{serial}` + Classic or AXI path.
    DataPort { serial: String },
}

/// Classify a URL path into an enforcement tier.
///
/// Examines the path segments after `/api/v1/` to determine whether this is
/// a management route, device-level route, or data port route.
///
/// ## Path patterns:
///
/// - `/api/v1/server/*` → Management
/// - `/api/v1/devices` → Management (list all devices)
/// - `/api/v1/resources/*` → Management
/// - `/api/v1/users/*` → Management
/// - `/api/v1/events/*` → Management
/// - `/api/v1/disconnect` → Management
/// - `/api/v1/heartbeat` → Management
/// - `/api/v1/devices/{serial}` → DeviceLevel
/// - `/api/v1/devices/{serial}/classic/*` → DataPort
/// - `/api/v1/devices/{serial}/axi/*` → DataPort
/// - `/api/v1/devices/{serial}/*` (other) → DeviceLevel
fn classify_route(path: &str) -> RouteTier {
    // Strip the /api/v1/ prefix to work with the remaining path segments.
    let stripped = path
        .strip_prefix("/api/v1/")
        .unwrap_or(path.strip_prefix("/api/v1").unwrap_or(path));

    let segments: Vec<&str> = stripped.split('/').filter(|s| !s.is_empty()).collect();

    match segments.first().copied() {
        Some("devices") if segments.len() >= 3 => {
            // /devices/{serial}/... — check what comes after the serial.
            let serial = segments[1].to_string();
            match segments.get(2).copied() {
                Some("classic") | Some("axi") => RouteTier::DataPort { serial },
                _ => RouteTier::DeviceLevel,
            }
        }
        Some("devices") if segments.len() == 2 => {
            // /devices/{serial} — device info (no sub-path).
            RouteTier::DeviceLevel
        }
        _ => {
            // Everything else: /server/*, /devices (list), /resources/*,
            // /users/*, /events/*, /disconnect, /heartbeat, etc.
            RouteTier::Management
        }
    }
}



// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use ok_fpga_core::manager::DeviceManager;
    use ok_fpga_core::resource::{AccessMode, ResourceRegistry, Resource};
    use ok_fpga_core::service_manager::ServiceManager;
    use ok_fpga_core::user::UserSessionRegistry;
    use tower::ServiceExt;

    /// Build a test app with the access enforcement middleware at the given level.
    fn test_app(enforcement: EndpointEnforcement) -> (Router, Arc<AppState>) {
        let resource_registry = Arc::new(ResourceRegistry::new());
        let user_sessions = Arc::new(UserSessionRegistry::new());
        let auth_provider: Arc<dyn ok_fpga_core::auth::AuthProvider> =
            Arc::from(ok_fpga_core::auth::create_provider("none", vec![], None));
        let session_manager = Arc::new(ok_fpga_core::auth::SessionManager::new(3600));

        let state = Arc::new(AppState::new(
            Arc::new(DeviceManager::new()),
            resource_registry,
            Arc::new(ServiceManager::new()),
            None,
            auth_provider,
            session_manager,
            user_sessions,
            enforcement,
        ));

        // Create a minimal router with representative routes for each tier.
        let app = Router::new()
            // Tier 1: Management
            .route("/api/v1/server/status", get(|| async { "ok" }))
            .route("/api/v1/resources", get(|| async { "resources" }))
            // Tier 2: Device-level
            .route("/api/v1/devices/{serial}", get(|| async { "device_info" }))
            // Tier 3: Data port
            .route(
                "/api/v1/devices/{serial}/classic/wires/out",
                get(|| async { "wire_out" }),
            )
            .route(
                "/api/v1/devices/{serial}/axi/read",
                get(|| async { "axi_read" }),
            )
            .layer(middleware::from_fn_with_state(
                state.clone(),
                access_enforcement_middleware,
            ))
            .with_state(state.clone());

        (app, state)
    }

    // --- Off mode ---

    #[tokio::test]
    async fn off_mode_passes_through_without_header() {
        let (app, _state) = test_app(EndpointEnforcement::Off);

        let request = axum::http::Request::builder()
            .uri("/api/v1/devices/SN001/classic/wires/out")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- Warn mode ---

    #[tokio::test]
    async fn warn_mode_tier1_allows_with_header() {
        let (app, _state) = test_app(EndpointEnforcement::Warn);

        let request = axum::http::Request::builder()
            .uri("/api/v1/server/status")
            .header("X-User-Id", "alice")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- Enforce mode ---

    #[tokio::test]
    async fn enforce_mode_tier1_no_user_id_returns_403() {
        let (app, _state) = test_app(EndpointEnforcement::Enforce);

        let request = axum::http::Request::builder()
            .uri("/api/v1/server/status")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn enforce_mode_tier1_with_user_id_allows() {
        let (app, _state) = test_app(EndpointEnforcement::Enforce);

        let request = axum::http::Request::builder()
            .uri("/api/v1/server/status")
            .header("X-User-Id", "alice")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn enforce_mode_tier2_no_user_id_returns_403() {
        let (app, _state) = test_app(EndpointEnforcement::Enforce);

        let request = axum::http::Request::builder()
            .uri("/api/v1/devices/SN001")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn enforce_mode_tier3_no_user_id_returns_403() {
        let (app, _state) = test_app(EndpointEnforcement::Enforce);

        let request = axum::http::Request::builder()
            .uri("/api/v1/devices/SN001/classic/wires/out")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn enforce_mode_tier3_no_resource_returns_403() {
        let (app, _state) = test_app(EndpointEnforcement::Enforce);

        // Alice has a user_id but no resource on SN001.
        let request = axum::http::Request::builder()
            .uri("/api/v1/devices/SN001/classic/wires/out")
            .header("X-User-Id", "alice")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn enforce_mode_tier3_with_resource_allows() {
        let (app, state) = test_app(EndpointEnforcement::Enforce);

        // Register a resource on SN001 and have alice acquire it.
        let resource = Resource::new(
            "r1".to_string(),
            "test_resource".to_string(),
            None,
            "SN001".to_string(),
            AccessMode::Shared,
        );
        state.resource_registry.register(resource);
        state
            .resource_registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        let request = axum::http::Request::builder()
            .uri("/api/v1/devices/SN001/classic/wires/out")
            .header("X-User-Id", "alice")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // --- Route classification ---

    #[test]
    fn classify_management_routes() {
        assert!(matches!(
            classify_route("/api/v1/server/status"),
            RouteTier::Management
        ));
        assert!(matches!(
            classify_route("/api/v1/resources"),
            RouteTier::Management
        ));
        assert!(matches!(
            classify_route("/api/v1/users"),
            RouteTier::Management
        ));
        assert!(matches!(
            classify_route("/api/v1/events/stream"),
            RouteTier::Management
        ));
        assert!(matches!(
            classify_route("/api/v1/devices"),
            RouteTier::Management
        ));
    }

    #[test]
    fn classify_device_level_routes() {
        assert!(matches!(
            classify_route("/api/v1/devices/SN001"),
            RouteTier::DeviceLevel
        ));
        assert!(matches!(
            classify_route("/api/v1/devices/SN001/configure"),
            RouteTier::DeviceLevel
        ));
        assert!(matches!(
            classify_route("/api/v1/devices/SN001/sensors"),
            RouteTier::DeviceLevel
        ));
    }

    #[test]
    fn classify_data_port_routes() {
        match classify_route("/api/v1/devices/SN001/classic/wires/out") {
            RouteTier::DataPort { serial } => assert_eq!(serial, "SN001"),
            other => panic!("Expected DataPort, got {:?}", other),
        }
        match classify_route("/api/v1/devices/SN001/axi/read") {
            RouteTier::DataPort { serial } => assert_eq!(serial, "SN001"),
            other => panic!("Expected DataPort, got {:?}", other),
        }
    }
}
