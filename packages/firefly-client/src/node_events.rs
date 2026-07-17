use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::task::ready;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use dashmap::DashMap;
use futures::{Stream, StreamExt};
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
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

impl DeployEvent {
    const fn deploy_id(&self) -> &DeployId {
        match self {
            Self::Finalized { id, .. } => id,
        }
    }
}

type DeploySubscriptions = Arc<DashMap<DeployId, DashMap<Uuid, oneshot::Sender<bool>>>>;
type WalletSubscriptions = Arc<DashMap<WalletAddress, broadcast::Sender<DeployEvent>>>;

/// A deploy that finalized recently, retained so that late or reconnecting
/// subscribers can still observe it.
///
/// Carries everything needed to reconstruct a wallet-scoped `DeployEvent`. The
/// previous `(errored, Instant)` tuple could not — it lacked `cost` and the
/// deployer — which is why a client whose WebSocket was down at finalization
/// never learned that its deploy had landed, and reported a successful deploy
/// as a timeout.
#[derive(Debug, Clone)]
struct FinalizedDeploy {
    wallet_address: WalletAddress,
    cost: u64,
    errored: bool,
    finalized_at: tokio::time::Instant,
}

impl FinalizedDeploy {
    const fn to_event(&self, id: DeployId) -> DeployEvent {
        DeployEvent::Finalized {
            id,
            cost: self.cost,
            errored: self.errored,
        }
    }
}

/// Cache of recently finalized deploys so late subscribers can still observe them.
type FinalizedDeploys = Arc<DashMap<DeployId, FinalizedDeploy>>;

/// How long finalized deploy results are retained in the cache.
const FINALIZED_CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// How often the cache cleanup task runs.
const FINALIZED_CACHE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

/// Ring capacity for each wallet's live-event broadcast. Sized so that a burst
/// of blocks cannot lag a subscriber that is briefly slow to poll.
const WALLET_BROADCAST_CAPACITY: usize = 256;

/// Upper bound on how many cached events one `subscribe` may replay, so a client
/// cannot be flooded on connect.
const MAX_REPLAY_EVENTS: usize = 256;

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
        tokio::spawn(
            dispatch_events(
                tx.subscribe(),
                deploy_subscriptions.clone(),
                wallet_subscriptions.clone(),
                finalized_deploys.clone(),
            )
            .in_current_span(),
        );

        // Cache cleanup task — evict entries older than FINALIZED_CACHE_TTL.
        tokio::spawn({
            let finalized_deploys = finalized_deploys.clone();
            async move {
                loop {
                    tokio::time::sleep(FINALIZED_CACHE_CLEANUP_INTERVAL).await;
                    let cutoff = tokio::time::Instant::now() - FINALIZED_CACHE_TTL;
                    finalized_deploys.retain(|_, entry| entry.finalized_at > cutoff);
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
        let cached = self
            .finalized_deploys
            .get(deploy_id)
            .map(|entry| entry.errored);

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

    /// Subscribe to finalization events for `wallet_address`.
    ///
    /// Delivers, in order: any deploys for this wallet that finalized within
    /// `FINALIZED_CACHE_TTL` (oldest first), followed by live events.
    ///
    /// The replay is what makes a transient WebSocket drop survivable. Before
    /// it, a deploy that finalized while the client was disconnected — or in the
    /// window between the deploy being submitted and the client subscribing —
    /// was lost forever, and the client reported a *successful* deploy as a
    /// timeout.
    ///
    /// Ordering is load-bearing. We join the live broadcast (S1) *before*
    /// scanning the cache (S2). Dispatch inserts into the cache (D1) before it
    /// sends to the broadcast (D2). So for any deploy D:
    ///   * D2 after S1  — our receiver's cursor was set at S1, so we get it live.
    ///   * D2 before S1 — then D1 < D2 < S1 < S2, so the S2 scan must observe it.
    ///
    /// The cases are exhaustive, so no deploy can slip between the two paths. The
    /// naive order (scan, then subscribe) leaves exactly such a gap — and that gap
    /// is the bug being fixed here. Both paths can fire for the same deploy, so
    /// replayed ids are deduped against the live stream in
    /// `WalletSubscription::poll_next`.
    pub fn subscribe_for_deploys(&self, wallet_address: WalletAddress) -> WalletSubscription {
        // S1: join the live broadcast first.
        //
        // `subscribe()` must happen *inside* the entry guard: it serializes us
        // against a concurrent last-subscriber `Drop`, which could otherwise
        // remove the entry between a clone and the subscribe, leaving us holding
        // a receiver on an orphaned sender that dispatch would never find. The
        // guard must be released before S2 — holding a `wallet_subscriptions`
        // shard while taking `finalized_deploys` shards would nest the two maps
        // in the opposite order from dispatch.
        let rx = {
            let tx = self
                .wallet_subscriptions
                .entry(wallet_address.clone())
                .or_insert_with(|| broadcast::Sender::new(WALLET_BROADCAST_CAPACITY));
            tx.subscribe()
        };

        // S2: snapshot the cache. TTL-filtered here rather than trusting the
        // cleanup task, which only runs every FINALIZED_CACHE_CLEANUP_INTERVAL.
        //
        // `checked_sub` because this runs on the request path: a bare
        // `Instant::now() - TTL` panics if the process has been up for less than
        // the TTL. `None` means "nothing can be older than the TTL yet", so every
        // cached entry is still eligible.
        let cutoff = tokio::time::Instant::now().checked_sub(FINALIZED_CACHE_TTL);
        let mut replay: Vec<(tokio::time::Instant, DeployEvent)> = self
            .finalized_deploys
            .iter()
            .filter(|entry| {
                entry.wallet_address == wallet_address
                    && cutoff.is_none_or(|cutoff| entry.finalized_at > cutoff)
            })
            .map(|entry| (entry.finalized_at, entry.to_event(entry.key().clone())))
            .collect();

        // DashMap iteration order is shard-arbitrary; sort so replay is chronological.
        replay.sort_by_key(|(at, _)| *at);
        if replay.len() > MAX_REPLAY_EVENTS {
            replay.drain(..replay.len() - MAX_REPLAY_EVENTS); // keep the newest
        }

        let replay: VecDeque<DeployEvent> = replay.into_iter().map(|(_, event)| event).collect();
        let replayed: HashSet<DeployId> = replay
            .iter()
            .map(|event| event.deploy_id().clone())
            .collect();

        WalletSubscription {
            wallet_address,
            wallet_subscriptions: self.wallet_subscriptions.clone(),
            replay,
            replayed,
            rx: BroadcastStream::new(rx),
        }
    }
}

/// Consume node events and fan finalized deploys out to waiters, wallet
/// subscribers, and the replay cache.
///
/// Shared by `NodeEvents::new` and `NodeEvents::new_for_test` so the two cannot
/// drift — they previously held copy-pasted, already-divergent copies, which
/// meant tests could exercise different logic than production.
async fn dispatch_events(
    mut rx: broadcast::Receiver<NodeEvent>,
    deploy_subscriptions: DeploySubscriptions,
    wallet_subscriptions: WalletSubscriptions,
    finalized_deploys: FinalizedDeploys,
) {
    loop {
        let deploys = match rx.recv().await {
            Ok(NodeEvent::Started | NodeEvent::Other) => continue,
            Ok(NodeEvent::BlockAdded { .. }) => continue,
            Ok(NodeEvent::BlockCreated { .. }) => continue,
            Ok(NodeEvent::BlockFinalised { payload }) => {
                tracing::info!(deploy_count = payload.deploys.len(), "block finalised");
                for deploy in &payload.deploys {
                    tracing::info!(
                        deploy_id = ?deploy.id,
                        errored = deploy.errored,
                        cost = deploy.cost,
                        "  deploy in finalized block"
                    );
                }
                payload.deploys
            }
            Err(broadcast::error::RecvError::Closed) => return,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
        };

        for deploy in deploys {
            let errored = deploy.errored;
            let wallet_address: WalletAddress = deploy.deployer.into();

            // D1: cache BEFORE the live send, so a subscriber that misses the
            // send still finds the deploy when it scans (see subscribe_for_deploys).
            finalized_deploys.insert(
                deploy.id.clone(),
                FinalizedDeploy {
                    wallet_address: wallet_address.clone(),
                    cost: deploy.cost,
                    errored,
                    finalized_at: tokio::time::Instant::now(),
                },
            );

            // Notify any existing waiters via oneshot channels.
            deploy_subscriptions
                .remove(&deploy.id)
                .map(|(_, waiters)| waiters)
                .into_iter()
                .flatten()
                .for_each(|(_, sender)| {
                    let _ = sender.send(errored);
                });

            // D2: then send live.
            if let Some(subscription) = wallet_subscriptions.get(&wallet_address) {
                let _ = subscription.send(deploy.into());
            } else {
                tracing::debug!(
                    deploy_id = ?deploy.id,
                    wallet_address = ?wallet_address,
                    "no live wallet subscription for deploy; cached for replay"
                );
            }
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
    /// Cached events (oldest first) captured at subscribe time, drained before
    /// any live event is yielded.
    replay: VecDeque<DeployEvent>,
    /// Ids present in `replay`, so that the same deploy arriving live is not
    /// emitted twice. Seeded once at construction and only ever drained, so it
    /// is bounded by the replay snapshot and never grows.
    replayed: HashSet<DeployId>,
    rx: BroadcastStream<DeployEvent>,
}

impl Stream for WalletSubscription {
    type Item = DeployEvent;

    // The `continue` in the `Lagged` arm is technically redundant (the loop would
    // iterate anyway), but it is kept deliberately and explicitly: it is the
    // entire point of the fix. Without re-polling, this task is left with no
    // waker registered and the stream stalls forever. Leaving the arm to fall out
    // silently invites a future reader to "tidy" it into a `Poll::Pending`, which
    // would reintroduce the bug.
    #[allow(clippy::needless_continue)]
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // History first, oldest first. Draining the replay fully before polling
        // the live stream makes it impossible to emit a deploy live and then
        // replay it out of order, and gives a simple contract: history, then live.
        if let Some(event) = this.replay.pop_front() {
            return std::task::Poll::Ready(Some(event));
        }

        loop {
            match ready!(this.rx.poll_next_unpin(cx)) {
                Some(Ok(event)) => {
                    // Skip a live event we already emitted from the replay snapshot
                    // (both paths can legitimately fire for the same deploy).
                    if !this.replayed.is_empty() && this.replayed.remove(event.deploy_id()) {
                        continue;
                    }
                    return std::task::Poll::Ready(Some(event));
                }
                Some(Err(BroadcastStreamRecvError::Lagged(skipped))) => {
                    tracing::warn!(
                        wallet_address = ?this.wallet_address,
                        skipped,
                        "wallet deploy subscription lagged; dropped events (recoverable on resubscribe)"
                    );
                    // MUST re-poll rather than return `Pending`. On `Lagged`,
                    // `BroadcastStream` has already replaced its inner `Recv`
                    // future — dropping the waiter node registered with the
                    // channel — without polling the fresh one. Returning
                    // `Pending` here would leave this task with NO waker
                    // registered, so no later `send` could ever wake it: the
                    // stream would stall permanently and the client would once
                    // again see a successful deploy as a timeout. `ready!` only
                    // yields `Pending` when the inner stream is `Pending`, which
                    // is exactly when the waker *is* armed.
                    continue;
                }
                None => return std::task::Poll::Ready(None), // all senders dropped
            }
        }
    }
}

impl Drop for WalletSubscription {
    fn drop(&mut self) {
        self.wallet_subscriptions
            .remove_if(&self.wallet_address, |_, sender| {
                // `<= 1`, not `== 0`: a type's fields are dropped *after* its
                // `Drop::drop` body runs, so `self.rx` still owns our own
                // receiver here and the count always includes us. The previous
                // `== 0` could therefore never be true — the entry and its ring
                // buffer leaked for every wallet that ever connected.
                sender.receiver_count() <= 1
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

        // Spawn only the event dispatch task — the SAME function production
        // uses, so tests cannot exercise different logic than production (the
        // two previously held copy-pasted, already-divergent loops).
        tokio::spawn(dispatch_events(
            tx.subscribe(),
            deploy_subscriptions.clone(),
            wallet_subscriptions.clone(),
            finalized_deploys.clone(),
        ));

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

    /// Helper to create a `BlockFinalised` event with a single deploy.
    fn finalize_event(deploy_id: &str, errored: bool, cost: u64) -> NodeEvent {
        finalize_event_for(deploy_id, errored, cost, 1)
    }

    /// Like `finalize_event`, but for a chosen deployer key.
    fn finalize_event_for(deploy_id: &str, errored: bool, cost: u64, key_index: u8) -> NodeEvent {
        NodeEvent::BlockFinalised {
            payload: BlockEventPayload {
                block_hash: BlockId::from("block-1".to_owned()),
                deploys: vec![BlockEventDeploy {
                    id: DeployId::from(deploy_id.to_owned()),
                    cost,
                    deployer: test_public_key(key_index),
                    errored,
                }],
            },
        }
    }

    /// A single `BlockFinalised` carrying `count` deploys, used to overrun a
    /// subscriber's broadcast ring within one dispatch pass. Keeping it to ONE
    /// `NodeEvent` means the dispatch channel itself cannot lag, so the test is
    /// deterministic.
    fn finalize_event_burst(count: usize, key_index: u8) -> NodeEvent {
        NodeEvent::BlockFinalised {
            payload: BlockEventPayload {
                block_hash: BlockId::from("block-burst".to_owned()),
                deploys: (0..count)
                    .map(|i| BlockEventDeploy {
                        id: DeployId::from(format!("deploy-{i}")),
                        cost: i as u64,
                        deployer: test_public_key(key_index),
                        errored: false,
                    })
                    .collect(),
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
        events.finalized_deploys.insert(
            deploy_id.clone(),
            FinalizedDeploy {
                wallet_address: test_public_key(1).into(),
                cost: 0,
                errored: false,
                finalized_at: tokio::time::Instant::now(),
            },
        );

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
        let w1 = tokio::spawn(async move { e1.wait_for_deploy(&d1, Duration::from_secs(5)).await });

        let e2 = events.clone();
        let d2 = deploy_id.clone();
        let w2 = tokio::spawn(async move { e2.wait_for_deploy(&d2, Duration::from_secs(5)).await });

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
    // replay tests (the "successful deploy reported as timeout" bug)
    // -------------------------------------------------------

    /// The reported incident, distilled: the deploy finalized while nobody was
    /// subscribed (the browser's WebSocket was down), so the live event was
    /// dropped on the floor and a SUCCESSFUL deploy surfaced to the user as a
    /// timeout. A late subscriber must still observe it.
    #[tokio::test]
    async fn test_subscribe_replays_deploy_finalized_before_subscribe() {
        let (events, tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        // Finalize with NO subscriber listening.
        let _ = tx.send(finalize_event("deploy-early", false, 777));
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Only now subscribe — the event must still be delivered.
        let mut sub = events.subscribe_for_deploys(wallet_address);

        match tokio::time::timeout(Duration::from_millis(200), sub.next()).await {
            Ok(Some(DeployEvent::Finalized { id, cost, errored })) => {
                assert_eq!(id, DeployId::from("deploy-early".to_owned()));
                assert_eq!(cost, 777, "cost must survive the cache round-trip");
                assert!(!errored);
            }
            other => panic!("expected replayed Finalized event, got {other:?}"),
        }
    }

    /// The exact shape of the incident: subscribed, the WebSocket drops, the
    /// deploy finalizes during the gap, then the client reconnects.
    #[tokio::test]
    async fn test_subscribe_replay_after_reconnect_gap() {
        let (events, tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        let sub = events.subscribe_for_deploys(wallet_address.clone());
        drop(sub); // the WebSocket drops

        // Finalization happens while nobody is listening.
        let _ = tx.send(finalize_event("deploy-gap", false, 5));
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Client reconnects — must learn the deploy landed.
        let mut sub = events.subscribe_for_deploys(wallet_address);

        match tokio::time::timeout(Duration::from_millis(200), sub.next()).await {
            Ok(Some(DeployEvent::Finalized { id, .. })) => {
                assert_eq!(id, DeployId::from("deploy-gap".to_owned()));
            }
            other => panic!("expected replay after reconnect, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_replay_is_scoped_to_wallet() {
        let (events, tx) = NodeEvents::new_for_test();

        let _ = tx.send(finalize_event_for("deploy-w1", false, 1, 1));
        let _ = tx.send(finalize_event_for("deploy-w2", false, 2, 2));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let wallet_1: WalletAddress = test_public_key(1).into();
        let mut sub = events.subscribe_for_deploys(wallet_1);

        match tokio::time::timeout(Duration::from_millis(200), sub.next()).await {
            Ok(Some(DeployEvent::Finalized { id, .. })) => {
                assert_eq!(id, DeployId::from("deploy-w1".to_owned()));
            }
            other => panic!("expected only wallet 1's deploy, got {other:?}"),
        }

        assert!(
            tokio::time::timeout(Duration::from_millis(100), sub.next())
                .await
                .is_err(),
            "another wallet's deploy must not be replayed into this stream"
        );
    }

    /// The subscribe-then-scan ordering deliberately makes the
    /// "replayed AND delivered live" window reachable, so the same deploy must
    /// still be emitted exactly once.
    #[tokio::test]
    async fn test_no_duplicate_when_replayed_deploy_also_arrives_live() {
        let (events, tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        // Cache deploy-x with nobody listening.
        let _ = tx.send(finalize_event("deploy-x", false, 1));
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Subscribe: deploy-x is now in the replay snapshot.
        let mut sub = events.subscribe_for_deploys(wallet_address);

        // deploy-x ALSO arrives live, followed by a genuinely new deploy-y.
        let _ = tx.send(finalize_event("deploy-x", false, 1));
        let _ = tx.send(finalize_event("deploy-y", false, 2));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut seen = Vec::new();
        while let Ok(Some(DeployEvent::Finalized { id, .. })) =
            tokio::time::timeout(Duration::from_millis(150), sub.next()).await
        {
            seen.push(id);
        }

        assert_eq!(
            seen,
            vec![
                DeployId::from("deploy-x".to_owned()),
                DeployId::from("deploy-y".to_owned())
            ],
            "deploy-x must be emitted exactly once (from replay), not twice"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_replay_skips_entries_older_than_ttl() {
        let (events, _tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        events.finalized_deploys.insert(
            DeployId::from("deploy-stale".to_owned()),
            FinalizedDeploy {
                wallet_address: wallet_address.clone(),
                cost: 1,
                errored: false,
                finalized_at: tokio::time::Instant::now(),
            },
        );

        tokio::time::advance(FINALIZED_CACHE_TTL + Duration::from_secs(1)).await;

        events.finalized_deploys.insert(
            DeployId::from("deploy-fresh".to_owned()),
            FinalizedDeploy {
                wallet_address: wallet_address.clone(),
                cost: 2,
                errored: false,
                finalized_at: tokio::time::Instant::now(),
            },
        );

        let sub = events.subscribe_for_deploys(wallet_address);

        let replayed: Vec<DeployId> = sub
            .replay
            .iter()
            .map(|event| event.deploy_id().clone())
            .collect();
        assert_eq!(
            replayed,
            vec![DeployId::from("deploy-fresh".to_owned())],
            "entries older than the TTL must not be replayed"
        );
    }

    /// Regression: a `Lagged` must not stall the stream forever.
    ///
    /// The old `poll_next` mapped `Lagged` to `Poll::Pending` *without a
    /// registered waker* (`BroadcastStream` drops its waiter node on lag), so the
    /// subscription went permanently silent — producing the very same
    /// "successful deploy reported as a timeout" symptom. This test hangs on the
    /// old code and passes with the fix.
    #[tokio::test]
    async fn test_subscribe_lagged_does_not_stall_stream() {
        let (events, tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        let mut sub = events.subscribe_for_deploys(wallet_address);

        // Overrun the wallet ring within a single dispatch pass while nobody polls.
        let burst = WALLET_BROADCAST_CAPACITY + 44;
        let _ = tx.send(finalize_event_burst(burst, 1));
        tokio::time::sleep(Duration::from_millis(200)).await;

        // We must still make progress and reach the newest deploy.
        let newest = DeployId::from(format!("deploy-{}", burst - 1));
        let found = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(DeployEvent::Finalized { id, .. }) = sub.next().await {
                if id == newest {
                    return true;
                }
            }
            false
        })
        .await
        .expect("stream stalled permanently after Lagged");

        assert!(found, "must still reach the newest deploy after lagging");
    }

    /// Regression: the wallet entry (and its ring) must be reclaimed when the
    /// last subscriber drops. The old `receiver_count() == 0` could never be true
    /// — fields drop *after* `Drop::drop` — so every wallet that ever connected
    /// leaked an entry forever.
    #[tokio::test]
    async fn test_wallet_subscription_entry_removed_on_last_drop() {
        let (events, _tx) = NodeEvents::new_for_test();
        let wallet_address: WalletAddress = test_public_key(1).into();

        let sub = events.subscribe_for_deploys(wallet_address.clone());
        assert!(events.wallet_subscriptions.contains_key(&wallet_address));

        drop(sub);
        assert!(
            !events.wallet_subscriptions.contains_key(&wallet_address),
            "entry must be reclaimed when the last subscriber drops"
        );
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
