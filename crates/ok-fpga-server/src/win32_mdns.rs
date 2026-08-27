//! # Windows-specific Win32 mDNS Service Registration
//!
//! Calls `DnsServiceConstructInstance` and `DnsServiceRegister` directly from
//! the Win32 DNS API to include the machine's IPv4 address as `pIp4`. This
//! ensures the DNS-SD response includes an A record, which is required by
//! RFC 6763 §12 and by compliant browsers like `avahi-browse`.
//!
//! ## Why not use dns-sd-native on Windows?
//!
//! The `dns-sd-native` crate's `ServiceRegistrationBuilder` passes `None` for
//! both `pIp4` and `pIp6` when calling `DnsServiceConstructInstance`. This
//! results in a `DNS_SERVICE_INSTANCE` with no address records. Windows'
//! dnscache will advertise PTR + SRV + TXT but omit A/AAAA records, making
//! the service invisible to compliant mDNS browsers.
//!
//! On Linux, `dns-sd-native` uses Avahi's D-Bus API which handles A/AAAA
//! records automatically, so this Windows-specific code is not needed there.
//!
//! ## Implementation
//!
//! This module is a thin wrapper around the same Win32 API that `dns-sd-native`
//! uses, with the key difference being that we resolve and pass the machine's
//! IPv4 address. The registration handle (`Win32ServiceRegistration`) implements
//! `Drop` to deregister the service when the handle is dropped.

use std::ffi::c_void;
use std::net::Ipv4Addr;
use std::ptr::NonNull;

use tracing::{trace, error};
use widestring::U16CString;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::NetworkManagement::Dns;
use windows::core::PCWSTR;
use windows_strings::*;

use tokio::sync::oneshot;

/// Error type for Windows mDNS registration failures.
#[derive(Debug)]
pub enum Win32MdnsError {
    /// The DNS Client service is not running or unavailable.
    DaemonAbsent(String),
    /// The service instance could not be constructed.
    ConstructionFailed(String),
    /// The registration API call failed.
    RegistrationFailed(String),
    /// A string parameter contains an interior NUL byte.
    InvalidParameter(String),
}

impl std::fmt::Display for Win32MdnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DaemonAbsent(msg) => write!(f, "DNS-SD daemon not available: {}", msg),
            Self::ConstructionFailed(msg) => {
                write!(f, "DnsServiceConstructInstance failed: {}", msg)
            }
            Self::RegistrationFailed(msg) => write!(f, "DnsServiceRegister failed: {}", msg),
            Self::InvalidParameter(msg) => write!(f, "invalid parameter: {}", msg),
        }
    }
}

impl Win32MdnsError {
    /// Returns true if this error indicates the OS mDNS daemon is not running.
    /// Used by the tiered fallback strategy to decide whether to fall back to
    /// the embedded mdns-sd responder.
    pub fn is_daemon_absent(&self) -> bool {
        matches!(self, Self::DaemonAbsent(_))
    }
}

// ============================================================================
// Constants
// ============================================================================

const DNS_QUERY_REQUEST_VERSION1: u32 = 1;
const DNS_REQUEST_PENDING: u32 = 9506;

// ============================================================================
// RAII wrapper around DNS_SERVICE_INSTANCE
// ============================================================================

/// RAII wrapper around a `DNS_SERVICE_INSTANCE` produced by
/// `DnsServiceConstructInstance`. Dropping it releases the instance via
/// `DnsServiceFreeInstance`.
struct ServiceInstance(NonNull<Dns::DNS_SERVICE_INSTANCE>);

// SAFETY: This wrapper is the sole owner of the underlying
// `DNS_SERVICE_INSTANCE` heap allocation. The raw pointer is only ever used
// as an opaque handle passed to the DNS-SD API and is never dereferenced in
// Rust, so the value is safe to move and share between threads.
unsafe impl Send for ServiceInstance {}
unsafe impl Sync for ServiceInstance {}

impl ServiceInstance {
    /// Construct a new `DNS_SERVICE_INSTANCE` with the machine's IPv4 address.
    ///
    /// Unlike `dns-sd-native`, this passes a valid `pIp4` so the service
    /// registration includes an A record.
    fn new(
        instance_name: &str,
        host_name: &str,
        port: u16,
        ipv4_addr: Ipv4Addr,
        key_value_pairs: &[(U16CString, U16CString)],
    ) -> Result<Self, Win32MdnsError> {
        let instance_name = HSTRING::from(instance_name);
        let host_name = HSTRING::from(host_name);

        let key_ptrs: Vec<PCWSTR> = key_value_pairs
            .iter()
            .map(|(k, _)| PCWSTR(k.as_ptr()))
            .collect();
        let value_ptrs: Vec<PCWSTR> = key_value_pairs
            .iter()
            .map(|(_, v)| PCWSTR(v.as_ptr()))
            .collect();

        // Convert the IPv4 address to a DWORD in network byte order.
        // IP4_ADDRESS is a DWORD (u32) in network byte order.
        let mut ip4_addr: u32 = u32::from_be_bytes(ipv4_addr.octets());

        // SAFETY: `instance_name` / `host_name` are valid HSTRINGs,
        // `ip4_addr` points to a valid u32, and the key/value pointer
        // arrays (and their backing `key_value_pairs` vec) outlive this call.
        let instance = unsafe {
            Dns::DnsServiceConstructInstance(
                &instance_name,
                &host_name,
                Some(&mut ip4_addr),   // pIp4: A record — THIS IS THE KEY FIX
                None,                  // pIp6: no AAAA record (server binds 0.0.0.0)
                port,
                0, // priority
                0, // weight
                key_value_pairs.len().try_into().unwrap(),
                key_ptrs.as_ptr(),
                value_ptrs.as_ptr(),
            )
        };

        NonNull::new(instance)
            .map(Self)
            .ok_or_else(|| {
                Win32MdnsError::ConstructionFailed(
                    "DnsServiceConstructInstance returned null".into(),
                )
            })
    }

    /// Returns the raw instance pointer for passing to the DNS-SD API.
    fn as_ptr(&self) -> *mut Dns::DNS_SERVICE_INSTANCE {
        self.0.as_ptr()
    }
}

impl Drop for ServiceInstance {
    fn drop(&mut self) {
        // SAFETY: the instance was produced by `DnsServiceConstructInstance`
        // and has not been freed yet. `DnsServiceFreeInstance` is the
        // documented way to release it.
        unsafe {
            Dns::DnsServiceFreeInstance(self.0.as_ptr());
        }
    }
}

// ============================================================================
// Registration handle — deregisters on drop
// ============================================================================

/// Handle to an active Win32 DNS-SD service registration.
///
/// Dropping this handle deregisters the service via `DnsServiceDeRegister`,
/// notifying remote clients that the service is no longer available.
pub struct Win32ServiceRegistration {
    /// The service instance (owns the DNS_SERVICE_INSTANCE allocation).
    /// `None` if already deregistered.
    instance: Option<ServiceInstance>,
}

/// Context passed to the Win32 callback. Owns the `ServiceInstance` while
/// the async registration is in flight.
struct CallbackContext {
    status_tx: Option<oneshot::Sender<(u32, ServiceInstance)>>,
    instance: ServiceInstance,
}

impl Win32ServiceRegistration {
    fn deregister(
        &mut self,
        status_tx: Option<oneshot::Sender<(u32, ServiceInstance)>>,
    ) -> Result<(), String> {
        let Some(instance) = self.instance.take() else {
            return Ok(()); // Already deregistered.
        };

        let request = build_request(instance, status_tx);

        // SAFETY: `request` references the instance and context, both owned
        // by the `CallbackContext` behind `context_ptr`, which stays alive
        // until the completion callback reclaims it.
        let result = unsafe { Dns::DnsServiceDeRegister(&request, None) };
        trace!("DnsServiceDeRegister result: {result}");

        if result != DNS_REQUEST_PENDING {
            // The callback will not fire; reclaim the context.
            // SAFETY: `context_ptr` came from `Box::into_raw` in `build_request`.
            let context_ptr = request.pQueryContext as *mut CallbackContext;
            unsafe {
                drop(Box::from_raw(context_ptr));
            }
            return Err(format!(
                "deregistration failed to start with status {result}"
            ));
        }
        Ok(())
    }
}

impl Drop for Win32ServiceRegistration {
    fn drop(&mut self) {
        // Fire-and-forget deregistration.
        self.deregister(None).ok();
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Register a DNS-SD service with the Windows DNS Client service (dnscache).
///
/// Unlike `dns-sd-native`, this function passes the machine's IPv4 address
/// as `pIp4` to `DnsServiceConstructInstance`, ensuring the DNS-SD response
/// includes an A record (required by RFC 6763 §12).
///
/// The returned `Win32ServiceRegistration` handle deregisters the service
/// when dropped.
pub async fn register_service(
    instance_name: &str,
    service_type: &str,
    hostname: &str,
    port: u16,
    ipv4_addr: Ipv4Addr,
    txt_props: &[(String, String)],
) -> Result<Win32ServiceRegistration, Win32MdnsError> {
    // Build the TXT record key-value pairs as wide strings.
    let key_value_pairs: Vec<(U16CString, U16CString)> = txt_props
        .iter()
        .map(|(k, v)| {
            let k = U16CString::from_str(k).map_err(|e| {
                Win32MdnsError::InvalidParameter(format!(
                    "TXT key '{}' contains nul at position {}",
                    k,
                    e.nul_position()
                ))
            })?;
            let v = U16CString::from_str(v).map_err(|e| {
                Win32MdnsError::InvalidParameter(format!(
                    "TXT value '{}' contains nul at position {}",
                    v,
                    e.nul_position()
                ))
            })?;
            Ok((k, v))
        })
        .collect::<Result<Vec<_>, Win32MdnsError>>()?;

    // Build the full instance name: "name.type.local"
    let full_instance_name = format!("{}.{}.local", instance_name, service_type);
    let full_host_name = format!("{}.local", hostname);

    let instance = ServiceInstance::new(
        &full_instance_name,
        &full_host_name,
        port,
        ipv4_addr,
        &key_value_pairs,
    )?;

    // Submit the Win32 registration synchronously (returns a channel receiver).
    // This helper exists so the `DNS_SERVICE_REGISTER_REQUEST` (which contains
    // raw pointers and is not `Send`) is created and dropped before any `.await`.
    let rx = submit_registration(instance)?;

    // Wait for the completion callback.
    let (status, instance) = rx.await.map_err(|_| {
        Win32MdnsError::RegistrationFailed(
            "registration callback channel closed unexpectedly".into(),
        )
    })?;

    if status != ERROR_SUCCESS.0 {
        return Err(Win32MdnsError::RegistrationFailed(format!(
            "registration failed with status: {}",
            status
        )));
    }

    Ok(Win32ServiceRegistration {
        instance: Some(instance),
    })
}

/// Attempt to deregister a stale DNS-SD service by name (Phase 43).
///
/// Constructs a minimal `DNS_SERVICE_INSTANCE` with the given fully-qualified
/// name and calls `DnsServiceDeRegister`.  This is a **best-effort** cleanup:
/// if the service doesn't exist in the Windows DNS Client cache, or if
/// deregistration fails for any reason, the error is logged at `TRACE` level
/// and ignored.
///
/// Use cases:
/// - **Crash recovery**: remove a registration from a previous server run
///   that exited ungracefully (the `Drop` impl never ran).
/// - **Migration cleanup**: remove a registration under the old bare
///   `"okfpga-server"` name from pre-Phase 42 binaries.
pub fn try_deregister_stale(instance_name: &str, service_type: &str) {
    let full_instance_name = format!("{}.{}.local", instance_name, service_type);
    let instance_hstring = HSTRING::from(&full_instance_name);
    // Host name is required but not used for deregistration matching.
    let host_hstring = HSTRING::from("localhost.local");

    // SAFETY: DnsServiceConstructInstance is the documented way to build the
    // instance structure.  We pass None for pIp4/pIp6 and 0 for all numeric
    // fields since only the instance name is needed for deregistration matching.
    let instance = unsafe {
        Dns::DnsServiceConstructInstance(
            &instance_hstring,
            &host_hstring,
            None,  // pIp4
            None,  // pIp6
            0,     // port
            0,     // priority
            0,     // weight
            0,     // property_count
            std::ptr::null(),  // keys
            std::ptr::null(),  // values
        )
    };

    let Some(instance_ptr) = NonNull::new(instance) else {
        trace!(
            name = %full_instance_name,
            "stale deregistration: DnsServiceConstructInstance returned null (ignored)"
        );
        return;
    };

    let instance = ServiceInstance(instance_ptr);

    // Build a deregistration request using the same callback infrastructure.
    let request = build_request(instance, None);

    // SAFETY: request references the instance owned by the CallbackContext.
    let result = unsafe { Dns::DnsServiceDeRegister(&request, None) };

    if result == DNS_REQUEST_PENDING {
        trace!(
            name = %full_instance_name,
            "stale deregistration submitted (pending callback)"
        );
        // The callback will fire asynchronously and reclaim the context.
    } else {
        // Reclaim the context since the callback won't fire.
        let context_ptr = request.pQueryContext as *mut CallbackContext;
        unsafe {
            drop(Box::from_raw(context_ptr));
        }
        trace!(
            name = %full_instance_name,
            status = result,
            "stale deregistration: no matching registration found (ignored)"
        );
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Submit a `DnsServiceRegister` call synchronously, returning the oneshot
/// receiver that will deliver the completion callback result.
///
/// This function exists to isolate the `DNS_SERVICE_REGISTER_REQUEST` (which
/// contains raw pointers and is not `Send`) from any `.await` point in the
/// calling async function. The request is created, submitted, and dropped
/// entirely within this synchronous scope.
fn submit_registration(
    instance: ServiceInstance,
) -> Result<oneshot::Receiver<(u32, ServiceInstance)>, Win32MdnsError> {
    let (tx, rx) = oneshot::channel::<(u32, ServiceInstance)>();

    let request = build_request(instance, Some(tx));

    // SAFETY: `request` references the instance and context, both owned by
    // the `CallbackContext` behind `context_ptr`, which stays alive until
    // the completion callback reclaims it.
    let result = unsafe { Dns::DnsServiceRegister(&request, None) };
    trace!("DnsServiceRegister result: {result}");

    if result != DNS_REQUEST_PENDING {
        // The callback will not fire; reclaim the context.
        let context_ptr = request.pQueryContext as *mut CallbackContext;
        unsafe {
            drop(Box::from_raw(context_ptr));
        }

        // Check if this is a "daemon not running" error.
        // Windows error 1753 = "The endpoint mapper service is unavailable"
        // Windows error 1062 = "The service has not been started"
        if result == 1753 || result == 1062 {
            return Err(Win32MdnsError::DaemonAbsent(format!(
                "DnsServiceRegister failed with status: {} (DNS Client service may be disabled)",
                result
            )));
        }

        return Err(Win32MdnsError::RegistrationFailed(format!(
            "DnsServiceRegister failed with status: {}",
            result
        )));
    }

    Ok(rx)
}

/// Build a `DNS_SERVICE_REGISTER_REQUEST` that transfers ownership of
/// `instance` into a freshly heap-allocated `CallbackContext`.
fn build_request(
    instance: ServiceInstance,
    status_tx: Option<oneshot::Sender<(u32, ServiceInstance)>>,
) -> Dns::DNS_SERVICE_REGISTER_REQUEST {
    let service_instance = instance.as_ptr();
    let context = Box::new(CallbackContext {
        status_tx,
        instance,
    });
    let context_ptr = Box::into_raw(context) as *mut c_void;

    Dns::DNS_SERVICE_REGISTER_REQUEST {
        Version: DNS_QUERY_REQUEST_VERSION1,
        InterfaceIndex: 0, // All interfaces
        pServiceInstance: service_instance,
        pRegisterCompletionCallback: Some(completion_callback),
        pQueryContext: context_ptr,
        ..Default::default()
    }
}

/// Completion callback invoked by the DNS-SD API once a register or
/// deregister request finishes.
extern "system" fn completion_callback(
    status: u32,
    context: *const c_void,
    _service_instance: *const Dns::DNS_SERVICE_INSTANCE,
) {
    // SAFETY: `context` was produced by `Box::into_raw` in `build_request`
    // and is reclaimed exactly once, here, when the OS invokes the callback.
    let ctx = unsafe { Box::from_raw(context as *mut CallbackContext) };
    let CallbackContext {
        status_tx,
        instance,
    } = *ctx;

    if status == ERROR_SUCCESS.0 {
        trace!("DNS-SD operation completed successfully");
    } else {
        error!("DNS-SD operation failed with status: {status}");
    }

    match status_tx {
        // Hand the instance back to the waiting caller.
        Some(status_tx) => {
            let _ = status_tx.send((status, instance));
        }
        // Fire-and-forget (drop on deregister): dropping instance frees it.
        None => drop(instance),
    }
}
