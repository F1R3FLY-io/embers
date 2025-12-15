use anyhow::anyhow;
use atrium_api::agent::Agent;
use atrium_api::agent::atp_agent::CredentialSession;
use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::app::bsky::{embed, feed};
use atrium_api::com::atproto::repo::{create_record, strong_ref};
use atrium_api::record::KnownRecord;
use atrium_api::types::string::{AtIdentifier, Datetime};
use atrium_api::types::{Collection, TryIntoUnknown, Union};
use atrium_xrpc_client::reqwest::ReqwestClient;
use futures::{StreamExt, stream};

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

        let reply = request
            .reply_to
            .map(|reply_to| {
                let parent_cid = reply_to
                    .parent
                    .cid
                    .parse()
                    .map_err(|err| anyhow::anyhow!("invalid parent cid: {err:?}"))?;
                let root_cid = reply_to
                    .root
                    .cid
                    .parse()
                    .map_err(|err| anyhow::anyhow!("invalid root cid: {err:?}"))?;

                anyhow::Ok(
                    feed::post::ReplyRefData {
                        parent: strong_ref::MainData {
                            cid: parent_cid,
                            uri: reply_to.parent.uri,
                        }
                        .into(),
                        root: strong_ref::MainData {
                            cid: root_cid,
                            uri: reply_to.root.uri,
                        }
                        .into(),
                    }
                    .into(),
                )
            })
            .transpose()?;

        agent
            .api
            .com
            .atproto
            .repo
            .create_record(
                create_record::InputData {
                    collection: feed::Post::nsid(),
                    record: KnownRecord::from(
                        transform_to_post(
                            feed::post::RecordData {
                                created_at: Datetime::now(),
                                embed: None,
                                entities: None,
                                facets: None,
                                labels: None,
                                langs: None,
                                reply,
                                tags: None,
                                text: Default::default(),
                            },
                            &agent,
                            resp,
                        )
                        .await,
                    )
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

async fn transform_to_post<S>(
    mut acc: feed::post::RecordData,
    agent: &Agent<S>,
    value: serde_json::Value,
) -> feed::post::RecordData
where
    S: atrium_api::agent::SessionManager + Send + Sync,
{
    match value {
        serde_json::Value::Null => acc,
        serde_json::Value::Bool(b) => {
            acc.text.push_str(&b.to_string());
            acc
        }
        serde_json::Value::Number(number) => {
            acc.text.push_str(&number.to_string());
            acc
        }
        serde_json::Value::String(string) => {
            if string.chars().take(50).all(|c| c.is_ascii_hexdigit()) {
                // ignore tts for now
            } else if string.starts_with("http") {
                // tti
                let Ok(resp) = reqwest::get(string).await else {
                    return acc;
                };

                let Ok(bytes) = resp.bytes().await else {
                    return acc;
                };

                let Ok(blob_ref) = agent.api.com.atproto.repo.upload_blob(bytes.to_vec()).await
                else {
                    return acc;
                };

                acc.embed = Some(Union::Refs(
                    feed::post::RecordEmbedRefs::AppBskyEmbedImagesMain(Box::new(
                        embed::images::MainData {
                            images: vec![
                                embed::images::ImageData {
                                    alt: Default::default(),
                                    aspect_ratio: None,
                                    image: blob_ref.data.blob,
                                }
                                .into(),
                            ],
                        }
                        .into(),
                    )),
                ));
            } else {
                acc.text.push_str(&string);
            }

            acc
        }
        serde_json::Value::Array(values) => {
            stream::iter(values)
                .fold(acc, |acc, value| {
                    Box::pin(transform_to_post(acc, agent, value))
                })
                .await
        }
        serde_json::Value::Object(map) => {
            stream::iter(map)
                .fold(acc, |acc, (_, value)| {
                    Box::pin(transform_to_post(acc, agent, value))
                })
                .await
        }
    }
}
