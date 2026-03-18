# Stage 1: Build for ARM64
FROM rust:1.88-bookworm AS builder

RUN apt-get update && apt-get install -y \
    gcc-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add aarch64-unknown-linux-musl

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc
ENV CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc

RUN cargo build --release --target aarch64-unknown-linux-musl

# Stage 2: Minimal runtime
FROM scratch

COPY --from=builder /build/target/aarch64-unknown-linux-musl/release/goldentooth-mcp /goldentooth-mcp

EXPOSE 8080 8443

ENTRYPOINT ["/goldentooth-mcp"]
