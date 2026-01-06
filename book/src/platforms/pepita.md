# pepita (MicroVM) Adapter

The `PepitaAdapter` manages daemons in lightweight microVMs via the pepita VMM.

## Features

- MicroVM lifecycle management via `pepita` CLI
- Vsock communication for host-guest IPC
- KVM-based virtualization
- Memory isolation per daemon
- Fast startup times

## Usage

```rust
use duende_core::adapters::PepitaAdapter;
use duende_core::types::Signal;

// Default adapter
let adapter = PepitaAdapter::new();

// With custom vsock port
let adapter = PepitaAdapter::with_vsock_port(9000);

// With kernel and rootfs images
let adapter = PepitaAdapter::with_images(
    "/boot/vmlinuz",
    "/var/lib/pepita/rootfs.img"
);

// Spawn daemon in microVM
let handle = adapter.spawn(Box::new(my_daemon)).await?;
println!("VM ID: {}", handle.pepita_vm_id().unwrap());
println!("Vsock CID: {}", handle.vsock_cid().unwrap());

// Check status
let status = adapter.status(&handle).await?;

// Send signal
adapter.signal(&handle, Signal::Term).await?;

// Destroy VM
adapter.destroy(handle.pepita_vm_id().unwrap()).await?;
```

## How It Works

1. **Spawn**:
   - Allocates vsock CID
   - Runs `pepita run --kernel <path> --rootfs <path> --vsock-cid <cid> --name duende-vm-<name>`
2. **Signal**: Runs `pepita signal --name <vm_id> --signal <sig>`
3. **Status**: Runs `pepita status --name <vm_id> --json`
4. **Destroy**: Runs `pepita destroy --name <vm_id> --force`

## Architecture

```
Host                          MicroVM
┌─────────────────┐          ┌─────────────────┐
│  PepitaAdapter  │          │  pepita guest   │
│  ┌───────────┐  │  vsock   │  ┌───────────┐  │
│  │ VmManager ├──┼──────────┼──┤ DaemonCtl │  │
│  └───────────┘  │          │  └───────────┘  │
└─────────────────┘          └─────────────────┘
```

## Verification

```bash
# List duende VMs
pepita list | grep duende-vm

# Check specific VM
pepita status --name duende-vm-my-daemon

# View VM logs
pepita logs --name duende-vm-my-daemon
```

## Requirements

- Linux with KVM support (`/dev/kvm`)
- pepita VMM installed
- Kernel and rootfs images configured
- `pepita` CLI in PATH

## Platform Detection

The adapter is selected when:
- Running inside a pepita microVM
- `PEPITA_VSOCK_CID` environment variable set

```rust
use duende_core::platform::detect_platform;
use duende_core::adapters::select_adapter;

let platform = detect_platform();  // Returns Platform::PepitaMicroVM
let adapter = select_adapter(platform);  // Returns PepitaAdapter
```

## Configuration

```toml
[platform.pepita]
vcpus = 2
memory_mb = 256
kernel_path = "/boot/vmlinuz"
rootfs_path = "/var/lib/pepita/rootfs.ext4"
vsock_base_port = 5000
```
