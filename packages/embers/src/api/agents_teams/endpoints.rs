use core::marker::PhantomData;

use firefly_client::models::WalletAddress;
use firefly_client::{
    NodeEventSource,
    NodeEvents,
    ReadNode,
    ReadNodeClient,
    WriteNode,
    WriteNodeClient,
};
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
    PrepareRevealOutputResponse,
    PublishToFireskyReq,
    PublishToFireskyResp,
    PublishedState,
    ReferencingTeam,
    RevealOutputResp,
    RunCommandReq,
    RunCommandResp,
    RunReq,
    RunResp,
    RunState,
    SaveAgentsTeamReq,
    SaveAgentsTeamResp,
    SendRevealOutputResponse,
};
use crate::api::common::{ApiResult, ApiTags, MaybeNotFound, PrepareResponse, SendRequest, SendResp, SignedContract, Stringified, VoidResult};
use crate::domain::agents::AgentsService;
use crate::domain::agents_teams::{AgentsTeamsService, models};

pub struct AgentsTeamsApi<R = ReadNodeClient, W = WriteNodeClient, N = NodeEvents>(
    PhantomData<(R, W, N)>,
);

impl<R, W, N> Default for AgentsTeamsApi<R, W, N> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

#[OpenApi(prefix_path = "/ai-agents-teams", tag = ApiTags::AIAgentsTeams)]
impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsApi<R, W, N> {
    #[oai(path = "/:address", method = "get", operation_id = "agents_teams_list")]
    async fn list(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<AgentsTeams> {
        agents_teams.list(address.0).await.into()
    }

    #[oai(
        path = "/:address/:id/versions",
        method = "get",
        operation_id = "agents_teams_list_versions"
    )]
    async fn list_versions(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> MaybeNotFound<AgentsTeams> {
        agents_teams.list_versions(address.0, id).await.into()
    }

    #[oai(
        path = "/:address/:id/versions/:version",
        method = "get",
        operation_id = "agents_teams_get_version"
    )]
    async fn get(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Path(version): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> MaybeNotFound<AgentsTeam> {
        agents_teams.get(address.0, id, version).await.into()
    }

    #[oai(
        path = "/create/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_create"
    )]
    async fn prepare_create(
        &self,
        Json(body): Json<CreateAgentsTeamReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<CreateAgentsTeamResp>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_create_contract(body.into()),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/create/send",
        method = "post",
        operation_id = "agents_teams_send_create"
    )]
    async fn create(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateAgentsTeamReq, CreateAgentsTeamResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        agents_teams
            .deploy_signed_create(body.request.into())
            .await
            .into()
    }

    #[oai(
        path = "/deploy/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_deploy"
    )]
    async fn prepare_deploy(
        &self,
        Json(body): Json<DeployAgentsTeamReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(agents): Data<&AgentsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<DeployAgentsTeamResp>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_deploy_contract(body.into(), agents),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/deploy/send",
        method = "post",
        operation_id = "agents_teams_send_deploy"
    )]
    async fn deploy(
        &self,
        SendRequest(body): SendRequest<
            DeploySignedAgentsTeamReq,
            DeployAgentsTeamReq,
            DeployAgentsTeamResp,
        >,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        agents_teams
            .deploy_signed_deploy(body.request.into())
            .await
            .into()
    }

    #[oai(
        path = "/run/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_run"
    )]
    async fn prepare_run(
        &self,
        Json(body): Json<RunReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<RunResp>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_run_agents_team_contract(body.into()),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/run/send",
        method = "post",
        operation_id = "agents_teams_send_run"
    )]
    async fn run(
        &self,
        SendRequest(body): SendRequest<SignedContract, RunReq, RunResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<serde_json::Value> {
        agents_teams
            .deploy_signed_run_agents_team(models::DeploySignedRunReq {
                contract: body.request.into(),
                prepare_request: body.prepare_request.into(),
                prepare_response: body.prepare_response.into(),
            })
            .await
            .into()
    }

    #[oai(
        path = "/run-on-firesky/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_run_on_firesky"
    )]
    async fn prepare_run_on_firesky(
        &self,
        Json(body): Json<RunReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<RunResp>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_run_om_firesky_contract(body.into()),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/run-on-firesky/send",
        method = "post",
        operation_id = "agents_teams_send_run_on_firesky"
    )]
    async fn run_on_firesky(
        &self,
        SendRequest(body): SendRequest<DeploySignedRunOnFireskyReq, RunReq, RunResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> VoidResult {
        Box::pin(
            agents_teams.deploy_signed_run_on_firesky(models::DeploySignedRunOnFireskyReq {
                agents_team: body.prepare_request.agents_team.clone().into(),
                reply_to: body.request.reply_to.map(Into::into),
                run_id: body.prepare_response.run_id.clone(),
                run: models::DeploySignedRunReq {
                    contract: body.request.contract.into(),
                    prepare_request: body.prepare_request.into(),
                    prepare_response: body.prepare_response.into(),
                },
            }),
        )
        .await
        .into()
    }

    #[oai(
        path = "/runs/:run_id",
        method = "get",
        operation_id = "agents_teams_get_run"
    )]
    async fn get_run(
        &self,
        Path(run_id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> MaybeNotFound<RunState> {
        agents_teams.get_run(run_id).await.into()
    }

    #[oai(
        path = "/runs/:run_id/commands/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_run_command"
    )]
    async fn prepare_run_command(
        &self,
        Path(run_id): Path<String>,
        Json(body): Json<RunCommandReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<RunCommandResp>> {
        PrepareResponse::from_call(
            body,
            |body| async {
                let request = body.into_domain(run_id)?;
                agents_teams.prepare_run_command(request).await
            },
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/runs/:run_id/commands/send",
        method = "post",
        operation_id = "agents_teams_send_run_command"
    )]
    async fn send_run_command(
        &self,
        Path(run_id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, RunCommandReq, RunCommandResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<serde_json::Value> {
        let prepare_request = match body.prepare_request.into_domain(run_id) {
            Ok(request) => request,
            Err(err) => return ApiResult::InternalError(Json(err.into())),
        };
        agents_teams
            .deploy_signed_run_command(models::DeploySignedRunCommandReq {
                contract: body.request.into(),
                prepare_request,
                prepare_response: body.prepare_response.into(),
            })
            .await
            .into()
    }

    #[oai(
        path = "/runs/:run_id/output/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_reveal_output"
    )]
    async fn prepare_reveal_output(
        &self,
        Path(run_id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> PrepareRevealOutputResponse {
        match PrepareResponse::from_call(
            run_id,
            |run_id| agents_teams.prepare_reveal_output(models::RevealOutputReq { run_id }),
            encoding_key,
        )
        .await
        {
            Ok(response) => PrepareRevealOutputResponse::Ok(Json(response)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/runs/:run_id/output/send",
        method = "post",
        operation_id = "agents_teams_send_reveal_output"
    )]
    async fn send_reveal_output(
        &self,
        Path(run_id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, String, RevealOutputResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> SendRevealOutputResponse {
        if body.prepare_request != run_id {
            return SendRevealOutputResponse::BadRequest(Json(
                anyhow::anyhow!("output reveal run id differs from the prepared run").into(),
            ));
        }
        match agents_teams
            .deploy_signed_reveal_output(models::DeploySignedRevealOutputReq {
                contract: body.request.into(),
                prepare_request: models::RevealOutputReq { run_id },
                prepare_response: body.prepare_response.into(),
            })
            .await
        {
            Ok(value) => SendRevealOutputResponse::Ok(Json(value)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/:id/save/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_save"
    )]
    async fn prepare_save(
        &self,
        Path(id): Path<String>,
        Json(body): Json<SaveAgentsTeamReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<SaveAgentsTeamResp>> {
        PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_save_contract(id, body.into()),
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/:id/save/send",
        method = "post",
        operation_id = "agents_teams_send_save"
    )]
    async fn save(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, SaveAgentsTeamReq, SaveAgentsTeamResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        agents_teams.deploy_signed_save(body.request.into()).await.into()
    }

    #[oai(
        path = "/:id/delete/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_delete"
    )]
    async fn prepare_delete(
        &self,
        Path(id): Path<String>,
        Data(agents): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<DeleteAgentsTeamResp> {
        agents.prepare_delete_contract(id).await.into()
    }

    #[oai(
        path = "/:id/delete/send",
        method = "post",
        operation_id = "agents_teams_send_delete"
    )]
    async fn delete(
        &self,
        #[allow(unused_variables)] Path(id): Path<String>,
        Json(body): Json<SignedContract>,
        Data(agents): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        agents.deploy_signed_delete(body.into()).await.into()
    }

    /// Every team whose latest version uses the given agent, so deleting that agent
    /// can name what it would break before doing it. See `referencing_teams`.
    #[oai(
        path = "/:address/referencing/:agent_id",
        method = "get",
        operation_id = "agents_teams_referencing_teams"
    )]
    async fn referencing_teams(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(agent_id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<Vec<ReferencingTeam>> {
        agents_teams
            .referencing_teams(address.0, agent_id)
            .await
            .map(|teams| teams.into_iter().map(Into::into).collect::<Vec<ReferencingTeam>>())
            .into()
    }

    /// Stops a published team answering mentions, by removing the record that marks
    /// its account as a team. The account and handle are kept, so it is reversible
    /// by publishing again.
    #[oai(
        path = "/:address/:id/unpublish",
        method = "post",
        operation_id = "agents_teams_unpublish"
    )]
    async fn unpublish(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> VoidResult {
        agents_teams.unpublish(address.0, id).await.into()
    }

    /// Reports a team's publication as the chain records it, reconciled against
    /// the PDS. Nothing previously compared the two, so a destroyed account left the
    /// interface presenting a team as published with no way to notice.
    #[oai(
        path = "/:address/:id/published",
        method = "get",
        operation_id = "agents_teams_published_state"
    )]
    async fn published_state(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> MaybeNotFound<PublishedState> {
        agents_teams.published_state(address.0, id).await.into()
    }

    #[oai(
        path = "/:address/:id/publish-to-firesky/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_publish_to_firesky"
    )]
    async fn prepare_publish_to_firesky(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Json(body): Json<PublishToFireskyReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> ApiResult<PrepareResponse<PublishToFireskyResp>> {
        PrepareResponse::from_call(
            body,
            |body| {
                agents_teams.prepare_publish_to_firesky_contract(address.into(), id, body.into())
            },
            encoding_key,
        )
        .await
        .into()
    }

    #[oai(
        path = "/:address/:id/publish-to-firesky/send",
        method = "post",
        operation_id = "agents_teams_send_publish_to_firesky"
    )]
    async fn publish_to_firesky(
        &self,
        #[allow(unused_variables)] Path(address): Path<Stringified<WalletAddress>>,
        #[allow(unused_variables)] Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, PublishToFireskyReq, PublishToFireskyResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<SendResp> {
        agents_teams
            .deploy_signed_publish_to_firesky(body.request.into())
            .await
            .into()
    }
}
