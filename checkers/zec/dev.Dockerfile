FROM rust:1.85-slim-bullseye

WORKDIR /usr/src/zec

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    git \
    make \
    && rm -rf /var/lib/apt/lists/*

CMD ["cargo", "run"] 