use ai_agents::models::InitAgentsEnv;
use anyhow::Context;
use askama::Template;
use common::bootstrap_contracts;
use firefly_client::{BlocksClient, ReadNodeClient, WriteNodeClient};
use poem::listener::TcpListener;
use poem::middleware::{Compression, Cors, NormalizePath, RequestId, Tracing, TrailingSlash};
use poem::{EndpointExt, Route, Server};
use poem_openapi::OpenApiService;
use tracing::trace;
use wallets::models::InitWalletEnv;

use crate::ai_agents::api::AIAgents;
use crate::configuration::collect_config;
use crate::wallets::api::WalletsApi;

mod ai_agents;
mod common;
mod configuration;
mod wallets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = collect_config().context("can't read bootstrap configuration")?;

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

    let read_client = ReadNodeClient::new(config.read_node_url.clone());
    let mut write_client =
        WriteNodeClient::new(config.deploy_service_url, config.propose_service_url).await?;
    let blocks_client = BlocksClient::new(config.read_node_url);

    let contracts = vec![
        InitAgentsEnv
            .render()
            .expect("Can't render InitAgentsEnv bootstrap code"),
        InitWalletEnv
            .render()
            .expect("Can't render InitWalletEnv bootstrap code"),
    ];
    let _ = bootstrap_contracts(&mut write_client, &config.service_key, contracts).await;

    let api = OpenApiService::new((WalletsApi, AIAgents), "Embers API", "0.1.0").url_prefix("/api");

    let ui = api.swagger_ui();
    let spec = api.spec_endpoint();

    let routes = Route::new()
        .nest("/api", api)
        .nest("/swagger-ui/index.html", ui)
        .nest("/swagger-ui/openapi.json", spec)
        .data(read_client)
        .data(write_client)
        .data(blocks_client)
        .with(Cors::new().allow_origin_regex("*"))
        .with(RequestId::default())
        .with(Tracing)
        .with(Compression::default())
        .with(NormalizePath::new(TrailingSlash::Trim));

    let port = config.port;
    let address = format!("::1:{port}");

    Server::new(TcpListener::bind(address))
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
