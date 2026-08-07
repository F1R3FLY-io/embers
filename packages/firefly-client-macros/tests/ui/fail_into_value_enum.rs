use firefly_client_macros::IntoValue;

// IntoValue supports only structs; an enum must be rejected at compile time.
#[derive(IntoValue)]
enum Unsupported {
    A,
    B(String),
}

fn main() {}
