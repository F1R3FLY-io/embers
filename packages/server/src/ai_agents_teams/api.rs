use firefly_client::WriteNodeClient;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::ai_agents_teams::api::dtos::DemoReq;
use crate::ai_agents_teams::handlers::deploy_demo;
use crate::common::api::dtos::ApiTags;

mod dtos;

#[derive(Debug, Clone)]
pub struct AIAgentsTeams;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/ai-agents-teams", tag = ApiTags::AIAgentsTeams)]
impl AIAgentsTeams {
    #[oai(path = "/demo", method = "get")]
    async fn demo(
        &self,
        Json(body): Json<DemoReq>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_demo(&mut client, body.name)
            .await
            .map_err(Into::into)
    }
}
