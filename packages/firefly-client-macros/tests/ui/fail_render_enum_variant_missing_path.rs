use firefly_client_macros::Render;

// Every enum variant must carry its own template path; the second variant omits
// it and must be rejected.
#[derive(Render)]
enum Command {
    #[template(path = "named.txt")]
    First { label: String },
    Second { label: String },
}

fn main() {}
