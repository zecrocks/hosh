FROM rustlang/rust:nightly-slim AS builder

WORKDIR /usr/src/app

# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

# Dependency caching
COPY Cargo.toml Cargo.lock ./

RUN mkdir -p src && \
    echo "fn main() {println!(\"dummy\");}" > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/btc-backend*

# Copy real source code
COPY src ./src

# Break the Cargo cache, build
RUN touch ./src/main.rs && cargo build --release

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
COPY --from=builder /usr/src/app/target/release/btc-backend /app/btc-backend

# Set the binary as executable
RUN chmod +x /app/btc-backend

# Add environment variable for run mode
ENV RUN_MODE=server

EXPOSE 5000

CMD ["/app/btc-backend"]

