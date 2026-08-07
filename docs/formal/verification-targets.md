# Verification targets — traceability

This is the map from the [invariant registry](INVARIANTS.md) (what must hold, with
runtime anchors and proptest mirrors) to the formal artifacts that check each
property, and the tool each uses. The per-model run commands and pass/fail verdicts
are in the [verdict table](README.md#verdict-table); the precise statements and
`file:line` anchors are in [INVARIANTS.md](INVARIANTS.md). This document is the
coverage overview and the invariant → model traceability, plus the rationale for the
multi-tool split.

## Why eight tools

Each property class is checked with the tool whose logic fits it, rather than forcing
everything into one formalism:

| Tool | Fits | Backend models |
| ---- | ---- | -------------- |
| **SMT** (z3 + cvc5) | arithmetic, encodings, bounds — decided, cross-checked by two solvers | history-restore conservation, quorum thresholds, retry budget, overflow, capability/AAD encoding injectivity, nonce birthday bound |
| **TLA⁺** (TLC) | concurrency, consistency, timed safety+liveness over small models | DeployOnce, ObserverConsistency, TimersRecovery |
| **ProVerif** | symbolic protocol authentication/secrecy (Horn-clause) | two-phase JWT, single-use reveal |
| **Tamarin** | linear-fact protocol state (CAS, unforgeability) | run-command fencing, continuation capabilities |
| **Storm** | quantitative/probabilistic reliability | observer read-after-write flake bound |
| **mCRL2** | process-algebra deadlock/progress of a choreography | red-team broadcast/fan-in |
| **Rocq** | deep, unbounded, machine-checked proof (no model bound) | journal-merge subsequences, progress-boundary characterization, kernel termination |
| **explicit-state** (checker) | finite lifecycle FSMs, dependency-free re-check | run / publication / reveal lifecycles |

The overlaps are deliberate: the continuation-store capability property, for example,
is checked at the *encoding* level by SMT (injectivity of the HMAC/AAD pre-image) and
at the *protocol* level by Tamarin (unforgeability under Dolev-Yao) — two independent
angles on INV-S4–S6.

## Invariant family → landed artifact

| Family | Invariants | Validated by (this host) | Tool |
| ------ | ---------- | ------------------------ | ---- |
| Consistency / transport | INV-C1, C2, C3 | `tla/ObserverConsistency.tla` (+ flake witness), `storm/observer_retry.pm` | TLC, Storm |
| Consistency (deploy-once) | INV-C4, C5 | `tla/DeployOnce.tla` (+ retry witness), `smt/retry_budget.smt2` | TLC, SMT |
| Timers / recovery | INV-T1, T3, T4 | `tla/TimersRecovery.tla` (UPPAAL substitute — see below) | TLC |
| Security (two-phase) | INV-S1 | `proverif/two_phase_jwt.pv` | ProVerif |
| Security (reveal) | INV-S2, S3 | `proverif/reveal_output.pv` | ProVerif |
| Security (store capabilities) | INV-S4, S5, S6 | `smt/capability_encoding.smt2`, `tamarin/continuation_capabilities.spthy` | SMT, Tamarin |
| Durable fencing | INV-D4, S7, S8 | `tamarin/run_command_fencing.spthy` | Tamarin |
| Durable merge | INV-D8 | `rocq/MergeJournal.v` | Rocq |
| Kernel termination | INV-K6, K14 | `rocq/ProgressBoundary.v`, `rocq/KernelTermination.v` | Rocq |
| Arithmetic | INV-A1, A2, A3, A4 | `smt/{history_restore_conservation,quorum_thresholds,retry_budget,overflow_bv}.smt2` | SMT |
| Lifecycles | run / publication / reveal | `ctl/{run,publication,reveal}_lifecycle.lts.json` | explicit-state |

Everything above is green under `cargo make fv`.

## The spec ↔ test bridge

The lifecycle FSMs (`ctl/*.lts.json`) are the same models the A5 lifecycle property
tests mirror, and the SMT/TLA⁺/Rocq invariants are named to match the `fv_inv_*`
proptest mirrors in [INVARIANTS.md](INVARIANTS.md) — so each property is checked both
abstractly (a model) and against the real code (a property test). The registry's
"Proptest mirror" and "Formal home" columns are the contract between the two.

## Pending and out of scope

- **Pending (frontend repo)**: the kernel design-level TLA⁺ modules — `CrossScopeTokens`,
  `RouteGroups`, `JoinEpochs`, `DurableMultiCommand`, `CancelCompensation`,
  `MigrationPinning`, `DurableRecovery` — belong in
  `embers-frontend/docs/agent-teams/formal/` beside the frozen validated specs, and
  land as a coordinated PR (the frontend agent has been notified). INV-K7–K13 and
  D3/D5/D6 are covered by proptest mirrors here today and gain their abstract model
  there.
- **Tool limitation**: UPPAAL's `verifyta` is not licensed on this host, so the
  timed-automaton `uppaal/timers_recovery.xml` is a reference; the identical
  invariants are machine-checked by `tla/TimersRecovery.tla` under TLC.
- **Out of scope**: consensus/RSpace internals, crypto-primitive mathematics, the
  Rholang interpreter, and FUSE durability internals — enumerated with rationale in
  the [README](README.md#deliberately-out-of-scope).
