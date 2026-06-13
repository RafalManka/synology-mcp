# ── Builder stage ─────────────────────────────────────────────────────────────
FROM rust:1.96-slim AS builder

RUN apt-get update && apt-get install -y \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /app

# Cache dependencies — copy manifests first, build a dummy binary, then replace source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs \
    && cargo build --release --target x86_64-unknown-linux-musl \
    && rm -rf src

# Build the real binary
COPY src ./src
RUN touch src/main.rs \
    && cargo build --release --target x86_64-unknown-linux-musl

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/synology-mcp /synology-mcp
ENTRYPOINT ["/synology-mcp"]
