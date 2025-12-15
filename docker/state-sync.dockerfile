FROM rust:1.92-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config protobuf-compiler && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY packages/firefly-client-macros firefly-client-macros
COPY packages/firefly-client firefly-client
COPY packages/state-sync state-sync

WORKDIR /app/state-sync
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

ARG POSTGRESQL_VERSION
WORKDIR /app
RUN apt-get update && \
    apt-get install -y --no-install-recommends gnupg wget && \
    echo "deb http://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" > /etc/apt/sources.list.d/pgdg.list && \
    wget --quiet -O - https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor -o /etc/apt/trusted.gpg.d/postgresql.gpg && \
    apt-get update && \
    apt-get install -y --no-install-recommends postgresql-client-${POSTGRESQL_VERSION} && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/state-sync/target/release/state-sync ./

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/state-sync"]
