FROM rust:1.85-slim-bullseye

WORKDIR /usr/src/app

# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

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

# Set environment variable for incremental builds
ENV RUST_INCREMENTAL=1

# Use cargo-watch with improved options for better development experience
CMD ["cargo", "watch", "-q", "-c", "-w", "src", "-x", "run"] 