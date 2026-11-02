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
// Holder Entry (ADR-026)
// ============================================================================

/// Tracks a single holder's user ID and the mode in which they acquired.
///
/// Prior to ADR-026, `Resource::holders` was `Vec<UserId>`. Now it is
/// `Vec<HolderEntry>` so the enforcement system knows whether each holder
/// acquired as Shared (read+write), Exclusive (sole holder), or ReadOnly
/// (observe-only, no writes, no system disturbance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolderEntry {
    /// The user's identity string.
    pub user_id: UserId,
    /// The mode in which this user acquired the resource.
    pub mode: AccessMode,
}

impl HolderEntry {
    /// Create a new holder entry.
    pub fn new(user_id: UserId, mode: AccessMode) -> Self {
        HolderEntry { user_id, mode }
    }
}

impl fmt::Display for HolderEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.user_id, self.mode)
    }
}

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
///
/// ## Wire format
///
/// On the wire (JSON, SSE) a state is written in `snake_case`, exactly as the
/// OpenAPI `Resource.state` enum lists it: `unavailable`, `initializing`,
/// `available`, `in_use_shared`, `in_use_locked` (ADR-034, roadmap S-09).
/// The `serde(rename_all)` attribute below makes serde produce those strings,
/// and [`ResourceState::as_str`] returns the same text for code that builds
/// payloads by hand. `Display` stays human-readable ("In Use (Shared)") for
/// log and error messages only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl ResourceState {
    /// The spec (OpenAPI) string for this state, e.g. `"in_use_shared"`.
    ///
    /// Use this, not `Display` or `Debug`, whenever a state goes into an API
    /// response or event payload.
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceState::Unavailable => "unavailable",
            ResourceState::Initializing => "initializing",
            ResourceState::Available => "available",
            ResourceState::InUseShared => "in_use_shared",
            ResourceState::InUseLocked => "in_use_locked",
        }
    }
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
///
/// The same enum is used for two different things (ADR-034):
/// - the **declared** mode of a resource or item (`Shared` or `Exclusive`
///   only; `ReadOnly` is never a declared mode), and
/// - the **holder** mode a user acquired with (`Shared`, `Exclusive` or
///   `ReadOnly`).
///
/// On the wire the values are `shared`, `exclusive` and `read_only`
/// (`serde(rename_all = "snake_case")`, [`AccessMode::as_str`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    /// Multiple users can hold the resource simultaneously.
    /// Holders may temporarily `lock` the resource for uninterrupted access.
    Shared,

    /// Only one user can hold the resource at a time.
    /// The resource is always implicitly locked when acquired.
    Exclusive,

    /// Observe-only access — the holder can read output endpoints and address
    /// ranges but cannot perform any write operations or call operations that
    /// disturb the FPGA state (e.g., UpdateWireOuts, UpdateTriggerOuts).
    ///
    /// ReadOnly holders cannot lock a resource. They CAN coexist with Shared
    /// or Exclusive holders. See ADR-026.
    ReadOnly,
}

impl AccessMode {
    /// The spec string for this mode: `"shared"`, `"exclusive"` or `"read_only"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            AccessMode::Shared => "shared",
            AccessMode::Exclusive => "exclusive",
            AccessMode::ReadOnly => "read_only",
        }
    }

    /// Parse a **holder** mode string (`shared`, `exclusive`, `read_only`).
    ///
    /// Returns `None` for anything else, so the caller can answer with a
    /// validation error (422 / `INVALID_ARGUMENT`).
    pub fn parse_holder(s: &str) -> Option<AccessMode> {
        match s {
            "shared" => Some(AccessMode::Shared),
            "exclusive" => Some(AccessMode::Exclusive),
            "read_only" => Some(AccessMode::ReadOnly),
            _ => None,
        }
    }

    /// Parse a **declared** mode string (`shared` or `exclusive` only).
    ///
    /// `read_only` is a way to *acquire* a resource, not a way to *declare*
    /// one, so it is rejected here (ADR-034, D59).
    pub fn parse_declared(s: &str) -> Option<AccessMode> {
        match s {
            "shared" => Some(AccessMode::Shared),
            "exclusive" => Some(AccessMode::Exclusive),
            _ => None,
        }
    }
}

impl fmt::Display for AccessMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessMode::Shared => write!(f, "Shared"),
            AccessMode::Exclusive => write!(f, "Exclusive"),
            AccessMode::ReadOnly => write!(f, "ReadOnly"),
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
#[serde(rename_all = "snake_case")]
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
    /// Returns `true` for write-direction endpoints (host → FPGA).
    ///
    /// Input endpoints are WireIn, TriggerIn, PipeIn, and BtPipeIn. These push
    /// data from the host into the FPGA. A ReadOnly holder cannot use input
    /// endpoints because doing so modifies the FPGA state (ADR-026).
    pub fn is_input(&self) -> bool {
        matches!(
            self,
            EndpointType::WireIn
                | EndpointType::TriggerIn
                | EndpointType::PipeIn
                | EndpointType::BtPipeIn
        )
    }

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

    /// Parse the spec name of an endpoint type (`wire_in`, `bt_pipe_out`, ...).
    ///
    /// These are the same strings `Display` prints and the OpenAPI
    /// `EndpointEntry.type` enum lists. Returns `None` for unknown names.
    pub fn parse(s: &str) -> Option<EndpointType> {
        match s {
            "wire_in" => Some(EndpointType::WireIn),
            "wire_out" => Some(EndpointType::WireOut),
            "trigger_in" => Some(EndpointType::TriggerIn),
            "trigger_out" => Some(EndpointType::TriggerOut),
            "pipe_in" => Some(EndpointType::PipeIn),
            "pipe_out" => Some(EndpointType::PipeOut),
            "bt_pipe_in" => Some(EndpointType::BtPipeIn),
            "bt_pipe_out" => Some(EndpointType::BtPipeOut),
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
    /// If `true`, ReadOnly holders cannot access this endpoint (ADR-026).
    /// Private endpoints contain sensitive data that should not be observable
    /// by read-only users.
    #[serde(default)]
    pub private: bool,
}

/// Declaration of all Classic endpoints used by a resource (ADR-034 §3).
///
/// Three cases are kept apart on purpose (D55, D68):
/// - `Resource.endpoints == None` — the client sent no `endpoints` field: an
///   exclusive resource that claims every classic EP. The EPs are **not**
///   expanded into `entries`; coverage code treats `None` as "all".
/// - `Some` with empty `entries` — the resource claims no classic EP.
/// - `Some` with entries — the resource claims exactly the listed EPs.
///
/// `default_access` only fills in a missing `access` on a listed entry when
/// the declaration is built (D61); it never claims an unlisted EP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointDeclaration {
    /// Mode used for listed entries that did not give their own `access`.
    /// `None` when the client did not send `default_access`.
    #[serde(default)]
    pub default_access: Option<AccessMode>,
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
    /// Optional short name for the range (e.g. "csr"), shown in conflicts
    /// and on the dashboard.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Start of the range (inclusive), e.g. 0x44A6_0000.
    pub start: u32,
    /// End of the range (inclusive), e.g. 0x44A8_FFFF.
    pub end: u32,
    /// Whether this range is shared or exclusive.
    pub access: AccessMode,
    /// If `true`, ReadOnly holders cannot read addresses in this range (ADR-026).
    /// Private ranges contain sensitive data that should not be observable by
    /// read-only users. Applies to Classic (via resolve_classic_address), AXI,
    /// and okRegisterBridge operations.
    #[serde(default)]
    pub private: bool,
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
    /// Each entry records the user ID and the mode in which they acquired
    /// (Shared, Exclusive, or ReadOnly). See ADR-026.
    pub holders: Vec<HolderEntry>,

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

    /// If `true`, ReadOnly users cannot acquire this resource at all (ADR-026).
    /// This is the whole-resource-level Private flag. When set, the resource
    /// is invisible to read-only holders — they get `AcquisitionDenied::PrivateResource`.
    #[serde(default)]
    pub private: bool,
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
            private: false,
        }
    }

    /// Returns `true` if the resource can potentially be acquired.
    pub fn is_acquirable(&self) -> bool {
        matches!(
            self.state,
            ResourceState::Available | ResourceState::InUseShared
        )
    }

    /// Returns `true` if the given user is a holder (any mode).
    ///
    /// Convenience method that avoids exposing the `HolderEntry` internals
    /// to callers that only need to know if a user is present.
    pub fn has_holder(&self, user_id: &str) -> bool {
        self.holders.iter().any(|h| h.user_id == user_id)
    }

    /// Returns the list of user IDs for all holders (regardless of mode).
    ///
    /// This is a backward-compatibility helper for code that previously used
    /// `resource.holders` as `Vec<UserId>`. Returns owned strings.
    pub fn holder_ids(&self) -> Vec<&str> {
        self.holders.iter().map(|h| h.user_id.as_str()).collect()
    }

    /// Find the holder entry for a specific user.
    pub fn holder_entry(&self, user_id: &str) -> Option<&HolderEntry> {
        self.holders.iter().find(|h| h.user_id == user_id)
    }

    /// Returns `true` if `self` and `other` declare at least one common item
    /// (a Classic endpoint address declared by both, or an overlapping 32-bit
    /// `AddressRange`) (ADR-034 B1).
    pub fn shares_item_with(&self, other: &Resource) -> bool {
        // 1. Check Classic endpoint overlap.
        let classic_overlap = match (&self.endpoints, &other.endpoints) {
            (Some(a), Some(b)) => a
                .entries
                .iter()
                .any(|ea| b.entries.iter().any(|eb| ea.address == eb.address)),
            (Some(a), None) if other.interfaces.classic => {
                a.entries.iter().any(|ea| ea.address <= 0x7F)
            }
            (None, Some(b)) if self.interfaces.classic => {
                b.entries.iter().any(|eb| eb.address <= 0x7F)
            }
            (None, None) => self.interfaces.classic && other.interfaces.classic,
            _ => false,
        };
        if classic_overlap {
            return true;
        }

        // 2. Check 32-bit address range overlap.
        self.address_ranges
            .iter()
            .any(|ra| other.address_ranges.iter().any(|rb| ra.overlaps(rb)))
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
// Structured Conflict / Validation Error (ADR-034, D73)
// ============================================================================

/// What kind of problem a [`ResourceConflict`] describes (D73).
///
/// `Validation` means the request itself is wrong (REST 422,
/// gRPC `INVALID_ARGUMENT`). Every other code is a clash with existing
/// server state (REST 409, gRPC `FAILED_PRECONDITION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictCode {
    /// The new resource overlaps an item of an existing resource.
    Overlap,
    /// The resource is locked (explicitly or implicitly) by someone else.
    Locked,
    /// The resource is blocked by a lock on an overlapping resource (44-D).
    Blocked,
    /// The caller already holds the resource in a different mode, or
    /// requested a mode incompatible with the resource (44-C).
    ModeMismatch,
    /// The resource is marked `private`, so `read_only` acquisition is denied (44-C).
    Private,
    /// The request is invalid (missing field, bad type/address, ...).
    Validation,
}

impl ConflictCode {
    /// The spec string for this code, e.g. `"mode_mismatch"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictCode::Overlap => "overlap",
            ConflictCode::Locked => "locked",
            ConflictCode::Blocked => "blocked",
            ConflictCode::ModeMismatch => "mode_mismatch",
            ConflictCode::Private => "private",
            ConflictCode::Validation => "validation",
        }
    }
}

/// The kind of item a [`ResourceConflict`] points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictItemType {
    /// A Classic endpoint; `item` is its address, e.g. `"0x05"`.
    Endpoint,
    /// An address range; `item` is `"0xSTART-0xEND"`.
    AddressRange,
}

impl ConflictItemType {
    /// The spec string for this item type.
    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictItemType::Endpoint => "endpoint",
            ConflictItemType::AddressRange => "address_range",
        }
    }
}

/// Machine-readable detail attached to a 409 / 422 response (D73).
///
/// REST puts it in `ProblemDetail.conflict`; gRPC sends the same fields as a
/// `ResourceConflictDetail` message in the status details. Every field
/// except `code` is optional because not every error names a resource or an
/// item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceConflict {
    /// What went wrong.
    pub code: ConflictCode,
    /// The existing resource involved, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<ResourceId>,
    /// Name of that resource, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
    /// The kind of item involved, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<ConflictItemType>,
    /// The item itself (EP address or range), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// Every conflicting item with the named existing resource (EP as
    /// `0xNN`, range as `0xSTART-0xEND`); `item` stays the first one
    /// (44-B, owner decision Q4). Empty, and left out of the JSON, when
    /// there is no item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
}

impl ResourceConflict {
    /// A conflict with only a code; add details with the builder methods.
    ///
    /// This is the *builder* pattern: each `with_*` method takes `self` by
    /// value and returns it, so calls can be chained.
    pub fn new(code: ConflictCode) -> Self {
        ResourceConflict {
            code,
            resource_id: None,
            resource_name: None,
            item_type: None,
            item: None,
            items: Vec::new(),
        }
    }

    /// Name the existing resource involved.
    pub fn with_resource(mut self, id: &str, name: &str) -> Self {
        self.resource_id = Some(id.to_string());
        self.resource_name = Some(name.to_string());
        self
    }

    /// Name a Classic endpoint (formatted as `0xNN`).
    pub fn with_endpoint(mut self, address: u8) -> Self {
        self.item_type = Some(ConflictItemType::Endpoint);
        self.item = Some(format_ep_address(address));
        self
    }

    /// Name an address range (formatted as `0xSTART-0xEND`).
    pub fn with_range(mut self, start: u32, end: u32) -> Self {
        self.item_type = Some(ConflictItemType::AddressRange);
        self.item = Some(format!("{}-{}", format_u32_address(start), format_u32_address(end)));
        self
    }

    /// Attach the list of every conflicting item (see [`Self::items`]).
    pub fn with_items(mut self, items: Vec<String>) -> Self {
        self.items = items;
        self
    }
}

/// A resource operation failed: human text plus the structured detail.
///
/// The transport layer turns this into a REST `ProblemDetail` or a gRPC
/// `Status`. `conflict.code` decides the status: `Validation` → 422 /
/// `INVALID_ARGUMENT`, anything else → 409 / `FAILED_PRECONDITION`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceError {
    /// Human-readable explanation (goes into `ProblemDetail.detail`).
    pub detail: String,
    /// Machine-readable detail.
    pub conflict: ResourceConflict,
}

impl ResourceError {
    /// A validation (request) error with no item.
    pub fn validation(detail: impl Into<String>) -> Self {
        ResourceError {
            detail: detail.into(),
            conflict: ResourceConflict::new(ConflictCode::Validation),
        }
    }

    /// Returns `true` if this is a request error (422 / `INVALID_ARGUMENT`).
    pub fn is_validation(&self) -> bool {
        self.conflict.code == ConflictCode::Validation
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.detail)
    }
}

/// Format a Classic endpoint address the way the API writes it: `0x05`.
pub fn format_ep_address(address: u8) -> String {
    format!("0x{:02X}", address)
}

/// Format a 32-bit address the way the API writes it: `0x44A60000`.
pub fn format_u32_address(address: u32) -> String {
    format!("0x{:08X}", address)
}

// ============================================================================
// Holdings (per-user view, ADR-034 P5)
// ============================================================================

/// One resource held by a user, as shown in `UserSession.holdings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    /// The held resource.
    pub resource_id: ResourceId,
    /// The mode the user acquired with.
    pub mode: AccessMode,
    /// `true` if this user holds the resource's lock.
    pub locked: bool,
}

// ============================================================================
// Acquisition Outcome & Denied Error
// ============================================================================

/// What [`ResourceRegistry::acquire`] did when the call succeeded (ADR-034, A1).
///
/// Re-acquiring a resource in the **same** mode by the same user is an
/// idempotent no-op (`D28`, `D43`): the call returns `Ok(AlreadyHeld)` so
/// transport handlers can skip duplicate change events while still touching
/// the user's session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcquireOutcome {
    /// The user was added as a new holder of the resource.
    Acquired,
    /// The user already held the resource in the requested mode (no state change).
    AlreadyHeld,
}

/// Reason why a resource acquisition was denied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AcquisitionDenied {
    /// The resource is not in a state that allows acquisition.
    InvalidState { current_state: ResourceState },

    /// The resource is exclusively locked by another user.
    ExclusivelyHeld { held_by: UserId },

    /// The user requested exclusive mode but the resource is already shared.
    AlreadyShared { holder_count: usize },

    /// The user already holds the resource in a different mode ("release, then acquire", A1/D7/D43).
    HolderModeMismatch {
        user_id: UserId,
        existing_mode: AccessMode,
        requested_mode: AccessMode,
    },

    /// The requested mode is incompatible with the resource's declared `access_mode` (D62).
    DeclaredModeMismatch {
        declared: AccessMode,
        requested: AccessMode,
    },

    /// The resource is marked private — ReadOnly acquisition is denied (ADR-026).
    PrivateResource { resource_id: String },

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
            AcquisitionDenied::HolderModeMismatch {
                user_id,
                existing_mode,
                requested_mode,
            } => {
                write!(
                    f,
                    "User '{}' already holds the resource as '{}' (requested '{}') — release first, then acquire",
                    user_id,
                    existing_mode.as_str(),
                    requested_mode.as_str()
                )
            }
            AcquisitionDenied::DeclaredModeMismatch {
                declared,
                requested,
            } => {
                write!(
                    f,
                    "Resource access_mode is '{}', cannot acquire as '{}'",
                    declared.as_str(),
                    requested.as_str()
                )
            }
            AcquisitionDenied::PrivateResource { resource_id } => {
                write!(
                    f,
                    "Resource '{}' is private — ReadOnly acquisition denied",
                    resource_id
                )
            }
            AcquisitionDenied::Custom { reason } => {
                write!(f, "Acquisition denied: {}", reason)
            }
        }
    }
}

impl AcquisitionDenied {
    /// The structured D73 detail for this denial, when one of the D73 codes
    /// fits; `None` otherwise (the response then carries only text).
    ///
    /// - `ExclusivelyHeld` → `locked` (someone else holds the resource).
    /// - `AlreadyShared` / `HolderModeMismatch` / `DeclaredModeMismatch` →
    ///   `mode_mismatch` (ADR-034 A1, D62).
    /// - `PrivateResource` → `private` (ReadOnly requested on a private
    ///   resource, 44-C owner decision Q2).
    pub fn to_conflict(&self, resource: &Resource) -> Option<ResourceConflict> {
        let code = match self {
            AcquisitionDenied::ExclusivelyHeld { .. } => ConflictCode::Locked,
            AcquisitionDenied::AlreadyShared { .. }
            | AcquisitionDenied::HolderModeMismatch { .. }
            | AcquisitionDenied::DeclaredModeMismatch { .. } => ConflictCode::ModeMismatch,
            AcquisitionDenied::PrivateResource { .. } => ConflictCode::Private,
            _ => return None,
        };
        Some(ResourceConflict::new(code).with_resource(&resource.id, &resource.name))
    }
}

/// Why [`ResourceRegistry::try_register`] refused a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterError {
    /// A resource with the same ID (`{serial}:{name}`) already exists.
    /// REST 409 / gRPC `FAILED_PRECONDITION`, without a `conflict` object
    /// (no D73 code describes a duplicate name).
    Duplicate,
    /// The registration check failed (overlap or validation).
    Rejected(ResourceError),
}

// ============================================================================
// Lock / Unlock & Release Outcomes (ADR-034, Phase 44-D)
// ============================================================================

/// What [`ResourceRegistry::lock`] did when the call succeeded (ADR-034, 44-D).
///
/// Re-locking a resource when the caller already holds its lock is an
/// idempotent no-op (44-D Q2): the call returns [`LockOutcome::AlreadyLockedBySelf`]
/// so transport handlers can affirm the lock without emitting duplicate
/// `ResourceLocked` / `ResourceChanged` events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockOutcome {
    /// The caller just acquired the lock (`state` became `InUseLocked`).
    Locked,
    /// The caller already held the lock on this resource (no state change).
    AlreadyLockedBySelf,
}

/// Why a [`ResourceRegistry::lock`] or [`ResourceRegistry::unlock`] call failed (ADR-034, 44-D).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// The resource does not exist (REST 404 / gRPC `NOT_FOUND`).
    NotFound(String),
    /// The resource is not a `shared` resource, or is not held by the caller,
    /// or the caller holds it as `read_only`, or the caller is not the locker
    /// on unlock when no one else holds the lock (REST 409 / gRPC
    /// `FAILED_PRECONDITION` without a `conflict` object).
    PreconditionFailed(String),
    /// The resource is already locked by another user (`conflict.code = "locked"`, B3)
    /// or is blocked by another user's lock on an overlapping resource
    /// (`conflict.code = "blocked"`, B1/B3).
    Conflict(ResourceError),
}

impl LockError {
    /// Human-readable error message.
    pub fn detail(&self) -> &str {
        match self {
            LockError::NotFound(msg) | LockError::PreconditionFailed(msg) => msg,
            LockError::Conflict(err) => &err.detail,
        }
    }

    /// Optional structured [`ResourceConflict`] for 409 / `FAILED_PRECONDITION`.
    pub fn conflict(&self) -> Option<&ResourceConflict> {
        match self {
            LockError::Conflict(err) => Some(&err.conflict),
            _ => None,
        }
    }
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.detail())
    }
}

/// Information about a resource that was released by a user (44-D Q3).
///
/// Returned by [`ResourceRegistry::release_with_info`] and
/// [`ResourceRegistry::release_all_for_user_with_info`] so transport handlers
/// and the session sweep task can emit `ResourceUnlocked` whenever a release
/// implicitly ends a shared-resource lock, followed by `ResourceChanged`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasedResource {
    /// ID of the released resource.
    pub resource_id: ResourceId,
    /// `true` if the releasing user held an explicit shared-resource lock on
    /// this resource (`access_mode == Some(Shared)` and `locked_by == Some(user)`).
    pub was_locked: bool,
    /// State of the resource after the release (`Available` or `InUseShared`).
    pub new_state: ResourceState,
}

/// Find the first resource (in deterministic `id` ascending order) on the
/// same device that blocks `target` for `user_id` (ADR-034 B1, B4, D35).
///
/// Resource `target` is blocked for `user_id` when another resource `other`
/// (`other.id != target.id`, `other.device_serial == target.device_serial`)
/// is currently locked by a different user (`other.locked_by == Some(v)` with
/// `v != user_id`) and declares at least one common item with `target`.
fn find_blocking_resource<'a, I>(
    resources: I,
    target: &Resource,
    user_id: &str,
) -> Option<&'a Resource>
where
    I: IntoIterator<Item = &'a Resource>,
{
    let mut candidates: Vec<&'a Resource> = resources
        .into_iter()
        .filter(|r| {
            r.id != target.id
                && r.device_serial == target.device_serial
                && r.locked_by.as_deref().is_some_and(|locker| locker != user_id)
                && target.shares_item_with(r)
        })
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    candidates.into_iter().next()
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
// ============================================================================
// Classic Address Resolution (ADR-025)
// ============================================================================

/// Result of a developer-defined Classic endpoint → address resolution.
///
/// Returned by [`ResourceAccessPolicy::resolve_classic_address()`] to tell the
/// server how to enforce access for a Classic endpoint operation that may
/// target a memory-mapped address.
///
/// This hook fires ONLY after the server has already verified:
/// - The user is authenticated
/// - The user has acquired a resource covering this endpoint
/// - The resource is not locked by another user
///
/// The developer's only job is to resolve the endpoint operation to an
/// address (or indicate that no address mapping applies).
///
/// See also: [ADR-025](docs/architecture/adr/ADR-025-classic-address-mapping.md)
/// and the [Developer Guide](docs/guides/classic-address-mapping.md).
#[derive(Debug, Clone, PartialEq)]
pub enum ClassicAddressResolution {
    /// This endpoint is NOT an address carrier. Use standard endpoint-level
    /// enforcement only (the default). The `check_endpoint_access()` call
    /// returns `Ok(())` immediately (endpoint coverage was already confirmed
    /// by the upstream check).
    NoMapping,

    /// This endpoint operation targets the given memory-mapped address.
    /// The server will check whether this address falls within the
    /// resource's declared `address_ranges`, and if the range's access mode
    /// (Shared vs Exclusive) permits the operation.
    ///
    /// For Exclusive address ranges: the user must hold the resource's lock.
    /// For Shared address ranges: any holder may access.
    MapsToAddress(u64),

    /// This endpoint is infrastructure that any resource holder may use,
    /// regardless of address ranges. Skip address enforcement entirely.
    /// Use this for data registers, clock control, status readback, etc.
    AllowAlways,
}

// ============================================================================
// Access Denied Error (ADR-024)
// ============================================================================

/// Error returned when endpoint or address access is denied (ADR-024).
///
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

    /// The operation targets an exclusive address range but the user does
    /// not hold the lock on the resource (ADR-025).
    ///
    /// This happens when a developer's `resolve_classic_address()` hook
    /// maps a Classic endpoint operation to a memory-mapped address that
    /// falls within an Exclusive address range. Only the lock holder may
    /// access exclusive ranges on a shared resource.
    ExclusiveAddressRange {
        user_id: String,
        resource_id: String,
        address: u64,
        range_start: u32,
        range_end: u32,
    },

    /// The user acquired in ReadOnly mode but attempted a write operation
    /// or a state-disturbing operation like UpdateWireOuts (ADR-026).
    ReadOnlyWriteAttempt {
        user_id: String,
        resource_id: String,
        /// Human-readable description of the denied operation.
        detail: String,
    },

    /// The data is marked Private and the user is a ReadOnly holder (ADR-026).
    ///
    /// Private endpoints, address ranges, or resources cannot be read by
    /// ReadOnly holders. This applies to Classic, AXI, and okRegisterBridge.
    PrivateDataDenied {
        user_id: String,
        resource_id: String,
        /// Human-readable description of what was denied.
        detail: String,
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
            AccessDenied::ExclusiveAddressRange {
                user_id,
                resource_id,
                address,
                range_start,
                range_end,
            } => {
                write!(
                    f,
                    "User '{}' cannot access exclusive address range \
                     0x{:08X}–0x{:08X} on resource '{}' without holding the lock \
                     (resolved address: 0x{:08X})",
                    user_id, range_start, range_end, resource_id, address
                )
            }
            AccessDenied::ReadOnlyWriteAttempt {
                user_id,
                resource_id,
                detail,
            } => {
                write!(
                    f,
                    "User '{}' acquired resource '{}' as ReadOnly — \
                     write operation denied: {}",
                    user_id, resource_id, detail
                )
            }
            AccessDenied::PrivateDataDenied {
                user_id,
                resource_id,
                detail,
            } => {
                write!(
                    f,
                    "User '{}' acquired resource '{}' as ReadOnly — \
                     private data access denied: {}",
                    user_id, resource_id, detail
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

    // ========================================================================
    // ADR-025: Developer-Defined Classic Address Mapping
    // ========================================================================

    /// Resolve a Classic endpoint operation to a memory-mapped address.
    ///
    /// Called by the server AFTER upstream checks have confirmed:
    /// - User holds the resource
    /// - Resource is not locked by another user
    /// - Endpoint is covered by the resource's declarations
    ///
    /// The developer overrides this to express how their HDL design maps
    /// Classic endpoint operations to memory-mapped addresses.
    ///
    /// # Arguments
    ///
    /// * `resource` — the acquired resource (contains `address_ranges`)
    /// * `user` — the user performing the operation
    /// * `endpoint` — Classic endpoint address (0x00–0xFF)
    /// * `ep_type` — endpoint type (WireIn, WireOut, TriggerIn, etc.)
    /// * `value` — the value being written/triggered, if available:
    ///   - WireIn: the value being written (u32)
    ///   - TriggerIn: the trigger mask (u32)
    ///   - WireOut / TriggerOut: None (read — value not known before FFI)
    ///   - PipeIn / PipeOut: None (byte stream, not a single address)
    ///
    /// # Returns
    ///
    /// - `NoMapping` — no address involvement; standard enforcement applies
    /// - `MapsToAddress(addr)` — this operation targets `addr`; server checks
    ///   it against the resource's `address_ranges`
    /// - `AllowAlways` — skip address enforcement for this endpoint
    ///
    /// # Default
    ///
    /// Returns [`ClassicAddressResolution::NoMapping`] — no address mapping.
    /// All existing behavior is preserved.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// impl ResourceAccessPolicy for MyHdlPolicy {
    ///     fn resolve_classic_address(
    ///         &self,
    ///         _resource: &Resource,
    ///         _user: &UserId,
    ///         endpoint: u8,
    ///         ep_type: &EndpointType,
    ///         value: Option<u32>,
    ///     ) -> ClassicAddressResolution {
    ///         // WireIn 0x01 is the address register — the value IS the
    ///         // target address.
    ///         if endpoint == 0x01 && *ep_type == EndpointType::WireIn {
    ///             value.map_or(
    ///                 ClassicAddressResolution::NoMapping,
    ///                 |v| ClassicAddressResolution::MapsToAddress(v as u64),
    ///             )
    ///         } else {
    ///             ClassicAddressResolution::AllowAlways
    ///         }
    ///     }
    /// }
    /// ```
    fn resolve_classic_address(
        &self,
        _resource: &Resource,
        _user: &UserId,
        _endpoint: u8,
        _ep_type: &EndpointType,
        _value: Option<u32>,
    ) -> ClassicAddressResolution {
        ClassicAddressResolution::NoMapping
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
        user: &UserId,
        requested_mode: AccessMode,
    ) -> Result<(), AcquisitionDenied> {
        //Unavailable or Initializing resources cannot be acquired at all.
        if matches!(
            resource.state,
            ResourceState::Unavailable | ResourceState::Initializing
        ) {
            return Err(AcquisitionDenied::InvalidState {
                current_state: resource.state.clone(),
            });
        }

        // ADR-026 / 44-C Q2: Private resource check — ReadOnly acquisition is
        // never permitted on a private resource (`conflict.code = "private"`).
        if requested_mode == AccessMode::ReadOnly && resource.private {
            return Err(AcquisitionDenied::PrivateResource {
                resource_id: resource.id.clone(),
            });
        }

        // ADR-034 A1 (D2, D7, D28, D43): Repeat acquire by an existing holder.
        // - Same mode: idempotent success (even if the resource is locked).
        // - Different mode: 409 mode_mismatch ("release, then acquire").
        if let Some(existing) = resource.holder_entry(user) {
            if existing.mode == requested_mode {
                return Ok(());
            } else {
                return Err(AcquisitionDenied::HolderModeMismatch {
                    user_id: user.clone(),
                    existing_mode: existing.mode,
                    requested_mode,
                });
            }
        }

        // ADR-034 D62: A non-ReadOnly request must match the resource's
        // declared access_mode (e.g. requesting Shared on an Exclusive
        // resource or Exclusive on a Shared resource is a 409 mode_mismatch).
        if requested_mode != AccessMode::ReadOnly {
            if let Some(declared) = resource.access_mode {
                if requested_mode != declared {
                    return Err(AcquisitionDenied::DeclaredModeMismatch {
                        declared,
                        requested: requested_mode,
                    });
                }
            }
        }

        match &resource.state {
            ResourceState::Available => {
                // Available resources can be acquired in any compatible mode.
                Ok(())
            }
            ResourceState::InUseShared => {
                // Shared resources accept more Shared users and ReadOnly users.
                // Exclusive resources in InUseShared (because they have only
                // ReadOnly holders, D25b) accept an Exclusive holder (D25c).
                if requested_mode == AccessMode::Shared
                    || requested_mode == AccessMode::ReadOnly
                    || (requested_mode == AccessMode::Exclusive
                        && resource
                            .holders
                            .iter()
                            .all(|h| h.mode == AccessMode::ReadOnly))
                {
                    Ok(())
                } else {
                    Err(AcquisitionDenied::AlreadyShared {
                        holder_count: resource.holders.len(),
                    })
                }
            }
            ResourceState::InUseLocked => {
                // ADR-026 / D25a: ReadOnly users CAN join even a locked/exclusive
                // resource — they don't interfere with the holder's operations.
                if requested_mode == AccessMode::ReadOnly {
                    return Ok(());
                }
                // All other modes are denied on locked resources.
                let held_by = resource
                    .locked_by
                    .clone()
                    .or_else(|| {
                        resource
                            .holders
                            .iter()
                            .find(|h| h.mode != AccessMode::ReadOnly)
                            .or_else(|| resource.holders.first())
                            .map(|h| h.user_id.clone())
                    })
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

    /// Register a resource after running `check` against the resources that
    /// already exist on the same device (ADR-034 registration).
    ///
    /// The check and the insert happen while holding the registry's write
    /// lock, so two clients registering at the same moment cannot both pass
    /// the overlap check (no "time-of-check to time-of-use" gap).
    ///
    /// `check` is a closure: the caller passes the rule to apply (today
    /// [`crate::declaration::check_overlap`]). It receives the existing
    /// resources of the device and the new resource.
    ///
    /// # Errors
    /// - [`RegisterError::Duplicate`] if a resource with the same ID exists.
    /// - [`RegisterError::Rejected`] if `check` returned an error.
    pub fn try_register<F>(&self, resource: Resource, check: F) -> Result<(), RegisterError>
    where
        F: FnOnce(&[Resource], &Resource) -> Result<(), ResourceError>,
    {
        let mut map = self.resources.write().unwrap();
        if map.contains_key(&resource.id) {
            return Err(RegisterError::Duplicate);
        }
        let existing: Vec<Resource> = map
            .values()
            .filter(|r| r.device_serial == resource.device_serial)
            .cloned()
            .collect();
        check(&existing, &resource).map_err(RegisterError::Rejected)?;
        map.insert(resource.id.clone(), resource);
        Ok(())
    }

    /// Every resource `user_id` holds, with the mode and whether the user
    /// holds its lock (ADR-034 P5, `UserSession.holdings`).
    ///
    /// The list is sorted by resource ID so responses are stable.
    pub fn holdings_for(&self, user_id: &str) -> Vec<Holding> {
        let map = self.resources.read().unwrap();
        let mut holdings: Vec<Holding> = map
            .values()
            .filter_map(|r| {
                r.holder_entry(user_id).map(|h| Holding {
                    resource_id: r.id.clone(),
                    mode: h.mode,
                    locked: r.locked_by.as_deref() == Some(user_id),
                })
            })
            .collect();
        holdings.sort_by(|a, b| a.resource_id.cmp(&b.resource_id));
        holdings
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
        self.release_all_for_user_with_info(user_id).len()
    }

    /// Release all resources held by `user_id` across all devices and return
    /// per-resource details (sorted by `resource_id`) so transport handlers
    /// can broadcast `ResourceUnlocked` and `ResourceChanged` events (44-D Q3).
    pub fn release_all_for_user_with_info(&self, user_id: &UserId) -> Vec<ReleasedResource> {
        let mut map = self.resources.write().unwrap();
        let mut released = Vec::new();

        for resource in map.values_mut() {
            let before = resource.holders.len();
            resource.holders.retain(|h| h.user_id != *user_id);
            let removed = resource.holders.len() < before;

            if !removed {
                continue;
            }

            let was_locked = resource.access_mode == Some(AccessMode::Shared)
                && resource.locked_by.as_deref() == Some(user_id);

            // If the releasing user was the locker, clear the lock.
            if resource.locked_by.as_deref() == Some(user_id) {
                resource.locked_by = None;
            }

            if resource.holders.is_empty() {
                // No more holders — transition back to Available.
                resource.state = ResourceState::Available;
                resource.locked_by = None;
            } else if resource.locked_by.is_none() {
                // Locker (or exclusive holder) left while other holders
                // (e.g. Shared or ReadOnly) remain — back to InUseShared.
                resource.state = ResourceState::InUseShared;
            }

            released.push(ReleasedResource {
                resource_id: resource.id.clone(),
                was_locked,
                new_state: resource.state.clone(),
            });
        }

        released.sort_by(|a, b| a.resource_id.cmp(&b.resource_id));
        released
    }

    /// Attempt to acquire a resource for a user (ADR-020 §3, ADR-034 A1).
    ///
    /// Returns [`Ok(AcquireOutcome::Acquired)`] when a new holder entry was
    /// added, [`Ok(AcquireOutcome::AlreadyHeld)`] when the user already held
    /// the resource in the same mode (idempotent no-op, D28/D43), or
    /// `Err(AcquisitionDenied)` if the policy denied the request.
    pub fn acquire(
        &self,
        resource_id: &str,
        user_id: &UserId,
        mode: AccessMode,
    ) -> Result<AcquireOutcome, AcquisitionDenied> {
        let mut map = self.resources.write().unwrap();
        let resource = map
            .get(resource_id)
            .ok_or_else(|| AcquisitionDenied::Custom {
                reason: format!("Resource '{}' not found", resource_id),
            })?;

        // Check policy first (on immutable reference).
        self.policy.can_acquire(resource, user_id, mode)?;

        // Idempotent repeat acquire (A1, D28, D43): if the user already holds
        // the resource in the same mode, leave holders, state, and locked_by
        // untouched.
        if let Some(existing) = resource.holder_entry(user_id) {
            if existing.mode == mode {
                return Ok(AcquireOutcome::AlreadyHeld);
            }
        }

        // Policy approved — mutate the resource.
        let resource = map.get_mut(resource_id).unwrap();
        resource.holders.push(HolderEntry::new(user_id.clone(), mode));

        match mode {
            AccessMode::Exclusive => {
                // Exclusive resources are implicitly locked by their exclusive
                // holder (ADR-034, D25c, S-22).
                resource.state = ResourceState::InUseLocked;
                resource.locked_by = Some(user_id.clone());
            }
            AccessMode::Shared => {
                if resource.state != ResourceState::InUseLocked {
                    resource.state = ResourceState::InUseShared;
                }
            }
            AccessMode::ReadOnly => {
                // ReadOnly holders do NOT change InUseLocked or InUseShared
                // (D25a), but transition Available -> InUseShared (D25b).
                if resource.state == ResourceState::Available {
                    resource.state = ResourceState::InUseShared;
                }
            }
        }

        Ok(AcquireOutcome::Acquired)
    }

    /// Release a resource for a user.
    ///
    /// Returns `true` if the user was holding the resource and it was released.
    /// Returns `false` if the user was not holding the resource.
    pub fn release(&self, resource_id: &str, user_id: &UserId) -> bool {
        self.release_with_info(resource_id, user_id).is_some()
    }

    /// Release a resource for `user_id` and return details about whether an
    /// explicit shared-resource lock ended and the new resource state (44-D Q3).
    pub fn release_with_info(
        &self,
        resource_id: &str,
        user_id: &UserId,
    ) -> Option<ReleasedResource> {
        let mut map = self.resources.write().unwrap();
        let resource = map.get_mut(resource_id)?;

        let before = resource.holders.len();
        resource.holders.retain(|h| h.user_id != *user_id);
        let removed = resource.holders.len() < before;

        if !removed {
            return None;
        }

        let was_locked = resource.access_mode == Some(AccessMode::Shared)
            && resource.locked_by.as_deref() == Some(user_id);

        // If the releasing user was the locker, clear the lock.
        if resource.locked_by.as_deref() == Some(user_id) {
            resource.locked_by = None;
        }

        if resource.holders.is_empty() {
            // No more holders — transition back to Available.
            resource.state = ResourceState::Available;
            resource.locked_by = None;
        } else if resource.locked_by.is_none() {
            // Locker (or exclusive holder) left while other holders
            // (e.g. Shared or ReadOnly) remain — back to InUseShared.
            resource.state = ResourceState::InUseShared;
        }

        Some(ReleasedResource {
            resource_id: resource.id.clone(),
            was_locked,
            new_state: resource.state.clone(),
        })
    }

    /// If `resource_id` is blocked for `user_id` by another resource's lock on
    /// the same device (ADR-034 B1, B4, D35), returns the [`ResourceConflict`]
    /// (`code: ConflictCode::Blocked`, `resource_id`, `resource_name`).
    pub fn blocking_conflict_for(
        &self,
        resource_id: &str,
        user_id: &str,
    ) -> Option<ResourceConflict> {
        let map = self.resources.read().unwrap();
        let target = map.get(resource_id)?;
        let blocker = find_blocking_resource(map.values(), target, user_id)?;
        Some(
            ResourceConflict::new(ConflictCode::Blocked)
                .with_resource(&blocker.id, &blocker.name),
        )
    }

    /// Lock a shared resource for exclusive use by a holder (ADR-020 §3, ADR-034 B1–B4).
    ///
    /// Preconditions:
    /// - Resource must exist (`LockError::NotFound` otherwise).
    /// - Resource's `access_mode` must be `Shared` (`LockError::PreconditionFailed` otherwise).
    /// - `user_id` must be a current holder, and not in `ReadOnly` mode
    ///   (`LockError::PreconditionFailed` otherwise).
    /// - If `user_id` already holds the lock on this resource, returns
    ///   `Ok(LockOutcome::AlreadyLockedBySelf)` (idempotent, 44-D Q2).
    /// - If another user holds the lock on this resource, returns
    ///   `Err(LockError::Conflict)` with `ConflictCode::Locked` (B3).
    /// - If another user holds the lock on an overlapping resource on the
    ///   same device, returns `Err(LockError::Conflict)` with
    ///   `ConflictCode::Blocked` naming that resource (B1, B3, B4, D35).
    pub fn lock(&self, resource_id: &str, user_id: &UserId) -> Result<LockOutcome, LockError> {
        let mut map = self.resources.write().unwrap();
        let resource = map.get(resource_id).ok_or_else(|| {
            LockError::NotFound(format!("Resource '{}' not found", resource_id))
        })?;

        if resource.access_mode != Some(AccessMode::Shared) {
            return Err(LockError::PreconditionFailed(
                "Cannot lock/unlock an exclusive resource — it is always implicitly locked"
                    .to_string(),
            ));
        }

        let Some(entry) = resource.holder_entry(user_id) else {
            return Err(LockError::PreconditionFailed(format!(
                "User '{}' is not a holder of resource '{}'",
                user_id, resource_id
            )));
        };

        // ADR-026: ReadOnly holders cannot lock.
        if entry.mode == AccessMode::ReadOnly {
            return Err(LockError::PreconditionFailed(format!(
                "User '{}' acquired resource '{}' as ReadOnly — cannot lock",
                user_id, resource_id
            )));
        }

        // Check if this resource itself is already locked.
        if let Some(ref locker) = resource.locked_by {
            if locker == user_id {
                // 44-D Q2: Repeat lock by the current lock holder is an
                // idempotent success.
                return Ok(LockOutcome::AlreadyLockedBySelf);
            }
            return Err(LockError::Conflict(ResourceError {
                detail: format!(
                    "Resource '{}' is already locked by '{}'",
                    resource_id, locker
                ),
                conflict: ResourceConflict::new(ConflictCode::Locked)
                    .with_resource(&resource.id, &resource.name),
            }));
        }

        // B1 / B3 / B4 / D35: Check whether another resource on the same
        // device is locked by a different user and shares at least one item
        // with this resource.
        if let Some(blocker) = find_blocking_resource(map.values(), resource, user_id) {
            let blocker_locker = blocker.locked_by.as_deref().unwrap_or("unknown");
            return Err(LockError::Conflict(ResourceError {
                detail: format!(
                    "Resource '{}' is blocked for '{}' by resource '{}' (locked by '{}')",
                    resource_id, user_id, blocker.id, blocker_locker
                ),
                conflict: ResourceConflict::new(ConflictCode::Blocked)
                    .with_resource(&blocker.id, &blocker.name),
            }));
        }

        let resource = map.get_mut(resource_id).unwrap();
        resource.locked_by = Some(user_id.clone());
        resource.state = ResourceState::InUseLocked;
        Ok(LockOutcome::Locked)
    }

    /// Unlock a previously locked shared resource (ADR-020 §3).
    ///
    /// Preconditions:
    /// - Resource must exist (`LockError::NotFound` otherwise).
    /// - Resource's `access_mode` must be `Shared`.
    /// - `user_id` must be the user who locked the resource.
    pub fn unlock(&self, resource_id: &str, user_id: &UserId) -> Result<(), LockError> {
        let mut map = self.resources.write().unwrap();
        let resource = map.get_mut(resource_id).ok_or_else(|| {
            LockError::NotFound(format!("Resource '{}' not found", resource_id))
        })?;

        if resource.access_mode != Some(AccessMode::Shared) {
            return Err(LockError::PreconditionFailed(
                "Cannot lock/unlock an exclusive resource — it is always implicitly locked"
                    .to_string(),
            ));
        }

        if resource.locked_by.as_deref() != Some(user_id.as_str()) {
            if let Some(ref locker) = resource.locked_by {
                return Err(LockError::Conflict(ResourceError {
                    detail: format!(
                        "User '{}' did not lock resource '{}' (locked by '{}')",
                        user_id, resource_id, locker
                    ),
                    conflict: ResourceConflict::new(ConflictCode::Locked)
                        .with_resource(&resource.id, &resource.name),
                }));
            }
            return Err(LockError::PreconditionFailed(format!(
                "User '{}' did not lock resource '{}'",
                user_id, resource_id
            )));
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
                let holder_ids: Vec<UserId> = resource.holders.iter().map(|h| h.user_id.clone()).collect();
                self.policy
                    .on_resource_invalidated(&resource, &holder_ids);
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
            r.device_serial == device_serial && r.has_holder(user_id)
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
    /// 3. If endpoint is covered, call `policy.resolve_classic_address()`
    ///    (ADR-025). If the hook returns `MapsToAddress(addr)`, check the
    ///    resolved address against the resource's `address_ranges`.
    /// 4. If no held resource covers the endpoint → DENY
    ///
    /// The `value` parameter is the wire value / trigger mask / pipe data being
    /// written (if available). It is passed to the developer's
    /// `resolve_classic_address()` hook. See ADR-025 §4 for which endpoint
    /// types populate this field.
    ///
    /// Returns `Ok(())` if authorized, or `Err(AccessDenied)` with the reason.
    pub fn check_endpoint_access(
        &self,
        user_id: &str,
        device_serial: &str,
        endpoint: u8,
        ep_type: &EndpointType,
        value: Option<u32>,
    ) -> Result<(), AccessDenied> {
        let map = self.resources.read().unwrap();

        // Find all resources on this device held by this user.
        let held_resources: Vec<&Resource> = map
            .values()
            .filter(|r| {
                r.device_serial == device_serial
                    && r.has_holder(user_id)
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
            // Check endpoint coverage (ADR-024 §2 + §3).
            let endpoint_covered = match &resource.endpoints {
                None => {
                    // Legacy resource: no endpoint declarations at all.
                    // Covers ALL endpoints on the device (ADR-024 §3).
                    true
                }
                Some(ep_decl) => {
                    // Check explicit entries first.
                    let explicit_match = ep_decl.entries.iter().any(|entry| {
                        entry.address == endpoint && entry.ep_type == *ep_type
                    });
                    if explicit_match {
                        true
                    } else {
                        // No explicit match — default_access covers all
                        // endpoints not explicitly listed.
                        true
                    }
                }
            };

            if !endpoint_covered {
                continue;
            }

            // Endpoint is covered by this resource. Check ADR-026 ReadOnly
            // restrictions before lock enforcement so a ReadOnly holder
            // attempting a write on an Exclusive or locked resource gets
            // ReadOnlyWriteAttempt rather than LockedByOther.
            let holder_entry = resource.holder_entry(user_id);
            let is_readonly = holder_entry
                .map(|h| h.mode == AccessMode::ReadOnly)
                .unwrap_or(false);

            if is_readonly {
                // ADR-026 §5: ReadOnly holders cannot use input endpoints
                // (WireIn, TriggerIn, PipeIn, BtPipeIn).
                if ep_type.is_input() {
                    return Err(AccessDenied::ReadOnlyWriteAttempt {
                        user_id: user_id.to_string(),
                        resource_id: resource.id.clone(),
                        detail: format!(
                            "endpoint 0x{:02X} ({}) is an input endpoint",
                            endpoint, ep_type
                        ),
                    });
                }

                // ADR-026 §2: Private endpoints are hidden from ReadOnly.
                if let Some(ep_decl) = &resource.endpoints {
                    let entry = ep_decl.entries.iter().find(|e| {
                        e.address == endpoint && e.ep_type == *ep_type
                    });
                    if let Some(entry) = entry {
                        if entry.private {
                            return Err(AccessDenied::PrivateDataDenied {
                                user_id: user_id.to_string(),
                                resource_id: resource.id.clone(),
                                detail: format!(
                                    "endpoint 0x{:02X} ({}) is private",
                                    endpoint, ep_type
                                ),
                            });
                        }
                    }
                }
            }

            // Lock enforcement (ADR-024 §6, amended by Phase 33):
            // If the resource is locked by another user, WRITE operations
            // (input endpoints) are blocked. READ operations (output
            // endpoints) are still allowed for all holders, including
            // ReadOnly observers (ADR-026 §5).
            if resource.state == ResourceState::InUseLocked {
                if let Some(ref locker) = resource.locked_by {
                    if locker != user_id && ep_type.is_input() {
                        // Write operation on a resource locked by another
                        // user — skip it. We don't immediately deny because
                        // the user might hold another resource that covers
                        // this endpoint.
                        continue;
                    }
                }
            }

            // Call the developer's address resolution hook (ADR-025).
            //
            // At this point, upstream checks have passed:
            //   ✅ User holds this resource
            //   ✅ Lock check passed (writes blocked if locked by other)
            //   ✅ Endpoint is covered by the resource's declarations
            //   ✅ ReadOnly input/private checks passed (ADR-026)
            //
            // The hook's only job is to resolve endpoint + value → address.
            let resolution = self.policy.resolve_classic_address(
                resource, &user_id.to_string(), endpoint, ep_type, value,
            );

            match resolution {
                ClassicAddressResolution::NoMapping => {
                    // No address mapping — standard endpoint enforcement done.
                    return Ok(());
                }
                ClassicAddressResolution::AllowAlways => {
                    // Infrastructure endpoint — any holder may use.
                    return Ok(());
                }
                ClassicAddressResolution::MapsToAddress(addr) => {
                    // The endpoint operation targets a memory-mapped address.
                    // Check whether the address falls within the resource's
                    // declared address_ranges.
                    if resource.address_ranges.is_empty() {
                        // No address ranges declared — legacy resource covers
                        // all addresses (same as ADR-024 §3 for endpoints).
                        return Ok(());
                    }

                    // Find the matching address range.
                    let matching_range = resource.address_ranges.iter().find(|range| {
                        addr >= range.start as u64 && addr <= range.end as u64
                    });

                    match matching_range {
                        None => {
                            // Address not in any declared range — deny.
                            return Err(AccessDenied::NoCoveringResource {
                                user_id: user_id.to_string(),
                                device_serial: device_serial.to_string(),
                                detail: format!(
                                    "resolved address 0x{:08X} (from endpoint 0x{:02X} {})",
                                    addr, endpoint, ep_type
                                ),
                            });
                        }
                        Some(range) => {
                            // ADR-026: Private address ranges deny ReadOnly.
                            if is_readonly && range.private {
                                return Err(AccessDenied::PrivateDataDenied {
                                    user_id: user_id.to_string(),
                                    resource_id: resource.id.clone(),
                                    detail: format!(
                                        "address range 0x{:08X}–0x{:08X} is private",
                                        range.start, range.end
                                    ),
                                });
                            }

                            match range.access {
                                AccessMode::Shared | AccessMode::ReadOnly => {
                                    // Shared address range — any holder can access.
                                    return Ok(());
                                }
                                AccessMode::Exclusive => {
                                    // Exclusive address range — user must hold
                                    // the lock on this resource.
                                    let user_holds_lock = resource
                                        .locked_by
                                        .as_deref()
                                        == Some(user_id);

                                    if user_holds_lock {
                                        return Ok(());
                                    } else {
                                        return Err(AccessDenied::ExclusiveAddressRange {
                                            user_id: user_id.to_string(),
                                            resource_id: resource.id.clone(),
                                            address: addr,
                                            range_start: range.start,
                                            range_end: range.end,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // If we got here, every held resource was either:
        //   - locked by someone else AND the operation is a write, or
        //   - didn't cover the endpoint.
        //
        // Check if ALL held resources were locked-by-other for a write op.
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
    /// The `is_write` parameter controls lock enforcement: write operations
    /// are blocked when the resource is locked by another user, but read
    /// operations are allowed (ADR-024 §6, amended by Phase 33).
    ///
    /// For resources without `address_ranges`, the resource is considered to
    /// cover ALL addresses (legacy mode, same as §3 for endpoints).
    pub fn check_address_access(
        &self,
        user_id: &str,
        device_serial: &str,
        address: u64,
        is_write: bool,
    ) -> Result<(), AccessDenied> {
        let map = self.resources.read().unwrap();

        let held_resources: Vec<&Resource> = map
            .values()
            .filter(|r| {
                r.device_serial == device_serial
                    && r.has_holder(user_id)
            })
            .collect();

        if held_resources.is_empty() {
            return Err(AccessDenied::NoResourceOnDevice {
                user_id: user_id.to_string(),
                device_serial: device_serial.to_string(),
            });
        }

        for resource in &held_resources {
            // Lock enforcement (ADR-024 §6, amended by Phase 33):
            // Write operations are blocked when locked by another user.
            // Read operations are allowed for all holders.
            if resource.state == ResourceState::InUseLocked {
                if let Some(ref locker) = resource.locked_by {
                    if locker != user_id && is_write {
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

            // ADR-026: Check ReadOnly status for this holder.
            let holder_entry = resource.holder_entry(user_id);
            let is_readonly = holder_entry
                .map(|h| h.mode == AccessMode::ReadOnly)
                .unwrap_or(false);

            // Check if the requested address falls within any declared range.
            let matching_range = resource.address_ranges.iter().find(|range| {
                address >= range.start as u64 && address <= range.end as u64
            });

            if let Some(range) = matching_range {
                // ADR-026: Private address ranges deny ReadOnly holders.
                if is_readonly && range.private {
                    return Err(AccessDenied::PrivateDataDenied {
                        user_id: user_id.to_string(),
                        resource_id: resource.id.clone(),
                        detail: format!(
                            "address range 0x{:08X}–0x{:08X} is private",
                            range.start, range.end
                        ),
                    });
                }
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
        assert_eq!(r.holders.len(), 1);
        assert!(r.has_holder("alice"));
        assert_eq!(r.holders[0].mode, AccessMode::Shared);
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
        assert!(r.has_holder("alice"));
        assert!(r.has_holder("bob"));
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
        assert_eq!(r.holders.len(), 1);
        assert!(r.has_holder("bob"));
        assert_eq!(r.holders[0].mode, AccessMode::Shared);
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
        assert!(registry.get("r2").unwrap().has_holder("bob"));
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
        assert!(registry.get("r1").unwrap().has_holder("bob"));
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
        assert!(r.has_holder("bob"));
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
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn, None)
            .is_ok());
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0xA0, &EndpointType::PipeOut, None)
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
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn, None)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    #[test]
    fn check_endpoint_access_with_explicit_endpoints() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        // Declare that this resource covers WireIn at 0x00 and PipeOut at 0xA0.
        resource.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x00,
                    ep_type: EndpointType::WireIn,
                    access: AccessMode::Shared,
                    description: Some("Control register".to_string()),
                    private: false,
                },
                EndpointEntry {
                    address: 0xA0,
                    ep_type: EndpointType::PipeOut,
                    access: AccessMode::Shared,
                    description: Some("Data readout".to_string()),
                    private: false,
                },
            ],
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Declared endpoint — allowed.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn, None)
            .is_ok());
        // Also declared — allowed.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0xA0, &EndpointType::PipeOut, None)
            .is_ok());
        // Not declared, but EndpointDeclaration has default_access → allowed.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x01, &EndpointType::WireIn, None)
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
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn, None)
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
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn, None)
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
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn, None)
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
            .check_address_access("alice", "DEV_A", 0x44A6_0000, false)
            .is_ok());
    }

    #[test]
    fn check_address_access_in_range_allowed() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            name: None,
            description: None,
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
            private: false,
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Address in range — allowed.
        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A6_1000, false)
            .is_ok());
        // Address at range boundary — allowed.
        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A6_0000, false)
            .is_ok());
        assert!(registry
            .check_address_access("alice", "DEV_A", 0x44A8_FFFF, false)
            .is_ok());
    }

    #[test]
    fn check_address_access_out_of_range_denied() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            name: None,
            description: None,
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
            private: false,
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Address outside range — denied.
        let err = registry
            .check_address_access("alice", "DEV_A", 0x44A9_0000, false)
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
            .check_address_access("bob", "DEV_A", 0x44A6_0000, false)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::NoResourceOnDevice { .. }));
    }

    #[test]
    fn check_address_access_locked_by_other_denied() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            name: None,
            description: None,
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
            private: false,
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
            .check_address_access("bob", "DEV_A", 0x44A6_1000, true)
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

    // =========================================================================
    // ADR-025: Classic Address Mapping tests
    // =========================================================================

    /// Custom policy for ADR-025 tests.
    ///
    /// Maps WireIn 0x01 → MapsToAddress(value), all others → AllowAlways.
    struct TestAddressMappingPolicy;

    impl ResourceAccessPolicy for TestAddressMappingPolicy {
        fn can_acquire(
            &self,
            resource: &Resource,
            user: &UserId,
            mode: AccessMode,
        ) -> Result<(), AcquisitionDenied> {
            DefaultAccessPolicy.can_acquire(resource, user, mode)
        }

        fn resolve_classic_address(
            &self,
            _resource: &Resource,
            _user: &UserId,
            endpoint: u8,
            ep_type: &EndpointType,
            value: Option<u32>,
        ) -> ClassicAddressResolution {
            match (endpoint, ep_type) {
                (0x01, EndpointType::WireIn) => {
                    value.map_or(ClassicAddressResolution::NoMapping, |v| {
                        ClassicAddressResolution::MapsToAddress(v as u64)
                    })
                }
                _ => ClassicAddressResolution::AllowAlways,
            }
        }
    }

    /// Helper: create a resource with address ranges for ADR-025 tests.
    fn test_resource_with_address_ranges(id: &str, serial: &str) -> Resource {
        let mut resource = Resource::new(
            id.to_string(),
            format!("resource_{}", id),
            None,
            serial.to_string(),
            AccessMode::Shared,
        );
        resource.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x01,
                    ep_type: EndpointType::WireIn,
                    access: AccessMode::Shared,
                    description: Some("Address register".to_string()),
                    private: false,
                },
                EndpointEntry {
                    address: 0x02,
                    ep_type: EndpointType::WireIn,
                    access: AccessMode::Shared,
                    description: Some("Data register".to_string()),
                    private: false,
                },
            ],
        });
        resource.address_ranges = vec![
            AddressRange {
                name: None,
                description: None,
                start: 0x0000,
                end: 0x00FF,
                access: AccessMode::Exclusive, // DAC control — requires lock
                private: false,
            },
            AddressRange {
                name: None,
                description: None,
                start: 0x0100,
                end: 0x01FF,
                access: AccessMode::Shared, // Status — any holder
                private: false,
            },
        ];
        resource
    }

    #[test]
    fn adr025_default_policy_returns_no_mapping() {
        // DefaultAccessPolicy (used by ResourceRegistry::new()) should return
        // NoMapping, preserving all ADR-024 behavior.
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Any value should be allowed because DefaultAccessPolicy returns NoMapping.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x00, &EndpointType::WireIn, Some(0x42))
            .is_ok());
    }

    #[test]
    fn adr025_maps_to_shared_address_allows() {
        // Address 0x0150 is in the Shared range 0x0100–0x01FF.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Write address 0x0150 to WireIn 0x01 → MapsToAddress(0x0150)
        // → in Shared range → allowed.
        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0150)
            )
            .is_ok());
    }

    #[test]
    fn adr025_maps_to_exclusive_address_with_lock_allows() {
        // Address 0x0042 is in the Exclusive range 0x0000–0x00FF.
        // Alice holds the lock → should be allowed.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0042)
            )
            .is_ok());
    }

    #[test]
    fn adr025_maps_to_exclusive_address_without_lock_denies() {
        // Address 0x0042 is in the Exclusive range 0x0000–0x00FF.
        // When the resource is locked by Alice, Bob gets LockedByOther (upstream).
        // When the resource is NOT locked, Bob gets ExclusiveAddressRange (ADR-025).
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();

        // --- Scenario 1: Resource is UNLOCKED ---
        // Bob tries to access exclusive range without holding a lock.
        let err = registry
            .check_endpoint_access(
                "bob", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0042)
            )
            .unwrap_err();
        assert!(matches!(err, AccessDenied::ExclusiveAddressRange { .. }));

        // --- Scenario 2: Resource is LOCKED by Alice ---
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Alice (lock holder) can access exclusive range.
        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0042)
            )
            .is_ok());

        // Bob is locked out entirely (upstream lock check).
        let err = registry
            .check_endpoint_access(
                "bob", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0042)
            )
            .unwrap_err();
        assert!(matches!(err, AccessDenied::LockedByOther { .. }));
    }

    #[test]
    fn adr025_allow_always_bypasses_address_check() {
        // WireIn 0x02 (data register) → AllowAlways from our test policy.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Any value on WireIn 0x02 is AllowAlways — no address check.
        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x02, &EndpointType::WireIn, Some(0xDEAD)
            )
            .is_ok());
    }

    #[test]
    fn adr025_address_out_of_all_ranges_denies() {
        // Address 0x0300 is NOT in any declared range.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        let err = registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0300)
            )
            .unwrap_err();
        assert!(matches!(err, AccessDenied::NoCoveringResource { .. }));
    }

    #[test]
    fn adr025_no_address_ranges_legacy_allows_all() {
        // Resource with NO address_ranges → legacy mode, covers all addresses.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        let mut resource = test_resource_for_device("r1", "DEV_A");
        // Ensure no address ranges (legacy).
        resource.address_ranges.clear();
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // Even though MapsToAddress returns an address, no address_ranges = allow.
        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0xFFFF)
            )
            .is_ok());
    }

    #[test]
    fn adr025_pipe_operations_default_no_mapping() {
        // Pipe operations pass None as value → NoMapping → standard enforcement.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        // PipeIn endpoint — TestAddressMappingPolicy returns AllowAlways for
        // non-0x01 endpoints. Pipes should work.
        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x80, &EndpointType::PipeIn, None
            )
            .is_ok());
    }

    #[test]
    fn adr025_shared_address_no_lock_required() {
        // Even without a lock, shared address ranges allow access.
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        registry.register(test_resource_with_address_ranges("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        // No lock held by anyone.

        // Both users can access the shared range (0x0100–0x01FF).
        assert!(registry
            .check_endpoint_access(
                "alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0100)
            )
            .is_ok());
        assert!(registry
            .check_endpoint_access(
                "bob", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x01FF)
            )
            .is_ok());
    }

    // ====================================================================
    // Phase 30: ReadOnly Acquisition & Private Data (ADR-026)
    // ====================================================================

    #[test]
    fn readonly_acquire_shared_resource() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        // Alice acquires shared, Bob acquires ReadOnly.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();

        let r = registry.get("r1").unwrap();
        assert_eq!(r.holders.len(), 2);
        assert!(r.has_holder("alice"));
        assert!(r.has_holder("bob"));
        assert_eq!(r.holder_entry("alice").unwrap().mode, AccessMode::Shared);
        assert_eq!(r.holder_entry("bob").unwrap().mode, AccessMode::ReadOnly);
    }

    #[test]
    fn readonly_acquire_locked_resource_succeeds() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        // Alice acquires shared and locks the resource.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);

        // Bob can still join as ReadOnly even though it's locked.
        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();
        let r = registry.get("r1").unwrap();
        assert_eq!(r.holders.len(), 2);
        assert_eq!(r.holder_entry("bob").unwrap().mode, AccessMode::ReadOnly);
    }

    #[test]
    fn readonly_acquire_exclusive_resource_succeeds() {
        let registry = ResourceRegistry::new();
        let resource = Resource::new(
            "r1".to_string(),
            "Test".to_string(),
            None,
            "DEV_A".to_string(),
            AccessMode::Exclusive,
        );
        registry.register(resource);

        // Alice acquires exclusively.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Exclusive).unwrap();
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);

        // Bob can join as ReadOnly even on an exclusive resource.
        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();
        let r = registry.get("r1").unwrap();
        assert_eq!(r.holders.len(), 2);
        assert_eq!(r.holder_entry("bob").unwrap().mode, AccessMode::ReadOnly);
    }

    #[test]
    fn readonly_acquire_private_resource_denied() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource("r1");
        resource.private = true;
        registry.register(resource);

        // ReadOnly acquisition of a private resource is denied.
        let err = registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap_err();
        assert!(matches!(err, AcquisitionDenied::PrivateResource { .. }));

        // Non-ReadOnly acquisition still works.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        assert!(registry.get("r1").unwrap().has_holder("alice"));
    }

    #[test]
    fn readonly_cannot_lock() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();

        // ReadOnly holders cannot lock.
        let err = registry.lock("r1", &"bob".to_string());
        assert!(err.is_err());
    }

    #[test]
    fn readonly_denies_input_endpoint() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x00,
                    ep_type: EndpointType::WireIn,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0x20,
                    ep_type: EndpointType::WireOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
            ],
        });
        registry.register(resource);

        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();

        // ReadOnly holder CAN read WireOut.
        assert!(registry
            .check_endpoint_access("bob", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .is_ok());

        // ReadOnly holder CANNOT write WireIn.
        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn, None)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::ReadOnlyWriteAttempt { .. }));
    }

    #[test]
    fn readonly_denies_private_endpoint() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x20,
                    ep_type: EndpointType::WireOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: true, // Private endpoint
                },
                EndpointEntry {
                    address: 0x21,
                    ep_type: EndpointType::WireOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false, // Non-private
                },
            ],
        });
        registry.register(resource);

        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();

        // ReadOnly holder CANNOT read private endpoint.
        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::PrivateDataDenied { .. }));

        // ReadOnly holder CAN read non-private endpoint.
        assert!(registry
            .check_endpoint_access("bob", "DEV_A", 0x21, &EndpointType::WireOut, None)
            .is_ok());
    }

    #[test]
    fn readonly_denies_private_address_range_via_check_address() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges = vec![
            AddressRange {
                name: None,
                description: None,
                start: 0x0000,
                end: 0x00FF,
                access: AccessMode::Shared,
                private: true, // Private range
            },
            AddressRange {
                name: None,
                description: None,
                start: 0x0100,
                end: 0x01FF,
                access: AccessMode::Shared,
                private: false, // Non-private range
            },
        ];
        registry.register(resource);

        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();

        // ReadOnly holder CANNOT read from private address range.
        let err = registry
            .check_address_access("bob", "DEV_A", 0x0050, false)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::PrivateDataDenied { .. }));

        // ReadOnly holder CAN read from non-private address range.
        assert!(registry
            .check_address_access("bob", "DEV_A", 0x0150, false)
            .is_ok());
    }

    #[test]
    fn readonly_does_not_change_state_for_exclusive() {
        let registry = ResourceRegistry::new();
        let resource = Resource::new(
            "r1".to_string(),
            "Test".to_string(),
            None,
            "DEV_A".to_string(),
            AccessMode::Exclusive,
        );
        registry.register(resource);

        // Alice acquires exclusively — state becomes InUseLocked.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Exclusive).unwrap();
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);

        // Bob joins ReadOnly — state should NOT change.
        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);
    }

    #[test]
    fn readonly_first_holder_sets_state_to_in_use_shared() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        // First holder is ReadOnly — state should go from Available to InUseShared.
        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();
        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::InUseShared);
    }

    #[test]
    fn readonly_release_leaves_resource_available() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("r1"));

        registry.acquire("r1", &"bob".to_string(), AccessMode::ReadOnly).unwrap();
        assert!(registry.release("r1", &"bob".to_string()));

        let r = registry.get("r1").unwrap();
        assert_eq!(r.state, ResourceState::Available);
        assert!(r.holders.is_empty());
    }

    #[test]
    fn shared_holder_can_access_private_data() {
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x20,
                    ep_type: EndpointType::WireOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: true, // Private endpoint
                },
            ],
        });
        registry.register(resource);

        // Shared holder CAN access private endpoints.
        registry.acquire("r1", &"alice".to_string(), AccessMode::Shared).unwrap();
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .is_ok());
    }

    #[test]
    fn endpoint_type_is_input() {
        assert!(EndpointType::WireIn.is_input());
        assert!(EndpointType::TriggerIn.is_input());
        assert!(EndpointType::PipeIn.is_input());
        assert!(EndpointType::BtPipeIn.is_input());

        assert!(!EndpointType::WireOut.is_input());
        assert!(!EndpointType::TriggerOut.is_input());
        assert!(!EndpointType::PipeOut.is_input());
        assert!(!EndpointType::BtPipeOut.is_input());
    }

    // =========================================================================
    // Phase 33: Read-Aware Lock Enforcement
    // =========================================================================

    #[test]
    fn locked_shared_holder_can_read_output_endpoint() {
        // Phase 33: Shared holder can read WireOut on a locked resource.
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob (non-locker) can READ (WireOut).
        assert!(registry
            .check_endpoint_access("bob", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .is_ok());
        // Bob can also read TriggerOut.
        assert!(registry
            .check_endpoint_access("bob", "DEV_A", 0x60, &EndpointType::TriggerOut, None)
            .is_ok());
    }

    #[test]
    fn locked_shared_holder_cannot_write_input_endpoint() {
        // Phase 33: Shared holder CANNOT write WireIn on a locked resource.
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob (non-locker) CANNOT write (WireIn).
        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn, None)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::LockedByOther { .. }));
    }

    #[test]
    fn locked_readonly_holder_can_read_output_endpoint() {
        // Phase 33 + ADR-026 §5: ReadOnly holder can read WireOut on a
        // locked resource.
        let registry = ResourceRegistry::new();
        registry.register(test_resource_for_device("r1", "DEV_A"));
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob (ReadOnly, non-locker) can READ (WireOut).
        assert!(registry
            .check_endpoint_access("bob", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .is_ok());
    }

    #[test]
    fn locked_readonly_holder_denied_private_endpoint() {
        // Phase 33: ReadOnly holder CANNOT read a private endpoint, even
        // when the lock check is bypassed.
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![EndpointEntry {
                address: 0x20,
                ep_type: EndpointType::WireOut,
                access: AccessMode::Shared,
                description: Some("private wire out".to_string()),
                private: true,
            }],
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob can bypass lock for reads, but private check still denies.
        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::PrivateDataDenied { .. }));
    }

    #[test]
    fn locked_address_read_allowed_for_non_locker() {
        // Phase 33: Non-lock-holder can READ address when locked.
        let registry = ResourceRegistry::new();
        let mut resource = test_resource_for_device("r1", "DEV_A");
        resource.address_ranges.push(AddressRange {
            name: None,
            description: None,
            start: 0x44A6_0000,
            end: 0x44A8_FFFF,
            access: AccessMode::Shared,
            private: false,
        });
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("r1", &"alice".to_string()).unwrap();

        // Bob (non-locker) can READ (is_write=false).
        assert!(registry
            .check_address_access("bob", "DEV_A", 0x44A6_1000, false)
            .is_ok());

        // Bob (non-locker) CANNOT write (is_write=true).
        let err = registry
            .check_address_access("bob", "DEV_A", 0x44A6_1000, true)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::LockedByOther { .. }));
    }

    #[test]
    fn exclusive_resource_allows_readonly_read() {
        // Phase 33 clarification: Exclusive = always-locked, NOT always-private.
        // ReadOnly holders CAN read output endpoints on Exclusive resources.
        let registry = ResourceRegistry::new();
        let resource = test_exclusive_resource_for_device("r1", "DEV_A");
        registry.register(resource);
        registry
            .acquire("r1", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap();
        registry
            .acquire("r1", &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap();

        // Bob (ReadOnly) can READ (WireOut) on Exclusive resource.
        assert!(registry
            .check_endpoint_access("bob", "DEV_A", 0x20, &EndpointType::WireOut, None)
            .is_ok());

        // Bob (ReadOnly) CANNOT write (WireIn) — denied by ReadOnly check.
        let err = registry
            .check_endpoint_access("bob", "DEV_A", 0x00, &EndpointType::WireIn, None)
            .unwrap_err();
        assert!(matches!(err, AccessDenied::ReadOnlyWriteAttempt { .. }));
    }

    // ── ADR-034 / 44-A wire formats ─────────────────────────────

    #[test]
    fn test_resource_state_as_str_uses_spec_strings() {
        let cases = [
            (ResourceState::Unavailable, "unavailable"),
            (ResourceState::Initializing, "initializing"),
            (ResourceState::Available, "available"),
            (ResourceState::InUseShared, "in_use_shared"),
            (ResourceState::InUseLocked, "in_use_locked"),
        ];
        for (state, text) in cases {
            assert_eq!(state.as_str(), text);
            // serde must agree with as_str().
            assert_eq!(serde_json::to_string(&state).unwrap(), format!("\"{}\"", text));
        }
    }

    #[test]
    fn test_access_mode_as_str_uses_spec_strings() {
        for (mode, text) in [
            (AccessMode::Shared, "shared"),
            (AccessMode::Exclusive, "exclusive"),
            (AccessMode::ReadOnly, "read_only"),
        ] {
            assert_eq!(mode.as_str(), text);
            assert_eq!(serde_json::to_string(&mode).unwrap(), format!("\"{}\"", text));
            assert_eq!(AccessMode::parse_holder(text), Some(mode));
        }
        assert_eq!(AccessMode::parse_declared("read_only"), None);
    }

    #[test]
    fn test_endpoint_type_parse_matches_display() {
        for t in [
            EndpointType::WireIn,
            EndpointType::WireOut,
            EndpointType::TriggerIn,
            EndpointType::TriggerOut,
            EndpointType::PipeIn,
            EndpointType::PipeOut,
            EndpointType::BtPipeIn,
            EndpointType::BtPipeOut,
        ] {
            assert_eq!(EndpointType::parse(&t.to_string()), Some(t));
        }
        assert_eq!(EndpointType::parse("wire"), None);
    }

    #[test]
    fn test_resource_conflict_serializes_snake_case() {
        let c = ResourceConflict::new(ConflictCode::ModeMismatch)
            .with_resource("SN:a", "a")
            .with_range(0x10, 0x1F);
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["code"], "mode_mismatch");
        assert_eq!(v["resource_id"], "SN:a");
        assert_eq!(v["resource_name"], "a");
        assert_eq!(v["item_type"], "address_range");
        assert_eq!(v["item"], "0x00000010-0x0000001F");
    }

    #[test]
    fn test_resource_conflict_omits_optional_fields() {
        let v = serde_json::to_value(ResourceConflict::new(ConflictCode::Validation)).unwrap();
        assert_eq!(v, serde_json::json!({"code": "validation"}));
    }

    #[test]
    fn test_acquisition_denied_to_conflict_codes() {
        let r = test_resource("r1");
        let held = AcquisitionDenied::ExclusivelyHeld { held_by: "bob".into() };
        assert_eq!(held.to_conflict(&r).unwrap().code, ConflictCode::Locked);
        let shared = AcquisitionDenied::AlreadyShared { holder_count: 2 };
        assert_eq!(shared.to_conflict(&r).unwrap().code, ConflictCode::ModeMismatch);
        let holder_mismatch = AcquisitionDenied::HolderModeMismatch {
            user_id: "alice".into(),
            existing_mode: AccessMode::Shared,
            requested_mode: AccessMode::ReadOnly,
        };
        assert_eq!(holder_mismatch.to_conflict(&r).unwrap().code, ConflictCode::ModeMismatch);
        let decl_mismatch = AcquisitionDenied::DeclaredModeMismatch {
            declared: AccessMode::Exclusive,
            requested: AccessMode::Shared,
        };
        assert_eq!(decl_mismatch.to_conflict(&r).unwrap().code, ConflictCode::ModeMismatch);
        let priv_res = AcquisitionDenied::PrivateResource { resource_id: "r1".into() };
        let priv_conflict = priv_res.to_conflict(&r).unwrap();
        assert_eq!(priv_conflict.code, ConflictCode::Private);
        assert_eq!(priv_conflict.resource_id.as_deref(), Some("r1"));
        let custom = AcquisitionDenied::Custom { reason: "x".into() };
        assert!(custom.to_conflict(&r).is_none());
    }

    #[test]
    fn test_holdings_for_include_mode_and_locked() {
        let registry = ResourceRegistry::new();
        let mut shared = test_resource("b_shared");
        shared.access_mode = Some(AccessMode::Shared);
        registry.register(shared);
        registry.register(test_resource("a_other"));
        registry
            .acquire("b_shared", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry.lock("b_shared", &"alice".to_string()).unwrap();
        registry
            .acquire("a_other", &"alice".to_string(), AccessMode::ReadOnly)
            .unwrap();

        let h = registry.holdings_for("alice");
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].resource_id, "a_other");
        assert_eq!(h[0].mode, AccessMode::ReadOnly);
        assert!(!h[0].locked);
        assert_eq!(h[1].resource_id, "b_shared");
        assert_eq!(h[1].mode, AccessMode::Shared);
        assert!(h[1].locked);
        assert!(registry.holdings_for("nobody").is_empty());
    }

    #[test]
    fn test_try_register_duplicate_and_rejected() {
        let registry = ResourceRegistry::new();
        assert!(registry.try_register(test_resource("r1"), |_, _| Ok(())).is_ok());
        assert_eq!(
            registry.try_register(test_resource("r1"), |_, _| Ok(())),
            Err(RegisterError::Duplicate)
        );
        let err = registry
            .try_register(test_resource("r2"), |existing, _| {
                assert_eq!(existing.len(), 1);
                Err(ResourceError::validation("nope"))
            })
            .unwrap_err();
        assert!(matches!(err, RegisterError::Rejected(ref e) if e.is_validation()));
        assert!(registry.get("r2").is_none());
    }

    // ── ADR-034 / 44-C acquisition rules (A1) ───────────────────

    #[test]
    fn test_acquire_same_mode_idempotent() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("shared_r"));
        registry.register(test_exclusive_resource("excl_r"));

        // Shared repeat acquire is idempotent and does not duplicate HolderEntry.
        assert_eq!(
            registry
                .acquire("shared_r", &"alice".to_string(), AccessMode::Shared)
                .unwrap(),
            AcquireOutcome::Acquired
        );
        assert_eq!(
            registry
                .acquire("shared_r", &"alice".to_string(), AccessMode::Shared)
                .unwrap(),
            AcquireOutcome::AlreadyHeld
        );
        assert_eq!(registry.get("shared_r").unwrap().holders.len(), 1);

        // Exclusive repeat acquire is also idempotent (even though state == InUseLocked).
        assert_eq!(
            registry
                .acquire("excl_r", &"alice".to_string(), AccessMode::Exclusive)
                .unwrap(),
            AcquireOutcome::Acquired
        );
        assert_eq!(
            registry
                .acquire("excl_r", &"alice".to_string(), AccessMode::Exclusive)
                .unwrap(),
            AcquireOutcome::AlreadyHeld
        );
        let r = registry.get("excl_r").unwrap();
        assert_eq!(r.holders.len(), 1);
        assert_eq!(r.locked_by.as_deref(), Some("alice"));
    }

    #[test]
    fn test_acquire_different_mode_conflict() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("shared_r"));
        registry
            .acquire("shared_r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        let err = registry
            .acquire("shared_r", &"alice".to_string(), AccessMode::ReadOnly)
            .unwrap_err();
        assert!(matches!(
            err,
            AcquisitionDenied::HolderModeMismatch {
                existing_mode: AccessMode::Shared,
                requested_mode: AccessMode::ReadOnly,
                ..
            }
        ));
        let r = registry.get("shared_r").unwrap();
        assert_eq!(err.to_conflict(&r).unwrap().code, ConflictCode::ModeMismatch);
    }

    #[test]
    fn test_acquire_declared_mode_mismatch_conflict() {
        let registry = ResourceRegistry::new();
        registry.register(test_resource("shared_r"));
        registry.register(test_exclusive_resource("excl_r"));

        // Exclusive requested on an available Shared resource -> DeclaredModeMismatch.
        let err1 = registry
            .acquire("shared_r", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap_err();
        assert!(matches!(
            err1,
            AcquisitionDenied::DeclaredModeMismatch {
                declared: AccessMode::Shared,
                requested: AccessMode::Exclusive,
            }
        ));
        assert_eq!(
            err1.to_conflict(&registry.get("shared_r").unwrap()).unwrap().code,
            ConflictCode::ModeMismatch
        );

        // Shared requested on an available Exclusive resource -> DeclaredModeMismatch.
        let err2 = registry
            .acquire("excl_r", &"alice".to_string(), AccessMode::Shared)
            .unwrap_err();
        assert!(matches!(
            err2,
            AcquisitionDenied::DeclaredModeMismatch {
                declared: AccessMode::Exclusive,
                requested: AccessMode::Shared,
            }
        ));
        assert_eq!(
            err2.to_conflict(&registry.get("excl_r").unwrap()).unwrap().code,
            ConflictCode::ModeMismatch
        );
    }

    #[test]
    fn test_acquire_readonly_private_denied() {
        let registry = ResourceRegistry::new();
        let mut r = test_exclusive_resource("priv_r");
        r.private = true;
        registry.register(r);

        let err = registry
            .acquire("priv_r", &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap_err();
        assert!(matches!(err, AcquisitionDenied::PrivateResource { .. }));
        let stored = registry.get("priv_r").unwrap();
        let conflict = err.to_conflict(&stored).unwrap();
        assert_eq!(conflict.code, ConflictCode::Private);
        assert_eq!(conflict.resource_id.as_deref(), Some("priv_r"));
    }

    #[test]
    fn test_acquire_exclusive_with_readonly_holders_allowed() {
        let registry = ResourceRegistry::new();
        registry.register(test_exclusive_resource("excl_r"));

        // D25b: ReadOnly acquires available Exclusive resource -> InUseShared, locked_by = None.
        registry
            .acquire("excl_r", &"bob".to_string(), AccessMode::ReadOnly)
            .unwrap();
        let r = registry.get("excl_r").unwrap();
        assert_eq!(r.state, ResourceState::InUseShared);
        assert!(r.locked_by.is_none());

        // D25c: Exclusive holder can still acquire while ReadOnly holder is present.
        registry
            .acquire("excl_r", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap();
        let r = registry.get("excl_r").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);
        assert_eq!(r.locked_by.as_deref(), Some("alice"));
        assert_eq!(r.holders.len(), 2);

        // When the Exclusive holder releases and ReadOnly remains, state returns to InUseShared.
        assert!(registry.release("excl_r", &"alice".to_string()));
        let r = registry.get("excl_r").unwrap();
        assert_eq!(r.state, ResourceState::InUseShared);
        assert!(r.locked_by.is_none());
        assert_eq!(r.holders.len(), 1);
    }

    #[test]
    fn test_acquire_exclusive_holder_writes_without_lock_allowed() {
        // S-22: An Exclusive resource sets locked_by = Some(user_id) on acquire,
        // so the holder passes the ExclusiveAddressRange check without calling lock().
        let policy = Box::new(TestAddressMappingPolicy);
        let registry = ResourceRegistry::with_policy(policy);
        let mut r = test_resource_with_address_ranges("excl_r", "DEV_A");
        r.access_mode = Some(AccessMode::Exclusive);
        registry.register(r);

        registry
            .acquire("excl_r", &"alice".to_string(), AccessMode::Exclusive)
            .unwrap();
        let stored = registry.get("excl_r").unwrap();
        assert_eq!(stored.locked_by.as_deref(), Some("alice"));

        // Address 0x0042 is in the Exclusive range 0x0000–0x00FF.
        assert!(registry
            .check_endpoint_access("alice", "DEV_A", 0x01, &EndpointType::WireIn, Some(0x0042))
            .is_ok());
    }

    // ── Phase 44-D: Locking and blocking (B1–B4, D35) ────────────

    /// Build a shared resource on `serial` declaring the given WireIn endpoints as shared.
    fn shared_ep_resource(id: &str, name: &str, serial: &str, eps: &[u8]) -> Resource {
        let mut r = Resource::new(
            id.to_string(),
            name.to_string(),
            None,
            serial.to_string(),
            AccessMode::Shared,
        );
        r.interfaces.classic = true;
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: eps
                .iter()
                .map(|&addr| EndpointEntry {
                    address: addr,
                    ep_type: EndpointType::WireIn,
                    access: AccessMode::Shared,
                    private: false,
                    description: None,
                })
                .collect(),
        });
        r
    }

    #[test]
    fn test_blocking_shared_item_blocks_other_resource() {
        // B1, B3: R = {0x00, 0x01} and S = {0x01, 0x02} share endpoint 0x01 on DEV_A.
        // Alice locks R -> S is blocked for Bob with ConflictCode::Blocked naming R.
        let registry = ResourceRegistry::new();
        registry.register(shared_ep_resource("DEV_A:res_r", "res_r", "DEV_A", &[0x00, 0x01]));
        registry.register(shared_ep_resource("DEV_A:res_s", "res_s", "DEV_A", &[0x01, 0x02]));

        registry
            .acquire("DEV_A:res_r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("DEV_A:res_s", &"bob".to_string(), AccessMode::Shared)
            .unwrap();

        assert_eq!(
            registry.lock("DEV_A:res_r", &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );

        let err = registry.lock("DEV_A:res_s", &"bob".to_string()).unwrap_err();
        let conflict = err.conflict().expect("Blocked error must carry a conflict");
        assert_eq!(conflict.code, ConflictCode::Blocked);
        assert_eq!(conflict.resource_id.as_deref(), Some("DEV_A:res_r"));
        assert_eq!(conflict.resource_name.as_deref(), Some("res_r"));
        assert!(conflict.item_type.is_none());
        assert!(conflict.item.is_none());
        assert!(conflict.items.is_empty());

        // Also verify overlapping address ranges trigger blocking.
        let registry_ranges = ResourceRegistry::new();
        let mut r_range = Resource::new(
            "DEV_A:range_r".to_string(),
            "range_r".to_string(),
            None,
            "DEV_A".to_string(),
            AccessMode::Shared,
        );
        r_range.address_ranges.push(AddressRange {
            name: None,
            start: 0x1000,
            end: 0x1FFF,
            access: AccessMode::Shared,
            private: false,
            description: None,
        });
        let mut s_range = Resource::new(
            "DEV_A:range_s".to_string(),
            "range_s".to_string(),
            None,
            "DEV_A".to_string(),
            AccessMode::Shared,
        );
        s_range.address_ranges.push(AddressRange {
            name: None,
            start: 0x1800,
            end: 0x2FFF,
            access: AccessMode::Shared,
            private: false,
            description: None,
        });
        registry_ranges.register(r_range);
        registry_ranges.register(s_range);
        registry_ranges
            .acquire("DEV_A:range_r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry_ranges
            .acquire("DEV_A:range_s", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        registry_ranges
            .lock("DEV_A:range_r", &"alice".to_string())
            .unwrap();

        let err2 = registry_ranges
            .lock("DEV_A:range_s", &"bob".to_string())
            .unwrap_err();
        let conflict2 = err2.conflict().unwrap();
        assert_eq!(conflict2.code, ConflictCode::Blocked);
        assert_eq!(conflict2.resource_id.as_deref(), Some("DEV_A:range_r"));
        assert_eq!(conflict2.resource_name.as_deref(), Some("range_r"));
    }

    #[test]
    fn test_blocking_non_transitive() {
        // B4: R = {0x00, 0x01}, S = {0x01, 0x02}, T = {0x02, 0x03}.
        // Alice locks R -> S is blocked for Bob, but T is NOT blocked for Bob.
        let registry = ResourceRegistry::new();
        registry.register(shared_ep_resource("DEV_A:r", "r", "DEV_A", &[0x00, 0x01]));
        registry.register(shared_ep_resource("DEV_A:s", "s", "DEV_A", &[0x01, 0x02]));
        registry.register(shared_ep_resource("DEV_A:t", "t", "DEV_A", &[0x02, 0x03]));

        registry
            .acquire("DEV_A:r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("DEV_A:s", &"bob".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("DEV_A:t", &"bob".to_string(), AccessMode::Shared)
            .unwrap();

        registry.lock("DEV_A:r", &"alice".to_string()).unwrap();

        // S is blocked for Bob (shares 0x01 with R).
        let err_s = registry.lock("DEV_A:s", &"bob".to_string()).unwrap_err();
        assert_eq!(err_s.conflict().unwrap().code, ConflictCode::Blocked);

        // T is NOT blocked for Bob (shares 0x02 with S, which is unlocked, and nothing with R).
        assert_eq!(
            registry.lock("DEV_A:t", &"bob".to_string()).unwrap(),
            LockOutcome::Locked
        );
    }

    #[test]
    fn test_blocking_own_lock_not_blocked() {
        // D35: The locker is never blocked by her own lock.
        // Alice holds and locks R = {0x00, 0x01}, and also holds S = {0x01, 0x02}.
        // Alice can lock S as well.
        let registry = ResourceRegistry::new();
        registry.register(shared_ep_resource("DEV_A:r", "r", "DEV_A", &[0x00, 0x01]));
        registry.register(shared_ep_resource("DEV_A:s", "s", "DEV_A", &[0x01, 0x02]));

        registry
            .acquire("DEV_A:r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();
        registry
            .acquire("DEV_A:s", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        assert_eq!(
            registry.lock("DEV_A:r", &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );
        assert!(registry.blocking_conflict_for("DEV_A:s", "alice").is_none());
        assert_eq!(
            registry.lock("DEV_A:s", &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );
    }

    #[test]
    fn test_lock_repeat_by_same_user_is_idempotent() {
        // 44-D Q2: If Alice holds the lock and calls lock() again, it succeeds
        // with LockOutcome::AlreadyLockedBySelf.
        let registry = ResourceRegistry::new();
        registry.register(shared_ep_resource("DEV_A:r", "r", "DEV_A", &[0x00]));
        registry
            .acquire("DEV_A:r", &"alice".to_string(), AccessMode::Shared)
            .unwrap();

        assert_eq!(
            registry.lock("DEV_A:r", &"alice".to_string()).unwrap(),
            LockOutcome::Locked
        );
        assert_eq!(
            registry.lock("DEV_A:r", &"alice".to_string()).unwrap(),
            LockOutcome::AlreadyLockedBySelf
        );
        let r = registry.get("DEV_A:r").unwrap();
        assert_eq!(r.state, ResourceState::InUseLocked);
        assert_eq!(r.locked_by.as_deref(), Some("alice"));
    }
}

