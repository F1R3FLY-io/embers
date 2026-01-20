use firefly_client::models::WalletAddress;
use poem::web::Data;
use poem_openapi::OpenApi;
use poem_openapi::param::Path;
use poem_openapi::payload::Json;

use crate::api::agents_teams::models::{
    AgentsTeam,
    AgentsTeams,
    CreateAgentsTeamReq,
    CreateAgentsTeamResp,
    DeleteAgentsTeamResp,
    DeployAgentsTeamReq,
    DeployAgentsTeamResp,
    DeploySignedAgentsTeamReq,
    DeploySignedRunOnFireskyReq,
    PublishToFireskyReq,
    PublishToFireskyResp,
    RunReq,
    RunResp,
    SaveAgentsTeamReq,
    SaveAgentsTeamResp,
};
use crate::api::common::{
    ApiTags,
    MaybeNotFound,
    PrepareResponse,
    SendRequest,
    SendResp,
    SignedContract,
    Stringified,
};
use crate::domain::agents_teams::{AgentsTeamsService, models};

#[derive(Debug, Clone)]
pub struct AgentsTeamsApi;

#[OpenApi(prefix_path = "/ai-agents-teams", tag = ApiTags::AIAgentsTeams)]
impl AgentsTeamsApi {
    #[oai(path = "/:address", method = "get")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<AgentsTeams>> {
        let agents_teams = agents_teams.list(address.0).await?;
        Ok(Json(agents_teams.into()))
    }

    #[oai(path = "/:address/:id/versions", method = "get")]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> MaybeNotFound<AgentsTeams> {
        agents_teams.list_versions(address.0, id).await.into()
    }

    #[oai(path = "/:address/:id/versions/:version", method = "get")]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> MaybeNotFound<AgentsTeam> {
        agents_teams.get(address.0, id, version).await.into()
    }

    #[oai(path = "/create/prepare", method = "post")]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateAgentsTeamReq>,
        Data(agents_teams): Data<&AgentsTeamsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<CreateAgentsTeamResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_create_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/create/send", method = "post")]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateAgentsTeamReq, CreateAgentsTeamResp>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents_teams
            .deploy_signed_create(body.request.into())
            .await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/deploy/prepare", method = "post")]
    async fn prepare_deploy(
        &self,
        Json(body): Json<DeployAgentsTeamReq>,
        Data(agents_teams): Data<&AgentsTeamsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<DeployAgentsTeamResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_deploy_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/deploy/send", method = "post")]
    async fn deploy(
        &self,
        SendRequest(body): SendRequest<
            DeploySignedAgentsTeamReq,
            DeployAgentsTeamReq,
            DeployAgentsTeamResp,
        >,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents_teams
            .deploy_signed_deploy(body.request.into())
            .await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/run/prepare", method = "post")]
    async fn prepare_run(
        &self,
        Json(body): Json<RunReq>,
        Data(agents_teams): Data<&AgentsTeamsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<RunResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_run_agents_team_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/run/send", method = "post")]
    async fn run(
        &self,
        SendRequest(body): SendRequest<SignedContract, RunReq, RunResp>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<serde_json::Value>> {
        agents_teams
            .deploy_signed_run_agents_team(body.request.into())
            .await
            .map(Json)
            .map_err(Into::into)
    }

    #[oai(path = "/run-on-firesky/prepare", method = "post")]
    async fn prepare_run_on_firesky(
        &self,
        Json(body): Json<RunReq>,
        Data(agents_teams): Data<&AgentsTeamsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<RunResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_run_om_firesky_contract(body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/run-on-firesky/send", method = "post")]
    async fn run_on_firesky(
        &self,
        SendRequest(body): SendRequest<DeploySignedRunOnFireskyReq, RunReq, RunResp>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<()> {
        agents_teams
            .deploy_signed_run_on_firesky(models::DeploySignedRunOnFireskyReq {
                contract: body.request.contract.into(),
                agents_team: body.prepare_request.agents_team.into(),
                reply_to: body.request.reply_to.map(Into::into),
            })
            .await?;
        Ok(())
    }

    #[oai(path = "/:id/save/prepare", method = "post")]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveAgentsTeamReq>,
        Data(agents_teams): Data<&AgentsTeamsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<SaveAgentsTeamResp>>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_save_contract(id, body.into()),
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/:id/save/send", method = "post")]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, SaveAgentsTeamReq, SaveAgentsTeamResp>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents_teams.deploy_signed_save(body.request.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/:id/delete/prepare", method = "post")]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<DeleteAgentsTeamResp>> {
        let contract = agents.prepare_delete_contract(id).await?;
        Ok(Json(contract.into()))
    }

    #[oai(path = "/:id/delete/send", method = "post")]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents.deploy_signed_delete(body.into()).await?;
        Ok(Json(deploy_id.into()))
    }

    #[oai(path = "/:address/:id/publish-to-firesky/prepare", method = "post")]
    async fn prepare_publish_to_firesky(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Json(body): Json<PublishToFireskyReq>,
        Data(agents_teams): Data<&AgentsTeamsService>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> poem::Result<Json<PrepareResponse<PublishToFireskyResp>>> {
        PrepareResponse::from_call(
            body,
            |body| {
                agents_teams.prepare_publish_to_firesky_contract(address.into(), id, body.into())
            },
            encoding_key,
        )
        .await
        .map(Json)
        .map_err(Into::into)
    }

    #[oai(path = "/:address/:id/publish-to-firesky/send", method = "post")]
    async fn publish_to_firesky(
        &self,
        #[allow(unused_variables)] Path(address): Path<Stringified<WalletAddress>>,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, PublishToFireskyReq, PublishToFireskyResp>,
        Data(agents_teams): Data<&AgentsTeamsService>,
    ) -> poem::Result<Json<SendResp>> {
        let deploy_id = agents_teams
            .deploy_signed_publish_to_firesky(body.request.into())
            .await?;
        Ok(Json(deploy_id.into()))
    }
}
