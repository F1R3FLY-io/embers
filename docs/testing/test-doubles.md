# Test doubles — fidelity ledger

The [test architecture](test-architecture.md) runs against loopback doubles that
speak the real services' wire contracts. A double is only trustworthy if you know
where it stops mirroring reality, so this is the fidelity ledger: for each double,
what it faithfully reproduces and where it deliberately diverges. A test that depends
on a divergent behavior belongs at a higher layer (L2/L3), not against the double.

## Trait mocks (`domain/test_helpers.rs`)

- **Mirrors:** the `ReadNode`/`WriteNode`/`NodeEventSource` trait contracts; the
  retry-on-empty read semantics via empty-before/values-before sequencing; finality
  events.
- **Diverges:** responses are substring-matched canned JSON, not evaluated Rholang;
  no cost accounting, no DAG, no signature verification; delays are zero.

## Fake node (`packages/firefly-test-node`)

- **Mirrors:** the validator gRPC deploy service, the observer `explore-deploy` HTTP
  envelope (`{expr, block}`) with scriptable empty→populated lag, and the WebSocket
  event stream (`started`, injectable kebab-case `block-finalised` with
  `deploys[{id,cost,deployer,errored}]`). Runs on `127.0.0.1:0`, so `firefly-client`
  is exercised against a real socket and codec.
- **Diverges:** it does **not** evaluate Rholang, compute real phlo costs, build a
  block DAG, or verify signatures; channel reads are matched, not executed.

## OpenAI stub

- **Mirrors:** the four OpenAI endpoints the node calls (`/v1/models`,
  `/v1/chat/completions`, `/v1/images/generations`, `/v1/audio/speech`) at the
  contract level — contract-tested with the node's own `openai-api-rs` client version
  so responses deserialize (fidelity by construction). Image URLs and audio are served
  from loopback. The node reaches it via `OPENAI_API_BASE`; embers never calls OpenAI.
- **Diverges:** chat content is a deterministic echo, not a model completion; no rate
  limits (429s), no auth, no streaming; latency is ~0.

## Fake PDS

- **Mirrors:** the ATProto XRPC calls the F1r3sky publish path makes (create session,
  app-passwords, `uploadBlob`, `putRecord`) plus a served blob asset, on loopback.
- **Diverges:** no DID resolution, no lexicon validation, no firehose; sessions are
  immortal.

## JWT replay (accepted by design)

Not a double but a pinned behavior: the two-phase `send` does **not** consume its
token, so a replayed envelope is accepted at the API layer (the chain is the
anti-replay authority). This is asserted as intended behavior, and machine-checked in
[two_phase_jwt.pv](../formal/proverif/two_phase_jwt.pv) (the injective correspondence
is deliberately false). Contrast the single-use reveal protocol
([reveal_output.pv](../formal/proverif/reveal_output.pv)), whose injective
correspondence holds.

## Why these divergences are safe

Everything a double omits — Rholang evaluation, real costs, consensus, DID resolution,
model semantics, latency, rate limits — is exercised at **L2** against the real
[local stack](../operations/local-stack.md) node + F1r3drive + PDS (with only the LLM
stubbed), and at **L3** against real infrastructure. The doubles exist to make L1 fast
and hermetic, not to replace those layers; the ledger above is the map of exactly
which properties each layer is responsible for.
