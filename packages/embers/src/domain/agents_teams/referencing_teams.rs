use firefly_client::models::WalletAddress;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::compilation::references_agent;
use crate::domain::agents_teams::models::ReferencingTeam;
use crate::domain::common::record_trace;

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    /// Every team whose latest version uses the given agent.
    ///
    /// Deleting an agent a team references leaves that team permanently
    /// undeployable, and nothing shows it: the team's graph carries the agent's
    /// *name* so the editor keeps drawing the node, and the failure appears only at
    /// deploy time, in another process, naming an identifier the author never sees.
    /// Unlike a dangling edge there is no repair — the node is well formed, only its
    /// referent is gone. This is what lets deletion warn first.
    ///
    /// **Latest versions only, deliberately.** A team whose latest version does not
    /// use the agent is not broken going forward, and treating it as affected would
    /// destroy work to protect a historical version nobody is deploying. The cost is
    /// bounded the same way: one list read plus one parse per team, rather than a
    /// read per version of every team.
    ///
    /// Fails loudly if any team cannot be read. A partial list would under-report
    /// what is about to break, and a caller showing "1 team affected" when the true
    /// answer is three is worse than showing an error.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, agent_id),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn referencing_teams(
        &self,
        address: WalletAddress,
        agent_id: String,
    ) -> anyhow::Result<Vec<ReferencingTeam>> {
        record_trace!(address, agent_id);

        let teams = self.list(address.clone()).await?;

        let mut referencing = Vec::new();
        for header in teams.agents_teams {
            let Some(team) = self
                .get(address.clone(), header.id.clone(), "latest".to_owned())
                .await?
            else {
                // Listed a moment ago and gone now: a concurrent delete, not a
                // failure. Nothing references it any more either way.
                continue;
            };

            let Some(graph) = team.graph else {
                continue;
            };

            // Scans contexts rather than going through `parse`, so a team whose
            // graph is semantically broken -- a dangling edge, say -- is still
            // reported. Such a team is already undeployable, but it does still use
            // the agent, and omitting it would answer "no teams affected" when one
            // is. Failing loudly on an unreadable graph is deliberate for the same
            // reason: an under-reported list is worse than an error.
            if !references_agent(&graph, &agent_id)? {
                continue;
            }

            // A published team keeps answering mentions from its deployed contract
            // even after the team record is gone, so the caller has to be told which
            // ones those are. Looked up per deployment rather than from this
            // version's uri: publication is keyed by the uri current when it was
            // published, which is usually an older deployment than the latest.
            let publication = self
                .publication_of(address.clone(), header.id.clone())
                .await?;

            referencing.push(ReferencingTeam {
                id: header.id,
                name: team.name,
                published: publication.is_some(),
                // May be absent on a publication predating handles being recorded:
                // still live, just not nameable. `published` carries the fact either
                // way, so the caller never has to infer it from a missing handle.
                published_handle: publication.and_then(|p| p.credentials.handle),
                // The version actually scanned. `get` echoes the requested
                // "latest" rather than resolving it, so the header's is the real one.
                version: header.version,
            });
        }

        Ok(referencing)
    }
}
