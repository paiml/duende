//! Configuration for memory locking behavior.

/// Configuration for memory locking operations.
///
/// Use [`MlockConfig::builder()`] for a fluent configuration API:
///
/// ```rust
/// use duende_mlock::MlockConfig;
///
/// let config = MlockConfig::builder()
///     .current(true)      // Lock pages currently mapped
///     .future(true)       // Lock pages mapped in the future
///     .required(false)    // Don't fail if mlock fails
///     .build();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // These are independent flags, not state
pub struct MlockConfig {
    /// Lock pages currently mapped into the address space (`MCL_CURRENT`).
    current: bool,
    /// Lock pages that become mapped in the future (`MCL_FUTURE`).
    future: bool,
    /// Whether memory locking failure is fatal.
    ///
    /// - `true`: Return `Err` if mlock fails
    /// - `false`: Return `Ok(MlockStatus::Failed { .. })` if mlock fails
    required: bool,
    /// Lock pages only when they are faulted in (`MCL_ONFAULT`).
    ///
    /// Available on Linux 4.4+. Reduces initial memory footprint by
    /// deferring locking until pages are actually accessed.
    ///
    /// Ignored on platforms that don't support it.
    onfault: bool,
}

impl MlockConfig {
    /// Create a new configuration builder.
    ///
    /// # Example
    ///
    /// ```rust
    /// use duende_mlock::MlockConfig;
    ///
    /// let config = MlockConfig::builder()
    ///     .required(false)
    ///     .build();
    /// ```
    #[must_use]
    pub const fn builder() -> MlockConfigBuilder {
        MlockConfigBuilder::new()
    }

    /// Whether to lock currently mapped pages.
    #[must_use]
    pub const fn current(&self) -> bool {
        self.current
    }

    /// Whether to lock future page mappings.
    #[must_use]
    pub const fn future(&self) -> bool {
        self.future
    }

    /// Whether mlock failure is fatal.
    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }

    /// Whether to use on-fault locking (Linux 4.4+).
    #[must_use]
    pub const fn onfault(&self) -> bool {
        self.onfault
    }

    /// Convert to libc mlockall flags.
    #[cfg(unix)]
    pub(crate) const fn as_flags(self) -> libc::c_int {
        let mut flags = 0;

        if self.current {
            flags |= libc::MCL_CURRENT;
        }

        if self.future {
            flags |= libc::MCL_FUTURE;
        }

        // MCL_ONFAULT is Linux 4.4+ only
        #[cfg(target_os = "linux")]
        if self.onfault {
            // MCL_ONFAULT = 4 (not always in libc)
            const MCL_ONFAULT: libc::c_int = 4;
            flags |= MCL_ONFAULT;
        }

        flags
    }
}

impl Default for MlockConfig {
    /// Default configuration: lock current and future pages, required mode.
    ///
    /// Equivalent to:
    /// ```rust
    /// use duende_mlock::MlockConfig;
    ///
    /// let config = MlockConfig::builder()
    ///     .current(true)
    ///     .future(true)
    ///     .required(true)
    ///     .onfault(false)
    ///     .build();
    /// ```
    fn default() -> Self {
        Self {
            current: true,
            future: true,
            required: true,
            onfault: false,
        }
    }
}

/// Builder for [`MlockConfig`].
///
/// # Example
///
/// ```rust
/// use duende_mlock::MlockConfig;
///
/// let config = MlockConfig::builder()
///     .current(true)
///     .future(true)
///     .required(false)
///     .build();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MlockConfigBuilder {
    config: MlockConfig,
}

impl MlockConfigBuilder {
    /// Create a new builder with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            config: MlockConfig {
                current: true,
                future: true,
                required: true,
                onfault: false,
            },
        }
    }

    /// Lock pages currently mapped (`MCL_CURRENT`).
    ///
    /// Default: `true`
    #[must_use]
    pub const fn current(mut self, value: bool) -> Self {
        self.config.current = value;
        self
    }

    /// Lock pages mapped in the future (`MCL_FUTURE`).
    ///
    /// Default: `true`
    #[must_use]
    pub const fn future(mut self, value: bool) -> Self {
        self.config.future = value;
        self
    }

    /// Whether mlock failure should return an error.
    ///
    /// - `true` (default): Return `Err(MlockError)` on failure
    /// - `false`: Return `Ok(MlockStatus::Failed { .. })` on failure
    ///
    /// Use `false` when mlock is optional and the daemon should continue
    /// with degraded safety guarantees.
    #[must_use]
    pub const fn required(mut self, value: bool) -> Self {
        self.config.required = value;
        self
    }

    /// Use on-fault locking (`MCL_ONFAULT`, Linux 4.4+).
    ///
    /// When enabled, pages are locked only when they are first accessed
    /// (faulted in), rather than immediately. This reduces initial memory
    /// pressure for daemons with large potential address spaces.
    ///
    /// Default: `false`
    ///
    /// Ignored on platforms that don't support it.
    #[must_use]
    pub const fn onfault(mut self, value: bool) -> Self {
        self.config.onfault = value;
        self
    }

    /// Build the configuration.
    #[must_use]
    pub const fn build(self) -> MlockConfig {
        self.config
    }
}

impl Default for MlockConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let config = MlockConfig::default();
        assert!(config.current());
        assert!(config.future());
        assert!(config.required());
        assert!(!config.onfault());
    }

    #[test]
    fn test_builder_all_false() {
        let config = MlockConfig::builder()
            .current(false)
            .future(false)
            .required(false)
            .onfault(false)
            .build();

        assert!(!config.current());
        assert!(!config.future());
        assert!(!config.required());
        assert!(!config.onfault());
    }

    #[test]
    fn test_builder_all_true() {
        let config = MlockConfig::builder()
            .current(true)
            .future(true)
            .required(true)
            .onfault(true)
            .build();

        assert!(config.current());
        assert!(config.future());
        assert!(config.required());
        assert!(config.onfault());
    }

    #[cfg(unix)]
    #[test]
    fn test_as_flags_default() {
        let config = MlockConfig::default();
        let flags = config.as_flags();
        assert_eq!(flags, libc::MCL_CURRENT | libc::MCL_FUTURE);
    }

    #[cfg(unix)]
    #[test]
    fn test_as_flags_current_only() {
        let config = MlockConfig::builder()
            .current(true)
            .future(false)
            .build();
        let flags = config.as_flags();
        assert_eq!(flags, libc::MCL_CURRENT);
    }

    #[cfg(unix)]
    #[test]
    fn test_as_flags_none() {
        let config = MlockConfig::builder()
            .current(false)
            .future(false)
            .build();
        let flags = config.as_flags();
        assert_eq!(flags, 0);
    }
}
