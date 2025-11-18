FROM rust:1.85-slim-bullseye

# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-watch for hot reloading (with --locked to use exact version)
RUN cargo install cargo-watch --locked --version 8.4.0

# Set environment variable for incremental compilation
ENV RUST_INCREMENTAL=1

# Default command that can be overridden by child images
CMD ["cargo", "watch", "-q", "-c", "-w", "src", "-x", "run"] 