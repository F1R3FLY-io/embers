use firefly_client::{
    NodeEventSource,
    NodeEvents,
    ReadNode,
    ReadNodeClient,
    WriteNode,
    WriteNodeClient,
};
use poem::middleware::{
    Compression,
    Cors,
    NormalizePath,
    RequestId,
    SetHeader,
    Tracing,
    TrailingSlash,
};
use poem::{Endpoint, EndpointExt, Route};
use poem_openapi::OpenApiService;

use crate::api::agents::AgentsApi;
use crate::api::agents_teams::AgentsTeamsApi;
use crate::api::oslfs::OslfsApi;
use crate::api::service::ServiceApi;
use crate::api::testnet::TestnetApi;
use crate::api::wallets::WalletsApi;
use crate::domain::agents::AgentsService;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::oslfs::OslfsService;
use crate::domain::testnet::TestnetService;
use crate::domain::wallets::WalletsService;

/// The five bootstrapped domain services the HTTP app injects into its
/// handlers, generic over the node-client triple so tests can wire mocks.
pub struct AppServices<R: ReadNode, W: WriteNode, N: NodeEventSource> {
    pub agents: AgentsService<R, W, N>,
    pub agents_teams: AgentsTeamsService<R, W, N>,
    pub oslfs: OslfsService<R, W, N>,
    pub wallets: WalletsService<R, W, N>,
    pub testnet: TestnetService<R, W, N>,
}

/// The complete API surface, in registration order.
type ApiImplementations<R, W, N> = (
    ServiceApi,
    TestnetApi<R, W, N>,
    WalletsApi<R, W, N>,
    AgentsApi<R, W, N>,
    AgentsTeamsApi<R, W, N>,
    OslfsApi<R, W, N>,
);

/// Builds the bare `OpenApiService` (no routes, data, or middleware) so the
/// spec and the served app always derive from the same endpoint tuple.
fn build_openapi_service<R: ReadNode, W: WriteNode, N: NodeEventSource>()
-> OpenApiService<ApiImplementations<R, W, N>, ()> {
    OpenApiService::new(
        (
            ServiceApi,
            TestnetApi::<R, W, N>::default(),
            WalletsApi::<R, W, N>::default(),
            AgentsApi::<R, W, N>::default(),
            AgentsTeamsApi::<R, W, N>::default(),
            OslfsApi::<R, W, N>::default(),
        ),
        "Embers API",
        "0.1.0",
    )
    .url_prefix("/api")
}

/// The generated `OpenAPI` spec for the concrete production client types; the
/// schema generator writes this to `schema.json`.
#[allow(
    dead_code,
    reason = "called by the generate_schema binary; unused when compiled into the embers binary"
)]
pub fn openapi_spec_json() -> String {
    build_openapi_service::<ReadNodeClient, WriteNodeClient, NodeEvents>().spec()
}

/// Assembles the full HTTP app: API routes, swagger endpoints, injected
/// services and JWT keys, and the middleware chain.
pub fn build_app<R: ReadNode, W: WriteNode, N: NodeEventSource>(
    services: AppServices<R, W, N>,
    encoding_key: jsonwebtoken::EncodingKey,
    decoding_key: jsonwebtoken::DecodingKey,
) -> impl Endpoint {
    let api = build_openapi_service::<R, W, N>();

    let ui = api.swagger_ui();
    let spec = api.spec_endpoint();
    let spec_yaml = api.spec_endpoint_yaml();

    Route::new()
        .nest(
            "/api",
            api.with(SetHeader::new().overriding("Cache-Control", "no-store")),
        )
        .nest("/swagger-ui/index.html", ui)
        .nest("/swagger-ui/openapi.json", spec)
        .nest("/swagger-ui/openapi.yaml", spec_yaml)
        .data(encoding_key)
        .data(decoding_key)
        .data(services.agents)
        .data(services.agents_teams)
        .data(services.oslfs)
        .data(services.wallets)
        .data(services.testnet)
        .with(Cors::new().allow_origin_regex("*"))
        .with(RequestId::default())
        .with(Tracing)
        .with(Compression::default())
        .with(NormalizePath::new(TrailingSlash::Trim))
}
