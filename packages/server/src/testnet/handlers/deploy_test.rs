use std::time::Duration;

use anyhow::anyhow;
use firefly_client::models::DeployId;
use firefly_client::{ReadNodeClient, WriteNodeClient, template};

use crate::common::prepare_for_signing;
use crate::common::tracing::record_trace;
use crate::testnet::blockchain;
use crate::testnet::models::{
    DeploySignedTestReq,
    DeploySignedTestResp,
    DeployTestReq,
    DeployTestResp,
};

template! {
    #[template(path = "ai_agents/get_logs.rho")]
    #[derive(Debug, Clone)]
    struct GetLogs {
        deploy_id: DeployId,
    }
}

#[tracing::instrument(level = "info", skip_all, fields(request), ret(Debug, level = "trace"))]
pub async fn prepare_test_contract(
    request: DeployTestReq,
    client: &mut WriteNodeClient,
) -> anyhow::Result<DeployTestResp> {
    record_trace!(request);

    let valid_after = client.get_head_block_index().await?;
    Ok(DeployTestResp {
        env_contract: request.env.map(|env| prepare_for_signing(env, valid_after)),
        test_contract: prepare_for_signing(request.test, valid_after),
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
    client: &mut WriteNodeClient,
    read_client: &ReadNodeClient,
    request: DeploySignedTestReq,
) -> anyhow::Result<DeploySignedTestResp> {
    record_trace!(request);

    if let Some(contract) = request.env {
        let result = client.deploy_signed_contract(contract).await;
        if let Err(err) = result {
            return Ok(DeploySignedTestResp::EnvDeployFailed {
                error: err.to_string(),
            });
        }

        client.propose().await?;
    }

    let result = client.deploy_signed_contract(request.test).await;
    let deploy_id = match result {
        Ok(deploy_id) => deploy_id,
        Err(err) => {
            return Ok(DeploySignedTestResp::TestDeployFailed {
                error: err.to_string(),
            });
        }
    };

    let block_hash = client.propose().await?;

    let finalized = read_client
        .wait_finalization(block_hash, Duration::from_secs(15))
        .await?;

    if !finalized {
        return Err(anyhow!("block is not finalized"));
    }

    let code = GetLogs { deploy_id }.render()?;

    let logs: Option<Vec<blockchain::dtos::Log>> = read_client.get_data(code).await?;

    Ok(DeploySignedTestResp::Ok {
        logs: logs
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect(),
    })
}
