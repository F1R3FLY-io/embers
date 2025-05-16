use api::wallet::WalletApi;
use configuration::collect_config;
use domain::wallet::WalletService;
use firefly_api::{BlocksClient, ReadNodeClient, WriteNodeClient};
use poem::listener::TcpListener;
use poem::middleware::Cors;
use poem::{EndpointExt, Route, Server};
use poem_openapi::OpenApiService;
use storage::firefly::FireflyApi;

mod api;
mod configuration;
mod domain;
mod storage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = collect_config()?;

    let read_node = ReadNodeClient::new(config.read_node_url.clone());
    let write_node = WriteNodeClient::new(
        config.default_wallet_key,
        config.deploy_service_url,
        config.propose_service_url,
    )
    .await?;
    let blocks_client = BlocksClient::new(config.read_node_url);
    let firefly = FireflyApi::new(read_node, write_node, blocks_client);

    let wallet_service = WalletService::new(config.default_wallet_address, firefly);

    let api_service = OpenApiService::new(WalletApi, "Embers API", "1.0.0").url_prefix("/api");

    let ui = api_service.swagger_ui();
    let spec = api_service.spec_endpoint();

    let route = Route::new()
        .nest("/api", api_service)
        .nest("/swagger-ui/index.html", ui)
        .nest("/swagger-ui/openapi.json", spec)
        .data(wallet_service)
        .with(Cors::new().allow_origin_regex("*"));

    Server::new(TcpListener::bind("0.0.0.0:8000"))
        .run_with_graceful_shutdown(
            route,
            async move {
                let _ = tokio::signal::ctrl_c().await;
            },
            None,
        )
        .await?;

    Ok(())
}
