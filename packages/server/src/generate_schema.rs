mod ai_agents;
mod ai_agents_teams;
mod common;
mod testnet;
mod wallets;

use ai_agents::api::AIAgents;
use ai_agents_teams::api::AIAgentsTeams;
use poem_openapi::OpenApiService;
use testnet::api::Testnet;
use wallets::api::WalletsApi;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let api = OpenApiService::new(
        (Testnet, WalletsApi, AIAgents, AIAgentsTeams),
        "Embers API",
        "0.1.0",
    )
    .url_prefix("/api");
    std::fs::write("schema.json", api.spec())
}
