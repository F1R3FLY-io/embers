# Key inventory

This is the catalog of every secret embers consumes: what it is, where it comes
from, what it protects, and how (or whether) it may be rotated. It also records a
security finding in the committed staging compose and its remediation.

All secrets are supplied through the `EMBERS__…` environment (see
[configuration](../operations/configuration.md)) or, for the co-located F1r3drive,
the entrypoint environment. **None belongs in version control.**

## Secret classes

| # | Secret | Env var | Protects | Rotation |
| - | ------ | ------- | -------- | -------- |
| 1 | AES encryption key | `EMBERS__AES_ENCRYPTION_KEY` | Backend-held AES key material (32 bytes, hex). | Rotatable; re-encrypts nothing on chain. |
| 2 | Continuation store **encryption** key | `EMBERS__CONTINUATION_STORE__ENCRYPTION_KEY` | AES-256-GCM confidentiality of continuation objects (32 bytes, hex). | Rotating it makes **existing** continuations unreadable — drain runs first. Must differ from #3 (validated). |
| 3 | Continuation store **capability** key | `EMBERS__CONTINUATION_STORE__CAPABILITY_KEY` | HMAC-SHA256 capability derivation → object paths (32 bytes, hex). | Rotating it re-addresses every object — drain runs first. Must differ from #2. |
| 4 | Mainnet service key | `EMBERS__MAINNET__SERVICE_KEY` | secp256k1 key that signs and funds embers' own mainnet deploys. | Rotatable (it is an actor, not an address); fund the new key. |
| 5 | Mainnet environment keys | `EMBERS__MAINNET__{WALLETS,AGENTS,AGENTS_TEAMS,OSLFS}_ENV_KEY` | secp256k1 keys whose **public** keys derive each domain's on-chain `env_uri`. | **Identity, not a password** — see the warning below. |
| 6 | Testnet service key | `EMBERS__TESTNET__SERVICE_KEY` | secp256k1 key signing embers' testnet deploys. | Rotatable; fund the new key. |
| 7 | Testnet environment key | `EMBERS__TESTNET__ENV_KEY` | Derives the testnet domain `env_uri`. | Identity — same warning as #5. |
| 8 | F1r3drive wallet key | `F1R3DRIVE_WALLET_KEY` (entrypoint) | secp256k1 key the co-located F1r3drive uses to read/write the drive. | Rotatable at the F1r3drive level; the drive's own cipher-key **file** must persist (losing it makes drive content undecryptable). |

The per-process **JWT secret** is *not* in this table because it is never
configured: embers generates a fresh random one at boot and never persists it (see
[boot and runtime](../architecture/boot-and-runtime.md)). A restart rotates it
automatically, harmlessly invalidating outstanding `prepare` tokens.

## Environment keys are identities — do not rotate casually

Classes #5 and #7 are different in kind from the others. A domain's `env_uri` is
derived from its environment **public** key (see
[environment registry](../design/environment-registry.md)). Rotating an environment
key therefore **changes the domain's on-chain address**: all agents, teams, OSLFs,
wallets, or runs registered under the old `env_uri` become orphaned, invisible to
the new key. Treat an environment key as a permanent identity, back it up, and
rotate it only as a deliberate migration — never as routine hygiene.

The service keys (#4, #6) and the store keys (#2, #3) are ordinary rotatable
secrets by comparison, subject to the drain/re-encryption caveats noted in the
table.

## Finding: plaintext secrets in the staging compose

`docker/docker-compose-staging.yaml` historically embedded **real** secret values
directly in the committed `environment:` block — the AES key, all four mainnet
environment keys, both service keys, and the testnet environment key. Committing
secrets exposes them to everyone with repository access and to the full git history,
and — because environment keys are identities (above) — a leaked environment key
cannot simply be rotated without orphaning the domain's on-chain data.

**Remediation applied** (see [Docker guide](../operations/docker.md)):

- The staging compose no longer contains any secret value. Secrets are supplied via
  an **untracked** `env_file` (`docker/embers-staging.secrets.env`), with a committed
  `docker/embers-staging.secrets.env.example` documenting every required key with
  placeholder values. The real file is `.gitignore`d.
- Non-secret topology (node URLs, address/port, F1r3drive hosts and mount root) stays
  inline in the committed compose.
- The compose was also brought up to date with the current binary: it now supplies
  the required `EMBERS__CONTINUATION_STORE__*` variables and the co-located
  F1r3drive setup so it can actually boot (it previously could not).

**Remediation still required by an operator** (cannot be done in the repository):

- The secret values that were committed are **compromised** and must be **rotated**
  out of use — with the environment keys, that means treating the affected
  mainnet/testnet domains as burned and re-provisioning under fresh keys, since the
  data under the old `env_uri` cannot be migrated by a key change alone.
- Consider purging the values from history (e.g. `git filter-repo`) if the
  repository is or was shared; note this rewrites history and is a coordinated
  action, out of scope for the in-repo fix.
