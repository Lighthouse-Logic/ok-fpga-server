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

/// Design-level access constraint for a resource (ADR-020 §3).
///
/// This is set at **registration time** based on the FPGA HDL bitfile design,
/// not at acquisition time. If the resource's hardware design allows concurrent
/// access from multiple users, use `Shared`. If concurrent access would cause
/// interference, use `Exclusive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessMode {
    /// Multiple users can hold the resource simultaneously.
    /// Holders may temporarily `lock` the resource for uninterrupted access.
    Shared,

    /// Only one user can hold the resource at a time.
    /// The resource is always implicitly locked when acquired.
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
// Interface Declaration (ADR-020 §6)
// ============================================================================

/// Which data port interfaces a resource uses.
///
/// At least one interface must be enabled. The server validates these
/// declarations against the device's actual capabilities at registration time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceDeclaration {
    /// Uses Classic FrontPanel endpoints (wires, triggers, pipes).
    pub classic: bool,
    /// Uses AXI4-Lite register access (CSRs, configuration).
    pub axi_lite: bool,
    /// Uses AXI4 burst memory-mapped access (BRAM, DDR).
    pub axi_full: bool,
    /// Uses AXI4-Stream addressless streaming (DSP, ADC/DAC).
    pub axi_stream: bool,
}

impl Default for InterfaceDeclaration {
    /// Defaults to Classic-only, matching the most common use case.
    fn default() -> Self {
        InterfaceDeclaration {
            classic: true,
            axi_lite: false,
            axi_full: false,
            axi_stream: false,
        }
    }
}

impl InterfaceDeclaration {
    /// Returns `true` if at least one interface is enabled.
    pub fn has_any(&self) -> bool {
        self.classic || self.axi_lite || self.axi_full || self.axi_stream
    }

    /// Returns `true` if any AXI interface is enabled.
    pub fn has_axi(&self) -> bool {
        self.axi_lite || self.axi_full || self.axi_stream
    }
}

// ============================================================================
// Classic Endpoint Types (ADR-020 §7)
// ============================================================================

/// The type of a Classic FrontPanel endpoint.
///
/// Each type has a defined address range. Standard Pipes and Block-Throttled
/// Pipes share the same address ranges (0x80–0x9F for input, 0xA0–0xBF for
/// output). See `docs/notes-on-Pipes-and-Block-Throttled-Pipes.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndpointType {
    /// WireIn endpoints: addresses 0x00–0x1F.
    WireIn,
    /// WireOut endpoints: addresses 0x20–0x3F.
    WireOut,
    /// TriggerIn endpoints: addresses 0x40–0x5F.
    TriggerIn,
    /// TriggerOut endpoints: addresses 0x60–0x7F.
    TriggerOut,
    /// Standard PipeIn endpoints: addresses 0x80–0x9F.
    PipeIn,
    /// Standard PipeOut endpoints: addresses 0xA0–0xBF.
    PipeOut,
    /// Block-Throttled PipeIn endpoints: addresses 0x80–0x9F (shared with PipeIn).
    BtPipeIn,
    /// Block-Throttled PipeOut endpoints: addresses 0xA0–0xBF (shared with PipeOut).
    BtPipeOut,
}

impl EndpointType {
    /// Returns the valid address range (inclusive) for this endpoint type.
    pub fn address_range(&self) -> (u8, u8) {
        match self {
            EndpointType::WireIn => (0x00, 0x1F),
            EndpointType::WireOut => (0x20, 0x3F),
            EndpointType::TriggerIn => (0x40, 0x5F),
            EndpointType::TriggerOut => (0x60, 0x7F),
            EndpointType::PipeIn | EndpointType::BtPipeIn => (0x80, 0x9F),
            EndpointType::PipeOut | EndpointType::BtPipeOut => (0xA0, 0xBF),
        }
    }

    /// Returns `true` if the given address is valid for this endpoint type.
    pub fn is_valid_address(&self, addr: u8) -> bool {
        let (lo, hi) = self.address_range();
        addr >= lo && addr <= hi
    }

    /// Infer the endpoint type from a raw address (returns the "default" type
    /// for the range, preferring standard pipes over BT pipes).
    pub fn from_address(addr: u8) -> Option<EndpointType> {
        match addr {
            0x00..=0x1F => Some(EndpointType::WireIn),
            0x20..=0x3F => Some(EndpointType::WireOut),
            0x40..=0x5F => Some(EndpointType::TriggerIn),
            0x60..=0x7F => Some(EndpointType::TriggerOut),
            0x80..=0x9F => Some(EndpointType::PipeIn),
            0xA0..=0xBF => Some(EndpointType::PipeOut),
            _ => None,
        }
    }
}

impl fmt::Display for EndpointType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointType::WireIn => write!(f, "wire_in"),
            EndpointType::WireOut => write!(f, "wire_out"),
            EndpointType::TriggerIn => write!(f, "trigger_in"),
            EndpointType::TriggerOut => write!(f, "trigger_out"),
            EndpointType::PipeIn => write!(f, "pipe_in"),
            EndpointType::PipeOut => write!(f, "pipe_out"),
            EndpointType::BtPipeIn => write!(f, "bt_pipe_in"),
            EndpointType::BtPipeOut => write!(f, "bt_pipe_out"),
        }
    }
}

// ============================================================================
// Endpoint Entry and Declaration (ADR-020 §7)
// ============================================================================

/// A single Classic endpoint declaration within a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointEntry {
    /// Endpoint address (0x00–0xBF).
    pub address: u8,
    /// The type of endpoint (must match the address range).
    pub ep_type: EndpointType,
    /// Whether this endpoint is shared or exclusive.
    pub access: AccessMode,
    /// Optional human-readable description.
    pub description: Option<String>,
}

/// Declaration of all Classic endpoints used by a resource (ADR-020 §7).
///
/// The `default_access` applies to any endpoint NOT explicitly listed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDeclaration {
    /// Default access mode for endpoints not listed in `entries`.
    pub default_access: AccessMode,
    /// Explicitly declared endpoints with per-EP access modes.
    pub entries: Vec<EndpointEntry>,
}

// ============================================================================
// Address Range Declaration (ADR-020 §8)
// ============================================================================

/// A 32-bit address range used by a resource (ADR-020 §8).
///
/// Represents a memory-mapped region in the FPGA design. Ranges need not
/// be contiguous; a resource may declare multiple ranges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressRange {
    /// Start of the range (inclusive), e.g. 0x44A6_0000.
    pub start: u32,
    /// End of the range (inclusive), e.g. 0x44A8_FFFF.
    pub end: u32,
    /// Whether this range is shared or exclusive.
    pub access: AccessMode,
}

impl AddressRange {
    /// Returns `true` if this range overlaps with `other`.
    pub fn overlaps(&self, other: &AddressRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

impl fmt::Display for AddressRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08X}–0x{:08X} ({})", self.start, self.end, self.access)
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
/// ## Enhanced Schema (ADR-020)
///
/// Resources now include:
/// - **`access_mode`**: Design-level constraint (shared vs. exclusive), set at
///   registration time — not a runtime user choice.
/// - **`interfaces`**: Which data port interfaces the resource uses.
/// - **`endpoints`**: Classic endpoint declarations with per-EP sharing.
/// - **`address_ranges`**: 32-bit memory-mapped address ranges.
/// - **`metadata`**: Extensible key-value pairs.
///
/// ## Example
///
/// ```
/// use ok_fpga_core::resource::{Resource, ResourceState, AccessMode};
///
/// let resource = Resource::new(
///     "ABC123:dac_ch1".to_string(),
///     "dac_ch1".to_string(),
///     Some("DAC Channel 1 — 16-bit analog output".to_string()),
///     "ABC1234567".to_string(),
///     AccessMode::Exclusive,
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

    /// Design-level access mode constraint (ADR-020 §3).
    /// Set at registration time. Determines whether the resource can be shared
    /// or is always exclusive. `None` for legacy resources created without a mode.
    pub access_mode: Option<AccessMode>,

    /// User ID that currently holds the lock on a shared resource (ADR-020 §3).
    /// Only set when `access_mode == Some(Shared)` and the resource is locked.
    pub locked_by: Option<UserId>,

    /// Which data port interfaces this resource uses (ADR-020 §6).
    pub interfaces: InterfaceDeclaration,

    /// Classic endpoint declarations (ADR-020 §7).
    /// Required when `interfaces.classic == true`.
    pub endpoints: Option<EndpointDeclaration>,

    /// 32-bit address ranges used by this resource (ADR-020 §8).
    pub address_ranges: Vec<AddressRange>,

    /// Extensible key-value metadata (ADR-020 §9).
    pub metadata: HashMap<String, String>,
}

impl Resource {
    /// Create a new resource in the `Available` state with no holders.
    pub fn new(
        id: ResourceId,
        name: String,
        description: Option<String>,
        device_serial: String,
        access_mode: AccessMode,
    ) -> Self {
        Resource {
            id,
            name,
            description,
            device_serial,
            state: ResourceState::Available,
            holders: Vec::new(),
            access_mode: Some(access_mode),
            locked_by: None,
            interfaces: InterfaceDeclaration::default(),
            endpoints: None,
            address_ranges: Vec::new(),
            metadata: HashMap::new(),
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
// Access Denied Error (ADR-024)
// ============================================================================

/// Reason why an endpoint or address access check was denied.
///
/// This error type is used by the runtime enforcement system (ADR-024) to
/// explain why a user's FPGA operation was rejected. It is distinct from
/// [`AcquisitionDenied`], which covers resource *acquisition* decisions.
///
/// ## When is this used?
///
/// When `endpoint_enforcement` is set to `"warn"` or `"enforce"`, every
/// Classic endpoint operation (wire/trigger/pipe) and AXI address operation
/// is checked against the resource registry. If the check fails, an
/// `AccessDenied` error is returned with a human-readable explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessDenied {
    /// No user identity was provided on the request.
    ///
    /// When enforcement is active (`warn` or `enforce`), every request must
    /// include a user identity (`X-User-Id` header for REST, `x-user-id`
    /// metadata for gRPC).
    NoUserId,

    /// The user does not hold any resource on the target device.
    ///
    /// This means the user has not acquired any resource on the device they
    /// are trying to operate on.
    NoResourceOnDevice {
        user_id: String,
        device_serial: String,
    },

    /// The user holds a resource on the device, but it does not cover the
    /// specific endpoint or address being accessed.
    NoCoveringResource {
        user_id: String,
        device_serial: String,
        /// Human-readable description of what was denied, e.g.,
        /// "endpoint 0x00 (wire_in)" or "AXI address 0x44A60000".
        detail: String,
    },

    /// The resource covering this endpoint is locked by another user.
    ///
    /// A locked resource can ONLY be used by the user who holds the lock.
    /// Other holders remain in the holders list but their operations are
    /// rejected until the resource is unlocked.
    LockedByOther {
        user_id: String,
        locked_by: String,
        resource_id: String,
    },
}

impl fmt::Display for AccessDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessDenied::NoUserId => {
                write!(f, "User identity required (X-User-Id header missing)")
            }
            AccessDenied::NoResourceOnDevice {
                user_id,
                device_serial,
            } => {
                write!(
                    f,
                    "User '{}' does not hold any resource on device '{}'",
                    user_id, device_serial
                )
            }
            AccessDenied::NoCoveringResource {
                user_id,
                device_serial,
                detail,
            } => {
                write!(
                    f,
                    "User '{}' does not hold a resource covering {} on device '{}'",
                    user_id, detail, device_serial
                )
            }
            AccessDenied::LockedByOther {
                user_id,
                locked_by,
                resource_id,
            } => {
                write!(
                    f,
                    "Resource '{}' is locked by '{}' — user '{}' cannot access it",
                    resource_id, locked_by, user_id
                )
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

    // ========================================================================
    // Phase 22B Stubs — Override these for HDL-specific access control
    // ========================================================================

    /// Validate whether a user can access a specific AXI address.
    ///
    /// Called by the REST/gRPC AXI handlers before issuing an AXI read/write.
    /// The default implementation **always allows access** (`Ok(())`).
    ///
    /// Override this to implement HDL-specific address range protection. For
    /// example, you could check the resource's `address_ranges` declarations
    /// and deny access if the user doesn't hold an address range that contains
    /// the requested address.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// impl ResourceAccessPolicy for MyHdlPolicy {
    ///     fn validate_address_access(
    ///         &self,
    ///         resource: &Resource,
    ///         user: &UserId,
    ///         address: u64,
    ///     ) -> Result<(), AcquisitionDenied> {
    ///         // Check if the requested address falls within any of the
    ///         // resource's declared address ranges.
    ///         for range in &resource.address_ranges {
    ///             if address >= range.start && address <= range.end {
    ///                 return Ok(());
    ///             }
    ///         }
    ///         Err(AcquisitionDenied::Custom {
    ///             reason: format!("Address 0x{:08X} not in declared ranges", address),
    ///         })
    ///     }
    /// }
    /// ```
    fn validate_address_access(
        &self,
        _resource: &Resource,
        _user: &UserId,
        _address: u64,
    ) -> Result<(), AcquisitionDenied> {
        Ok(()) // Stub: allow all addresses by default
    }

    /// Validate whether a user can access a specific Classic endpoint.
    ///
    /// Called by the REST/gRPC Classic handlers before WireIn/WireOut/
    /// TriggerIn/TriggerOut/PipeIn/PipeOut operations. The default
    /// implementation **always allows access** (`Ok(())`).
    ///
    /// Override this to enforce per-endpoint access rules declared in the
    /// resource's `EndpointDeclaration`. For example, deny access if an
    /// endpoint is declared `exclusive` and is held by another user.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// impl ResourceAccessPolicy for MyHdlPolicy {
    ///     fn validate_endpoint_access(
    ///         &self,
    ///         resource: &Resource,
    ///         user: &UserId,
    ///         endpoint: u8,
    ///         ep_type: &EndpointType,
    ///     ) -> Result<(), AcquisitionDenied> {
    ///         if let Some(ref ep_decl) = resource.endpoints {
    ///             for entry in &ep_decl.entries {
    ///                 if entry.address == endpoint {
    ///                     // Found the endpoint — check access mode
    ///                     // ... your HDL-specific logic here ...
    ///                 }
    ///             }
    ///         }
    ///         Ok(())
    ///     }
    /// }
    /// ```
    fn validate_endpoint_access(
        &self,
        _resource: &Resource,
        _user: &UserId,
        _endpoint: u8,
        _ep_type: &EndpointType,
    ) -> Result<(), AcquisitionDenied> {
        Ok(()) // Stub: allow all endpoints by default
    }

    /// Process a custom metadata key during resource registration.
    ///
    /// Called once during `create_resource` for each key in the resource's
    /// `metadata` map. The default implementation **accepts all keys**
    /// without validation (`Ok(())`).
    ///
    /// Override this to validate or act on HDL-specific metadata. For
    /// example, you could require certain metadata keys to be present
    /// or reject unknown keys.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// impl ResourceAccessPolicy for MyHdlPolicy {
    ///     fn on_metadata_key(
    ///         &self,
    ///         resource: &Resource,
    ///         key: &str,
    ///         value: &str,
    ///     ) -> Result<(), String> {
    ///         match key {
    ///             "bitfile_version" => {
    ///                 // Validate version format
    ///                 if !value.starts_with("v") {
    ///                     return Err("bitfile_version must start with 'v'".into());
    ///                 }
    ///                 Ok(())
    ///             }
    ///             _ => Ok(()), // Accept unknown keys
    ///         }
    ///     }
    /// }
    /// ```
    fn on_metadata_key(
        &self,
        _resource: &Resource,
        _key: &str,
        _value: &str,
    ) -> Result<(), String> {
        Ok(()) // Stub: accept all metadata by default
    }
}

// ============================================================================
// Default Access Policy
// ============================================================================

/// The default resource access policy implementing the rules from ADR-006
/// and ADR-020.
///
/// This is a **stub** that developers can replace with HDL-specific logic
/// by implementing the [`ResourceAccessPolicy`] trait.
///
/// ## ADR-020 Acquisition Rules
///
/// The access mode is now a design-level constraint on the resource, not
/// a user's runtime choice. The `requested_mode` parameter is used for
/// backward compatibility but should match the resource's `access_mode`.
///
/// - `Available` → allow acquire
/// - `InUseShared` (resource `access_mode == Shared`) → allow if not locked
/// - `InUseLocked` → deny unless it's a shared resource temporarily locked
///   (the lock holder blocks others until unlock)
/// - `Unavailable` or `Initializing` → deny
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
                // Available resources can be acquired.
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
                // Locked — deny everyone.
                let held_by = resource
                    .locked_by
                    .clone()
                    .or_else(|| resource.holders.first().cloned())
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

    /// Remove all resources for a specific device serial.
    ///
    /// Used when the FPGA is re-programmed: all resources for the device
    /// become invalid because the bitfile has changed.
    ///
    /// Returns the number of resources removed.
    pub fn remove_for_device(&self, serial: &str) -> usize {
        let mut map = self.resources.write().unwrap();
        let ids_to_remove: Vec<String> = map
            .values()
            .filter(|r| r.device_serial == serial)
            .map(|r| r.id.clone())
            .collect();
        let count = ids_to_remove.len();
        for id in ids_to_remove {
            map.remove(&id);
        }
        count
    }

    /// Release all resources held by a specific user, across all devices.
    ///
    /// Used by the `POST /disconnect` endpoint (Phase 25-Fix-H, F10) to
    /// cleanly release everything a user holds in one atomic operation.
    /// Each resource transitions back to `Available` if no other holders
    /// remain, following the same logic as `release()`.
    ///
    /// Returns the number of resources that were actually released.
    pub fn release_all_for_user(&self, user_id: &UserId) -> usize {
        let mut map = self.resources.write().unwrap();
        let mut released = 0;

        for resource in map.values_mut() {
            let before = resource.holders.len();
            resource.holders.retain(|h| h != user_id);
            let removed = resource.holders.len() < before;

            if !removed {
                continue;
            }

            released += 1;

            // If the releasing user was the locker, clear the lock.
            if resource.locked_by.as_deref() == Some(user_id) {
                resource.locked_by = None;
            }

            if resource.holders.is_empty() {
                // No more holders — transition back to Available.
                resource.state = ResourceState::Available;
                resource.locked_by = None;
            } else if resource.locked_by.is_none()
                && resource.access_mode == Some(AccessMode::Shared)
            {
                // Shared resource, locker left, back to InUseShared.
                resource.state = ResourceState::InUseShared;
            }
        }

        released
    }

    /// Attempt to acquire a resource for a user (ADR-020 §3).
    ///
    /// The access mode is determined by the resource's `access_mode` field
    /// (set at registration time), not by the caller. The `mode` parameter
    /// is used for backward compatibility and should match the resource's
    /// declared mode.
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

        // Use the resource's declared access mode if available,
        // falling back to the caller-provided mode for compatibility.
        let effective_mode = resource.access_mode.unwrap_or(mode);
        resource.state = match effective_mode {
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

            // If the releasing user was the locker, clear the lock.
            if removed && resource.locked_by.as_deref() == Some(user_id) {
                resource.locked_by = None;
            }

            if removed && resource.holders.is_empty() {
                // No more holders — transition back to Available.
                resource.state = ResourceState::Available;
                resource.locked_by = None;
            } else if removed
                && resource.locked_by.is_none()
                && resource.access_mode == Some(AccessMode::Shared)
            {
                // Shared resource, locker left, back to InUseShared.
                resource.state = ResourceState::InUseShared;
            }

            removed
        } else {
            false
        }
    }

    /// Lock a shared resource for exclusive use by a holder (ADR-020 §3).
    ///
    /// Preconditions:
    /// - Resource must exist and be in `InUseShared` state.
    /// - `user_id` must be a current holder.
    /// - Resource's `access_mode` must be `Shared` (exclusive resources are
    ///   always implicitly locked).
    ///
    /// Returns `Ok(())` on success, or an error string.
    pub fn lock(&self, resource_id: &str, user_id: &UserId) -> Result<(), String> {
        let mut map = self.resources.write().unwrap();
        let resource = map.get_mut(resource_id).ok_or_else(|| {
            format!("Resource '{}' not found", resource_id)
        })?;

        if resource.access_mode != Some(AccessMode::Shared) {
            return Err("Cannot lock/unlock an exclusive resource — it is always implicitly locked".to_string());
        }

        if !resource.holders.contains(user_id) {
            return Err(format!(
                "User '{}' is not a holder of resource '{}'",
                user_id, resource_id
            ));
        }

        if resource.locked_by.is_some() {
            return Err(format!(
                "Resource '{}' is already locked by '{}'",
                resource_id,
                resource.locked_by.as_deref().unwrap_or("unknown")
            ));
        }

        resource.locked_by = Some(user_id.clone());
        resource.state = ResourceState::InUseLocked;
        Ok(())
    }

    /// Unlock a previously locked shared resource (ADR-020 §3).
    ///
    /// Preconditions:
    /// - Resource must exist and be in `InUseLocked` state.
    /// - `user_id` must be the user who locked the resource.
    ///
    /// Returns `Ok(())` on success, or an error string.
    pub fn unlock(&self, resource_id: &str, user_id: &UserId) -> Result<(), String> {
        let mut map = self.resources.write().unwrap();
        let resource = map.get_mut(resource_id).ok_or_else(|| {
            format!("Resource '{}' not found", resource_id)
        })?;

        if resource.access_mode != Some(AccessMode::Shared) {
            return Err("Cannot lock/unlock an exclusive resource — it is always implicitly locked".to_string());
        }

        if resource.locked_by.as_deref() != Some(user_id.as_str()) {
            return Err(format!(
                "User '{}' did not lock resource '{}'",
                user_id, resource_id
            ));
        }

        resource.locked_by = None;
        resource.state = ResourceState::InUseShared;
        Ok(())
    }

    /// Delete all resources for a specific device (ADR-020 §4).
    ///
    /// Called when a device is disconnected. Resources are **deleted**
    /// (not just marked Unavailable) because the device state is unknown
    /// after reconnection — it may have lost power or been reprogrammed.
    ///
    /// Returns the list of deleted resources (for event notifications).
    pub fn invalidate_device(&self, serial: &str) -> Vec<Resource> {
        let mut map = self.resources.write().unwrap();
        let mut deleted = Vec::new();

        // Collect resource IDs to delete (can't mutate while iterating).
        let ids_to_delete: Vec<ResourceId> = map
            .values()
            .filter(|r| r.device_serial == serial)
            .map(|r| r.id.clone())
            .collect();

        for id in ids_to_delete {
            if let Some(resource) = map.remove(&id) {
                // Notify the policy before deleting.
                self.policy
                    .on_resource_invalidated(&resource, &resource.holders);
                deleted.push(resource);
            }
        }

        deleted
    }

    // ========================================================================
    // Endpoint Access Enforcement (ADR-024)
    // ========================================================================

    /// Check if a user holds ANY resource on the given device.
    ///
    /// This is the **device-level gate** used by the REST middleware (ADR-024
    /// §4, Tier 3) for early rejection. It answers: "Has this user acquired
    /// at least one resource on this device?"
    ///
    /// It does NOT check endpoint coverage or lock status — that is the
    /// handler's job via [`check_endpoint_access()`] or [`check_address_access()`].
    ///
    /// Returns `Ok(())` if the user holds at least one resource on the device,
    /// or `Err(AccessDenied::NoResourceOnDevice)` if not.
    pub fn check_device_access(
        &self,
        user_id: &str,
        device_serial: &str,
    ) -> Result<(), AccessDenied> {
        let map = self.resources.read().unwrap();

        // Look for ANY resource on this device where the user is a holder.
        let has_resource = map.values().any(|r| {
            r.device_serial == device_serial && r.holders.contains(&user_id.to_string())
        });

        if has_resource {
            Ok(())
        } else {
            Err(AccessDenied::NoResourceOnDevice {
                user_id: user_id.to_string(),
                device_serial: device_serial.to_string(),
            })
        }
    }

    /// Check if a user is authorized to access a specific Classic endpoint.
    ///
    /// This implements the **authorization rule** from ADR-024 §2:
    ///
    /// 1. Find all resources on `device_serial` where `user_id ∈ holders`
    /// 2. For each held resource:
    ///    - If `InUseLocked` and `locked_by != user_id` → DENY (locked by other)
    ///    - Check endpoint coverage:
    ///      - No `endpoints` field (legacy resource) → ALLOW (covers all EPs)
    ///      - Explicit entry with matching address → ALLOW
    ///      - No matching entry, but `default_access` exists → ALLOW
    ///      - `entries` is empty and `default_access` exists → ALLOW
    /// 3. If no held resource covers the endpoint → DENY
    ///
    /// Returns `Ok(())` if authorized, or `Err(AccessDenied)` with the reason.
    pub fn check_endpoint_access(
        &self,
        user_id: &str,
        device_serial: &str,
        endpoint: u8,
        ep_type: &EndpointType,
    ) -> Result<(), AccessDenied> {
        let map = self.resources.read().unwrap();

        // Find all resources on this device held by this user.
        let held_resources: Vec<&Resource> = map
            .values()
            .filter(|r| {
                r.device_serial == device_serial
                    && r.holders.contains(&user_id.to_string())
            })
            .collect();

        if held_resources.is_empty() {
            return Err(AccessDenied::NoResourceOnDevice {
                user_id: user_id.to_string(),
                device_serial: device_serial.to_string(),
            });
        }

        // Check each held resource for endpoint coverage.
        for resource in &held_resources {
            // Lock enforcement (ADR-024 §6): if the resource is locked by
            // someone else, this user cannot use it even though they are a
            // holder. Skip to the next resource.
            if resource.state == ResourceState::InUseLocked {
                if let Some(ref locker) = resource.locked_by {
                    if locker != user_id {
                        // This resource is locked by another user — skip it.
                        // We don't immediately deny because the user might hold
                        // another resource that covers this endpoint.
                        continue;
                    }
                }
            }

            // Check endpoint coverage (ADR-024 §2 + §3).
            match &resource.endpoints {
                None => {
                    // Legacy resource: no endpoint declarations at all.
                    // Covers ALL endpoints on the device (ADR-024 §3).
                    return Ok(());
                }
                Some(ep_decl) => {
                    // Check explicit entries first.
                    let explicit_match = ep_decl.entries.iter().any(|entry| {
                        entry.address == endpoint && entry.ep_type == *ep_type
                    });
                    if explicit_match {
                        return Ok(());
                    }

                    // No explicit match — check if default_access covers it.
                    // If there IS an EndpointDeclaration with a default_access,
                    // then any endpoint not explicitly listed is covered by the
                    // default. Even if entries is empty, the default covers all.
                    // (The EndpointDeclaration always has a default_access field.)
                    return Ok(());
                }
            }
        }

        // If we got here, every held resource was either locked by someone
        // else, or didn't cover the endpoint.
        //
        // Check if ALL held resources were locked-by-other (distinct error).
        let all_locked = held_resources.iter().all(|r| {
            r.state == ResourceState::InUseLocked
                && r.locked_by.as_deref() != Some(user_id)
        });

        if all_locked {
            // Report the first locked resource for the error message.
            let first = held_resources[0];
            Err(AccessDenied::LockedByOther {
                user_id: user_id.to_string(),
                locked_by: first
                    .locked_by
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                resource_id: first.id.clone(),
            })
        } else {
            Err(AccessDenied::NoCoveringResource {
                user_id: user_id.to_string(),
                device_serial: device_serial.to_string(),
                detail: format!("endpoint 0x{:02X} ({})", endpoint, ep_type),
            })
        }
    }

    /// Check if a user is authorized to access a specific AXI address.
    ///
    /// Same logic as [`check_endpoint_access()`] but for AXI address ranges
    /// instead of Classic endpoints. Uses the resource's `address_ranges`
    /// field to determine coverage.
    ///
    /// For resources without `address_ranges`, the resource is considered to
    /// cover ALL addresses (legacy mode, same as §3 for endpoints).
    pub fn check_address_access(
        &self,
        user_id: &str,
        device_serial: &str,
        address: u64,
    ) -> Result<(), AccessDenied> {
        let map = self.resources.read().unwrap();

        let held_resources: Vec<&Resource> = map
            .values()
            .filter(|r| {
                r.device_serial == device_serial
                    && r.holders.contains(&user_id.to_string())
            })
            .collect();

        if held_resources.is_empty() {
            return Err(AccessDenied::NoResourceOnDevice {
                user_id: user_id.to_string(),
                device_serial: device_serial.to_string(),
            });
        }

        for resource in &held_resources {
            // Lock enforcement — same as check_endpoint_access.
            if resource.state == ResourceState::InUseLocked {
                if let Some(ref locker) = resource.locked_by {
                    if locker != user_id {
                        continue;
                    }
                }
            }

            // Address coverage check.
            if resource.address_ranges.is_empty() {
                // No address ranges declared — legacy resource covers all
                // addresses (same logic as §3 for endpoints).
                return Ok(());
            }

            // Check if the requested address falls within any declared range.
            let in_range = resource.address_ranges.iter().any(|range| {
                address >= range.start as u64 && address <= range.end as u64
            });
            if in_range {
                return Ok(());
            }
        }

        // Same locked-by-other vs not-covering distinction.
        let all_locked = held_resources.iter().all(|r| {
            r.state == ResourceState::InUseLocked
                && r.locked_by.as_deref() != Some(user_id)
        });

        if all_locked {
            let first = held_resources[0];
            Err(AccessDenied::LockedByOther {
                user_id: user_id.to_string(),
                locked_by: first
                    .locked_by
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                resource_id: first.id.clone(),
            })
        } else {
            Err(AccessDenied::NoCoveringResource {
                user_id: user_id.to_string(),
                device_serial: device_serial.to_string(),
                detail: format!("AXI address 0x{:08X}", address),
            })
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

    /// Create a shared test resource with the given ID.
    fn test_resource(id: &str) -> Resource {
        Resource::new(
            id.to_string(),
            format!("resource_{}", id),
            None,
            "ABC123".to_string(),
            AccessMode::Shared,
        )
    }

    /// Create an exclusive test resource with the given ID.
    fn test_exclusive_resource(id: &str) -> Resource {
        Resource::new(
            id.to_string(),
            format!("resource_{}", id),
            None,
            "ABC123".to_string(),
            AccessMode::Exclusive,
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
    fn acquire_exclusive_resource_blocks_others() {
        let registry = ResourceRegistry::new();
        registry.register(test_exclusive_resource("r1"));

        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap();

        // Exclusive resource is always locked — second user cannot acquire.
        let result = registry.acquire("r1", &"bob".to_string(), AccessMode::Exclusive);
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
    fn lock_unlock_shared_resource() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        // Alice and Bob both acquire the shared resource.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.acquire("r1", &"bob".to_string(), AccessMode::Shared).unwrap();

        // Alice locks it.
        assert!(registry.lock("r1", &"alice".to_string()).is_ok());
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);
        assert_eq!(r.locked_by, Some("alice".to_string()));

        // Bob cannot lock it (already locked).
        assert!(registry.lock("r1", &"bob".to_string()).is_err());

        // Alice unlocks it.
        assert!(registry.unlock("r1", &"alice".to_string()).is_ok());
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseShared);
        assert_eq!(r.locked_by, None);
    }

    #[test]
    fn lock_exclusive_resource_rejected() {
        let registry = ResourceRegistry::new();
        registry.register(test_exclusive_resource("r1"));

        registry.acquire("r1", &"alice".to_string(), AccessMode::Exclusive).unwrap();

        // Attempting to lock/unlock an exclusive resource is an error.
        assert!(registry.lock("r1", &"alice".to_string()).is_err());
        assert!(registry.unlock("r1", &"alice".to_string()).is_err());
    }

    #[test]
    fn invalidate_device_deletes_resources() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        let deleted = registry.invalidate_device("ABC123");

        // Resource is deleted, not just marked Unavailable.
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].id, "r1");
        assert!(registry.get("r1").is_none());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn list_for_device_filters_correctly() {
        let registry = ResourceRegistry::new();
        registry.register(Resource::new(
            "r1".to_string(),
            "res1".to_string(),
            None,
            "DEV_A".to_string(),
            AccessMode::Shared,
        ));
        registry.register(Resource::new(
            "r2".to_string(),
            "res2".to_string(),
            None,
            "DEV_B".to_string(),
            AccessMode::Shared,
        ));

        let dev_a_resources = registry.list_for_device("DEV_A");
        assert_eq!(dev_a_resources.len(), 1);
        assert_eq!(dev_a_resources[0].device_serial, "DEV_A");
    }

    #[test]
    fn release_lock_holder_clears_lock() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.acquire("r1", &"bob".to_string(), AccessMode::Shared).unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Alice releases (was the locker) — resource should go back to InUseShared.
        assert!(registry.release("r1", &"alice".to_string()));
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseShared);
        assert_eq!(r.locked_by, None);
        assert_eq!(r.holders, vec!["bob"]);
    }

    // ====================================================================
    // Phase 22B: ResourceAccessPolicy stub method tests
    // ====================================================================

    #[test]
    fn default_policy_validate_address_access_allows_all() {
        let policy = DefaultAccessPolicy;
        let resource = test_resource("r1");
        let user = "alice".to_string();
        // Any address should be allowed by default.
        assert!(policy.validate_address_access(&resource, &user, 0x0000_0000).is_ok());
        assert!(policy.validate_address_access(&resource, &user, 0xFFFF_FFFF).is_ok());
        assert!(policy.validate_address_access(&resource, &user, 0xDEAD_BEEF).is_ok());
    }

    #[test]
    fn default_policy_validate_endpoint_access_allows_all() {
        let policy = DefaultAccessPolicy;
        let resource = test_resource("r1");
        let user = "alice".to_string();
        // Any endpoint/type should be allowed by default.
        assert!(policy.validate_endpoint_access(&resource, &user, 0x00, &EndpointType::WireIn).is_ok());
        assert!(policy.validate_endpoint_access(&resource, &user, 0x20, &EndpointType::WireOut).is_ok());
        assert!(policy.validate_endpoint_access(&resource, &user, 0x40, &EndpointType::TriggerIn).is_ok());
        assert!(policy.validate_endpoint_access(&resource, &user, 0x80, &EndpointType::PipeIn).is_ok());
    }

    #[test]
    fn default_policy_on_metadata_key_accepts_all() {
        let policy = DefaultAccessPolicy;
        let resource = test_resource("r1");
        // Any metadata key/value should be accepted by default.
        assert!(policy.on_metadata_key(&resource, "bitfile_version", "v1.0").is_ok());
        assert!(policy.on_metadata_key(&resource, "custom_key", "any_value").is_ok());
        assert!(policy.on_metadata_key(&resource, "", "").is_ok());
    }

    // ========================================================================
    // remove_for_device tests (Phase 25-Fix-F, F3)
    // ========================================================================

    /// Helper: create a resource for a specific device serial.
    fn test_resource_for_device(id: &str, serial: &str) -> Resource {
        Resource::new(
            id.to_string(),
            format!("resource_{}", id),
            None,
            serial.to_string(),
            AccessMode::Shared,
        )
    }

    #[test]
    fn remove_for_device_removes_matching_resources() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry.register(test_resource_for_device("r2", "DEV_A"));
        registry.register(test_resource_for_device("r3", "DEV_B"));
        assert_eq!(registry.count(), 3);

        let removed = registry.remove_for_device("DEV_A");
        assert_eq!(removed, 2);
        assert_eq!(registry.count(), 1);
        // Only DEV_B resource should remain.
        assert!(registry.get("r3").is_some());
        assert!(registry.get("r1").is_none());
        assert!(registry.get("r2").is_none());
    }

    #[test]
    fn remove_for_device_returns_zero_when_no_match() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        assert_eq!(registry.count(), 1);

        let removed = registry.remove_for_device("DEV_NONEXISTENT");
        assert_eq!(removed, 0);
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn remove_for_device_on_empty_registry() {
        let registry = ResourceRegistry::new();
        let removed = registry.remove_for_device("DEV_A");
        assert_eq!(removed, 0);
        assert_eq!(registry.count(), 0);
    }

    // ========================================================================
    // release_all_for_user tests (Phase 25-Fix-H, F10)
    // ========================================================================

    #[test]
    fn release_all_for_user_releases_multiple_resources() {
        let registry = ResourceRegistry::new();
        let r1 = test_resource_for_device("r1", "DEV_A");
        let r2 = test_resource_for_device("r2", "DEV_B");
        registry.register(r1);
        registry.register(r2);
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.acquire("r2", &"alice".to_string(), AccessMode::Shared).unwrap();

        let released = registry.release_all_for_user(&"alice".to_string());
        assert_eq!(released, 2);
        // Both should be back to Available.
        assert_eq!(registry.get("r1").unwrap().state, ResourceState::Available);
        assert_eq!(registry.get("r2").unwrap().state, ResourceState::Available);
    }

    #[test]
    fn release_all_for_user_only_releases_held_resources() {
        let registry = ResourceRegistry::new();
        let r1 = test_resource_for_device("r1", "DEV_A");
        let r2 = test_resource_for_device("r2", "DEV_A");
        registry.register(r1);
        registry.register(r2);
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.acquire("r2", &"bob".to_string(), AccessMode::Shared).unwrap();

        let released = registry.release_all_for_user(&"alice".to_string());
        assert_eq!(released, 1);
        // r1 is Available (alice released), r2 still held by bob.
        assert_eq!(registry.get("r1").unwrap().state, ResourceState::Available);
        assert!(registry.get("r2").unwrap().holders.contains(&"bob".to_string()));
    }

    #[test]
    fn release_all_for_user_returns_zero_when_not_holding() {
        let registry = ResourceRegistry::new();
        let r1 = test_resource_for_device("r1", "DEV_A");
        registry.register(r1);
        registry.acquire("r1", &"bob".to_string(), AccessMode::Shared).unwrap();

        let released = registry.release_all_for_user(&"alice".to_string());
        assert_eq!(released, 0);
        // bob still holds r1.
        assert!(registry.get("r1").unwrap().holders.contains(&"bob".to_string()));
    }

    #[test]
    fn release_all_for_user_clears_lock_if_locker() {
        let registry = ResourceRegistry::new();
        let r1 = test_resource_for_device("r1", "DEV_A");
        registry.register(r1);
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.acquire("r1", &"bob".to_string(), AccessMode::Shared).unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        let released = registry.release_all_for_user(&"alice".to_string());
        assert_eq!(released, 1);
        // Lock should be cleared, bob still holds.
        let r = registry.get("r1").unwrap();
        assert!(r.locked_by.is_none());
        assert!(r.holders.contains(&"bob".to_string()));
        assert_eq!(r.state, ResourceState::InUseShared);
    }

    #[test]
    fn release_all_for_user_on_empty_registry() {
        let registry = ResourceRegistry::new();
        let released = registry.release_all_for_user(&"alice".to_string());
        assert_eq!(released, 0);
    }

    // ========================================================================
    // ADR-024 Endpoint Access Enforcement tests
    // ========================================================================

    // --- check_device_access ---

    #[test]
    fn check_device_access_allows_holder() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Alice holds a resource on DEV_A — should be allowed.
        assert!(registry.check_device_access("alice", "DEV_A").is_ok());
    }

    #[test]
    fn check_device_access_denies_non_holder() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Bob does NOT hold a resource on DEV_A — should be denied.
        let err = registry.check_device_access("bob", "DEV_A").unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    #[test]
    fn check_device_access_denies_wrong_device() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Alice holds a resource on DEV_A, NOT on DEV_B.
        let err = registry.check_device_access("alice", "DEV_B").unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    #[test]
    fn check_device_access_on_empty_registry() {
        let registry = ResourceRegistry::new();
        let err = registry.check_device_access("alice", "DEV_A").unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    // --- check_endpoint_access ---

    #[test]
    fn check_endpoint_access_legacy_resource_covers_all() {
        // Legacy resource: no endpoints field → covers ALL endpoints.
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Any endpoint should be allowed because the resource has no
        // endpoint declarations (legacy mode, ADR-024 §3).
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn)
            .is_ok());
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0xA0, &EndpointType::PipeOut)
            .is_ok());
    }

    #[test]
    fn check_endpoint_access_denies_non_holder() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    #[test]
    fn check_endpoint_access_with_explicit_endpoints() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        // Declare that this resource covers WireIn at 0x00 and PipeOut at 0xA0.
        resource.endpoints = Some(EndpointDeclaration {
            default_access: AccessMode::Shared,
            entries: vec![
                EndpointEntry {
                    address: 0x00,
                    ep_type: EndpointType::WireIn,
                    access: AccessMode::Shared,
                    description: Some("Control register".to_string()),
                },
                EndpointEntry {
                    address: 0xA0,
                    ep_type: EndpointType::PipeOut,
                    access: AccessMode::Shared,
                    description: Some("Data readout".to_string()),
                },
            ],
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Declared endpoint — allowed.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn)
            .is_ok());
        // Also declared — allowed.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0xA0, &EndpointType::PipeOut)
            .is_ok());
        // Not declared, but EndpointDeclaration has default_access → allowed.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x01, &EndpointType::WireIn)
            .is_ok());
    }

    #[test]
    fn check_endpoint_access_locked_by_holder_allowed() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        // Alice locks the resource.
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Alice (lock holder) can still access.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn)
            .is_ok());
    }

    #[test]
    fn check_endpoint_access_locked_by_other_denied() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        // Alice locks the resource.
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob is a holder but the resource is locked by Alice — denied.
        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::LockedByOther { .. }));
    }

    #[test]
    fn check_endpoint_access_exclusive_resource_allows_holder() {
        // Exclusive resources are always implicitly locked by their holder.
        let registry = ResourceRegistry::new();
        let r = test_exclusive_resource_for_device("r1", "DEV_A");
        registry.register(r);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap();

        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn)
            .is_ok());
    }

    // --- check_address_access ---

    #[test]
    fn check_address_access_legacy_covers_all() {
        // Legacy resource: no address_ranges → covers ALL addresses.
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A6_0000)
            .is_ok());
    }

    #[test]
    fn check_address_access_in_range_allowed() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Address in range — allowed.
        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A6_1000)
            .is_ok());
        // Address at range boundary — allowed.
        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A6_0000)
            .is_ok());
        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A8_FFFF)
            .is_ok());
    }

    #[test]
    fn check_address_access_out_of_range_denied() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Address outside range — denied.
        let err = registry
            .check_address_access("alice", "DEV_A", 0x44A9_0000)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::NoCoveringResource { .. }));
    }

    #[test]
    fn check_address_access_denies_non_holder() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        let err = registry
            .check_address_access("bob", "DEV_A", 0x44A6_0000)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    #[test]
    fn check_address_access_locked_by_other_denied() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob is locked out.
        let err = registry
            .check_address_access("bob", "DEV_A", 0x44A6_1000)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::LockedByOther { .. }));
    }

    /// Helper: create an exclusive test resource for a specific device serial.
    fn test_exclusive_resource_for_device(id: &str, serial: &str) -> Resource {
        Resource::new(
            id.to_string(),
            format!("resource_{}", id),
            None,
            serial.to_string(),
            AccessMode::Exclusive,
        )
    }
}

