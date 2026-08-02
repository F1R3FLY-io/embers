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
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use futures::{StreamExt, stream};

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{DeploySignedRunOnFireskyReq, RunReq, RunResp};
use crate::domain::common::upload_blob_from_url;

const TERMINAL_REPLY_LIMIT: usize = 280;

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_run_om_firesky_contract(
        &self,
        request: RunReq,
    ) -> anyhow::Result<RunResp> {
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
    pub async fn deploy_signed_run_on_firesky(
        &self,
        request: DeploySignedRunOnFireskyReq,
    ) -> anyhow::Result<()> {
        let run_id = request.run_id.clone();
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

        let cred = self
            .firesky_accounts
            .get(&request.agents_team)
            .ok_or_else(|| anyhow!("agents team is not connected to firesky"))?
            .clone();

        let resp = self.deploy_signed_run_agents_team(request.run).await?;

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
                    record: KnownRecord::from(finalize_terminal_reply(
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
                        &run_id,
                    ))
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

fn finalize_terminal_reply(
    mut post: feed::post::RecordData,
    run_id: &str,
) -> feed::post::RecordData {
    let base = std::env::var("EMBERS__PUBLIC_RUN_URL")
        .unwrap_or_else(|_| "http://localhost:5173/runs".to_owned());
    let absolute_url = format!("{}/{run_id}", base.trim_end_matches('/'));
    let run_url = if absolute_url.chars().count() + "Run: ".chars().count() <= TERMINAL_REPLY_LIMIT
    {
        absolute_url
    } else {
        format!("/runs/{run_id}")
    };
    let prefix = if post.text.is_empty() { "" } else { "\n" };
    let suffix = format!("{prefix}Run: {run_url}");
    let suffix_len = suffix.chars().count();

    if post.text.chars().count() + suffix_len > TERMINAL_REPLY_LIMIT {
        let text_budget = TERMINAL_REPLY_LIMIT.saturating_sub(suffix_len);
        post.text = match text_budget {
            0 => String::new(),
            1 => "…".to_owned(),
            _ => {
                let mut text = post.text.chars().take(text_budget - 1).collect::<String>();
                text.push('…');
                text
            }
        };
    }

    post.text.push_str(&suffix);
    post
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
                let Ok(image) = upload_blob_from_url(agent, &string).await else {
                    return acc;
                };

                acc.embed = Some(Union::Refs(
                    feed::post::RecordEmbedRefs::AppBskyEmbedImagesMain(Box::new(
                        embed::images::MainData {
                            images: vec![
                                embed::images::ImageData {
                                    alt: Default::default(),
                                    aspect_ratio: None,
                                    image,
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

#[cfg(test)]
mod tests {
    use atrium_api::app::bsky::feed;
    use atrium_api::types::string::Datetime;

    use super::{TERMINAL_REPLY_LIMIT, finalize_terminal_reply};

    fn post(text: String) -> feed::post::RecordData {
        feed::post::RecordData {
            created_at: Datetime::now(),
            embed: None,
            entities: None,
            facets: None,
            labels: None,
            langs: None,
            reply: None,
            tags: None,
            text,
        }
    }

    #[test]
    fn terminal_reply_is_bounded_and_preserves_run_link() {
        let run_id = "0198fbe2-7cc7-7000-8000-000000000001";
        let result = finalize_terminal_reply(post("α".repeat(400)), run_id);

        assert_eq!(result.text.chars().count(), TERMINAL_REPLY_LIMIT);
        assert!(result.text.ends_with(&format!("/runs/{run_id}")));
        assert!(result.text.contains('…'));
    }

    #[test]
    fn terminal_reply_with_only_an_embed_still_names_the_run() {
        let result = finalize_terminal_reply(post(String::new()), "run-id");

        assert_eq!(result.text, "Run: http://localhost:5173/runs/run-id");
    }
}
