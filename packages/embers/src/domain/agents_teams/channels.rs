//! Per-team channel reachability aggregation: where is this team reachable,
//! per channel, with honestly three-valued answers.
//!
//! The rendering rule this feeds is the `published_state` lesson: `None`
//! means "could not be determined" and must never be shown as "missing".
//! Mail reachability is `None` by design (embers does not probe SMTP), and
//! the chat row is derived from grant presence -- the gateway dials out and
//! exposes no status surface, so active grants are the honest maximum embers
//! can report.

use std::collections::BTreeSet;

use firefly_client::models::WalletAddress;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use super::AgentsTeamsService;
use super::models::{ChatChannelState, MailChannelState, PublishedChannels};
use super::publish_to_mail::MAIL_CHANNEL;
use crate::domain::common::record_trace;

pub const CHAT_CHANNEL: &str = "chat";

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn published_channels(
        &self,
        address: WalletAddress,
        id: String,
    ) -> anyhow::Result<PublishedChannels> {
        record_trace!(address, id);

        // The three lookups are independent reads; failures in one channel's
        // source must not silently blank another, so each is awaited and
        // propagated rather than folded into a lossy default.
        let firesky = self.published_state(address.clone(), id.clone()).await?;

        let mail = self
            .get_team_channel_bindings(id.clone())
            .await?
            .into_iter()
            .filter(|binding| binding.channel == MAIL_CHANNEL)
            .map(|binding| MailChannelState {
                address: binding.address,
                active: binding.active,
                // Deliberately undetermined: embers does not probe SMTP.
                reachable: None,
            })
            .collect();

        // Grants name a deployment URI, and every deploy mints a fresh one,
        // so the chat row aggregates grants across all of the team's deployed
        // versions rather than only the latest.
        let mut team_uris = BTreeSet::new();
        if let Some(versions) = self.list_versions(address, id).await? {
            for header in versions.agents_teams {
                if let Some(uri) = header.uri {
                    team_uris.insert(uri);
                }
            }
        }
        let mut principals = BTreeSet::new();
        let mut active_grant_count: u64 = 0;
        for uri in team_uris {
            for grant in self.list_grants_for_team(uri).await? {
                if grant.channel == CHAT_CHANNEL && !grant.revoked {
                    active_grant_count += 1;
                    principals.insert(grant.channel_principal);
                }
            }
        }

        Ok(PublishedChannels {
            firesky,
            mail,
            chat: ChatChannelState {
                active_grant_count,
                principals: principals.into_iter().collect(),
            },
        })
    }
}
