use std::time::Duration;

use anyhow::anyhow;
use firefly_client::models::{DeployId, Uri};
use firefly_client::rendering::Render;
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use crate::blockchain::testnet::models;
use crate::domain::common::{prepare_for_signing, record_trace};
use crate::domain::testnet::TestnetService;
use crate::domain::testnet::models::{
    DeploySignedTestReq,
    DeploySignedTestResp,
    DeployTestReq,
    DeployTestResp,
};

#[derive(Debug, Clone, Render)]
#[template(path = "testnet/get_logs.rho")]
struct GetLogs {
    env_uri: Uri,
    deploy_id: DeployId,
}

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> TestnetService<R, W, N> {
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn prepare_test_contract(
        &self,
        request: DeployTestReq,
    ) -> anyhow::Result<DeployTestResp> {
        record_trace!(request);

        let valid_after = self.write_client.clone().get_head_block_index().await?;
        Ok(DeployTestResp {
            env_contract: request.env.map(|env| {
                prepare_for_signing()
                    .code(env)
                    .valid_after_block_number(valid_after)
                    .call()
            }),
            test_contract: prepare_for_signing()
                .code(request.test)
                .valid_after_block_number(valid_after)
                .call(),
        })
    }

    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(request),
        err(Debug),
        ret(Debug, level = "trace")
    )]
    pub async fn deploy_test_contract(
        &self,
        request: DeploySignedTestReq,
    ) -> anyhow::Result<DeploySignedTestResp> {
        record_trace!(request);

        let mut write_client = self.write_client.clone();

        if let Some(contract) = request.env {
            let result = write_client.deploy_signed_contract(contract).await;
            if let Err(err) = result {
                return Ok(DeploySignedTestResp::EnvDeployFailed {
                    error: err.to_string(),
                });
            }
        }

        let result = write_client.deploy_signed_contract(request.test).await;
        let deploy_id = match result {
            Ok(deploy_id) => deploy_id,
            Err(err) => {
                return Ok(DeploySignedTestResp::TestDeployFailed {
                    error: err.to_string(),
                });
            }
        };

        let result = self
            .observer_node_events
            .wait_for_deploy(&deploy_id, Duration::from_mins(1))
            .await;

        match result {
            Some(true) => return Err(anyhow!("deploy {deploy_id} errored on chain")),
            None => return Err(anyhow!("block is not finalized")),
            Some(false) => {}
        }

        let code = GetLogs {
            deploy_id,
            env_uri: self.uri.clone(),
        }
        .render()?;

        let logs: Option<Vec<models::Log>> = self.read_client.get_data(code).await?;

        Ok(DeploySignedTestResp::Ok {
            logs: logs
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::test_helpers::*;
    use crate::domain::testnet::models::{DeployTestReq, LogLevel};
    use serde_json::json;

    fn make_service_with(
        write_client: MockWriteNode,
        read_client: MockReadNode,
        observer: MockNodeEventSource,
    ) -> TestnetService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        TestnetService {
            uri: test_uri(),
            service_key: test_secret_key(),
            write_client,
            read_client,
            observer_node_events: observer,
        }
    }

    fn make_service() -> TestnetService<MockReadNode, MockWriteNode, MockNodeEventSource> {
        make_service_with(
            MockWriteNode::new().with_head_block_index(10),
            MockReadNode::new(),
            MockNodeEventSource::new(),
        )
    }

    // -------------------------------------------------------
    // prepare_test_contract tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_prepare_with_env_and_test() {
        let service = make_service();
        let request = DeployTestReq {
            env: Some("env-code".into()),
            test: "test-code".into(),
        };

        let resp = service
            .prepare_test_contract(request)
            .await
            .expect("prepare_test_contract should succeed");

        assert!(
            resp.env_contract.is_some(),
            "env_contract should be present when env is Some"
        );
        // test_contract is always present
        assert!(
            !resp.test_contract.0.is_empty(),
            "test_contract should be non-empty"
        );
    }

    #[tokio::test]
    async fn test_prepare_test_only() {
        let service = make_service();
        let request = DeployTestReq {
            env: None,
            test: "test-code".into(),
        };

        let resp = service
            .prepare_test_contract(request)
            .await
            .expect("prepare_test_contract should succeed");

        assert!(
            resp.env_contract.is_none(),
            "env_contract should be None when env is None"
        );
        assert!(
            !resp.test_contract.0.is_empty(),
            "test_contract should be non-empty"
        );
    }

    // -------------------------------------------------------
    // deploy_test_contract tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_deploy_env_failure_short_circuits() {
        let write_client = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Err(anyhow!("env deploy exploded")));

        let service = make_service_with(
            write_client,
            MockReadNode::new(),
            MockNodeEventSource::new(),
        );

        let request = DeploySignedTestReq {
            env: Some(test_signed_code()),
            test: test_signed_code(),
        };

        let resp = service
            .deploy_test_contract(request)
            .await
            .expect("deploy_test_contract should return Ok(EnvDeployFailed), not Err");

        match resp {
            DeploySignedTestResp::EnvDeployFailed { error } => {
                assert!(
                    error.contains("env deploy exploded"),
                    "expected error to contain 'env deploy exploded', got: {error}"
                );
            }
            other => panic!("expected EnvDeployFailed, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_deploy_test_failure() {
        // First deploy (env) succeeds, second deploy (test) fails
        let write_client = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(test_deploy_id()))
            .with_deploy_response(Err(anyhow!("test deploy failed")));

        let service = make_service_with(
            write_client,
            MockReadNode::new(),
            MockNodeEventSource::new(),
        );

        let request = DeploySignedTestReq {
            env: Some(test_signed_code()),
            test: test_signed_code(),
        };

        let resp = service
            .deploy_test_contract(request)
            .await
            .expect("deploy_test_contract should return Ok(TestDeployFailed), not Err");

        match resp {
            DeploySignedTestResp::TestDeployFailed { error } => {
                assert!(
                    error.contains("test deploy failed"),
                    "expected error to contain 'test deploy failed', got: {error}"
                );
            }
            other => panic!("expected TestDeployFailed, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_deploy_chain_error() {
        let deploy_id = test_deploy_id();
        let observer = MockNodeEventSource::new()
            .with_wait_result(&deploy_id.to_string(), Some(true));

        let write_client = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(deploy_id));

        let service = make_service_with(
            write_client,
            MockReadNode::new(),
            observer,
        );

        let request = DeploySignedTestReq {
            env: None,
            test: test_signed_code(),
        };

        let result = service.deploy_test_contract(request).await;
        assert!(result.is_err(), "expected Err when deploy errored on chain");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("errored on chain"),
            "expected 'errored on chain' in error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_deploy_not_finalized() {
        let deploy_id = test_deploy_id();
        let observer = MockNodeEventSource::new()
            .with_wait_result(&deploy_id.to_string(), None);

        let write_client = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(deploy_id));

        let service = make_service_with(
            write_client,
            MockReadNode::new(),
            observer,
        );

        let request = DeploySignedTestReq {
            env: None,
            test: test_signed_code(),
        };

        let result = service.deploy_test_contract(request).await;
        assert!(result.is_err(), "expected Err when block is not finalized");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("block is not finalized"),
            "expected 'block is not finalized' in error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_deploy_success_fetches_logs() {
        let deploy_id = test_deploy_id();

        // wait_for_deploy returns Some(false) -> success (no error)
        let observer = MockNodeEventSource::new()
            .with_wait_result(&deploy_id.to_string(), Some(false));

        let write_client = MockWriteNode::new()
            .with_head_block_index(10)
            .with_deploy_response(Ok(deploy_id));

        // The rendered GetLogs template contains "get" -- match on that to return mock logs
        let read_client = MockReadNode::new().on_code_containing(
            "get",
            json!([
                { "level": "info", "message": "test passed" },
                { "level": "error", "message": "assertion failed" }
            ]),
        );

        let service = make_service_with(write_client, read_client, observer);

        let request = DeploySignedTestReq {
            env: None,
            test: test_signed_code(),
        };

        let resp = service
            .deploy_test_contract(request)
            .await
            .expect("deploy_test_contract should succeed");

        match resp {
            DeploySignedTestResp::Ok { logs } => {
                assert_eq!(logs.len(), 2, "expected 2 log entries, got {}", logs.len());
                assert!(
                    matches!(logs[0].level, LogLevel::Info),
                    "expected first log level Info, got {:?}",
                    logs[0].level
                );
                assert_eq!(logs[0].message, "test passed");
                assert!(
                    matches!(logs[1].level, LogLevel::Error),
                    "expected second log level Error, got {:?}",
                    logs[1].level
                );
                assert_eq!(logs[1].message, "assertion failed");
            }
            other => panic!("expected Ok with logs, got: {other:?}"),
        }
    }
}
