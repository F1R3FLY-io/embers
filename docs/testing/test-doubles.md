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

- **Mirrors:** the validator gRPC service — reusing the **exact** generated
  `casper.v1.DeployService` trait (`do_deploy`, `show_main_chain`, `get_data_at_name`
  are behavioral) — the observer `explore-deploy` HTTP endpoint (which accepts the
  client's `{"term": …}` JSON and records the term) with scriptable empty→populated
  lag, and the WebSocket event stream (`started`, injectable kebab-case
  `block-finalised` with `deploys[{id,cost,deployer,errored}]`). Runs on
  `127.0.0.1:0`, so `firefly-client` is exercised against a real socket and codec;
  `EventControl` exposes deterministic `wait_for_connections` so tests need no sleeps.
- **Diverges:** it does **not** evaluate Rholang, compute real phlo/cost, build a
  block DAG, or verify signatures/`valid_after_block_number`. Matching is structural,
  not semantic — channel reads are keyed by **exact channel name** and explore
  responses are selected by **call position** (`responses[min(i, len-1)]`), not by
  parsing the submitted term (this is the network-level counterpart to the embers
  `MockReadNode`'s substring matching). Every gRPC RPC outside the three behavioral
  ones returns a truthful `Status::unimplemented`. `show_main_chain` returns a single
  depth-1 canned block.

## OpenAI stub (`packages/openai-stub`)

- **Mirrors:** the four OpenAI endpoints the node calls (`/v1/models`,
  `/v1/chat/completions`, `/v1/images/generations`, `/v1/audio/speech`) at the
  contract level — **contract-tested with the node's own `openai-api-rs` 6.0.13** so
  every response deserializes through that client (fidelity by construction). Image
  URLs and audio (a first-principles valid PNG/WAV) are served from loopback. A lib
  (`OpenAiStub::start()` on `127.0.0.1:0`, `base_url()` = the `/v1`-suffixed
  `OPENAI_API_BASE` value) and a bin (`openai-stub`, `OPENAI_STUB_ADDR` default
  `127.0.0.1:8571`); `start_on` refuses a non-loopback bind. The node reaches it via
  `OPENAI_API_BASE`; embers never calls OpenAI.
- **Diverges:** no model actually runs — chat content is a deterministic echo, image
  and audio are canned/derived; `Authorization` is accepted but ignored (no auth); no
  429/quota/error statuses; no streaming/SSE; `usage` is always zero; `list_models`
  returns only `id/object/created/owned_by`. `GET /models` is nonetheless required —
  the node's startup validation calls it and cannot be disabled by env (a documented
  f1r3node quirk).

## Fake PDS (embers `#[cfg(test)] testing/fake_pds.rs`)

- **Mirrors:** the ATProto XRPC the F1r3sky publish path calls, with responses shaped
  to atrium's generated types (valid `Did`/`Handle`/`Cid`/`Datetime`):
  `com.atproto.server.{createAccount,createSession}`, app-password list/create,
  `com.atproto.repo.{uploadBlob,putRecord,deleteRecord}`, plus a served `GET
  /logo.png` — all on loopback. `createAccount` returns 400 so the publish path takes
  the sign-in branch. The happy-path test asserts the exact XRPC sequence.
- **Diverges:** no credential/app-password auth verification; no DID/handle
  resolution; no Lexicon record or blob-byte validation; sessions are immortal (no
  expiry/refresh); nothing persists across calls beyond test-armed counters;
  `/logo.png` is opaque bytes, not a decodable image; `createAccount` always refuses
  (only the sign-in / publish-again path is modeled).

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
