//! Memory Locking Example
//!
//! Demonstrates DT-007: Swap Deadlock Prevention using duende's mlock support.
//!
//! # Usage
//!
//! ```bash
//! # Run without privileges (may fail or succeed depending on ulimits)
//! cargo run --example mlock
//!
//! # Run with mlock required (will fail without CAP_IPC_LOCK)
//! cargo run --example mlock -- --required
//!
//! # Check current memory lock status
//! cargo run --example mlock -- --status
//! ```
//!
//! # Docker Usage
//!
//! ```bash
//! # Build and run in Docker with CAP_IPC_LOCK
//! docker run --cap-add=IPC_LOCK -v $(pwd):/app -w /app rust:1.83 \
//!     cargo run --example mlock
//! ```

use duende_core::ResourceConfig;
use duende_platform::{MlockResult, apply_memory_config, is_memory_locked, lock_daemon_memory};

fn main() {
    // Initialize tracing for log output
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    if args.iter().any(|a| a == "--status") {
        print_status();
        return;
    }

    let required = args.iter().any(|a| a == "--required");

    println!("=== duende mlock Example ===");
    println!("DT-007: Swap Deadlock Prevention\n");

    // Method 1: Direct API call
    println!("Method 1: Direct lock_daemon_memory() call");
    println!("  required = {}", required);

    match lock_daemon_memory(required) {
        Ok(MlockResult::Success) => {
            println!("  Result: SUCCESS - All memory locked");
            println!("  VmLck: {} KB", get_vmlck_kb());
        }
        Ok(MlockResult::Disabled) => {
            println!("  Result: DISABLED - Platform doesn't support mlock");
        }
        Ok(MlockResult::Failed(errno)) => {
            println!("  Result: FAILED (errno={}) - Continuing anyway", errno);
            println!("  Hint: Run with CAP_IPC_LOCK or as root");
        }
        Err(e) => {
            println!("  Result: ERROR - {}", e);
            println!("  This is fatal because required=true");
            std::process::exit(1);
        }
    }

    // Unlock for next test
    let _ = duende_platform::memory::unlock_daemon_memory();
    println!();

    // Method 2: Using ResourceConfig
    println!("Method 2: Using apply_memory_config()");

    let mut config = ResourceConfig::default();
    config.lock_memory = true;
    config.lock_memory_required = required;

    println!("  lock_memory = {}", config.lock_memory);
    println!("  lock_memory_required = {}", config.lock_memory_required);

    match apply_memory_config(&config) {
        Ok(()) => {
            println!("  Result: SUCCESS");
            println!("  VmLck: {} KB", get_vmlck_kb());
        }
        Err(e) => {
            println!("  Result: ERROR - {}", e);
            if required {
                std::process::exit(1);
            }
        }
    }

    println!();
    println!("=== Example Complete ===");
    println!();
    println!("For production use in containers:");
    println!("  docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 ...");
}

fn print_help() {
    println!(
        r#"duende mlock Example - DT-007: Swap Deadlock Prevention

USAGE:
    cargo run --example mlock [OPTIONS]

OPTIONS:
    --required    Make mlock failure fatal (exit with error)
    --status      Print current memory lock status and exit
    --help, -h    Print this help message

DESCRIPTION:
    This example demonstrates duende's memory locking capability, which is
    CRITICAL for daemons that serve as swap devices (e.g., trueno-ublk).

    Without memory locking, a swap-device daemon can deadlock:
    1. Kernel needs to swap pages OUT to the daemon's device
    2. Daemon needs memory to process I/O request
    3. Kernel tries to swap out daemon's pages to free memory
    4. Swap goes to the same daemon → DEADLOCK

EXAMPLES:
    # Basic test (may succeed or fail depending on privileges)
    cargo run --example mlock

    # Require mlock to succeed (for swap device daemons)
    cargo run --example mlock -- --required

    # Check current status
    cargo run --example mlock -- --status

    # Run in Docker with proper capabilities
    docker run --rm --cap-add=IPC_LOCK -v $(pwd):/app -w /app rust:1.83 \
        cargo run --example mlock -- --required
"#
    );
}

fn print_status() {
    println!("=== Memory Lock Status ===\n");

    // Check if memory is locked
    let locked = is_memory_locked();
    println!("Memory Locked: {}", if locked { "YES" } else { "NO" });
    println!("VmLck: {} KB", get_vmlck_kb());

    // Print capabilities
    println!("\nCapabilities:");
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("Cap") {
                println!("  {}", line);
            }
        }
    }

    // Check CAP_IPC_LOCK specifically
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("CapEff:") {
                if let Some(hex) = line.split_whitespace().nth(1) {
                    if let Ok(caps) = u64::from_str_radix(hex, 16) {
                        let has_ipc_lock = (caps & (1 << 14)) != 0;
                        println!(
                            "\nCAP_IPC_LOCK: {}",
                            if has_ipc_lock {
                                "GRANTED"
                            } else {
                                "NOT GRANTED"
                            }
                        );
                    }
                }
            }
        }
    }

    // Print memlock limits
    println!("\nMemory Lock Limits:");
    if let Ok(limits) = std::fs::read_to_string("/proc/self/limits") {
        for line in limits.lines() {
            if line.contains("locked") {
                println!("  {}", line);
            }
        }
    }
}

fn get_vmlck_kb() -> u64 {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmLck:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return kb;
                    }
                }
            }
        }
    }
    0
}
