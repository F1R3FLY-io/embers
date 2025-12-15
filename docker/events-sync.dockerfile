FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config protobuf-compiler && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY packages/firefly-client-macros firefly-client-macros
COPY packages/firefly-client firefly-client
COPY packages/events-sync events-sync

WORKDIR /app/events-sync
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

WORKDIR /app

COPY --from=builder /app/events-sync/target/release/events-sync ./

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/events-sync"]
