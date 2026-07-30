use anyhow::anyhow;
use atrium_api::agent::Agent;
use atrium_api::agent::atp_agent::CredentialSession;
use atrium_api::agent::atp_agent::store::MemorySessionStore;
use atrium_api::com::atproto::repo;
use atrium_api::types::Collection;
use atrium_api::types::string::AtIdentifier;
use atrium_xrpc_client::reqwest::ReqwestClient;
use firefly_client::models::WalletAddress;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::publish_to_firesky::{AgentsTeamConfig, SELF_RKEY};
use crate::domain::common::record_trace;

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    /// Stops a published team from answering mentions, by removing the
    /// `com.f1r3sky.agentsteam.config` record that marks its account as a team.
    ///
    /// **The account and handle are kept**, so this is reversible by publishing
    /// again. That is the useful granularity: the destructive alternative is
    /// deleting the atproto account, which frees the handle irreversibly and is far
    /// more than "stop responding" should mean.
    ///
    /// The stored credential on chain is **not** removed, because it cannot be: the
    /// env contract keeps firesky tokens in a stack exposing only `push` and
    /// `toList`, with no removal. A publication is therefore permanent history, and
    /// what actually varies is whether the config record is present. `published_state`
    /// reports that as `active` for exactly this reason — without it, unpublishing
    /// would leave no observable trace, which is the failure mode this whole line of
    /// work exists to remove.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn unpublish(&self, address: WalletAddress, id: String) -> anyhow::Result<()> {
        record_trace!(address, id);

        let state = self
            .published_state(address, id)
            .await?
            .ok_or_else(|| anyhow!("agents team is not published"))?;

        let credentials = self
            .firesky_accounts
            .get(&state.uri)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("agents team is not published"))?;

        // Either identifies the repo. `did` is preferred because a handle can be
        // reassigned; the fallback covers publications recorded before either was
        // captured, which would otherwise be permanently un-unpublishable.
        let repo = match (credentials.did.as_deref(), credentials.handle.as_deref()) {
            (Some(did), _) => did.parse::<AtIdentifier>(),
            (None, Some(handle)) => handle.parse::<AtIdentifier>(),
            (None, None) => {
                return Err(anyhow!(
                    "this publication predates handles being recorded, so the account it \
                     created cannot be identified; remove its \
                     com.f1r3sky.agentsteam.config record directly to stop mentions"
                ));
            }
        }
        .map_err(|err| anyhow!("stored account identifier is not usable: {err}"))?;

        let session = CredentialSession::new(
            ReqwestClient::new(credentials.pds_url.clone()),
            MemorySessionStore::default(),
        );
        session
            .login(credentials.email.clone(), credentials.token.clone())
            .await?;

        Agent::new(session)
            .api
            .com
            .atproto
            .repo
            .delete_record(
                repo::delete_record::InputData {
                    collection: AgentsTeamConfig::nsid(),
                    repo,
                    rkey: SELF_RKEY.clone(),
                    swap_commit: None,
                    swap_record: None,
                }
                .into(),
            )
            .await?;

        Ok(())
    }
}
