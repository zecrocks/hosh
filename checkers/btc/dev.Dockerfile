FROM rust:1.85-slim-bullseye AS builder

WORKDIR /usr/src/btc

# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-watch for hot reloading
RUN cargo install cargo-watch

# Set environment variable for run mode
ENV RUN_MODE=worker
ENV RUST_INCREMENTAL=1

# Use cargo-watch with specific options:
# -q: Quiet mode (less output)
# -c: Clear screen between runs
# -w: Watch only specific directories
# -x: Execute command
CMD ["cargo", "watch", "-q", "-c", "-w", "src", "-x", "run"] 