# WOS (WebAssembly OS) Adapter

The `WosAdapter` manages daemons as WebAssembly processes in WOS (WebAssembly Operating System).

## Features

- Process lifecycle management via `wos-ctl` CLI
- 8-level priority scheduler (0-7)
- WebAssembly module isolation
- Capability-based security
- Message-passing IPC

## Priority Levels

| Level | Name | Use Case |
|-------|------|----------|
| 0 | Critical | Kernel tasks, watchdogs |
| 1 | High | System services |
| 2 | Above Normal | Important daemons |
| 3 | Normal+ | User services with boost |
| 4 | Normal | Default for daemons |
| 5 | Below Normal | Background tasks |
| 6 | Low | Batch processing |
| 7 | Idle | Only when system idle |

## Usage

```rust
use duende_core::adapters::WosAdapter;
use duende_core::types::Signal;

// Default adapter (priority 4 - Normal)
let adapter = WosAdapter::new();

// With custom priority
let adapter = WosAdapter::with_priority(2);  // Above Normal

// Spawn daemon as WOS process
let handle = adapter.spawn(Box::new(my_daemon)).await?;
println!("WOS PID: {}", handle.wos_pid().unwrap());

// Check status
let status = adapter.status(&handle).await?;

// Send signal
adapter.signal(&handle, Signal::Term).await?;
```

## How It Works

1. **Spawn**:
   - Allocates PID (starts at 2, PID 1 is init)
   - Runs `wos-ctl spawn --name <name> --priority <level> --wasm <path>`
2. **Signal**: Runs `wos-ctl kill --pid <pid> --signal <sig>`
3. **Status**: Runs `wos-ctl status --pid <pid> --json`
4. **Terminate**: Runs `wos-ctl terminate --pid <pid>`

## Architecture

```
WOS Kernel
┌───────────────────────────────────────────┐
│  ┌─────────────┐    ┌─────────────────┐   │
│  │  Scheduler  │    │  Process Table  │   │
│  │  (8-level)  │    │                 │   │
│  └─────────────┘    └─────────────────┘   │
│         │                    │            │
│         ▼                    ▼            │
│  ┌─────────────────────────────────────┐  │
│  │         WASM Runtime                │  │
│  │  ┌───────┐ ┌───────┐ ┌───────┐     │  │
│  │  │ PID 1 │ │ PID 2 │ │ PID 3 │ ... │  │
│  │  │ init  │ │daemon1│ │daemon2│     │  │
│  │  └───────┘ └───────┘ └───────┘     │  │
│  └─────────────────────────────────────┘  │
└───────────────────────────────────────────┘
```

## Verification

```bash
# List duende processes
wos-ctl ps | grep duende

# Check specific process
wos-ctl status --pid 42

# View process logs
wos-ctl logs --pid 42
```

## Requirements

- WOS runtime installed
- `wos-ctl` CLI in PATH
- WebAssembly module compiled for WASI

## Platform Detection

The adapter is selected when:
- Running inside WOS environment
- `WOS_VERSION` environment variable set

```rust
use duende_core::platform::detect_platform;
use duende_core::adapters::select_adapter;

let platform = detect_platform();  // Returns Platform::Wos
let adapter = select_adapter(platform);  // Returns WosAdapter
```

## Configuration

```toml
[platform.wos]
priority = 4  # 0-7, default is 4 (Normal)
capabilities = ["net", "fs:read"]
```
