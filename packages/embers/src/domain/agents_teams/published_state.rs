use anyhow::anyhow;
use atrium_api::client::AtpServiceClient;
use atrium_api::types::string::Handle;
use atrium_xrpc_client::reqwest::ReqwestClient;
use firefly_client::models::WalletAddress;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::PublishedState;
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

        let versions = self
            .list_versions(address, id)
            .await?
            .ok_or_else(|| anyhow!("agents team not found"))?;

        // Newest deploy first, so the first published deployment found is the most
        // recent one and the newest deployed uri is the head of the list.
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

        let account_exists = match credentials.handle.as_deref() {
            // Nothing to check against: this publish predates handles being recorded.
            None => None,
            Some(handle) => resolves(&credentials.pds_url, handle).await,
        };

        Ok(Some(PublishedState {
            account_exists,
            did: credentials.did,
            handle: credentials.handle,
            // A published deployment that is no longer the newest means mentions run
            // an older build of the team than the editor shows. Silent today, and
            // the cause of "I re-deployed but nothing changed".
            is_latest_deployment: newest_deployed_uri.as_ref() == Some(&uri),
            published_version: version,
            uri,
        }))
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
