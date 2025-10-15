use std::sync::Arc;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use dashmap::DashMap;
use futures::StreamExt;
use tokio::sync::{Notify, broadcast};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::models::{DeployId, NodeEvent};

type DeploySubscriptions = Arc<DashMap<DeployId, DashMap<Uuid, Arc<Notify>>>>;

#[derive(Clone)]
pub struct NodeEvents {
    deploy_subscriptions: DeploySubscriptions,
}

impl NodeEvents {
    pub fn new(url: &str) -> Self {
        let url = format!("{url}/ws/events");
        let tx = broadcast::Sender::<NodeEvent>::new(32);
        let deploy_subscriptions = DeploySubscriptions::default();

        tokio::spawn({
            let tx = tx.clone();
            async move {
                loop {
                    let Ok((mut stream, _)) =
                        (|| async { tokio_tungstenite::connect_async(&url).await })
                            .retry(ExponentialBuilder::default().without_max_times())
                            .await
                    else {
                        return;
                    };

                    while let Some(msg) = stream.next().await {
                        let buff = match msg {
                            Ok(Message::Text(buff)) => buff,
                            Ok(_) => continue,
                            Err(err) => {
                                tracing::debug!("ws error: {err:?}");
                                continue;
                            }
                        };

                        let event = match serde_json::from_str(&buff) {
                            Ok(event) => event,
                            Err(err) => {
                                tracing::debug!("serde ws error: {err:?}");
                                continue;
                            }
                        };

                        let _ = tx.send(event);
                    }
                }
            }
        });

        tokio::spawn({
            let mut rx = tx.subscribe();
            let deploy_subscriptions = deploy_subscriptions.clone();
            async move {
                loop {
                    let deploy_ids = match rx.recv().await {
                        Ok(NodeEvent::Started) => continue,
                        Ok(NodeEvent::BlockCreated { payload }) => payload.deploy_ids,
                        Ok(NodeEvent::BlockAdded { payload }) => payload.deploy_ids,
                        Ok(NodeEvent::BlockFinalised { .. }) => continue,
                        Err(broadcast::error::RecvError::Closed) => return,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    };

                    deploy_ids
                        .into_iter()
                        .filter_map(|deploy_id| {
                            deploy_subscriptions.remove(&deploy_id).map(|(_, v)| v)
                        })
                        .flatten()
                        .for_each(|(_, v)| v.notify_waiters());
                }
            }
        });

        Self {
            deploy_subscriptions,
        }
    }

    pub fn wait_for_deploy(
        &self,
        deploy_id: &DeployId,
        max_wait: Duration,
    ) -> impl Future<Output = bool> {
        let id = Uuid::now_v7();

        let notify = Arc::<Notify>::default();
        let notified = notify.clone().notified_owned();

        self.deploy_subscriptions
            .entry(deploy_id.clone())
            .or_default()
            .insert(id, notify);

        let guard = scopeguard::guard(
            self.deploy_subscriptions.clone(),
            move |deploy_subscriptions| {
                deploy_subscriptions.remove_if(deploy_id, |_, submap| {
                    submap.remove(&id);
                    submap.is_empty()
                });
            },
        );

        async move {
            tokio::select! {
                _ = notified => {
                    scopeguard::ScopeGuard::into_inner(guard); // defuse
                    true
                },
                _ = tokio::time::sleep(max_wait) => false,
            }
        }
    }
}
