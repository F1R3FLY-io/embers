#!/usr/bin/env bash
#
# Run the Storm probabilistic models under docs/formal/storm/ and check each
# acceptance threshold. Exits non-zero on any breach or a missing solver, so it can
# gate CI / `cargo make`.
#
# observer_retry: property 1 is P=? [F "flake"] — the give-up-without-value
# probability at the pessimistic defaults. Acceptance: strictly below 1e-3.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 2   # docs/formal/

command -v storm >/dev/null 2>&1 || { echo "error: storm not found on PATH" >&2; exit 2; }

status=0

out=$(storm --prism storm/observer_retry.pm --prop storm/observer_retry.props 2>/dev/null)
mapfile -t results < <(printf '%s\n' "$out" | grep 'Result (for initial states)' | sed -E 's/.*: *//')

flake="${results[0]:-NaN}"
success="${results[1]:-NaN}"

if awk -v f="$flake" 'BEGIN { exit !(f+0 < 1e-3) }'; then
  printf 'ok:   storm/observer_retry.pm  P(flake)=%s < 1e-3  (P(success)=%s)\n' "$flake" "$success"
else
  printf 'FAIL: storm/observer_retry.pm  P(flake)=%s is not < 1e-3\n' "$flake"
  status=1
fi

if [[ $status -eq 0 ]]; then
  echo "all Storm models within acceptance thresholds"
fi
exit $status
