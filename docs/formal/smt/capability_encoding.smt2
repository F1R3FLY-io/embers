; INV-S4 / INV-S6 — continuation-store capability & AAD encoding injectivity
; (see ../INVARIANTS.md).
;
; The continuation store derives a per-object capability and an AEAD associated-
; data string by NUL-delimited concatenation of the artifact identity fields:
;
;   capability = HMAC-SHA256(capability_key, kind ‖ 0x00 ‖ run_id ‖ 0x00 ‖ object_id)
;                                            (continuation_store.rs:312, derive_capability)
;   aad        = "embers-agent-teams-v1" ‖ 0x00 ‖ kind ‖ 0x00 ‖ run_id
;                                         ‖ 0x00 ‖ object_id ‖ 0x00 ‖ capability_hash
;                                            (continuation_store.rs:372, aad)
;
; HMAC's own collision resistance and the AEAD's integrity are cryptographic
; assumptions (out of SMT scope — see the Tamarin model continuation_capabilities.spthy).
; What SMT discharges here is the *pre-image* obligation those assumptions rest on:
; the byte string fed to HMAC, and the byte string used as AAD, are INJECTIVE in the
; identity tuple — so two distinct artifacts can never present the same HMAC input or
; the same AAD. Injectivity holds because the 0x00 delimiter is reserved: the identity
; fields are NUL-free (kind is a fixed enum string; run_id/object_id are validated
; identifiers), so the delimiters partition the concatenation unambiguously.
;
; Obligations (all discharged by cvc5):
;   1. capability encoding injective under NUL-free fields                -> unsat
;   2. aad encoding injective under NUL-free fields (5-field, with the
;      constant prefix and trailing capability_hash)                      -> unsat
;   3. NUL-freeness is load-bearing: dropping it exhibits a collision     -> sat
;
; SOLVER NOTE: this model is discharged by cvc5 ONLY. z3's word-equation solver is
; incomplete on the unbounded  str.contains-negation  equations here — it times out
; on obligations 1-2 (empirically: unsat only for field lengths <= ~6, timeout at 16+),
; whereas cvc5's string solver decides the unbounded case. The verify_smt.sh table
; records this model as cvc5-only for exactly this reason; the arithmetic companion
; (nonce_collision_bound.smt2) is cross-checked by BOTH solvers.
;
; Runtime anchors: packages/embers/src/domain/agents_teams/continuation_store.rs:312
;   (derive_capability), :372 (aad), :323 (object_path — the capability is the whole
;   filename, so path injectivity reduces to capability injectivity).
; Validate:  cvc5 --incremental capability_encoding.smt2      (expect: unsat, unsat, sat)

(set-logic ALL)

; The reserved delimiter: a single character (the NUL byte 0x00 in the code). Modeling
; it as an arbitrary single character that the fields avoid is strictly more general
; than pinning it to NUL, so the injectivity result carries over.
(declare-const D String)
(assert (= (str.len D) 1))

; ---------------------------------------------------------------------------
; Obligation 1 — capability encoding  kind ‖ D ‖ run_id ‖ D ‖ object_id  is
; injective in (kind, run_id, object_id) when every field is NUL-free (expect: unsat).
; ---------------------------------------------------------------------------
(push 1)
(declare-const k1 String)(declare-const r1 String)(declare-const o1 String)
(declare-const k2 String)(declare-const r2 String)(declare-const o2 String)

; NUL-free precondition (the fields never contain the reserved delimiter):
(assert (not (str.contains k1 D)))(assert (not (str.contains r1 D)))(assert (not (str.contains o1 D)))
(assert (not (str.contains k2 D)))(assert (not (str.contains r2 D)))(assert (not (str.contains o2 D)))

; Distinct identity tuples...
(assert (or (distinct k1 k2) (distinct r1 r2) (distinct o1 o2)))
; ...but equal encodings — the negation of injectivity.
(assert (= (str.++ k1 D r1 D o1) (str.++ k2 D r2 D o2)))
(check-sat)   ; EXPECT: unsat  (no such collision exists — the encoding is injective)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 2 — aad encoding is injective in (kind, run_id, object_id,
; capability_hash) when every field is NUL-free (expect: unsat). The constant prefix
; P is identical on both sides (it cancels); capability_hash h is the NUL-free trailing
; field. Injectivity in all five fields implies injectivity in the identity triple,
; since capability_hash is itself a function of that triple.
; ---------------------------------------------------------------------------
(push 1)
(declare-const P String)   ; the fixed "embers-agent-teams-v1" prefix (NUL-free)
(assert (not (str.contains P D)))
(declare-const k1 String)(declare-const r1 String)(declare-const o1 String)(declare-const h1 String)
(declare-const k2 String)(declare-const r2 String)(declare-const o2 String)(declare-const h2 String)

(assert (not (str.contains k1 D)))(assert (not (str.contains r1 D)))
(assert (not (str.contains o1 D)))(assert (not (str.contains h1 D)))
(assert (not (str.contains k2 D)))(assert (not (str.contains r2 D)))
(assert (not (str.contains o2 D)))(assert (not (str.contains h2 D)))

(assert (or (distinct k1 k2) (distinct r1 r2) (distinct o1 o2) (distinct h1 h2)))
(assert (= (str.++ P D k1 D r1 D o1 D h1) (str.++ P D k2 D r2 D o2 D h2)))
(check-sat)   ; EXPECT: unsat  (the AAD is injective)
(pop 1)

; ---------------------------------------------------------------------------
; Obligation 3 — NUL-freeness is load-bearing (expect: sat). Drop the NUL-free
; precondition on the run_id/object_id fields and a collision appears: a byte string
; with two interior NULs splits two ways, e.g.
;   (kind, run_id, object_id) = (k, "a",   "b␀c")   and
;   (kind, run_id, object_id) = (k, "a␀b", "c")
; both encode to  k ␀ a ␀ b ␀ c . This is exactly why the store must reject (or the
; caller must guarantee) NUL-free identity fields; the sat witness documents the
; failure mode the precondition forecloses.
; ---------------------------------------------------------------------------
(push 1)
(declare-const k1 String)(declare-const r1 String)(declare-const o1 String)
(declare-const k2 String)(declare-const r2 String)(declare-const o2 String)
; kind stays NUL-free (it is a fixed enum), but run_id/object_id are NOT constrained:
(assert (not (str.contains k1 D)))(assert (not (str.contains k2 D)))
(assert (or (distinct k1 k2) (distinct r1 r2) (distinct o1 o2)))
(assert (= (str.++ k1 D r1 D o1) (str.++ k2 D r2 D o2)))
(check-sat)   ; EXPECT: sat  (a NUL-in-field collision exists)
(pop 1)
