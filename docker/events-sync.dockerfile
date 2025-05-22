FROM rust:1.86-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev protobuf-compiler && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

COPY packages/etc etc
COPY packages/firefly-client firefly-client
COPY packages/events-sync events-sync

WORKDIR /app/events-sync
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

WORKDIR /app
RUN apt-get update && \
    apt-get install -y libssl3 && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/events-sync/target/release/events-sync ./

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/events-sync"]
