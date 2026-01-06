# Container Adapter

The `ContainerAdapter` manages daemons in Docker, Podman, or containerd containers.

## Features

- Docker, Podman, and containerd runtime support
- Container lifecycle management
- Signal forwarding via `docker/podman kill`
- Status queries via container inspect
- Automatic runtime detection

## Usage

```rust
use duende_core::adapters::ContainerAdapter;
use duende_core::types::Signal;

// Auto-detect runtime (Docker > Podman > containerd)
let adapter = ContainerAdapter::new();

// Explicit runtime selection
let adapter = ContainerAdapter::docker();
let adapter = ContainerAdapter::podman();
let adapter = ContainerAdapter::containerd();

// With custom default image
let adapter = ContainerAdapter::with_image("my-daemon:latest");

// Spawn daemon in container
let handle = adapter.spawn(Box::new(my_daemon)).await?;
println!("Container: {}", handle.container_id().unwrap());

// Check status
let status = adapter.status(&handle).await?;

// Send signal
adapter.signal(&handle, Signal::Term).await?;
```

## How It Works

### Docker/Podman

1. **Spawn**: `docker run -d --name duende-<name> <image>`
2. **Signal**: `docker kill --signal=<sig> <container>`
3. **Status**: `docker inspect --format='{{.State.Status}}' <container>`
4. **Stop**: `docker stop <container> && docker rm <container>`

### containerd

1. **Spawn**: `ctr run -d <image> duende-<name>`
2. **Signal**: `ctr task kill --signal <sig> duende-<name>`
3. **Status**: `ctr task ls | grep duende-<name>`
4. **Stop**: `ctr task kill duende-<name> && ctr container rm duende-<name>`

## Verification

```bash
# Docker
docker ps | grep duende
docker logs duende-my-daemon

# Podman
podman ps | grep duende
podman logs duende-my-daemon

# containerd
ctr task ls | grep duende
ctr task logs duende-my-daemon
```

## mlock in Containers

For swap device daemons (DT-007), memory locking requires special container flags:

```bash
# Docker/Podman
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 my-daemon

# Or in docker-compose.yml
services:
  my-daemon:
    image: my-daemon:latest
    cap_add:
      - IPC_LOCK
    ulimits:
      memlock:
        soft: -1
        hard: -1
```

## Platform Detection

The adapter is selected when running inside a container:

```rust
use duende_core::platform::detect_platform;
use duende_core::adapters::select_adapter;

let platform = detect_platform();  // Returns Platform::Container
let adapter = select_adapter(platform);  // Returns ContainerAdapter
```

Container detection checks:
- `/.dockerenv` file exists
- `/run/.containerenv` file exists
- `container` environment variable set
- cgroup indicates container runtime

## Requirements

- Docker, Podman, or containerd installed and running
- CLI tools (`docker`, `podman`, or `ctr`) in PATH
- Appropriate permissions to manage containers
