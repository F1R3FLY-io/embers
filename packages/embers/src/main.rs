use anyhow::Context;
use firefly_client::{NodeEvents, ReadNodeClient, WriteNodeClient};
use poem::listener::TcpListener;
use poem::middleware::{Compression, Cors, NormalizePath, RequestId, Tracing, TrailingSlash};
use poem::{EndpointExt, Route, Server};
use poem_openapi::OpenApiService;
use secp256k1::rand;
use secp256k1::rand::distr::{Alphanumeric, SampleString};
use tokio::try_join;

use crate::api::agents::AgentsApi;
use crate::api::agents_teams::AgentsTeamsApi;
use crate::api::oslfs::OslfsApi;
use crate::api::service::ServiceApi;
use crate::api::testnet::TestnetApi;
use crate::api::wallets::WalletsApi;
use crate::configuration::collect_config;
use crate::domain::agents::AgentsService;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::oslfs::OslfsService;
use crate::domain::testnet::TestnetService;
use crate::domain::wallets::WalletsService;

mod api;
mod blockchain;
mod configuration;
mod domain;

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

    let read_client = ReadNodeClient::new(config.mainnet.observer_url);
    let validator_node_events = NodeEvents::new(&config.mainnet.validator_ws_api_url);
    let observer_node_events = NodeEvents::new(&config.mainnet.observer_ws_api_url);

    let testnet_read_client = ReadNodeClient::new(config.testnet.observer_url);
    let _testnet_validator_node_events = NodeEvents::new(&config.testnet.validator_ws_api_url);
    let testnet_observer_node_events = NodeEvents::new(&config.testnet.observer_ws_api_url);

    let ((agents_service, agents_teams_service, oslfs_service, wallets_service), testnet_service) =
        try_join!(
            async {
                let mut write_client = WriteNodeClient::new(
                    config.mainnet.deploy_service_url,
                    config.mainnet.propose_service_url,
                )
                .await?;

                let agents_service = AgentsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.agents_env_key,
                )
                .await?;

                let agents_teams_service = AgentsTeamsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    observer_node_events.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.agents_teams_env_key,
                    config.aes_encryption_key.into(),
                )
                .await?;

                let oslfs_service = OslfsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.oslfs_env_key,
                )
                .await?;

                let wallets_service = WalletsService::bootstrap(
                    write_client.clone(),
                    read_client,
                    validator_node_events,
                    observer_node_events,
                    &config.mainnet.service_key,
                    &config.mainnet.wallets_env_key,
                )
                .await?;

                write_client.propose().await?;

                anyhow::Ok((
                    agents_service,
                    agents_teams_service,
                    oslfs_service,
                    wallets_service,
                ))
            },
            async {
                let mut testnet_write_client = WriteNodeClient::new(
                    config.testnet.deploy_service_url,
                    config.testnet.propose_service_url,
                )
                .await?;

                let testnet_service = TestnetService::bootstrap(
                    testnet_write_client.clone(),
                    testnet_read_client,
                    testnet_observer_node_events,
                    config.testnet.service_key,
                    &config.testnet.env_key,
                )
                .await?;

                testnet_write_client.propose().await?;

                anyhow::Ok(testnet_service)
            },
        )?;

    let secret = Alphanumeric.sample_string(&mut rand::rng(), 20);

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

    let ui = api.swagger_ui();
    let spec = api.spec_endpoint();
    let spec_yaml = api.spec_endpoint_yaml();

    let routes = Route::new()
        .nest("/api", api)
        .nest("/swagger-ui/index.html", ui)
        .nest("/swagger-ui/openapi.json", spec)
        .nest("/swagger-ui/openapi.yaml", spec_yaml)
        .data(jsonwebtoken::EncodingKey::from_secret(secret.as_ref()))
        .data(jsonwebtoken::DecodingKey::from_secret(secret.as_ref()))
        .data(agents_service)
        .data(agents_teams_service)
        .data(oslfs_service)
        .data(wallets_service)
        .data(testnet_service)
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
