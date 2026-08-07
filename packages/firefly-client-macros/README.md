# firefly-client-macros

Two derive macros that turn Rust types into Rholang, used throughout the Embers
backend so that no Rholang is ever hand-concatenated.

This crate is the compile-time half of the templating system; the runtime traits
and value model it targets live in `firefly-client` (`rendering.rs`). The full
design — the value model, the escaping discipline, and how it is tested — is in
[Rholang templating](../../docs/design/rholang-templating.md).

## `#[derive(IntoValue)]`

Lowers a struct into a `firefly_client::rendering::Value` (whose `Display` is
Rholang syntax). Supported shapes:

- a **named struct** becomes a `Value::Map` (field name → lowered value);
- a **newtype** (single-field tuple struct) passes through to its inner value;
- a **multi-field tuple struct** becomes a `Value::Tuple`;
- a **unit struct** becomes the empty tuple.

Deriving it on an enum is a compile error.

```rust
use firefly_client_macros::IntoValue;

#[derive(IntoValue)]
struct Transfer {
    from: String,   // -> a quoted Rholang string
    amount: i64,    // -> a bare integer
}
```

## `#[derive(Render)]`

Fills an Askama template with a struct's fields to produce a Rholang string. The
`#[template(path = "…")]` attribute names the template (resolved from the crate's
`templates/` directory at build time); each field is lowered through `IntoValue`
unless marked `#[template(direct = true)]`, in which case it is rendered
verbatim. `Render::render()` returns the rendered Rholang.

```rust
use firefly_client::rendering::Render;

#[derive(Render)]
#[template(path = "wallets/transfer.rho")]
struct TransferContract {
    env_uri: firefly_client::models::Uri,
    amount: i64,
    // …
}

let rholang = TransferContract { /* … */ }.render()?;
```

Supported shapes are named structs, unit structs, and enums (each enum variant
carries its own `#[template(path = …)]`). A tuple struct, or an enum variant
missing its path, is a compile error.

## Testing

- `tests/ui.rs` — a [trybuild](https://docs.rs/trybuild) suite pinning that valid
  shapes compile and each unsupported shape is rejected with a stable diagnostic
  (golden `.stderr` against the pinned toolchain).
- `tests/render_derive.rs` — the `Render` pass path against real Askama templates
  in `templates/`, asserting both the `IntoValue`-lowered and `direct` field
  modes render as expected.

Run with `cargo test -p firefly-client-macros`.
