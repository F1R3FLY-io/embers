use std::time::Duration;

use anyhow::Context;
use firefly_client::models::Uri;
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
                let write_client = WriteNodeClient::new(config.mainnet.deploy_service_url).await?;

                let (agents_service, agents_deploy_id) = AgentsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    validator_node_events.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.agents_env_key,
                )
                .await?;

                let (agents_teams_service, agents_teams_deploy_id) = AgentsTeamsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    validator_node_events.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.agents_teams_env_key,
                    config.aes_encryption_key.into(),
                )
                .await?;

                let (oslfs_service, oslfs_deploy_id) = OslfsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    validator_node_events.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.oslfs_env_key,
                )
                .await?;

                let (wallets_service, wallets_deploy_id) = WalletsService::bootstrap(
                    write_client.clone(),
                    read_client.clone(),
                    validator_node_events.clone(),
                    observer_node_events.clone(),
                    &config.mainnet.service_key,
                    &config.mainnet.wallets_env_key,
                )
                .await?;

                tracing::info!(
                    agents = %agents_deploy_id,
                    agents_teams = %agents_teams_deploy_id,
                    oslfs = %oslfs_deploy_id,
                    wallets = %wallets_deploy_id,
                    "mainnet init deploys submitted, waiting for finalization"
                );

                let init_timeout = Duration::from_secs(60);
                let agents_waiter =
                    validator_node_events.wait_for_deploy(&agents_deploy_id, init_timeout);
                let agents_teams_waiter =
                    validator_node_events.wait_for_deploy(&agents_teams_deploy_id, init_timeout);
                let oslfs_waiter =
                    validator_node_events.wait_for_deploy(&oslfs_deploy_id, init_timeout);
                let wallets_waiter =
                    validator_node_events.wait_for_deploy(&wallets_deploy_id, init_timeout);

                // Wait for all init deploys to finalize
                let (a, at, o, w) = tokio::join!(
                    agents_waiter,
                    agents_teams_waiter,
                    oslfs_waiter,
                    wallets_waiter
                );
                for (name, result) in [
                    ("agents", a),
                    ("agents_teams", at),
                    ("oslfs", o),
                    ("wallets", w),
                ] {
                    match result {
                        Some(true) => anyhow::bail!("{name} init deploy errored on chain"),
                        None => anyhow::bail!("{name} init deploy not finalized within 60s"),
                        Some(false) => tracing::info!("{name} init deploy finalized successfully"),
                    }
                }

                // Verify envs are readable from the observer node
                verify_env_readable(&read_client, "agents", &agents_service.uri).await?;
                verify_env_readable(&read_client, "agents_teams", &agents_teams_service.uri)
                    .await?;
                verify_env_readable(&read_client, "oslfs", &oslfs_service.uri).await?;
                verify_env_readable(&read_client, "wallets", &wallets_service.uri).await?;

                anyhow::Ok((
                    agents_service,
                    agents_teams_service,
                    oslfs_service,
                    wallets_service,
                ))
            },
            async {
                let testnet_write_client =
                    WriteNodeClient::new(config.testnet.deploy_service_url).await?;

                let (testnet_service, testnet_deploy_id) = TestnetService::bootstrap(
                    testnet_write_client.clone(),
                    testnet_read_client,
                    testnet_observer_node_events.clone(),
                    config.testnet.service_key,
                    &config.testnet.env_key,
                )
                .await?;

                tracing::info!(deploy_id = %testnet_deploy_id, "testnet init deploy submitted");

                let testnet_waiter = testnet_observer_node_events
                    .wait_for_deploy(&testnet_deploy_id, Duration::from_secs(60));

                match testnet_waiter.await {
                    Some(true) => anyhow::bail!("testnet init deploy errored on chain"),
                    None => anyhow::bail!("testnet init deploy not finalized within 60s"),
                    Some(false) => tracing::info!("testnet init deploy finalized successfully"),
                }

                anyhow::Ok(testnet_service)
            },
        )?;

    // Spawn periodic registry health check task
    {
        let health_read_client = read_client.clone();
        let env_uris: Vec<(String, Uri)> = vec![
            ("agents".into(), agents_service.uri.clone()),
            ("agents_teams".into(), agents_teams_service.uri.clone()),
            ("oslfs".into(), oslfs_service.uri.clone()),
            ("wallets".into(), wallets_service.uri.clone()),
        ];
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;

                // Control probe: check a well-known system URI
                let system_probe_code =
                    r#"new ret, rl(`rho:registry:lookup`) in { rl!(`rho:lang:treeHashMap`, *ret) }"#
                        .to_string();
                match health_read_client
                    .get_data::<serde_json::Value>(system_probe_code)
                    .await
                {
                    Ok(value) => tracing::info!(
                        value = %value,
                        "registry health: system URI (treeHashMap) OK"
                    ),
                    Err(err) => tracing::error!(
                        error = %err,
                        "registry health: system URI (treeHashMap) FAILED — entire registry may be broken"
                    ),
                }

                // Probe each env URI
                for (name, uri) in &env_uris {
                    let uri_str: &str = uri.as_ref();
                    let code = format!(
                        r#"new ret, rl(`rho:registry:lookup`) in {{ rl!(`{uri_str}`, *ret) }}"#,
                    );
                    match health_read_client.get_data::<serde_json::Value>(code).await {
                        Ok(serde_json::Value::Null) => tracing::warn!(
                            service = name.as_str(),
                            uri = uri_str,
                            "registry health: {name} env registered as Nil"
                        ),
                        Ok(value) => tracing::info!(
                            service = name.as_str(),
                            uri = uri_str,
                            value = %value,
                            "registry health: {name} env OK"
                        ),
                        Err(err) => tracing::error!(
                            service = name.as_str(),
                            uri = uri_str,
                            error = %err,
                            "registry health: {name} env LOST"
                        ),
                    }
                }
            }
        });
        tracing::info!("periodic registry health check started (every 60s)");
    }

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

async fn verify_env_readable(
    read_client: &ReadNodeClient,
    name: &str,
    env_uri: &Uri,
) -> anyhow::Result<()> {
    let uri_str: &str = env_uri.as_ref();
    let code = format!(r#"new ret, rl(`rho:registry:lookup`) in {{ rl!(`{uri_str}`, *ret) }}"#,);
    let result: Result<serde_json::Value, _> = read_client
        .get_data_with_retry(code, 10, Duration::from_millis(500))
        .await;
    match &result {
        Ok(serde_json::Value::Null) => {
            anyhow::bail!(
                "{name} env URI registered as Nil in registry — init contract did not register properly"
            );
        }
        Ok(value) => tracing::info!("{name} env registry lookup succeeded: {value}"),
        Err(err) => {
            anyhow::bail!("{name} env not readable from observer: {err}");
        }
    }
    Ok(())
}
