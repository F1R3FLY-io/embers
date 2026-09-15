; INV-A1 — history-restore token conservation (see ../INVARIANTS.md).
;
; The kernel keeps a stored live-token counter `alpha` (ScopeInstance.active_tokens)
; that must equal the ground-truth live population `live` at every quiescent point
; (INV-K1 / CounterFaithful). This file models the FOUR count-changing operations
; as their arithmetic effect on (alpha, live) and discharges two obligations:
;
;   1. Under the correct semantics every operation preserves alpha = live, for all
;      symbolic pre-states and operation choices  ->  the negation is `unsat`.
;   2. The pre-fix `restore` (which grew the live population but left the counter
;      unchanged) breaks it  ->  a concrete `sat` counterexample. That is the
;      machine-checked shadow of the defect fixed in commit 63906a4
;      ("account for every token a history node restores").
;
; Runtime anchor: packages/embers/src/domain/agents_teams/runtime.rs:1052-1082
;   (restore_history), :1110-1142 (consume), :1448-1457 (fan-out), seeding.
; Validate:  z3 -smt2 history_restore_conservation.smt2                 (expect: unsat, then sat)
;            cvc5 --incremental --produce-models history_restore_conservation.smt2   (same;
;            --incremental is required for the push/pop blocks below)

(set-logic QF_LIA)
(set-option :produce-models true)

; ---------------------------------------------------------------------------
; Obligation 1 — correct semantics preserve conservation (expect: unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const alpha Int)   ; stored counter, pre-state
(declare-const live  Int)   ; live population, pre-state
(declare-const n     Int)   ; seed count / restored population
(declare-const k     Int)   ; fan-out arity
(declare-const op    Int)   ; 0 = seed(+n), 1 = fan-out(+(k-1)), 2 = consume(-1), 3 = restore(+n)

(assert (= alpha live))     ; precondition: the counter is faithful
(assert (>= n 0))
(assert (>= k 1))
(assert (and (>= op 0) (<= op 3)))

; The correct semantics apply the SAME delta to the counter and the population.
(define-fun delta () Int
  (ite (= op 0) n
  (ite (= op 1) (- k 1)
  (ite (= op 2) (- 1)
                n))))       ; op = 3: restore adds the restored population n

; A consume only fires on a non-empty scope (the kernel snapshots at count zero).
(assert (=> (= op 2) (>= live 1)))

(declare-const alpha2 Int)
(declare-const live2  Int)
(assert (= alpha2 (+ alpha delta)))
(assert (= live2  (+ live  delta)))

; Seek a reachable post-state that violates conservation. None exists.
(assert (not (= alpha2 live2)))
(check-sat)                 ; EXPECT: unsat
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 2 — the pre-fix restore is falsifiable (expect: sat + model).
; ---------------------------------------------------------------------------
(push 1)
(declare-const alpha Int)
(declare-const live  Int)
(declare-const m     Int)   ; restored population, strictly positive to exhibit the bug

(assert (= alpha live))
(assert (>= m 1))

(declare-const alpha2 Int)
(declare-const live2  Int)
(assert (= alpha2 alpha))            ; BUG (pre-63906a4): restore left the counter unchanged
(assert (= live2  (+ live m)))       ; but the live population grew by the restored tokens
(assert (not (= alpha2 live2)))      ; so conservation fails
(check-sat)                          ; EXPECT: sat
(get-model)
(pop 1)
