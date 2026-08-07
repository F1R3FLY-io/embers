# Deployment

How to stand up a complete embers deployment: two F1r3fly clusters, a F1r3drive
mount, and the embers backend, configured and brought up health-gated. This is the
production/staging counterpart to the [local stack](local-stack.md) dev-net.

## Prerequisites

- **Two F1r3fly clusters** — `mainnet` and `testnet` — each with at least one
  **validator** (gRPC deploy + WebSocket events) and one **observer** (HTTP
  explore-deploy). embers addresses both; see
  [deploy lifecycle and consistency](../architecture/deploy-lifecycle-and-consistency.md).
- **A F1r3drive FUSE mount** for the continuation store. embers refuses to boot
  otherwise (see [F1r3drive integration](../integrations/f1r3drive.md)). In Docker
  this is provided by the co-located F1r3drive in the embers image (see
  [Docker guide](docker.md)); natively, by `stack.sh drive`.
- **Funded service-key wallets** on each network (the keys that sign embers'
  deploys) and the per-domain **environment keys** — see [key inventory](../security/key-inventory.md).

## Configuration

embers is configured entirely from the `EMBERS__…` environment. The complete,
authoritative variable matrix — every required and defaulted key, with types — is in
[configuration](configuration.md). At minimum a deployment must supply
`ADDRESS`/`PORT`/`LOG_LEVEL`, the full `MAINNET`/`TESTNET` blocks (four URLs +
service key + the environment keys per network), `AES_ENCRYPTION_KEY`, and the
`CONTINUATION_STORE` block (`MOUNT_PATH` inside the F1r3drive mount, plus distinct
`ENCRYPTION_KEY` and `CAPABILITY_KEY`).

**Secrets are never committed.** Supply them via an untracked `env_file` (the staging
compose ships a `embers-staging.secrets.env.example` template); see
[key inventory](../security/key-inventory.md).

## Bring-up

The recommended path is Docker (see [Docker guide](docker.md)): the
`docker-compose-staging.yaml` file brings up the four node services, `embers` (with
its co-located F1r3drive), `embers-frontend`, and an nginx TLS proxy, with `embers`
gated on the nodes' health checks.

Boot itself is **fail-fast** (see [boot and runtime](../architecture/boot-and-runtime.md)):
embers opens the continuation store (verifying the F1r3drive mount) first, then
bootstraps all five domains — each deploying its init, awaiting finality, and
verifying the environment is observer-readable — and refuses to serve if any of that
fails. A backend that comes up healthy has therefore already proven its node
connectivity, its mount, and its on-chain registries.

## Upgrade and restart semantics

- **Prepares do not survive a restart, and that is fine.** The JWT secret is
  per-process (see [boot and runtime](../architecture/boot-and-runtime.md)); a
  restart invalidates outstanding `prepare` tokens, and a client simply re-prepares.
- **The continuation cipher-key material and the F1r3drive mount must persist across
  restarts and upgrades.** If the continuation `ENCRYPTION_KEY`/`CAPABILITY_KEY` or
  the F1r3drive cipher-key file changes, previously written continuations become
  unreadable — keep them on stable, backed-up storage (the Docker image keeps the
  drive key file on a named volume).
- **Environment keys are identities, not passwords.** Changing a domain's
  environment key changes its on-chain address and orphans existing data; treat key
  changes as deliberate migrations. See [key inventory](../security/key-inventory.md).
- **In-flight durable runs survive restarts** — they are recovered from the
  continuation store by the recovery scheduler, independent of any client token.

## Frontend

`embers-frontend` needs `API_URL` pointing at the embers backend. In the staging
compose the nginx proxy terminates TLS on `:443` and fronts both.
