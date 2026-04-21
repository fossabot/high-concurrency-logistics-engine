FROM rust:slim AS chef
# No need for libssl-dev anymore
RUN apt-get update && apt-get install -y \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
WORKDIR /axum-api



FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /axum-api/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/axum-api/target \
    cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/axum-api/target \
    cargo build --release && \
    cp /axum-api/target/release/axum-api /axum-api/axum-api

FROM debian:bookworm-slim AS runtime
COPY --from=builder /axum-api/axum-api .
EXPOSE 8080
CMD ["./axum-api"]
