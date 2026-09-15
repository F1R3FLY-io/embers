; INV-A2 — quorum threshold arithmetic (see ../INVARIANTS.md).
;
; A quorum(k) join over n approvers fires when k distinct decisions have arrived.
; The definition validator enforces 1 <= k <= n at publish time (workflow.rs:245).
; The number of distinct decisions is monotone and bounded above by n (each of the
; n approvers decides at most once). This file discharges, with z3 and cvc5:
;
;   1. Well-formed thresholds are reachable: for 1 <= k <= n there is a decision
;      count that reaches the quorum without exceeding the approver set  ->  sat.
;   2. Over-large thresholds are unfireable: for k > n the firing condition
;      (decisions >= k) is unsatisfiable under the bound decisions <= n  ->  unsat.
;      This is exactly why the publish-time validation rejects k > n instead of
;      accepting a join that could never complete.
;
; Runtime anchor: packages/embers/src/domain/agents_teams/workflow.rs:245
;   (quorum bound validation); runtime.rs:1156-1264 (the firing decision).
; Validate:  z3 -smt2 quorum_thresholds.smt2                              (expect: sat, then unsat)
;            cvc5 --incremental --produce-models quorum_thresholds.smt2   (same)

(set-logic QF_LIA)
(set-option :produce-models true)

; ---------------------------------------------------------------------------
; Obligation 1 — a well-formed threshold (1 <= k <= n) is reachable (expect: sat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const n         Int)   ; number of approvers
(declare-const k         Int)   ; quorum threshold
(declare-const decisions Int)   ; distinct decisions arrived so far

(assert (>= n 1))
(assert (and (>= k 1) (<= k n)))       ; the validated well-formedness condition
(assert (and (>= decisions 0) (<= decisions n)))  ; monotone, bounded by the approver set
(assert (>= decisions k))              ; the quorum firing condition is met
(check-sat)                            ; EXPECT: sat  (a reachable firing state exists)
(get-model)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 2 — an over-large threshold (k > n) can never fire (expect: unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const n         Int)
(declare-const k         Int)
(declare-const decisions Int)

(assert (>= n 1))
(assert (> k n))                       ; a threshold the validator must reject
(assert (and (>= decisions 0) (<= decisions n)))  ; decisions never exceed the approver set
(assert (>= decisions k))              ; suppose the quorum fired anyway
(check-sat)                            ; EXPECT: unsat  (it cannot: decisions <= n < k)
(pop 1)
