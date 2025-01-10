FROM rust:latest AS builder

WORKDIR /app

COPY Cargo.toml .

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src/

# Now copy the real source code
COPY src/ src/

RUN cargo build --release

# ---- Runtime Stage ----
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y ca-certificates procps && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nats-example /usr/local/bin/nats-example

HEALTHCHECK --interval=5s --timeout=3s --start-period=2s --retries=3 \
    CMD pgrep nats-example || exit 1

# Run the application
CMD ["nats-example"]

