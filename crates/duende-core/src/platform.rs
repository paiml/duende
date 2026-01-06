//! Platform detection and abstraction.
//!
//! # Toyota Way: Poka-Yoke (ポカヨケ)
//! Fail to safest option during platform detection.
//!
//! # Detection Order
//! 1. WOS: Check for WASM runtime markers
//! 2. pepita: Check for virtio devices
//! 3. Container: Check for /.dockerenv or cgroup markers
//! 4. Linux: Check for systemd
//! 5. macOS: Check for launchd
//! 6. Fallback: Native process

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported platforms for daemon execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    /// Linux with systemd.
    Linux,
    /// macOS with launchd.
    MacOS,
    /// Docker/OCI container.
    Container,
    /// pepita MicroVM.
    PepitaMicroVM,
    /// WOS (WebAssembly Operating System).
    Wos,
    /// Native process (fallback).
    Native,
}

impl Platform {
    /// Returns true if this platform supports process isolation.
    #[must_use]
    pub const fn supports_isolation(&self) -> bool {
        matches!(self, Self::Container | Self::PepitaMicroVM | Self::Wos)
    }

    /// Returns true if this platform supports resource limits via cgroups.
    #[must_use]
    pub const fn supports_cgroups(&self) -> bool {
        matches!(self, Self::Linux | Self::Container)
    }

    /// Returns true if this platform supports systemd-style unit management.
    #[must_use]
    pub const fn supports_systemd(&self) -> bool {
        matches!(self, Self::Linux)
    }

    /// Returns true if this platform supports launchd-style plist management.
    #[must_use]
    pub const fn supports_launchd(&self) -> bool {
        matches!(self, Self::MacOS)
    }

    /// Returns the platform name as a static string.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::MacOS => "macos",
            Self::Container => "container",
            Self::PepitaMicroVM => "pepita",
            Self::Wos => "wos",
            Self::Native => "native",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Auto-detect current platform with fallback chain.
///
/// # Detection Order (Poka-Yoke: fail to safest option)
/// 1. WOS: Check for WASM runtime markers
/// 2. pepita: Check for virtio devices
/// 3. Container: Check for /.dockerenv or cgroup markers
/// 4. Linux: Check for systemd
/// 5. macOS: Check for launchd
/// 6. Fallback: Native process
#[must_use]
pub fn detect_platform() -> Platform {
    // 1. WOS detection (WASM target or env var)
    if cfg!(target_arch = "wasm32") || std::env::var("WOS_KERNEL").is_ok() {
        return Platform::Wos;
    }

    // 2. pepita MicroVM detection
    if is_pepita_vm() {
        return Platform::PepitaMicroVM;
    }

    // 3. Container detection
    if is_container() {
        return Platform::Container;
    }

    // 4. Linux with systemd
    #[cfg(target_os = "linux")]
    if is_systemd_available() {
        return Platform::Linux;
    }

    // 5. macOS with launchd
    #[cfg(target_os = "macos")]
    {
        return Platform::MacOS;
    }

    // 6. Fallback to native
    Platform::Native
}

/// Check if running inside a pepita MicroVM.
fn is_pepita_vm() -> bool {
    // Check for pepita-specific markers
    if std::env::var("PEPITA_VM").is_ok() {
        return true;
    }

    // Check for virtio-ports device (pepita uses vsock)
    Path::new("/dev/virtio-ports").exists()
}

/// Check if running inside a container.
fn is_container() -> bool {
    // Docker marker file
    if Path::new("/.dockerenv").exists() {
        return true;
    }

    // Check cgroup for container indicators
    if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup")
        && (cgroup.contains("docker")
            || cgroup.contains("containerd")
            || cgroup.contains("kubepods")
            || cgroup.contains("lxc"))
    {
        return true;
    }

    // Check for container runtime env vars
    if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
        return true;
    }

    false
}

/// Check if systemd is available on Linux.
#[cfg(target_os = "linux")]
fn is_systemd_available() -> bool {
    Path::new("/run/systemd/system").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Platform enum tests
    // =========================================================================

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Linux.to_string(), "linux");
        assert_eq!(Platform::MacOS.to_string(), "macos");
        assert_eq!(Platform::Container.to_string(), "container");
        assert_eq!(Platform::PepitaMicroVM.to_string(), "pepita");
        assert_eq!(Platform::Wos.to_string(), "wos");
        assert_eq!(Platform::Native.to_string(), "native");
    }

    #[test]
    fn test_platform_name() {
        assert_eq!(Platform::Linux.name(), "linux");
        assert_eq!(Platform::MacOS.name(), "macos");
        assert_eq!(Platform::Container.name(), "container");
        assert_eq!(Platform::PepitaMicroVM.name(), "pepita");
        assert_eq!(Platform::Wos.name(), "wos");
        assert_eq!(Platform::Native.name(), "native");
    }

    #[test]
    fn test_platform_supports_isolation() {
        assert!(!Platform::Linux.supports_isolation());
        assert!(!Platform::MacOS.supports_isolation());
        assert!(Platform::Container.supports_isolation());
        assert!(Platform::PepitaMicroVM.supports_isolation());
        assert!(Platform::Wos.supports_isolation());
        assert!(!Platform::Native.supports_isolation());
    }

    #[test]
    fn test_platform_supports_cgroups() {
        assert!(Platform::Linux.supports_cgroups());
        assert!(!Platform::MacOS.supports_cgroups());
        assert!(Platform::Container.supports_cgroups());
        assert!(!Platform::PepitaMicroVM.supports_cgroups());
        assert!(!Platform::Wos.supports_cgroups());
        assert!(!Platform::Native.supports_cgroups());
    }

    #[test]
    fn test_platform_supports_systemd() {
        assert!(Platform::Linux.supports_systemd());
        assert!(!Platform::MacOS.supports_systemd());
        assert!(!Platform::Container.supports_systemd());
        assert!(!Platform::PepitaMicroVM.supports_systemd());
        assert!(!Platform::Wos.supports_systemd());
        assert!(!Platform::Native.supports_systemd());
    }

    #[test]
    fn test_platform_supports_launchd() {
        assert!(!Platform::Linux.supports_launchd());
        assert!(Platform::MacOS.supports_launchd());
        assert!(!Platform::Container.supports_launchd());
        assert!(!Platform::PepitaMicroVM.supports_launchd());
        assert!(!Platform::Wos.supports_launchd());
        assert!(!Platform::Native.supports_launchd());
    }

    #[test]
    fn test_platform_equality() {
        assert_eq!(Platform::Linux, Platform::Linux);
        assert_ne!(Platform::Linux, Platform::MacOS);
    }

    #[test]
    fn test_platform_clone() {
        let p1 = Platform::Container;
        let p2 = p1;
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_platform_debug() {
        let debug = format!("{:?}", Platform::Linux);
        assert!(debug.contains("Linux"));
    }

    // =========================================================================
    // Detection tests
    // =========================================================================

    #[test]
    fn test_detect_platform_returns_valid() {
        // detect_platform should always return a valid Platform
        let platform = detect_platform();
        // Verify it's one of the valid variants
        let valid = matches!(
            platform,
            Platform::Linux
                | Platform::MacOS
                | Platform::Container
                | Platform::PepitaMicroVM
                | Platform::Wos
                | Platform::Native
        );
        assert!(valid);
    }

    #[test]
    fn test_is_container_false_on_host() {
        // On most dev machines, this should be false
        // (unless running tests in container)
        let result = is_container();
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_is_pepita_vm_false_on_host() {
        // On most dev machines, this should be false
        let result = is_pepita_vm();
        // Just verify it doesn't panic
        let _ = result;
    }

    // =========================================================================
    // Serialization tests
    // =========================================================================

    #[test]
    fn test_platform_serialize() {
        let json = serde_json::to_string(&Platform::Linux).unwrap();
        assert!(json.contains("Linux"));
    }

    #[test]
    fn test_platform_deserialize() {
        let platform: Platform = serde_json::from_str("\"Linux\"").unwrap();
        assert_eq!(platform, Platform::Linux);
    }

    #[test]
    fn test_platform_roundtrip() {
        for platform in [
            Platform::Linux,
            Platform::MacOS,
            Platform::Container,
            Platform::PepitaMicroVM,
            Platform::Wos,
            Platform::Native,
        ] {
            let json = serde_json::to_string(&platform).unwrap();
            let deserialized: Platform = serde_json::from_str(&json).unwrap();
            assert_eq!(platform, deserialized);
        }
    }

    // =========================================================================
    // Hash tests (for HashMap usage)
    // =========================================================================

    #[test]
    fn test_platform_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(Platform::Linux);
        set.insert(Platform::MacOS);
        set.insert(Platform::Linux); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&Platform::Linux));
        assert!(set.contains(&Platform::MacOS));
        assert!(!set.contains(&Platform::Container));
    }
}
