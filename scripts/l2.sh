#!/usr/bin/env bash
#
# L2 orchestration — bring up the loopback dev-net with the OpenAI calls stubbed
# locally, run the marker-selected pytest suite against it, and tear it down. Every
# endpoint is loopback; the machine never reaches an external host (the campaign's
# hard no-egress constraint). See docs/testing/test-architecture.md and
# docs/operations/local-stack.md.
#
#   scripts/l2.sh up        build embers + the stub, arm secrets.env, start the stack
#   scripts/l2.sh test      run the devnet (L2) pytest suite against the running stack
#   scripts/l2.sh recovery  run the durable-recovery test (restarts embers mid-run)
#   scripts/l2.sh rust      run the hermetic L1 Rust suite
#   scripts/l2.sh down      stop the stub unit and the stack
#
# The OpenAI stub is the key to hermeticity: the NODE (not embers) points its
# OPENAI_API_BASE at the stub, so even model calls stay on loopback. This script
# arms that by writing a MANAGED marker block into the sibling stack's untracked
# secrets.env; it refuses if a different, unmanaged base is already set.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
STACK_DIR="${EMBERS_LOCAL_STACK_DIR:-$REPO/../embers-local-stack}"
STACK="$STACK_DIR/stack.sh"
SECRETS="$STACK_DIR/secrets.env"
STUB_ADDR="${OPENAI_STUB_ADDR:-127.0.0.1:8571}"
STUB_BASE="http://${STUB_ADDR}/v1"
STUB_UNIT="embers-openai-stub"
EMBERS_URL="${EMBERS_API_URL:-[::1]:8080}"
RUSTFLAGS="${RUSTFLAGS:--C target-feature=+aes,+sse2}"
export RUSTFLAGS
MARKER_BEGIN="# >>> embers-l2 managed (OpenAI stub) >>>"
MARKER_END="# <<< embers-l2 managed (OpenAI stub) <<<"

log() { printf '[l2] %s\n' "$*"; }
die() { printf '[l2] FATAL: %s\n' "$*" >&2; exit 1; }

require_stack() { [[ -x "$STACK" ]] || die "stack.sh not found/executable at $STACK (set EMBERS_LOCAL_STACK_DIR)"; }

build() {
  log "building embers + openai-stub (RUSTFLAGS=$RUSTFLAGS)"
  ( cd "$REPO" && cargo build --release -p embers -p openai-stub )
}

start_stub() {
  # Idempotent user unit; restart-on-failure. Serves loopback only.
  if systemctl --user is-active --quiet "$STUB_UNIT" 2>/dev/null; then
    log "openai-stub already running ($STUB_UNIT)"
  else
    log "starting openai-stub on $STUB_ADDR ($STUB_UNIT)"
    systemd-run --user --unit="$STUB_UNIT" --property=Restart=on-failure \
      --setenv=OPENAI_STUB_ADDR="$STUB_ADDR" \
      "$REPO/target/release/openai-stub" >/dev/null
  fi
  # wait for /v1/models
  for _ in $(seq 1 30); do
    if curl -sf -o /dev/null "http://${STUB_ADDR}/v1/models"; then log "stub is serving /v1/models"; return 0; fi
    sleep 0.5
  done
  die "openai-stub did not come up on $STUB_ADDR"
}

arm_secrets() {
  [[ -f "$SECRETS" ]] || die "secrets.env not found at $SECRETS (operator-provided; see the stack's secrets.env.example)"
  if grep -q "$MARKER_BEGIN" "$SECRETS"; then
    log "secrets.env already carries the managed OpenAI-stub marker"
    return 0
  fi
  # Refuse to clobber a different, unmanaged base.
  if grep -qE '^\s*export\s+OPENAI_API_BASE=' "$SECRETS"; then
    local cur
    cur="$(grep -E '^\s*export\s+OPENAI_API_BASE=' "$SECRETS" | tail -1)"
    [[ "$cur" == *"$STUB_BASE"* ]] || die "secrets.env sets a different OPENAI_API_BASE ($cur); refusing to override. Remove it to let L2 manage it."
    log "secrets.env already points OPENAI_API_BASE at the stub"
    return 0
  fi
  log "arming secrets.env with OPENAI_API_BASE=$STUB_BASE (managed block)"
  { echo ""; echo "$MARKER_BEGIN"; echo "export OPENAI_API_BASE=$STUB_BASE"; echo "$MARKER_END"; } >> "$SECRETS"
}

wait_http() { # url label
  for _ in $(seq 1 120); do curl -sf -o /dev/null "$1" && { log "$2 ready"; return 0; }; sleep 1; done
  die "$2 not reachable at $1"
}

check_stub_uptake() {
  # The node's startup validation must have hit the stub's /v1/models; if not, the
  # node did not pick up OPENAI_API_BASE (it re-sources secrets.env on start).
  local hits
  hits="$(curl -sf "http://${STUB_ADDR}/stub/requests" | grep -c '/v1/models' || true)"
  if [[ "${hits:-0}" -ge 1 ]]; then
    log "stub uptake confirmed (node hit /v1/models $hits time(s))"
  else
    die "the node has not hit the stub's /v1/models — it did not pick up OPENAI_API_BASE. Restart the node units (stack.sh repair-nodes) after arming secrets.env."
  fi
}

check_schema_fresh() {
  # Staleness trap (frontend agent's suggestion): the RUNNING server's spec must match
  # the repo's generated schema, or a test is asserting against a stale binary.
  local served repo_spec
  served="$(curl -sf "http://${EMBERS_URL#[}"/swagger-ui/openapi.json 2>/dev/null || curl -sf "http://[::1]:8080/swagger-ui/openapi.json" || true)"
  repo_spec="$(cd "$REPO" && cargo run --quiet -p embers --bin generate-schema 2>/dev/null || true)"
  if [[ -n "$served" && -n "$repo_spec" ]]; then
    if [[ "$(printf '%s' "$served" | sha256sum)" == "$(printf '%s' "$repo_spec" | sha256sum)" ]]; then
      log "served OpenAPI matches the repo schema"
    else
      log "WARNING: the running server's OpenAPI differs from the repo schema — the deployed embers may be stale (rebuild + restart embers)."
    fi
  fi
}

cmd_up() {
  require_stack
  build
  arm_secrets
  start_stub
  log "bringing up the stack (stack.sh up)"
  "$STACK" up
  wait_http "http://127.0.0.1:40403/status" "validator HTTP"
  wait_http "http://127.0.0.1:40413/status" "observer HTTP"
  wait_http "http://[::1]:8080/swagger-ui/openapi.json" "embers"
  check_stub_uptake
  check_schema_fresh
  log "L2 up. Run: scripts/l2.sh test"
}

cmd_test() {
  log "running the devnet (L2) suite against $EMBERS_URL"
  ( cd "$REPO/packages/embers" && EMBERS_API_URL="$EMBERS_URL" \
      poetry run pytest -m "devnet and not remote" --junitxml="$REPO/target/pytest-l2.xml" )
}

cmd_recovery() {
  log "running the durable-recovery test"
  ( cd "$REPO/packages/embers" && EMBERS_API_URL="$EMBERS_URL" \
      poetry run pytest -m "recovery and not remote" --junitxml="$REPO/target/pytest-l2-recovery.xml" )
}

cmd_rust() { ( cd "$REPO" && cargo make test ); }

cmd_down() {
  log "stopping openai-stub ($STUB_UNIT)"
  systemctl --user stop "$STUB_UNIT" 2>/dev/null || true
  systemctl --user reset-failed "$STUB_UNIT" 2>/dev/null || true
  if [[ -x "$STACK" ]]; then log "stopping the stack"; "$STACK" stop || true; fi
}

case "${1:-}" in
  up) cmd_up ;;
  test) cmd_test ;;
  recovery) cmd_recovery ;;
  rust) cmd_rust ;;
  down) cmd_down ;;
  *) echo "usage: $0 {up|test|recovery|rust|down}" >&2; exit 2 ;;
esac
