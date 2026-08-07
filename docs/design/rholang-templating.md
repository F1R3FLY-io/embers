# Rholang templating

Everything Embers deploys is Rholang, and Embers never hand-concatenates it.
A Rust struct is turned into a Rholang program by two derives working together:
`Render` (an Askama template, filled at compile time) and `IntoValue` (which
lowers a Rust value into an escaped Rholang literal). This document explains the
pipeline, the value model, and the escaping discipline that keeps rendered
Rholang injection-safe.

Implementations: `packages/firefly-client-macros/src/{render,into_value}.rs`
(the derives) and `packages/firefly-client/src/rendering.rs` (the `Value` model,
the `Render`/`IntoValue` traits, and the escaper).

## The pipeline

```text
Rust struct  --#[derive(Render)] #[template(path = "…rho")]-->  Askama template
     |                                                                |
     |  each non-`direct` field lowered by IntoValue                  |  compiled into the
     v                                                                v  binary at build time
Rholang Value literals  ------------------------------------>  rendered Rholang  -->  deploy
```

A `Render`-deriving struct names a template file; at build time the derive emits
an Askama `Template` whose fields are the struct's, each converted through
`IntoValue` unless marked `direct`. `Render::render()` produces the final Rholang
string. Because templates are compiled into the binary, a template that
references a missing field fails the build, and — via the
[parser oracle](../testing/test-architecture.md) — a template that renders
syntactically invalid Rholang fails a test rather than a live deploy.

## The value model

`IntoValue` lowers a Rust value to a `rendering::Value`, whose `Display` is
Rholang syntax:

| `Value` | Renders as | Notes |
| ------- | ---------- | ----- |
| `Nil` | `Nil` | |
| `Bool(b)` | `true` / `false` | |
| `Int(n)` | `n` | bare integer |
| `String(s)` | `"…"` | quoted, with the escaping below |
| `Bytes(b)` | `"<hex>".hexToBytes()` | |
| `Uri(u)` | `` `u` `` | backtick-quoted name |
| `Inline(s)` | `s` | rendered verbatim — an escape hatch for pre-formed Rholang |
| `Tuple` / `List` / `Set` / `Map` | `(…)` / `[…]` / `Set(…)` / `{…}` | elements/entries lowered recursively; map keys are quoted strings |

`#[derive(IntoValue)]` supports structs only: a named struct becomes a `Map`
(field name → lowered value), a multi-field tuple struct becomes a `Tuple`, a
newtype passes through to its inner value, and a unit struct becomes the empty
tuple. Deriving it on an enum is a compile error (checked by the trybuild suite).

## Escaping discipline

String values are the injection surface, and the escaper is deliberately tiny and
total:

```text
escape_rho_string(s) = s.replace('\\', "\\\\").replace('"', "\\\"")
```

Backslashes are doubled first, then quotes are escaped — so the transform is
invertible (`unescape ∘ escape = identity`) and injective. Those two properties
are not asserted by inspection; they are
[property-tested](../../packages/firefly-client/src/rendering.rs) over arbitrary
strings (campaign step A2): a value embedded in a Rholang double-quoted literal is
preserved exactly and unambiguously, so no crafted string can break out of its
literal. The Foreign-Language-Term machinery adds a further template-injection
defense by excluding the backtick delimiter from FLT bodies.

## Direct vs lowered fields

A `Render` field is lowered through `IntoValue` by default (so a `String` field
renders as a quoted Rholang string). Marking a field `#[template(direct = true)]`
keeps its own type and renders it verbatim — used when the field already holds
formed Rholang (an `Inline`, or a pre-rendered fragment) that must not be
re-quoted.

## Worked example

The wallet boost contract is a `Render` struct against `templates/wallets/boost.rho`:

```rust
#[derive(Debug, Clone, Render)]
#[template(path = "wallets/boost.rho")]
struct BoostContract {
    env_uri: Uri,
    timestamp: DateTime<Utc>,
    wallet_address_from: WalletAddress,
    wallet_address_to: WalletAddress,
    amount: i64,
    description: Option<String>,
    post_author_did: String,
    post_id: Option<String>,
}
```

Each field lowers through `IntoValue` (the `Uri` to a backtick name, the
addresses to their base58 string literals, the `i64` bare, the `Option<String>`
to a quoted string or `Nil`), and the template splices the results into the
boost contract's Rholang. The templates themselves are catalogued in
[`templates/README.md`](../../packages/embers/templates/README.md).

## How it is tested

- **The derives** — a [trybuild suite](../../packages/firefly-client-macros/tests/ui.rs)
  pins that valid shapes compile and each unsupported shape (an `IntoValue` enum,
  a `Render` tuple struct, a `Render` enum variant missing its path) is rejected
  with a stable diagnostic; a [render integration test](../../packages/firefly-client-macros/tests/render_derive.rs)
  checks the rendered output for both field modes (campaign step A3).
- **The escaper** — the round-trip and injectivity properties above (A2).
- **The whole corpus** — every `Render` struct is rendered with fixture data and
  parsed under the real Rholang grammar by the
  [oracle](../testing/test-architecture.md) (A5/A9).

## Related

- [Environment registry](environment-registry.md) — the init templates every
  domain shares.
- [Crate architecture](../architecture/crate-architecture.md) — where the derives
  live.
