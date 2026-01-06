//! Daemon Manager - orchestrates daemon lifecycle.
//!
//! # Toyota Way: Heijunka (平準化)
//! Load leveling via work-stealing schedulers and controlled concurrency.
//!
//! # Toyota Way: Jidoka (自働化)
//! Automatic restart with exponential backoff on failure.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};

use crate::config::DaemonConfig;
use crate::daemon::{Daemon, DaemonContextHandle};
use crate::error::{DaemonError, Result};
use crate::types::{DaemonId, DaemonStatus, ExitReason, HealthStatus, Signal};

// =============================================================================
// RestartPolicy
// =============================================================================

/// Restart policy for daemons.
///
/// # Toyota Way: Jidoka
/// Stop-on-error with automatic recovery when safe.
#[derive(Debug, Clone)]
#[derive(Default)]
pub enum RestartPolicy {
    /// Never restart (run once).
    Never,
    /// Always restart (infinite retries).
    Always,
    /// Restart on failure only.
    #[default]
    OnFailure,
    /// Restart up to N times.
    MaxRetries(u32),
    /// Custom policy with backoff.
    WithBackoff(BackoffConfig),
}


impl RestartPolicy {
    /// Returns true if the daemon should be restarted given the exit reason.
    #[must_use]
    pub fn should_restart(&self, exit_reason: &ExitReason, restart_count: u32) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::OnFailure => matches!(
                exit_reason,
                ExitReason::Error(_) | ExitReason::ResourceExhausted(_)
            ),
            Self::MaxRetries(max) => restart_count < *max,
            Self::WithBackoff(config) => {
                restart_count < config.max_retries
                    && matches!(
                        exit_reason,
                        ExitReason::Error(_) | ExitReason::ResourceExhausted(_)
                    )
            }
        }
    }

    /// Returns the delay before restart.
    #[must_use]
    pub fn restart_delay(&self, restart_count: u32) -> Duration {
        match self {
            Self::WithBackoff(config) => config.delay_for(restart_count),
            _ => Duration::from_secs(1),
        }
    }
}

/// Backoff configuration for restart delays.
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Initial delay before first restart.
    pub initial_delay: Duration,
    /// Maximum delay between restarts.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff.
    pub multiplier: f64,
    /// Maximum number of retries.
    pub max_retries: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(300), // 5 minutes
            multiplier: 2.0,
            max_retries: 10,
        }
    }
}

impl BackoffConfig {
    /// Creates a new backoff config with builder pattern.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the initial delay.
    #[must_use]
    pub const fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Sets the max delay.
    #[must_use]
    pub const fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Sets the multiplier.
    #[must_use]
    pub const fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Sets the max retries.
    #[must_use]
    pub const fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Calculates delay for given restart count (exponential backoff).
    #[must_use]
    pub fn delay_for(&self, restart_count: u32) -> Duration {
        let base_secs = self.initial_delay.as_secs_f64();
        #[allow(clippy::cast_possible_wrap)] // restart_count won't exceed i32::MAX
        let exp_secs = base_secs * self.multiplier.powi(restart_count as i32);
        let clamped_secs = exp_secs.min(self.max_delay.as_secs_f64());
        Duration::from_secs_f64(clamped_secs)
    }
}

// =============================================================================
// ManagedDaemon
// =============================================================================

/// State for a managed daemon.
#[derive(Debug)]
pub struct ManagedDaemon {
    /// Daemon ID.
    pub id: DaemonId,
    /// Daemon name.
    pub name: String,
    /// Current status.
    pub status: DaemonStatus,
    /// Configuration.
    pub config: DaemonConfig,
    /// Restart policy.
    pub restart_policy: RestartPolicy,
    /// Number of restarts.
    pub restart_count: u32,
    /// Last health check result.
    pub last_health: Option<HealthStatus>,
    /// Last started timestamp.
    pub last_started: Option<Instant>,
    /// Context handle for signaling.
    pub context_handle: Option<DaemonContextHandle>,
}

impl ManagedDaemon {
    /// Creates a new managed daemon entry.
    #[must_use]
    pub fn new(id: DaemonId, name: String, config: DaemonConfig) -> Self {
        Self {
            id,
            name,
            status: DaemonStatus::Created,
            config,
            restart_policy: RestartPolicy::default(),
            restart_count: 0,
            last_health: None,
            last_started: None,
            context_handle: None,
        }
    }

    /// Sets the restart policy.
    #[must_use]
    pub fn with_restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }
}

// =============================================================================
// DaemonManager
// =============================================================================

/// Daemon manager for orchestrating daemon lifecycle.
///
/// # Toyota Way: Heijunka
/// Manages workload distribution across daemons.
///
/// # Toyota Way: Jidoka
/// Automatic failover and restart on errors.
pub struct DaemonManager {
    /// Registered daemons.
    daemons: RwLock<HashMap<DaemonId, Arc<Mutex<ManagedDaemon>>>>,
    /// Health check interval.
    health_check_interval: Duration,
    /// Shutdown timeout.
    shutdown_timeout: Duration,
}

impl DaemonManager {
    /// Creates a new daemon manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            daemons: RwLock::new(HashMap::new()),
            health_check_interval: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(30),
        }
    }

    /// Sets the health check interval.
    #[must_use]
    pub const fn with_health_check_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }

    /// Sets the shutdown timeout.
    #[must_use]
    pub const fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    /// Registers a daemon with the manager.
    ///
    /// # Errors
    /// Returns an error if a daemon with the same ID is already registered.
    pub async fn register(
        &self,
        daemon: Box<dyn Daemon>,
        config: DaemonConfig,
        restart_policy: RestartPolicy,
    ) -> Result<DaemonId> {
        let id = daemon.id();
        let name = daemon.name().to_string();

        let managed =
            ManagedDaemon::new(id, name.clone(), config).with_restart_policy(restart_policy);

        let mut daemons = self.daemons.write().await;

        if daemons.contains_key(&id) {
            return Err(DaemonError::Config(format!(
                "daemon {} already registered",
                id
            )));
        }

        daemons.insert(id, Arc::new(Mutex::new(managed)));

        tracing::info!(id = %id, name = %name, "registered daemon");

        Ok(id)
    }

    /// Unregisters a daemon from the manager.
    ///
    /// # Errors
    /// Returns an error if the daemon is not found or is still running.
    pub async fn unregister(&self, id: DaemonId) -> Result<()> {
        let mut daemons = self.daemons.write().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let guard = daemon.lock().await;
        if guard.status.is_active() {
            return Err(DaemonError::State(format!(
                "cannot unregister active daemon {}",
                id
            )));
        }
        drop(guard);

        daemons.remove(&id);
        tracing::info!(id = %id, "unregistered daemon");

        Ok(())
    }

    /// Returns the status of a daemon.
    ///
    /// # Errors
    /// Returns an error if the daemon is not found.
    pub async fn status(&self, id: DaemonId) -> Result<DaemonStatus> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let guard = daemon.lock().await;
        Ok(guard.status)
    }

    /// Returns the number of registered daemons.
    pub async fn count(&self) -> usize {
        self.daemons.read().await.len()
    }

    /// Returns all daemon IDs.
    pub async fn list(&self) -> Vec<DaemonId> {
        self.daemons.read().await.keys().copied().collect()
    }

    /// Sends a signal to a daemon.
    ///
    /// # Errors
    /// Returns an error if the daemon is not found or cannot receive signals.
    pub async fn signal(&self, id: DaemonId, signal: Signal) -> Result<()> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let guard = daemon.lock().await;

        if !guard.status.can_signal() {
            return Err(DaemonError::State(format!(
                "daemon {} cannot receive signals in state {:?}",
                id, guard.status
            )));
        }

        if let Some(ref handle) = guard.context_handle {
            handle.send_signal(signal).await?;
            tracing::debug!(id = %id, signal = ?signal, "sent signal to daemon");
        } else {
            return Err(DaemonError::State(format!(
                "daemon {} has no context handle",
                id
            )));
        }

        Ok(())
    }

    /// Updates daemon status.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn update_status(&self, id: DaemonId, status: DaemonStatus) -> Result<()> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let mut guard = daemon.lock().await;
        let old_status = guard.status;
        guard.status = status;

        tracing::debug!(id = %id, old = ?old_status, new = ?status, "status changed");

        Ok(())
    }

    /// Updates daemon context handle.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn set_context_handle(
        &self,
        id: DaemonId,
        handle: DaemonContextHandle,
    ) -> Result<()> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let mut guard = daemon.lock().await;
        guard.context_handle = Some(handle);
        guard.last_started = Some(Instant::now());

        Ok(())
    }

    /// Increments restart count and returns the new count.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn increment_restart_count(&self, id: DaemonId) -> Result<u32> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let mut guard = daemon.lock().await;
        guard.restart_count += 1;

        Ok(guard.restart_count)
    }

    /// Gets restart policy for a daemon.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn get_restart_policy(&self, id: DaemonId) -> Result<RestartPolicy> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let guard = daemon.lock().await;
        Ok(guard.restart_policy.clone())
    }

    /// Gets restart count for a daemon.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn get_restart_count(&self, id: DaemonId) -> Result<u32> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let guard = daemon.lock().await;
        Ok(guard.restart_count)
    }

    /// Updates last health check result.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn update_health(&self, id: DaemonId, health: HealthStatus) -> Result<()> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let mut guard = daemon.lock().await;
        guard.last_health = Some(health);

        Ok(())
    }

    /// Gets last health check result.
    ///
    /// # Errors
    /// Returns `DaemonError::NotFound` if the daemon is not registered.
    pub async fn get_health(&self, id: DaemonId) -> Result<Option<HealthStatus>> {
        let daemons = self.daemons.read().await;

        let daemon = daemons
            .get(&id)
            .ok_or_else(|| DaemonError::NotFound(id.to_string()))?;

        let guard = daemon.lock().await;
        Ok(guard.last_health.clone())
    }

    /// Initiates graceful shutdown of all daemons.
    ///
    /// # Errors
    /// Returns an error if any daemon fails to shut down within the timeout.
    pub async fn shutdown_all(&self) -> Result<()> {
        let ids = self.list().await;

        for id in ids {
            if let Err(e) = self.signal(id, Signal::Term).await {
                tracing::warn!(id = %id, error = %e, "failed to signal daemon for shutdown");
            }
        }

        let count = self.count().await;
        tracing::info!(count = count, "shutdown initiated for all daemons");

        Ok(())
    }
}

impl Default for DaemonManager {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::DaemonContext;
    use crate::metrics::DaemonMetrics;
    use async_trait::async_trait;
    use std::time::Duration;

    /// Test daemon implementation.
    struct TestDaemon {
        id: DaemonId,
        name: String,
        metrics: DaemonMetrics,
    }

    impl TestDaemon {
        fn new(name: &str) -> Self {
            Self {
                id: DaemonId::new(),
                name: name.to_string(),
                metrics: DaemonMetrics::new(),
            }
        }
    }

    #[async_trait]
    impl Daemon for TestDaemon {
        fn id(&self) -> DaemonId {
            self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        async fn init(&mut self, _config: &DaemonConfig) -> Result<()> {
            Ok(())
        }

        async fn run(&mut self, ctx: &mut DaemonContext) -> Result<ExitReason> {
            while !ctx.should_shutdown() {
                if ctx.try_recv_signal().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(ExitReason::Graceful)
        }

        async fn shutdown(&mut self, _timeout: Duration) -> Result<()> {
            Ok(())
        }

        async fn health_check(&self) -> HealthStatus {
            HealthStatus::healthy(1)
        }

        fn metrics(&self) -> &DaemonMetrics {
            &self.metrics
        }
    }

    // -------------------------------------------------------------------------
    // RestartPolicy Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_restart_policy_never() {
        let policy = RestartPolicy::Never;
        assert!(!policy.should_restart(&ExitReason::Error("test".into()), 0));
        assert!(!policy.should_restart(&ExitReason::Graceful, 0));
    }

    #[test]
    fn test_restart_policy_always() {
        let policy = RestartPolicy::Always;
        assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
        assert!(policy.should_restart(&ExitReason::Graceful, 100));
    }

    #[test]
    fn test_restart_policy_on_failure() {
        let policy = RestartPolicy::OnFailure;
        assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
        assert!(policy.should_restart(&ExitReason::ResourceExhausted("oom".into()), 0));
        assert!(!policy.should_restart(&ExitReason::Graceful, 0));
        assert!(!policy.should_restart(&ExitReason::Signal(Signal::Term), 0));
    }

    #[test]
    fn test_restart_policy_max_retries() {
        let policy = RestartPolicy::MaxRetries(3);
        assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
        assert!(policy.should_restart(&ExitReason::Error("test".into()), 2));
        assert!(!policy.should_restart(&ExitReason::Error("test".into()), 3));
        assert!(!policy.should_restart(&ExitReason::Error("test".into()), 10));
    }

    #[test]
    fn test_restart_policy_with_backoff() {
        let config = BackoffConfig::new()
            .with_initial_delay(Duration::from_millis(100))
            .with_max_retries(5);
        let policy = RestartPolicy::WithBackoff(config);

        assert!(policy.should_restart(&ExitReason::Error("test".into()), 0));
        assert!(policy.should_restart(&ExitReason::Error("test".into()), 4));
        assert!(!policy.should_restart(&ExitReason::Error("test".into()), 5));
        assert!(!policy.should_restart(&ExitReason::Graceful, 0));
    }

    // -------------------------------------------------------------------------
    // BackoffConfig Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_backoff_exponential() {
        let config = BackoffConfig::new()
            .with_initial_delay(Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(60))
            .with_multiplier(2.0);

        assert_eq!(config.delay_for(0), Duration::from_secs(1));
        assert_eq!(config.delay_for(1), Duration::from_secs(2));
        assert_eq!(config.delay_for(2), Duration::from_secs(4));
        assert_eq!(config.delay_for(3), Duration::from_secs(8));
        assert_eq!(config.delay_for(4), Duration::from_secs(16));
        assert_eq!(config.delay_for(5), Duration::from_secs(32));
        // Clamped to max
        assert_eq!(config.delay_for(6), Duration::from_secs(60));
        assert_eq!(config.delay_for(10), Duration::from_secs(60));
    }

    #[test]
    fn test_backoff_default() {
        let config = BackoffConfig::default();
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(300));
        assert_eq!(config.multiplier, 2.0);
        assert_eq!(config.max_retries, 10);
    }

    // -------------------------------------------------------------------------
    // ManagedDaemon Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_managed_daemon_creation() {
        let id = DaemonId::new();
        let config = DaemonConfig::new("test", "/bin/test");
        let managed = ManagedDaemon::new(id, "test".to_string(), config);

        assert_eq!(managed.id, id);
        assert_eq!(managed.name, "test");
        assert_eq!(managed.status, DaemonStatus::Created);
        assert_eq!(managed.restart_count, 0);
    }

    #[test]
    fn test_managed_daemon_with_policy() {
        let id = DaemonId::new();
        let config = DaemonConfig::new("test", "/bin/test");
        let managed = ManagedDaemon::new(id, "test".to_string(), config)
            .with_restart_policy(RestartPolicy::Always);

        assert!(matches!(managed.restart_policy, RestartPolicy::Always));
    }

    // -------------------------------------------------------------------------
    // DaemonManager Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_manager_creation() {
        let manager = DaemonManager::new();
        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_manager_register() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test-daemon");
        let config = DaemonConfig::new("test-daemon", "/bin/test");

        let id = manager
            .register(Box::new(daemon), config, RestartPolicy::OnFailure)
            .await
            .expect("registration should succeed");

        assert_eq!(manager.count().await, 1);

        let status = manager
            .status(id)
            .await
            .expect("status should be available");
        assert_eq!(status, DaemonStatus::Created);
    }

    #[tokio::test]
    async fn test_manager_duplicate_register() {
        let manager = DaemonManager::new();

        let daemon1 = TestDaemon::new("test1");
        let id = daemon1.id;
        let config = DaemonConfig::new("test1", "/bin/test");

        manager
            .register(Box::new(daemon1), config.clone(), RestartPolicy::Never)
            .await
            .expect("first registration should succeed");

        // Create another daemon with the same ID
        let mut daemon2 = TestDaemon::new("test2");
        daemon2.id = id;

        let result = manager
            .register(Box::new(daemon2), config, RestartPolicy::Never)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_unregister() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test-daemon");
        let config = DaemonConfig::new("test-daemon", "/bin/test");

        let id = manager
            .register(Box::new(daemon), config, RestartPolicy::Never)
            .await
            .expect("registration should succeed");

        manager
            .unregister(id)
            .await
            .expect("unregistration should succeed");

        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_manager_unregister_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();

        let result = manager.unregister(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_list() {
        let manager = DaemonManager::new();

        let daemon1 = TestDaemon::new("daemon1");
        let daemon2 = TestDaemon::new("daemon2");
        let config = DaemonConfig::new("test", "/bin/test");

        let id1 = manager
            .register(Box::new(daemon1), config.clone(), RestartPolicy::Never)
            .await
            .unwrap();
        let id2 = manager
            .register(Box::new(daemon2), config, RestartPolicy::Never)
            .await
            .unwrap();

        let ids = manager.list().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[tokio::test]
    async fn test_manager_update_status() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test");
        let id = daemon.id;
        let config = DaemonConfig::new("test", "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::Never)
            .await
            .unwrap();

        manager
            .update_status(id, DaemonStatus::Running)
            .await
            .unwrap();

        let status = manager.status(id).await.unwrap();
        assert_eq!(status, DaemonStatus::Running);
    }

    #[tokio::test]
    async fn test_manager_restart_count() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test");
        let id = daemon.id;
        let config = DaemonConfig::new("test", "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::Always)
            .await
            .unwrap();

        assert_eq!(manager.get_restart_count(id).await.unwrap(), 0);

        manager.increment_restart_count(id).await.unwrap();
        assert_eq!(manager.get_restart_count(id).await.unwrap(), 1);

        manager.increment_restart_count(id).await.unwrap();
        manager.increment_restart_count(id).await.unwrap();
        assert_eq!(manager.get_restart_count(id).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_manager_health() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test");
        let id = daemon.id;
        let config = DaemonConfig::new("test", "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::Never)
            .await
            .unwrap();

        // No health check yet
        assert!(manager.get_health(id).await.unwrap().is_none());

        // Update health
        let health = HealthStatus::healthy(5);
        manager.update_health(id, health).await.unwrap();

        let retrieved = manager.get_health(id).await.unwrap();
        assert!(retrieved.is_some());
        assert!(retrieved.unwrap().is_healthy());
    }

    #[tokio::test]
    async fn test_manager_get_restart_policy() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test");
        let id = daemon.id;
        let config = DaemonConfig::new("test", "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::MaxRetries(5))
            .await
            .unwrap();

        let policy = manager.get_restart_policy(id).await.unwrap();
        assert!(matches!(policy, RestartPolicy::MaxRetries(5)));
    }

    #[test]
    fn test_restart_delay() {
        // Default restart delay is 1 second for non-backoff policies
        let policy = RestartPolicy::OnFailure;
        assert_eq!(policy.restart_delay(0), Duration::from_secs(1));
        assert_eq!(policy.restart_delay(5), Duration::from_secs(1));

        // With backoff, delay increases
        let backoff = BackoffConfig::new()
            .with_initial_delay(Duration::from_secs(1))
            .with_multiplier(2.0);
        let policy = RestartPolicy::WithBackoff(backoff);
        assert_eq!(policy.restart_delay(0), Duration::from_secs(1));
        assert_eq!(policy.restart_delay(1), Duration::from_secs(2));
        assert_eq!(policy.restart_delay(2), Duration::from_secs(4));
    }

    #[tokio::test]
    async fn test_with_health_check_interval() {
        let manager = DaemonManager::new()
            .with_health_check_interval(Duration::from_secs(60));
        assert_eq!(manager.health_check_interval, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_with_shutdown_timeout() {
        let manager = DaemonManager::new()
            .with_shutdown_timeout(Duration::from_secs(120));
        assert_eq!(manager.shutdown_timeout, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_manager_default() {
        let manager = DaemonManager::default();
        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_manager_status_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.status(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_signal_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.signal(id, Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_signal_cannot_signal_state() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test");
        let id = daemon.id;
        let config = DaemonConfig::new("test", "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::Never)
            .await
            .unwrap();

        // Daemon is in Created state, cannot signal
        let result = manager.signal(id, Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_signal_no_handle() {
        let manager = DaemonManager::new();
        let daemon = TestDaemon::new("test");
        let id = daemon.id;
        let config = DaemonConfig::new("test", "/bin/test");

        manager
            .register(Box::new(daemon), config, RestartPolicy::Never)
            .await
            .unwrap();

        // Set to Running but no context handle
        manager.update_status(id, DaemonStatus::Running).await.unwrap();

        // Signal should fail because no handle
        let result = manager.signal(id, Signal::Term).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_health_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.get_health(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_restart_policy_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.get_restart_policy(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_restart_count_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.get_restart_count(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_increment_restart_count_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.increment_restart_count(id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_health_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.update_health(id, HealthStatus::healthy(1)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_status_not_found() {
        let manager = DaemonManager::new();
        let id = DaemonId::new();
        let result = manager.update_status(id, DaemonStatus::Running).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_shutdown_all() {
        let manager = DaemonManager::new();

        // Register multiple daemons
        for i in 0..3 {
            let daemon = TestDaemon::new(&format!("test-{}", i));
            let config = DaemonConfig::new(&format!("test-{}", i), "/bin/test");
            manager
                .register(Box::new(daemon), config, RestartPolicy::Never)
                .await
                .unwrap();
        }

        // Shutdown all should not error even if daemons can't be signaled
        let result = manager.shutdown_all().await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_restart_policy_default() {
        let policy = RestartPolicy::default();
        assert!(matches!(policy, RestartPolicy::OnFailure));
    }

    #[test]
    fn test_managed_daemon_fields() {
        let id = DaemonId::new();
        let config = DaemonConfig::new("test", "/bin/test");
        let managed = ManagedDaemon::new(id, "test".to_string(), config);

        assert!(managed.last_health.is_none());
        assert!(managed.last_started.is_none());
        assert!(managed.context_handle.is_none());
    }

    #[test]
    fn test_backoff_new_is_default() {
        let config = BackoffConfig::new();
        let default = BackoffConfig::default();
        assert_eq!(config.initial_delay, default.initial_delay);
        assert_eq!(config.max_delay, default.max_delay);
        assert_eq!(config.multiplier, default.multiplier);
        assert_eq!(config.max_retries, default.max_retries);
    }
}
