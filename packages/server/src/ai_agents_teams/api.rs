use firefly_client::{ReadNodeClient, WriteNodeClient};
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::ai_agents_teams::api::dtos::{
    AgentsTeam,
    AgentsTeams,
    CreateAgentsTeamReq,
    CreateAgentsTeamResp,
    DeployDemoReq,
    RunDemoReq,
    SaveAgentsTeamReq,
    SaveAgentsTeamResp,
};
use crate::ai_agents_teams::handlers::{
    deploy_demo,
    deploy_signed_create_agents_team,
    deploy_signed_save_agents_team,
    get_agents_team,
    list_agents_team_versions,
    list_agents_teams,
    prepare_create_agents_team_contract,
    prepare_save_agents_team_contract,
    run_demo,
};
use crate::common::api::dtos::{ApiTags, MaybeNotFound, ParseFromString, SignedContract};
use crate::common::models::WalletAddress;

mod dtos;

#[derive(Debug, Clone)]
pub struct AIAgentsTeams;

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

    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> poem::Result<Json<AgentsTeams>> {
        let agents_teams = list_agents_teams(address.0, read_client).await?;
        Ok(Json(agents_teams.into()))
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Path(id): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> MaybeNotFound<AgentsTeams> {
        list_agents_team_versions(address.0, id, read_client)
            .await
            .into()
    }

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<ParseFromString<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(read_client): Data<&ReadNodeClient>,
    ) -> MaybeNotFound<AgentsTeam> {
        get_agents_team(address.0, id, version, read_client)
            .await
            .into()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(input): Json<CreateAgentsTeamReq>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<Json<CreateAgentsTeamResp>> {
        let mut client = client.to_owned();
        let contract = prepare_create_agents_team_contract(input.into(), &mut client).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        Json(body): Json<SignedContract>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_create_agents_team(&mut client, body.into())
            .await
            .map_err(Into::into)
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(input): Json<SaveAgentsTeamReq>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<Json<SaveAgentsTeamResp>> {
        let mut client = client.to_owned();
        let contract = prepare_save_agents_team_contract(id, input.into(), &mut client).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(client): Data<&WriteNodeClient>,
    ) -> poem::Result<()> {
        let mut client = client.to_owned();
        deploy_signed_save_agents_team(&mut client, body.into())
            .await
            .map_err(Into::into)
    }
}
