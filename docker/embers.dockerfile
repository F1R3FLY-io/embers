FROM --platform=$BUILDPLATFORM rust:1.89-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev libssl-dev:arm64 protobuf-compiler clang gcc-aarch64-linux-gnu && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rustup target add x86_64-unknown-linux-gnu && \
    rustup target add aarch64-unknown-linux-gnu

COPY packages/firefly-client firefly-client
COPY packages/server server
COPY .cargo server/.cargo

WORKDIR /app/server

ARG TARGETPLATFORM
RUN \
    ( [ "$TARGETPLATFORM" = "linux/arm64" ] && \
    cargo build --release --target aarch64-unknown-linux-gnu && \
    mv target/aarch64-unknown-linux-gnu/release/server /app/server-release \
    ) || \
    ( [ "$TARGETPLATFORM" = "linux/amd64" ] && \
    cargo build --release --target x86_64-unknown-linux-gnu && \
    mv target/x86_64-unknown-linux-gnu/release/server /app/server-release \
    ) || \
    { echo "Error: Unsupported TARGETPLATFORM: ${TARGETPLATFORM}" >&2; exit 1; }

FROM debian:bookworm-slim AS runtime

WORKDIR /app
RUN apt-get update && \
    apt-get install -y --no-install-recommends libssl3 && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/server-release ./server

EXPOSE 3000

ENV EMBERS__PORT="3000"
ENV EMBERS__ADDRESS="::1"
ENV EMBERS__LOG_LEVEL="info"

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/server"]
