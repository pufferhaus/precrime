FROM --platform=linux/arm64 debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential pkg-config \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    libcairo2-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal
ENV PATH=/root/.cargo/bin:$PATH

# Add the target explicitly so `cargo build --target aarch64-unknown-linux-gnu`
# produces output in target/aarch64-unknown-linux-gnu/release/ (consistent
# with the cross output path used by deploy targets).
RUN rustup target add aarch64-unknown-linux-gnu
