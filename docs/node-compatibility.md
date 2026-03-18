# Embers — Node Compatibility Issues

Embers was built against an older version of the f1r3fly node. The node has since evolved (both Scala and Rust implementations share these changes), and embers has not been updated. This document tracks known incompatibilities between the current embers codebase and the current f1r3fly node.

## Incompatibilities

### 1. ~~Explicit Propose Calls~~ — FIXED (update #6)

**Severity: High**

~~Embers calls `ProposeService::propose()` explicitly after every deployment. Current nodes auto-propose via heartbeat, making these calls redundant at best and potentially problematic.~~

All propose calls removed. Deploys enter the mempool and get auto-proposed by heartbeat. There were 15 explicit `propose()` call sites in the embers main package plus additional calls in events-sync and state-sync:

- `domain/wallets/transfer.rs:65` — after transfer deploy
- `domain/wallets/boost.rs:67` — after boost transfer deploy
- `domain/agents/deploy.rs:97` — after agent deploy
- `domain/agents_teams/deploy.rs:111` — after agents team deploy
- `domain/agents_teams/run_agents_team.rs:75` — after run agents team deploy
- `domain/oslfs/save.rs:73` — after OSLFS save deploy
- `domain/testnet/deploy_test.rs:76` — after env deploy
- `domain/testnet/deploy_test.rs:93` — after test deploy
- `main.rs:98` — mainnet bootstrap
- `main.rs:123` — testnet bootstrap
- `packages/events-sync/src/main.rs` — 3 call sites
- `packages/state-sync/src/main.rs` — 2 call sites

The embers deploy workflow is: `doDeploy()` → `propose()` → wait for `BlockFinalised` event. The propose step should be removed, relying instead on the heartbeat auto-propose to include queued deploys in the next block.

**What to verify:** Whether the node's `ProposeService::propose()` endpoint still accepts calls without error, or returns an error/no-op. If it errors, embers will fail on every deploy.

### 2. Rholang System Contract Availability

**Severity: High**

Embers deploys 41 Rholang templates that depend on system contracts being available in the registry. These must all be present and API-compatible in the current node's Rholang interpreter:

**Core system contracts used:**
- ~~`rho:rchain:revVault`~~ → `rho:vault:system` — token vault (findOrCreate, transfer) — **FIXED in update #5**
- `rho:registry:lookup` — registry lookup
- `rho:registry:insertSigned:secp256k1` — signed registry inserts (used for all env initialization)
- `rho:deploy:data` — deploy metadata injection
- ~~`rho:rev:address`~~ → `rho:vault:address` — deployer ID to address conversion — **FIXED in update #5**

**Data structure contracts used:**
- `rho:lang:treeHashMap` — multi-level map storage (primary data structure for agents, teams, wallets, OSLFS)
- `rho:lang:listOps` — list operations (parMap, unorderedParMap)
- `rho:lang:stack` — stack data structure
- `rho:lang:either` — either/result monad

**Other contracts:**
- `rho:io:devNull` — discard values
- `rho:execution:abort` — abort on error
- `rho:io:grpcTell` — gRPC communication (events-sync only)

If any of these are missing or have different APIs, the corresponding embers features will break.

### 3. Deploy Data Tuple Format

**Severity: High**

Embers expects `rho:deploy:data!(*ch)` to return a tuple destructured as `(_, deployerId, deployId)`. This is used extensively across initialization templates to identify the deployer.

If the node returns a different tuple structure, all deploy templates that use deployer identity will fail.

### 4. Registry Lookup Return Format

**Severity: High**

Embers expects registry lookups via `rho:registry:lookup` to return `(versionNumber, bundledContract)` tuples, destructured as `for(@(_, contract) <- returnCh)`.

If the registry returns a different format, all service initialization will fail.

### 5. ~~WebSocket Event Format~~ — FIXED (update #4)

**Severity: Medium**

Embers subscribes to `ws://<node>:40405/ws/events` and expects JSON text frames with this structure:

```json
{
  "event": "block-finalised",
  "schema-version": 1,
  "payload": {
    "block-hash": "<hash>",
    "deploys": [
      {
        "id": "<deploy-id>",
        "cost": 12345,
        "deployer": "<public-key>",
        "errored": false
      }
    ]
  }
}
```

Expected event tags: `started`, `block-added`, `block-created`, `block-finalised`.

Embers uses `BlockFinalised` events to:
- Trigger wallet subscription callbacks
- Detect when specific deploy IDs have been finalized (with 1-minute timeout)

If the node uses different event names, payload structure, or doesn't implement the WebSocket endpoint, embers will not detect deploy finalization.

### 6. ~~Synchronous Propose Behavior~~ — FIXED (update #6)

**Severity: Medium**

~~Embers calls `propose(ProposeQuery { is_async: false })` — requesting synchronous propose.~~ Propose calls removed entirely. No longer relevant.

## Unknown / Needs Verification

### Rholang Interpreter Completeness

The 41 Rholang templates use pattern matching, tuple destructuring, channel-based communication, unforgeable names, and registry operations. The current interpreter needs to support all of these features for embers to function.

### SystemVault Transfer Semantics

Embers' wallet transfer feature (`wallets/init.rho`) calls `SystemVault!("findOrCreate", ...)` and `vault!("transfer", ...)` via `rho:vault:system`. The node must implement compatible SystemVault contract behavior (previously called revVault).

### Signed Registry Insert

Embers uses `rho:registry:insertSigned:secp256k1` for all environment initialization. This requires the node to support secp256k1 signature verification within the Rholang interpreter.

## Confirmed Compatible

- **gRPC DeployService** — protobuf definitions shared via f1r3fly-models git dep (update #4)
- **HTTP REST `/api/explore-deploy`** — sends `application/json` with `{"term": "..."}` body (update #3)
- **WebSocket events** — uses `F1r3flyEvent` from f1r3fly-shared (update #4)
- **Deploy ID response** — handles both `"Success! DeployId is: "` and `"Success!\nDeployId is: "` (update #2)
- **RhoExpr/RhoUnforg serialization** — both sides use external tagging, no change needed
- **JSON field naming (camelCase)** — embers doesn't parse block JSON fields, no impact
- **Port layout** — 40400-40405 (protocol, gRPC ext, gRPC int, HTTP, discovery, admin)

## Update Log

See [embers-rust-node-updates.md](embers-rust-node-updates.md) for the full change log (7 updates).
