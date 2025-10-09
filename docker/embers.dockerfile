FROM --platform=$BUILDPLATFORM rust:1.90-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config protobuf-compiler clang gcc-aarch64-linux-gnu libc6-dev-arm64-cross && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rustup target add x86_64-unknown-linux-gnu && \
    rustup target add aarch64-unknown-linux-gnu

COPY packages/firefly-client-macros firefly-client-macros
COPY packages/firefly-client firefly-client
COPY packages/embers embers
COPY .cargo embers/.cargo

WORKDIR /app/embers

ARG TARGETPLATFORM
RUN \
    ( [ "$TARGETPLATFORM" = "linux/arm64" ] && \
    cargo build --release --target aarch64-unknown-linux-gnu && \
    mv target/aarch64-unknown-linux-gnu/release/embers /app/embers-release \
    ) || \
    ( [ "$TARGETPLATFORM" = "linux/amd64" ] && \
    cargo build --release --target x86_64-unknown-linux-gnu && \
    mv target/x86_64-unknown-linux-gnu/release/embers /app/embers-release \
    ) || \
    { echo "Error: Unsupported TARGETPLATFORM: ${TARGETPLATFORM}" >&2; exit 1; }

FROM debian:bookworm-slim AS runtime

WORKDIR /app

COPY --from=builder /app/embers-release ./embers

EXPOSE 3000

ENV EMBERS__PORT="3000"
ENV EMBERS__ADDRESS="::"
ENV EMBERS__LOG_LEVEL="info"

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/embers"]
