# state-sync

A CLI that checkpoints a PostgreSQL database onto Firefly (f1r3node) and restores
it — a way to keep a Postgres-backed service's state durable on chain. It is the
realized form of the "periodic dump sync" option from the PDS-to-Firefly
migration sketch.

It is a standalone binary configured by command-line flags. The three global
flags match `events-sync`:

| Flag | Meaning |
| ---- | ------- |
| `--wallet-key` | The wallet key (hex) that signs this service's deploys. |
| `--deploy-service-url` | The Firefly validator's deploy service URL. |
| `--service-id` | A globally-unique identifier for this service's on-chain channels. |

## Subcommands

- **`upload`** — snapshot the database to the chain on a fixed cadence. Every
  `--interval` seconds it runs `pg_dump` against `--db-url`, deploys the SQL dump
  to Firefly under a fresh channel, and records the dump's hash under this
  service's hash-list channel. `pg_dump`'s own snapshot semantics give each
  checkpoint a consistent point-in-time view.
- **`download --hash <block>`** — restore a snapshot. It reads this service's
  hash list at the given block hash and fetches the corresponding dump.
- **`init`** — deploy this service's on-chain contract (the hash-list channel the
  other two coordinate through). Run once per `--service-id`.

## Where it runs

The Docker `state-sync` compose profile runs `state-sync-init` then a
long-running `state-sync upload` against the local Postgres and node (a 300 s
interval by default); see the [Docker guide](../../docs/operations/docker.md).
This is the only PostgreSQL touchpoint in the workspace — `embers` itself uses no
database; its state is on chain or in the F1r3drive continuation store.

Build and run with `cargo run -p state-sync -- <subcommand> …`.
