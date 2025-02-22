FROM rust:1.84-slim-bullseye

WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install cargo-watch

COPY Cargo.toml Cargo.lock ./
COPY src ./src

CMD ["cargo", "watch", "-x", "run"] 