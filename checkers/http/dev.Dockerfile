FROM rust:1.85-slim-bullseye

WORKDIR /usr/src/app

# Install cargo-watch
RUN cargo install cargo-watch

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build && \
    rm -rf src

# Copy the real source code
COPY . .

# Run with cargo-watch for hot reloading
CMD ["cargo", "watch", "-x", "run"] 