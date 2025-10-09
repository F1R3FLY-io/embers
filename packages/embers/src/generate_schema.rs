mod ai_agents;
mod ai_agents_teams;
mod common;
mod testnet;
mod wallets;

use poem_openapi::OpenApiService;

use crate::ai_agents::api::AIAgents;
use crate::ai_agents_teams::api::AIAgentsTeams;
use crate::common::api::Service;
use crate::testnet::api::Testnet;
use crate::wallets::api::WalletsApi;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let api = OpenApiService::new(
        (Service, Testnet, WalletsApi, AIAgents, AIAgentsTeams),
        "Embers API",
        "0.1.0",
    )
    .url_prefix("/api");
    std::fs::write("schema.json", api.spec())
}
