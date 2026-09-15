#!/usr/bin/env bash
#
# Linearize each mCRL2 model under docs/formal/mcrl2/ and solve its modal-mu formulas,
# checking each verdict against the expected one. Exits non-zero on any mismatch or a
# missing tool, so it can gate CI / `cargo make`. Intermediate .lps/.pbes files go to a
# scratch dir that is removed on exit.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 2   # docs/formal/

for tool in mcrl22lps lps2pbes pbessolve; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool not found on PATH" >&2; exit 2; }
done

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

status=0

# model  ->  "formula.mcf=expected ..." checks
model="mcrl2/red_team_choreography.mcrl2"
if ! mcrl22lps "$model" "$tmp/model.lps" >/dev/null 2>&1; then
  echo "FAIL: $model did not linearize"
  exit 1
fi

declare -A EXPECTED=(
  ["mcrl2/nodeadlock.mcf"]="true"
  ["mcrl2/join_progress.mcf"]="true"
)

for mcf in "${!EXPECTED[@]}"; do
  expected="${EXPECTED[$mcf]}"
  got=$( { lps2pbes -f "$mcf" "$tmp/model.lps" "$tmp/prop.pbes" >/dev/null 2>&1 \
           && pbessolve "$tmp/prop.pbes" 2>/dev/null; } | grep -oE 'true|false' | head -1)
  if [[ "$got" == "$expected" ]]; then
    printf 'ok:   %-28s -> %s\n' "$mcf" "$got"
  else
    printf 'FAIL: %-28s expected [%s], got [%s]\n' "$mcf" "$expected" "$got"
    status=1
  fi
done

if [[ $status -eq 0 ]]; then
  echo "all mCRL2 properties hold"
fi
exit $status
