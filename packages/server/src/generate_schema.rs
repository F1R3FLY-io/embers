mod ai_agents;
mod common;
mod wallets;

use ai_agents::api::AIAgents;
use poem::test::TestClient;
use poem_openapi::OpenApiService;
use wallets::api::WalletsApi;

#[tokio::main]
async fn main() {
    let api = OpenApiService::new((WalletsApi, AIAgents), "Embers API", "0.1.0");

    std::fs::write("schema.json", spec.spec())
}
