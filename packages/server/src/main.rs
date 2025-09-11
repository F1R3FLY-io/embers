use anyhow::Context;
use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::listener::TcpListener;
use poem::middleware::{Compression, Cors, NormalizePath, RequestId, Tracing, TrailingSlash};
use poem::{EndpointExt, Route, Server};
use poem_openapi::OpenApiService;
use tokio::try_join;

use crate::ai_agents::api::AIAgents;
use crate::ai_agents_teams::api::AIAgentsTeams;
use crate::bootstrap::{bootstrap_mainnet_contracts, bootstrap_testnet_contracts};
use crate::common::api::TestNet;
use crate::configuration::collect_config;
use crate::testnet::api::Testnet;
use crate::wallets::api::WalletsApi;

mod ai_agents;
mod ai_agents_teams;
mod bootstrap;
mod common;
mod configuration;
mod testnet;
mod wallets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = collect_config().context("failed to read configuration")?;

    let env_filter = tracing_subscriber::EnvFilter::try_new(config.log_level)
        .context("failed to init log filter")?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .event_format(
            tracing_subscriber::fmt::format()
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    let read_client = ReadNodeClient::new(config.mainnet.read_node_url);
    let testnet_read_client = ReadNodeClient::new(config.testnet.read_node_url);

    let (write_client, testnet_write_client) = try_join!(
        async {
            let mut write_client = WriteNodeClient::new(
                config.mainnet.deploy_service_url,
                config.mainnet.propose_service_url,
            )
            .await?;

            bootstrap_mainnet_contracts(&mut write_client, &config.mainnet.service_key).await?;

            anyhow::Ok(write_client)
        },
        async {
            let mut testnet_write_client = WriteNodeClient::new(
                config.testnet.deploy_service_url,
                config.testnet.propose_service_url,
            )
            .await?;

            bootstrap_testnet_contracts(&mut testnet_write_client, &config.testnet.service_key)
                .await?;

            anyhow::Ok(testnet_write_client)
        },
    )?;

    let api = OpenApiService::new(
        (Testnet, WalletsApi, AIAgents, AIAgentsTeams),
        "Embers API",
        "0.1.0",
    )
    .url_prefix("/api");

    let ui = api.swagger_ui();
    let spec = api.spec_endpoint();
    let spec_yaml = api.spec_endpoint_yaml();

    let routes = Route::new()
        .nest("/api", api)
        .nest("/swagger-ui/index.html", ui)
        .nest("/swagger-ui/openapi.json", spec)
        .nest("/swagger-ui/openapi.yaml", spec_yaml)
        .data(read_client)
        .data(write_client)
        .data(TestNet(testnet_read_client))
        .data(TestNet(testnet_write_client))
        .data(TestNet(config.testnet.service_key))
        .with(Cors::new().allow_origin_regex("*"))
        .with(RequestId::default())
        .with(Tracing)
        .with(Compression::default())
        .with(NormalizePath::new(TrailingSlash::Trim));

    Server::new(TcpListener::bind((config.address, config.port)))
        .run_with_graceful_shutdown(
            routes,
            async move {
                let _ = tokio::signal::ctrl_c()
                    .await
                    .inspect_err(|err| tracing::warn!("ctrl_c error: {err:?}"));
            },
            None,
        )
        .await?;

    Ok(())
}
