//! # Multi-User Resource Management
//!
//! This module implements the resource model from ADR-006. A "resource" is a
//! named, user-defined capability exposed by a programmed FPGA. Resources can
//! be shared among multiple users or locked for exclusive access.
//!
//! ## Key Concepts
//!
//! - **Resources are NOT entire FPGAs** — one FPGA may expose multiple resources.
//! - **Resources only exist after programming** — an unprogrammed FPGA has no resources.
//! - **Resources are developer-defined** — they are not auto-discovered from hardware.
//! - **Access control is pluggable** — the [`ResourceAccessPolicy`](crate::resource::ResourceAccessPolicy) trait defines the
//!   rules for who can acquire what.

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

// ============================================================================
// Resource Identity
// ============================================================================

/// A unique identifier for a resource.
///
/// Resource IDs are strings that uniquely identify a resource across the
/// entire server. They are typically structured as `{serial}:{resource_name}`.
pub type ResourceId = String;

/// A unique identifier for a user.
///
/// In the initial version (no authentication), users are self-identified
/// via a header or parameter. The auth stubs (ADR-008) will eventually
/// provide authenticated user identity.
pub type UserId = String;

// ============================================================================
// Resource State
// ============================================================================

/// The current state of a resource.
///
/// Resources have five possible states (ADR-006). Transitions are governed
/// by the resource lifecycle and access control logic.
///
/// ## State Transitions
///
/// ```text
/// Unavailable ──► Initializing ──► Available ──► InUseShared
///      ▲                              │              │
///      │                              ▼              │
///      │                        InUseLocked          │
///      │                              │              │
///      └──────────────────────────────┴──────────────┘
///                    (release / disconnect / reprogram)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceState {
    /// Default state. FPGA not programmed, or resource explicitly marked
    /// unavailable.
    Unavailable,

    /// FPGA is being programmed or resource is being set up. Transitional.
    Initializing,

    /// Resource is ready and no user has acquired it.
    Available,

    /// One or more users are using the resource concurrently. Other users
    /// may also acquire it (in shared mode).
    InUseShared,

    /// Exactly one user has exclusive access. No other users may acquire it
    /// until released.
    InUseLocked,
}

impl fmt::Display for ResourceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceState::Unavailable => write!(f, "Unavailable"),
            ResourceState::Initializing => write!(f, "Initializing"),
            ResourceState::Available => write!(f, "Available"),
            ResourceState::InUseShared => write!(f, "In Use (Shared)"),
            ResourceState::InUseLocked => write!(f, "In Use (Locked)"),
        }
    }
}

// ============================================================================
// Access Mode
// ============================================================================

/// How a user requests to use a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    /// The user wants shared access. Multiple users can hold the resource
    /// simultaneously.
    Shared,

    /// The user wants exclusive access. No other users can hold the resource.
    Exclusive,
}

impl fmt::Display for AccessMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessMode::Shared => write!(f, "Shared"),
            AccessMode::Exclusive => write!(f, "Exclusive"),
        }
    }
}

// ============================================================================
// Resource Struct
// ============================================================================

/// A named, addressable unit of functionality exposed by a programmed FPGA.
///
/// Resources are defined by the developer/integrator (not auto-discovered).
/// Each resource is associated with a specific FPGA serial number and is
/// only valid while the FPGA is programmed with the expected bitfile.
///
/// ## Example
///
/// ```
/// use ok_fpga_core::resource::{Resource, ResourceState};
///
/// let resource = Resource::new(
///     "ABC123:dac_ch1".to_string(),
///     "dac_ch1".to_string(),
///     Some("DAC Channel 1 — 16-bit analog output".to_string()),
///     "ABC1234567".to_string(),
/// );
///
/// assert_eq!(resource.state, ResourceState::Available);
/// assert!(resource.holders.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Unique identifier for this resource.
    pub id: ResourceId,

    /// Human-readable name (e.g., "dac_channel_1").
    pub name: String,

    /// Optional description of what this resource does.
    pub description: Option<String>,

    /// Serial number of the FPGA that provides this resource.
    pub device_serial: String,

    /// Current state of the resource.
    pub state: ResourceState,

    /// Users currently holding this resource.
    /// Empty if the resource is `Available` or `Unavailable`.
    pub holders: Vec<UserId>,

    /// Access mode when the resource is in use.
    /// `None` when the resource is not in use.
    pub access_mode: Option<AccessMode>,
}

impl Resource {
    /// Create a new resource in the `Available` state with no holders.
    pub fn new(
        id: ResourceId,
        name: String,
        description: Option<String>,
        device_serial: String,
    ) -> Self {
        Resource {
            id,
            name,
            description,
            device_serial,
            state: ResourceState::Available,
            holders: Vec::new(),
            access_mode: None,
        }
    }

    /// Returns `true` if the resource can potentially be acquired.
    pub fn is_acquirable(&self) -> bool {
        matches!(
            self.state,
            ResourceState::Available | ResourceState::InUseShared
        )
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Resource '{}' ({}) — {} [holders: {}]",
            self.name,
            self.id,
            self.state,
            self.holders.len()
        )
    }
}

// ============================================================================
// Acquisition Denied Error
// ============================================================================

/// Reason why a resource acquisition was denied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcquisitionDenied {
    /// The resource is not in a state that allows acquisition.
    InvalidState { current_state: ResourceState },

    /// The resource is exclusively locked by another user.
    ExclusivelyHeld { held_by: UserId },

    /// The user requested exclusive mode but the resource is already shared.
    AlreadyShared { holder_count: usize },

    /// Custom denial reason (for pluggable policies).
    Custom { reason: String },
}

impl fmt::Display for AcquisitionDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AcquisitionDenied::InvalidState { current_state } => {
                write!(f, "Resource is {}, cannot acquire", current_state)
            }
            AcquisitionDenied::ExclusivelyHeld { held_by } => {
                write!(f, "Resource is exclusively held by '{}'", held_by)
            }
            AcquisitionDenied::AlreadyShared { holder_count } => {
                write!(
                    f,
                    "Resource is shared by {} users, cannot lock exclusively",
                    holder_count
                )
            }
            AcquisitionDenied::Custom { reason } => {
                write!(f, "Acquisition denied: {}", reason)
            }
        }
    }
}

// ============================================================================
// Resource Access Policy Trait
// ============================================================================

/// Defines the rules for resource acquisition and release.
///
/// This trait is the extensibility point for custom access control logic.
/// The default implementation ([`DefaultAccessPolicy`]) uses the simple rules
/// from ADR-006. Developers can replace this with HDL-specific logic.
///
/// ## Default Rules
///
/// - `Available` → allow acquire in either mode
/// - `InUseShared` + request `Shared` → allow
/// - `InUseShared` + request `Exclusive` → deny
/// - `InUseLocked` → deny
/// - `Unavailable` or `Initializing` → deny
pub trait ResourceAccessPolicy: Send + Sync {
    /// Determine if the given user can acquire the resource in the requested mode.
    ///
    /// Returns `Ok(())` if the acquisition is allowed, or
    /// `Err(AcquisitionDenied)` with the reason.
    fn can_acquire(
        &self,
        resource: &Resource,
        user: &UserId,
        requested_mode: AccessMode,
    ) -> Result<(), AcquisitionDenied>;

    /// Called when a resource's FPGA is disconnected or reprogrammed.
    ///
    /// The default implementation does nothing. Override this to implement
    /// custom notification or cleanup logic.
    fn on_resource_invalidated(&self, _resource: &Resource, _holders: &[UserId]) {
        // Default: no-op. Subclasses can override for custom behavior.
    }
}

// ============================================================================
// Default Access Policy
// ============================================================================

/// The default resource access policy implementing the rules from ADR-006.
///
/// This is a **stub** that developers can replace with HDL-specific logic
/// by implementing the [`ResourceAccessPolicy`] trait.
pub struct DefaultAccessPolicy;

impl ResourceAccessPolicy for DefaultAccessPolicy {
    fn can_acquire(
        &self,
        resource: &Resource,
        _user: &UserId,
        requested_mode: AccessMode,
    ) -> Result<(), AcquisitionDenied> {
        match &resource.state {
            ResourceState::Available => {
                // Available resources can be acquired in any mode.
                Ok(())
            }
            ResourceState::InUseShared => {
                // Shared resources can accept more shared users.
                if requested_mode == AccessMode::Shared {
                    Ok(())
                } else {
                    Err(AcquisitionDenied::AlreadyShared {
                        holder_count: resource.holders.len(),
                    })
                }
            }
            ResourceState::InUseLocked => {
                // Exclusively locked — deny everyone.
                let held_by = resource
                    .holders
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                Err(AcquisitionDenied::ExclusivelyHeld { held_by })
            }
            other => {
                // Unavailable or Initializing — cannot acquire.
                Err(AcquisitionDenied::InvalidState {
                    current_state: other.clone(),
                })
            }
        }
    }
}

// ============================================================================
// Resource Registry
// ============================================================================

/// Thread-safe registry of all resources across all devices.
///
/// The registry is wrapped in `Arc<RwLock<...>>` so it can be shared
/// between the device manager, the REST/gRPC handlers, and the event
/// broadcast system.
///
/// ## Usage
///
/// ```
/// use ok_fpga_core::resource::ResourceRegistry;
///
/// let registry = ResourceRegistry::new();
/// assert_eq!(registry.count(), 0);
/// ```
pub struct ResourceRegistry {
    /// Map from resource ID to resource.
    resources: RwLock<HashMap<ResourceId, Resource>>,

    /// The access policy used for acquire/release decisions.
    policy: Box<dyn ResourceAccessPolicy>,
}

impl ResourceRegistry {
    /// Create a new empty resource registry with the default access policy.
    pub fn new() -> Self {
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            policy: Box::new(DefaultAccessPolicy),
        }
    }

    /// Create a new registry with a custom access policy.
    pub fn with_policy(policy: Box<dyn ResourceAccessPolicy>) -> Self {
        ResourceRegistry {
            resources: RwLock::new(HashMap::new()),
            policy,
        }
    }

    /// Returns the number of resources in the registry.
    pub fn count(&self) -> usize {
        self.resources.read().unwrap().len()
    }

    /// Register a new resource. Returns `true` if the resource was added,
    /// `false` if a resource with the same ID already exists.
    pub fn register(&self, resource: Resource) -> bool {
        let mut map = self.resources.write().unwrap();
        if map.contains_key(&resource.id) {
            false
        } else {
            map.insert(resource.id.clone(), resource);
            true
        }
    }

    /// Remove a resource by ID. Returns the removed resource, or `None`.
    pub fn remove(&self, id: &str) -> Option<Resource> {
        self.resources.write().unwrap().remove(id)
    }

    /// Get a clone of a resource by ID.
    pub fn get(&self, id: &str) -> Option<Resource> {
        self.resources.read().unwrap().get(id).cloned()
    }

    /// List all resources (returns cloned values).
    pub fn list(&self) -> Vec<Resource> {
        self.resources.read().unwrap().values().cloned().collect()
    }

    /// List all resources for a specific device serial.
    pub fn list_for_device(&self, serial: &str) -> Vec<Resource> {
        self.resources
            .read()
            .unwrap()
            .values()
            .filter(|r| r.device_serial == serial)
            .cloned()
            .collect()
    }

    /// Attempt to acquire a resource for a user.
    ///
    /// Returns `Ok(())` if the acquisition succeeded, or
    /// `Err(AcquisitionDenied)` if the policy denied it.
    pub fn acquire(
        &self,
        resource_id: &str,
        user_id: &UserId,
        mode: AccessMode,
    ) -> Result<(), AcquisitionDenied> {
        let mut map = self.resources.write().unwrap();
        let resource = map
            .get(resource_id)
            .ok_or_else(|| AcquisitionDenied::Custom {
                reason: format!("Resource '{}' not found", resource_id),
            })?;

        // Check policy first (on immutable reference).
        self.policy.can_acquire(resource, user_id, mode)?;

        // Policy approved — mutate the resource.
        let resource = map.get_mut(resource_id).unwrap();
        resource.holders.push(user_id.clone());
        resource.access_mode = Some(mode);
        resource.state = match mode {
            AccessMode::Shared => ResourceState::InUseShared,
            AccessMode::Exclusive => ResourceState::InUseLocked,
        };

        Ok(())
    }

    /// Release a resource for a user.
    ///
    /// Returns `true` if the user was holding the resource and it was released.
    /// Returns `false` if the user was not holding the resource.
    pub fn release(&self, resource_id: &str, user_id: &UserId) -> bool {
        let mut map = self.resources.write().unwrap();
        if let Some(resource) = map.get_mut(resource_id) {
            let before = resource.holders.len();
            resource.holders.retain(|h| h != user_id);
            let removed = resource.holders.len() < before;

            if removed && resource.holders.is_empty() {
                // No more holders — transition back to Available.
                resource.state = ResourceState::Available;
                resource.access_mode = None;
            }

            removed
        } else {
            false
        }
    }

    /// Invalidate all resources for a specific device.
    ///
    /// This is called when a device is disconnected or reprogrammed.
    /// All resources for the device are set to `Unavailable` and all
    /// holders are cleared.
    pub fn invalidate_device(&self, serial: &str) {
        let mut map = self.resources.write().unwrap();
        for resource in map.values_mut() {
            if resource.device_serial == serial {
                // Notify the policy before clearing.
                self.policy
                    .on_resource_invalidated(resource, &resource.holders);

                resource.state = ResourceState::Unavailable;
                resource.holders.clear();
                resource.access_mode = None;
            }
        }
    }
}

impl Default for ResourceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_resource(id: &str) -> Resource {
        Resource::new(
            id.to_string(),
            format!("resource_{}", id),
            None,
            "ABC123".to_string(),
        )
    }

    #[test]
    fn register_and_list_resources() {
        let registry = ResourceRegistry::new();
        assert!(registry.register(test_resource("r1")));
        assert!(registry.register(test_resource("r2")));
        assert_eq!(registry.count(), 2);

        // Duplicate registration fails.
        assert!(!registry.register(test_resource("r1")));
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn acquire_available_resource() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        let result = registry.acquire("r1", &"alice".to_string(), AccessMode::Shared);
        assert!(result.is_ok());

        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseShared);
        assert_eq!(r.holders, vec!["alice"]);
    }

    #[test]
    fn acquire_exclusive_blocks_others() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap();

        let result = registry.acquire("r1", &"bob".to_string(), AccessMode::Shared);
        assert!(result.is_err());
    }

    #[test]
    fn shared_allows_multiple_users() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();

        let r = registry.get("r1").unwrap();
        assert_eq!(r.holders.len(), 2);
    }

    #[test]
    fn release_transitions_back_to_available() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        assert!(registry.release("r1", &"alice".to_string()));

        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::Available);
        assert!(r.holders.is_empty());
    }

    #[test]
    fn invalidate_device_clears_resources() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        registry.invalidate_device("ABC123");

        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::Unavailable);
        assert!(r.holders.is_empty());
    }

    #[test]
    fn list_for_device_filters_correctly() {
        let registry = ResourceRegistry::new();
        registry.register(Resource::new(
            "r1".to_string(),
            "res1".to_string(),
            None,
            "DEV_A".to_string(),
        ));
        registry.register(Resource::new(
            "r2".to_string(),
            "res2".to_string(),
            None,
            "DEV_B".to_string(),
        ));

        let dev_a_resources = registry.list_for_device("DEV_A");
        assert_eq!(dev_a_resources.len(), 1);
        assert_eq!(dev_a_resources[0].device_serial, "DEV_A");
    }
}
