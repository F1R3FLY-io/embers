#!/usr/bin/env bash
#
# Run every ProVerif model under docs/formal/proverif/ and check its RESULT verdict
# sequence against the expected one recorded here. Exits non-zero on any mismatch or
# a missing solver, so it can gate CI / `cargo make`.
#
# ProVerif prints one `RESULT ... is true.` / `... is false.` line per query, in
# declaration order. A refuted injective correspondence additionally prints a
# `RESULT (but ... is true.)` line for the non-injective fallback — that line is part
# of the pinned sequence (it classifies as `true`). Keep this table in sync with each
# model's header when adding or reordering queries.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 2   # docs/formal/

# model  ->  space-separated expected verdicts, in RESULT order
declare -A EXPECTED=(
  # binding=true, secrecy=true, replay inj=false (+ non-inj "but" line=true), restart=true
  ["proverif/two_phase_jwt.pv"]="true true false true true"
  # secrecy=true, single-use inj=true, owner-only=true
  ["proverif/reveal_output.pv"]="true true true"
)

command -v proverif >/dev/null 2>&1 || { echo "error: proverif not found on PATH" >&2; exit 2; }

status=0
for model in "${!EXPECTED[@]}"; do
  expected="${EXPECTED[$model]}"
  # Map each RESULT line to a token; unrecognized RESULT lines pass through unchanged
  # so an unexpected verdict shape fails the comparison loudly.
  got=$(proverif "$model" 2>/dev/null \
        | grep '^RESULT' \
        | sed -E 's/.*is true\.?\)?$/true/; s/.*is false\.?$/false/; s/.*cannot be proved.*/cannotprove/' \
        | tr '\n' ' ' | sed 's/ *$//')
  if [[ "$got" == "$expected" ]]; then
    printf 'ok:   %-32s proverif -> %s\n' "$model" "$got"
  else
    printf 'FAIL: %-32s proverif expected [%s], got [%s]\n' "$model" "$expected" "$got"
    status=1
  fi
done

if [[ $status -eq 0 ]]; then
  echo "all ProVerif models validated"
fi
exit $status
