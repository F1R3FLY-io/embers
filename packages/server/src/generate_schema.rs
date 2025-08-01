mod ai_agents;
mod common;
mod wallets;

use ai_agents::api::AIAgents;
use poem_openapi::OpenApiService;
use wallets::api::WalletsApi;

#[tokio::main]
async fn main() {
    let api = OpenApiService::new((WalletsApi, AIAgents), "Embers API", "0.1.0").url_prefix("/api");

    let _ = std::fs::write("schema.json", api.spec());
}
