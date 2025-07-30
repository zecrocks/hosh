FROM rust:1.85-slim-bullseye

# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /usr/src/nostr-alert

# Copy Cargo files for dependency resolution
COPY Cargo.toml Cargo.lock* ./

# Copy source code
COPY src ./src

# Build the executable
RUN cargo build --release

# Copy the binary to a standard location
RUN cp target/release/nostr-alert /usr/local/bin/nostr-alert

# Run the pre-built executable
CMD ["nostr-alert"] 