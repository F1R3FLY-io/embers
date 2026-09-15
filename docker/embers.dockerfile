FROM --platform=$BUILDPLATFORM rust:1.93-slim-bookworm AS builder

WORKDIR /app

# libssl-dev in both architectures: the dependency tree links system OpenSSL
# (openssl-sys), natively for the amd64 target and via the arm64 multiarch
# packages when cross-compiling. cmake and make serve native build scripts
# (libz-ng-sys builds zlib-ng through the cmake crate).
RUN dpkg --add-architecture arm64 && \
    apt-get update && \
    apt-get install -y pkg-config protobuf-compiler clang gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
       libssl-dev libssl-dev:arm64 cmake make

# The workspace pins a nightly toolchain (rust-toolchain.toml); copying it
# before any rustup/cargo invocation makes rustup install and use that exact
# toolchain, and the target additions below apply to it rather than to the
# image's stable default.
COPY rust-toolchain.toml ./
RUN rustup target add x86_64-unknown-linux-gnu && \
    rustup target add aarch64-unknown-linux-gnu

# Build inside the workspace with its Cargo.lock (--locked) so the container
# binary uses exactly the dependency versions the native builds pin.
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY packages packages

# gxhash requires the AES intrinsics as explicit target features (+sse2 is
# x86-only; on aarch64 only +aes is valid) - the same flags the native stack
# and CI export.
ARG TARGETPLATFORM
RUN \
    ( [ "$TARGETPLATFORM" = "linux/arm64" ] && \
    RUSTFLAGS="-C target-feature=+aes" \
    PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig \
    cargo build --release --locked -p embers --target aarch64-unknown-linux-gnu && \
    mv target/aarch64-unknown-linux-gnu/release/embers /app/embers-release \
    ) || \
    ( [ "$TARGETPLATFORM" = "linux/amd64" ] && \
    RUSTFLAGS="-C target-feature=+aes,+sse2" \
    cargo build --release --locked -p embers --target x86_64-unknown-linux-gnu && \
    mv target/x86_64-unknown-linux-gnu/release/embers /app/embers-release \
    ) || \
    { echo "Error: Unsupported TARGETPLATFORM: ${TARGETPLATFORM}" >&2; exit 1; }

# ---------------------------------------------------------------------------
# F1r3drive source. The default is a git clone of the upstream repository;
# override with a local checkout (no network, exact working tree) via:
#   docker build --build-context f1r3drive-src=/path/to/f1r3drive ...
# The scratch stage exists so the named build context can replace it with a
# directory whose root has the same shape.
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS f1r3drive-clone
RUN apt-get update && \
    apt-get install -y --no-install-recommends git ca-certificates && \
    rm -rf /var/lib/apt/lists/*
ARG F1R3DRIVE_GIT_URL=https://github.com/F1R3FLY-io/f1r3drive.git
ARG F1R3DRIVE_GIT_REF=main
RUN git clone "$F1R3DRIVE_GIT_URL" /out && \
    git -C /out checkout "$F1R3DRIVE_GIT_REF" && \
    rm -rf /out/.git

FROM scratch AS f1r3drive-src
COPY --from=f1r3drive-clone /out /

# F1r3drive builds with a Gradle toolchain pinned to Java 17 (build.gradle);
# the shaded JAR is the runnable artifact.
FROM eclipse-temurin:17-jdk-jammy AS f1r3drive-build
WORKDIR /work
COPY --from=f1r3drive-src / /work/
RUN --mount=type=cache,target=/root/.gradle ./gradlew --no-daemon shadowJar -x test

# ---------------------------------------------------------------------------
# Runtime: embers plus the co-located F1r3drive FUSE process, supervised by
# docker/embers-entrypoint.sh. Embers hard-requires its continuation store to
# sit on a FUSE mount whose fsname contains "f1r3drive", so the drive runs in
# the same container and mount namespace. The container must be started with
# devices: [/dev/fuse] and cap_add: [SYS_ADMIN] (and apparmor:unconfined on
# AppArmor hosts).
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates curl fuse3 libfuse2 openjdk-17-jre-headless \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/embers-release ./embers
COPY --from=f1r3drive-build /work/build/libs/f1r3drive-app.jar /app/f1r3drive-app.jar
COPY docker/embers-entrypoint.sh /app/embers-entrypoint.sh
RUN chmod +x /app/embers-entrypoint.sh

EXPOSE 3000

ENV EMBERS__PORT="3000"
ENV EMBERS__ADDRESS="::"
ENV EMBERS__LOG_LEVEL="info"
ENV F1R3DRIVE_ENABLED="true" \
    F1R3DRIVE_JAR="/app/f1r3drive-app.jar" \
    F1R3DRIVE_MOUNT="/mnt/f1r3drive" \
    F1R3DRIVE_KEY_FILE="/data/f1r3drive/f1r3drive-cipher.key" \
    F1R3DRIVE_VALIDATOR_PORT="40401" \
    F1R3DRIVE_OBSERVER_PORT="40401" \
    F1R3DRIVE_MANUAL_PROPOSE="true" \
    F1R3DRIVE_START_TIMEOUT_SECONDS="180" \
    NODE_WAIT_TIMEOUT_SECONDS="300"

STOPSIGNAL SIGINT
ENTRYPOINT ["/app/embers-entrypoint.sh"]
