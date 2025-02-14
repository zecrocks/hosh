FROM rust:1.84-slim-bullseye as builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Copy manifests
COPY checkers/http/Cargo.toml checkers/http/Cargo.lock ./

# Build dummy main to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {println!(\"dummy\");}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/checker_http*

# Copy source code
COPY checkers/http/src ./src

# Build actual binary
RUN touch ./src/main.rs && cargo build --release

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/checker-http /usr/local/bin/

CMD ["checker-http"] 