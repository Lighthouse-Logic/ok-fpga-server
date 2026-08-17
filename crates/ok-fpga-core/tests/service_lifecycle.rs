//! # Service Lifecycle Integration Tests
//!
//! Tests the ServiceManager's ability to start, stop, and restart services
//! with event broadcasting — the core orchestration logic from ADR-010.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use ok_fpga_core::service::{ManagedService, ServiceError, ServiceStatus};
use ok_fpga_core::service_manager::ServiceManager;

// ============================================================================
// Test Service Implementations
// ============================================================================

/// A mock service that tracks start/stop calls.
struct MockService {
    name: &'static str,
    status: AtomicU8,
    start_count: std::sync::atomic::AtomicU32,
    stop_count: std::sync::atomic::AtomicU32,
}

impl MockService {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            status: AtomicU8::new(0),
            start_count: std::sync::atomic::AtomicU32::new(0),
            stop_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn start_count(&self) -> u32 {
        self.start_count.load(Ordering::SeqCst)
    }

    fn stop_count(&self) -> u32 {
        self.stop_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl ManagedService for MockService {
    fn name(&self) -> &str {
        self.name
    }

    fn status(&self) -> ServiceStatus {
        match self.status.load(Ordering::SeqCst) {
            0 => ServiceStatus::Stopped,
            1 => ServiceStatus::Running,
            _ => ServiceStatus::Failed {
                reason: "unknown".to_string(),
            },
        }
    }

    async fn start(&self) -> Result<(), ServiceError> {
        self.start_count.fetch_add(1, Ordering::SeqCst);
        self.status.store(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        self.status.store(0, Ordering::SeqCst);
        Ok(())
    }
}

/// A mock service that fails on start.
struct FailingService;

#[async_trait::async_trait]
impl ManagedService for FailingService {
    fn name(&self) -> &str {
        "failing"
    }

    fn status(&self) -> ServiceStatus {
        ServiceStatus::Failed {
            reason: "always fails".to_string(),
        }
    }

    async fn start(&self) -> Result<(), ServiceError> {
        Err(ServiceError::OperationFailed {
            name: "failing".to_string(),
            message: "intentional failure for testing".to_string(),
        })
    }

    async fn stop(&self) -> Result<(), ServiceError> {
        Ok(())
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn service_manager_full_lifecycle() {
    let sm = ServiceManager::new();

    let rest = Arc::new(MockService::new("rest"));
    let grpc = Arc::new(MockService::new("grpc"));
    let mqtt = Arc::new(MockService::new("mqtt"));

    sm.register(rest.clone(), true);
    sm.register(grpc.clone(), true);
    sm.register(mqtt.clone(), false); // registered but disabled

    // start_enabled should only start rest and grpc.
    sm.start_enabled().await;

    assert_eq!(rest.start_count(), 1);
    assert_eq!(grpc.start_count(), 1);
    assert_eq!(mqtt.start_count(), 0);

    let list = sm.list();
    assert_eq!(list.len(), 3);

    // Stop all should stop rest and grpc (mqtt was never started).
    sm.stop_all().await;

    assert_eq!(rest.stop_count(), 1);
    assert_eq!(grpc.stop_count(), 1);
    assert_eq!(mqtt.stop_count(), 0); // was never started, so not stopped
}

#[tokio::test]
async fn service_manager_restart_cycles() {
    let sm = ServiceManager::new();
    let svc = Arc::new(MockService::new("test"));
    sm.register(svc.clone(), true);

    // Start.
    sm.start_enabled().await;
    assert_eq!(svc.start_count(), 1);

    // Restart via stop + start.
    let _ = sm.restart("test").await;
    assert_eq!(svc.stop_count(), 1);
    assert_eq!(svc.start_count(), 2);

    // Restart again.
    let _ = sm.restart("test").await;
    assert_eq!(svc.stop_count(), 2);
    assert_eq!(svc.start_count(), 3);
}

#[tokio::test]
async fn service_manager_event_subscription() {
    use ok_fpga_core::service::ServiceEvent;

    let sm = ServiceManager::new();
    let svc = Arc::new(MockService::new("evented"));
    sm.register(svc.clone(), true);

    let mut rx = sm.subscribe();

    // Start service and check event.
    sm.start_enabled().await;
    let event = rx.recv().await.expect("should receive start event");
    match event {
        ServiceEvent::Started { ref name } => {
            assert_eq!(name, "evented");
        }
        _ => panic!("expected Started event, got: {:?}", event),
    }
}

#[tokio::test]
async fn service_manager_handles_start_failure_gracefully() {
    let sm = ServiceManager::new();
    let good = Arc::new(MockService::new("good"));
    let bad = Arc::new(FailingService);

    sm.register(good.clone(), true);
    sm.register(bad, true);

    // start_enabled should not panic — it should log and continue.
    sm.start_enabled().await;

    // Good service should still have started.
    assert_eq!(good.start_count(), 1);
}

#[tokio::test]
async fn service_manager_list_shows_all_services() {
    let sm = ServiceManager::new();

    sm.register(Arc::new(MockService::new("alpha")), true);
    sm.register(Arc::new(MockService::new("beta")), false);
    sm.register(Arc::new(MockService::new("gamma")), true);

    let list = sm.list();
    assert_eq!(list.len(), 3);

    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
    assert!(names.contains(&"gamma"));
}
