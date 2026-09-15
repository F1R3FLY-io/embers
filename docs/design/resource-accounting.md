# Resource accounting

An agent-team run consumes bounded resources along several independent axes, and
embers validates and enforces a limit on each. This document catalogs those axes,
where each is set and checked, and who pays for what.

Implementations: `packages/embers/src/domain/agents_teams/workflow.rs` (per-node
`NodeResources` and validation), `runtime.rs` (enforcement), and
`continuation_store.rs` (the storage cap).

## The resource axes

| Axis | Type | Bounds what | Set / defaulted | Validated |
| ---- | ---- | ----------- | --------------- | --------- |
| `max_phlo` | per node | The **on-chain Rholang execution budget** (phlo — F1r3fly's metered execution unit) for that node's deploy. | Per-node `NodeResources` | `max_phlo > 0` (workflow.rs:792) |
| `fuel` | per node | The **kernel-level step budget** for that node — how much the durable kernel will spend advancing it, independent of on-chain phlo. | Per-node `NodeResources` | `fuel > 0` (workflow.rs:792) |
| `max_output_bytes` | per node | The **size of a node's output value**. | `node.policy.max_output_bytes` | `> 0` when set (workflow.rs:155); enforced at runtime.rs:244 |
| `max_object_bytes` | store-wide | The **size of a single continuation object** on the F1r3drive store. | `CONTINUATION_STORE__MAX_OBJECT_BYTES`, default **64 MiB** (`DEFAULT_MAX_OBJECT_BYTES`) | `> 0` (continuation_store.rs:68); enforced on put and read |

`max_phlo`, `fuel`, and `max_output_bytes` are the three fields of a node's
`NodeResources`, and the kernel rejects a definition unless **all three are strictly
positive** (workflow.rs:792) — a node with a zero budget on any axis is a
malformed definition, caught at validation rather than stalling at run time.

`max_object_bytes` is separate: it is a property of the
[continuation store](../architecture/continuation-store.md), not of a node. A put
whose plaintext exceeds it is rejected, and a read tolerates only `max_object_bytes
+ 1024` (the AEAD framing overhead), so a corrupt or oversized object cannot be
silently accepted.

## Why the two compute budgets are distinct

`max_phlo` and `fuel` measure different things and must not be conflated:

- **`max_phlo`** is spent **on chain**, by the node's Rholang deploy, and is what the
  validator meters. It is funded by the network **service key** (see below).
- **`fuel`** is spent **in the kernel**, bounding how far embers' durable runtime will
  drive a single node — the backstop that turns a runaway kernel loop into a bounded
  error rather than divergence (the termination guarantee formalized in
  [KernelTermination](../formal/rocq/ProgressBoundary.v) and the kernel property
  tests). A node can exhaust `fuel` without ever exhausting `max_phlo`, and vice
  versa.

## Who funds what

| Cost | Paid by |
| ---- | ------- |
| Every deploy embers submits (init contracts, run steps, node executions) — its phlo | The network **service key** (`EMBERS__{MAINNET,TESTNET}__SERVICE_KEY`), a funded wallet. See [key inventory](../security/key-inventory.md). |
| A **boost** (a social value transfer attributing an amount to a F1r3sky post) | The **sending wallet**, as the transferred value — a first-class amount, not an infrastructure cost. |
| Continuation-store space | Local F1r3drive storage, bounded per object by `max_object_bytes`. |

## Latency budget

The other bounded resource is time on the read-back path: a read that chases a
just-finalized write retries under a total budget (`TOTAL_READ_BUDGET = 150 s`,
required to cover at least two request attempts) with capped exponential backoff.
That budget and its schedule math are documented in
[deploy lifecycle and consistency](../architecture/deploy-lifecycle-and-consistency.md)
and quantified probabilistically by the
[observer-retry Storm model](../formal/storm/observer_retry.pm).
