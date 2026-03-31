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
use crate::traits::NodeEventSource;

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

#[cfg(test)]
impl NodeEvents {
    /// Creates a `NodeEvents` without spawning the WebSocket connection or
    /// cache-cleanup tasks.  Only the event dispatch task is started so that
    /// tests can inject events through the returned `broadcast::Sender`.
    pub(crate) fn new_for_test() -> (Self, broadcast::Sender<NodeEvent>) {
        let tx = broadcast::Sender::<NodeEvent>::new(32);
        let deploy_subscriptions = DeploySubscriptions::default();
        let wallet_subscriptions = WalletSubscriptions::default();
        let finalized_deploys = FinalizedDeploys::default();

        // Spawn only the event dispatch task (same logic as production).
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
                        Ok(NodeEvent::BlockFinalised { payload }) => payload.deploys,
                        Err(broadcast::error::RecvError::Closed) => return,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    };

                    for deploy in deploys {
                        let errored = deploy.errored;

                        finalized_deploys.insert(
                            deploy.id.clone(),
                            (errored, tokio::time::Instant::now()),
                        );

                        deploy_subscriptions
                            .remove(&deploy.id)
                            .map(|(_, waiters)| waiters)
                            .into_iter()
                            .flatten()
                            .for_each(|(_, sender)| {
                                let _ = sender.send(errored);
                            });

                        let wallet_address: WalletAddress = deploy.deployer.into();
                        if let Some(subscription) =
                            wallet_subscriptions.get(&wallet_address)
                        {
                            let _ = subscription.send(deploy.into());
                        }
                    }
                }
            }
        });

        (
            Self {
                deploy_subscriptions,
                wallet_subscriptions,
                finalized_deploys,
            },
            tx,
        )
    }
}

impl NodeEventSource for NodeEvents {
    fn wait_for_deploy(
        &self,
        deploy_id: &DeployId,
        max_wait: Duration,
    ) -> impl Future<Output = Option<bool>> + Send {
        self.wait_for_deploy(deploy_id, max_wait)
    }

    fn subscribe_for_deploys(
        &self,
        wallet_address: WalletAddress,
    ) -> impl Stream<Item = DeployEvent> + Send {
        self.subscribe_for_deploys(wallet_address)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::StreamExt;
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    use super::*;
    use crate::models::{BlockEventPayload, BlockId, NodeEvent};

    /// Helper to create a test public key deterministically from an index.
    fn test_public_key(index: u8) -> PublicKey {
        let secp = Secp256k1::new();
        let mut bytes = [0u8; 32];
        bytes[31] = index.max(1); // ensure non-zero
        let sk = SecretKey::from_byte_array(bytes).expect("valid secret key");
        PublicKey::from_secret_key(&secp, &sk)
    }

    /// Helper to create a BlockFinalised event with a single deploy.
    fn finalize_event(deploy_id: &str, errored: bool, cost: u64) -> NodeEvent {
        NodeEvent::BlockFinalised {
            payload: BlockEventPayload {
                block_hash: BlockId::from("block-1".to_owned()),
                deploys: vec![BlockEventDeploy {
                    id: DeployId::from(deploy_id.to_owned()),
                    cost,
                    deployer: test_public_key(1),
                    errored,
                }],
            },
        }
    }

    // -------------------------------------------------------
    // wait_for_deploy tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_wait_for_deploy_success() {
        let (events, tx) = NodeEvents::new_for_test();
        let deploy_id = DeployId::from("deploy-1".to_owned());

        let events_clone = events.clone();
        let deploy_id_clone = deploy_id.clone();
        let handle = tokio::spawn(async move {
            events_clone
                .wait_for_deploy(&deploy_id_clone, Duration::from_secs(5))
                .await
        });

        // Give the wait_for_deploy time to register
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let _ = tx.send(finalize_event("deploy-1", false, 100));

        let result = handle.await.expect("task should not panic");
        assert_eq!(result, Some(false));
    }

    #[tokio::test]
    async fn test_wait_for_deploy_errored() {
        let (events, tx) = NodeEvents::new_for_test();
        let deploy_id = DeployId::from("deploy-err".to_owned());

        let events_clone = events.clone();
        let deploy_id_clone = deploy_id.clone();
        let handle = tokio::spawn(async move {
            events_clone
                .wait_for_deploy(&deploy_id_clone, Duration::from_secs(5))
                .await
        });

        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let _ = tx.send(finalize_event("deploy-err", true, 50));

        let result = handle.await.expect("task should not panic");
        assert_eq!(result, Some(true));
    }

    #[tokio::test]
    async fn test_wait_for_deploy_timeout() {
        let (events, _tx) = NodeEvents::new_for_test();
        let deploy_id = DeployId::from("deploy-never".to_owned());

        let result = events
            .wait_for_deploy(&deploy_id, Duration::from_millis(50))
            .await;
        assert_eq!(result, None);

        // Verify subscription was cleaned up via scopeguard
        assert!(
            !events.deploy_subscriptions.contains_key(&deploy_id),
            "subscription should be cleaned up after timeout"
        );
    }

    #[tokio::test]
    async fn test_wait_for_deploy_cached_result_immediate() {
        let (events, _tx) = NodeEvents::new_for_test();
        let deploy_id = DeployId::from("deploy-cached".to_owned());

        // Pre-populate the cache
        events
            .finalized_deploys
            .insert(deploy_id.clone(), (false, tokio::time::Instant::now()));

        let result = events
            .wait_for_deploy(&deploy_id, Duration::from_secs(5))
            .await;
        assert_eq!(result, Some(false));
    }

    #[tokio::test]
    async fn test_wait_for_deploy_multiple_waiters() {
        let (events, tx) = NodeEvents::new_for_test();
        let deploy_id = DeployId::from("deploy-multi".to_owned());

        let e1 = events.clone();
        let d1 = deploy_id.clone();
        let w1 = tokio::spawn(async move {
            e1.wait_for_deploy(&d1, Duration::from_secs(5)).await
        });

        let e2 = events.clone();
        let d2 = deploy_id.clone();
        let w2 = tokio::spawn(async move {
            e2.wait_for_deploy(&d2, Duration::from_secs(5)).await
        });

        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let _ = tx.send(finalize_event("deploy-multi", true, 200));

        assert_eq!(w1.await.unwrap(), Some(true));
        assert_eq!(w2.await.unwrap(), Some(true));
    }

    // -------------------------------------------------------
    // subscribe_for_deploys tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_subscribe_receives_events() {
        let (events, tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        let mut sub = events.subscribe_for_deploys(wallet_address);

        // Send event
        let _ = tx.send(finalize_event("deploy-sub", false, 300));

        // Give dispatch task time to process
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Try to receive
        match tokio::time::timeout(Duration::from_millis(100), sub.next()).await {
            Ok(Some(DeployEvent::Finalized { id, cost, errored })) => {
                assert_eq!(id, DeployId::from("deploy-sub".to_owned()));
                assert_eq!(cost, 300);
                assert!(!errored);
            }
            other => panic!("expected Finalized event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_subscribe_drop_with_multiple_subscribers() {
        let (events, _tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        let sub1 = events.subscribe_for_deploys(wallet_address.clone());
        let sub2 = events.subscribe_for_deploys(wallet_address.clone());
        assert!(events.wallet_subscriptions.contains_key(&wallet_address));

        // Drop one -- entry should still exist since the other subscriber is alive
        drop(sub1);
        assert!(
            events.wallet_subscriptions.contains_key(&wallet_address),
            "entry should remain with active subscriber"
        );

        drop(sub2);
    }

    // -------------------------------------------------------
    // dispatch tests
    // -------------------------------------------------------

    #[tokio::test]
    async fn test_dispatch_ignores_non_finalised_events() {
        let (events, tx) = NodeEvents::new_for_test();
        let deploy_id = DeployId::from("deploy-ignored".to_owned());

        // Start waiting
        let events_clone = events.clone();
        let deploy_id_clone = deploy_id.clone();
        let handle = tokio::spawn(async move {
            events_clone
                .wait_for_deploy(&deploy_id_clone, Duration::from_millis(100))
                .await
        });

        // Send non-finalised events
        let _ = tx.send(NodeEvent::Started);
        let _ = tx.send(NodeEvent::BlockAdded {
            payload: BlockEventPayload {
                block_hash: BlockId::from("b1".to_owned()),
                deploys: vec![],
            },
        });
        let _ = tx.send(NodeEvent::BlockCreated {
            payload: BlockEventPayload {
                block_hash: BlockId::from("b2".to_owned()),
                deploys: vec![],
            },
        });

        // Should timeout since no BlockFinalised was sent
        let result = handle.await.unwrap();
        assert_eq!(result, None, "non-finalised events should not trigger wait");
    }

    // -------------------------------------------------------
    // DeployEvent conversion
    // -------------------------------------------------------

    #[test]
    fn test_block_event_deploy_into_deploy_event() {
        let deploy = BlockEventDeploy {
            id: DeployId::from("d1".to_owned()),
            cost: 42,
            deployer: test_public_key(1),
            errored: true,
        };

        let event: DeployEvent = deploy.into();
        match event {
            DeployEvent::Finalized { id, cost, errored } => {
                assert_eq!(id, DeployId::from("d1".to_owned()));
                assert_eq!(cost, 42);
                assert!(errored);
            }
        }
    }
}
