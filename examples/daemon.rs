// Examples are allowed to use expect/unwrap for simplicity
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::unnecessary_debug_formatting
)]

//! Duende Daemon Example
//!
//! Demonstrates the full daemon lifecycle using the Duende framework.
//!
//! # Usage
//!
//! ```bash
//! # Run the daemon locally
//! cargo run --example daemon
//!
//! # Run with memory locking (requires CAP_IPC_LOCK)
//! cargo run --example daemon -- --mlock
//!
//! # Run in foreground mode (don't daemonize)
//! cargo run --example daemon -- --foreground
//! ```
//!
//! # Docker Usage
//!
//! ```bash
//! docker run --rm -it --cap-add=IPC_LOCK \
//!     -v $(pwd):/app -w /app rust:1.83 \
//!     cargo run --example daemon -- --mlock
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use duende_core::{
    Daemon, DaemonConfig, DaemonContext, DaemonId, DaemonMetrics, ExitReason, HealthStatus,
};
use duende_platform::{MlockResult, is_memory_locked, lock_daemon_memory};
use tokio::signal;

/// Example counter daemon that increments a counter every second.
struct CounterDaemon {
    id: DaemonId,
    name: String,
    metrics: DaemonMetrics,
    counter: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    start_time: Option<Instant>,
}

impl CounterDaemon {
    fn new(name: &str) -> Self {
        Self {
            id: DaemonId::new(),
            name: name.to_string(),
            metrics: DaemonMetrics::new(),
            counter: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            start_time: None,
        }
    }
}

#[async_trait]
impl Daemon for CounterDaemon {
    fn id(&self) -> DaemonId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn init(&mut self, config: &DaemonConfig) -> duende_core::error::Result<()> {
        println!("[INIT] Daemon '{}' initializing...", config.name);
        println!("[INIT] Binary: {:?}", config.binary_path);
        println!("[INIT] Resources: {:?}", config.resources);

        self.start_time = Some(Instant::now());
        self.running.store(true, Ordering::SeqCst);

        println!("[INIT] Initialization complete");
        Ok(())
    }

    async fn run(&mut self, ctx: &mut DaemonContext) -> duende_core::error::Result<ExitReason> {
        println!("[RUN] Daemon starting main loop...");
        println!("[RUN] Press Ctrl+C to stop");

        while !ctx.should_shutdown() {
            // Increment counter
            let count = self.counter.fetch_add(1, Ordering::Relaxed) + 1;

            // Record metrics
            self.metrics.record_request();

            // Calculate uptime
            let uptime = self.start_time.map(|t| t.elapsed()).unwrap_or_default();

            // Print status every iteration
            println!(
                "[RUN] Count: {} | Uptime: {:.1}s | Rate: {:.2}/s | Memory locked: {}",
                count,
                uptime.as_secs_f64(),
                self.metrics.requests_per_second(),
                if is_memory_locked() { "YES" } else { "NO" }
            );

            // Check for signals
            if let Some(sig) = ctx.try_recv_signal() {
                println!("[RUN] Received signal: {:?}", sig);
                break;
            }

            // Sleep for 1 second
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        println!("[RUN] Main loop exiting");
        Ok(ExitReason::Graceful)
    }

    async fn shutdown(&mut self, timeout: Duration) -> duende_core::error::Result<()> {
        println!(
            "[SHUTDOWN] Graceful shutdown starting (timeout: {:?})",
            timeout
        );

        self.running.store(false, Ordering::SeqCst);

        // Simulate cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;

        let final_count = self.counter.load(Ordering::Relaxed);
        let uptime = self.start_time.map(|t| t.elapsed()).unwrap_or_default();

        println!("[SHUTDOWN] Final count: {}", final_count);
        println!("[SHUTDOWN] Total uptime: {:.1}s", uptime.as_secs_f64());
        println!("[SHUTDOWN] Shutdown complete");

        Ok(())
    }

    async fn health_check(&self) -> HealthStatus {
        if self.running.load(Ordering::Relaxed) {
            HealthStatus::healthy(1)
        } else {
            HealthStatus::unhealthy("Daemon not running", 0)
        }
    }

    fn metrics(&self) -> &DaemonMetrics {
        &self.metrics
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse arguments
    let args: Vec<String> = std::env::args().collect();
    let use_mlock = args.iter().any(|a| a == "--mlock");
    let _foreground = args.iter().any(|a| a == "--foreground" || a == "-f");

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Duende Daemon Example");
        println!();
        println!("Usage: daemon [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --mlock       Lock memory to prevent swap (requires CAP_IPC_LOCK)");
        println!("  --foreground  Run in foreground mode");
        println!("  --help        Show this help");
        return Ok(());
    }

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║              DUENDE DAEMON EXAMPLE                         ║");
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Framework: Duende (Cross-Platform Daemon Tooling)         ║");
    println!("║  Iron Lotus: Toyota Production System for Software         ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Memory locking (DT-007: Swap Deadlock Prevention)
    if use_mlock {
        println!("[MLOCK] Attempting to lock memory...");
        match lock_daemon_memory(false) {
            Ok(MlockResult::Success) => {
                println!("[MLOCK] Memory locked successfully");
                println!("[MLOCK] Swap deadlock prevention: ACTIVE");
            }
            Ok(MlockResult::Failed(errno)) => {
                println!("[MLOCK] Memory lock failed (errno={})", errno);
                println!("[MLOCK] Hint: Run with CAP_IPC_LOCK capability");
                println!("[MLOCK]   docker run --cap-add=IPC_LOCK ...");
            }
            Ok(MlockResult::Disabled) => {
                println!("[MLOCK] Memory locking not supported on this platform");
            }
            Err(e) => {
                println!("[MLOCK] Error: {}", e);
            }
        }
        println!();
    }

    // Create daemon
    let mut daemon = CounterDaemon::new("counter-daemon");

    // Create config
    let config = DaemonConfig::new("counter-daemon", "/usr/bin/counter-daemon");

    // Create context (returns context and handle for signaling)
    let (mut ctx, handle) = DaemonContext::new(config.clone());

    // Setup Ctrl+C handler
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        println!();
        println!("[SIGNAL] Ctrl+C received, initiating shutdown...");
        let _ = handle_clone.shutdown().await;
    });

    // Initialize daemon
    daemon.init(&config).await?;

    // Health check
    let health = daemon.health_check().await;
    println!(
        "[HEALTH] Status: {}",
        if health.is_healthy() {
            "HEALTHY"
        } else {
            "UNHEALTHY"
        }
    );
    println!();

    // Run daemon
    let exit_reason = daemon.run(&mut ctx).await?;
    println!();
    println!("[EXIT] Reason: {:?}", exit_reason);

    // Shutdown
    daemon.shutdown(Duration::from_secs(5)).await?;

    println!();
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║                    DAEMON STOPPED                          ║");
    println!("╚════════════════════════════════════════════════════════════╝");

    Ok(())
}
