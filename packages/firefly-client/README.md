# firefly-client

The client library for talking to an f1r3node ("Firefly") node, and the Rholang
value/render machinery the Embers backend builds on. Every embers binary depends
on this crate.

## The three node clients

An f1r3node deployment is addressed as three endpoints, and this crate wraps each
with the resilience policy that endpoint needs. The design rationale — finality
versus visibility, and why the write and read paths make opposite retry choices —
is in [deploy lifecycle and consistency](../../docs/architecture/deploy-lifecycle-and-consistency.md).

- **`WriteNodeClient`** — gRPC `casper.v1.DeployService` against the **validator**:
  submit a deploy, read the head block, read a channel's value. A deploy is
  submitted **exactly once and never retried** (a lost response is ambiguous: the
  deploy may have landed), with a request deadline sized from the measured cold
  path. Deploys are signed with secp256k1 over a BLAKE2b-256 digest
  (`helpers::sign_prepared_contract`), which is deterministic (RFC 6979) so an
  outboxed deploy can be resubmitted byte-identically.
- **`ReadNodeClient`** — HTTP `POST /api/explore-deploy` against the **observer**.
  A read that reads back the effect of a just-finalized write uses the
  **retry-on-empty** helpers (`get_data_with_retry` / `get_data_or_none_with_retry`):
  they retry only on the empty (`ReturnValueMissing`) result, with capped
  exponential backoff and jitter under a total budget that always covers at least
  two request attempts. A plain read performs exactly one attempt — an empty
  answer is the answer.
- **`NodeEvents`** — the validator WebSocket `/ws/events` fan-out: `wait_for_deploy`
  (returning `Some(false)` on a clean finalize, `Some(true)` on an errored deploy,
  `None` on timeout) and per-wallet deploy subscriptions with a replay cache.

`CommunicationService` is the reverse half — a gRPC server implementing the node's
notification interface — used by the `events-sync` binary, not by `embers`.

## The trait seams

`ReadNode`, `WriteNode`, and `NodeEventSource` abstract the three clients. Every
embers domain service and API struct is generic over this triple with the real
clients as defaults, so the whole backend can be driven against mocks in-process
(see [test architecture](../../docs/testing/test-architecture.md)). `NodeEvents`
additionally offers a `#[cfg(test)] new_for_test` constructor that injects a
broadcast channel while reusing the production dispatch loop.

## Models and rendering

- `firefly_client::models` re-exports the f1r3node protobuf types (`casper`,
  `rhoapi`, `servicemodelapi`) plus the domain newtypes: `WalletAddress` and `Uri`
  (base58check / zbase32 codecs with checksums), `DeployId`, `BlockId`,
  `SignedCode`, `DeployData`.
- `firefly_client::rendering` is the Rholang value model (`Value`, `IntoValue`,
  `Render`), re-exporting the derives from `firefly-client-macros`. See
  [Rholang templating](../../docs/design/rholang-templating.md).

## Testing

- Unit and property tests for the codecs, signing, escaping, and retry budget
  (`src/models.rs`, `src/helpers.rs`, `src/rendering.rs`, `src/read_node_client.rs`).
- `read_node_client` HTTP behavior against a `wiremock` server; `node_events`
  against an injected channel.

Run with `cargo test -p firefly-client`.
