use std::time::Duration;

use firefly_client::models::{DeployId, SignedCode};
use firefly_client::{NodeEventSource, ReadNode, WriteNode};

use super::AgentsTeamsService;

impl<R: ReadNode, W: WriteNode, N: NodeEventSource> AgentsTeamsService<R, W, N> {
    pub(crate) async fn submit_and_finalize_effect(
        &self,
        contract: SignedCode,
        timeout: Duration,
    ) -> anyhow::Result<(DeployId, Option<bool>)> {
        submit_and_finalize_effect(
            self.write_client.clone(),
            &self.read_client,
            &self.observer_node_events,
            contract,
            timeout,
        )
        .await
    }

    pub(crate) async fn wait_for_deploy_finalization(
        &self,
        deploy_id: &DeployId,
        timeout: Duration,
    ) -> Option<bool> {
        wait_for_finalization(
            &self.read_client,
            &self.observer_node_events,
            deploy_id,
            timeout,
        )
        .await
    }
}

async fn submit_and_finalize_effect<R: ReadNode, W: WriteNode, N: NodeEventSource>(
    mut write: W,
    read: &R,
    events: &N,
    contract: SignedCode,
    timeout: Duration,
) -> anyhow::Result<(DeployId, Option<bool>)> {
    let signed_id = DeployId::from(hex::encode(&contract.sig));
    match write.deploy_signed_contract(contract).await {
        Ok(id) => {
            let finalized = wait_for_finalization(read, events, &id, timeout).await;
            Ok((id, finalized))
        }
        Err(error) => {
            tracing::warn!(deploy_id = %signed_id, %error, "Effect submission was not acknowledged; checking finality of the persisted signed deployment");
            match wait_for_finalization(read, events, &signed_id, timeout).await {
                Some(errored) => Ok((signed_id, Some(errored))),
                None => Err(error.context(format!(
                    "Effect {signed_id} submission remains unconfirmed; its persisted outbox is available for recovery"
                ))),
            }
        }
    }
}

async fn wait_for_finalization<R: ReadNode, N: NodeEventSource>(
    read: &R,
    events: &N,
    deploy_id: &DeployId,
    timeout: Duration,
) -> Option<bool> {
    let notification = events.wait_for_deploy(deploy_id, timeout);
    let polling = async {
        loop {
            match read.deploy_finalization(deploy_id).await {
                Ok(Some(errored)) => return Some(errored),
                Ok(None) => {}
                Err(error) => {
                    tracing::debug!(%deploy_id, %error, "Observer finalization read failed; awaiting finality within the existing deadline")
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(notification, polling, deadline);
    let mut notification_open = true;
    loop {
        tokio::select! {
            biased;
            result = &mut notification, if notification_open => {
                if result.is_some() {
                    return result;
                }
                notification_open = false;
            }
            () = &mut deadline => return None,
            result = &mut polling => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::domain::test_helpers::{MockNodeEventSource, MockReadNode, MockWriteNode};

    fn signed_effect() -> SignedCode {
        SignedCode {
            contract: vec![1, 2, 3],
            sig: vec![0xab, 0xcd],
            sig_algorithm: "secp256k1".to_owned(),
            deployer: vec![],
        }
    }

    #[tokio::test]
    async fn lost_submission_acknowledgement_uses_exact_signed_id_without_resubmitting() {
        for errored in [false, true] {
            let read =
                MockReadNode::new().on_code_containing("deploy-finalization:abcd", json!(errored));
            let events = MockNodeEventSource::new().with_wait_result("abcd", None);
            let write = MockWriteNode::new()
                .with_deploy_response(Err(anyhow::anyhow!("submission RPC timed out")));
            let (id, result) = submit_and_finalize_effect(
                write.clone(),
                &read,
                &events,
                signed_effect(),
                Duration::from_secs(2),
            )
            .await
            .expect("observer proves the submitted effect finalized");
            assert_eq!(id.to_string(), "abcd");
            assert_eq!(result, Some(errored));
            assert_eq!(write.deployed_contracts(), vec![vec![1, 2, 3]]);
            assert_eq!(read.calls().len(), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn unconfirmed_submission_preserves_error_for_recovery_at_deadline() {
        let read = MockReadNode::new();
        let events = MockNodeEventSource::new().with_wait_result("abcd", None);
        let write = MockWriteNode::new()
            .with_deploy_response(Err(anyhow::anyhow!("submission RPC timed out")));
        let start = tokio::time::Instant::now();
        let error = submit_and_finalize_effect(
            write.clone(),
            &read,
            &events,
            signed_effect(),
            Duration::from_secs(3),
        )
        .await
        .expect_err("no acknowledgement or finality is not success");
        assert!(format!("{error:#}").contains("submission RPC timed out"));
        assert!(error.to_string().contains("persisted outbox"));
        assert_eq!(start.elapsed(), Duration::from_secs(3));
        assert_eq!(write.deployed_contracts().len(), 1);
    }

    #[tokio::test]
    async fn cached_notification_needs_no_status_request() {
        let read = MockReadNode::new();
        let events = MockNodeEventSource::new().with_wait_result("cached", Some(false));
        let id = DeployId::from("cached".to_owned());
        assert_eq!(
            wait_for_finalization(&read, &events, &id, Duration::ZERO).await,
            Some(false)
        );
        assert!(read.calls().is_empty());
    }

    #[tokio::test]
    async fn missed_notification_uses_finalized_observer_status() {
        for errored in [false, true] {
            let read = MockReadNode::new()
                .on_code_containing("deploy-finalization:missed", json!(errored));
            let events = MockNodeEventSource::new().with_wait_result("missed", None);
            let id = DeployId::from("missed".to_owned());
            assert_eq!(
                wait_for_finalization(&read, &events, &id, Duration::from_secs(2)).await,
                Some(errored)
            );
            assert_eq!(read.calls().len(), 1);
        }
    }

    #[tokio::test]
    async fn pending_observer_status_does_not_claim_success() {
        let read = MockReadNode::new();
        let events = MockNodeEventSource::new().with_wait_result("pending", None);
        let id = DeployId::from("pending".to_owned());
        let timeout = Duration::from_millis(20);
        let started = tokio::time::Instant::now();
        assert_eq!(
            wait_for_finalization(&read, &events, &id, timeout).await,
            None
        );
        assert!(started.elapsed() >= timeout);
        assert_eq!(read.calls().len(), 1);
    }

    #[tokio::test]
    async fn observer_can_finalize_after_the_notification_stream_closes() {
        let read = MockReadNode::new().on_code_containing_empty_then(
            "deploy-finalization:later",
            1,
            json!(false),
        );
        let events = MockNodeEventSource::new().with_wait_result("later", None);
        let id = DeployId::from("later".to_owned());
        assert_eq!(
            wait_for_finalization(&read, &events, &id, Duration::from_secs(3)).await,
            Some(false)
        );
        assert_eq!(read.calls().len(), 2);
    }
}
