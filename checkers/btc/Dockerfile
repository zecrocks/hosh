FROM rust:1.85-slim-bullseye AS builder


WORKDIR /usr/src/btc

# Install required dependencies for OpenSSL
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# First, copy only files needed for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs for dependency build
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/btc_backend*

# Now copy the real source code
COPY . .

# Build the application
RUN cargo build --release

# Create the runtime image
FROM debian:bullseye-slim

# Install necessary runtime dependencies
RUN apt-get update && \
    apt-get install -y \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/btc/target/release/btc-backend /usr/local/bin/btc-backend

# Set the startup command
CMD ["btc-backend"]

