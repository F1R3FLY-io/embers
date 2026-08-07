#!/usr/bin/env bash
#
# Prove every Tamarin model under docs/formal/tamarin/ and require that ALL of its
# lemmas are verified (none falsified). Exits non-zero on any falsified/errored lemma
# or a missing tool, so it can gate CI / `cargo make`.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 2   # docs/formal/
command -v tamarin-prover >/dev/null 2>&1 || { echo "error: tamarin-prover not found on PATH" >&2; exit 2; }

status=0
shopt -s nullglob
for model in tamarin/*.spthy; do
  out=$(timeout 600 tamarin-prover --prove "$model" 2>/dev/null)
  summary=$(sed -n '/summary of summaries/,$p' <<<"$out")
  total=$(grep -cE '\((all-traces|exists-trace)\):' <<<"$summary")
  verified=$(grep -c 'verified' <<<"$summary")
  falsified=$(grep -c 'falsified' <<<"$summary")
  if [[ "$total" -gt 0 && "$falsified" -eq 0 && "$verified" -eq "$total" ]]; then
    printf 'ok:   %-40s %d/%d lemmas verified\n' "$model" "$verified" "$total"
  else
    printf 'FAIL: %-40s verified=%d falsified=%d total=%d\n' "$model" "$verified" "$falsified" "$total"
    status=1
  fi
done

if [[ $status -eq 0 ]]; then
  echo "all Tamarin lemmas verified"
fi
exit $status
