# Troubleshooting

## mlock Issues

### EPERM: Operation not permitted

**Cause:** Missing CAP_IPC_LOCK capability.

**Solution:**
```bash
# Docker
docker run --cap-add=IPC_LOCK ...

# systemd
AmbientCapabilities=CAP_IPC_LOCK

# setcap
sudo setcap cap_ipc_lock+ep /usr/bin/my-daemon
```

### ENOMEM: Cannot allocate memory

**Cause:** Memlock ulimit exceeded.

**Solution:**
```bash
# Docker
docker run --ulimit memlock=-1:-1 ...

# systemd
LimitMEMLOCK=infinity

# shell
ulimit -l unlimited
```

## Container Issues

### Daemon deadlocks under memory pressure

**Cause:** Memory not locked, daemon pages being swapped.

**Solution:**
```bash
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 ...
```

See [Memory Locking](./mlock.md) for full details.

## General Issues

### Daemon fails to start

1. Check logs: `journalctl -u my-daemon`
2. Verify configuration: `my-daemon --check-config`
3. Check permissions on binary and config files
4. Verify resource limits are reasonable

### High memory usage

1. Check for memory leaks with `heaptrack` or `valgrind`
2. Review `VmRSS` in `/proc/<pid>/status`
3. Ensure mlock is not locking more than needed
