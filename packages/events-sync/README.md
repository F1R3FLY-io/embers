# events-sync

A CLI that bridges the AT Protocol firehose and Firefly (f1r3node), in both
directions. It lets on-chain activity appear as an ATProto sync stream, and
lets an external ATProto firehose be recorded on chain.

It is a standalone binary, configured by command-line flags (not the `EMBERS__`
environment). The three global flags identify the actor and the target node:

| Flag | Meaning |
| ---- | ------- |
| `--wallet-key` | The wallet key (hex) that signs this service's deploys. |
| `--deploy-service-url` | The Firefly validator's deploy service URL. |
| `--service-id` | A globally-unique identifier for this service's on-chain channels. |

## Subcommands

- **`listen`** — serve Firefly's events outward. It runs two servers: a gRPC
  communication service (which the node calls to notify it of updates) and an
  ATProto sync API implementing `com.atproto.sync.subscribeRepos`, so ATProto
  consumers can subscribe to the chain-sourced repo stream. `--extra-sources`
  merges additional upstream `subscribeRepos` firehoses into the served stream.
  (Flags: the two serve addresses, the externally-reachable host, and the extra
  sources — see `events-sync listen --help`.)
- **`push`** — record an external firehose on chain. It subscribes to an ATProto
  `subscribeRepos` URL and deploys the events to Firefly in windows, flushing
  when either bound is reached:

  ```math
  \text{flush} \iff (\text{now} - t_{\text{start}} \ge \Theta_t) \;\lor\; (|W| \ge \Theta_n)
  ```

  (Flags: the sync URL to listen on, the time threshold in seconds, and the size
  threshold — see `events-sync push --help`. The Docker `multiple-networks`
  profile runs it with `time-threshold=3`, `size-threshold=16`.)
- **`init`** — deploy this service's listener/notifier registry contract to the
  chain (the contract that `listen` and `push` coordinate through). Run once per
  `--service-id`.

## Where it runs

The Docker `multiple-networks` compose profile wires `events-init`,
`events-listen`, and `events-push` together against the local node and a host
PDS; see the [Docker guide](../../docs/operations/docker.md).

Build and run with `cargo run -p events-sync -- <subcommand> …`.
