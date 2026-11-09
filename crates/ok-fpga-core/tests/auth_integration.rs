// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Authentication Integration Tests
//!
//! Tests the full auth provider lifecycle: creation, authentication,
//! session management, and provider switching.

use ok_fpga_core::auth::*;

// ============================================================================
// Auth Provider Factory Tests
// ============================================================================

#[test]
fn factory_creates_no_auth_by_default() {
    let provider = create_provider("none", vec![], None);
    assert_eq!(provider.provider_name(), "none");
    assert!(!provider.is_required());
}

#[test]
fn factory_creates_local_auth() {
    let users = vec![LocalUser {
        username: "admin".to_string(),
        password_hash: "$2b$12$test".to_string(),
        display_name: "Admin User".to_string(),
        groups: vec!["admin".to_string()],
    }];

    let provider = create_provider("local", users, None);
    assert_eq!(provider.provider_name(), "local");
    assert!(provider.is_required());
}

#[test]
fn factory_creates_ldap_auth() {
    let ldap_config = LdapConfig {
        url: "ldaps://ldap.example.com:636".to_string(),
        use_ssl: true,
        domain_components: vec!["example".to_string(), "com".to_string()],
        organizational_units: vec!["Users".to_string()],
        uid_attribute: "uid".to_string(),
        bind_dn: None,
        bind_password: None,
        group_attribute: Some("memberOf".to_string()),
    };

    let provider = create_provider("ldap", vec![], Some(ldap_config));
    assert_eq!(provider.provider_name(), "ldap");
    assert!(provider.is_required());
}

#[test]
fn factory_falls_back_to_none_for_unknown() {
    let provider = create_provider("unknown_provider", vec![], None);
    assert_eq!(provider.provider_name(), "none");
    assert!(!provider.is_required());
}

// ============================================================================
// Authentication Flow Tests
// ============================================================================

#[tokio::test]
async fn no_auth_accepts_any_credentials() {
    let provider = create_provider("none", vec![], None);

    let creds = Credentials {
        username: "anyone".to_string(),
        password: "anything".to_string(),
    };

    let result = provider.authenticate(&creds).await;
    assert!(result.is_ok());

    let auth_user = result.unwrap();
    assert_eq!(auth_user.user_id, "anonymous");
}

#[tokio::test]
async fn local_auth_accepts_known_user() {
    let users = vec![LocalUser {
        username: "testuser".to_string(),
        password_hash: "$2b$12$placeholder".to_string(),
        display_name: "Test User".to_string(),
        groups: vec!["users".to_string()],
    }];

    let provider = create_provider("local", users, None);
    let creds = Credentials {
        username: "testuser".to_string(),
        password: "any_password".to_string(), // stub accepts any non-empty password
    };

    let result = provider.authenticate(&creds).await;
    assert!(result.is_ok());

    let auth_user = result.unwrap();
    assert_eq!(auth_user.user_id, "testuser");
    assert_eq!(auth_user.display_name, Some("Test User".to_string()));
    assert_eq!(auth_user.groups, vec!["users"]);
}

#[tokio::test]
async fn local_auth_rejects_unknown_user() {
    let users = vec![LocalUser {
        username: "admin".to_string(),
        password_hash: "$2b$12$test".to_string(),
        display_name: "Admin".to_string(),
        groups: vec![],
    }];

    let provider = create_provider("local", users, None);
    let creds = Credentials {
        username: "nonexistent".to_string(),
        password: "password".to_string(),
    };

    let result = provider.authenticate(&creds).await;
    assert!(result.is_err());
}

// ============================================================================
// Session Manager Integration Tests
// ============================================================================

#[test]
fn session_lifecycle_create_validate_destroy() {
    let sm = SessionManager::new(3600);

    let auth_user = AuthenticatedUser {
        user_id: "alice".to_string(),
        display_name: Some("Alice".to_string()),
        groups: vec!["admin".to_string()],
    };

    // create_session returns the session ID (String).
    let session_id = sm.create_session(auth_user);
    assert!(!session_id.is_empty());

    // get_session returns Option<AuthenticatedUser>.
    let retrieved = sm.get_session(&session_id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().user_id, "alice");

    // remove_session is void.
    sm.remove_session(&session_id);

    // Should no longer be valid.
    let gone = sm.get_session(&session_id);
    assert!(gone.is_none());
}

#[test]
fn multiple_sessions_are_independent() {
    let sm = SessionManager::new(3600);

    let user1 = AuthenticatedUser {
        user_id: "alice".to_string(),
        display_name: Some("Alice".to_string()),
        groups: vec![],
    };
    let user2 = AuthenticatedUser {
        user_id: "bob".to_string(),
        display_name: Some("Bob".to_string()),
        groups: vec![],
    };

    let id1 = sm.create_session(user1);
    let id2 = sm.create_session(user2);

    // Both should be valid.
    assert!(sm.get_session(&id1).is_some());
    assert!(sm.get_session(&id2).is_some());

    // Remove one — the other should still be valid.
    sm.remove_session(&id1);
    assert!(sm.get_session(&id1).is_none());
    assert!(sm.get_session(&id2).is_some());
}

#[test]
fn session_count_tracks_active_sessions() {
    let sm = SessionManager::new(3600);
    assert_eq!(sm.session_count(), 0);

    let id1 = sm.create_session(AuthenticatedUser {
        user_id: "a".to_string(),
        display_name: None,
        groups: vec![],
    });
    assert_eq!(sm.session_count(), 1);

    let _id2 = sm.create_session(AuthenticatedUser {
        user_id: "b".to_string(),
        display_name: None,
        groups: vec![],
    });
    assert_eq!(sm.session_count(), 2);

    sm.remove_session(&id1);
    assert_eq!(sm.session_count(), 1);
}

#[test]
fn credentials_debug_does_not_leak_password() {
    let creds = Credentials {
        username: "admin".to_string(),
        password: "super_secret_password".to_string(),
    };

    let debug_output = format!("{:?}", creds);
    assert!(
        !debug_output.contains("super_secret"),
        "password should be redacted in debug output: {}",
        debug_output
    );
    assert!(
        debug_output.contains("[REDACTED]"),
        "should contain [REDACTED]: {}",
        debug_output
    );
}
