# Shared invariant registry

This registry is the bridge between three things that must never drift apart:
the **prose** describing what the Embers backend guarantees, the **executable
property-based tests** that check those guarantees against the running Rust code,
and the **formal models** (TLA⁺, SMT, Rocq, UPPAAL, ProVerif/Tamarin, mCRL2,
Storm) that check them abstractly. Every invariant has a stable identifier. A
test that checks it is named `fv_inv_<id>_<slug>`; a formal model that checks it
cites the same id. Grep the id and you find all three faces of one property.

> For the current validated/landed status of each formal home and the invariant →
> model → tool traceability, see [verification-targets.md](verification-targets.md)
> and the [verdict table](README.md#verdict-table).

## How to read a row

| Column | Meaning |
| ------ | ------- |
| **ID** | Stable identifier. `K` = kernel, `D` = durable protocol, `T` = timers, `C` = consistency/transport, `S` = security, `A` = arithmetic. |
| **Statement** | The property, precisely. Mathematics is written as GitHub-safe MathJax. |
| **Runtime anchor** | The `file:line` region in the Rust implementation the property is about. |
| **Proptest mirror** | The `fv_inv_*` test asserting it against the real code (— if not yet written). |
| **Formal home** | The model that checks it abstractly (— if not yet built; the frontend TLA⁺ set is at `embers-frontend/docs/agent-teams/formal/`). |

Symbols used below, defined once. $`\alpha(s)`$ is the kernel's stored live-token
count for scope $`s`$ (`active_tokens`). $`\mathrm{live}(s)`$ is the ground-truth
set of live tokens whose innermost scope is $`s`$. A **quiescent point** is any
state the kernel returns to the caller in — i.e. after a `KernelOutcome` (an
effect batch, a guard batch, a suspension, or a terminal); the microstep loop
inside `run_until_blocked` is not observable without instrumenting the kernel, so
token-level invariants are asserted at quiescent points.

## Kernel invariants (INV-K)

Home for the abstract models: the frozen frontend specs `StatechartScopes.tla`
(K1–K4), `AgentTeamsWorkflow.tla` (K5–K6), and the planned extensions
`RouteGroups.tla` (K7–K8), `JoinEpochs.tla` (K9–K11), `CancelCompensation.tla`
(K12), `MigrationPinning.tla` (K13). Runtime anchors are into
`packages/embers/src/domain/agents_teams/runtime.rs`.

| ID | Statement | Runtime anchor | Proptest mirror | Formal home |
| -- | --------- | -------------- | --------------- | ----------- |
| INV-K1 | Token conservation (`CounterFaithful`): at every quiescent point, for every live scope instance $`s`$, $`\alpha(s) = \lvert \mathrm{live}(s) \rvert`$ — the stored counter equals the recounted live population, across seeding, fan-out, consumption, and history restore. | `runtime.rs` enter/consume/fan-out (`:1020-1142`, `:1448-1457`) | `fv_inv_k1_token_conservation` | `StatechartScopes.tla` + planned `CrossScopeTokens.tla` (global sum) |
| INV-K2 | Emptiness is completion: a running scope is empty **iff** it completes at that instant. | `runtime.rs:692-709, 1110-1142` | (covered within K1 driver) | `StatechartScopes.tla` |
| INV-K3 | Entry/exit ordering: an entry activity strictly precedes any token in the scope; an exit activity runs only on a drained scope. | `runtime.rs:624-656` | (K1 driver, scoped generators) | `StatechartScopes.tla` |
| INV-K4 | History correctness: history exists only after a completion captured it, carries the scope's declared mode, and is never captured by a `none`-mode scope. | `runtime.rs:1052-1082` | (K1 driver) | `StatechartScopes.tla` |
| INV-K5 | Bounded counters: each token's per-transition traversal count never exceeds the declared bound. | `runtime.rs` `iterations` map | `fv_inv_k14_bounded_cycles_exhaust` | `AgentTeamsWorkflow.tla` |
| INV-K6 | Hot-segment safety: a hot segment (no wait/bound/yield crossing) is bounded by the residual-graph node count. | `workflow.rs` Tarjan check | `fv_inv_w4_hot_cycle_rejected_boundary_accepted` | ✅ Rocq `ProgressBoundary.v`; `AgentTeamsWorkflow.tla`; proof in frontend `operational-semantics.md` |
| INV-K7 | First-match exclusivity: at most one edge fires per first-match activation, the highest-priority true one. | `workflow.rs:416, 1200-1202` | (planned `fv_inv_k7_*`) | planned `RouteGroups.tla` |
| INV-K8 | All-match completeness: every true edge fires exactly once; the `noMatch` disposition iff no edge is true. | `workflow.rs:1200-1202` | (planned `fv_inv_k8_*`) | planned `RouteGroups.tla` |
| INV-K9 | Join epoch isolation: a join fires only from arrivals of a single correlation epoch. | `runtime.rs:1156-1264, 1368-1393` | `fv_inv_k11_*` (epoch-scoped) | planned `JoinEpochs.tla` |
| INV-K10 | Duplicate-arrival rejection: a second arrival with the same producer and epoch is rejected. | `runtime.rs:1189` | (existing example tests; see caveat) | planned `JoinEpochs.tla` |
| INV-K11 | Quorum fires at k: a `quorum(k)` join fires exactly when the k-th distinct decision arrives and never re-fires; `race` closes on the first arrival and applies its declared late policy; `all`/`each`/`reduce` complete correctly. | `runtime.rs:1156-1264` | `fv_inv_k11_quorum_fires_at_k`, `fv_inv_k11_race_first_arrival_wins`, `fv_inv_k11_all_each_reduce_complete` | planned `JoinEpochs.tla` |
| INV-K12 | Compensation LIFO: after cancellation only compensations dispatch, in reverse completion order, each at most once. | `runtime.rs:476-481, 1599` | (planned `fv_inv_k12_*`) | planned `CancelCompensation.tla` |
| INV-K13 | Migration hash-fencing: `Migrate(from, to)` applies iff the current definition hash equals `from`; afterward every step reads `to`. | `runtime.rs:490-507` | (planned `fv_inv_k13_*`) | planned `MigrationPinning.tla` |
| INV-K14 | Termination / step budget: a valid definition's run either completes or stops at the $`10^6`$-microstep budget error — it never diverges and never panics. | `runtime.rs:721-733` | `fv_inv_k14_no_divergence`, `fv_inv_k14_step_budget_stops_hot_loops` | ✅ Rocq `KernelTermination.v` (hot-segment bound) |

## Durable-protocol invariants (INV-D)

Home: the frozen frontend `DurableHierarchy.tla` (D1–D7) and the planned
`DurableRecovery.tla` (crash/idempotency). Runtime anchors into
`runtime.rs`, `orchestration.rs`, `run_commands.rs`, `get_run.rs`.

| ID | Statement | Runtime anchor | Proptest mirror | Formal home |
| -- | --------- | -------------- | --------------- | ----------- |
| INV-D1 | The public ledger revision never points to an artifact whose durable checkpoint does not exist. | `continuation_store.rs:89-173` | (A2 store props) | `DurableHierarchy.tla` |
| INV-D2 | Each advance increments the revision by exactly one and clears the claimed command. | `orchestration.rs` advance | — | `DurableHierarchy.tla` |
| INV-D3 | At most one command is claimed per run; a claim requires an empty slot and a status in `{running, suspended}`. | `run_commands.rs:319-335` | — | planned `DurableMultiCommand.tla` |
| INV-D4 | Revision fencing: a claimed or applied command is fenced to the ledger revision it claimed. | `run_commands.rs:218-241` | — | `DurableHierarchy.tla` |
| INV-D5 | Apply-exactly-once per effect key: the observation store is a function `effect_key ↦ observation`; a re-observed key applies nothing new. | `orchestration.rs:728-737` | (planned `fv_inv_d5_*`) | planned `DurableRecovery.tla` |
| INV-D6 | Dispatch-at-least-once: an outboxed effect is eventually dispatched under fair scheduling. | `orchestration.rs` outbox | — | planned `DurableRecovery.tla` |
| INV-D7 | `runStarted` is the first public event of any run. | ledger | (K1 driver observes ordering) | `DurableHierarchy.tla` |
| INV-D8 | Canonical event order: the committed journal ordering is a deterministic function of the inputs; both the durable journal and the RSpace sequence are subsequences of the merge. | `get_run.rs:185-211` | `fv_inv_d8_event_order_deterministic`, `fv_inv_replay_deterministic` | planned Rocq `MergeJournal.v` |

## Timer invariants (INV-T)

Home: planned UPPAAL `timers_recovery.xml`. Runtime anchors into `runtime.rs`
(guards) and `recovery.rs` (scheduler).

| ID | Statement | Runtime anchor | Proptest mirror | Formal home |
| -- | --------- | -------------- | --------------- | ----------- |
| INV-T1 | No early fire: a timer resume requires $`\text{now} \ge \text{deadline}`$. | `runtime.rs:420-421` | (planned `fv_inv_t1_*`) | planned UPPAAL |
| INV-T2 | Deadline exact match: a `TimerFired` whose expected deadline differs from the wait's `due_at` is rejected. | `runtime.rs:420` | (planned `fv_inv_t2_*`) | planned UPPAAL |
| INV-T3 | No lost timer: a due, suspended run has a resume command prepared within one poll period plus a bounded processing delay. | `recovery.rs:27-63, 134-176` | (planned integration test) | planned UPPAAL |
| INV-T4 | Late decision held: a timed-out approval routes a `timedOut` disposition to a wait, and a late decision is delivered there rather than lost. | `runtime.rs:422-426` | (planned) | planned UPPAAL |

## Consistency / transport invariants (INV-C)

Home: planned TLA⁺ `ObserverConsistency.tla`, `DeployOnce.tla`, Storm
`observer_retry.pm`. Runtime anchors into
`packages/firefly-client/src/{read_node_client,write_node_client}.rs`.

| ID | Statement | Runtime anchor | Proptest mirror | Formal home |
| -- | --------- | -------------- | --------------- | ----------- |
| INV-C1 | Retry only on empty: a read retry fires only on the `ReturnValueMissing` (empty) result, never on another error. | `read_node_client.rs:197` | `retry-budget` prop (A2) | planned `ObserverConsistency.tla` |
| INV-C2 | No premature `None`: a read returns `Ok(None)` only after the full retry budget is spent. | `read_node_client.rs:217-234` | (A2 retry prop) | planned `ObserverConsistency.tla` |
| INV-C3 | Plain GET no retry: a non-retry read path performs exactly one attempt. | `read_node_client.rs:160-168` | (A2 retry prop) | planned `ObserverConsistency.tla` |
| INV-C4 | Deploy at-most-once: no ambiguous write is ever resubmitted. | `write_node_client.rs:179-180` | (A4 harness pins single deploy) | planned `DeployOnce.tla` |
| INV-C5 | Budget fits two attempts: $`\text{TOTAL\_READ\_BUDGET} \ge 2 \times \text{request\_timeout}`$ (150 s ≥ 2 × 60 s today). | `read_node_client.rs:30, 658` | existing budget test | planned SMT `retry_budget.smt2` |

## Security invariants (INV-S)

Home: planned ProVerif (`two_phase_jwt.pv`, `reveal_output.pv`) and Tamarin
(`run_command_fencing.spthy`, `continuation_capabilities.spthy`). Runtime anchors
into `api/common.rs`, `reveal_output.rs`, `continuation_store.rs`, `runtime.rs`.

| ID | Statement | Runtime anchor | Proptest mirror | Formal home |
| -- | --------- | -------------- | --------------- | ----------- |
| INV-S1 | Send bound to prepare: an accepted send corresponds to a prior prepare with a matching pair-hash and an unexpired token. **See finding `embers-jwt-binding-noncrypto-hash`: the binding hash is a 64-bit non-cryptographic `DefaultHasher`; the model must idealize it and recommend blake2b-256.** | `api/common.rs:498-611` | `fv`-adjacent A4 tests (`tampered_*`, `expired_*`, `token_minted_by_another_endpoint_*`) | planned ProVerif |
| INV-S2 | Reveal single-use: a disclosure implies a fresh `outputRevealed` event; a replay hits the `Consumed` state. | `reveal_output.rs:150-202` | existing reveal tests | planned ProVerif |
| INV-S3 | Fail-closed on missing ack: no positive on-chain acknowledgement means no disclosure. | `reveal_output.rs:207-222` | existing reveal tests | planned ProVerif |
| INV-S4 | Capability unforgeable: no valid capability path exists without the capability key (HMAC-SHA256). | `continuation_store.rs:312-321` | `a_different_capability_key_cannot_address_the_object` (A2) | planned Tamarin |
| INV-S5 | Identity-reuse rejected: the same identity with different plaintext is rejected, never overwritten. | `continuation_store.rs:112-119, 152-161` | `reusing_an_identity_with_different_bytes_is_rejected` (A2) | planned Tamarin |
| INV-S6 | Tamper detected: any content-hash or AEAD/AAD mismatch is rejected on read. | `continuation_store.rs:191-204, 287-310` | `any_single_byte_of_tampering_is_detected` (A2) | planned Tamarin |
| INV-S7 | Approver authorization: a non-listed actor's `Approve`/`Reject` is rejected. | `runtime.rs:439-442, 470-474` | (K11 driver approvers) | planned Tamarin |
| INV-S8 | Duplicate decision rejected: a reused decision id and an approver re-vote are both rejected. | `runtime.rs:443-456` | (K11 driver) | planned Tamarin |

## Arithmetic invariants (INV-A)

Home: planned SMT (`z3` + `cvc5`). All are discharged as `unsat` of the negation.

| ID | Statement | Runtime anchor | Proptest mirror | Formal home |
| -- | --------- | -------------- | --------------- | ----------- |
| INV-A1 | History-restore conservation: the `seed`/`fan-out`/`consume`/`restore` arithmetic preserves $`\alpha = \lvert\mathrm{live}\rvert`$; the pre-fix `restore` (no counter adjustment) is `sat` (the 63906a4 bug). | `runtime.rs:1052-1082` | `fv_inv_k1_token_conservation` | planned `history_restore_conservation.smt2` |
| INV-A2 | Quorum thresholds: $`1 \le k \le n`$ implies the monotone decision counter crosses $`k`$ exactly once; $`k > n`$ is unfireable. | `workflow.rs:245` | `fv_inv_k11_quorum_fires_at_k` | planned `quorum_thresholds.smt2` |
| INV-A3 | Retry-schedule cap: $`\sum_{i=1}^{n} \min(m \cdot i, \text{CAP}) \le n \cdot \text{CAP}`$. | `workflow.rs:893-909` | `fv_inv_w5_retry_delay_deterministic_and_bounded` | planned `retry_budget.smt2` |
| INV-A4 | No overflow: the widened `multiplier × attempt` product and the step-budget arithmetic never wrap. | `runtime.rs:723` | (K14 tests) | planned `overflow_bv.smt2` (bitvector) |

## Status

The proptest mirrors named without a "(planned)" qualifier exist and pass today
(campaign steps A2 and A5). The formal homes marked "planned" are the Workstream
B deliverables; each will be validated with its tool and its verdict recorded in
[README.md](README.md). The three frozen frontend specs
(`StatechartScopes.tla`, `AgentTeamsWorkflow.tla`, `DurableHierarchy.tla`) are
already validated — see `embers-frontend/docs/agent-teams/formal/`.
