# Iron Lotus Framework

The Iron Lotus Framework is PAIML's application of TPS principles to software.

## Core Tenets

1. **No Panics** - Explicit error handling everywhere
2. **No Unwrap** - All errors must be handled
3. **Traceable** - All operations traceable to syscalls
4. **Measured** - Continuous metrics collection

## Clippy Configuration

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
```

## Error Handling

All functions that can fail return `Result`:

```rust
pub fn do_something() -> Result<Output, Error> {
    // ...
}
```

## Syscall Tracing

Integration with `renacer` for syscall tracing:

```rust
let tracer = adapter.attach_tracer(&handle).await?;
// All daemon syscalls are now traced
```

## Stack Integration

Iron Lotus principles are applied across the PAIML Sovereign AI Stack:

- `trueno` - SIMD/GPU compute primitives
- `aprender` - ML algorithms
- `realizar` - Inference engine
- `duende` - Daemon framework
- `renacer` - Syscall tracing
