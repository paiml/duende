# Container

The Container adapter manages daemons in Docker/OCI containers.

## Status

**Stub implementation** - Not yet fully implemented.

## Features (Planned)

- Docker container lifecycle
- OCI runtime support
- Resource limits via cgroups
- Health checks

## mlock in Containers

See [Memory Locking](../mlock.md) for container-specific configuration:

```bash
docker run --cap-add=IPC_LOCK --ulimit memlock=-1:-1 my-daemon
```
