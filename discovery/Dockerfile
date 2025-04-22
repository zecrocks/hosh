FROM rust:1.85-slim-bullseye AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app
COPY . .

RUN cargo build --release

FROM debian:bullseye-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/discovery /usr/local/bin/discovery

CMD ["discovery"] 