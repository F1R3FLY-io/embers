use anyhow::anyhow;
use atrium_api::agent::Agent;
use atrium_api::agent::atp_agent::CredentialSession;
use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::app::bsky::feed;
use atrium_api::com::atproto::repo::create_record;
use atrium_api::record::KnownRecord;
use atrium_api::types::string::{AtIdentifier, Datetime};
use atrium_api::types::{Collection, TryIntoUnknown};
use atrium_xrpc_client::reqwest::ReqwestClient;

use crate::ai_agents_teams::handlers::AgentsTeamsService;
use crate::ai_agents_teams::models::{
    DeploySignedRunAgentsTeamFireskyReq,
    RunAgentsTeamReq,
    RunAgentsTeamResp,
};

impl AgentsTeamsService {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_run_agents_team_firesky_contract(
        &self,
        request: RunAgentsTeamReq,
    ) -> anyhow::Result<RunAgentsTeamResp> {
        if !self.firesky_accounts.contains_key(&request.agents_team) {
            return Err(anyhow!("agents team is not connected to firesky"));
        }
        self.prepare_run_agents_team_contract(request).await
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_run_agents_team_firesky(
        &self,
        request: DeploySignedRunAgentsTeamFireskyReq,
    ) -> anyhow::Result<()> {
        let cred = self
            .firesky_accounts
            .get(&request.agents_team)
            .ok_or_else(|| anyhow!("agents team is not connected to firesky"))?
            .clone();

        let resp = self.deploy_signed_run_agents_team(request.contract).await?;

        let http_client = ReqwestClient::new(cred.pds_url);
        let session = CredentialSession::new(http_client, MemorySessionStore::default());
        let did = session.login(cred.email, cred.token).await?.data.did;

        let agent = Agent::new(session);

        agent
            .api
            .com
            .atproto
            .repo
            .create_record(
                create_record::InputData {
                    collection: feed::Post::nsid(),
                    record: KnownRecord::from(feed::post::RecordData {
                        created_at: Datetime::now(),
                        embed: None,
                        entities: None,
                        facets: None,
                        labels: None,
                        langs: None,
                        reply: None,
                        tags: None,
                        text: serde_json::to_string(&resp)?,
                    })
                    .try_into_unknown()?,
                    repo: AtIdentifier::Did(did),
                    rkey: None,
                    swap_commit: None,
                    validate: None,
                }
                .into(),
            )
            .await?;

        Ok(())
    }
}
