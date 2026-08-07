#!/usr/bin/env bash
#
# Compile every Rocq proof under docs/formal/rocq/ and gate on it being fully
# constructive: no Admitted/Abort/admit in the source, and every listed theorem
# reports "Closed under the global context" from Print Assumptions (so no axiom or
# admit slipped in). Exits non-zero on any failure. Heavy proofs compile under a
# systemd-run resource cap when available.

set -uo pipefail

cd "$(dirname "$0")/../rocq" || exit 2
command -v coqc >/dev/null 2>&1 || { echo "error: coqc not found on PATH" >&2; exit 2; }

CAP=(systemd-run --user --scope -p MemoryMax=8G -p CPUQuota=400% --quiet)
command -v systemd-run >/dev/null 2>&1 || CAP=(env)

# file  ->  space-separated theorem names to assumption-check
declare -A THEOREMS=(
  ["MergeJournal.v"]="durable_is_subseq on_chain_is_subseq"
  ["ProgressBoundary.v"]="progress_boundary_iff_residual_acyclic"
)

status=0
for v in "${!THEOREMS[@]}"; do
  # 1. source-level gate: these must never appear in a complete proof
  if grep -nE '\bAdmitted\b|\bAbort\b|(^|[^[:alpha:]])admit[[:space:].]' "$v" >/dev/null; then
    printf 'FAIL: %-20s contains an admit/Abort\n' "$v"; status=1; continue
  fi

  # 2. compile
  if ! "${CAP[@]}" coqc -Q . Embers "$v" >/dev/null 2>&1; then
    printf 'FAIL: %-20s did not compile\n' "$v"; status=1; continue
  fi

  # 3. Print Assumptions for each theorem must be closed under the global context
  module="${v%.v}"
  # NB: the helper's basename becomes a Coq module name, so it must be dot-free
  # (mktemp's tmp.XXXX.v is rejected). Keep it in-tree; it is gitignored.
  check="_assumptions_check.v"
  { echo "From Embers Require Import ${module}."
    for thm in ${THEOREMS[$v]}; do echo "Print Assumptions ${thm}."; done
  } > "$check"
  out=$("${CAP[@]}" coqc -Q . Embers "$check" 2>&1)
  rm -f "$check" "${check%.v}.vo" "${check%.v}.glob" 2>/dev/null

  n_thms=$(wc -w <<<"${THEOREMS[$v]}")
  n_closed=$(grep -c 'Closed under the global context' <<<"$out")
  if [[ "$n_closed" -eq "$n_thms" ]]; then
    printf 'ok:   %-20s %d theorem(s), all closed under the global context\n' "$v" "$n_thms"
  else
    printf 'FAIL: %-20s only %d/%d theorems axiom-free\n' "$v" "$n_closed" "$n_thms"; status=1
  fi
done

if [[ $status -eq 0 ]]; then
  echo "all Rocq proofs compile and are axiom-free"
fi
exit $status
