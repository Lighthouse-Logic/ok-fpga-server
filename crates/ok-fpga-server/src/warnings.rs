// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Registration-time warnings
//!
//! Some declarations are *allowed* but probably not what the HDL architect
//! meant. The server accepts them and writes a `warn!` line to the log, so
//! the problem is visible without breaking the client. Each warning has its
//! own on/off switch in `[access_control]` of `config.toml` (owner decision
//! Q1, 44-B: "detailed warnings are always better").
//!
//! REST (`rest/resources.rs`) and gRPC (`grpc/device_service.rs`) both call
//! these functions, so the two APIs log the same text.

use ok_fpga_core::declaration::self_overlapping_ranges;
use ok_fpga_core::resource::{EndpointType, Resource};
use tracing::warn;

/// Build one human-readable message per pair of overlapping ranges inside
/// `resource`.
///
/// Kept separate from the logging so tests can check the text without
/// capturing log output.
pub fn self_overlap_messages(resource: &Resource) -> Vec<String> {
    self_overlapping_ranges(resource)
        .into_iter()
        .map(|(i, j)| {
            let a = &resource.address_ranges[i];
            let b = &resource.address_ranges[j];
            let mut msg = format!(
                "resource '{}': address range #{} ({}) overlaps range #{} ({}) in the same \
                 declaration",
                resource.id, i, a, j, b
            );
            // Overlapping ranges that disagree on `private` need a
            // tie-break; the more restrictive one (private) wins (44-E).
            if a.private != b.private {
                msg.push_str("; their 'private' flags differ, so the private one applies to the shared addresses");
            }
            msg
        })
        .collect()
}

/// Log a `warn!` line for every pair of overlapping ranges in `resource`.
///
/// Called only when `access_control.warn_range_self_overlap` is `true`.
pub fn warn_self_overlapping_ranges(resource: &Resource) {
    for msg in self_overlap_messages(resource) {
        warn!(resource_id = %resource.id, "{}", msg);
    }
}

/// Build human-readable warning messages when `ReadOnly` holders could join
/// `resource` and reach a non-private `TriggerOut`, `PipeOut`, `BtPipeOut`,
/// or `AddressRange` where a read can cause hardware side effects (D75 F4).
pub fn readonly_side_effects_messages(resource: &Resource) -> Vec<String> {
    // When `resource.private` is true, `ReadOnly` holders cannot acquire the
    // resource at all (D57), so no side-effect warning applies.
    if resource.private {
        return Vec::new();
    }

    let mut msgs = Vec::new();

    if resource.interfaces.classic {
        match &resource.endpoints {
            None => {
                msgs.push(format!(
                    "resource '{}': omitted 'endpoints' covers non-private TriggerOut endpoints \
                     (0x60–0x7F); a ReadOnly holder calling UpdateTriggerOuts will clear device \
                     trigger latches (mark resource 'private: true' or declare TriggerOuts with \
                     'private: true' if triggers must be protected, D75 F4)",
                    resource.id
                ));
            }
            Some(ep_decl) => {
                for entry in &ep_decl.entries {
                    if entry.private {
                        continue;
                    }
                    match entry.ep_type {
                        EndpointType::TriggerOut => {
                            msgs.push(format!(
                                "resource '{}': endpoint 0x{:02X} (trigger_out) is not marked \
                                 'private'; a ReadOnly holder calling UpdateTriggerOuts will \
                                 clear device trigger latches (D75 F4)",
                                resource.id, entry.address
                            ));
                        }
                        EndpointType::PipeOut | EndpointType::BtPipeOut => {
                            msgs.push(format!(
                                "resource '{}': endpoint 0x{:02X} ({}) is not marked 'private'; \
                                 a ReadOnly holder reading this pipe will pulse ep_read and may \
                                 drain a hardware FIFO (D75 F4)",
                                resource.id, entry.address, entry.ep_type
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    for range in &resource.address_ranges {
        if !range.private {
            msgs.push(format!(
                "resource '{}': address range 0x{:08X}-0x{:08X} ({}) is not marked 'private'; a \
                 ReadOnly holder reading this range may trigger clear-on-read hardware side \
                 effects (D75 F4)",
                resource.id, range.start, range.end, range
            ));
        }
    }

    msgs
}

/// Log a `warn!` line for every `ReadOnly` side-effect risk on `resource`.
///
/// Called only when `access_control.warn_readonly_side_effects` is `true` (D75 F4).
pub fn warn_readonly_side_effects(resource: &Resource) {
    for msg in readonly_side_effects_messages(resource) {
        warn!(resource_id = %resource.id, "{}", msg);
    }
}

/// Returns `true` if `resource` explicitly declares at least one `Pipe` or
/// `BTPipe` endpoint (`0x80..=0xBF`).
fn declares_pipes(resource: &Resource) -> bool {
    resource
        .endpoints
        .as_ref()
        .is_some_and(|ep| ep.entries.iter().any(|e| e.address >= 0x80))
}

/// Build human-readable warning messages for omitted `endpoints` (`None`) and
/// Pipe/BTPipe coexistence on the same device (owner decision 44-E Q2, D77).
///
/// - When `new_resource` is a Classic resource with `endpoints: None`, warns
///   that the server assumes all Wire and Trigger endpoints (`0x00–0x7F`),
///   leaves Pipe/BTPipe (`0x80–0xBF`) available to other resources, and notes
///   that `"endpoints": {"entries": []}` can be used if no Classic endpoints
///   are needed (`D59`).
/// - When `new_resource` is registered alongside an existing resource on the
///   same device where one has `endpoints: None` and the other declares
///   Pipe/BTPipe endpoints (`0x80–0xBF`), warns that a hardware conflict could
///   occur if the `endpoints: None` resource also uses Pipes without declaring
///   them.
pub fn wildcard_endpoint_messages(
    existing_on_device: &[Resource],
    new_resource: &Resource,
) -> Vec<String> {
    if !new_resource.interfaces.classic {
        return Vec::new();
    }

    let mut msgs = Vec::new();

    if new_resource.endpoints.is_none() {
        msgs.push(format!(
            "resource '{}': 'endpoints' omitted — assuming all Wire and Trigger endpoints \
             (0x00–0x7F); Pipe/BTPipe endpoints (0x80–0xBF) are not claimed and remain \
             available to other resources (pass '\"endpoints\": {{\"entries\": []}}' if this \
             resource uses no Classic endpoints, D59/D77)",
            new_resource.id
        ));

        for other in existing_on_device {
            if other.id != new_resource.id && other.interfaces.classic && declares_pipes(other) {
                msgs.push(format!(
                    "resource '{}': registered with omitted 'endpoints' (assumed 0x00–0x7F) on \
                     device '{}' alongside resource '{}' which declares Pipe/BTPipe endpoints \
                     (0x80–0xBF); a hardware conflict may occur if '{}' also uses Pipes",
                    new_resource.id, new_resource.device_serial, other.id, new_resource.id
                ));
            }
        }
    } else if declares_pipes(new_resource) {
        for other in existing_on_device {
            if other.id != new_resource.id && other.interfaces.classic && other.endpoints.is_none()
            {
                msgs.push(format!(
                    "resource '{}': declares Pipe/BTPipe endpoints (0x80–0xBF) on device '{}' \
                     alongside resource '{}' which omitted 'endpoints' (assumed 0x00–0x7F); a \
                     hardware conflict may occur if '{}' also uses Pipes",
                    new_resource.id, new_resource.device_serial, other.id, other.id
                ));
            }
        }
    }

    msgs
}

/// Log a `warn!` line for omitted `endpoints` and Pipe/BTPipe coexistence
/// warnings (owner decision 44-E Q2).
pub fn warn_wildcard_endpoints(existing_on_device: &[Resource], new_resource: &Resource) {
    for msg in wildcard_endpoint_messages(existing_on_device, new_resource) {
        warn!(resource_id = %new_resource.id, "{}", msg);
    }
}

/// Active `[access_control]` warning switches for a resource registration
/// request (Phase 45 Q4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistrationWarningToggles {
    /// Emit warnings when `address_ranges` inside one declaration overlap
    /// (`warn_range_self_overlap`).
    pub self_overlap: bool,
    /// Emit warnings when `ReadOnly` holders could reach a non-private
    /// side-effect item (`warn_readonly_side_effects`).
    pub readonly_side_effects: bool,
    /// Emit warnings when a Classic resource omits `endpoints` (`0x00–0x7F`
    /// assumed) or coexists with a Pipe-declaring resource
    /// (`warn_wildcard_endpoints`).
    pub wildcard_endpoints: bool,
}

impl Default for RegistrationWarningToggles {
    fn default() -> Self {
        Self {
            self_overlap: true,
            readonly_side_effects: true,
            wildcard_endpoints: true,
        }
    }
}

/// Collect and log all enabled registration-time advisory warnings for a
/// newly registered `resource` in a deterministic order (Phase 45 Q4):
///
/// 1. [`self_overlap_messages`] (when `toggles.self_overlap` is `true`)
/// 2. [`readonly_side_effects_messages`] (when `toggles.readonly_side_effects` is `true`)
/// 3. [`wildcard_endpoint_messages`] (when `toggles.wildcard_endpoints` is `true`,
///    with `existing_on_device` sorted by `id` for deterministic ordering)
///
/// Every collected message is logged once via `tracing::warn!` and returned in
/// the resulting `Vec<String>` so REST `POST /resources` (`ResourceView.warnings`)
/// and gRPC `CreateResource` (`ResourceResponse.warnings`) surface identical
/// warning lists.
pub fn collect_and_log_registration_warnings(
    resource: &Resource,
    existing_on_device: &[Resource],
    toggles: RegistrationWarningToggles,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if toggles.self_overlap {
        warnings.extend(self_overlap_messages(resource));
    }

    if toggles.readonly_side_effects {
        warnings.extend(readonly_side_effects_messages(resource));
    }

    if toggles.wildcard_endpoints {
        let mut sorted = existing_on_device.to_vec();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        warnings.extend(wildcard_endpoint_messages(&sorted, resource));
    }

    for msg in &warnings {
        warn!(resource_id = %resource.id, "{}", msg);
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use ok_fpga_core::resource::{AccessMode, AddressRange};

    fn range(start: u32, end: u32, private: bool) -> AddressRange {
        AddressRange {
            name: None,
            description: None,
            start,
            end,
            access: AccessMode::Shared,
            private,
        }
    }

    fn resource_with(ranges: Vec<AddressRange>) -> Resource {
        let mut r = Resource::new(
            "SN1:r".into(),
            "r".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        r.address_ranges = ranges;
        r
    }

    #[test]
    fn test_self_overlap_messages_overlapping_ranges_one_message() {
        let r = resource_with(vec![range(0x1000, 0x1FFF, false), range(0x1800, 0x27FF, false)]);
        let msgs = self_overlap_messages(&r);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("#0") && msgs[0].contains("#1"));
        assert!(!msgs[0].contains("private"));
    }

    #[test]
    fn test_self_overlap_messages_private_differs_mentions_private() {
        let r = resource_with(vec![range(0x1000, 0x1FFF, false), range(0x1000, 0x10FF, true)]);
        let msgs = self_overlap_messages(&r);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("private one applies"));
    }

    #[test]
    fn test_self_overlap_messages_disjoint_ranges_none() {
        let r = resource_with(vec![range(0x1000, 0x1FFF, false), range(0x2000, 0x2FFF, false)]);
        assert!(self_overlap_messages(&r).is_empty());
    }

    #[test]
    fn test_readonly_side_effects_messages_warns_non_private_pipe_out_trigger_out_and_range() {
        use ok_fpga_core::resource::{EndpointDeclaration, EndpointEntry};
        let mut r = resource_with(vec![range(0x1000, 0x1FFF, false)]);
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x20,
                    ep_type: EndpointType::WireOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0x60,
                    ep_type: EndpointType::TriggerOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0xA0,
                    ep_type: EndpointType::PipeOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
                EndpointEntry {
                    address: 0xA1,
                    ep_type: EndpointType::BtPipeOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: false,
                },
            ],
        });
        let msgs = readonly_side_effects_messages(&r);
        // WireOut (0x20) has no side effect; TriggerOut (0x60), PipeOut (0xA0),
        // BtPipeOut (0xA1), and the non-private range all warn (4 messages).
        assert_eq!(msgs.len(), 4);
        assert!(msgs.iter().any(|m| m.contains("0x60 (trigger_out)")));
        assert!(msgs.iter().any(|m| m.contains("0xA0 (pipe_out)")));
        assert!(msgs.iter().any(|m| m.contains("0xA1 (bt_pipe_out)")));
        assert!(msgs.iter().any(|m| m.contains("0x00001000-0x00001FFF")));
    }

    #[test]
    fn test_readonly_side_effects_messages_silent_when_resource_or_items_private() {
        use ok_fpga_core::resource::{EndpointDeclaration, EndpointEntry};
        // 1. All items marked `private: true` -> zero warnings.
        let mut r = resource_with(vec![range(0x1000, 0x1FFF, true)]);
        r.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![
                EndpointEntry {
                    address: 0x60,
                    ep_type: EndpointType::TriggerOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: true,
                },
                EndpointEntry {
                    address: 0xA0,
                    ep_type: EndpointType::PipeOut,
                    access: AccessMode::Shared,
                    description: None,
                    private: true,
                },
            ],
        });
        assert!(readonly_side_effects_messages(&r).is_empty());

        // 2. Resource itself marked `private: true` -> ReadOnly cannot acquire -> zero warnings.
        let mut r_priv = resource_with(vec![range(0x1000, 0x1FFF, false)]);
        r_priv.private = true;
        assert!(readonly_side_effects_messages(&r_priv).is_empty());
    }

    #[test]
    fn test_wildcard_endpoint_messages_warns_on_omitted_endpoints_and_pipe_coexistence() {
        use ok_fpga_core::resource::{EndpointDeclaration, EndpointEntry};
        // 1. Omitted `endpoints` warns about 0x00–0x7F assumption and {"entries": []}.
        let wildcard = Resource::new(
            "SN1:wild".into(),
            "wild".into(),
            None,
            "SN1".into(),
            AccessMode::Exclusive,
        );
        let msgs = wildcard_endpoint_messages(&[], &wildcard);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("0x00–0x7F"));
        assert!(msgs[0].contains("\"entries\": []"));

        // 2. Registering a Pipe-using resource alongside `wildcard` warns about coexistence.
        let mut pipe_res = Resource::new(
            "SN1:pipe".into(),
            "pipe".into(),
            None,
            "SN1".into(),
            AccessMode::Shared,
        );
        pipe_res.endpoints = Some(EndpointDeclaration {
            default_access: Some(AccessMode::Shared),
            entries: vec![EndpointEntry {
                address: 0x80,
                ep_type: EndpointType::PipeIn,
                access: AccessMode::Shared,
                description: None,
                private: false,
            }],
        });
        let msgs2 = wildcard_endpoint_messages(&[wildcard], &pipe_res);
        assert_eq!(msgs2.len(), 1);
        assert!(msgs2[0].contains("SN1:wild"));
        assert!(msgs2[0].contains("0x80–0xBF"));
    }

    #[test]
    fn test_collect_and_log_registration_warnings_orders_and_respects_toggles() {
        use ok_fpga_core::resource::{EndpointDeclaration, EndpointEntry};
        // Build an existing Pipe resource (`SN1:z_pipe` and `SN1:a_pipe` out of
        // alphabetical order to test sorting) and a new resource with omitted
        // `endpoints` + self-overlapping non-private ranges so all 3 categories
        // fire.
        let make_pipe = |id: &str, name: &str| {
            let mut p = Resource::new(
                id.into(),
                name.into(),
                None,
                "SN1".into(),
                AccessMode::Shared,
            );
            p.endpoints = Some(EndpointDeclaration {
                default_access: Some(AccessMode::Shared),
                entries: vec![EndpointEntry {
                    address: 0x80,
                    ep_type: EndpointType::PipeIn,
                    access: AccessMode::Shared,
                    description: None,
                    private: true,
                }],
            });
            p
        };
        let existing = vec![make_pipe("SN1:z_pipe", "z_pipe"), make_pipe("SN1:a_pipe", "a_pipe")];

        let mut new_res = Resource::new(
            "SN1:wild".into(),
            "wild".into(),
            None,
            "SN1".into(),
            AccessMode::Exclusive,
        );
        new_res.address_ranges = vec![range(0x1000, 0x1FFF, false), range(0x1800, 0x27FF, false)];

        // 1. All toggles enabled: self_overlap -> readonly_side_effects -> wildcard (sorted a_pipe then z_pipe).
        let all = collect_and_log_registration_warnings(
            &new_res,
            &existing,
            RegistrationWarningToggles::default(),
        );
        // 1 self-overlap + 3 readonly (omitted TriggerOuts + 2 non-private ranges) + 3 wildcard (omitted + a_pipe + z_pipe) = 7
        assert_eq!(all.len(), 7);
        assert!(all[0].contains("address range #0"));
        assert!(all[1].contains("0x60–0x7F"));
        assert!(all[2].contains("0x00001000-0x00001FFF"));
        assert!(all[3].contains("0x00001800-0x000027FF"));
        assert!(all[4].contains("0x00–0x7F"));
        assert!(all[5].contains("SN1:a_pipe"));
        assert!(all[6].contains("SN1:z_pipe"));

        // 2. Disable self_overlap only.
        let no_self = collect_and_log_registration_warnings(
            &new_res,
            &existing,
            RegistrationWarningToggles {
                self_overlap: false,
                readonly_side_effects: true,
                wildcard_endpoints: true,
            },
        );
        assert_eq!(no_self.len(), 6);
        assert!(!no_self.iter().any(|m| m.contains("address range #0")));

        // 3. Disable readonly_side_effects only.
        let no_ro = collect_and_log_registration_warnings(
            &new_res,
            &existing,
            RegistrationWarningToggles {
                self_overlap: true,
                readonly_side_effects: false,
                wildcard_endpoints: true,
            },
        );
        assert_eq!(no_ro.len(), 4);

        // 4. Disable wildcard_endpoints only.
        let no_wild = collect_and_log_registration_warnings(
            &new_res,
            &existing,
            RegistrationWarningToggles {
                self_overlap: true,
                readonly_side_effects: true,
                wildcard_endpoints: false,
            },
        );
        assert_eq!(no_wild.len(), 4);
        assert!(!no_wild.iter().any(|m| m.contains("SN1:a_pipe")));
    }
}
