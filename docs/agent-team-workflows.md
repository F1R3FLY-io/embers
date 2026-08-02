# Agent Teams Workflow v2 backend

The canonical cross-repository documentation is [Agent Teams Workflow v2](../../embers-frontend/docs/agent-teams/README.md). It includes the portable schema, editor/operator guide, runtime architecture, operational semantics and proofs, threat model, TLA+ abstraction, diagrams, roadmap, and [reproducible conformance matrix](../../embers-frontend/docs/agent-teams/engineering/conformance-and-testing.md).

## Backend boundary

The Embers backend:

- accepts an additive typed `definition` while preserving legacy GraphL records;
- applies permissive draft and strict publish/deploy validation;
- resolves immutable agent versions before compilation;
- lowers nodes and routers to persistent Rholang contracts so cyclic sends can revisit them;
- implements deterministic first-match/all-match routes, pure guards, exact hot-cycle validation, bounded counters, and correlated `all`, `each`, `quorum`, `race`, and `reduce` joins;
- executes compound/parallel scopes, entry/exit activities, completion propagation, shallow/deep history, pinned child workflows, retry, timeout, cancellation, and reverse-order Saga compensation;
- stores encrypted requests, commands, continuations, effect outboxes, observations, and outputs through an owner-isolated F1r3drive mount;
- keeps a serialized RSpace run register containing only public identities, hashes, revisions, status, qualified waits, deadlines, approval grants, and event types;
- authenticates `signal`, `event`, `timerFired`, `approve`, `reject`, `cancel`, `retry`, and `migrate` commands against an exact revision and wait;
- retries recovery from pinned encrypted checkpoints, recursively schedules nested timers, and acknowledges exact post-finality ledger state before dependent work;
- returns a UUIDv7 run capability from prepare and serves its metadata-only inspector endpoint;
- caps F1r3sky terminal replies at 280 characters while retaining the run URL.

The executable boundary requires no f1r3node or MeTTaIL modification. Foreign Language Term activities are represented by a pinned portable ABI but remain rejected until the Rholang 1.4 specification and the under-review FLT/virtual-host-bridge FIPS stabilize. Any additional protected-runtime dependency must be proposed through `../FIPS/` before implementation.

## Finality and transport policy

Consensus finality and observer visibility are checked separately. A mutation deploy is submitted once, awaited through finality, and then verified by polling for its exact postcondition. Named Agent Team deployment returns the final system-metadata deploy identity so client synchronization includes URI and `lastDeploy` publication rather than only the executable contract.

`EMBERS__FINALIZATION__DEPLOY_TIMEOUT_MILLIS` is a positive, validated shared budget for implicit deploy-finalization waits. It defaults to 120,000 milliseconds; explicit activity timeouts remain authoritative. The gRPC write client retains a hard 30-second per-request deadline and TCP keepalive. Its read-only head-block lookup uses at most three attempts with a 250-millisecond base delay, while deploy submissions are never retried because a lost response leaves their mutation outcome ambiguous.

The synchronous live-test client uses a 180-second outer request budget. This preserves headroom for response transport and exact observer-state polling after the server's finalization wait.

## Canonical event journal

Every durable checkpoint carries the complete ordered public run journal inside its encrypted F1r3drive continuation. The corresponding RSpace run register remains deliberately bounded: it publishes the checkpoint event and content/capability hashes, not a continually growing event array. Inspector reads verify both hashes, decrypt the exact `revision-N` continuation, validate its run/team/definition identity, and project the journal with contiguous sequence numbers.

Command claims and rejections can occur between checkpoints, so the read projection preserves any additional coordination occurrences present in the RSpace register. A terminal status also contributes its terminal event when a legacy or interrupted checkpoint does not already contain it. This makes approvals, rejections, retries, scope transitions, and terminal outcomes observable without leaking private payloads into RSpace or embedding an unbounded history in deploy source. Missing, tampered, or identity-mismatched durable artifacts fail the inspector read; they are never silently replaced by the bounded ledger view.

## Key modules

| Concern                                                   | Location                                                                    |
| --------------------------------------------------------- | --------------------------------------------------------------------------- |
| Typed definition and publish validation                   | `packages/embers/src/domain/agents_teams/workflow.rs`                       |
| Direct Rholang workflow compiler                          | `packages/embers/src/domain/agents_teams/compilation/workflow_rendering.rs` |
| Deterministic durable kernel                              | `packages/embers/src/domain/agents_teams/runtime.rs`                        |
| Effects, guards, nested teams, and checkpoint publication | `packages/embers/src/domain/agents_teams/orchestration.rs`                  |
| Encrypted F1r3drive artifact boundary                     | `packages/embers/src/domain/agents_teams/continuation_store.rs`             |
| Signed run-command protocol                               | `packages/embers/src/domain/agents_teams/run_commands.rs`                   |
| Crash/timer recovery                                      | `packages/embers/src/domain/agents_teams/recovery.rs`                       |
| Deployed workflow manifest and activity contracts         | `packages/embers/templates/agents_teams/deploy_workflow.rho`                |
| Serialized metadata ledger and authorization              | `packages/embers/templates/agents_teams/init.rho`                           |
| Verified canonical event-journal projection              | `packages/embers/src/domain/agents_teams/get_run.rs`                        |
| Bounded social reply                                      | `packages/embers/src/domain/agents_teams/run_on_firesky.rs`                 |

The cross-repository documentation includes colored, reproducible PlantUML/SVG diagrams for the system boundary, run protocol, token semantics, cycle proof, authority boundaries, durable commit, and hierarchical continuations. The executable TLA+ models under `../../embers-frontend/docs/agent-teams/formal/` check cyclic progress and durable root/child invariants.
