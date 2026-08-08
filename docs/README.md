# Embers backend documentation

Embers is the Rust backend that fronts the F1r3fly stack: it serves an HTTP API
(55 operations) through which wallets, AI agents, agent teams, and OSLF documents
are read, created, deployed, run, and published. It deploys Rholang to f1r3node
validators, reads results back from observer nodes, runs a durable agent-team
workflow kernel over an encrypted F1r3drive-backed store, and integrates with the
F1r3sky (AT Protocol) social layer.

This directory is the backend's documentation. The **canonical agent-teams
theory** — statechart and operational semantics, the durable-execution and
capability models, and the validated TLA⁺ specifications — lives in the sibling
repository under `../../embers-frontend/docs/agent-teams/`; the documents here
cross-link that canon rather than duplicating it, and cover what is specific to
the executable backend.

## Reading paths

- **Using the API** → [`api/`](api/) — the protocol spec, one document per
  operation, and the diagrams. This is also the usage documentation.
- **Operating a deployment** → [`operations/`](operations/) —
  [configuration](operations/configuration.md) (start here; every variable is
  defined once), deployment, Docker, the local dev-net stack, CI and tooling.
- **Understanding the internals** → [`architecture/`](architecture/) — the
  [crate map](architecture/crate-architecture.md), the
  [deploy lifecycle and consistency model](architecture/deploy-lifecycle-and-consistency.md),
  the [continuation store](architecture/continuation-store.md), the
  [durable agent-team kernel](architecture/agent-team-kernel.md), the boot
  sequence, and the event journal; and [`design/`](design/) for the environment
  registry, Rholang templating, and resource accounting.
- **Reviewing security** → [`security/`](security/) — the
  [security model](security/security-model.md) and the key inventory.
- **Integrations** → [`integrations/`](integrations/) — f1r3drive, F1r3sky/AT
  Protocol, events-sync, state-sync.
- **Testing and verification** → [`testing/`](testing/) and the
  [formal-verification program](formal/README.md) with its
  [invariant registry](formal/INVARIANTS.md).

## Conventions

Diagrams follow the palette and numbering defined once in
[`diagrams/README.md`](diagrams/README.md) (PlantUML sources with committed SVGs;
numbering continues the cross-repo sequence at 27+). Mathematics uses
GitHub-safe MathJax (backtick-wrapped `$`…`$` inline, fenced `math` blocks for
display). Citations link DOIs where they exist.

## Glossary

Defined once here; every document links a term on first use.

- **Validator (write node)** — the f1r3node that accepts gRPC deploys and
  proposes blocks. **Observer (read node)** — a non-proposing follower that
  serves HTTP `explore-deploy` reads. The Rust type is `ReadNodeClient`; prose
  prefers *observer*.
- **Finality** — consensus commitment of a deploy's block, a validator-side event
  detected over WebSocket. **Visibility** — the observer's state reflecting that
  deploy. Visibility lags finality; "confirmed" is never used unqualified. (See
  [deploy lifecycle and consistency](architecture/deploy-lifecycle-and-consistency.md).)
- **Env** — a per-domain root contract inserted into the registry at boot (one
  each for wallets, agents, agent teams, OSLF documents, plus one testnet env).
  Its **registry URI** is its address, derived from its **env key**.
- **Capability** — authority by possession of an unforgeable name: the HMAC-derived
  artifact name in the continuation store, or the UUIDv7 run URL for a run's
  metadata.
- **Envelope** — a signed `send` payload: the echoed prepare request and response,
  the signed contract, and the binding token.
- **Boost** — a social value-transfer operation attributing an amount to a
  F1r3sky post.
- **OSLF** — *Operational Semantics in Logical Form* (Meredith & Stay); an OSLF
  **document** is a stored, versioned query expression authored in the OSLF
  editor.
- **FLT** — *Foreign Language Term*, the pinned portable ABI for foreign-language
  activities (validated but not yet executed).
- **Continuation store** — the encrypted, capability-addressed object store on the
  F1r3drive FUSE mount that holds a durable run's private state between steps.
- **Kernel** — the deterministic, durable Agent-Teams orchestration engine
  (`runtime.rs`) that runs a workflow-v2 run in the embers process. Its checkpoint
  is the **continuation** (the `ContinuationEnvelope`), distinct from an *RSpace /
  Rholang continuation* — the `for(x<-chan){P}` consumer f1r3node persists. (See
  [the durable agent-team kernel](architecture/agent-team-kernel.md).)
- **Effect key** — the domain-separated hash of (run, execution, node, epoch,
  attempt) that names an activity effect; the observation store is a function of
  it, giving apply-exactly-once recovery.
- **Revision fence** — the monotone checkpoint number a run advances by exactly one
  per `advanceRun`; commands and advances are bound (fenced) to the revision they
  claimed.
