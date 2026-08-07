# Embers Web API reference

This directory is the reference documentation for the Embers HTTP API: the blockchain-backed
service through which wallets, AI agents, agent teams, and OSLF documents are read, created,
deployed, run, and published. It has three layers:

- [HTTP API and OpenAPI surface](http-api.md) — the protocol-level specification: the served
  OpenAPI description and Swagger UI, the universal two-phase prepare, sign, send protocol,
  the authentication and authorization model, error semantics, and the pinned
  specification-to-SDK generation chain. Protocol mechanics are defined once, there.
- [`methods/`](methods/) — one document per logical operation (a standalone endpoint, or a
  prepare/send pair documented together as the single protocol transaction it is). Every
  document specifies semantics, request and response schemas, signing and binding, retry
  semantics, a complete sequence diagram, a worked example, and SDK usage.
- [`diagrams/`](diagrams/) — PlantUML sources with rendered SVGs committed alongside
  (regenerate with `plantuml -tsvg docs/api/diagrams/**/*.puml` from this repository root).

## The served surface

The backend serves one OpenAPI 3.0 service titled `Embers API`, version `0.1.0`, mounted
under the `/api` prefix and composed from six modules: `ServiceApi`, `TestnetApi`,
`WalletsApi`, `AgentsApi`, `AgentsTeamsApi`, and `OslfsApi`. The live description and an
interactive Swagger UI are served at `/swagger-ui/openapi.json`, `/swagger-ui/openapi.yaml`,
and `/swagger-ui/index.html` (default local base `http://localhost:8080`). The
[served surface](http-api.md#the-served-surface) section of the protocol specification covers
these routes and how to browse them safely.

## Authentication and signing, in one paragraph

Reads are either public (collection and version lookups under a wallet address) or
capability-scoped (run inspection answers to whoever holds the UUIDv7 run URL). Every
mutation is a blockchain deploy signed client-side by a wallet key that never leaves the
client: the client calls `prepare`, verifies and signs the returned contract bytes locally
(secp256k1 over a blake2b-256 digest), and posts the signed contract back through `send`,
where a hash-bound token guarantees the send belongs to exactly one prepare exchange. The
[two-phase protocol](http-api.md#the-two-phase-prepare-sign-send-protocol) and
[authentication and authorization](http-api.md#authentication-and-authorization) sections are
normative; each method document states only what is specific to that operation (which key
signs which contract, whether a token is present, and what the deploy identifier is).

## Conventions that hold across the API

- Every JSON request and response body is served with content type
  `application/json; charset=utf-8`.
- Collection reads return complete collections: the schema defines no pagination, filter, or
  sort query parameters anywhere on the surface.
- Not-found responses (HTTP 404) carry no body. Server faults (HTTP 500) carry an
  `InternalError` body. A few operations report domain failures inside an HTTP 200 response
  (a discriminated union or a state readback); each such case is called out in its method
  document.
- Domain-specific string formats (`uint64`, `timestamp-millis`, `bytes`, `blockchain-address`,
  `public-key`, `blockchain-uri`, `graphl`) map to validated SDK entity types via the
  [format mapping table](http-api.md#the-specification-artifact-chain).
- Every operation carries an `operationId` (for example `wallets_prepare_transfer`),
  declared on the backend endpoint and camelized by the generator into the client method
  name (`walletsPrepareTransfer`) — the stable programmatic identifier for the operation.
  Each method document lists it in its header block.
- Request and response models are documented by the generated TypeScript declarations
  quoted in each method document, doc comments included. The comments originate as Rust
  `///` doc comments on the backend API models (`packages/embers/src/api/*/models.rs` and
  `api/common.rs` in this repository) and flow through the OpenAPI snapshot into
  the generated SDK, so the quoted declarations are self-documenting and machine-checked
  against the generated sources. Property names are the SDK's camelCase; the wire JSON
  uses snake_case where they differ (the generated codecs translate; each document's
  worked example shows wire names). Custom scalar types named in the declarations
  (`Address`, `Base64`, `Int64`, `UInt64`, `Uri`, `TimestampMillis`, `Graph`, `PublicKey`)
  are the validated entities of the [format mapping table](http-api.md#the-specification-artifact-chain).
- Documentation integrity is machine-checked: `pnpm docs:check` (from the sibling
  embers-frontend repository root) verifies every relative link and anchor and every
  inline-math delimiter across that repository's `docs/`, `README.md`, and `CLAUDE.md`
  plus this directory — and, for the API method documents, that every quoted
  TypeScript declaration matches the generated model it names, member for member and
  comment for comment, and that every schema pin names the snapshot's last-touching
  commit. The checker resolves this directory via the sibling path `../embers/docs/api`
  and skips it with a warning when the embers checkout is absent. Run it after any
  documentation change.

## Method index

Each entry is one logical operation. Prepare/send pairs are documented in a single document
covering both HTTP methods end-to-end.

### Service

| Operation | Methods and paths | Reference |
| --- | --- | --- |
| Service readiness | `GET /api/service/ready` | [methods/service/ready.md](methods/service/ready.md) |

### Testnet

| Operation | Methods and paths | Reference |
| --- | --- | --- |
| Wallet faucet | `POST /api/testnet/wallet` | [methods/testnet/wallet.md](methods/testnet/wallet.md) |
| Testnet deploy | `POST /api/testnet/deploy/prepare` + `/send` | [methods/testnet/deploy.md](methods/testnet/deploy.md) |

### Wallets

| Operation | Methods and paths | Reference |
| --- | --- | --- |
| Wallet state and history | `GET /api/wallets/{address}/state` | [methods/wallets/state.md](methods/wallets/state.md) |
| Transfer | `POST /api/wallets/transfer/prepare` + `/send` | [methods/wallets/transfer.md](methods/wallets/transfer.md) |
| Boost | `POST /api/wallets/boost/prepare` + `/send` | [methods/wallets/boost.md](methods/wallets/boost.md) |
| Deploys WebSocket | `GET /api/wallets/{address}/deploys` (101 upgrade) | [methods/wallets/deploys-websocket.md](methods/wallets/deploys-websocket.md) |

### AI agents

| Operation | Methods and paths | Reference |
| --- | --- | --- |
| List agents | `GET /api/ai-agents/{address}` | [methods/ai-agents/list.md](methods/ai-agents/list.md) |
| List versions | `GET /api/ai-agents/{address}/{id}/versions` | [methods/ai-agents/versions.md](methods/ai-agents/versions.md) |
| Fetch version | `GET /api/ai-agents/{address}/{id}/versions/{version}` | [methods/ai-agents/version.md](methods/ai-agents/version.md) |
| Create | `POST /api/ai-agents/create/prepare` + `/send` | [methods/ai-agents/create.md](methods/ai-agents/create.md) |
| Save | `POST /api/ai-agents/{id}/save/prepare` + `/send` | [methods/ai-agents/save.md](methods/ai-agents/save.md) |
| Deploy | `POST /api/ai-agents/deploy/prepare` + `/send` | [methods/ai-agents/deploy.md](methods/ai-agents/deploy.md) |
| Delete | `POST /api/ai-agents/{id}/delete/prepare` + `/send` | [methods/ai-agents/delete.md](methods/ai-agents/delete.md) |

### AI agent teams

| Operation | Methods and paths | Reference |
| --- | --- | --- |
| List teams | `GET /api/ai-agents-teams/{address}` | [methods/ai-agents-teams/list.md](methods/ai-agents-teams/list.md) |
| List versions | `GET /api/ai-agents-teams/{address}/{id}/versions` | [methods/ai-agents-teams/versions.md](methods/ai-agents-teams/versions.md) |
| Fetch version | `GET /api/ai-agents-teams/{address}/{id}/versions/{version}` | [methods/ai-agents-teams/version.md](methods/ai-agents-teams/version.md) |
| Create | `POST /api/ai-agents-teams/create/prepare` + `/send` | [methods/ai-agents-teams/create.md](methods/ai-agents-teams/create.md) |
| Save | `POST /api/ai-agents-teams/{id}/save/prepare` + `/send` | [methods/ai-agents-teams/save.md](methods/ai-agents-teams/save.md) |
| Deploy | `POST /api/ai-agents-teams/deploy/prepare` + `/send` | [methods/ai-agents-teams/deploy.md](methods/ai-agents-teams/deploy.md) |
| Delete | `POST /api/ai-agents-teams/{id}/delete/prepare` + `/send` | [methods/ai-agents-teams/delete.md](methods/ai-agents-teams/delete.md) |
| Run | `POST /api/ai-agents-teams/run/prepare` + `/send` | [methods/ai-agents-teams/run.md](methods/ai-agents-teams/run.md) |
| Run state | `GET /api/ai-agents-teams/runs/{run_id}` | [methods/ai-agents-teams/run-state.md](methods/ai-agents-teams/run-state.md) |
| Run commands | `POST /api/ai-agents-teams/runs/{run_id}/commands/prepare` + `/send` | [methods/ai-agents-teams/run-commands.md](methods/ai-agents-teams/run-commands.md) |
| Reveal output | `POST /api/ai-agents-teams/runs/{run_id}/output/prepare` + `/send` | [methods/ai-agents-teams/reveal-output.md](methods/ai-agents-teams/reveal-output.md) |
| Run on F1r3sky | `POST /api/ai-agents-teams/run-on-firesky/prepare` + `/send` | [methods/ai-agents-teams/run-on-firesky.md](methods/ai-agents-teams/run-on-firesky.md) |
| Publish to F1r3sky | `POST /api/ai-agents-teams/{address}/{id}/publish-to-firesky/prepare` + `/send` | [methods/ai-agents-teams/publish-to-firesky.md](methods/ai-agents-teams/publish-to-firesky.md) |
| Published state | `GET /api/ai-agents-teams/{address}/{id}/published` | [methods/ai-agents-teams/published.md](methods/ai-agents-teams/published.md) |
| Unpublish | `POST /api/ai-agents-teams/{address}/{id}/unpublish` | [methods/ai-agents-teams/unpublish.md](methods/ai-agents-teams/unpublish.md) |
| Referencing teams | `GET /api/ai-agents-teams/{address}/referencing/{agent_id}` | [methods/ai-agents-teams/referencing.md](methods/ai-agents-teams/referencing.md) |

### OSLF documents

| Operation | Methods and paths | Reference |
| --- | --- | --- |
| List documents | `GET /api/oslfs/{address}` | [methods/oslfs/list.md](methods/oslfs/list.md) |
| List versions | `GET /api/oslfs/{address}/{id}/versions` | [methods/oslfs/versions.md](methods/oslfs/versions.md) |
| Fetch version | `GET /api/oslfs/{address}/{id}/versions/{version}` | [methods/oslfs/version.md](methods/oslfs/version.md) |
| Create | `POST /api/oslfs/create/prepare` + `/send` | [methods/oslfs/create.md](methods/oslfs/create.md) |
| Save | `POST /api/oslfs/{id}/save/prepare` + `/send` | [methods/oslfs/save.md](methods/oslfs/save.md) |
| Delete | `POST /api/oslfs/{id}/delete/prepare` + `/send` | [methods/oslfs/delete.md](methods/oslfs/delete.md) |

## Maintenance

The reference is tied to the OpenAPI snapshot it documents, and its upkeep spans two sibling
repositories: this reference lives in the **embers** backend repository, next to the endpoint
sources it anchors (`packages/embers/src/api/*/endpoints.rs`), while the OpenAPI snapshot,
the generated TypeScript SDK, and the documentation checker live in the **embers-frontend**
repository. The workflow below assumes both repositories are cloned side by side under one
workspace directory. Repo-relative paths of the form `packages/client/…` — including the
framing sentence above every quoted TypeScript declaration in `methods/` — refer to
**embers-frontend**; paths of the form `packages/embers/…` refer to this repository.

- **Schema pin.** Every method document's header table carries a row
  `| Schema pin | packages/client/schema.json @ 01e8ea0 (Embers API 0.1.0) |` naming the
  last embers-frontend commit that touched the snapshot when the document was written.
  `pnpm docs:check` (run in embers-frontend) enforces freshness against
  `git log -1 --format=%h -- packages/client/schema.json` in the embers-frontend history;
  after a commit touches the snapshot, bump every pin in one pass (from this repository's
  root) and commit the documentation with it:

  ```sh
  NEW=$(git -C ../embers-frontend log -1 --format=%h -- packages/client/schema.json)
  grep -rl "Schema pin" docs/api | xargs sed -i "s/@ \`[0-9a-f]\{7,\}\`/@ \`$NEW\`/"
  ```
- **Updating after a schema change.** In embers-frontend, regenerate the snapshot per the
  [client package README](../../../embers-frontend/packages/client/README.md) (fetch the served specification
  from a running backend, then `pnpm generate-api-client` and `pnpm normalize-api-client`).
  Then, still in embers-frontend, `git diff <old-pin> -- packages/client/schema.json`, map
  each changed path to its owning document through the method index above, update those
  documents and their diagrams, and bump their pin lines.
- **Drift checks divide cleanly.** The
  [conformance evidence table](../../../embers-frontend/docs/agent-teams/engineering/conformance-and-testing.md) checks
  the snapshot against the served specification (backend-to-snapshot drift); the schema pins
  check the snapshot against this reference (snapshot-to-docs drift).
- **Operation identifiers.** All 55 operations declare an `operation_id` on their backend
  endpoint (`#[oai(...)]` attributes in `packages/embers/src/api/*/endpoints.rs`), so the
  generated method names are stable under path refactoring. The offline
  `cargo run --bin generate_schema` in this repository's `packages/embers` regenerates the
  snapshot without a running service. Model **field** documentation flows the same way:
  Rust `///` doc comments on the API model structs become OpenAPI property descriptions
  and, through regeneration, the JSDoc on the generated SDK declarations this reference
  quotes. Adding `summary`/`description` doc comments to the remaining endpoints (three
  carry them today) remains worthwhile for Swagger UI readers.

## Cross-references

- [Workflow Definition v2](../../../embers-frontend/docs/agent-teams/specification/workflow-definition-v2.md) — the
  portable document the agent-teams endpoints carry.
- [Durable and hierarchical execution](../../../embers-frontend/docs/agent-teams/architecture/durable-execution.md) —
  the run ledger and command authorization the run endpoints front.
- [Object-capability model](../../../embers-frontend/docs/agent-teams/security/capability-model.md) — how the API's
  three authorities are classified.
- [Editor and operator guide](../../../embers-frontend/docs/agent-teams/usage/editor-and-operations.md) — local stack
  startup and a raw two-phase command walk-through.
