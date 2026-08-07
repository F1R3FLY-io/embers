# Configuration

This is the single, canonical description of how the Embers backend binary
(`embers`) is configured. Every other operations document — the
[deployment guide](deployment.md), the [Docker guide](docker.md), the
[local-stack runbook](local-stack.md) — links here rather than restating a
variable, so there is exactly one place a variable's name, type, default, and
meaning are defined.

The two auxiliary binaries, `events-sync` and `state-sync`, are configured by
CLI flags instead of the environment; their flags are documented with each
binary (see the [events-sync](../integrations/events-sync.md) and
[state-sync](../integrations/state-sync.md) integration guides).

## Mechanism

`embers` reads all of its configuration from **environment variables** through
[Figment](https://docs.rs/figment), with the prefix `EMBERS__` and `__` (double
underscore) as the nesting separator (`packages/embers/src/configuration.rs`,
`collect_config`). There is no configuration file and there are no command-line
arguments. A nested field `mainnet.deploy_service_url` is therefore set by the
variable `EMBERS__MAINNET__DEPLOY_SERVICE_URL`.

Two field kinds need a specific string format:

- **secp256k1 secret keys** (`*_KEY` fields typed `SecretKey`) are 64-hex-character
  strings (32 bytes), each a valid secp256k1 scalar.
- **symmetric keys** (the AES key and the two continuation-store keys, typed
  `[u8; 32]`) are 64-hex-character strings, decoded by `deserialize_hex_key`.

Everything is required unless a default is listed below. A missing required
variable, a malformed hex key, or a violated invariant (below) aborts start-up
with a contextual error before the server binds.

## The configuration matrix

### Top-level (`Config`)

| Variable | Type | Required | Meaning |
| -------- | ---- | -------- | ------- |
| `EMBERS__ADDRESS` | string | yes | The bind address. The dev stack binds the IPv6 loopback `::1`. |
| `EMBERS__PORT` | `u16` | yes | The bind port. **There is no default** — the value must be set. The `cargo make run` task and the dev stack use `8080`; the container image defaults `EMBERS__PORT=3000` in its entrypoint. State this here so no other document has to. |
| `EMBERS__LOG_LEVEL` | string | yes | The `tracing` `EnvFilter` directive (e.g. `info`, or `embers=debug,tower_http=warn`). |
| `EMBERS__AES_ENCRYPTION_KEY` | 32-byte hex | yes | Encrypts F1r3sky credential envelopes stored on chain. |
| `EMBERS__MAINNET__*` | `MainNet` | yes | The mainnet node stack (below). |
| `EMBERS__TESTNET__*` | `TestNet` | yes | The testnet node stack (below). |
| `EMBERS__CONTINUATION_STORE__*` | `ContinuationStore` | yes | The durable Agent-Teams artifact store (below). |
| `EMBERS__FINALIZATION__*` | `Finalization` | no | Deploy-finality budget (below); fully defaulted. |

### Mainnet node stack (`MainNet`)

| Variable | Type | Meaning |
| -------- | ---- | ------- |
| `EMBERS__MAINNET__DEPLOY_SERVICE_URL` | string | The **validator** gRPC endpoint deploys are submitted to. |
| `EMBERS__MAINNET__VALIDATOR_WS_API_URL` | string | The validator WebSocket (`/ws/events`) for deploy-finality events. |
| `EMBERS__MAINNET__OBSERVER_URL` | string | The **observer** HTTP endpoint reads (`/api/explore-deploy`) go to. |
| `EMBERS__MAINNET__OBSERVER_WS_API_URL` | string | The observer WebSocket for its events. |
| `EMBERS__MAINNET__SERVICE_KEY` | secp256k1 hex | The service wallet key that signs the service's own deploys (env init, agent-team runtime deploys, faucet). |
| `EMBERS__MAINNET__WALLETS_ENV_KEY` | secp256k1 hex | Identity of the wallets env registry contract. |
| `EMBERS__MAINNET__AGENTS_ENV_KEY` | secp256k1 hex | Identity of the agents env registry contract. |
| `EMBERS__MAINNET__AGENTS_TEAMS_ENV_KEY` | secp256k1 hex | Identity of the agent-teams env registry contract. |
| `EMBERS__MAINNET__OSLFS_ENV_KEY` | secp256k1 hex | Identity of the OSLF-documents env registry contract. |

Each `*_ENV_KEY` is the **identity** of its on-chain env contract — the
registry URI is derived from the key — so the four keys must be distinct
secrets, and rotating one relocates that env (see
[Key inventory](../security/key-inventory.md)).

### Testnet node stack (`TestNet`)

Identical shape to `MainNet` but with a single env key, because the testnet
hosts one combined sandbox env:

| Variable | Type | Meaning |
| -------- | ---- | ------- |
| `EMBERS__TESTNET__DEPLOY_SERVICE_URL` | string | Testnet validator gRPC endpoint. |
| `EMBERS__TESTNET__VALIDATOR_WS_API_URL` | string | Testnet validator WebSocket. |
| `EMBERS__TESTNET__OBSERVER_URL` | string | Testnet observer HTTP endpoint. |
| `EMBERS__TESTNET__OBSERVER_WS_API_URL` | string | Testnet observer WebSocket. |
| `EMBERS__TESTNET__SERVICE_KEY` | secp256k1 hex | Testnet service wallet key (funds the faucet). |
| `EMBERS__TESTNET__ENV_KEY` | secp256k1 hex | Identity of the testnet env registry contract. |

### Continuation store (`ContinuationStore`)

| Variable | Type | Required | Default | Meaning |
| -------- | ---- | -------- | ------- | ------- |
| `EMBERS__CONTINUATION_STORE__MOUNT_PATH` | path | yes | — | Directory for the encrypted artifact store. **Must be inside a FUSE mount whose fsname contains `f1r3drive`** — `embers` refuses to start otherwise (see [F1r3drive integration](../integrations/f1r3drive.md)). |
| `EMBERS__CONTINUATION_STORE__ENCRYPTION_KEY` | 32-byte hex | yes | — | AES-256-GCM key for artifact contents. |
| `EMBERS__CONTINUATION_STORE__CAPABILITY_KEY` | 32-byte hex | yes | — | HMAC-SHA256 key deriving artifact capability names. Must differ from the encryption key (invariant below). |
| `EMBERS__CONTINUATION_STORE__MAX_OBJECT_BYTES` | `u64` | no | 64 MiB | Per-artifact size limit. |
| `EMBERS__CONTINUATION_STORE__RECOVERY_POLL_MILLIS` | `u64` | no | 1000 | Cadence of the durable-recovery scheduler. Must be `> 0`. |

### Finalization (`Finalization`)

| Variable | Type | Required | Default | Meaning |
| -------- | ---- | -------- | ------- | ------- |
| `EMBERS__FINALIZATION__DEPLOY_TIMEOUT_MILLIS` | `u64` | no | 120000 | The shared deploy-finality wait budget. Must be `> 0`. |

## Boot-time invariants

`collect_config` enforces three invariants after extraction and aborts start-up
with the quoted message if any is violated (`configuration.rs`):

1. `continuation encryption and capability keys must be independent` — the two
   continuation-store keys must differ. The encryption key protects artifact
   contents (AES-GCM); the capability key derives their names (HMAC). Reusing one
   key for both would let a party who can read the on-chain capability material
   participate in the confidentiality boundary.
2. `continuation recovery poll interval must be positive` —
   `RECOVERY_POLL_MILLIS > 0`.
3. `deploy finalization timeout must be positive` — `DEPLOY_TIMEOUT_MILLIS > 0`.

## Container supervisor variables

When `embers` runs in its published image it shares a container with the
co-located F1r3drive process, started first by `docker/embers-entrypoint.sh`.
Those variables configure the supervisor, not `embers` directly, and are
documented in full in the [Docker guide](docker.md); named here for
completeness: `F1R3DRIVE_ENABLED`, `F1R3DRIVE_JAR`, `F1R3DRIVE_MOUNT`,
`F1R3DRIVE_KEY_FILE`, `F1R3DRIVE_VALIDATOR_HOST`, `F1R3DRIVE_VALIDATOR_PORT`,
`F1R3DRIVE_OBSERVER_HOST`, `F1R3DRIVE_OBSERVER_PORT`, `F1R3DRIVE_WALLET_ADDRESS`,
`F1R3DRIVE_WALLET_KEY`, `F1R3DRIVE_MANUAL_PROPOSE`,
`F1R3DRIVE_START_TIMEOUT_SECONDS`, `NODE_HTTP_WAIT_URL`,
`NODE_WAIT_TIMEOUT_SECONDS`.

## A complete reference environment

A minimal, internally-consistent environment for a local two-node dev net (the
node URLs match the native `embers-local-stack` ports; keys shown are
illustrative placeholders — never reuse them, see
[Key inventory](../security/key-inventory.md)):

```bash
export EMBERS__ADDRESS='::1'
export EMBERS__PORT=8080
export EMBERS__LOG_LEVEL=info
export EMBERS__AES_ENCRYPTION_KEY=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

export EMBERS__MAINNET__DEPLOY_SERVICE_URL=http://localhost:40401
export EMBERS__MAINNET__VALIDATOR_WS_API_URL=ws://localhost:40403
export EMBERS__MAINNET__OBSERVER_URL=http://localhost:40413
export EMBERS__MAINNET__OBSERVER_WS_API_URL=ws://localhost:40413
export EMBERS__MAINNET__SERVICE_KEY=0101010101010101010101010101010101010101010101010101010101010101
export EMBERS__MAINNET__WALLETS_ENV_KEY=0202020202020202020202020202020202020202020202020202020202020202
export EMBERS__MAINNET__AGENTS_ENV_KEY=0303030303030303030303030303030303030303030303030303030303030303
export EMBERS__MAINNET__AGENTS_TEAMS_ENV_KEY=0404040404040404040404040404040404040404040404040404040404040404
export EMBERS__MAINNET__OSLFS_ENV_KEY=0505050505050505050505050505050505050505050505050505050505050505

export EMBERS__TESTNET__DEPLOY_SERVICE_URL=http://localhost:40401
export EMBERS__TESTNET__VALIDATOR_WS_API_URL=ws://localhost:40403
export EMBERS__TESTNET__OBSERVER_URL=http://localhost:40413
export EMBERS__TESTNET__OBSERVER_WS_API_URL=ws://localhost:40413
export EMBERS__TESTNET__SERVICE_KEY=0606060606060606060606060606060606060606060606060606060606060606
export EMBERS__TESTNET__ENV_KEY=0707070707070707070707070707070707070707070707070707070707070707

export EMBERS__CONTINUATION_STORE__MOUNT_PATH=/run/embers/f1r3drive/<wallet-address>
export EMBERS__CONTINUATION_STORE__ENCRYPTION_KEY=1111111111111111111111111111111111111111111111111111111111111111
export EMBERS__CONTINUATION_STORE__CAPABILITY_KEY=2222222222222222222222222222222222222222222222222222222222222222
```

## What this replaces

The historical `docs/deployment.md` documented several variables that do not
exist in `Config` (`EMBERS__MAINNET__PROPOSE_SERVICE_URL`,
`EMBERS__MAINNET__READ_NODE_URL`) and omitted the WebSocket URLs, the env keys,
the AES key, the whole continuation-store block, and the finalization timeout.
This matrix, generated from `configuration.rs`, is authoritative; the deployment
guide has been rewritten to link here.
