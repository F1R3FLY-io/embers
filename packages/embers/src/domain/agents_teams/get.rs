use std::time::Duration;

use firefly_client::models::{Uri, WalletAddress};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::blockchain::agents_teams::models;
use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::AgentsTeam;
use crate::domain::common::record_trace;

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get.rho")]
struct Get {
    env_uri: Uri,
    address: WalletAddress,
    id: String,
    version: String,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id, version),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn get(
        &self,
        address: WalletAddress,
        id: String,
        version: String,
    ) -> anyhow::Result<Option<AgentsTeam>> {
        record_trace!(address, id, version);

        let code = Get {
            env_uri: self.uri.clone(),
            address,
            id,
            version,
        }
        .render()?;

        let agents_team: Option<models::AgentsTeam> =
            self.read_client.get_data_or_none(code).await?.flatten();
        Ok(agents_team.map(Into::into))
    }

    /// Like `get`, but retries on empty explore-deploy results to handle observer tuplespace lag.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(address, id, version),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn get_with_retry(
        &self,
        address: WalletAddress,
        id: String,
        version: String,
        max_retries: u32,
        delay: Duration,
    ) -> anyhow::Result<Option<AgentsTeam>> {
        record_trace!(address, id, version);

        let code = Get {
            env_uri: self.uri.clone(),
            address,
            id,
            version,
        }
        .render()?;

        let agents_team: Option<models::AgentsTeam> = self
            .read_client
            .get_data_or_none_with_retry(code, max_retries, delay)
            .await?
            .flatten();
        Ok(agents_team.map(Into::into))
    }

    /// Poll until the observer exposes the exact deployed projection, not just
    /// any version of the team record. A finalized deployment may briefly be
    /// preceded by a successful read of the older record whose URI is still
    /// absent, so retrying only empty reads is insufficient.
    pub async fn get_deployed_with_retry(
        &self,
        address: WalletAddress,
        id: String,
        version: String,
        max_retries: u32,
        delay: Duration,
    ) -> anyhow::Result<Option<AgentsTeam>> {
        let mut last_observed = None;
        for attempt in 0..=max_retries {
            let observed = self
                .get(address.clone(), id.clone(), version.clone())
                .await?;
            if observed.as_ref().is_some_and(|team| team.uri.is_some()) {
                return Ok(observed);
            }
            last_observed = observed;
            if attempt < max_retries {
                tokio::time::sleep(delay).await;
            }
        }
        Ok(last_observed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use dashmap::DashMap;
    use serde_json::json;

    use super::*;
    use crate::domain::agents_teams::AgentsTeamsService;
    use crate::domain::test_helpers::*;

    fn team(uri: Option<&str>) -> serde_json::Value {
        json!({
            "id":"team-id",
            "version":"team-version",
            "created_at":0,
            "last_deploy":null,
            "uri":uri,
            "name":"Team",
            "description":null,
            "shard":null,
            "logo":null,
            "graph":null,
            "definition":null
        })
    }

    #[tokio::test]
    async fn deployed_read_retries_a_successful_stale_record() {
        let deployed_uri = test_uri();
        let read = MockReadNode::new().on_code_containing_values_then(
            "team-id",
            vec![team(None), team(None)],
            team(Some(AsRef::<String>::as_ref(&deployed_uri))),
        );
        let service = AgentsTeamsService {
            uri: test_uri(),
            write_client: MockWriteNode::new(),
            read_client: read.clone(),
            observer_node_events: MockNodeEventSource::new(),
            service_key: test_secret_key(),
            aes_encryption_key: [42u8; 32].into(),
            continuation_store: crate::domain::agents_teams::continuation_store::F1r3driveContinuationStore::open_for_test(
                tempfile::tempdir().expect("temporary store").path(),
                [7u8; 32],
                [9u8; 32],
            )
            .expect("test continuation store"),
            deploy_finalization_timeout: Duration::from_secs(5),
            run_locks: Arc::new(DashMap::new()),
            firesky_accounts: Arc::new(DashMap::new()),
        };

        let deployed = service
            .get_deployed_with_retry(
                WalletAddress::from(test_public_key()),
                "team-id".to_owned(),
                "latest".to_owned(),
                2,
                Duration::ZERO,
            )
            .await
            .expect("the observer sequence should remain readable")
            .expect("the deployed record should become visible");

        assert_eq!(deployed.uri.expect("deployed URI"), deployed_uri);
        assert_eq!(read.calls().len(), 3);
    }
}
