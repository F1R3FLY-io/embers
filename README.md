# embers

The Rust backend for **F1r3sky** — wallets, social publishing, and durable AI
**agent teams** — fronting the F1r3fly stack. Embers is an HTTP service that
compiles requests into Rholang, deploys them to an f1r3node ("Firefly") network,
and reads results back through the observer, all behind a signed two-phase
protocol.

It never hand-writes Rholang (a struct is rendered through a compile-time
templating derive) and never assumes a write is instantly visible (reads that
chase a just-finalized write retry against observer lag).

## Workspace

A Cargo workspace of five crates plus a detached parser oracle:

| Crate | Kind | What it is |
| ----- | ---- | ---------- |
| [`embers`](packages/embers/) | binary | The HTTP API service — six OpenAPI groups, the agent-team kernel, the F1r3drive-backed continuation store. |
| [`firefly-client`](packages/firefly-client/README.md) | library | The f1r3node clients (validator gRPC, observer HTTP, event WebSocket), the retry policy, and the Rholang value/render model. |
| [`firefly-client-macros`](packages/firefly-client-macros/README.md) | proc-macro | `#[derive(IntoValue)]` and `#[derive(Render)]` — Rust types into injection-safe Rholang. |
| [`events-sync`](packages/events-sync/README.md) | binary | The AT Protocol firehose ↔ Firefly bridge (both directions). |
| [`state-sync`](packages/state-sync/README.md) | binary | Checkpoints a PostgreSQL database onto Firefly and restores it. |
| [`oracle`](oracle/) | detached | Parses every rendered template under the real Rholang grammar (own workspace; excluded from the root). |

The dependency graph and the trait seams (`ReadNode` / `WriteNode` /
`NodeEventSource`, which make the whole backend generic over mockable clients) are
in [crate architecture](docs/architecture/crate-architecture.md).

## The API

Six groups, served as one OpenAPI 3 service under `/api` with Swagger UI at
`/swagger-ui/index.html`:

- **service** — liveness/readiness.
- **wallets** — balances, transfers, boosts.
- **agents** — single-agent deploy and invocation.
- **agents_teams** — durable multi-agent workflows (the kernel).
- **oslfs** — Operational-Semantics-in-Logical-Form objects.
- **testnet** — test-network faucet/utilities.

Every mutating call is **two-phase**: a `prepare` returns the exact Rholang to be
deployed plus a JWT binding request to response; a `send` re-verifies that binding
and deploys. The full operation catalog is the
[Web API reference](docs/api/README.md); the protocol and its consistency model
are in [deploy lifecycle and consistency](docs/architecture/deploy-lifecycle-and-consistency.md).

## Build and run

Embers uses `gxhash`, which needs AES/SSE2 target features on x86-64:

```sh
RUSTFLAGS="-C target-feature=+aes,+sse2" cargo make test    # or: cargo build --release
```

The service is configured entirely from the `EMBERS__…` environment (node URLs,
service keys, the AES continuation-store key, and the F1r3drive mount path); the
full matrix and reference configs are in
[configuration](docs/operations/configuration.md). It will not boot without an
[F1r3drive](docs/integrations/f1r3drive.md) FUSE mount for the continuation store.

For a full loopback dev-net (native node + F1r3drive + PDS + embers), use the
sibling `embers-local-stack`; see [local stack](docs/operations/local-stack.md).

## Documentation

[`docs/README.md`](docs/README.md) is the map and glossary. Highlights:

- **Architecture** — [deploy lifecycle & consistency](docs/architecture/deploy-lifecycle-and-consistency.md), [crate architecture](docs/architecture/crate-architecture.md), [continuation store](docs/architecture/continuation-store.md).
- **Design** — [Rholang templating](docs/design/rholang-templating.md).
- **Security** — [security model](docs/security/security-model.md).
- **Operations** — [configuration](docs/operations/configuration.md).
- **Formal** — [verification registry](docs/formal/README.md) and the [invariant catalog](docs/formal/INVARIANTS.md).

## Testing

Tests run in three layers: **L1** is hermetic (every external — node, OpenAI, PDS —
is stubbed on loopback; this is `cargo make test`), **L2** drives a real local
dev-net over loopback with the LLM stubbed locally, and **L3** targets remote
infrastructure and is implemented but gated off by default. All traffic in L1/L2
is loopback-only. Rust suites are example-based and property-based (proptest); the
Python suite (`packages/embers/tests/`) is marker-layered. See
[test architecture](docs/testing/test-architecture.md).
