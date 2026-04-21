FROM rust:slim AS chef
# No need for libssl-dev anymore
RUN apt-get update && apt-get install -y \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
WORKDIR /token-gen



FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /token-gen/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/token-gen/target \
    cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/token-gen/target \
    cargo build --release && \
    cp /token-gen/target/release/token-gen /token-gen/token-gen

FROM debian:bookworm-slim AS runtime
COPY --from=builder /token-gen/token-gen .
CMD ["./token-gen"]
