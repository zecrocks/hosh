FROM rust:1.75-slim-bullseye as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy source and build dependencies
RUN mkdir -p src && \
    echo "fn main() {println!(\"dummy\");}" > src/main.rs && \
    # cargo generate-lockfile && \
    cargo build --release && \
    rm -rf src target/release/checker-http

# Copy actual source
COPY src ./src

# Build for release
RUN touch ./src/main.rs && cargo build --release

FROM debian:bullseye-slim

WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the app directory from builder
COPY --from=builder /usr/src/app /usr/src/app

COPY --from=builder /usr/src/app/target/release/checker-http /usr/local/bin/checker-http

CMD ["checker-http"] 