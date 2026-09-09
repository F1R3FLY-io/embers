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
use poem_openapi::param::{Header, Path, Query};
use poem_openapi::payload::Json;

use super::models::{TracePrepareResponse, TraceResponse};
use crate::api::agents_teams::models::{
    AgentsTeam,
    AgentsTeams,
    ChannelBinding,
    CreateAgentsTeamReq,
    CreateAgentsTeamResp,
    CreateGrantReq,
    CreateGrantResp,
    DeleteAgentsTeamResp,
    DeployAgentsTeamReq,
    DeployAgentsTeamResp,
    DeploySignedAgentsTeamReq,
    DeploySignedRunOnFireskyReq,
    ErrorBody,
    Grant,
    GrantChannel,
    PrepareCreateGrantResponse,
    PreparePublishToMailResponse,
    PrepareRevealOutputResponse,
    PrepareRevokeGrantResponse,
    PrepareRunUnderGrantResponse,
    PrepareUnpublishMailResponse,
    PublishToFireskyReq,
    PublishToFireskyResp,
    PublishToMailReq,
    PublishToMailResp,
    PublishedChannels,
    PublishedState,
    ReferencingTeam,
    RevealOutputResp,
    RevokeGrantResp,
    RunCommandReq,
    RunCommandResp,
    RunReq,
    RunResp,
    RunState,
    RunUnderGrantReq,
    RunUnderGrantResp,
    SaveAgentsTeamReq,
    SaveAgentsTeamResp,
    SendGrantResponse,
    SendPublishToMailResponse,
    SendRevealOutputResponse,
    SendRunUnderGrantResponse,
    UnpublishMailResp,
};
use crate::api::common::{
    ApiResult,
    ApiTags,
    MaybeNotFound,
    PrepareResponse,
    SendRequest,
    SendResp,
    SignedContract,
    Stringified,
    VoidResult,
};
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
    #[oai(
        path = "/runs/:run_id/trace/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_trace"
    )]
    async fn prepare_trace(
        &self,
        Path(run_id): Path<String>,
        Data(service): Data<&AgentsTeamsService<R, W, N>>,
        Data(key): Data<&jsonwebtoken::EncodingKey>,
    ) -> TraceResponse<PrepareResponse<TracePrepareResponse>> {
        match PrepareResponse::from_call(run_id, |id| service.prepare_trace(id), key).await {
            Ok(value) => TraceResponse::Ok(Json(value)),
            Err(error) => error.into(),
        }
    }
    #[oai(
        path = "/runs/:run_id/trace/send",
        method = "post",
        operation_id = "agents_teams_send_trace"
    )]
    async fn send_trace(
        &self,
        Path(run_id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, String, TracePrepareResponse>,
        Data(service): Data<&AgentsTeamsService<R, W, N>>,
    ) -> TraceResponse<serde_json::Value> {
        if body.prepare_request != run_id {
            return crate::domain::agents_teams::trace::TraceError::Unauthorized.into();
        }
        let prepared = crate::domain::agents_teams::trace::TracePreparation {
            contract: crate::domain::common::PreparedContract(body.prepare_response.contract.0.0),
            nonce: body.prepare_response.nonce,
            expires_at: body.prepare_response.expires_at,
        };
        match service
            .authorize_trace(run_id, prepared, body.request.into())
            .await
        {
            Ok(value) => TraceResponse::Ok(Json(value)),
            Err(error) => error.into(),
        }
    }
    #[oai(
        path = "/runs/:run_id/trace",
        method = "get",
        operation_id = "agents_teams_read_trace"
    )]
    async fn read_trace(
        &self,
        Path(run_id): Path<String>,
        Header(authorization): Header<Option<String>>,
        Query(offset): Query<Option<usize>>,
        Data(service): Data<&AgentsTeamsService<R, W, N>>,
    ) -> TraceResponse<serde_json::Value> {
        match service
            .read_trace(
                run_id,
                authorization.as_deref().unwrap_or(""),
                offset.unwrap_or(0),
            )
            .await
        {
            Ok(value) => TraceResponse::Ok(Json(value)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/publication/pds",
        method = "get",
        operation_id = "agents_teams_describe_pds"
    )]
    async fn describe_pds(&self, Query(pds_url): Query<String>) -> ApiResult<serde_json::Value> {
        crate::domain::agents_teams::publish_to_firesky::discover_pds(&pds_url)
            .await
            .into()
    }

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
        agents_teams
            .deploy_signed_save(body.request.into())
            .await
            .into()
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
            .map(|teams| {
                teams
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<ReferencingTeam>>()
            })
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

    /// Prepares a standing channel grant: "channel identity C may initiate
    /// runs of team T owned by me, within caps, with the delegated
    /// authority". The grantor signs in the environment where their key
    /// lives; the chain derives the grantor from the deploy's signer.
    #[oai(
        path = "/grants/create/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_create_grant"
    )]
    async fn prepare_create_grant(
        &self,
        Json(body): Json<CreateGrantReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> PrepareCreateGrantResponse {
        match PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_create_grant(body.into()),
            encoding_key,
        )
        .await
        {
            Ok(response) => PrepareCreateGrantResponse::Ok(Json(response)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/grants/create/send",
        method = "post",
        operation_id = "agents_teams_send_create_grant"
    )]
    async fn send_create_grant(
        &self,
        SendRequest(body): SendRequest<SignedContract, CreateGrantReq, CreateGrantResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> SendGrantResponse {
        match agents_teams
            .deploy_signed_create_grant(models::DeploySignedCreateGrantReq {
                contract: body.request.into(),
                prepare_request: body.prepare_request.into(),
                prepare_response: body.prepare_response.into(),
            })
            .await
        {
            Ok(deploy_id) => SendGrantResponse::Ok(Json(deploy_id.into())),
            Err(error) => error.into(),
        }
    }

    /// Prepares a grant revocation. Revocation is delivery-only for in-flight
    /// runs: they complete for their owner, while the grantee's reveal and
    /// command claims fail from the moment of revocation. The grantor keeps
    /// `cancel` through the ordinary command envelope.
    #[oai(
        path = "/grants/:grant_id/revoke/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_revoke_grant"
    )]
    async fn prepare_revoke_grant(
        &self,
        Path(grant_id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> PrepareRevokeGrantResponse {
        match PrepareResponse::from_call(
            grant_id,
            |grant_id| agents_teams.prepare_revoke_grant(models::RevokeGrantReq { grant_id }),
            encoding_key,
        )
        .await
        {
            Ok(response) => PrepareRevokeGrantResponse::Ok(Json(response)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/grants/:grant_id/revoke/send",
        method = "post",
        operation_id = "agents_teams_send_revoke_grant"
    )]
    async fn send_revoke_grant(
        &self,
        Path(grant_id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, String, RevokeGrantResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> SendGrantResponse {
        if body.prepare_request != grant_id {
            return SendGrantResponse::InternalError(Json(ErrorBody::new(
                "internal_error",
                "grant revocation id differs from the prepared grant",
            )));
        }
        match agents_teams
            .deploy_signed_revoke_grant(models::DeploySignedRevokeGrantReq {
                contract: body.request.into(),
                prepare_request: models::RevokeGrantReq { grant_id },
                prepare_response: body.prepare_response.into(),
            })
            .await
        {
            Ok(deploy_id) => SendGrantResponse::Ok(Json(deploy_id.into())),
            Err(error) => error.into(),
        }
    }

    /// Grants created by a wallet, for the grant-management interface.
    /// Revoked grants are included, flagged.
    #[oai(
        path = "/grants/by-grantor/:address",
        method = "get",
        operation_id = "agents_teams_grants_by_grantor"
    )]
    async fn grants_by_grantor(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<Vec<Grant>> {
        agents_teams
            .list_grants_by_grantor(address.0.as_ref().to_owned())
            .await
            .map(|grants| grants.into_iter().map(Into::into).collect::<Vec<Grant>>())
            .into()
    }

    /// Grants naming a channel wallet as grantee. An operations cross-check:
    /// channel services consume the observer-readable chain clause directly
    /// and use this mirror only for diagnostics.
    #[oai(
        path = "/grants/by-grantee/:address",
        method = "get",
        operation_id = "agents_teams_grants_by_grantee"
    )]
    async fn grants_by_grantee(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<Vec<Grant>> {
        agents_teams
            .list_grants_by_grantee(address.0.as_ref().to_owned())
            .await
            .map(|grants| grants.into_iter().map(Into::into).collect::<Vec<Grant>>())
            .into()
    }

    /// Prepares a run under a grant: the channel wallet signs and pays the
    /// deploy, the grantor owns the run, and the chain adjudicates the grant
    /// atomically with run registration. Durable teams only.
    #[oai(
        path = "/run-under-grant/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_run_under_grant"
    )]
    async fn prepare_run_under_grant(
        &self,
        Json(body): Json<RunUnderGrantReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> PrepareRunUnderGrantResponse {
        match PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_run_under_grant(body.into()),
            encoding_key,
        )
        .await
        {
            Ok(response) => PrepareRunUnderGrantResponse::Ok(Json(response)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/run-under-grant/send",
        method = "post",
        operation_id = "agents_teams_send_run_under_grant"
    )]
    async fn send_run_under_grant(
        &self,
        SendRequest(body): SendRequest<SignedContract, RunUnderGrantReq, RunUnderGrantResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> SendRunUnderGrantResponse {
        match agents_teams
            .deploy_signed_run_under_grant(models::DeploySignedRunUnderGrantReq {
                contract: body.request.into(),
                prepare_request: body.prepare_request.into(),
                prepare_response: body.prepare_response.into(),
            })
            .await
        {
            Ok(value) => SendRunUnderGrantResponse::Ok(Json(value)),
            Err(error) => error.into(),
        }
    }

    /// Prepares a mail binding: `local@domain` recognized by the mail
    /// channel, bound on chain to the team's deployed registry URI. The
    /// prepare is side-effect light -- a pure render plus a uniqueness read
    /// -- and the binding carries no key material.
    #[oai(
        path = "/:address/:id/publish-to-mail/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_publish_to_mail"
    )]
    async fn prepare_publish_to_mail(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Json(body): Json<PublishToMailReq>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> PreparePublishToMailResponse {
        match PrepareResponse::from_call(
            body,
            |body| agents_teams.prepare_publish_to_mail(address.into(), body.into_domain(id)),
            encoding_key,
        )
        .await
        {
            Ok(response) => PreparePublishToMailResponse::Ok(Json(response)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/:address/:id/publish-to-mail/send",
        method = "post",
        operation_id = "agents_teams_send_publish_to_mail"
    )]
    async fn send_publish_to_mail(
        &self,
        #[allow(unused_variables)] Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, PublishToMailReq, PublishToMailResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> SendPublishToMailResponse {
        match agents_teams
            .deploy_signed_publish_to_mail(models::DeploySignedPublishToMailReq {
                contract: body.request.into(),
                prepare_request: body.prepare_request.into_domain(id),
                prepare_response: body.prepare_response.into(),
            })
            .await
        {
            Ok(deploy_id) => SendPublishToMailResponse::Ok(Json(deploy_id.into())),
            Err(error) => error.into(),
        }
    }

    /// Prepares deactivation of the team's mail binding. Reversible: the
    /// binding record is kept and publishing again reactivates the address.
    #[oai(
        path = "/:address/:id/unpublish-mail/prepare",
        method = "post",
        operation_id = "agents_teams_prepare_unpublish_mail"
    )]
    async fn prepare_unpublish_mail(
        &self,
        #[allow(unused_variables)] Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
        Data(encoding_key): Data<&jsonwebtoken::EncodingKey>,
    ) -> PrepareUnpublishMailResponse {
        match PrepareResponse::from_call(
            id,
            |team_id| agents_teams.prepare_unpublish_mail(models::UnpublishMailReq { team_id }),
            encoding_key,
        )
        .await
        {
            Ok(response) => PrepareUnpublishMailResponse::Ok(Json(response)),
            Err(error) => error.into(),
        }
    }

    #[oai(
        path = "/:address/:id/unpublish-mail/send",
        method = "post",
        operation_id = "agents_teams_send_unpublish_mail"
    )]
    async fn send_unpublish_mail(
        &self,
        #[allow(unused_variables)] Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        SendRequest(body): SendRequest<SignedContract, String, UnpublishMailResp>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> SendPublishToMailResponse {
        if body.prepare_request != id {
            return SendPublishToMailResponse::InternalError(Json(ErrorBody::new(
                "internal_error",
                "mail unpublish team id differs from the prepared team",
            )));
        }
        match agents_teams
            .deploy_signed_unpublish_mail(models::DeploySignedUnpublishMailReq {
                contract: body.request.into(),
                prepare_request: models::UnpublishMailReq { team_id: id },
                prepare_response: body.prepare_response.into(),
            })
            .await
        {
            Ok(deploy_id) => SendPublishToMailResponse::Ok(Json(deploy_id.into())),
            Err(error) => error.into(),
        }
    }

    /// Where the team is reachable, per channel, with honestly three-valued
    /// reachability. Mail reachability is never asserted (embers does not
    /// probe SMTP); chat reachability is derived from grant presence.
    #[oai(
        path = "/:address/:id/published-channels",
        method = "get",
        operation_id = "agents_teams_published_channels"
    )]
    async fn published_channels(
        &self,
        Path(address): Path<Stringified<WalletAddress>>,
        Path(id): Path<String>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<PublishedChannels> {
        agents_teams.published_channels(address.0, id).await.into()
    }

    /// Every binding for a channel, an HTTP mirror of the observer-readable
    /// chain clause for UI and operations.
    #[oai(
        path = "/bindings/by-channel/:channel",
        method = "get",
        operation_id = "agents_teams_bindings_by_channel"
    )]
    async fn bindings_by_channel(
        &self,
        Path(channel): Path<GrantChannel>,
        Data(agents_teams): Data<&AgentsTeamsService<R, W, N>>,
    ) -> ApiResult<Vec<ChannelBinding>> {
        agents_teams
            .list_channel_bindings(channel.as_str().to_owned())
            .await
            .map(|bindings| {
                bindings
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<ChannelBinding>>()
            })
            .into()
    }
}
