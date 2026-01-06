//! Platform detection.
//!
//! # Toyota Way: Poka-Yoke (ポカヨケ)
//! Auto-detect current platform with fallback chain.
//! Fail to safest option (Native) if detection fails.

use std::path::Path;

/// Supported platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Linux with systemd.
    Linux,
    /// macOS with launchd.
    MacOS,
    /// Docker/OCI container.
    Container,
    /// Pepita microVM.
    PepitaMicroVM,
    /// WebAssembly OS.
    Wos,
    /// Native process (fallback).
    Native,
}

impl Platform {
    /// Returns the platform name.
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
    // WOS: Check for WASM runtime
    if cfg!(target_arch = "wasm32") || std::env::var("WOS_KERNEL").is_ok() {
        return Platform::Wos;
    }

    // pepita: Check for virtio devices or env marker
    if Path::new("/dev/virtio-ports").exists() || std::env::var("PEPITA_VM").is_ok() {
        return Platform::PepitaMicroVM;
    }

    // Container: Check for Docker/containerd markers
    if is_container() {
        return Platform::Container;
    }

    // Linux: Check for systemd
    #[cfg(target_os = "linux")]
    if Path::new("/run/systemd/system").exists() {
        return Platform::Linux;
    }

    // macOS
    #[cfg(target_os = "macos")]
    return Platform::MacOS;

    // Fallback: Native
    Platform::Native
}

/// Checks if running inside a container.
fn is_container() -> bool {
    // Docker marker file
    if Path::new("/.dockerenv").exists() {
        return true;
    }

    // cgroup markers
    if let Ok(content) = std::fs::read_to_string("/proc/1/cgroup")
        && (content.contains("docker") || content.contains("containerd") || content.contains("lxc"))
    {
        return true;
    }

    // Container environment variable
    if std::env::var("container").is_ok() {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_name() {
        assert_eq!(Platform::Linux.name(), "linux");
        assert_eq!(Platform::Native.name(), "native");
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Linux.to_string(), "linux");
    }

    #[test]
    fn test_detect_platform_returns_valid() {
        let platform = detect_platform();
        // Should return some valid platform
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
}
