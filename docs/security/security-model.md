# Security model

This document states what the Embers backend enforces, and — as importantly —
what it deliberately does not. The unifying idea is that **the backend is not the
trust root for anything that matters**: authorship is proved by client-side
signatures the chain re-verifies, and authority is carried by capabilities, not
by server-side sessions.

## The four authorities

Every protected action answers to exactly one of four authorities. They do not
substitute for one another.

| Authority | Grants | Enforced by |
| --------- | ------ | ----------- |
| **Wallet signature** | The right to perform a mutation (create, save, delete, deploy, transfer, boost, run). | The client signs the deploy with its wallet key (secp256k1 over BLAKE2b-256); the **chain** verifies the signature at deploy time. The HTTP layer is never the trust root for authorship. |
| **Run capability URL** | The right to read a run's metadata ledger. | Possession of the UUIDv7 run URL (`/runs/{run_id}`). It is a read capability only — it grants no command authority. |
| **On-chain command ledger** | The right to command a run (signal, approve/reject, retry, cancel, migrate). | A signed command, claimed against the run ledger with a revision compare-and-set; the kernel checks the actor's authorization class and, for approvals, membership in the declared approver set. |
| **Owner reveal** | The right to read a run's final output value. | The run's on-chain owner, adjudicated by the `claimOutputReveal` clause; the ledger scrubs `owner` from every ordinary read. |

## What the server can and cannot do

- **It never holds a user's key.** The private key is used only in the client;
  the server sees the public key and the signature. A compromised backend cannot
  forge a mutation, because the chain would reject an unsigned or wrongly-signed
  deploy.
- **Its session secret binds exchanges, not identity.** The two-phase
  prepare/send token is signed with a per-process random secret generated at
  start-up. It guarantees that a `send` belongs to exactly one prior `prepare`
  exchange; it is not an authentication credential. A backend restart invalidates
  outstanding prepares — harmless, because `prepare` deploys nothing and the
  client simply prepares again.

## Fail-closed inventory

Where a check could fail open, it is written to fail closed:

- **Output reveal** counts the on-chain acknowledgement before releasing the
  decrypted value; a missing ack yields no disclosure (INV-S3), and a replayed
  reveal hits the `Consumed` state rather than disclosing again (INV-S2).
- **Continuation store** rejects on any capability-hash, content-hash, or AEAD
  mismatch rather than returning bytes (INV-S4–S6; see
  [the continuation store](../architecture/continuation-store.md)).
- **Env registry health** treats a registry entry that reads back as `Nil` as a
  lost env and logs it, rather than serving from an env that silently failed to
  register.
- **Foreign-term resource bounds** are validated (positive `fuel`, `max_phlo`,
  `max_output_bytes`) at publish time; an out-of-range bound is rejected, not
  clamped.
- **Deploy commands** are revision-fenced: a command claimed at one ledger
  revision cannot apply at another (INV-D4).

## Transport

The service assumes TLS termination in front of it (the staging topology puts
nginx there). It performs no request throttling of its own; rate limiting and
abuse controls are an edge concern. Never expose the backend with
`--allow-private-addresses`-style node flags in production.

## Known hardening item: the two-phase binding hash

The prepare/send token binds the exchange by hashing the request/response pair,
but with a 64-bit **non-cryptographic** `DefaultHasher` (`api/common.rs`), signed
into the JWT. The JWT MAC still prevents forging the token, but the value it
protects is a weak 64-bit digest; a client that can shape both echoed payloads
could hunt collisions offline (~`$`2^{32}`$` by the birthday bound). The
recommended hardening is a 256-bit cryptographic hash (BLAKE2b-256, already a
dependency) over a canonical serialization. This is a **client-visible protocol
change** (the token's binding semantics), so it is tracked as a decision to be
sequenced with a client-regeneration window, not applied silently — see the
finding `embers-jwt-binding-noncrypto-hash`. The planned ProVerif model
`two_phase_jwt.pv` models the binding as it is (idealizing the weak hash) and
records this as its central assumption.

## Where the rest lives

- [Key inventory](key-inventory.md) — the secrets the service holds, their blast
  radius, and rotation.
- [The continuation store](../architecture/continuation-store.md) — the
  encryption and capability scheme.
- [Invariant registry](../formal/INVARIANTS.md) — INV-S1…S8 and their tests.
- The capability and threat models for agent teams are canonical in the frontend
  docs (`../../../embers-frontend/docs/agent-teams/security/`); this document is
  the embers-backend view.
