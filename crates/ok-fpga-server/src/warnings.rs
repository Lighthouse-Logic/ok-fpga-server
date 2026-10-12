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
use ok_fpga_core::resource::Resource;
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
}
