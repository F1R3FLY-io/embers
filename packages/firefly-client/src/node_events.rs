use std::sync::Arc;
use std::task::ready;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use dashmap::DashMap;
use futures::{Stream, StreamExt};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::Instrument;
use uuid::Uuid;

use crate::models::{BlockEventDeploy, DeployId, NodeEvent, WalletAddress};

#[derive(Debug, Clone)]
pub enum DeployEvent {
    Finalized {
        id: DeployId,
        cost: u64,
        errored: bool,
    },
}

type DeploySubscriptions = Arc<DashMap<DeployId, DashMap<Uuid, oneshot::Sender<bool>>>>;
type WalletSubscriptions = Arc<DashMap<WalletAddress, broadcast::Sender<DeployEvent>>>;

/// Cache of recently finalized deploys so late subscribers can still observe them.
/// Maps deploy_id -> (errored, finalized_at).
type FinalizedDeploys = Arc<DashMap<DeployId, (bool, tokio::time::Instant)>>;

/// How long finalized deploy results are retained in the cache.
const FINALIZED_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// How often the cache cleanup task runs.
const FINALIZED_CACHE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct NodeEvents {
    deploy_subscriptions: DeploySubscriptions,
    wallet_subscriptions: WalletSubscriptions,
    finalized_deploys: FinalizedDeploys,
}

impl NodeEvents {
    pub fn new(url: &str) -> Self {
        let url = format!("{url}/ws/events");
        let tx = broadcast::Sender::<NodeEvent>::new(32);
        let deploy_subscriptions = DeploySubscriptions::default();
        let wallet_subscriptions = WalletSubscriptions::default();
        let finalized_deploys = FinalizedDeploys::default();

        // WebSocket connection task
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
                            Ok(event) => {
                                tracing::debug!("ignored ws event: {event:?}");
                                continue;
                            }
                            Err(err) => {
                                tracing::debug!("ws error: {err:?}");
                                continue;
                            }
                        };

                        let event = match serde_json::from_str(&buff) {
                            Ok(event) => event,
                            Err(err) => {
                                tracing::warn!("failed to deserialize ws event: {err:?}");
                                continue;
                            }
                        };

                        let _ = tx.send(event);
                    }
                }
            }
            .in_current_span()
        });

        // Event dispatch task
        tokio::spawn({
            let mut rx = tx.subscribe();
            let deploy_subscriptions = deploy_subscriptions.clone();
            let wallet_subscriptions = wallet_subscriptions.clone();
            let finalized_deploys = finalized_deploys.clone();
            async move {
                loop {
                    let deploys = match rx.recv().await {
                        Ok(NodeEvent::Started) => continue,
                        Ok(NodeEvent::BlockAdded { .. }) => continue,
                        Ok(NodeEvent::BlockCreated { .. }) => continue,
                        Ok(NodeEvent::BlockFinalised { payload }) => {
                            tracing::info!(
                                deploy_count = payload.deploys.len(),
                                "block finalised"
                            );
                            for deploy in &payload.deploys {
                                tracing::info!(
                                    deploy_id = ?deploy.id,
                                    errored = deploy.errored,
                                    cost = deploy.cost,
                                    "  deploy in finalized block"
                                );
                            }
                            payload.deploys
                        },
                        Err(broadcast::error::RecvError::Closed) => return,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    };

                    for deploy in deploys {
                        let errored = deploy.errored;

                        // Cache the finalization result so late subscribers can find it.
                        finalized_deploys.insert(
                            deploy.id.clone(),
                            (errored, tokio::time::Instant::now()),
                        );

                        // Notify any existing waiters via oneshot channels.
                        deploy_subscriptions
                            .remove(&deploy.id)
                            .map(|(_, waiters)| waiters)
                            .into_iter()
                            .flatten()
                            .for_each(|(_, sender)| { let _ = sender.send(errored); });

                        let wallet_address: WalletAddress = deploy.deployer.into();
                        if let Some(subscription) =
                            wallet_subscriptions.get(&wallet_address)
                        {
                            let _ = subscription.send(deploy.into());
                        } else {
                            tracing::debug!(
                                deploy_id = ?deploy.id,
                                wallet_address = ?wallet_address,
                                "no wallet subscription for deploy"
                            );
                        }
                    }
                }
            }
            .in_current_span()
        });

        // Cache cleanup task — evict entries older than FINALIZED_CACHE_TTL.
        tokio::spawn({
            let finalized_deploys = finalized_deploys.clone();
            async move {
                loop {
                    tokio::time::sleep(FINALIZED_CACHE_CLEANUP_INTERVAL).await;
                    let cutoff = tokio::time::Instant::now() - FINALIZED_CACHE_TTL;
                    finalized_deploys.retain(|_, (_, finalized_at)| *finalized_at > cutoff);
                }
            }
        });

        Self {
            deploy_subscriptions,
            wallet_subscriptions,
            finalized_deploys,
        }
    }

    /// Wait for a deploy to be finalized, returning `Some(errored)` on success
    /// or `None` on timeout.
    ///
    /// Uses a register-then-check pattern to avoid the race where finalization
    /// occurs between `deploy_signed_contract()` returning and this subscription
    /// being established:
    ///
    /// 1. Register oneshot in `deploy_subscriptions`
    /// 2. Check `finalized_deploys` cache — if hit, return immediately
    /// 3. Otherwise await the oneshot with timeout
    pub fn wait_for_deploy(
        &self,
        deploy_id: &DeployId,
        max_wait: Duration,
    ) -> impl Future<Output = Option<bool>> {
        let id = Uuid::now_v7();
        let (tx, rx) = oneshot::channel::<bool>();

        // Step 1: Register subscription FIRST (before checking cache).
        self.deploy_subscriptions
            .entry(deploy_id.clone())
            .or_default()
            .insert(id, tx);

        // Step 2: Check if the deploy was already finalized (cache hit).
        // This closes the race: if finalization happened before step 1, we catch
        // it here. If it happened between steps 1 and 2, both the cache and the
        // oneshot will have the result — we just use the cache.
        let cached = self.finalized_deploys.get(deploy_id).map(|entry| entry.0);

        if cached.is_some() {
            // Remove our subscription since we don't need it.
            self.deploy_subscriptions.remove_if(deploy_id, |_, submap| {
                submap.remove(&id);
                submap.is_empty()
            });
        }

        let deploy_subscriptions = self.deploy_subscriptions.clone();
        let deploy_id = deploy_id.clone();

        let guard = scopeguard::guard((), move |()| {
            deploy_subscriptions.remove_if(&deploy_id, |_, submap| {
                submap.remove(&id);
                submap.is_empty()
            });
        });

        async move {
            // If we got a cache hit, return immediately.
            if let Some(errored) = cached {
                scopeguard::ScopeGuard::into_inner(guard); // defuse
                return Some(errored);
            }

            // Step 3: Await the oneshot with timeout.
            tokio::select! {
                result = rx => {
                    scopeguard::ScopeGuard::into_inner(guard); // defuse
                    result.ok() // Some(errored) or None if sender dropped
                },
                _ = tokio::time::sleep(max_wait) => None,
            }
        }
    }

    pub fn subscribe_for_deploys(&self, wallet_address: WalletAddress) -> WalletSubscription {
        let tx = self
            .wallet_subscriptions
            .entry(wallet_address.clone())
            .or_insert_with(|| broadcast::Sender::new(32));

        WalletSubscription {
            wallet_address,
            wallet_subscriptions: self.wallet_subscriptions.clone(),
            rx: BroadcastStream::new(tx.subscribe()),
        }
    }
}

impl From<BlockEventDeploy> for DeployEvent {
    fn from(value: BlockEventDeploy) -> Self {
        Self::Finalized {
            id: value.id,
            cost: value.cost,
            errored: value.errored,
        }
    }
}

pub struct WalletSubscription {
    wallet_address: WalletAddress,
    wallet_subscriptions: WalletSubscriptions,
    rx: BroadcastStream<DeployEvent>,
}

impl Stream for WalletSubscription {
    type Item = DeployEvent;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        ready!(self.rx.poll_next_unpin(cx))
            .transpose()
            .map_or(std::task::Poll::Pending, std::task::Poll::Ready)
    }
}

impl Drop for WalletSubscription {
    fn drop(&mut self) {
        self.wallet_subscriptions
            .remove_if(&self.wallet_address, |_, sender| {
                sender.receiver_count() == 0
            });
    }
}
