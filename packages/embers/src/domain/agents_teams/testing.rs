//! Property-based testing surface for the durable Agent-Teams workflow kernel.
//!
//! Executable mirrors of the invariants the frontend TLA⁺ models check
//! (`embers-frontend/docs/agent-teams/formal/`), bound to the real kernel in
//! `runtime.rs` rather than to a model of it:
//!
//! - `generators` — shrink-friendly proptest strategies that construct valid
//!   [`super::workflow::WorkflowDefinition`] documents (linear, branching with
//!   every join policy, bounded/durable cycles, scoped hierarchies) plus the
//!   deliberately invalid hot cycles the Tarjan check must reject, and the
//!   command scripts that drive them.
//! - `kernel_model` — the invariant checkers themselves: token conservation
//!   (INV-K1 `CounterFaithful`), termination under the step budget (INV-K14),
//!   join-policy semantics (INV-K11), deterministic event journals (INV-D8),
//!   and byte-identical replay, each expressed as a function over the real
//!   [`super::runtime::ContinuationEnvelope`].
//! - `kernel_props` — the `proptest!` blocks binding generators to checkers.
//! - `workflow_props` — canonicalization, round-trip, hash-stability,
//!   hot-cycle rejection, and retry-delay determinism over `workflow.rs`.
//! - `graphl_props` — grammar-directed `GraphL` round-trip and mutation-totality
//!   properties (never raw bytes: the parser is a global-state C FFI).
//! - `render_corpus` — renders every Rholang template in the crate with
//!   representative fixtures into `target/rendered-corpus/` for the
//!   Rholang-parser oracle step.
//! - `fake_pds` — a loopback, in-process fake `ATProto` PDS answering the exact
//!   XRPC the `publish_to_firesky` / `unpublish` clients call, so those flows can
//!   be driven end to end offline.

pub(crate) mod fake_pds;
pub(crate) mod generators;
pub(crate) mod kernel_model;

mod graphl_props;
mod kernel_props;
mod render_corpus;
mod workflow_props;
