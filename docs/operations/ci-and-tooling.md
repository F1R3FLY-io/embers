# CI and tooling

The build, test, lint, schema, and formal-verification tasks, and how they gate
changes. Tasks are `cargo-make` targets in the root `Makefile.toml` (plus
`packages/embers/Makefile.toml` for embers-only tasks).

## Build flags

The workspace uses `gxhash`, which needs AES/SSE2 on x86-64. Those flags are set in
`.cargo/config.toml`, so plain `cargo make test` / `cargo build` already carry them —
no manual `RUSTFLAGS` is needed for normal work. (The one place it is still spelled
out is subagent/CI invocations that bypass the config: `RUSTFLAGS="-C
target-feature=+aes,+sse2"`.)

## Task reference

| Task | Does |
| ---- | ---- |
| `cargo make test` | The full Rust suite (cargo-make default): unit + property + in-process harness tests, all hermetic/loopback (L1). |
| `cargo make clippy` | Clippy across all targets. |
| `cargo make format` / `fmt` / `format-check` | rustfmt (apply / alias / check-only). |
| `cargo make audit` | `cargo audit` dependency scan. |
| `cargo make generate-schema` | Regenerate the OpenAPI `schema.json` from the endpoint tuple. |
| `cargo make oracle-test` | Render the template corpus and parse it under the real Rholang grammar (the [oracle](../testing/test-architecture.md)). |
| `cargo make fv-{smt,tlc,proverif,storm,mcrl2,rocq}` | Individual [formal-verification](../formal/README.md) tool tracks. |
| `cargo make fv` | The FV aggregate — runs every track and fails loudly on an unexpected verdict. |
| `cargo make docker-up` / `docker-down` | Bring the Docker stack up / down. |

## The schema guard

The genericization of the API structs (campaign step A4) is required to be
schema-neutral, and endpoint changes must be intentional. The guard is:

```sh
cargo make generate-schema && git diff --exit-code schema.json
```

A non-empty diff means the served OpenAPI surface changed — either an intended API
change (commit the new `schema.json`) or an accident (revert). Run it after any
change to the API structs. `schema.json` is `.gitignore`d as a build artifact and
regenerated; the canonical surface is the endpoint tuple in `app.rs`.

## Formal verification in CI

`cargo make fv` drives all six validated tool tracks (SMT via z3+cvc5, TLA⁺ via TLC,
ProVerif, Storm, mCRL2, Rocq) against their recorded verdicts; the per-track scripts
live in `docs/formal/scripts/`. Every track fails the build on an unexpected verdict,
so a regression in a model or the code it abstracts is caught. See the
[formal-verification registry](../formal/README.md) for the model/verdict table and
the one tool (UPPAAL's `verifyta`) whose license does not cover this host (its
invariants are checked by a TLA⁺ substitute instead).

## Python suite

The `packages/embers/tests/` pytest suite is marker-layered (campaign step A1):
`addopts` excludes `remote` by default, and markers (`devnet`, `pds`, `chroma`,
`gpt`, `remote`, `recovery`, `slow`) plus session-cached service probes skip cases
whose backing service is not up — so a bare `pytest` run collects and runs only what
the current environment supports. The layering (L1 hermetic, L2 loopback dev-net, L3
remote-gated) is described in [test architecture](../testing/test-architecture.md).

## Recommended CI shape

- Rust: `cargo make format-check`, `cargo make clippy`, `cargo make test` (with
  `PROPTEST_CASES` raised, e.g. 64, for more property coverage), the schema guard,
  `cargo make audit`.
- Formal: `cargo make fv` (fast — well under the budgeted wall time after first
  builds).
- Oracle: `cargo make oracle-test`.
- Docs link/diagram checks are a planned `docs-check` task.
