#!/usr/bin/env bash
#
# L2 test orchestration — manage the local OpenAI stub and run the marker-selected
# suites against an ALREADY-RUNNING loopback dev-net. Everything is loopback; nothing
# reaches an external host (the campaign's no-egress constraint).
#
# This script is deliberately NOT coupled to any specific dev-net launcher. Bringing
# up the dev-net — a node (validator + observer), a F1r3drive mount for the
# continuation store, and optionally a PDS — is your responsibility; use whatever
# local tooling you launch it with. Point that node's OPENAI_API_BASE at this stub
# (http://127.0.0.1:8571/v1) so model calls stay on loopback. See
# docs/testing/test-architecture.md and docs/operations/local-stack.md.
#
#   scripts/l2.sh stub        build + start the OpenAI stub (loopback user unit)
#   scripts/l2.sh stub-stop   stop the stub
#   scripts/l2.sh test        run the devnet (L2) suite against $EMBERS_API_URL
#   scripts/l2.sh recovery    run the durable-recovery suite (needs EMBERS_SERVICE_UNIT)
#   scripts/l2.sh rust        run the hermetic L1 Rust suite

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
STUB_ADDR="${OPENAI_STUB_ADDR:-127.0.0.1:8571}"
STUB_UNIT="embers-openai-stub"
EMBERS_URL="${EMBERS_API_URL:-[::1]:8080}"
export RUSTFLAGS="${RUSTFLAGS:--C target-feature=+aes,+sse2}"

log() { printf '[l2] %s\n' "$*"; }
die() { printf '[l2] FATAL: %s\n' "$*" >&2; exit 1; }

cmd_stub() {
  log "building openai-stub (RUSTFLAGS=$RUSTFLAGS)"
  ( cd "$REPO" && cargo build --release -p openai-stub )
  if systemctl --user is-active --quiet "$STUB_UNIT" 2>/dev/null; then
    log "openai-stub already running ($STUB_UNIT)"
  else
    log "starting openai-stub on $STUB_ADDR ($STUB_UNIT)"
    systemd-run --user --unit="$STUB_UNIT" --property=Restart=on-failure \
      --setenv=OPENAI_STUB_ADDR="$STUB_ADDR" "$REPO/target/release/openai-stub" >/dev/null
  fi
  for _ in $(seq 1 30); do
    if curl -sf -o /dev/null "http://${STUB_ADDR}/v1/models"; then
      log "stub serving /v1/models — set your node's OPENAI_API_BASE=http://${STUB_ADDR}/v1"
      return 0
    fi
    sleep 0.5
  done
  die "openai-stub did not come up on $STUB_ADDR"
}

cmd_stub_stop() {
  log "stopping openai-stub ($STUB_UNIT)"
  systemctl --user stop "$STUB_UNIT" 2>/dev/null || true
  systemctl --user reset-failed "$STUB_UNIT" 2>/dev/null || true
}

cmd_test() {
  log "running the devnet (L2) suite against $EMBERS_URL"
  ( cd "$REPO/packages/embers" && EMBERS_API_URL="$EMBERS_URL" \
      poetry run pytest -m "devnet and not remote" --junitxml="$REPO/target/pytest-l2.xml" )
}

cmd_recovery() {
  log "running the durable-recovery suite against $EMBERS_URL"
  ( cd "$REPO/packages/embers" && EMBERS_API_URL="$EMBERS_URL" \
      poetry run pytest -m "recovery and not remote" --junitxml="$REPO/target/pytest-l2-recovery.xml" )
}

cmd_rust() { ( cd "$REPO" && cargo make test ); }

case "${1:-}" in
  stub) cmd_stub ;;
  stub-stop) cmd_stub_stop ;;
  test) cmd_test ;;
  recovery) cmd_recovery ;;
  rust) cmd_rust ;;
  *) echo "usage: $0 {stub|stub-stop|test|recovery|rust}" >&2; exit 2 ;;
esac
