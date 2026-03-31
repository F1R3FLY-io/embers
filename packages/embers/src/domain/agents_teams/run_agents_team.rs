use std::time::Duration;

use anyhow::anyhow;
use firefly_client::models::{DeployId, SignedCode, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::domain::agents_teams::AgentsTeamsService;
use crate::domain::agents_teams::models::{RunReq, RunResp};
use crate::domain::common::{prepare_for_signing, record_trace};

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/run.rho")]
struct RunAgentsTeam {
    agents_team: Uri,
    prompt: String,
}

#[derive(Debug, Clone, Render)]
#[template(path = "agents_teams/get_run_result.rho")]
struct GetAgentsTeamResult {
    deploy_id: DeployId,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_run_agents_team_contract(
        &self,
        request: RunReq,
    ) -> anyhow::Result<RunResp> {
        record_trace!(request);

        let contract = RunAgentsTeam {
            agents_team: request.agents_team,
            prompt: request.prompt,
        }
        .render()?;

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(RunResp {
            contract: prepare_for_signing()
                .code(contract)
                .phlo_limit(request.phlo_limit)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(contract),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_signed_run_agents_team(
        &self,
        contract: SignedCode,
    ) -> anyhow::Result<serde_json::Value> {
        record_trace!(contract);

        let mut write_client = self.write_client.clone();

        let deploy_id = write_client.deploy_signed_contract(contract).await?;

        let result = self
            .observer_node_events
            .wait_for_deploy(&deploy_id, Duration::from_mins(1))
            .await;

        match result {
            Some(true) => return Err(anyhow!("deploy {deploy_id} errored on chain")),
            None => return Err(anyhow!("block is not finalized")),
            Some(false) => {}
        }

        let code = GetAgentsTeamResult { deploy_id }.render()?;
        self.read_client
            .get_data_with_retry(code, 5, Duration::from_millis(500))
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aes_gcm::{Aes256Gcm, Key};
    use dashmap::DashMap;

    use super::*;
    use crate::domain::test_helpers::*;

    fn make_service(
        event_source: MockNodeEventSource,
    ) -> AgentsTeamsService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        AgentsTeamsService {
            uri: test_uri(),
            write_client: MockWriteNode::new().with_deploy_response(Ok(test_deploy_id())),
            read_client: MockReadNode::new(),
            observer_node_events: event_source,
            aes_encryption_key: *Key::<Aes256Gcm>::from_slice(&[42u8; 32]),
            firesky_accounts: Arc::new(DashMap::new()),
        }
    }

    #[tokio::test]
    async fn test_deploy_signed_run_chain_error() {
        let event_source =
            MockNodeEventSource::new().with_wait_result("test-deploy-id", Some(true));
        let service = make_service(event_source);

        let result = service
            .deploy_signed_run_agents_team(test_signed_code())
            .await;

        assert!(result.is_err(), "expected Err when deploy errored on chain");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("errored on chain"),
            "expected 'errored on chain' in error message, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_deploy_signed_run_not_finalized() {
        let event_source = MockNodeEventSource::new().with_wait_result("test-deploy-id", None);
        let service = make_service(event_source);

        let result = service
            .deploy_signed_run_agents_team(test_signed_code())
            .await;

        assert!(result.is_err(), "expected Err when block is not finalized");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not finalized"),
            "expected 'not finalized' in error message, got: {err_msg}"
        );
    }
}
