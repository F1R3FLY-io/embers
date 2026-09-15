use firefly_client_macros::Render;

// Render supports named structs, unit structs, and enums — but not tuple
// structs, which have no field names to bind template variables to.
#[derive(Render)]
#[template(path = "named.txt")]
struct TupleShape(String);

fn main() {}
