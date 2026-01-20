use poem_openapi::OpenApiService;

use crate::api::ai_agents::AIAgents;
use crate::api::ai_agents_teams::AIAgentsTeams;
use crate::api::oslfs::OSLFS;
use crate::api::service::Service;
use crate::api::testnet::Testnet;
use crate::api::wallets::WalletsApi;

mod api;
mod blockchain;
mod domain;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let api = OpenApiService::new(
        (Service, Testnet, WalletsApi, AIAgents, AIAgentsTeams, OSLFS),
        "Embers API",
        "0.1.0",
    )
    .url_prefix("/api");
    std::fs::write("schema.json", api.spec())
}
