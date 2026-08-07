#!/usr/bin/env bash
#
# Run every SMT model under docs/formal/smt/ with BOTH z3 and cvc5 and check its
# check-sat verdict sequence against the expected one recorded here. Exits
# non-zero on any mismatch or missing solver, so it can gate CI / `cargo make`.
#
# The expected sequences match the (push/check-sat/pop) obligations documented in
# each model's header. Keep this table in sync when adding a model.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 2   # docs/formal/

# model  ->  space-separated expected verdicts, in order
declare -A EXPECTED=(
  ["smt/history_restore_conservation.smt2"]="unsat sat"
  ["smt/quorum_thresholds.smt2"]="sat unsat"
  ["smt/retry_budget.smt2"]="unsat unsat unsat"
  ["smt/overflow_bv.smt2"]="unsat sat"
)

for solver in z3 cvc5; do
  command -v "$solver" >/dev/null 2>&1 || { echo "error: $solver not found on PATH" >&2; exit 2; }
done

status=0
for model in "${!EXPECTED[@]}"; do
  expected="${EXPECTED[$model]}"
  for invocation in "z3 -smt2" "cvc5 --incremental"; do
    got=$($invocation "$model" 2>/dev/null | grep -E '^(sat|unsat)$' | tr '\n' ' ' | sed 's/ *$//')
    solver=${invocation%% *}
    if [[ "$got" == "$expected" ]]; then
      printf 'ok:   %-40s %-5s -> %s\n' "$model" "$solver" "$got"
    else
      printf 'FAIL: %-40s %-5s expected [%s], got [%s]\n' "$model" "$solver" "$expected" "$got"
      status=1
    fi
  done
done

if [[ $status -eq 0 ]]; then
  echo "all SMT models validated by z3 and cvc5"
fi
exit $status
