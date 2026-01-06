# Resource Limits

Duende provides resource limiting through `ResourceConfig`.

## Memory Limits

```toml
[resources]
memory_bytes = 536870912      # 512MB hard limit
memory_swap_bytes = 1073741824 # 1GB memory+swap
```

## CPU Limits

```toml
[resources]
cpu_quota_percent = 200.0  # 2 cores (200%)
cpu_shares = 1024          # Relative weight
```

## I/O Limits

```toml
[resources]
io_read_bps = 104857600   # 100MB/s read
io_write_bps = 52428800   # 50MB/s write
```

## Process Limits

```toml
[resources]
pids_max = 100        # Max child processes
open_files_max = 1024 # Max file descriptors
```

## Memory Locking

See [Memory Locking (mlock)](./mlock.md) for details.

```toml
[resources]
lock_memory = true
lock_memory_required = true
```
