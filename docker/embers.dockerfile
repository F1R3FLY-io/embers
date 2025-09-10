FROM --platform=$BUILDPLATFORM rust:1.89-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev protobuf-compiler clang && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

COPY packages/firefly-client firefly-client
COPY packages/server server

WORKDIR /app/server
RUN cargo build --release

FROM --platform=$TARGETPLATFORM debian:bookworm-slim AS runtime

WORKDIR /app
RUN apt-get update && \
    apt-get install -y libssl3 && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/server/target/release/server ./

EXPOSE 3000

ENV EMBERS__PORT="3000"
ENV EMBERS__ADDRESS="::1"
ENV EMBERS__LOG_LEVEL="info"

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/server"]
