FROM rust:1.75-bookworm

WORKDIR /usr/src/web

# Copy only the dependency files first
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build && \
    rm -rf src

# Command to run in development
CMD ["cargo", "run"] 