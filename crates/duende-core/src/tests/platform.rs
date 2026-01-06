//! Category F: Platform Adapter falsification tests (F101-F110).
//!
//! These tests verify the platform adapter properties defined in daemon-tools-spec.md
//! Section 9.2 using Popperian falsification methodology.

use crate::adapter::{DaemonHandle, PlatformAdapter};
use crate::adapters::{
    select_adapter, ContainerAdapter, LaunchdAdapter, NativeAdapter,
    PepitaAdapter, SystemdAdapter, WosAdapter,
};
use crate::platform::{detect_platform, Platform};
use crate::tests::mocks::MockDaemon;
use crate::types::{DaemonId, Signal};

/// F101: Platform detection returns valid platform
#[test]
fn f101_platform_detection_valid() {
    let platform = detect_platform();

    // Should return one of the valid platform variants
    assert!(matches!(
        platform,
        Platform::Linux
            | Platform::MacOS
            | Platform::Container
            | Platform::PepitaMicroVM
            | Platform::Wos
            | Platform::Native
    ));
}

/// F102: select_adapter returns correct type
#[test]
fn f102_select_adapter_correct_type() {
    // Each platform should get an adapter that reports that platform
    let platforms = [
        Platform::Linux,
        Platform::MacOS,
        Platform::Container,
        Platform::PepitaMicroVM,
        Platform::Wos,
        Platform::Native,
    ];

    for platform in platforms {
        let adapter = select_adapter(platform);
        assert_eq!(
            adapter.platform(),
            platform,
            "Adapter for {:?} should report {:?}",
            platform,
            platform
        );
    }
}

/// F103: Native adapter spawns process
#[tokio::test]
async fn f103_native_adapter_spawns() {
    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");

    // Handle should be valid
    assert!(!handle.id().as_uuid().is_nil());
}

/// F104: Native adapter tracks PID
#[tokio::test]
async fn f104_native_adapter_tracks_pid() {
    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");

    // Should have a valid PID
    if let crate::adapter::HandleData::Native { pid, .. } = handle.handle_data() {
        assert!(*pid > 0, "PID should be positive");
    } else {
        panic!("Expected Native handle");
    }
}

/// F105: Native adapter signals process
#[tokio::test]
async fn f105_native_adapter_signals() {
    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");

    // Should be able to send a non-terminating signal
    adapter
        .signal(&handle, Signal::Usr1)
        .await
        .expect("signal should succeed");

    // Clean up
    adapter.signal(&handle, Signal::Kill).await.ok();
}

/// F106: Native adapter reports status
#[tokio::test]
async fn f106_native_adapter_reports_status() {
    use crate::types::DaemonStatus;

    let adapter = NativeAdapter::new();
    let daemon = MockDaemon::new("test");

    let handle = adapter.spawn(Box::new(daemon)).await.expect("spawn should succeed");

    // Should be running
    let status = adapter.status(&handle).await.expect("status should succeed");
    assert_eq!(status, DaemonStatus::Running);

    // Kill and check status
    adapter.signal(&handle, Signal::Kill).await.ok();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let status = adapter.status(&handle).await.expect("status should succeed");
    assert!(status.is_terminal(), "Should be in terminal state after kill");
}

/// F107: Stub adapters return NotSupported
#[tokio::test]
async fn f107_stub_adapters_not_supported() {
    // Test all stub adapters
    let adapters: Vec<Box<dyn PlatformAdapter>> = vec![
        Box::new(SystemdAdapter::new()),
        Box::new(LaunchdAdapter::new()),
        Box::new(ContainerAdapter::docker()),
        Box::new(PepitaAdapter::new()),
        Box::new(WosAdapter::new()),
    ];

    for adapter in adapters {
        let daemon = MockDaemon::new("test");
        let result = adapter.spawn(Box::new(daemon)).await;
        assert!(result.is_err(), "Stub adapter spawn should fail");

        let err = result.unwrap_err();
        assert!(err.is_not_supported(), "Error should be NotSupported");
    }
}

/// F108: Handle serialization roundtrips
#[test]
fn f108_handle_serialization_roundtrips() {
    let handles = [
        DaemonHandle::native(DaemonId::new(), 12345),
        DaemonHandle::systemd(DaemonId::new(), "test.service"),
        DaemonHandle::launchd(DaemonId::new(), "com.test.daemon"),
        DaemonHandle::container(DaemonId::new(), "docker", "abc123"),
        DaemonHandle::pepita(DaemonId::new(), "vm-123", 5000),
        DaemonHandle::wos(DaemonId::new(), 42),
    ];

    for original in handles {
        let json = serde_json::to_string(&original).expect("serialize should succeed");
        let deserialized: DaemonHandle =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(original.id(), deserialized.id());
    }
}

/// F109: Handle display is informative
#[test]
fn f109_handle_display_informative() {
    let id = DaemonId::new();

    let handle = DaemonHandle::native(id, 12345);
    let display = format!("{:?}", handle);
    assert!(display.contains("Native"), "Display should mention platform");

    let handle = DaemonHandle::systemd(id, "test.service");
    let display = format!("{:?}", handle);
    assert!(display.contains("Systemd"), "Display should mention platform");

    let handle = DaemonHandle::container(id, "docker", "abc123");
    let display = format!("{:?}", handle);
    assert!(display.contains("Container"), "Display should mention platform");
}

/// F110: Platform isolation flags correct
#[test]
fn f110_platform_isolation_flags() {
    // Linux supports cgroups isolation
    assert!(
        Platform::Linux.supports_cgroups(),
        "Linux should support cgroups"
    );
    assert!(!Platform::MacOS.supports_cgroups());

    // macOS supports launchd
    assert!(
        Platform::MacOS.supports_launchd(),
        "macOS should support launchd"
    );
    assert!(!Platform::Linux.supports_launchd());

    // Linux supports systemd
    assert!(
        Platform::Linux.supports_systemd(),
        "Linux should support systemd"
    );
    assert!(!Platform::MacOS.supports_systemd());

    // VM/Container platforms support isolation
    assert!(
        Platform::Container.supports_isolation(),
        "Container should support isolation"
    );
    assert!(
        Platform::PepitaMicroVM.supports_isolation(),
        "pepita should support isolation"
    );
}
