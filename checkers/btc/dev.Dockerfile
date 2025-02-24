FROM rust:1.82-slim-bullseye

WORKDIR /usr/src/btc

# Install required dependencies for OpenSSL, Rust, and cargo
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

# Set environment variable for run mode
ENV RUN_MODE=worker

CMD ["cargo", "run"] 