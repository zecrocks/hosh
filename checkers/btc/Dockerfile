FROM rust:1.87-slim-bullseye AS chef
# We only pay the installation cost once, it will be cached from the second build onwards
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*
# Build dependencies - this is the caching Docker layer!
# Cache mounts speed up cargo registry and git dependencies
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp /app/target/release/btc-backend /app/btc-backend

# Runtime stage
FROM debian:bullseye-slim

WORKDIR /app

# Install runtime dependencies (including OpenSSL)
RUN apt-get update && apt-get install -y \
    wget \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/btc-backend /app/btc-backend

# Set the binary as executable
RUN chmod +x /app/btc-backend

# Add environment variable for run mode
ENV RUN_MODE=server

EXPOSE 5000

CMD ["/app/btc-backend"]

