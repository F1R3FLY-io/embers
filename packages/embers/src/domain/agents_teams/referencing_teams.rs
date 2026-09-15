use firefly_client::models::WalletAddress;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};
use futures::StreamExt;
use futures::stream::FuturesUnordered;

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::compilation::references_agent;
use crate::domain::agents_teams::models::ReferencingTeam;
use crate::domain::common::record_trace;

/// How many reads to have in flight at once.
///
/// Every team is an independent read and the node answers them faster together than
/// one after another: measured against the local network, two `listVersions` reads
/// took 62s in sequence and 43s concurrently. The gain is real but sub-linear -- the
/// node partially serialises -- so this is a small number rather than "all of them".
/// Unbounded fan-out would turn a wallet holding fifty teams into fifty simultaneous
/// explore-deploys and make the node slower for everyone, including this caller.
const READ_CONCURRENCY: usize = 8;

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

        let listed = self.list(address.clone()).await?;

        // Read every team, then keep the ones that use the agent. Concurrent because
        // the reads are independent and each costs tens of seconds against a busy
        // node; sequentially, the wait grows with the number of teams the wallet
        // holds and the confirmation dialog becomes unusable long before the list
        // does.
        let mut reads = FuturesUnordered::new();
        let team_count = listed.agents_teams.len();
        let mut headers = listed.agents_teams.into_iter();
        let mut referencing = Vec::with_capacity(team_count);

        loop {
            while reads.len() < READ_CONCURRENCY {
                let Some(header) = headers.next() else { break };
                let address = address.clone();
                reads.push(async move {
                    let team = self
                        .get(address, header.id.clone(), "latest".to_owned())
                        .await?;
                    anyhow::Ok((header, team))
                });
            }

            let Some(read) = reads.next().await else {
                break;
            };
            let (header, team) = read?;

            // Listed a moment ago and gone now: a concurrent delete, not a failure.
            // Nothing references it any more either way.
            let Some(team) = team else { continue };
            let Some(graph) = team.graph else { continue };

            // Scans contexts rather than going through `parse`, so a team whose
            // graph is semantically broken -- a dangling edge, say -- is still
            // reported. Such a team is already undeployable, but it does still use
            // the agent, and omitting it would answer "no teams affected" when one
            // is. Failing loudly on an unreadable graph is deliberate for the same
            // reason: an under-reported list is worse than an error.
            if !references_agent(&graph, &agent_id)? {
                continue;
            }

            referencing.push((header, team.name));
        }

        // A published team keeps answering mentions from its deployed contract even
        // after the team record is gone, so the caller has to be told which ones
        // those are. Looked up per deployment rather than from this version's uri:
        // publication is keyed by the uri current when it was published, which is
        // usually an older deployment than the latest.
        //
        // Only for the teams that actually reference the agent, and only after the
        // scan above, so a wallet full of unrelated teams costs nothing here.
        let mut publications = FuturesUnordered::new();
        let mut result = Vec::with_capacity(referencing.len());
        let mut pending = referencing.into_iter();

        loop {
            while publications.len() < READ_CONCURRENCY {
                let Some((header, name)) = pending.next() else {
                    break;
                };
                let address = address.clone();
                publications.push(async move {
                    let publication = self.publication_of(address, header.id.clone()).await?;
                    anyhow::Ok((header, name, publication))
                });
            }

            let Some(looked_up) = publications.next().await else {
                break;
            };
            let (header, name, publication) = looked_up?;

            result.push(ReferencingTeam {
                id: header.id,
                name,
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

        // Completion order is arbitrary once the reads run concurrently, and the
        // caller lists these to a user about to destroy them. Sorting by name keeps
        // the same wallet producing the same dialog twice running.
        result.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(result)
    }
}
