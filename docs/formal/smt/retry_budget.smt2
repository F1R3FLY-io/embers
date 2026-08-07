; INV-A3 + INV-C5 — read-retry budget arithmetic (see ../INVARIANTS.md).
;
; The observer read path retries on an empty result with exponential backoff whose
; per-attempt delay is capped, under a total wall-clock budget. Two obligations,
; both checked by z3 and cvc5:
;
;   1. INV-C5: the total budget covers two full request timeouts. In the code
;      TOTAL_READ_BUDGET = 150 s and request_timeout = 60 s, and the client asserts
;      TOTAL_READ_BUDGET >= 2 * request_timeout (read_node_client.rs:30, :658).
;   2. INV-A3: no scheduled backoff delay exceeds its cap (the min caps the
;      exponential term), and a full run of `attempts` capped delays is bounded by
;      attempts * cap — so the schedule can never overrun the budget by more than
;      a bounded amount. Modelled concretely for the code's schedule.
;
; The exponential term base*2^i is abstracted as a fresh non-negative `raw`: the
; capped delay is min(raw, cap), so the bound holds for any monotone backoff and
; QF_LIA stays decidable (no non-linear 2^i).
;
; Runtime anchor: packages/firefly-client/src/read_node_client.rs:30, :178-234, :658.
; Validate:  z3 -smt2 retry_budget.smt2                              (expect: unsat x3)
;            cvc5 --incremental retry_budget.smt2                     (same)

(set-logic QF_LIA)
(set-option :produce-models true)

; ---------------------------------------------------------------------------
; Obligation 1 — INV-C5: the budget covers two full request timeouts (unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const total_budget    Int)
(declare-const request_timeout Int)
(assert (= total_budget 150))       ; TOTAL_READ_BUDGET, seconds
(assert (= request_timeout 60))     ; request_timeout, seconds
(assert (< total_budget (* 2 request_timeout)))  ; negation of the asserted invariant
(check-sat)                          ; EXPECT: unsat  (150 >= 120 holds)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 2 — INV-A3: a capped delay never exceeds the cap (unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const raw   Int)   ; the uncapped exponential term base*2^i (abstracted)
(declare-const cap   Int)   ; the per-attempt cap (5 * base in the code)
(declare-const capped Int)
(assert (>= raw 0))
(assert (>= cap 0))
(assert (= capped (ite (<= raw cap) raw cap)))   ; capped = min(raw, cap)
(assert (> capped cap))                          ; suppose a delay overran its cap
(check-sat)                                      ; EXPECT: unsat
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 3 — INV-A3: a full run of `attempts` capped delays is bounded by
; attempts * cap (unsat). Modelled for the code's default of 5 retries.
; ---------------------------------------------------------------------------
(push 1)
(declare-const cap Int)
(assert (>= cap 0))
(declare-const d0 Int) (declare-const d1 Int) (declare-const d2 Int)
(declare-const d3 Int) (declare-const d4 Int)
; each attempt's delay is a capped value in [0, cap]
(assert (and (>= d0 0) (<= d0 cap)))
(assert (and (>= d1 0) (<= d1 cap)))
(assert (and (>= d2 0) (<= d2 cap)))
(assert (and (>= d3 0) (<= d3 cap)))
(assert (and (>= d4 0) (<= d4 cap)))
; suppose the total exceeded the attempts * cap bound
(assert (> (+ d0 d1 d2 d3 d4) (* 5 cap)))
(check-sat)                                      ; EXPECT: unsat
(pop 1)
