# Formal verification of the Embers backend

This directory holds the machine-checked models of the Embers backend's
protocols and kernel semantics. It complements — and never restates — the three
already-validated TLA⁺ specifications in the sibling repository at
`embers-frontend/docs/agent-teams/formal/` (`StatechartScopes.tla`,
`AgentTeamsWorkflow.tla`, `DurableHierarchy.tla`), which cover the durable
workflow kernel at the design level.

## Philosophy and ground rules

- **Every model is a deliberately finite, executable abstraction** that must be
  **validated with its own tool** as part of delivery. A specification that has
  never had its checker run is not done; its verdict is recorded in the table
  below with the exact command and the commit it was last validated at.
- **The bridge to the running code is the invariant registry**, not prose:
  [`INVARIANTS.md`](INVARIANTS.md) gives every guaranteed property a stable id.
  A property-based test asserting it against the real Rust is named
  `fv_inv_<id>_<slug>`; a model checking it abstractly cites the same id. The two
  faces are kept honest by sharing the identifier.
- **Rocq work carries no `Admitted`, no `admit`, and no new `Axiom`** — the
  campaign's standing rule; the runner greps for them and fails if present.
- Three kinds of deliverable: **positive models** (all invariants hold);
  **falsification witnesses** (a deliberately broken variant whose counterexample
  is persisted — following the frontend `RestoreHistoryNode` precedent); and
  **documented expected-fails** (a property the system does *not* promise, whose
  failing check is the honest machine-checked statement of that non-promise —
  e.g. deploy is at-most-once, not exactly-once).

## Where models live

| Home | Contents |
| ---- | -------- |
| `embers-frontend/docs/agent-teams/formal/` (existing, frozen) | Kernel design-level TLA⁺: statechart scopes, cyclic tokens, durable hierarchy. Kernel *extensions* (route groups, join epochs, cancellation, migration, multi-command) are added there as new files. |
| `embers/docs/formal/` (this directory) | Backend transport, consistency, security, and arithmetic models that abstract embers-repo code: `tla/`, `smt/`, `rocq/`, `uppaal/`, `proverif/`, `tamarin/`, `mcrl2/`, `storm/`, `ctl/`. |

## Verdict table

Filled in as each model lands and is validated. `INVARIANTS.md` is the index of
which properties each model discharges.

| Model | Tool | Command | Expected | Status |
| ----- | ---- | ------- | -------- | ------ |
| _(frontend)_ `StatechartScopes.tla` | TLC | `tlc -config StatechartScopes.cfg StatechartScopes.tla` | 231 distinct states, all invariants hold | ✅ validated (frontend) |
| _(frontend)_ `AgentTeamsWorkflow.tla` | TLC | `tlc -config AgentTeamsWorkflow.cfg AgentTeamsWorkflow.tla` | all invariants hold | ✅ validated (frontend) |
| _(frontend)_ `DurableHierarchy.tla` | TLC | `tlc -config DurableHierarchy.cfg DurableHierarchy.tla` | all invariants hold | ✅ validated (frontend) |
| `smt/history_restore_conservation.smt2` | Z3 + cvc5 | `z3 -smt2 history_restore_conservation.smt2` | `unsat sat` (+ pre-fix `sat` witness) | ✅ validated (INV-A1) |
| `smt/quorum_thresholds.smt2` | Z3 + cvc5 | `z3 -smt2 quorum_thresholds.smt2` | `sat unsat` | ✅ validated (INV-A2) |
| `smt/retry_budget.smt2` | Z3 + cvc5 | `z3 -smt2 retry_budget.smt2` | `unsat unsat unsat` | ✅ validated (INV-A3, C5) |
| `smt/overflow_bv.smt2` | Z3 + cvc5 | `z3 -smt2 overflow_bv.smt2` | `unsat sat` | ✅ validated (INV-A4) |
| `smt/capability_encoding.smt2` | cvc5 (z3 incomplete on the word equations) | `cvc5 --incremental capability_encoding.smt2` | `unsat unsat sat` | ✅ validated (INV-S4, S6) |
| `smt/nonce_collision_bound.smt2` | Z3 + cvc5 | `z3 -smt2 nonce_collision_bound.smt2` | `unsat sat sat` | ✅ validated (INV-S6) |
| `ctl/{run,publication,reveal}_lifecycle.lts.json` | pgmcp `behavioral_check` + hermetic Rust re-check | (see file) | formulas hold | ⏳ planned (lifecycles) |
| `tla/ObserverConsistency.tla` | TLC | `tlc -config ObserverConsistency.cfg ObserverConsistency.tla` | invariants + liveness hold | ✅ validated (INV-C1–C3) |
| `tla/DeployOnce.tla` | TLC | `tlc -config DeployOnce.cfg DeployOnce.tla` | pass + retry-witness counterexample | ✅ validated (INV-C4) |
| _frontend_ `RouteGroups.tla`, `JoinEpochs.tla`, `CancelCompensation.tla`, `MigrationPinning.tla`, `CrossScopeTokens.tla`, `DurableMultiCommand.tla`, `DurableRecovery.tla` | TLC (Apalache where state space explodes) | per file | pass | ⏳ planned (INV-K7–K13, D3/D5/D6) |
| `uppaal/timers_recovery.xml` | UPPAAL `verifyta` | `/opt/uppaal/bin/verifyta -u -q timers_recovery.xml queries.q` | all queries satisfied | ⏳ planned (INV-T1–T4) |
| `proverif/two_phase_jwt.pv` | ProVerif | `proverif two_phase_jwt.pv` | binding + secrecy + restart true; replay inj false (documented) | ✅ validated (INV-S1) |
| `proverif/reveal_output.pv` | ProVerif | `proverif reveal_output.pv` | secrecy + single-use (inj) + owner-only true | ✅ validated (INV-S2, S3) |
| `tamarin/run_command_fencing.spthy` | Tamarin | `tamarin-prover --prove run_command_fencing.spthy` | all lemmas verified | ⏳ planned (INV-D4, S7, S8) |
| `tamarin/continuation_capabilities.spthy` | Tamarin | `tamarin-prover --prove continuation_capabilities.spthy` | all lemmas verified | ⏳ planned (INV-S4–S6) |
| `mcrl2/red_team_choreography.mcrl2` | mCRL2 + pgmcp `protocol_soundness` | `mcrl22lps … \| lps2pbes -f nodeadlock.mcf \| pbessolve` | deadlock-free + progress | ⏳ planned |
| `storm/observer_retry.pm` | Storm | `storm --prism observer_retry.pm --prop observer_retry.props` | flake $`= 0.5^{10} \approx 9.77\times10^{-4} < 10^{-3}`$ at pessimistic defaults | ✅ validated (INV-C1–C3 evidence) |
| `rocq/MergeJournal.v`, `ProgressBoundary.v`, `KernelTermination.v` | Rocq | `make -f Makefile.rocq` (no admits) | clean compile | ⏳ planned (INV-D8, K6, K14) |

## Installed tools (verified on this machine)

`tlc`, `apalache-mc`, `z3`, `cvc5`, `coqc` (Rocq 9.1.1 + Iris/stdpp/paco/itree),
`mcrl22lps`/`lps2lts`/`lps2pbes`/`pbessolve`, `proverif`, `tamarin-prover`,
`spin`, `storm`, `maude`, `lean`; UPPAAL's headless `verifyta` at
`/opt/uppaal/bin/verifyta`; TLAPS at `~/.local/tlaps/bin/tlapm`. Heavy Rocq
builds run under a `systemd-run --user --scope` resource cap.

## Running everything

A `cargo make fv` aggregate task (with per-tool `fv-smt`, `fv-tlc`, `fv-uppaal`,
`fv-proverif`, `fv-tamarin`, `fv-mcrl2`, `fv-storm`, `fv-rocq` subtasks) drives
every model and fails loudly on an unexpected verdict; the scripts live under
`scripts/`. TLC metadirs are written under `target/fv/`. Counterexample witnesses
are persisted with pgmcp `crucible_trace_record_counterexample` and referenced by
id in the verdict table.

## Deliberately out of scope

RSpace and Casper consensus internals (finality is taken as an oracle event);
the cryptographic primitives themselves (secp256k1, BLAKE2b, AES-GCM, HMAC — the
security models are Dolev–Yao / ideal-AEAD, standard practice); the Rholang
interpreter's functional semantics (the compiled choreography's *coordination* is
modelled, its evaluation is not); f1r3drive FUSE durability internals (the
`sync_all` contract is assumed, as in `DurableHierarchy`); and the async runtime
(tokio/poem/tonic) as a trust base. Each is justified where it is relied upon.

## Citations

Leslie Lamport, _Specifying Systems_ (2002); Yu, Lamport, Matthews & Tuttle,
"Specifying and Verifying Systems with TLA+," [doi:10.1145/1133373.1133382](https://doi.org/10.1145/1133373.1133382).
De Moura & Bjørner, "Z3: An Efficient SMT Solver," [doi:10.1007/978-3-540-78800-3_24](https://doi.org/10.1007/978-3-540-78800-3_24).
Blanchet, "An Efficient Cryptographic Protocol Verifier" (ProVerif), [doi:10.1109/CSFW.2001.930138](https://doi.org/10.1109/CSFW.2001.930138).
Meier, Schmidt, Cremers & Basin, "The TAMARIN Prover," [doi:10.1007/978-3-642-39799-8_48](https://doi.org/10.1007/978-3-642-39799-8_48).
Bunte et al., "The mCRL2 Toolset," [doi:10.1007/978-3-030-17462-0_2](https://doi.org/10.1007/978-3-030-17462-0_2).
Garcia-Molina & Salem, "Sagas," [doi:10.1145/38713.38742](https://doi.org/10.1145/38713.38742).
