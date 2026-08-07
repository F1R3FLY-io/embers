# embers Python tests

The Python suite drives the embers HTTP API end to end. It is the **L2/L3** face of
the [test architecture](../../../docs/testing/test-architecture.md) — the Rust
`cargo make test` suite is the hermetic **L1** layer; these tests exercise a running
embers against a real (loopback) dev-net or a remote deployment.

## Running

```sh
poetry install
poetry run pytest            # collects and runs only what the environment supports
```

By default `addopts` excludes `remote`, and session-cached service probes **skip**
any case whose backing service (dev-net, PDS, Chroma, the GPT stub, a recoverable
embers unit) is not up — so a bare run never fails merely because a dependency is
absent. Point the suite at a specific backend with `EMBERS_API_URL` (default
`[::1]:8080`).

## Markers

Registered in `pyproject.toml`; applied per module/case and auto-applied to
`tests/api/**`:

| Marker | Selects |
| ------ | ------- |
| `devnet` | Needs a running embers + node (all `api/**`). |
| `pds` | Needs a PDS (the F1r3sky publish/unpublish/reveal modules). |
| `chroma` | Needs Chroma (the research-desk demo). |
| `gpt` | Needs the local OpenAI stub. |
| `recovery` | Restarts embers mid-run to prove durable recovery. |
| `slow` | Long-running (broadcast/fan-in, red-team). |
| `remote` | **L3** — excluded by default; double-gated on `EMBERS_L3_CONFIRM=1` **and** a non-loopback `EMBERS_API_URL`, so it is structurally unrunnable against loopback. |

Select a layer with the usual `-m` expressions, e.g. `pytest -m "devnet and not
remote"` for L2. See [CI and tooling](../../../docs/operations/ci-and-tooling.md) and
[local stack](../../../docs/operations/local-stack.md).
