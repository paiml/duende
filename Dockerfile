# Duende Daemon Example - Docker Image
#
# Build:
#   docker build -t duende-daemon .
#
# Run (with memory locking):
#   docker run --rm -it --cap-add=IPC_LOCK duende-daemon --mlock
#
# Run (without memory locking):
#   docker run --rm -it duende-daemon

FROM rust:slim-bookworm AS builder

WORKDIR /app

# Install build dependencies and nightly toolchain for edition 2024
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    && rm -rf /var/lib/apt/lists/* \
    && rustup toolchain install nightly \
    && rustup default nightly

# Copy workspace
COPY . .

# Build the daemon example in release mode
RUN cargo build --release --example daemon

# Runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Copy the built binary
COPY --from=builder /app/target/release/examples/daemon /app/daemon

# Run the daemon
ENTRYPOINT ["/app/daemon"]
CMD []
