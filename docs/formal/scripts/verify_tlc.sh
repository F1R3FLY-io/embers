#!/usr/bin/env bash
#
# Run each TLA+ model+config under docs/formal/tla/ with TLC and check the
# outcome against expectation. Positive configs must report "No error"; witness
# configs must report the SPECIFIC invariant violation they exist to exhibit.
# Exits non-zero on any mismatch. Keep the checks in sync when adding a model.

set -uo pipefail

cd "$(dirname "$0")/../tla" || exit 2
command -v tlc >/dev/null 2>&1 || { echo "error: tlc not found on PATH" >&2; exit 2; }

status=0

# check <module> <config> <expected-substring> <description>
check() {
  local module="$1" config="$2" expected="$3" desc="$4" out
  out=$(tlc -config "$config" "$module.tla" 2>&1)
  if grep -qF "$expected" <<<"$out"; then
    printf 'ok:   %-42s -> %s\n' "$config" "$desc"
  else
    printf 'FAIL: %-42s expected /%s/\n' "$config" "$expected"
    status=1
  fi
}

check DeployOnce          DeployOnce.cfg               "No error has been found"                    "no-retry: at-most-once landing"
check DeployOnce          DeployOnceRetry.cfg          "Invariant AtMostOnceLanding is violated"    "retry: duplicate-landing witness"
check ObserverConsistency ObserverConsistency.cfg      "No error has been found"                    "budget covers lag"
check ObserverConsistency ObserverConsistencyFlake.cfg "Invariant BudgetCoversLag is violated"      "flake witness"

rm -rf states   # TLC scratch metadir (gitignored anyway)

if [[ $status -eq 0 ]]; then
  echo "all TLA+ models validated (positive configs pass; witness configs falsify as expected)"
fi
exit $status
