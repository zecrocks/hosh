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
    rm -rf src target/release/hosh-web*

# Copy templates and real source code
COPY templates ./templates
COPY src ./src

# Break the Cargo cache, build
RUN touch ./src/main.rs && cargo build --release

COPY static ./static

# Runtime stage
FROM debian:bullseye-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary and templates from builder
COPY --from=builder /usr/src/app/target/release/hosh-web /app/hosh-web
COPY --from=builder /usr/src/app/templates /app/templates
COPY --from=builder /usr/src/app/static /app/static

# Set the binary as executable
RUN chmod +x /app/hosh-web

CMD ["/app/hosh-web"]
