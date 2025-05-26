use crate::configuration::collect_config;
use firefly_client::FireflyClient;
use firefly_client::{BlocksClient, ReadNodeClient, WriteNodeClient};
use poem::listener::TcpListener;
use poem::middleware::Cors;
use poem::{EndpointExt, Route, Server};
use poem_openapi::OpenApiService;
use routes::wallet::WalletApi;

mod configuration;
mod dtos;
mod routes;
mod wallet;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = collect_config().expect("Can't read bootstrap configuration");

    let firefly_client = {
        let read_client = ReadNodeClient::new(config.read_node_url.clone());
        let write_client =
            WriteNodeClient::new(config.deploy_service_url, config.propose_service_url).await?;
        let blocks_client = BlocksClient::new(config.read_node_url);

        FireflyClient::new(read_client, write_client, blocks_client)
    };

    let api = OpenApiService::new(WalletApi, "Embers API", "0.1.0").url_prefix("/api");

    let ui = api.swagger_ui();
    let spec = api.spec_endpoint();

    let routes = Route::new()
        .nest("/api", api)
        .nest("/swagger-ui/index.html", ui)
        .nest("/swagger-ui/openapi.json", spec)
        .data(firefly_client)
        .with(Cors::new().allow_origin_regex("*"));

    let port = config.port;
    let address = format!("::1:{port}");

    Server::new(TcpListener::bind(address))
        .run_with_graceful_shutdown(
            routes,
            async move {
                let _ = tokio::signal::ctrl_c().await;
            },
            None,
        )
        .await?;

    Ok(())
}
