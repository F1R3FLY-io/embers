#![allow(
    dead_code,
    reason = "the schema generator intentionally imports the complete API service graph"
)]

mod api;
mod app;
mod blockchain;
mod domain;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    std::fs::write("schema.json", app::openapi_spec_json())
}
