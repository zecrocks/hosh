FROM rust:1.84-slim-bullseye as builder

WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Dependency caching
COPY Cargo.toml Cargo.lock ./

RUN mkdir -p src && \
    echo "fn main() {println!(\"dummy\");}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/checker-http*

# Copy real source code
COPY src ./src

# Break the Cargo cache, build
RUN touch ./src/main.rs && cargo build --release

# Runtime stage
FROM debian:bullseye-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /usr/src/app/target/release/checker-http /app/checker-http

# Set the binary as executable
RUN chmod +x /app/checker-http

CMD ["/app/checker-http"] 