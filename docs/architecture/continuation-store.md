# The continuation store

The continuation store is where a durable Agent-Teams run keeps its private
state between steps: the encrypted, owner-isolated, content-addressed object
store on the F1r3drive FUSE mount. The on-chain run ledger holds only hashes and
pointers; the actual bytes — continuations, effect outboxes, observations,
outputs — live here. This document specifies its object format, its capability
scheme, and the guarantees each mechanism provides.

Implementation: `packages/embers/src/domain/agents_teams/continuation_store.rs`
(`F1r3driveContinuationStore`).

## What it stores, and the six artifact kinds

Every object is filed under one of six `ArtifactKind`s, each with a stable
directory name:

| Kind | Directory | Holds |
| ---- | --------- | ----- |
| `Command` | `commands/` | A signed, revision-fenced resume command awaiting application. |
| `Continuation` | `continuations/` | A serialized kernel continuation (the durable checkpoint of a run). |
| `EffectOutbox` | `effect-outbox/` | An effect request recorded before its activity is dispatched. |
| `Observation` | `observations/` | A signed, content-addressed activity result. |
| `Output` | `outputs/` | A run's final output value, released only by owner reveal. |
| `RunRequest` | `run-requests/` | The original durable-run request. |

## Why a FUSE mount

The store's directory must sit inside a **FUSE mount whose fsname contains
`f1r3drive`**; `embers` refuses to start otherwise. At open time
`verify_f1r3drive_mount` parses `/proc/self/mountinfo` and walks from the
configured path up to its mount point, checking the filesystem type and fsname.
This ties the private state to F1r3drive's chain-backed, encrypted storage rather
than an ordinary local directory, and it enforces owner isolation (a per-wallet
subdirectory appears only when that wallet is unlocked). The
`#[cfg(test)] open_for_test` constructor bypasses the check so unit tests can run
against a `tempfile` directory — the only place the mount requirement is relaxed,
and never in production. The mount contract is documented in full in the
[F1r3drive integration guide](../integrations/f1r3drive.md).

## Object format

Each stored file is:

```text
MAGIC (16 bytes)  ‖  nonce (12 bytes)  ‖  AES-256-GCM ciphertext
```

A fixed magic header identifies and versions the format; the 12-byte nonce is
freshly random per write; the ciphertext is AES-256-GCM under the store's
encryption key. The AEAD **additional authenticated data (AAD)** binds the
ciphertext to the object's full identity — its `kind`, `run_id`, `object_id`, and
`capability_hash` — so a ciphertext cannot be lifted from one identity and
replayed under another: decryption under a mismatched identity fails
authentication. AES-GCM's construction is specified in
[NIST SP 800-38D](https://doi.org/10.6028/NIST.SP.800-38D).

## Capabilities: unforgeable names

An object is addressed not by a guessable path but by a **capability** derived
with a keyed hash:

```math
\mathrm{cap} \;=\; \mathrm{HMAC\text{-}SHA256}\bigl(k_{\mathrm{cap}},\; \mathit{kind} \,\Vert\, \texttt{0x00} \,\Vert\, \mathit{run\_id} \,\Vert\, \texttt{0x00} \,\Vert\, \mathit{object\_id}\bigr),
```

hex-encoded, where `$`k_{\mathrm{cap}}`$` is the store's capability key. The file
lives at a sharded path `$`\mathit{kind}/\mathrm{cap}[0{:}2]/\mathrm{cap}.\texttt{encrypted}`$`.
Because HMAC is unforgeable without `$`k_{\mathrm{cap}}`$`
([RFC 2104](https://doi.org/10.17487/RFC2104),
[FIPS 198-1](https://doi.org/10.6028/NIST.FIPS.198-1)), a party lacking the
capability key cannot even *name* — let alone read — an object (INV-S4). This is
an object-capability discipline: possession of the derived name is the authority
to address the object.

The on-chain pointer stores the **hash of the capability**, not the capability
itself:

```math
\mathit{capability\_hash} \;=\; \mathrm{SHA256}(\mathrm{cap}),
```

([FIPS 180-4](https://doi.org/10.6028/NIST.FIPS.180-4)) so the public ledger
never reveals a usable capability. The two store keys must differ (a boot-time
invariant, see [Configuration](../operations/configuration.md)): the encryption
key protects contents, the capability key derives names; conflating them would
put name-derivation material and content-protection material in one secret.

## Writing: content-addressed and immutable

`put(kind, run_id, object_id, value)` (literate form):

```text
put(kind, run_id, object_id, value):
    validate_identity(run_id); validate_identity(object_id)   # non-empty, printable, <=512 bytes, no NUL/control
    plaintext := serialize(value)
    require |plaintext| <= max_object_bytes
    cap  := HMAC-SHA256(k_cap, kind ‖ 0x00 ‖ run_id ‖ 0x00 ‖ object_id)
    path := kind/cap[0:2]/cap.encrypted
    aad  := AAD(kind, run_id, object_id, SHA256(cap))
    with write_lock:
        if path exists:
            existing := decrypt_and_verify(path, aad)
            require existing == plaintext          # idempotency conflict -> error
            return pointer_for(existing)           # idempotent re-write is a no-op
        nonce := random_96_bits()
        write(path, MAGIC ‖ nonce ‖ AES-256-GCM(k_enc, nonce, plaintext, aad))
        return pointer_for(plaintext)
```

Two properties fall out. **Idempotency**: writing the same bytes to the same
identity twice succeeds and returns the same pointer. **Immutability under a
stable identity** (INV-S5): writing *different* bytes to an identity that already
holds a value is rejected — the store is content-addressed, so an identity binds
to exactly one value for its lifetime.

## Reading: three integrity gates

`get(pointer, run_id, object_id)` rejects unless all three hold: the pointer's
`run_id`/`object_id` match the arguments; the pointer's `capability_hash` matches
the one recomputed from the identity (so a pointer minted under a different
capability key is refused before any file access, INV-S4); and the on-disk
bytes' SHA-256 matches the pointer's `content_hash`. Only then is the ciphertext
AEAD-decrypted under the AAD. Any single-byte corruption of the stored file
changes its content hash and fails the third gate before decryption; a modified
ciphertext or a mismatched identity fails the AEAD tag — either way the read
errors rather than returning wrong bytes (INV-S6).

## Failure modes

| Situation | Result |
| --------- | ------ |
| Store path not on an `f1r3drive` FUSE mount | Start-up refuses (`verify_f1r3drive_mount`). |
| Same identity, different bytes | `put` errors (idempotency conflict). |
| Corrupted file (any byte) | `get` errors on the content-hash gate. |
| Wrong capability key | `get` errors on the capability-hash gate (cannot address the object). |
| Wrong encryption key | `get` errors on AEAD authentication. |
| Identity with NUL / control chars / > 512 bytes | `put` rejects at `validate_identity`. |

## How this is checked

The [`continuation_store` property tests](../../packages/embers/src/domain/agents_teams/continuation_store.rs)
exercise these guarantees over arbitrary JSON values: put∘get round-trips,
identity-reuse rejection (INV-S5), capability-key and encryption-key isolation
(INV-S4), and single-byte tamper detection (INV-S6). See the
[invariant registry](../formal/INVARIANTS.md) for the INV-S4…S6 rows and the
planned Tamarin model `continuation_capabilities.spthy`.

## Related

- [F1r3drive integration](../integrations/f1r3drive.md) — the FUSE mount contract.
- [Configuration](../operations/configuration.md) — the two store keys and the
  size/poll settings.
- [Security model](../security/security-model.md) — where the store sits among
  the platform's authorities.
