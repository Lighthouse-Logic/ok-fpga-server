//! # User Session Tracking
//!
//! This module tracks active users and their last activity timestamps.
//! It is the foundation for the liveness / heartbeat system described in
//! ADR-020 §5.
//!
//! ## How It Works
//!
//! 1. Every time a user makes a request (REST or gRPC), the implicit heartbeat
//!    middleware calls [`UserSessionRegistry::touch`] to update `last_seen_at`.
//! 2. Explicit heartbeat endpoints (`POST /heartbeat`, gRPC `Ping`) also
//!    call `touch()`.
//! 3. A background sweep task (Phase 21C) periodically scans for users
//!    whose `last_seen_at` is older than the configured timeout and
//!    auto-releases their held resources.
//!
//! ## Thread Safety
//!
//! [`UserSessionRegistry`] is protected by an `RwLock` and is safe to share
//! across async tasks via `Arc<UserSessionRegistry>`.
//!
//! ## Important Note on `X-User-Id`
//!
//! When `auth.provider = "none"`, the implicit heartbeat middleware identifies
//! users via the `X-User-Id` HTTP header (or `x-user-id` gRPC metadata).
//! **It is highly recommended** to always send this header even when
//! authentication is disabled, or liveness tracking (and auto-release of
//! expired resources) will not function.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// UserSession
// ============================================================================

/// Represents an active user's session on the server.
///
/// A session is created automatically when a user first interacts with
/// the server (via any REST or gRPC call that includes a user identity).
/// The `last_seen_at` timestamp is updated on every interaction.
///
/// ## Fields
///
/// - `user_id` — The unique identifier for this user. When `auth.provider = "none"`,
///   this comes from the `X-User-Id` header. When auth is enabled, it comes
///   from the authenticated session.
/// - `last_seen_at` — The last time this user was "seen" (made a request or
///   sent a heartbeat). Used by the sweep task to detect expired users.
/// - `held_resource_ids` — IDs of resources currently held (acquired) by this
///   user. Populated by cross-referencing the `ResourceRegistry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    /// Unique user identifier.
    pub user_id: String,

    /// Timestamp of last user activity (heartbeat, API call, etc.).
    pub last_seen_at: DateTime<Utc>,

    /// Resource IDs currently held by this user.
    /// This is a convenience snapshot — the canonical source of truth is
    /// the `ResourceRegistry`.
    pub held_resource_ids: Vec<String>,
}

impl UserSession {
    /// Create a new session with `last_seen_at` set to now.
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            last_seen_at: Utc::now(),
            held_resource_ids: Vec::new(),
        }
    }
}

// ============================================================================
// UserSessionRegistry
// ============================================================================

/// Thread-safe registry of active user sessions.
///
/// This is the central data structure for user liveness tracking.
/// It supports concurrent reads and exclusive writes via `RwLock`.
///
/// ## Example
///
/// ```rust
/// use ok_fpga_core::user::UserSessionRegistry;
/// use std::time::Duration;
///
/// let registry = UserSessionRegistry::new();
///
/// // User makes a request — touch their session
/// registry.touch("alice");
///
/// // Check if the user exists
/// assert!(registry.get("alice").is_some());
///
/// // List all active sessions
/// let sessions = registry.list();
/// assert_eq!(sessions.len(), 1);
///
/// // Find expired users (none yet, since we just touched)
/// let expired = registry.expired(Duration::from_secs(60));
/// assert!(expired.is_empty());
/// ```
pub struct UserSessionRegistry {
    /// Map of user_id → UserSession, protected by a reader-writer lock.
    ///
    /// - Read lock: `get()`, `list()`, `expired()` — multiple readers OK
    /// - Write lock: `touch()`, `remove()` — exclusive access
    sessions: RwLock<HashMap<String, UserSession>>,
}

impl UserSessionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Create or update a user's session timestamp.
    ///
    /// If the user already exists, their `last_seen_at` is updated to `Utc::now()`.
    /// If they don't exist, a new session is created.
    ///
    /// Returns `true` if a **new** session was created, `false` if an existing
    /// session was refreshed. This allows callers (e.g., the heartbeat middleware)
    /// to broadcast a `UserConnected` event only on first contact.
    ///
    /// This is the primary "heartbeat" mechanism — every API call that
    /// identifies a user should call this.
    pub fn touch(&self, user_id: &str) -> bool {
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        let is_new = !sessions.contains_key(user_id);
        let entry = sessions
            .entry(user_id.to_string())
            .or_insert_with(|| UserSession::new(user_id));
        entry.last_seen_at = Utc::now();
        is_new
    }

    /// Get a snapshot of a specific user's session.
    ///
    /// Returns `None` if the user has no active session.
    pub fn get(&self, user_id: &str) -> Option<UserSession> {
        let sessions = self.sessions.read().expect("session lock poisoned");
        sessions.get(user_id).cloned()
    }

    /// List all active user sessions.
    ///
    /// Returns a snapshot — the returned `Vec` is not live-updated.
    pub fn list(&self) -> Vec<UserSession> {
        let sessions = self.sessions.read().expect("session lock poisoned");
        sessions.values().cloned().collect()
    }

    /// Remove a user's session from the registry.
    ///
    /// Called by the sweep task after auto-releasing an expired user's
    /// resources, or when a user explicitly disconnects.
    pub fn remove(&self, user_id: &str) {
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        sessions.remove(user_id);
    }

    /// Find all sessions whose `last_seen_at` is older than `timeout`.
    ///
    /// Used by the sweep task to detect users who have gone silent
    /// (no heartbeat, no API calls) for longer than the configured timeout.
    ///
    /// ## Arguments
    ///
    /// * `timeout` — Duration of inactivity before a session is considered expired.
    ///   For example, `Duration::from_secs(60)` = expire after 60 seconds of silence.
    pub fn expired(&self, timeout: Duration) -> Vec<UserSession> {
        let sessions = self.sessions.read().expect("session lock poisoned");
        let cutoff = Utc::now() - chrono::Duration::from_std(timeout).unwrap_or_default();
        sessions
            .values()
            .filter(|s| s.last_seen_at < cutoff)
            .cloned()
            .collect()
    }

    /// Return the number of active sessions.
    pub fn count(&self) -> usize {
        let sessions = self.sessions.read().expect("session lock poisoned");
        sessions.len()
    }

    /// Update the `held_resource_ids` for a user.
    ///
    /// Called after a resource acquire or release to keep the session's
    /// resource list in sync with the `ResourceRegistry`.
    pub fn update_held_resources(&self, user_id: &str, resource_ids: Vec<String>) {
        let mut sessions = self.sessions.write().expect("session lock poisoned");
        if let Some(session) = sessions.get_mut(user_id) {
            session.held_resource_ids = resource_ids;
        }
    }
}

impl Default for UserSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_user_session_new_sets_timestamp() {
        let session = UserSession::new("alice");
        assert_eq!(session.user_id, "alice");
        assert!(session.held_resource_ids.is_empty());
        // Timestamp should be very close to now (within 1 second)
        let elapsed = Utc::now() - session.last_seen_at;
        assert!(elapsed.num_seconds() < 1);
    }

    #[test]
    fn test_registry_touch_creates_new_session() {
        let registry = UserSessionRegistry::new();
        assert_eq!(registry.count(), 0);

        let is_new = registry.touch("alice");
        assert!(is_new, "touch() should return true for a new session");
        assert_eq!(registry.count(), 1);

        let session = registry.get("alice").unwrap();
        assert_eq!(session.user_id, "alice");
    }

    #[test]
    fn test_registry_touch_updates_existing_session() {
        let registry = UserSessionRegistry::new();
        let is_new = registry.touch("alice");
        assert!(is_new);
        let first_seen = registry.get("alice").unwrap().last_seen_at;

        // Sleep briefly so the timestamp differs
        thread::sleep(Duration::from_millis(10));
        let is_new_again = registry.touch("alice");
        assert!(!is_new_again, "touch() should return false for existing session");
        let second_seen = registry.get("alice").unwrap().last_seen_at;

        assert!(second_seen >= first_seen);
        // Still only one session
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_registry_list_returns_all_sessions() {
        let registry = UserSessionRegistry::new();
        registry.touch("alice");
        registry.touch("bob");
        registry.touch("carol");

        let sessions = registry.list();
        assert_eq!(sessions.len(), 3);

        let names: Vec<String> = sessions.iter().map(|s| s.user_id.clone()).collect();
        assert!(names.contains(&"alice".to_string()));
        assert!(names.contains(&"bob".to_string()));
        assert!(names.contains(&"carol".to_string()));
    }

    #[test]
    fn test_registry_remove_deletes_session() {
        let registry = UserSessionRegistry::new();
        registry.touch("alice");
        registry.touch("bob");
        assert_eq!(registry.count(), 2);

        registry.remove("alice");
        assert_eq!(registry.count(), 1);
        assert!(registry.get("alice").is_none());
        assert!(registry.get("bob").is_some());
    }

    #[test]
    fn test_registry_remove_nonexistent_is_noop() {
        let registry = UserSessionRegistry::new();
        registry.remove("nobody"); // Should not panic
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_expired_returns_stale_sessions() {
        let registry = UserSessionRegistry::new();
        registry.touch("alice");

        // Manually backdate alice's session to 2 minutes ago
        {
            let mut sessions = registry.sessions.write().unwrap();
            if let Some(session) = sessions.get_mut("alice") {
                session.last_seen_at = Utc::now() - chrono::Duration::seconds(120);
            }
        }

        // Touch bob right now (fresh)
        registry.touch("bob");

        // Expire with 60s timeout — alice should be expired, bob should not
        let expired = registry.expired(Duration::from_secs(60));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].user_id, "alice");
    }

    #[test]
    fn test_registry_expired_with_no_stale_sessions() {
        let registry = UserSessionRegistry::new();
        registry.touch("alice");
        registry.touch("bob");

        // Nobody is expired with a 60s timeout
        let expired = registry.expired(Duration::from_secs(60));
        assert!(expired.is_empty());
    }

    #[test]
    fn test_registry_update_held_resources() {
        let registry = UserSessionRegistry::new();
        registry.touch("alice");

        // Initially no held resources
        let session = registry.get("alice").unwrap();
        assert!(session.held_resource_ids.is_empty());

        // Update held resources
        registry.update_held_resources(
            "alice",
            vec!["res-1".to_string(), "res-2".to_string()],
        );

        let session = registry.get("alice").unwrap();
        assert_eq!(session.held_resource_ids.len(), 2);
        assert!(session.held_resource_ids.contains(&"res-1".to_string()));
    }

    #[test]
    fn test_registry_update_held_resources_nonexistent_user_is_noop() {
        let registry = UserSessionRegistry::new();
        // Should not panic
        registry.update_held_resources("nobody", vec!["res-1".to_string()]);
        assert_eq!(registry.count(), 0);
    }
}
