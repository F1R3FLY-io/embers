# Embers — Rust Node Update Log

Tracking all changes required to get embers running against the current f1r3fly Rust node. Embers was originally built against an older Scala node and has not been updated.

See [node-compatibility.md](node-compatibility.md) for the full list of known incompatibilities.

---

## 1. Environment Configuration

**Problem:** Embers had no documented configuration for connecting to the Rust node shard. The existing `docker-compose.yaml` in `docker/` bundles its own Scala standalone nodes with different keys, ports, and genesis. Running embers against an external Rust shard requires a standalone env file pointing at the correct container hostnames and internal Docker ports.

**Fix:** Created `embers.env` and `embers.env.example` with the correct configuration for a Rust shard running via `f1r3node-rust/docker/shard.yml`.

Key decisions:
- **Deploys go to `rnode.validator1`** (gRPC DeployService on internal port 40401, ProposeService on 40402)
- **Reads go to `rnode.readonly`** (HTTP REST on 40403, WebSocket on 40405)
- **SERVICE_KEY uses the bootstrap wallet** (`5f668a7ee96d944a4494cc947e4005e172d7ab3461ee5538f1f2a45a835e9657`) which is funded in the shard's genesis
- **Mainnet and testnet point at the same cluster** for local dev (the Rust shard only runs one cluster)
- **Embers container joins the `f1r3fly` Docker network** so it can reach nodes by container hostname at internal ports (not host-mapped ports)
- **Host port 8080 maps to container port 3000** to avoid conflict with Grafana on host port 3000

Run command:
```bash
docker run --env-file ./embers.env --network f1r3fly -p 8080:3000 f1r3flyio/embers:latest
```

**Status:** Done. Embers starts, connects to the Rust shard via gRPC, and begins deploying Rholang init contracts.

**Files:**
- `embers.env` — local dev config (git-ignored)
- `embers.env.example` — template with placeholder values

---

## 2. Deploy ID Response Format Mismatch

**Problem:** Embers fails during bootstrap with `failed to extract deploy_id`. The Rust node returns the deploy ID in a different string format than embers expects.

Embers parses the `doDeploy` gRPC response by stripping a prefix:
```rust
// packages/firefly-client/src/write_node_client.rs:97
deploy_id
    .strip_prefix("Success! DeployId is: ")
    .map(|id| DeployId::from(id.to_owned()))
    .context("failed to extract deploy_id")
```

The Rust node formats the response with a newline after "Success!" instead of a space:
```rust
// f1r3node-rust: casper/src/rust/api/block_api.rs:322
Either::Right(deploy_id) => Ok(format!(
    "Success!\nDeployId is: {}",
    PrettyPrinter::build_string_no_limit(deploy_id.as_ref())
)),
```

Expected: `"Success! DeployId is: <hash>"`
Actual:   `"Success!\nDeployId is: <hash>"`

The `strip_prefix` returns `None`, embers treats it as an error, and bootstrap aborts.

This mismatch exists in both the `deploy()` method (line 97) and the `deploy_signed_contract()` method (line 128) — both use the same prefix string.

**Fix:** Replaced the rigid `strip_prefix("Success! DeployId is: ")` with an `extract_deploy_id()` helper that finds `"DeployId is: "` anywhere in the response string. This handles both the old space-delimited and current newline-delimited formats. Applied to both `deploy()` and `deploy_signed_contract()` methods.

**Status:** Fixed.

**Files:**
- `packages/firefly-client/src/write_node_client.rs` — added `extract_deploy_id()` helper, updated both call sites

---

## 3. Explore-Deploy Content-Type Mismatch

**Problem:** Embers fails during `agents_teams` bootstrap with `status 415 Unsupported Media Type, body Expected request with 'Content-Type: application/json'`. The observer's `/api/explore-deploy` endpoint now requires a JSON body, but embers sends raw Rholang code as `text/plain`.

Embers sends:
```
POST /api/explore-deploy
Content-Type: text/plain

new ch in { ... }
```

The node expects:
```
POST /api/explore-deploy
Content-Type: application/json

{"term": "new ch in { ... }"}
```

The node handler (`node/src/rust/web/shared_handlers.rs:137`) uses axum's `Json<SimpleExploreDeployRequest>` extractor, which rejects non-JSON content types.

**Fix:** Changed `explore_deploy()` in `read_node_client.rs` from `.body(rholang_code).header("Content-Type", "text/plain")` to `.json(&json!({"term": rholang_code}))`, which sets `Content-Type: application/json` and wraps the code in the expected JSON structure.

**Status:** Fixed.

**Files:**
- `packages/firefly-client/src/read_node_client.rs` — changed explore_deploy() to send JSON body

---

## 4. Shared Model Types — Replace Duplicated Protos with f1r3node Git Dependencies

**Problem:** Embers maintained its own copy of 10 protobuf files and compiled them locally via `build.rs`. These had already drifted from the node's canonical versions (RhoTypes.proto: 11K vs 13K, DeployServiceCommon.proto: 5.8K vs 6.6K). Hand-written WebSocket event types (`NodeEvent`, `BlockEventDeploy`) also diverged — `deployer` was typed as `secp256k1::PublicKey` but the node sends a hex string, causing all `block-finalised` events to silently fail deserialization. This was the root cause of the frontend timeout on save/create operations.

**Fix:** Replaced local proto compilation and hand-written event types with git dependencies on the node's crates, matching how `rust-client` does it:

```toml
f1r3fly-models = { package = "models", git = "https://github.com/F1R3FLY-io/f1r3node.git", branch = "rust/dev" }
f1r3fly-shared = { package = "shared", git = "https://github.com/F1R3FLY-io/f1r3node.git", branch = "rust/dev" }
```

Changes:
- Proto modules (`casper`, `rhoapi`, `servicemodelapi`) are now re-exported from `f1r3fly_models`
- WebSocket event types (`F1r3flyEvent`, `NodeDeployEvent`) come from `f1r3fly_shared`
- `deployer` field is now `String` (matching the node), parsed to `PublicKey` only when needed for wallet address routing
- `ByteString` type conversions added in `write_node_client.rs` (proto types changed from `Vec<u8>`)
- Added `rust-toolchain.toml` (nightly-2026-02-09) to match node's toolchain requirement
- Deleted `build.rs` and `protobuf/` directory (10 proto files + external deps)
- Removed `tonic-prost` and `tonic-prost-build` dependencies

**Status:** Fixed. All three workspace crates (`firefly-client`, `embers`, `events-sync`) compile cleanly.

**Files:**
- `packages/firefly-client/Cargo.toml` — added f1r3fly-models/shared, removed build deps
- `packages/firefly-client/src/models.rs` — re-exports from f1r3fly crates, adapter functions
- `packages/firefly-client/src/node_events.rs` — uses F1r3flyEvent, new deploy routing
- `packages/firefly-client/src/write_node_client.rs` — ByteString conversions
- `rust-toolchain.toml` — nightly toolchain
- Deleted: `packages/firefly-client/build.rs`, `packages/firefly-client/protobuf/`

---

## 5. Rholang System Contract URI Renames

**Problem:** The node renamed several system contract URIs. Embers' Rholang templates used the old URIs, causing contracts to silently fail — the `revAddress!("fromDeployerId", ...)` call in every init template never returned because `rho:rev:address` no longer exists. This was the root cause of `error=contract did not return any value` (500) on all list/get endpoints.

**URI changes applied:**

| Old (embers) | New (node) | Files affected |
|---|---|---|
| `rho:rev:address` | `rho:vault:address` | agents/init.rho, agents_teams/init.rho, oslfs/init.rho |
| `rho:rchain:revVault` | `rho:vault:system` | wallets/init.rho, testnet/fund_test_wallet.rho |
| `rho:rchain:deployerId` | `rho:system:deployerId` | common/insert_signed.rho, testnet/fund_test_wallet.rho |
| `rho:rchain:deployId` | `rho:system:deployId` | agents_teams/run.rho |

Also renamed the Rholang binding variable `revAddress` → `vaultAddress` in all templates for consistency.

The node registers legacy `rho:rchain:*` aliases for backward compatibility, but `rho:rev:address` has no alias — it was removed entirely. The other renames are forward-looking to match the canonical URIs.

**Status:** Fixed.

**Files:**
- `templates/agents/init.rho` — `rho:rev:address` → `rho:vault:address`
- `templates/agents_teams/init.rho` — `rho:rev:address` → `rho:vault:address`
- `templates/oslfs/init.rho` — `rho:rev:address` → `rho:vault:address`
- `templates/wallets/init.rho` — `rho:rchain:revVault` → `rho:vault:system`
- `templates/testnet/fund_test_wallet.rho` — `rho:rchain:revVault` → `rho:vault:system`, `rho:rchain:deployerId` → `rho:system:deployerId`
- `templates/common/insert_signed.rho` — `rho:rchain:deployerId` → `rho:system:deployerId`
- `templates/agents_teams/run.rho` — `rho:rchain:deployId` → `rho:system:deployId`

---

## 6. Remove Explicit Propose Calls

**Problem:** Embers called `ProposeService::propose()` after every deploy (15+ call sites in the main package, plus events-sync and state-sync). The current node auto-proposes via heartbeat, making these calls redundant. The propose response format may also have changed.

**Fix:** Removed the `ProposeServiceClient`, `propose()`, and `full_deploy()` methods from `WriteNodeClient`. Simplified `WriteNodeClient::new()` to take only `deploy_service_url`. Removed all `propose()` calls from domain files. Deploys now enter the mempool and get included in the next auto-proposed block. The frontend detects finalization via WebSocket.

**Deploy lifecycle (before):**
1. `deploy()` → `propose()` → return block_hash → frontend waits for finalization
2. Bootstrap: deploy all init contracts → `propose()` once

**Deploy lifecycle (after):**
1. `deploy()` → return deploy_id → heartbeat auto-proposes → frontend detects finalization via WebSocket
2. Bootstrap: deploy all init contracts → heartbeat auto-proposes

**Secondary packages (events-sync, state-sync):**
These packages used `full_deploy()` which returned `BlockId` from propose. Changed to `deploy()` which returns `DeployId`. Two `get_channel_value` calls that depend on block hashes are stubbed with TODOs — these packages need a redesign to resolve deploy_id → block_hash (via `find_deploy` or WebSocket events) before channel data lookup.

**Status:** Fixed (main embers package). Events-sync and state-sync have compilation stubs for block-hash-dependent lookups.

**Files:**
- `packages/firefly-client/src/write_node_client.rs` — removed propose_client, propose(), full_deploy()
- `packages/embers/src/main.rs` — removed propose() calls, simplified WriteNodeClient::new()
- 17 domain files — removed propose() calls
- `packages/events-sync/src/main.rs` — changed to deploy(), stubbed get_channel_value
- `packages/state-sync/src/main.rs` — changed to deploy(), stubbed download lookup

---

## 7. Dockerfile Updates for Shared Node Dependencies

**Problem:** The Dockerfile used `rust:1.93-slim-bookworm` (stable) but the node's transitive dependencies require nightly Rust features (`smallvec` with `may_dangle`, `gxhash` with AES intrinsics). Additionally, `openssl-sys` and `cmake` are now needed as build dependencies from the node's `crypto` crate.

**Fix:** Updated `docker/embers.dockerfile`:
- Added `rust-toolchain.toml` copy and nightly toolchain installation (`nightly-2026-02-09`)
- Added `libssl-dev` and `cmake` to apt-get install
- Added `RUSTFLAGS="-C target-feature=+aes,+neon"` for arm64 and `+aes,+sse2` for amd64 (required by `gxhash`)

**Status:** Fixed. Docker image builds and produces a working binary.

**Files:**
- `docker/embers.dockerfile` — nightly toolchain, new build deps, RUSTFLAGS

---

## 8. WebSocket Event Envelope Format + Port Fix

**Problem:** Two issues prevented WebSocket finalization events from reaching the frontend:

1. **Wrong port:** The env config pointed WebSocket URLs at port 40405 (admin), but the `/ws/events` endpoint is on port 40403 (HTTP REST). No connection was established.

2. **Envelope format mismatch:** The node wraps events in `{"event": "...", "schema-version": 1, "payload": {...}}` but `F1r3flyEvent` from `f1r3fly_shared` expects fields at the top level (internally tagged, no `payload` wrapper). Events were silently dropped during deserialization.

**Fix:**
- Changed WebSocket URLs from `:40405` to `:40403` in `embers.env` and `embers.env.example`
- Added envelope unwrapping in `node_events.rs`: extracts `payload` fields into the top-level JSON object before deserializing into `F1r3flyEvent`
- Changed `tracing::debug!` to `tracing::warn!` for deserialization errors to make failures visible

**Status:** Fixed. WebSocket events now flow through. The `started` event logs a harmless warning (node sends `"started"` but `F1r3flyEvent` expects `"node-started"`) — this is the initial handshake and doesn't affect block events.

**Files:**
- `embers.env` — WebSocket ports 40405 → 40403
- `embers.env.example` — WebSocket ports 40405 → 40403
- `packages/firefly-client/src/node_events.rs` — envelope unwrapping, warn-level logging

---

## 9. Peek Workaround — Tuplespace Reads Fail After Agent Team Save

**Symptom:** After saving an agent team (create deploy succeeds, finalization confirmed), all `get`, `list`, and `list_versions` endpoints for agents_teams return 500 with `contract did not return any value`. Agents and OSLFs work fine.

**Verified working:**
- Peek operator (`<<-`) works in isolation (tested via explore-deploy)
- `rho:vault:address`, `rho:registry:lookup`, `rho:lang:treeHashMap`, `rho:lang:stack`, `rho:lang:listOps` all resolve correctly
- The agents_teams env contract IS registered (registry lookup returns `(0, <unforgeable>)`)
- Agents list returns `{"agents": []}` (working)
- OSLFs list returns `{"oslfs": []}` (working)
- Agents_teams list fails with `contract did not return any value`

**Hypothesis:** The agents_teams `create` contract (line 77 in `init.rho`) uses a join with peek:
```rholang
for(<- nilCh; treeHashMap, @map <<- treeHashMapCh) { ... }
```
When the save triggers this create path, the COMM through the produce path may consume `treeHashMapCh` data despite the peek. After this, all subsequent reads that peek `treeHashMapCh` block forever because the data is gone.

This pattern is identical in agents `create` (line 103), but agents hasn't been saved yet so the bug hasn't triggered there.

**Related:** GitHub issue [#385](https://github.com/F1R3FLY-io/f1r3node/issues/385) — Rholang syntax differences between main and rust/dev branches. The `for ... ; ...` multi-binding syntax and `.rho` file differences may also be a factor.

**Root cause found:** The `AgentsTeamsService::bootstrap()` calls `getFireskyTokens` via explore-deploy during startup. This explore-deploy peeks `tokensCh` and `stackCh` channels. The Rust node's explore-deploy path triggers the peek bug (consumes peeked data), destroying `tokensCh` and `stackCh`. All subsequent contract calls that depend on these channels block forever, returning `"expr": []`.

Evidence:
- First explore-deploy call during bootstrap returns data (channels still populated)
- Second explore-deploy call from user request returns empty `"expr": []` (channels consumed)
- Agents and OSLFs work because their bootstrap only deploys — no explore-deploy reads
- Wallets also fails because `WalletsService::bootstrap` likely has the same read-during-bootstrap pattern

**Fix:** Replaced all `<<-` (peek) operators with `<-` (consume) + resend in `agents_teams/init.rho`. This is a workaround — consume the data, immediately resend it to the channel, then proceed. Applied to all 11 peek sites: 3 visit contracts, create, list, listVersions, save, delete, recordDeploy, saveFireskyToken, getFireskyTokens.

**Status:** Fixed. List, get, save, and deploy all work correctly with repeated calls.

**Files:**
- `templates/agents_teams/init.rho` — replaced all `<<-` with `<-` + resend pattern

---

## 10. Serde Default for Missing Optional Fields

**Problem:** When Rholang stores `Nil` for optional fields (description, shard, logo, last_deploy, uri), the node omits them entirely from the serialized map. Embers' `Deserialize` impls expected all keys to be present, causing `failed to deserialize filed model`.

**Fix:** Added `#[serde(default)]` to all `Option<T>` fields in blockchain model structs across agents, agents_teams, oslfs, and wallets.

**Status:** Fixed.

**Files:**
- `src/blockchain/agents_teams/models.rs`
- `src/blockchain/agents/models.rs`
- `src/blockchain/oslfs/models.rs`
- `src/blockchain/wallets/models.rs`

---

## 11. GraphL String Unescape on Read

**Problem:** The graph string stored on-chain as a Rholang string literal gains extra escape sequences (`"` becomes `\"`). When read back via explore-deploy, the GraphL parser fails to parse the over-escaped string: `failed to deserialize filed model`.

**Fix:** Added unescape step in the `Graph` custom deserializer: `graphl.replace("\\\"", "\"").replace("\\\\", "\\")` before passing to `graphl_parser::parse_to_ast()`.

**Status:** Fixed. Agent team deploy now works end-to-end.

**Files:**
- `src/blockchain/agents_teams/models.rs` — Graph deserializer unescape

---

## 12. find_deploy for Deploy ID Resolution

**Problem:** After removing `propose()` (update #6), events-sync and state-sync had stubbed `get_channel_value` calls because they no longer had a `BlockId` from propose. They need to resolve `DeployId` → `BlockId` to look up channel data.

**Fix:** Added `WriteNodeClient::find_deploy()` method that calls the gRPC `findDeploy` endpoint to resolve a deploy signature to its containing block's `LightBlockInfo`. Fixed events-sync and state-sync to use `find_deploy` before `get_channel_value`.

**Status:** Fixed. All three workspace crates compile cleanly.

**Files:**
- `packages/firefly-client/src/write_node_client.rs` — added `find_deploy()` method
- `packages/events-sync/src/main.rs` — use find_deploy in subscribe_to_firefly
- `packages/state-sync/src/main.rs` — use find_deploy in download command

---

## 13. Image Compression for F1R3Sky Blob Uploads

**Problem:** When posting agent team results to F1R3Sky (`run-on-firesky`), the DALL-E 3 image (1024x1024 PNG, ~2MB) exceeds the PDS blob upload limit of 976KB: `BlobTooLarge: This file is too large. It is 1.95MB but the maximum size is 976.56KB.`

**Fix:** Added image compression in `upload_blob_from_url()`. When an image exceeds 950KB, it's decoded and re-encoded as JPEG with progressive quality reduction (starting at 85, stepping down by 10 until under limit or quality reaches 20). Added `image` crate dependency with `png` + `jpeg` features.

**Status:** Fixed. Agent replies to F1R3Sky posts now include compressed images.

**Files:**
- `packages/embers/Cargo.toml` — added `image` dependency
- `packages/embers/src/domain/common.rs` — `compress_image()` function, updated `upload_blob_from_url()`
