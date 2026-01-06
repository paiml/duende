# duende-policy

Policy enforcement for the Duende daemon framework.

[![Crates.io](https://img.shields.io/crates/v/duende-policy.svg)](https://crates.io/crates/duende-policy)
[![Documentation](https://docs.rs/duende-policy/badge.svg)](https://docs.rs/duende-policy)
[![License](https://img.shields.io/crates/l/duende-policy.svg)](LICENSE)

## Overview

This crate provides policy enforcement mechanisms:

- **Quality gates**: PMAT-based code quality enforcement
- **Circuit breakers**: 3-state failure protection (Closed, Open, Half-Open)
- **Resource limiters**: cgroups/setrlimit enforcement
- **Jidoka automation**: Stop-on-error with recommendations

## Usage

### Circuit Breaker

```rust
use duende_policy::{CircuitBreaker, CircuitState};
use std::time::Duration;

let mut breaker = CircuitBreaker::new(5, Duration::from_secs(30));

if breaker.allow() {
    match do_work().await {
        Ok(_) => breaker.record_success(),
        Err(_) => breaker.record_failure(),
    }
}
```

### Jidoka Quality Gate

```rust
use duende_policy::{JidokaGate, JidokaCheck};

let gate = JidokaGate::new(vec![
    JidokaCheck::coverage(95.0),
    JidokaCheck::mutation_score(80.0),
    JidokaCheck::zero_satd(),
]);

match gate.check(&daemon) {
    JidokaResult::Pass => println!("Quality gate passed"),
    JidokaResult::Stop { violations, .. } => {
        eprintln!("Quality gate failed: {:?}", violations);
    }
}
```

## Iron Lotus Framework

- **Jidoka** (自働化): Automatic stop on quality violations
- **Poka-Yoke** (ポカヨケ): Mistake-proofing via policy enforcement
- **Standardized Work** (標準作業): Consistent policy application

## License

MIT OR Apache-2.0
