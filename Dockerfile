FROM rust:1.92-slim-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Install all build dependencies for the unified binary
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    make \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
# Build dependencies - this is the caching Docker layer!
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

# Build the unified binary
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release -p hosh && \
    cp /app/target/release/hosh /app/hosh

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Copy the unified binary
COPY --from=builder /app/hosh /app/hosh

# Copy web assets (templates and static files)
COPY --from=builder /app/crates/hosh-web/templates /app/templates
COPY --from=builder /app/crates/hosh-web/static /app/static

RUN chmod +x /app/hosh

# Default: run all roles
ENTRYPOINT ["/app/hosh"]
CMD ["--roles", "all"]
