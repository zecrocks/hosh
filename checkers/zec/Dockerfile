FROM rust:1.87-slim-bullseye AS builder

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
    # Only remove the dummy source and binary, keep the deps!
    rm -rf src target/release/checker-zec

# Now copy the real source code and rebuild
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
COPY --from=builder /usr/src/app/target/release/checker-zec /app/checker-zec

# Set the binary as executable
RUN chmod +x /app/checker-zec

CMD ["/app/checker-zec"]
