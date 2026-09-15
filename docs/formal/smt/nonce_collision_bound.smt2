; INV-S6 (nonce hygiene) — AES-256-GCM random-nonce birthday bound
; (see ../INVARIANTS.md).
;
; The continuation store draws a fresh 96-bit nonce per encryption from the OS CSPRNG
; (Aes256Gcm::generate_nonce(&mut OsRng); NONCE_BYTES = 12, continuation_store.rs:17,
; :123). GCM security degrades if a (key, nonce) pair ever repeats. With random nonces
; over a nonce space of size N = 2^96, the probability that q encryptions under one key
; produce a repeat is bounded by the birthday inequality
;
;     Pr[collision] <= C(q,2) / N = q(q-1) / (2 * 2^96).
;
; The store's operational target is Pr[collision] <= 2^-33 at q = 2^32 objects per key
; (four billion objects). Clearing denominators, that target is exactly the integer
; inequality
;
;     q(q-1)/2 <= 2^96 * 2^-33 = 2^63          (at q = 2^32).
;
; This file discharges that arithmetic with BOTH z3 and cvc5 (it is pure integer
; arithmetic over pinned constants, which both solvers evaluate directly):
;
;   1. the bound holds at q = 2^32: its violation is unsatisfiable      -> unsat
;   2. positively, the bound is satisfied at q = 2^32                   -> sat
;   3. the margin is finite — at q = 2^40 the 2^-33 target is exceeded  -> sat
;      (documents that per-key object counts must stay well under 2^40;
;       the store's sharded layout and per-run keys keep q far below 2^32).
;
; Runtime anchor: packages/embers/src/domain/agents_teams/continuation_store.rs:17
;   (NONCE_BYTES = 12), :123 (generate_nonce). Cross-model: overflow_bv.smt2 handles
;   the fixed-width overflow angle; this file is the exact-integer probability angle.
; Validate:  z3 -smt2 nonce_collision_bound.smt2                  (expect: unsat, sat, sat)
;            cvc5 --incremental nonce_collision_bound.smt2        (same)

; QF_NIA: the birthday term q(q-1)/2 is syntactically nonlinear (q*q), though q is
; pinned to a constant in every obligation, so each check reduces to ground evaluation.
(set-logic QF_NIA)
(set-option :produce-models true)

; N = 2^96 (nonce space); target exponent 33; cleared threshold T = 2^96 * 2^-33 = 2^63.
; 2^32 = 4294967296 ; 2^40 = 1099511627776 ; 2^63 = 9223372036854775808.

; ---------------------------------------------------------------------------
; Obligation 1 — the bound holds at the target load q = 2^32 (expect: unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const q Int)
(assert (= q 4294967296))                                  ; q = 2^32
(assert (> (div (* q (- q 1)) 2) 9223372036854775808))     ; suppose q(q-1)/2 > 2^63
(check-sat)   ; EXPECT: unsat  (q(q-1)/2 = 2^63 - 2^31 is NOT > 2^63)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 2 — positively, q(q-1)/2 <= 2^63 at q = 2^32, so Pr <= 2^-33 (expect: sat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const q Int)
(assert (= q 4294967296))                                  ; q = 2^32
(assert (<= (div (* q (- q 1)) 2) 9223372036854775808))    ; the target bound
(check-sat)   ; EXPECT: sat   (the bound is met with room: 2^63 - 2^31 <= 2^63)
(get-model)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 3 — the safety margin is finite: at q = 2^40 the 2^-33 target is
; exceeded (expect: sat), so per-key object counts must be controlled.
; ---------------------------------------------------------------------------
(push 1)
(declare-const q Int)
(assert (= q 1099511627776))                               ; q = 2^40
(assert (> (div (* q (- q 1)) 2) 9223372036854775808))     ; q(q-1)/2 > 2^63
(check-sat)   ; EXPECT: sat   (2^79 - 2^39 > 2^63 — the birthday target no longer holds)
(pop 1)
