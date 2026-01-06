# Container Memory Locking (mlock) Guide

## Overview

When running swap-device daemons like `trueno-ublk` in containers, memory locking
(`mlock`) is **CRITICAL** to prevent swap deadlock (DT-007).

## The Problem

Without memory locking, containers running as swap devices can deadlock:

1. Host kernel needs to swap pages OUT to the containerized swap device
2. Container daemon needs memory to process the I/O request
3. Kernel tries to swap out the container's pages to free memory
4. Swap goes to the same container → waiting for itself → **DEADLOCK**

Evidence from production (2026-01-06):
```
INFO: task trueno-ublk:59497 blocked for more than 122 seconds.
task:trueno-ublk state:D (uninterruptible sleep)
__swap_writepage+0x111/0x1a0
swap_writepage+0x5f/0xe0
```

## Solution

Use `mlockall(MCL_CURRENT | MCL_FUTURE)` to pin all daemon memory.

### Docker Run

```bash
# Minimum required
docker run --cap-add=IPC_LOCK your-image

# Recommended (unlimited memlock)
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 your-image

# Alternative: privileged mode (not recommended for production)
docker run --privileged your-image
```

### Docker Compose

```yaml
version: '3.8'
services:
  trueno-ublk:
    image: your-image
    cap_add:
      - IPC_LOCK
    ulimits:
      memlock:
        soft: -1
        hard: -1
    # If needed on some systems:
    # security_opt:
    #   - seccomp:unconfined
```

### Kubernetes

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: trueno-ublk
spec:
  containers:
  - name: trueno-ublk
    image: your-image
    securityContext:
      capabilities:
        add:
        - IPC_LOCK
    resources:
      limits:
        # Request unlimited memlock
        # Note: Kubernetes doesn't have direct memlock ulimit support
        # You may need a privileged container or custom seccomp profile
```

### Podman

```bash
# Same as Docker
podman run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 your-image
```

## Configuration

In your daemon's `DaemonConfig`:

```toml
[resources]
# Enable memory locking
lock_memory = true

# Make mlock failure fatal (recommended for swap devices)
lock_memory_required = true
```

Or in Rust:

```rust
use duende_core::ResourceConfig;
use duende_platform::apply_memory_config;

let mut config = ResourceConfig::default();
config.lock_memory = true;
config.lock_memory_required = true;

// Apply during daemon init
apply_memory_config(&config)?;
```

## Verifying mlock

### From Inside Container

```bash
# Check if memory is locked
grep VmLck /proc/self/status
# VmLck:     12345 kB  (non-zero = locked)

# Check capabilities
cat /proc/self/status | grep Cap
# Look for CAP_IPC_LOCK (bit 14) in CapEff
```

### From Host

```bash
# Check container capabilities
docker inspect --format='{{.HostConfig.CapAdd}}' <container>

# Check ulimits
docker inspect --format='{{.HostConfig.Ulimits}}' <container>
```

## Troubleshooting

### mlock fails with EPERM

**Cause**: Container lacks `CAP_IPC_LOCK` capability.

**Solution**: Add `--cap-add=IPC_LOCK` to docker run.

### mlock fails with ENOMEM

**Cause**: Memlock ulimit is too low.

**Solution**: Add `--ulimit memlock=-1:-1` for unlimited, or set a specific limit:
```bash
docker run --ulimit memlock=1073741824:1073741824 ...  # 1GB
```

### mlock succeeds but daemon still deadlocks

**Possible causes**:
1. Memory limit (`--memory`) is too restrictive
2. Swap limit (`--memory-swap`) is set
3. cgroup memory controller is limiting the container

**Solution**: Ensure adequate memory limits:
```bash
docker run --memory=2g --memory-swap=2g ...  # Disable swap for container
```

## Testing

Run the Docker mlock test suite:

```bash
cd /path/to/duende
./docker/test-mlock.sh
```

This tests mlock behavior across different privilege configurations.

## Security Considerations

- `CAP_IPC_LOCK` allows locking memory, which can impact host performance if abused
- Prefer specific capability grants over `--privileged`
- Set reasonable memlock limits in production (e.g., 2x expected daemon memory)
- Monitor container memory usage to detect leaks

## References

- [Linux mlockall(2) man page](https://man7.org/linux/man-pages/man2/mlock.2.html)
- [Docker capabilities documentation](https://docs.docker.com/engine/reference/run/#runtime-privilege-and-linux-capabilities)
- [Kubernetes security context](https://kubernetes.io/docs/tasks/configure-pod-container/security-context/)
