use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::payload::Json;

use crate::ai_agents_teams::api::dtos::{DeployDemoReq, RunDemoReq};
use crate::ai_agents_teams::handlers::{deploy_demo, run_demo};
use crate::common::api::dtos::ApiTags;

mod dtos;

#[derive(Debug, Clone)]
pub struct AIAgentsTeams;

#[allow(clippy::unused_async)]
#[OpenApi(prefix_path = "/ai-agents-teams", tag = ApiTags::AIAgentsTeams)]
impl AIAgentsTeams {
    #[oai(path = "/deploy-demo", method = "post")]
    async fn deploy_demo(
        &self,
        Json(body): Json<DeployDemoReq>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_demo(&mut client, body.name)
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/run-demo", method = "post")]
    async fn run_demo(
        &self,
        Json(body): Json<RunDemoReq>,
        Data(client): Data<&WriteNodeClient>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<serde_json::Value>> {
        let mut client = client.to_owned();
        let demo_result = run_demo(&mut client, read_client, body.name, body.prompt).await?;
        Ok(Json(demo_result))
    }
}
