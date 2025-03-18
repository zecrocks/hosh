FROM rust:1.82-slim-bullseye

WORKDIR /usr/src/zec

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-watch for hot reloading
RUN cargo install cargo-watch

# We'll mount the source code from the host
CMD ["cargo", "watch", "-x", "run"] 