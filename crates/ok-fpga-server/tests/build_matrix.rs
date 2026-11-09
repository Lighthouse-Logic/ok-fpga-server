// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Build Matrix & Config Integration Tests
//!
//! Tests that verify feature flag combinations and configuration parsing.
//! This file exists in the ok-fpga-server binary crate's `tests/` directory.
//! Since it's a binary crate, we can only test via ok-fpga-core public APIs.

use ok_fpga_core::manager::DeviceManager;
use ok_fpga_core::resource::ResourceRegistry;
use ok_fpga_core::service_manager::ServiceManager;

/// Core types are constructable and interoperable.
#[test]
fn core_types_are_constructable() {
    let dm = DeviceManager::new();
    let rr = ResourceRegistry::new();
    let sm = ServiceManager::new();

    assert_eq!(dm.device_count(), 0);
    assert_eq!(rr.count(), 0);
    assert_eq!(sm.list().len(), 0);
}

/// Auth system initializes correctly with all provider types.
#[test]
fn auth_providers_initialize() {
    use ok_fpga_core::auth;

    // None provider.
    let none = auth::create_provider("none", vec![], None);
    assert_eq!(none.provider_name(), "none");
    assert!(!none.is_required());

    // Local provider.
    let users = vec![auth::LocalUser {
        username: "test".to_string(),
        password_hash: "hash".to_string(),
        display_name: "Test".to_string(),
        groups: vec![],
    }];
    let local = auth::create_provider("local", users, None);
    assert_eq!(local.provider_name(), "local");
    assert!(local.is_required());

    // LDAP provider.
    let ldap = auth::create_provider("ldap", vec![], Some(auth::LdapConfig::default()));
    assert_eq!(ldap.provider_name(), "ldap");
    assert!(ldap.is_required());
}

/// Session manager handles concurrent sessions.
#[test]
fn session_manager_concurrent_sessions() {
    use ok_fpga_core::auth::{AuthenticatedUser, SessionManager};

    let sm = SessionManager::new(3600);

    // Create 100 sessions.
    let mut tokens = Vec::new();
    for i in 0..100 {
        let user = AuthenticatedUser {
            user_id: format!("user_{}", i),
            display_name: Some(format!("User {}", i)),
            groups: vec![],
        };
        let session_id = sm.create_session(user);
        tokens.push(session_id);
    }

    // All should be retrievable.
    for token in &tokens {
        assert!(sm.get_session(token).is_some());
    }

    // Remove half.
    for token in &tokens[..50] {
        sm.remove_session(token);
    }

    // First 50 should be gone, last 50 should remain.
    for token in &tokens[..50] {
        assert!(sm.get_session(token).is_none());
    }
    for token in &tokens[50..] {
        assert!(sm.get_session(token).is_some());
    }
}

/// LDAP config generates correct DN strings.
#[test]
fn ldap_config_dn_generation() {
    use ok_fpga_core::auth::LdapConfig;

    let config = LdapConfig {
        url: "ldaps://ldap.example.com:636".to_string(),
        use_ssl: true,
        domain_components: vec!["example".to_string(), "com".to_string()],
        organizational_units: vec!["Users".to_string(), "Engineering".to_string()],
        uid_attribute: "uid".to_string(),
        bind_dn: None,
        bind_password: None,
        group_attribute: Some("memberOf".to_string()),
    };

    assert_eq!(config.base_dn(), "DC=example,DC=com");
    assert_eq!(config.search_dn("Users"), "OU=Users,DC=example,DC=com");
    assert_eq!(
        config.user_bind_dn("jdoe", "Users"),
        "uid=jdoe,OU=Users,DC=example,DC=com"
    );
}
