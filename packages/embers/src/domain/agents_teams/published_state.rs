use anyhow::anyhow;
use atrium_api::client::AtpServiceClient;
use atrium_api::types::Collection;
use atrium_api::types::string::{AtIdentifier, Handle};
use atrium_xrpc_client::reqwest::ReqwestClient;
use firefly_client::models::{Uri, WalletAddress};
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{FireskyCredentials, PublishedState};
use crate::domain::agents_teams::publish_to_firesky::{AgentsTeamConfig, SELF_RKEY};
use crate::domain::common::record_trace;

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    /// Reports how a team is published, reconciled against the PDS holding the
    /// account.
    ///
    /// Two systems hold one fact. The publish record lives on chain; the account it
    /// names lives on a PDS. Nothing compared them, so wiping a development PDS left
    /// the interface presenting a team as published while `@handle` resolved to
    /// nothing — a state no error surfaced and no page reported.
    ///
    /// Publication is keyed by **deployment uri, not by team id**, which is the
    /// subtlety this has to get right. Every deploy mints a fresh uri, and the
    /// account's `com.f1r3sky.agentsteam.config` record keeps naming whichever uri
    /// was current when it was published. The question is therefore not "is this
    /// team published" but "which of its deployments is published, and is that still
    /// the newest one". Asking only about the latest version would report a
    /// genuinely published team as unpublished the moment it was re-deployed, or
    /// whenever the newest version happened not to be deployed at all.
    ///
    /// `Ok(None)` means no deployment of the team is published, which is not an
    /// error.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn published_state(
        &self,
        address: WalletAddress,
        id: String,
    ) -> anyhow::Result<Option<PublishedState>> {
        record_trace!(address, id);

        let Some(publication) = self.publication_of(address, id).await? else {
            return Ok(None);
        };
        let Publication {
            credentials,
            is_latest_deployment,
            uri,
            version,
        } = publication;

        let account_exists = match credentials.handle.as_deref() {
            // Nothing to check against: this publish predates handles being recorded.
            None => None,
            Some(handle) => resolves(&credentials.pds_url, handle).await,
        };

        // Whether the publication is actually in force. The chain-side credential is
        // permanent -- the env stores tokens in a push-only stack with no removal --
        // so "was published once" and "is published now" are different questions, and
        // only this one governs whether a mention fires.
        let active = match (account_exists, repo_identifier(&credentials)) {
            // No account, so no record on it either. Answering `Some(false)` here
            // rather than re-asking spares a request that cannot succeed.
            (Some(false), _) => Some(false),
            (_, Some(repo)) => config_record_present(&credentials.pds_url, repo).await,
            (_, None) => None,
        };

        Ok(Some(PublishedState {
            account_exists,
            active,
            did: credentials.did,
            handle: credentials.handle,
            // A published deployment that is no longer the newest means mentions run
            // an older build of the team than the editor shows. Silent today, and
            // the cause of "I re-deployed but nothing changed".
            is_latest_deployment,
            published_version: version,
            uri,
        }))
    }
}

/// Which deployment of a team is published, if any.
pub(super) struct Publication {
    pub credentials: FireskyCredentials,
    pub is_latest_deployment: bool,
    pub uri: Uri,
    pub version: String,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    /// Finds the published deployment of a team, without touching the PDS.
    ///
    /// Separate from `published_state` because two callers want different things:
    /// the state endpoint reconciles against the PDS, while a deletion warning only
    /// needs to name the handle and must not pay two network round-trips per team.
    ///
    /// Keyed by deployment uri rather than team id, because each deploy mints a
    /// fresh uri while the account keeps naming whichever was current when it was
    /// published -- so the published deployment is frequently not the newest.
    pub(super) async fn publication_of(
        &self,
        address: WalletAddress,
        id: String,
    ) -> anyhow::Result<Option<Publication>> {
        let versions = self
            .list_versions(address, id)
            .await?
            .ok_or_else(|| anyhow!("agents team not found"))?;

        // Newest deploy first, so the newest deployed uri heads the list and the
        // first published entry found is the most recent published one.
        let mut deployed: Vec<_> = versions
            .agents_teams
            .into_iter()
            .filter_map(|version| {
                version
                    .uri
                    .map(|uri| (version.last_deploy, version.version, uri))
            })
            .collect();
        deployed.sort_by(|left, right| right.0.cmp(&left.0));

        let newest_deployed_uri = deployed.first().map(|(_, _, uri)| uri.clone());

        let Some((_, version, uri)) = deployed
            .into_iter()
            .find(|(_, _, uri)| self.firesky_accounts.contains_key(uri))
        else {
            return Ok(None);
        };

        let credentials = self
            .firesky_accounts
            .get(&uri)
            .map(|entry| entry.clone())
            .ok_or_else(|| anyhow!("publication vanished while being read"))?;

        Ok(Some(Publication {
            credentials,
            is_latest_deployment: newest_deployed_uri.as_ref() == Some(&uri),
            uri,
            version,
        }))
    }
}

/// The account identifier to address the repo by, preferring the DID because a
/// handle can be reassigned to a different account.
fn repo_identifier(credentials: &FireskyCredentials) -> Option<String> {
    credentials
        .did
        .clone()
        .or_else(|| credentials.handle.clone())
}

/// Whether the account still carries the record that marks it as an agent team.
///
/// `None` on any transport failure, for the same reason as `resolves`: an
/// unreachable PDS is not evidence that the record is gone.
async fn config_record_present(pds_url: &str, repo: String) -> Option<bool> {
    let Ok(repo) = repo.parse::<AtIdentifier>() else {
        return None;
    };

    let client = AtpServiceClient::new(ReqwestClient::new(pds_url.to_owned()));

    match client
        .service
        .com
        .atproto
        .repo
        .get_record(
            atrium_api::com::atproto::repo::get_record::ParametersData {
                cid: None,
                collection: AgentsTeamConfig::nsid(),
                repo,
                rkey: SELF_RKEY.clone(),
            }
            .into(),
        )
        .await
    {
        Ok(_) => Some(true),
        // The PDS answered and does not have the record: the team was unpublished.
        Err(atrium_api::xrpc::Error::XrpcResponse(_)) => Some(false),
        Err(err) => {
            tracing::info!("could not read the agents-team config record from {pds_url}: {err}");
            None
        }
    }
}

/// Asks the PDS whether a handle still resolves.
///
/// A transport failure yields `None`, not `Some(false)`: an unreachable PDS says
/// nothing about whether the account exists, and reporting it as missing would turn
/// every momentary outage into a false alarm about lost publications.
async fn resolves(pds_url: &str, handle: &str) -> Option<bool> {
    let Ok(parsed) = Handle::new(handle.to_owned()) else {
        return None;
    };

    let client = AtpServiceClient::new(ReqwestClient::new(pds_url.to_owned()));

    match client
        .service
        .com
        .atproto
        .identity
        .resolve_handle(
            atrium_api::com::atproto::identity::resolve_handle::ParametersData { handle: parsed }
                .into(),
        )
        .await
    {
        Ok(_) => Some(true),
        // The PDS answered and said it does not know this handle. That is the
        // divergence worth reporting.
        Err(atrium_api::xrpc::Error::XrpcResponse(_)) => Some(false),
        Err(err) => {
            tracing::info!("could not resolve {handle} against {pds_url}: {err}");
            None
        }
    }
}
