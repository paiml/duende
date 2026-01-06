# macOS (launchd) Adapter

The `LaunchdAdapter` manages daemons via launchd plist files on macOS.

## Features

- Plist file generation in `~/Library/LaunchAgents/`
- Bootstrap/bootout via `launchctl`
- Signal forwarding via `launchctl kill`
- Status queries via `launchctl list`
- User and system domain support

## Usage

```rust
use duende_core::adapters::LaunchdAdapter;
use duende_core::types::Signal;

// User domain (default)
let adapter = LaunchdAdapter::new();

// System domain - requires root
let adapter = LaunchdAdapter::system();

// Spawn daemon
let handle = adapter.spawn(Box::new(my_daemon)).await?;
println!("Label: {}", handle.launchd_label().unwrap());

// Check status
let status = adapter.status(&handle).await?;

// Send signal
adapter.signal(&handle, Signal::Term).await?;
```

## How It Works

1. **Spawn**:
   - Writes plist to `~/Library/LaunchAgents/com.duende.<name>.plist`
   - Runs `launchctl bootstrap gui/<uid> <plist>`
2. **Signal**: Runs `launchctl kill <sig> gui/<uid>/com.duende.<name>`
3. **Status**: Runs `launchctl list com.duende.<name>`
4. **Stop**: Runs `launchctl bootout gui/<uid>/com.duende.<name>`

## Generated Plist

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.duende.my-daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/my-daemon</string>
    </array>
    <key>RunAtLoad</key>
    <false/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
```

## Verification

```bash
# List duende services
launchctl list | grep duende

# Check specific service
launchctl list com.duende.my-daemon

# View logs
log show --predicate 'subsystem == "com.duende.my-daemon"' --last 1h
```

## Platform Detection

The adapter is automatically selected on macOS:

```rust
use duende_core::platform::detect_platform;
use duende_core::adapters::select_adapter;

let platform = detect_platform();  // Returns Platform::MacOS
let adapter = select_adapter(platform);  // Returns LaunchdAdapter
```

## Requirements

- macOS 10.10+
- `launchctl` in PATH
- Write access to `~/Library/LaunchAgents/`
