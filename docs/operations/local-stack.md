# Local stack (dev-net)

`embers-local-stack` is a sibling repository whose `stack.sh` brings up a complete
**loopback** F1r3fly dev-net — a native F1r3fly node, a F1r3drive mount, PDS,
Postgres/valkey, and embers itself — so the backend can be exercised end to end
without any remote infrastructure. This is the substrate the **L2** test layer runs
against (see [test architecture](../testing/test-architecture.md)); everything binds
loopback, so no traffic leaves the machine.

This document is the `stack.sh` runbook. The stack lives outside this repository
(`../embers-local-stack/`); embers only needs to know how to talk to it.

## Topology

![local stack topology](../diagrams/27-local-stack-topology.svg)

| Component | Endpoint (loopback) | Managed as |
| --------- | ------------------- | ---------- |
| Validator node | gRPC `:40401`, HTTP `:40403` | systemd `--user` unit |
| Read-node / observer | HTTP `:40413` | systemd `--user` unit |
| F1r3drive | FUSE mount | systemd `--user` unit |
| embers | `[::1]:8080` | systemd `--user` unit |
| PDS | `:2583` | process |
| Postgres | `:5432` | systemd-managed |
| valkey | `:6379` | systemd-managed |
| Chroma (optional) | `:8000` | process |

The native node binary is built out of the sibling `../f1r3node-rust/` checkout.
embers is built from this repository with the `gxhash` target features
(`RUSTFLAGS="-C target-feature=+aes,+sse2"`).

## Subcommands

`stack.sh <subcommand>`:

| Subcommand | Does |
| ---------- | ---- |
| `wallet` | Set up / inspect the demo wallet. |
| `nodes` / `repair-nodes` | Bring up (or repair) the validator + read-node units. |
| `serve-validator` / `serve-read-node` | The unit entrypoints (re-exec'd by systemd; not run by hand). |
| `drive` / `serve-drive` | Mount F1r3drive (and its unit entrypoint). |
| `embers` / `start-embers` | Build and run embers against the local node + drive. |
| `backend` | Bring up the backend services together. |
| `web` / `stop-web` | Run / stop the web frontend. |
| `up` | Bring up the whole stack in order. |
| `status` | Show every unit's state and the bound ports. |
| `truncate` | Truncate the on-disk state (fresh chain). |
| `reset` | Tear down and re-initialize. |
| `stop` | Stop all stack units. |

The usual first run is `stack.sh up`; `stack.sh status` shows what is live;
`stack.sh stop` tears it down.

## Secrets and the OpenAI key

`stack.sh` sources an untracked `secrets.env` (copied from `secrets.env.example`)
and requires `CHATGPT_EMBERS_DEMO_API_KEY` to be set. The **node**, not embers,
holds and uses the OpenAI key — embers never calls OpenAI directly (the node's
`rho:ai:*` system channels do).

For a hermetic **L2** run the node's `OPENAI_API_BASE` is pointed at the local
[OpenAI stub](../testing/test-architecture.md) by adding an
`export OPENAI_API_BASE=http://127.0.0.1:8571/v1` line to `secrets.env` (the node
re-sources it), so even the model calls stay on loopback and the machine needs no
real GPT-4 access. This is the mechanism behind the local-only mandate: nothing in
L1 or L2 reaches an external host.

## Systemd unit naming

Units are user-scoped (`systemd-run --user`) and named per **run directory** so
multiple stacks can coexist: `stack.sh` computes `STACK_PROFILE_ID` as the
`sha256sum` of the normalized run directory (`EMBERS_STACK_RUN_DIR`, default
`<stack>/run`) and embeds a 12-char prefix of it in each unit name (validator,
read-node, F1r3drive, watchdog, embers). `stack.sh status` resolves and prints
them; to drive one directly, read the unit name from `status` rather than guessing.

## Relationship to embers

- embers is configured to reach the local node's three endpoints and the F1r3drive
  mount through its normal `EMBERS__…` environment; see
  [configuration](configuration.md).
- The [F1r3drive mount contract](../integrations/f1r3drive.md) is satisfied by
  `stack.sh drive`, which mounts a real FUSE `fsname=f1r3drive` filesystem.
- L2 orchestration (building embers + the stub, ensuring the stub uptake, running
  the `devnet`-marked pytest suite) is layered on top of `stack.sh`; see
  [test architecture](../testing/test-architecture.md).
