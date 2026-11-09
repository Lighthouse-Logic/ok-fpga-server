# ADR-008: Authentication Stubs — Local & LDAP

**Status**: Implemented  
**Date**: 2026-08-16  
**Deciders**: Architecture Team  

## Context

The project goals specify:

> "For this version of the server, there should be no login required, but simple stub code should be provided to enable local user definition on the web server. Similarly there should be code for using LDAP for given LDAP URL, with SSL encryption and stub code for domain components as well as organizational units and User IDs."

The authentication system must be:
1. **Off by default** — no login required
2. **Pluggable** — a trait that can be implemented for different auth backends
3. **Local user stub** — skeleton for a local user store
4. **LDAP stub** — skeleton for LDAP authentication with SSL, configurable DC, OU, and UID

## Decision

### Pluggable Auth Trait

```rust
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Authenticate a user with the given credentials.
    /// Returns the authenticated user identity, or an error.
    async fn authenticate(
        &self,
        credentials: &Credentials,
    ) -> Result<AuthenticatedUser, AuthError>;
    
    /// Check if authentication is required.
    /// Returns false for the no-auth provider.
    fn is_required(&self) -> bool;
}

pub struct Credentials {
    pub username: String,
    pub password: String, // will be handled securely; never logged
}

pub struct AuthenticatedUser {
    pub user_id: String,
    pub display_name: Option<String>,
    pub groups: Vec<String>,
}
```

### Provider Implementations

#### 1. `NoAuthProvider` (Default)

```rust
/// No authentication required. All requests are treated as coming from
/// a default anonymous user. User identity can be self-declared via
/// an X-User-ID header or query parameter.
pub struct NoAuthProvider;

impl AuthProvider for NoAuthProvider {
    fn is_required(&self) -> bool { false }
    
    async fn authenticate(&self, _: &Credentials) -> Result<AuthenticatedUser, AuthError> {
        Ok(AuthenticatedUser {
            user_id: "anonymous".into(),
            display_name: Some("Anonymous User".into()),
            groups: vec![],
        })
    }
}
```

#### 2. `LocalAuthProvider` (Stub)

```rust
/// Local user store. Users are defined in the TOML config file.
/// Passwords are stored as bcrypt hashes.
///
/// STUB: This implementation provides the structure and API.
/// Production use requires proper password hashing, session management,
/// and user administration endpoints.
pub struct LocalAuthProvider {
    users: HashMap<String, LocalUser>,
}

struct LocalUser {
    username: String,
    password_hash: String, // bcrypt
    display_name: String,
    groups: Vec<String>,
}
```

TOML config section:
```toml
[auth]
provider = "local"

[[auth.local.users]]
username = "admin"
password_hash = "$2b$12$..."   # bcrypt hash
display_name = "Administrator"
groups = ["admin"]

[[auth.local.users]]
username = "user1"
password_hash = "$2b$12$..."
display_name = "Test User"
groups = ["users"]
```

#### 3. `LdapAuthProvider` (Stub)

```rust
/// LDAP authentication with SSL/TLS.
///
/// STUB: This implementation provides the configuration structure,
/// LDAP bind logic skeleton, and search filter construction.
/// Production use requires proper SSL certificate validation,
/// connection pooling, and error handling.
pub struct LdapAuthProvider {
    config: LdapConfig,
}

pub struct LdapConfig {
    /// LDAP server URL (e.g., "ldaps://ldap.example.com:636")
    pub url: String,
    
    /// Use SSL/TLS (true for ldaps://)
    pub use_ssl: bool,
    
    /// Domain Components (e.g., ["example", "com"] → DC=example,DC=com)
    pub domain_components: Vec<String>,
    
    /// Organizational Units to search (e.g., ["Users", "Engineering"])
    pub organizational_units: Vec<String>,
    
    /// User ID attribute (typically "uid" or "sAMAccountName")
    pub uid_attribute: String,
    
    /// Optional bind DN for service account (if anonymous bind not allowed)
    pub bind_dn: Option<String>,
    
    /// Optional bind password for service account
    pub bind_password: Option<String>,
    
    /// Group membership attribute (e.g., "memberOf")
    pub group_attribute: Option<String>,
}
```

The LDAP bind DN is constructed as:
```
{uid_attribute}={username},OU={organizational_unit},DC={dc1},DC={dc2},...
```

TOML config section:
```toml
[auth]
provider = "ldap"

[auth.ldap]
url = "ldaps://ldap.example.com:636"
use_ssl = true
domain_components = ["example", "com"]
organizational_units = ["Users"]
uid_attribute = "uid"
# bind_dn = "cn=service,DC=example,DC=com"  # optional
# bind_password = "..."                      # optional
# group_attribute = "memberOf"               # optional
```

### Middleware Integration

Authentication is enforced via Axum/Tower middleware:

1. **REST**: Extract credentials from `Authorization: Basic ...` header or session cookie
2. **gRPC**: Extract credentials from gRPC metadata (`authorization` key)
3. **Web Dashboard**: Login form → `POST /api/v1/auth/login` → session cookie
4. **Middleware chain**: `AuthLayer` checks `AuthProvider.is_required()`:
   - If `false` → pass through (no-auth mode)
   - If `true` → validate credentials → inject `AuthenticatedUser` into request extensions

### Session Management (Stub)

For the web dashboard, a simple session mechanism:
- `POST /api/v1/auth/login` → validate credentials → return session cookie
- `POST /api/v1/auth/logout` → invalidate session
- Sessions stored in-memory (`HashMap<SessionId, AuthenticatedUser>`)
- Session expiry configurable (default: 8 hours)

## Consequences

### Positive
- **Zero friction by default** — no auth required out of the box
- **Pluggable** — adding new auth backends (OAuth2, SAML, etc.) just requires implementing the trait
- **LDAP-ready** — the configuration structure handles real-world LDAP deployments
- **Configurable** — auth provider selection and all parameters in TOML

### Negative
- **Stubs only** — neither local nor LDAP auth is production-ready in v1
- **No authorization** — the trait handles *authentication* (who are you?) but not *authorization* (what can you do?). Authorization is a future extension.
- **In-memory sessions** — sessions are lost on restart (acceptable for v1)

### Future Considerations
- **OAuth2 / OIDC provider** for SSO integration
- **Role-based authorization** tied to resource access policies
- **Persistent sessions** (database-backed or JWT)
- **API key authentication** for non-interactive clients

## References

- [Project Goals — Authentication](../../../ok-FPGA-Server_Project-goals.txt) — line 5
