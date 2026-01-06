//! Docker mlock test runner - Rust replacement for test-mlock.sh
//!
//! Runs mlock tests across different Docker privilege configurations.
//! DT-007: Swap Deadlock Prevention
//!
//! # Usage
//!
//! ```bash
//! cargo run -p duende-platform --bin mlock-docker-test
//! cargo run -p duende-platform --bin mlock-docker-test -- --build
//! ```

use std::process::{Command, ExitCode};

const IMAGE_NAME: &str = "duende-mlock-test";
const DOCKERFILE: &str = "docker/Dockerfile.mlock-test";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let force_build = args.iter().any(|a| a == "--build");

    println!("========================================");
    println!("duende mlock Docker Test Suite");
    println!("DT-007: Swap Deadlock Prevention");
    println!("========================================");
    println!();

    // Check Docker is available
    if !check_docker() {
        eprintln!("\x1b[31m[ERROR]\x1b[0m Docker is not installed or not in PATH");
        return ExitCode::FAILURE;
    }

    // Build image if needed
    if force_build || !image_exists() {
        if !build_image() {
            eprintln!("\x1b[31m[ERROR]\x1b[0m Failed to build Docker image");
            return ExitCode::FAILURE;
        }
    } else {
        println!("\x1b[32m[INFO]\x1b[0m Using existing image: {}", IMAGE_NAME);
    }

    println!();

    let mut passed = 0;
    let mut failed = 0;

    // Test 1: Default container (no capabilities)
    if run_test("Default container (no CAP_IPC_LOCK)", &[]) {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 2: With CAP_IPC_LOCK capability
    if run_test("With CAP_IPC_LOCK", &["--cap-add=IPC_LOCK"]) {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 3: With CAP_IPC_LOCK and unlimited memlock
    if run_test(
        "With CAP_IPC_LOCK + unlimited memlock",
        &["--cap-add=IPC_LOCK", "--ulimit", "memlock=-1:-1"],
    ) {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 4: Privileged container
    if run_test("Privileged container", &["--privileged"]) {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 5: With seccomp=unconfined
    if run_test(
        "With seccomp=unconfined",
        &["--cap-add=IPC_LOCK", "--security-opt", "seccomp=unconfined"],
    ) {
        passed += 1;
    } else {
        failed += 1;
    }

    // Summary
    println!();
    println!("========================================");
    println!("Test Summary");
    println!("========================================");
    println!("Passed: \x1b[32m{}\x1b[0m", passed);
    println!("Failed: \x1b[31m{}\x1b[0m", failed);
    println!();

    if failed > 0 {
        println!(
            "\x1b[33m[WARN]\x1b[0m Some tests failed - this may be expected depending on Docker configuration"
        );
        println!();
        println!("For production trueno-ublk containers, use:");
        println!("  docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 ...");
        println!();
        println!("Or in docker-compose.yml:");
        println!("  cap_add:");
        println!("    - IPC_LOCK");
        println!("  ulimits:");
        println!("    memlock:");
        println!("      soft: -1");
        println!("      hard: -1");
        return ExitCode::FAILURE;
    }

    println!("\x1b[32m[INFO]\x1b[0m All tests passed!");
    ExitCode::SUCCESS
}

fn check_docker() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn image_exists() -> bool {
    Command::new("docker")
        .args(["image", "inspect", IMAGE_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_image() -> bool {
    println!(
        "\x1b[32m[INFO]\x1b[0m Building Docker test image: {}",
        IMAGE_NAME
    );

    let status = Command::new("docker")
        .args(["build", "-f", DOCKERFILE, "-t", IMAGE_NAME, "."])
        .status();

    match status {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("Failed to run docker build: {}", e);
            false
        }
    }
}

fn run_test(name: &str, docker_args: &[&str]) -> bool {
    println!();
    println!("\x1b[33m=== {} ===\x1b[0m", name);
    println!(
        "Docker args: {}",
        if docker_args.is_empty() {
            "none".to_string()
        } else {
            docker_args.join(" ")
        }
    );
    println!();

    let mut cmd = Command::new("docker");
    cmd.arg("run").arg("--rm");

    for arg in docker_args {
        cmd.arg(arg);
    }

    cmd.arg(IMAGE_NAME);

    let status = cmd.status();

    match status {
        Ok(s) if s.success() => {
            println!();
            println!("\x1b[32mPASSED\x1b[0m: {}", name);
            true
        }
        Ok(s) => {
            println!();
            println!(
                "\x1b[31mFAILED\x1b[0m: {} (exit code: {:?})",
                name,
                s.code()
            );
            false
        }
        Err(e) => {
            println!();
            println!("\x1b[31mFAILED\x1b[0m: {} (error: {})", name, e);
            false
        }
    }
}
