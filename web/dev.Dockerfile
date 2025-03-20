FROM rust:1.82-slim-bullseye

WORKDIR /usr/src/web

# Install required dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Dependency caching
COPY Cargo.toml Cargo.lock ./

RUN mkdir -p src && \
    echo "fn main() {println!(\"dummy\");}" > src/main.rs && \
    cargo build && \
    rm -rf src target/debug/hosh-web*

# Command to run in development
CMD ["cargo", "run"] 