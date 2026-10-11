//! # Opal Kelly snapshot reads (ADR-034 §9, D76)
//!
//! FrontPanel reads output endpoints in two steps:
//!
//! 1. `UpdateWireOuts()` / `UpdateTriggerOuts()` talk to the FPGA. They copy
//!    the current WireOut values (or the TriggerOut latches "activated since
//!    the last call") into a *snapshot* held by the FrontPanel API. They take
//!    only the device handle, so they always cover **every** endpoint of that
//!    kind on the device.
//! 2. `GetWireOutValue(ep)`, `IsTriggered(ep, mask)` and
//!    `GetTriggerOutVector(ep)` only read that snapshot. They never touch the
//!    FPGA.
//!
//! The server keeps one FrontPanel handle per device, so the snapshot is
//! **shared by every client**. Read calls on the REST and gRPC APIs follow
//! the same two-step model: they read the snapshot unless the caller sets
//! `update`, in which case the server runs the matching `Update…` call and
//! the read back-to-back while it holds the device lock. That closes the gap
//! in which another client's update could replace the snapshot between a
//! client's own update and read. It does **not** tell a client how many
//! updates happened since its previous read; that needs the trigger event
//! log described in the roadmap (🔵 Later, "TriggerOut event log").
//!
//! The helper below holds only the ordering rule, so it can be tested
//! without a FrontPanel device (Tier 3).

/// Run an optional refresh, then a snapshot read.
///
/// - `update == false`: only `read` runs (plain Opal Kelly read of the
///   current snapshot).
/// - `update == true`: `refresh` runs first; if it fails its error is
///   returned and `read` does **not** run, so a client never receives a value
///   from a snapshot it asked to refresh but could not.
///
/// Callers must already hold the device lock, so nothing can run between
/// the two steps.
///
/// `impl FnOnce` means "any closure that can be called once". Using generics
/// here (instead of `Box<dyn FnOnce>`) lets the compiler inline the FFI
/// calls with no allocation.
pub fn read_snapshot<T, E>(
    update: bool,
    refresh: impl FnOnce() -> Result<(), E>,
    read: impl FnOnce() -> T,
) -> Result<T, E> {
    if update {
        refresh()?;
    }
    Ok(read())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn test_read_snapshot_without_update_skips_refresh() {
        let log = RefCell::new(Vec::new());
        let value: Result<u32, ()> = read_snapshot(
            false,
            || {
                log.borrow_mut().push("refresh");
                Ok(())
            },
            || {
                log.borrow_mut().push("read");
                7
            },
        );
        assert_eq!(value, Ok(7));
        assert_eq!(*log.borrow(), vec!["read"]);
    }

    #[test]
    fn test_read_snapshot_with_update_refreshes_before_read() {
        let log = RefCell::new(Vec::new());
        let value: Result<u32, ()> = read_snapshot(
            true,
            || {
                log.borrow_mut().push("refresh");
                Ok(())
            },
            || {
                log.borrow_mut().push("read");
                9
            },
        );
        assert_eq!(value, Ok(9));
        assert_eq!(*log.borrow(), vec!["refresh", "read"]);
    }

    #[test]
    fn test_read_snapshot_refresh_error_skips_read() {
        let log = RefCell::new(Vec::new());
        let value: Result<u32, &str> = read_snapshot(
            true,
            || Err("device gone"),
            || {
                log.borrow_mut().push("read");
                1
            },
        );
        assert_eq!(value, Err("device gone"));
        assert!(log.borrow().is_empty());
    }
}
