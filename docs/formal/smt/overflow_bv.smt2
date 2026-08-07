; INV-A4 — no arithmetic overflow in the retry/step arithmetic (see ../INVARIANTS.md).
;
; The retry-delay computation multiplies a u32 multiplier by a u32 attempt index.
; Done in u32 that product can wrap; the code widens first. This bit-vector model
; discharges, with z3 and cvc5:
;
;   1. The widened product is always safe: for every pair of u32 operands, the
;      exact product fits in u64 (its bits above 64 are zero)  ->  unsat that it
;      overflows.
;   2. The un-widened product is genuinely unsafe: some u32 pair has a true product
;      that exceeds u32  ->  sat, with a concrete witness. This is why the widening
;      is necessary rather than cosmetic.
;
; Runtime anchor: packages/embers/src/domain/agents_teams/{workflow.rs (retry delay),
;   runtime.rs:723 (the 10^6 step budget)}.
; Validate:  z3 -smt2 overflow_bv.smt2                              (expect: unsat, then sat)
;            cvc5 --incremental --produce-models overflow_bv.smt2    (same)

(set-logic QF_BV)
(set-option :produce-models true)

; ---------------------------------------------------------------------------
; Obligation 1 — the widened u32 * u32 product fits in u64 (expect: unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const m (_ BitVec 32))
(declare-const a (_ BitVec 32))
; the exact product, computed with enough width (128 bits) to never truncate
(define-fun prod128 () (_ BitVec 128)
  (bvmul ((_ zero_extend 96) m) ((_ zero_extend 96) a)))
; suppose the exact product does not fit in 64 bits (some bit at position >= 64 is set)
(assert (not (= ((_ extract 127 64) prod128) (_ bv0 64))))
(check-sat)                          ; EXPECT: unsat  (u32 * u32 < 2^64 always)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 2 — the un-widened product can exceed u32 (expect: sat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const m (_ BitVec 32))
(declare-const a (_ BitVec 32))
; the true product in 64 bits
(define-fun prod64 () (_ BitVec 64)
  (bvmul ((_ zero_extend 32) m) ((_ zero_extend 32) a)))
; the true product's high 32 bits are non-zero: it would overflow a u32 accumulator
(assert (not (= ((_ extract 63 32) prod64) (_ bv0 32))))
(check-sat)                          ; EXPECT: sat  (widening is necessary)
(get-model)
(pop 1)
