FROM rust:1.72

WORKDIR /app

# Update rustup and Cargo to ensure compatibility with lock file version 4
RUN rustup update && rustup show

# Copy dependency files to cache dependencies
COPY Cargo.toml Cargo.lock ./

# Add dummy main.rs to allow `cargo fetch`
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Pre-fetch dependencies
RUN cargo fetch

# Copy the rest of the project files (excluding target if needed)
COPY . .

# Add a build argument to switch between release and debug builds
ARG BUILD_MODE=release

# Print and build the application based on the mode
RUN echo "Building in $BUILD_MODE mode" && \
    if [ "$BUILD_MODE" = "release" ]; then \
      cargo build --release; \
    else \
      cargo build; \
    fi

EXPOSE 5000

# Run the application based on the mode
CMD ["sh", "-c", "if [ \"$BUILD_MODE\" = \"release\" ]; then cargo run --release; else cargo run; fi"]
