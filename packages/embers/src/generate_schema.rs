use poem_openapi::OpenApiService;

use crate::api::agents::AgentsApi;
use crate::api::agents_teams::AgentsTeamsApi;
use crate::api::oslfs::OslfsApi;
use crate::api::service::ServiceApi;
use crate::api::testnet::TestnetApi;
use crate::api::wallets::WalletsApi;

mod api;
mod blockchain;
mod domain;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let api = OpenApiService::new(
        (
            ServiceApi,
            TestnetApi,
            WalletsApi,
            AgentsApi,
            AgentsTeamsApi,
            OslfsApi,
        ),
        "Embers API",
        "0.1.0",
    )
    .url_prefix("/api");
    std::fs::write("schema.json", api.spec())
}
