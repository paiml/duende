# pepita (MicroVM)

The pepita adapter manages daemons in lightweight microVMs.

## Status

**Stub implementation** - Not yet fully implemented.

## Features (Planned)

- virtio-vsock communication
- Minimal kernel boot
- Memory isolation
- Fast startup (<100ms)

## Configuration

```toml
[platform]
vcpus = 2
kernel_path = "/boot/vmlinuz"
rootfs_path = "/var/lib/pepita/rootfs.ext4"
```
