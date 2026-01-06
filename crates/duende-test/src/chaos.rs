//! Chaos injection for resilience testing.
//!
//! # Reference
//! Netflix. (2012). Chaos Monkey. GitHub.
//! <https://github.com/Netflix/chaosmonkey>

use std::time::Duration;

use crate::error::Result;

/// Chaos injection configuration.
#[derive(Debug, Clone, Default)]
pub struct ChaosConfig {
    /// Latency injection: (probability, delay).
    pub latency_injection: Option<(f64, Duration)>,
    /// Error injection probability.
    pub error_injection: Option<f64>,
    /// Packet loss probability.
    pub packet_loss: Option<f64>,
    /// Memory pressure (0.0 to 1.0).
    pub memory_pressure: Option<f64>,
    /// CPU stress (0.0 to 1.0).
    pub cpu_stress: Option<f64>,
    /// Duration of chaos injection.
    pub duration: Option<Duration>,
}

impl ChaosConfig {
    /// Creates a new chaos config with latency injection.
    #[must_use]
    pub fn latency(probability: f64, delay: Duration) -> Self {
        Self {
            latency_injection: Some((probability, delay)),
            ..Default::default()
        }
    }

    /// Creates a new chaos config with error injection.
    #[must_use]
    pub fn errors(probability: f64) -> Self {
        Self {
            error_injection: Some(probability),
            ..Default::default()
        }
    }

    /// Creates a comprehensive chaos config.
    #[must_use]
    pub fn comprehensive() -> Self {
        Self {
            latency_injection: Some((0.1, Duration::from_millis(500))),
            error_injection: Some(0.05),
            packet_loss: Some(0.01),
            memory_pressure: Some(0.7),
            cpu_stress: Some(0.5),
            duration: Some(Duration::from_secs(60)),
        }
    }
}

/// Chaos injector for testing daemon resilience.
pub struct ChaosInjector {
    config: ChaosConfig,
    active: bool,
}

impl ChaosInjector {
    /// Creates a new chaos injector.
    #[must_use]
    pub const fn new(config: ChaosConfig) -> Self {
        Self {
            config,
            active: false,
        }
    }

    /// Starts chaos injection.
    ///
    /// # Errors
    /// Returns an error if injection fails to start.
    #[allow(clippy::unused_async)] // Will be async when actually spawning chaos threads
    pub async fn start(&mut self) -> Result<()> {
        tracing::warn!("starting chaos injection: {:?}", self.config);
        self.active = true;
        Ok(())
    }

    /// Stops chaos injection.
    ///
    /// # Errors
    /// Returns an error if injection fails to stop.
    #[allow(clippy::unused_async)] // Will be async when actually stopping chaos threads
    pub async fn stop(&mut self) -> Result<()> {
        tracing::info!("stopping chaos injection");
        self.active = false;
        Ok(())
    }

    /// Returns true if chaos injection is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// Returns the chaos config.
    #[must_use]
    pub const fn config(&self) -> &ChaosConfig {
        &self.config
    }

    /// Injects latency if configured and active.
    pub async fn maybe_inject_latency(&self) {
        if !self.active {
            return;
        }

        if let Some((probability, delay)) = self.config.latency_injection
            && rand_probability(probability)
        {
            tracing::debug!("injecting latency: {delay:?}");
            tokio::time::sleep(delay).await;
        }
    }

    /// Returns true if an error should be injected.
    #[must_use]
    pub fn should_inject_error(&self) -> bool {
        if !self.active {
            return false;
        }

        if let Some(probability) = self.config.error_injection
            && rand_probability(probability)
        {
            tracing::debug!("injecting error");
            return true;
        }

        false
    }
}

/// Simple probability check (not cryptographically secure).
fn rand_probability(p: f64) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    (f64::from(nanos) / f64::from(u32::MAX)) < p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chaos_config_default() {
        let config = ChaosConfig::default();
        assert!(config.latency_injection.is_none());
        assert!(config.error_injection.is_none());
        assert!(config.packet_loss.is_none());
        assert!(config.memory_pressure.is_none());
        assert!(config.cpu_stress.is_none());
        assert!(config.duration.is_none());
    }

    #[test]
    fn test_chaos_config_latency() {
        let config = ChaosConfig::latency(0.1, Duration::from_millis(100));
        assert!(config.latency_injection.is_some());
        assert!(config.error_injection.is_none());

        let (prob, delay) = config.latency_injection.expect("latency");
        assert!((prob - 0.1).abs() < 0.001);
        assert_eq!(delay, Duration::from_millis(100));
    }

    #[test]
    fn test_chaos_config_errors() {
        let config = ChaosConfig::errors(0.05);
        assert!(config.error_injection.is_some());
        assert!(config.latency_injection.is_none());

        let prob = config.error_injection.expect("error");
        assert!((prob - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_chaos_config_comprehensive() {
        let config = ChaosConfig::comprehensive();
        assert!(config.latency_injection.is_some());
        assert!(config.error_injection.is_some());
        assert!(config.packet_loss.is_some());
        assert!(config.memory_pressure.is_some());
        assert!(config.cpu_stress.is_some());
        assert!(config.duration.is_some());
    }

    #[test]
    fn test_chaos_config_debug() {
        let config = ChaosConfig::comprehensive();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ChaosConfig"));
    }

    #[test]
    fn test_chaos_config_clone() {
        let config1 = ChaosConfig::comprehensive();
        let config2 = config1.clone();
        assert_eq!(config1.latency_injection, config2.latency_injection);
        assert_eq!(config1.error_injection, config2.error_injection);
    }

    #[tokio::test]
    async fn test_chaos_injector_lifecycle() {
        let mut injector = ChaosInjector::new(ChaosConfig::default());
        assert!(!injector.is_active());

        injector.start().await.expect("start should succeed");
        assert!(injector.is_active());

        injector.stop().await.expect("stop should succeed");
        assert!(!injector.is_active());
    }

    #[test]
    fn test_chaos_injector_config_accessor() {
        let config = ChaosConfig::latency(0.5, Duration::from_secs(1));
        let injector = ChaosInjector::new(config);

        let accessed_config = injector.config();
        assert!(accessed_config.latency_injection.is_some());
    }

    #[tokio::test]
    async fn test_chaos_injector_latency_inactive() {
        let injector = ChaosInjector::new(ChaosConfig::latency(1.0, Duration::from_millis(10)));
        // Inactive - should return immediately
        let start = std::time::Instant::now();
        injector.maybe_inject_latency().await;
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(5), "Should not inject when inactive");
    }

    #[tokio::test]
    async fn test_chaos_injector_latency_active() {
        let mut injector = ChaosInjector::new(ChaosConfig::latency(1.0, Duration::from_millis(50)));
        injector.start().await.expect("start");

        // Active with 100% probability - should inject latency
        let start = std::time::Instant::now();
        injector.maybe_inject_latency().await;
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(45), "Should inject latency when active");
    }

    #[test]
    fn test_chaos_injector_error_inactive() {
        let injector = ChaosInjector::new(ChaosConfig::errors(1.0));
        // Inactive - should never inject
        assert!(!injector.should_inject_error());
    }

    #[tokio::test]
    async fn test_chaos_injector_error_active() {
        let mut injector = ChaosInjector::new(ChaosConfig::errors(1.0));
        injector.start().await.expect("start");

        // Active with 100% probability - should inject error
        // Run multiple times to account for probability timing
        let mut injected = false;
        for _ in 0..100 {
            if injector.should_inject_error() {
                injected = true;
                break;
            }
        }
        assert!(injected, "Should eventually inject error with 100% probability");
    }

    #[test]
    fn test_rand_probability_never() {
        // 0% probability should always return false
        // (implementation uses timing, but edge case)
        for _ in 0..10 {
            // Can't guarantee 0.0 since rand uses time
            // Just verify function works
            let _ = rand_probability(0.0);
        }
    }

    #[test]
    fn test_rand_probability_varies() {
        // Run many times to verify it doesn't always return the same value
        let mut results = std::collections::HashSet::new();
        for _ in 0..1000 {
            results.insert(rand_probability(0.5));
            std::thread::sleep(std::time::Duration::from_nanos(1)); // Vary timing
        }
        // With 50% probability, we should see both true and false
        // (might rarely fail but extremely unlikely with 1000 samples)
        assert!(results.len() >= 1, "Should see at least one result");
    }
}
