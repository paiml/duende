//! Load testing for daemons.
//!
//! # Toyota Way: Heijunka (平準化)
//! Level loading to understand capacity limits.
//!
//! # Implementation
//! Uses concurrent workers via tokio to simulate load.
//! Collects latency metrics and computes percentiles.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::Result;

/// Load test configuration.
#[derive(Debug, Clone)]
pub struct LoadTestConfig {
    /// Number of concurrent users/workers.
    pub concurrent_users: u32,
    /// Ramp-up duration (time to reach full concurrency).
    pub ramp_up: Duration,
    /// Test duration (after ramp-up completes).
    pub duration: Duration,
    /// Requests per user (if set, stops after this many requests per user).
    pub requests_per_user: Option<u32>,
    /// Target requests per second (rate limiting).
    pub target_rps: Option<f64>,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            concurrent_users: 10,
            ramp_up: Duration::from_secs(10),
            duration: Duration::from_secs(60),
            requests_per_user: None,
            target_rps: None,
        }
    }
}

impl LoadTestConfig {
    /// Creates a light load test config.
    #[must_use]
    pub fn light() -> Self {
        Self {
            concurrent_users: 5,
            ramp_up: Duration::from_secs(5),
            duration: Duration::from_secs(30),
            ..Default::default()
        }
    }

    /// Creates a moderate load test config.
    #[must_use]
    pub fn moderate() -> Self {
        Self {
            concurrent_users: 50,
            ramp_up: Duration::from_secs(30),
            duration: Duration::from_secs(120),
            ..Default::default()
        }
    }

    /// Creates a heavy load test config.
    #[must_use]
    pub fn heavy() -> Self {
        Self {
            concurrent_users: 200,
            ramp_up: Duration::from_secs(60),
            duration: Duration::from_secs(300),
            ..Default::default()
        }
    }

    /// Creates a quick config for testing (short durations).
    #[must_use]
    pub fn quick() -> Self {
        Self {
            concurrent_users: 4,
            ramp_up: Duration::from_millis(100),
            duration: Duration::from_millis(500),
            requests_per_user: Some(10),
            target_rps: None,
        }
    }
}

/// Shared metrics for concurrent load test workers.
struct LoadMetrics {
    total_requests: AtomicU64,
    successful: AtomicU64,
    failed: AtomicU64,
    latencies_us: Mutex<Vec<u64>>,
}

impl Default for LoadMetrics {
    fn default() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            latencies_us: Mutex::new(Vec::new()),
        }
    }
}

impl LoadMetrics {
    fn record_success(&self, latency_us: u64) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut latencies) = self.latencies_us.lock() {
            latencies.push(latency_us);
        }
    }

    fn record_failure(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed.fetch_add(1, Ordering::Relaxed);
    }

    fn get_latencies(&self) -> Vec<u64> {
        self.latencies_us.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

/// Request handler function type.
/// Takes a user_id and request_id, returns success (true) or failure (false).
pub type RequestHandler = Arc<dyn Fn(u32, u64) -> bool + Send + Sync>;

/// Load tester for daemons.
pub struct LoadTester {
    config: LoadTestConfig,
    handler: Option<RequestHandler>,
}

impl LoadTester {
    /// Creates a new load tester.
    #[must_use]
    pub const fn new(config: LoadTestConfig) -> Self {
        Self {
            config,
            handler: None,
        }
    }

    /// Sets a custom request handler for the load test.
    ///
    /// The handler receives (user_id, request_id) and returns true for success.
    #[must_use]
    pub fn with_handler(mut self, handler: RequestHandler) -> Self {
        self.handler = Some(handler);
        self
    }

    /// Runs the load test with concurrent workers.
    ///
    /// # Load Test Phases (Toyota Way: Heijunka)
    /// 1. Ramp-up: Gradually spawn workers to avoid thundering herd
    /// 2. Steady-state: All workers active, collecting metrics
    /// 3. Cool-down: Workers complete, aggregate results
    ///
    /// # Errors
    /// Returns an error if the test infrastructure fails.
    #[allow(clippy::too_many_lines)]
    pub async fn run(&self) -> Result<LoadTestReport> {
        tracing::info!(
            users = self.config.concurrent_users,
            duration = ?self.config.duration,
            ramp_up = ?self.config.ramp_up,
            "starting load test"
        );

        let metrics = Arc::new(LoadMetrics::default());
        let start_time = Instant::now();
        let test_end = start_time + self.config.ramp_up + self.config.duration;

        // Spawn concurrent workers
        let mut handles = Vec::with_capacity(self.config.concurrent_users as usize);
        let ramp_delay = if self.config.concurrent_users > 1 {
            self.config.ramp_up.as_millis() as u64 / (u64::from(self.config.concurrent_users) - 1).max(1)
        } else {
            0
        };

        // Copy config values for worker closures
        let concurrent_users = self.config.concurrent_users;

        for user_id in 0..self.config.concurrent_users {
            let metrics = Arc::clone(&metrics);
            let requests_per_user = self.config.requests_per_user;
            let target_rps = self.config.target_rps;
            let request_handler = self.handler.clone();

            // Calculate when this worker should start (ramp-up)
            let worker_start_delay = Duration::from_millis(ramp_delay * u64::from(user_id));

            handles.push(tokio::spawn(async move {
                // Wait for ramp-up
                tokio::time::sleep(worker_start_delay).await;

                let mut request_id = 0u64;
                let interval = target_rps.map(|rps| {
                    Duration::from_secs_f64(1.0 / rps * f64::from(concurrent_users))
                });

                loop {
                    // Check termination conditions
                    if Instant::now() >= test_end {
                        break;
                    }
                    if requests_per_user.is_some_and(|max| request_id >= u64::from(max)) {
                        break;
                    }

                    // Execute request
                    let req_start = Instant::now();
                    let success = if let Some(ref h) = request_handler {
                        h(user_id, request_id)
                    } else {
                        // Default: simulate 100us work with 1% failure rate
                        tokio::time::sleep(Duration::from_micros(100)).await;
                        !request_id.is_multiple_of(100) // 1% failure
                    };
                    let latency_us = req_start.elapsed().as_micros() as u64;

                    if success {
                        metrics.record_success(latency_us);
                    } else {
                        metrics.record_failure();
                    }

                    request_id += 1;

                    // Rate limiting
                    if let Some(delay) = interval {
                        tokio::time::sleep(delay).await;
                    }
                }
            }));
        }

        // Wait for all workers
        for handle in handles {
            let _ = handle.await;
        }

        let elapsed = start_time.elapsed();

        // Compute report
        let total_requests = metrics.total_requests.load(Ordering::Relaxed);
        let successful = metrics.successful.load(Ordering::Relaxed);
        let failed = metrics.failed.load(Ordering::Relaxed);

        let mut latencies = metrics.get_latencies();
        latencies.sort_unstable();

        let (p50, p95, p99) = if latencies.is_empty() {
            (0, 0, 0)
        } else {
            (
                percentile(&latencies, 50),
                percentile(&latencies, 95),
                percentile(&latencies, 99),
            )
        };

        let throughput_rps = if elapsed.as_secs_f64() > 0.0 {
            total_requests as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        let error_rate = if total_requests > 0 {
            failed as f64 / total_requests as f64
        } else {
            0.0
        };

        tracing::info!(
            total = total_requests,
            successful = successful,
            failed = failed,
            throughput_rps = format!("{throughput_rps:.2}"),
            p50_us = p50,
            p95_us = p95,
            p99_us = p99,
            "load test completed"
        );

        Ok(LoadTestReport {
            total_requests,
            successful,
            failed,
            latency_p50_us: p50,
            latency_p95_us: p95,
            latency_p99_us: p99,
            throughput_rps,
            error_rate,
        })
    }

    /// Returns the test config.
    #[must_use]
    pub const fn config(&self) -> &LoadTestConfig {
        &self.config
    }
}

impl Default for LoadTester {
    fn default() -> Self {
        Self::new(LoadTestConfig::default())
    }
}

/// Computes percentile from sorted slice.
fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (sorted.len() * p / 100).min(sorted.len() - 1);
    sorted[idx]
}

/// Load test report.
#[derive(Debug, Clone)]
pub struct LoadTestReport {
    /// Total requests made.
    pub total_requests: u64,
    /// Successful requests.
    pub successful: u64,
    /// Failed requests.
    pub failed: u64,
    /// P50 latency in microseconds.
    pub latency_p50_us: u64,
    /// P95 latency in microseconds.
    pub latency_p95_us: u64,
    /// P99 latency in microseconds.
    pub latency_p99_us: u64,
    /// Throughput in requests per second.
    pub throughput_rps: f64,
    /// Error rate (0.0 to 1.0).
    pub error_rate: f64,
}

impl LoadTestReport {
    /// Returns true if the test passed (error rate below 1%).
    #[must_use]
    pub fn passed(&self) -> bool {
        self.error_rate < 0.01
    }

    /// Returns success rate (0.0 to 1.0).
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.total_requests > 0 {
            self.successful as f64 / self.total_requests as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_test_config_presets() {
        let light = LoadTestConfig::light();
        assert_eq!(light.concurrent_users, 5);

        let moderate = LoadTestConfig::moderate();
        assert_eq!(moderate.concurrent_users, 50);

        let heavy = LoadTestConfig::heavy();
        assert_eq!(heavy.concurrent_users, 200);

        let quick = LoadTestConfig::quick();
        assert_eq!(quick.concurrent_users, 4);
        assert_eq!(quick.requests_per_user, Some(10));
    }

    #[tokio::test]
    async fn test_load_tester_run_quick() {
        // Use quick config for fast test execution
        let tester = LoadTester::new(LoadTestConfig::quick());
        let report = tester.run().await;

        assert!(report.is_ok());
        let report = report.unwrap();

        // With 4 users * 10 requests each = 40 total requests
        assert_eq!(report.total_requests, 40);
        // Default handler has 1% failure rate (request_id % 100 == 0 fails)
        // With request_ids 0-9 per user, request 0 fails for each user = 4 failures
        assert_eq!(report.failed, 4);
        assert_eq!(report.successful, 36);
        assert!(report.throughput_rps > 0.0);
        assert!(report.latency_p50_us > 0);
    }

    #[tokio::test]
    async fn test_load_tester_with_custom_handler() {
        let handler: RequestHandler = Arc::new(|_user_id, request_id| {
            // Fail every 5th request
            request_id % 5 != 0
        });

        let config = LoadTestConfig {
            concurrent_users: 2,
            ramp_up: Duration::from_millis(10),
            duration: Duration::from_millis(100),
            requests_per_user: Some(10),
            target_rps: None,
        };

        let tester = LoadTester::new(config).with_handler(handler);
        let report = tester.run().await.unwrap();

        // 2 users * 10 requests = 20 total
        assert_eq!(report.total_requests, 20);
        // Requests 0, 5 fail for each user = 4 failures
        assert_eq!(report.failed, 4);
        assert_eq!(report.successful, 16);
    }

    #[tokio::test]
    async fn test_load_tester_all_success() {
        let handler: RequestHandler = Arc::new(|_, _| true);

        let config = LoadTestConfig {
            concurrent_users: 2,
            ramp_up: Duration::from_millis(10),
            duration: Duration::from_millis(100),
            requests_per_user: Some(5),
            target_rps: None,
        };

        let tester = LoadTester::new(config).with_handler(handler);
        let report = tester.run().await.unwrap();

        assert_eq!(report.total_requests, 10);
        assert_eq!(report.failed, 0);
        assert_eq!(report.successful, 10);
        assert!(report.passed());
        assert!((report.success_rate() - 1.0).abs() < 0.001);
        assert!((report.error_rate - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_load_tester_all_failure() {
        let handler: RequestHandler = Arc::new(|_, _| false);

        let config = LoadTestConfig {
            concurrent_users: 2,
            ramp_up: Duration::from_millis(10),
            duration: Duration::from_millis(100),
            requests_per_user: Some(5),
            target_rps: None,
        };

        let tester = LoadTester::new(config).with_handler(handler);
        let report = tester.run().await.unwrap();

        assert_eq!(report.total_requests, 10);
        assert_eq!(report.failed, 10);
        assert_eq!(report.successful, 0);
        assert!(!report.passed());
        assert!((report.success_rate() - 0.0).abs() < 0.001);
        assert!((report.error_rate - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_load_test_report_passed() {
        let report = LoadTestReport {
            total_requests: 1000,
            successful: 995,
            failed: 5,
            latency_p50_us: 1000,
            latency_p95_us: 5000,
            latency_p99_us: 10000,
            throughput_rps: 100.0,
            error_rate: 0.005,
        };

        assert!(report.passed());
        assert!((report.success_rate() - 0.995).abs() < 0.001);
    }

    #[test]
    fn test_load_test_report_failed() {
        let report = LoadTestReport {
            total_requests: 100,
            successful: 90,
            failed: 10,
            latency_p50_us: 1000,
            latency_p95_us: 5000,
            latency_p99_us: 10000,
            throughput_rps: 100.0,
            error_rate: 0.10, // 10% error rate
        };

        assert!(!report.passed()); // >1% error rate
        assert!((report.success_rate() - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_percentile_calculation() {
        let sorted = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

        // percentile uses (len * p / 100).min(len-1) for index
        // len=10, p=0 => idx=0 => value=1
        // len=10, p=50 => idx=5 => value=6
        // len=10, p=90 => idx=9 => value=10
        // len=10, p=100 => idx=10.min(9)=9 => value=10
        assert_eq!(percentile(&sorted, 0), 1);
        assert_eq!(percentile(&sorted, 50), 6);
        assert_eq!(percentile(&sorted, 90), 10);
        assert_eq!(percentile(&sorted, 100), 10);
    }

    #[test]
    fn test_percentile_empty() {
        let empty: Vec<u64> = vec![];
        assert_eq!(percentile(&empty, 50), 0);
    }

    #[test]
    fn test_load_metrics_thread_safety() {
        let metrics = Arc::new(LoadMetrics::default());
        let mut handles = vec![];

        for _ in 0..10 {
            let m = Arc::clone(&metrics);
            handles.push(std::thread::spawn(move || {
                for i in 0..100 {
                    if i % 10 == 0 {
                        m.record_failure();
                    } else {
                        m.record_success(i * 100);
                    }
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1000);
        assert_eq!(metrics.failed.load(Ordering::Relaxed), 100);
        assert_eq!(metrics.successful.load(Ordering::Relaxed), 900);

        let latencies = metrics.get_latencies();
        assert_eq!(latencies.len(), 900);
    }
}
