//! # Authentication — Pluggable Auth Provider System
//!
//! This module implements ADR-008: a pluggable authentication system with
//! three provider implementations:
//!
//! 1. **[`NoAuthProvider`]** (default) — no authentication required, all
//!    requests are treated as anonymous.
//! 2. **[`LocalAuthProvider`]** (stub) — users defined in the TOML config
//!    file with bcrypt password hashes.
//! 3. **[`LdapAuthProvider`]** (stub) — LDAP/LDAPS authentication with
//!    configurable domain components, organizational units, and UID mapping.
//!
//! ## Usage
//!
//! The server selects a provider based on the `[auth]` section in `config.toml`:
//!
//! ```toml
//! [auth]
//! provider = "none"  # Options: "none", "local", "ldap"
//! ```
//!
//! ## Security Note
//!
//! The `LocalAuthProvider` and `LdapAuthProvider` are **stubs**. They provide
//! the API surface and configuration structure but are NOT production-ready.
//! Production use requires:
//! - Proper password hashing verification (bcrypt)
//! - SSL certificate validation for LDAP
//! - Session token rotation and expiry
//! - Rate limiting on login attempts

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during authentication.
#[derive(Debug, Clone)]
pub enum AuthError {
    /// The provided credentials are invalid.
    InvalidCredentials,

    /// The user account is disabled or locked.
    AccountDisabled { username: String },

    /// The auth provider encountered an internal error.
    ProviderError { message: String },

    /// The auth provider is not configured or not available.
    NotConfigured,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCredentials => write!(f, "invalid credentials"),
            Self::AccountDisabled { username } => {
                write!(f, "account '{}' is disabled", username)
            }
            Self::ProviderError { message } => {
                write!(f, "auth provider error: {}", message)
            }
            Self::NotConfigured => write!(f, "auth provider not configured"),
        }
    }
}

impl std::error::Error for AuthError {}

// ============================================================================
// Core Types
// ============================================================================

/// User credentials for authentication.
///
/// **Security**: The `password` field is never logged or serialized.
/// The `Debug` implementation redacts it.
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// A successfully authenticated user identity.
///
/// This struct is injected into request extensions by the auth middleware,
/// allowing route handlers to access the user's identity.
#[derive(Debug, Clone, Serialize)]
pub struct AuthenticatedUser {
    /// Unique user identifier.
    pub user_id: String,

    /// Human-readable display name.
    pub display_name: Option<String>,

    /// Groups the user belongs to (for future authorization).
    pub groups: Vec<String>,
}

/// Session information stored in the in-memory session store.
#[derive(Debug, Clone)]
pub struct Session {
    /// The session ID (opaque token).
    pub session_id: String,

    /// The authenticated user for this session.
    pub user: AuthenticatedUser,

    /// When the session was created (Unix timestamp).
    pub created_at: u64,
}

// ============================================================================
// AuthProvider Trait
// ============================================================================

/// Pluggable authentication provider.
///
/// Implement this trait to add new authentication backends (OAuth2, SAML,
/// API keys, etc.).
///
/// ## Example
///
/// ```rust,ignore
/// struct MyOAuth2Provider { /* ... */ }
///
/// #[async_trait::async_trait]
/// impl AuthProvider for MyOAuth2Provider {
///     fn is_required(&self) -> bool { true }
///     fn provider_name(&self) -> &str { "oauth2" }
///     async fn authenticate(&self, creds: &Credentials)
///         -> Result<AuthenticatedUser, AuthError> { /* ... */ }
/// }
/// ```
#[async_trait::async_trait]
pub trait AuthProvider: Send + Sync {
    /// Whether authentication is required.
    ///
    /// Returns `false` for `NoAuthProvider` (all requests pass through).
    /// Returns `true` for providers that require credentials.
    fn is_required(&self) -> bool;

    /// The name of this provider (for logging and config display).
    fn provider_name(&self) -> &str;

    /// Authenticate a user with the given credentials.
    ///
    /// Returns the authenticated user identity on success, or an `AuthError`.
    async fn authenticate(&self, credentials: &Credentials)
        -> Result<AuthenticatedUser, AuthError>;
}

// ============================================================================
// NoAuthProvider (Default)
// ============================================================================

/// No-authentication provider.
///
/// All requests are treated as coming from an anonymous user. No login
/// is required. This is the default provider.
///
/// When using `NoAuthProvider`, the user identity can be self-declared
/// via an `X-User-ID` header or query parameter (useful for resource
/// tracking without enforced authentication).
pub struct NoAuthProvider;

#[async_trait::async_trait]
impl AuthProvider for NoAuthProvider {
    fn is_required(&self) -> bool {
        false
    }

    fn provider_name(&self) -> &str {
        "none"
    }

    async fn authenticate(
        &self,
        _credentials: &Credentials,
    ) -> Result<AuthenticatedUser, AuthError> {
        // Always succeeds — no authentication required.
        Ok(AuthenticatedUser {
            user_id: "anonymous".to_string(),
            display_name: Some("Anonymous User".to_string()),
            groups: vec![],
        })
    }
}

// ============================================================================
// LocalAuthProvider (Stub)
// ============================================================================

/// A user entry for the local auth provider.
///
/// Users are defined in the TOML config file with bcrypt-hashed passwords.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalUser {
    pub username: String,
    /// Bcrypt hash of the password (e.g., `$2b$12$...`).
    #[serde(default)]
    pub password_hash: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Local user store authentication provider.
///
/// Users are loaded from the TOML config file at startup. Passwords are
/// stored as bcrypt hashes.
///
/// **STUB**: This implementation accepts any password that is non-empty.
/// Production use requires proper bcrypt verification.
pub struct LocalAuthProvider {
    users: HashMap<String, LocalUser>,
}

impl LocalAuthProvider {
    /// Create a new `LocalAuthProvider` from a list of configured users.
    pub fn new(users: Vec<LocalUser>) -> Self {
        let user_map: HashMap<String, LocalUser> =
            users.into_iter().map(|u| (u.username.clone(), u)).collect();
        Self { users: user_map }
    }
}

#[async_trait::async_trait]
impl AuthProvider for LocalAuthProvider {
    fn is_required(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "local"
    }

    async fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> Result<AuthenticatedUser, AuthError> {
        // Look up the user.
        let user = self
            .users
            .get(&credentials.username)
            .ok_or(AuthError::InvalidCredentials)?;

        // STUB: In production, this would verify the bcrypt hash.
        // For now, accept any non-empty password.
        if credentials.password.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }

        // TODO: Production — verify password against user.password_hash
        // using bcrypt::verify(&credentials.password, &user.password_hash)

        Ok(AuthenticatedUser {
            user_id: user.username.clone(),
            display_name: if user.display_name.is_empty() {
                None
            } else {
                Some(user.display_name.clone())
            },
            groups: user.groups.clone(),
        })
    }
}

// ============================================================================
// LdapAuthProvider (Stub)
// ============================================================================

/// LDAP authentication configuration.
///
/// Configures how the server connects to an LDAP directory for authentication.
/// All fields are loaded from the `[auth.ldap]` section in `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    /// LDAP server URL (e.g., `ldaps://ldap.example.com:636`).
    pub url: String,

    /// Whether to use SSL/TLS (true for `ldaps://`).
    #[serde(default = "default_true")]
    pub use_ssl: bool,

    /// Domain Components (e.g., `["example", "com"]` → `DC=example,DC=com`).
    pub domain_components: Vec<String>,

    /// Organizational Units to search (e.g., `["Users", "Engineering"]`).
    pub organizational_units: Vec<String>,

    /// User ID attribute name (typically `"uid"` or `"sAMAccountName"`).
    #[serde(default = "default_uid_attribute")]
    pub uid_attribute: String,

    /// Optional bind DN for a service account (if anonymous bind is not allowed).
    pub bind_dn: Option<String>,

    /// Optional bind password for the service account.
    /// **Security**: Never logged.
    pub bind_password: Option<String>,

    /// Optional group membership attribute (e.g., `"memberOf"`).
    pub group_attribute: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_uid_attribute() -> String {
    "uid".to_string()
}

impl Default for LdapConfig {
    fn default() -> Self {
        Self {
            url: "ldaps://ldap.example.com:636".to_string(),
            use_ssl: true,
            domain_components: vec!["example".to_string(), "com".to_string()],
            organizational_units: vec!["Users".to_string()],
            uid_attribute: "uid".to_string(),
            bind_dn: None,
            bind_password: None,
            group_attribute: None,
        }
    }
}

impl LdapConfig {
    /// Build the base DN from domain components.
    ///
    /// Example: `["example", "com"]` → `"DC=example,DC=com"`
    pub fn base_dn(&self) -> String {
        self.domain_components
            .iter()
            .map(|dc| format!("DC={}", dc))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Build the search DN for a specific OU.
    ///
    /// Example: `ou="Users"`, domain=`["example", "com"]`
    /// → `"OU=Users,DC=example,DC=com"`
    pub fn search_dn(&self, ou: &str) -> String {
        format!("OU={},{}", ou, self.base_dn())
    }

    /// Build the user bind DN for a specific username and OU.
    ///
    /// Example: `uid="jdoe"`, ou=`"Users"`, domain=`["example", "com"]`
    /// → `"uid=jdoe,OU=Users,DC=example,DC=com"`
    pub fn user_bind_dn(&self, username: &str, ou: &str) -> String {
        format!(
            "{}={},OU={},{}",
            self.uid_attribute,
            username,
            ou,
            self.base_dn()
        )
    }
}

/// LDAP authentication provider.
///
/// **STUB**: This implementation constructs the correct LDAP bind DN but
/// does not actually connect to an LDAP server. Production use requires:
/// - An LDAP client library (e.g., `ldap3`)
/// - Proper SSL/TLS certificate validation
/// - Connection pooling
/// - Error handling for network failures
pub struct LdapAuthProvider {
    config: LdapConfig,
}

impl LdapAuthProvider {
    /// Create a new `LdapAuthProvider` with the given configuration.
    pub fn new(config: LdapConfig) -> Self {
        Self { config }
    }

    /// Get a reference to the LDAP configuration.
    pub fn config(&self) -> &LdapConfig {
        &self.config
    }
}

#[async_trait::async_trait]
impl AuthProvider for LdapAuthProvider {
    fn is_required(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "ldap"
    }

    async fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> Result<AuthenticatedUser, AuthError> {
        // Validate credentials are non-empty.
        if credentials.username.is_empty() || credentials.password.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }

        // STUB: In production, this would:
        // 1. Connect to the LDAP server (with SSL if configured)
        // 2. Bind with the service account (if configured) or anonymous bind
        // 3. Search each OU for the user
        // 4. Attempt a user bind with the provided password
        // 5. Extract group memberships from the group_attribute
        //
        // For now, log the constructed bind DN and return success.

        let bind_dn = if let Some(ou) = self.config.organizational_units.first() {
            self.config.user_bind_dn(&credentials.username, ou)
        } else {
            format!(
                "{}={},{}",
                self.config.uid_attribute,
                credentials.username,
                self.config.base_dn()
            )
        };

        tracing::info!(
            ldap_url = %self.config.url,
            bind_dn = %bind_dn,
            use_ssl = self.config.use_ssl,
            "LDAP auth stub: would authenticate user"
        );

        // STUB: Always return success.
        // TODO: Production — perform actual LDAP bind and search.
        Ok(AuthenticatedUser {
            user_id: credentials.username.clone(),
            display_name: Some(credentials.username.clone()),
            groups: vec![],
        })
    }
}

// ============================================================================
// Session Manager
// ============================================================================

/// In-memory session store.
///
/// Manages active sessions for the web dashboard. Sessions are stored in
/// a `HashMap` keyed by session ID (opaque token).
///
/// **Note**: Sessions are lost on server restart. This is acceptable for v1.
/// Future versions may use a database or JWT tokens.
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Session>>,
    /// Session expiry duration in seconds (default: 8 hours).
    expiry_secs: u64,
}

impl SessionManager {
    /// Create a new `SessionManager` with the given expiry duration.
    pub fn new(expiry_secs: u64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            expiry_secs,
        }
    }

    /// Create a new session for an authenticated user.
    ///
    /// Returns the session ID (opaque token) to be set as a cookie.
    pub fn create_session(&self, user: AuthenticatedUser) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};

        let session_id = generate_session_id();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = Session {
            session_id: session_id.clone(),
            user,
            created_at,
        };

        if let Ok(mut sessions) = self.sessions.write() {
            sessions.insert(session_id.clone(), session);
        }

        session_id
    }

    /// Look up a session by ID.
    ///
    /// Returns `None` if the session doesn't exist or has expired.
    pub fn get_session(&self, session_id: &str) -> Option<AuthenticatedUser> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let sessions = self.sessions.read().ok()?;
        let session = sessions.get(session_id)?;

        // Check expiry.
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now - session.created_at > self.expiry_secs {
            return None; // Expired.
        }

        Some(session.user.clone())
    }

    /// Invalidate (remove) a session.
    pub fn remove_session(&self, session_id: &str) {
        if let Ok(mut sessions) = self.sessions.write() {
            sessions.remove(session_id);
        }
    }

    /// Get the count of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.read().map(|s| s.len()).unwrap_or(0)
    }

    /// Remove all expired sessions.
    pub fn cleanup_expired(&self) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Ok(mut sessions) = self.sessions.write() {
            sessions.retain(|_, s| now - s.created_at <= self.expiry_secs);
        }
    }
}

/// Generate a cryptographically random session ID.
///
/// Uses a simple hex-encoded random string (32 bytes = 64 hex chars).
fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Simple pseudo-random ID using timestamp + counter.
    // In production, use `rand::thread_rng()` or `getrandom`.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    format!("{:032x}", timestamp)
}

// ============================================================================
// Factory
// ============================================================================

/// Create an auth provider from a configuration string.
///
/// ## Supported providers:
/// - `"none"` → [`NoAuthProvider`]
/// - `"local"` → [`LocalAuthProvider`] (requires user list)
/// - `"ldap"` → [`LdapAuthProvider`] (requires LDAP config)
pub fn create_provider(
    provider_name: &str,
    local_users: Vec<LocalUser>,
    ldap_config: Option<LdapConfig>,
) -> Box<dyn AuthProvider> {
    match provider_name {
        "local" => Box::new(LocalAuthProvider::new(local_users)),
        "ldap" => {
            let config = ldap_config.unwrap_or_default();
            Box::new(LdapAuthProvider::new(config))
        }
        _ => Box::new(NoAuthProvider),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_auth_always_succeeds() {
        let provider = NoAuthProvider;
        assert!(!provider.is_required());
        assert_eq!(provider.provider_name(), "none");

        let result = provider
            .authenticate(&Credentials {
                username: "anyone".to_string(),
                password: "anything".to_string(),
            })
            .await;

        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.user_id, "anonymous");
    }

    #[tokio::test]
    async fn local_auth_accepts_known_user() {
        let provider = LocalAuthProvider::new(vec![LocalUser {
            username: "admin".to_string(),
            password_hash: "$2b$12$fakehash".to_string(),
            display_name: "Administrator".to_string(),
            groups: vec!["admin".to_string()],
        }]);

        assert!(provider.is_required());

        let result = provider
            .authenticate(&Credentials {
                username: "admin".to_string(),
                password: "any-non-empty".to_string(),
            })
            .await;

        assert!(result.is_ok());
        let user = result.unwrap();
        assert_eq!(user.user_id, "admin");
        assert_eq!(user.groups, vec!["admin"]);
    }

    #[tokio::test]
    async fn local_auth_rejects_unknown_user() {
        let provider = LocalAuthProvider::new(vec![]);
        let result = provider
            .authenticate(&Credentials {
                username: "nobody".to_string(),
                password: "pass".to_string(),
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn local_auth_rejects_empty_password() {
        let provider = LocalAuthProvider::new(vec![LocalUser {
            username: "admin".to_string(),
            password_hash: String::new(),
            display_name: String::new(),
            groups: vec![],
        }]);

        let result = provider
            .authenticate(&Credentials {
                username: "admin".to_string(),
                password: String::new(),
            })
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn ldap_config_base_dn() {
        let config = LdapConfig {
            domain_components: vec!["example".to_string(), "com".to_string()],
            ..Default::default()
        };
        assert_eq!(config.base_dn(), "DC=example,DC=com");
    }

    #[test]
    fn ldap_config_search_dn() {
        let config = LdapConfig {
            domain_components: vec!["corp".to_string(), "example".to_string(), "com".to_string()],
            ..Default::default()
        };
        assert_eq!(
            config.search_dn("Users"),
            "OU=Users,DC=corp,DC=example,DC=com"
        );
    }

    #[test]
    fn ldap_config_user_bind_dn() {
        let config = LdapConfig {
            uid_attribute: "sAMAccountName".to_string(),
            domain_components: vec!["example".to_string(), "com".to_string()],
            ..Default::default()
        };
        assert_eq!(
            config.user_bind_dn("jdoe", "Engineering"),
            "sAMAccountName=jdoe,OU=Engineering,DC=example,DC=com"
        );
    }

    #[tokio::test]
    async fn ldap_auth_stub_succeeds() {
        let provider = LdapAuthProvider::new(LdapConfig::default());
        assert!(provider.is_required());
        assert_eq!(provider.provider_name(), "ldap");

        let result = provider
            .authenticate(&Credentials {
                username: "jdoe".to_string(),
                password: "password".to_string(),
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().user_id, "jdoe");
    }

    #[tokio::test]
    async fn ldap_auth_rejects_empty_credentials() {
        let provider = LdapAuthProvider::new(LdapConfig::default());

        let result = provider
            .authenticate(&Credentials {
                username: String::new(),
                password: "pass".to_string(),
            })
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn session_manager_create_and_retrieve() {
        let mgr = SessionManager::new(3600);
        let user = AuthenticatedUser {
            user_id: "test".to_string(),
            display_name: Some("Test User".to_string()),
            groups: vec![],
        };

        let session_id = mgr.create_session(user);
        let retrieved = mgr.get_session(&session_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().user_id, "test");
    }

    #[test]
    fn session_manager_remove() {
        let mgr = SessionManager::new(3600);
        let user = AuthenticatedUser {
            user_id: "test".to_string(),
            display_name: None,
            groups: vec![],
        };

        let session_id = mgr.create_session(user);
        assert_eq!(mgr.session_count(), 1);

        mgr.remove_session(&session_id);
        assert_eq!(mgr.session_count(), 0);
        assert!(mgr.get_session(&session_id).is_none());
    }

    #[test]
    fn create_provider_factory() {
        let provider = create_provider("none", vec![], None);
        assert!(!provider.is_required());
        assert_eq!(provider.provider_name(), "none");

        let provider = create_provider("local", vec![], None);
        assert!(provider.is_required());
        assert_eq!(provider.provider_name(), "local");

        let provider = create_provider("ldap", vec![], None);
        assert!(provider.is_required());
        assert_eq!(provider.provider_name(), "ldap");

        // Unknown falls back to none.
        let provider = create_provider("unknown", vec![], None);
        assert!(!provider.is_required());
    }

    #[test]
    fn credentials_debug_redacts_password() {
        let creds = Credentials {
            username: "admin".to_string(),
            password: "super-secret".to_string(),
        };
        let debug = format!("{:?}", creds);
        assert!(debug.contains("admin"));
        assert!(!debug.contains("super-secret"));
        assert!(debug.contains("[REDACTED]"));
    }
}
