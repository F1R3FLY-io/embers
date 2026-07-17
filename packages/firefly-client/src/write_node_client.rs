use std::time::Duration;

use anyhow::{Context, anyhow};
use blake2::digest::consts::U32;
use blake2::{Blake2b, Digest};
use futures::TryStreamExt;
use prost::Message as _;
use secp256k1::{Message, Secp256k1, SecretKey};
use tonic::transport::{Channel, Endpoint};

use crate::helpers::FromExpr;
use crate::models::casper::v1::deploy_service_client::DeployServiceClient;
use crate::models::casper::v1::{block_info_response, deploy_response, rho_data_response};
use crate::models::casper::{BlocksQuery, DataAtNameByBlockQuery, DeployDataProto};
use crate::models::rhoapi::expr::ExprInstance;
use crate::models::rhoapi::{Expr, Par};
use crate::models::{BlockId, DeployData, DeployId, SignedCode, ValidAfter};
use crate::traits::WriteNode;

/// Transport resilience settings for the deploy-service gRPC channel.
///
/// These exist because a default `tonic::Endpoint` sets **none** of them, which
/// made the client wedge permanently: with no HTTP/2 keep-alive, a half-open
/// connection (NAT/conntrack eviction, container blip, peer restart without a
/// clean FIN) is never detected, so the h2 connection never reports an error —
/// and tonic's `Reconnect` only re-establishes on error. With no request
/// deadline, calls on that dead connection hang forever. Because every clone of
/// `Channel` shares one tower `Buffer` worker, a single hung call parks the
/// worker and *every* caller stalls. That is exactly the observed failure: all
/// `*/prepare` endpoints hung indefinitely while HTTP reads stayed fast, and
/// only a process restart cleared it.
#[derive(Clone, Copy, Debug)]
pub struct WriteNodeConfig {
    /// Hard deadline for any single call. Bounds queue-wait + connect + headers
    /// + body (see `deadline`).
    pub request_timeout: Duration,
    /// Bounds the TCP connect only (not the h2 handshake).
    pub connect_timeout: Duration,
    /// How often to send HTTP/2 PING frames while a request is in flight.
    /// `None` disables h2 keep-alive entirely.
    ///
    /// Tune with care. Unlike TCP keepalives — which the peer's *kernel* answers
    /// — an h2 PING must be acked by the node's *application* task, which can
    /// stall for seconds under block-processing load. Too tight a window
    /// therefore kills healthy connections: measured against a live f1r3node, an
    /// interval of 10s with a 5s ack window produced spurious
    /// `KeepAliveTimedOut` errors on a perfectly good connection.
    pub http2_keep_alive_interval: Option<Duration>,
    /// How long to wait for a PING ack before declaring the connection dead.
    pub keep_alive_timeout: Option<Duration>,
    /// Whether to ping while idle. Left off by default: grpc-java/Netty servers
    /// default to `permitKeepAliveWithoutCalls=false` and answer idle pings with
    /// `GOAWAY(ENHANCE_YOUR_CALM)`. This client must work against both the Scala
    /// and Rust nodes, so the idle case is covered by `tcp_keepalive` instead.
    pub keep_alive_while_idle: bool,
    /// Kernel-level keepalive — invisible to gRPC, so no `ENHANCE_YOUR_CALM` risk.
    /// Covers the idle-blackhole case that `keep_alive_while_idle = false` leaves
    /// open. Mirrors reqwest's `tcp_user_timeout`, which is empirically why the
    /// read (HTTP) client never wedged while this one did.
    pub tcp_keepalive: Option<Duration>,
}

impl Default for WriteNodeConfig {
    fn default() -> Self {
        Self {
            // ~1500x the observed healthy latency (6.5ms) and ~2 block times, so
            // it rides out a block-production stall without spurious failures,
            // while still failing fast enough that a blip cannot become a wedge.
            request_timeout: Duration::from_secs(10),
            // Must stay < request_timeout so a reconnect inside a request cannot
            // consume the whole budget.
            connect_timeout: Duration::from_secs(5),
            // Generous on purpose: the node must ack these from its application
            // task, and it can stall for seconds while processing a block. A dead
            // peer is still detected within interval + timeout (~60s) and the
            // channel then self-heals via tonic's `Reconnect`, while a merely-busy
            // node is never torn down. (Measured: 10s/5s spuriously killed a
            // healthy connection.)
            http2_keep_alive_interval: Some(Duration::from_secs(30)),
            keep_alive_timeout: Some(Duration::from_secs(30)),
            keep_alive_while_idle: false,
            tcp_keepalive: Some(Duration::from_secs(20)),
        }
    }
}

#[derive(Clone)]
pub struct WriteNodeClient {
    deploy_client: DeployServiceClient<Channel>,
    request_timeout: Duration,
}

/// Extract a deploy ID from the node's response string.
/// The Scala node returns `"Success! DeployId is: <id>"` while the
/// Rust node returns `"Success!\nDeployId is: <id>"`.
fn extract_deploy_id(response: &str) -> anyhow::Result<DeployId> {
    response
        .strip_prefix("Success! DeployId is: ")
        .or_else(|| response.strip_prefix("Success!\nDeployId is: "))
        .map(|id| DeployId::from(id.to_owned()))
        .context(format!(
            "failed to extract deploy_id from response: {response:?}"
        ))
}

impl WriteNodeClient {
    pub async fn new(deploy_service_url: String) -> anyhow::Result<Self> {
        Self::with_config(deploy_service_url, WriteNodeConfig::default()).await
    }

    pub async fn with_config(
        deploy_service_url: String,
        config: WriteNodeConfig,
    ) -> anyhow::Result<Self> {
        let mut endpoint = Endpoint::new(deploy_service_url)
            .context("invalid deploy service url")?
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .keep_alive_while_idle(config.keep_alive_while_idle)
            .tcp_keepalive(config.tcp_keepalive)
            .tcp_nodelay(true);

        if let Some(interval) = config.http2_keep_alive_interval {
            endpoint = endpoint.http2_keep_alive_interval(interval);
        }
        if let Some(timeout) = config.keep_alive_timeout {
            endpoint = endpoint.keep_alive_timeout(timeout);
        }

        // `connect_timeout` bounds only the TCP connect: the HTTP/2 handshake runs
        // *after* the connector returns, so a peer that accepts TCP and then never
        // speaks h2 would hang startup forever without this outer bound.
        let channel = tokio::time::timeout(config.connect_timeout * 2, endpoint.connect())
            .await
            .map_err(|_| anyhow!("timed out connecting to deploy service"))?
            .context("failed to connect to deploy service")?;

        Ok(Self {
            deploy_client: DeployServiceClient::new(channel),
            request_timeout: config.request_timeout,
        })
    }

    /// Apply the hard per-call deadline.
    ///
    /// An outer `tokio::time::timeout` is required — `Endpoint::timeout` alone is
    /// NOT sufficient, for two independent reasons:
    ///
    /// 1. tonic's `GrpcTimeout` layer sits *inside* the tower `Buffer`, so its
    ///    timer only starts once the buffer worker dispatches the request. If the
    ///    worker is head-of-line blocked by a stuck call, the request waits in the
    ///    queue with no timer at all — which is precisely how one hung call wedged
    ///    every caller.
    /// 2. `GrpcTimeout` resolves as soon as response *headers* arrive and then
    ///    drops its sleep, leaving a streaming body unbounded — and
    ///    `get_head_block_index` reads a server-streaming body.
    ///
    /// This wrapper bounds queue-wait + connect + handshake + headers + body.
    /// Dropping the future on expiry also frees the HTTP/2 stream slot, so zombie
    /// streams cannot exhaust `max_concurrent_streams`.
    async fn deadline<T>(
        timeout: Duration,
        fut: impl Future<Output = anyhow::Result<T>>,
    ) -> anyhow::Result<T> {
        tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| anyhow!("deploy service call timed out after {timeout:?}"))?
    }

    pub async fn deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        let timeout = self.request_timeout;
        Self::deadline(timeout, self.deploy_inner(key, deploy_data)).await
    }

    async fn deploy_inner(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        let valid_after_block_number = match deploy_data.valid_after_block_number {
            // `_inner`: the public caller already holds the single deadline for
            // this whole call, so we must not nest a second one here.
            ValidAfter::Head => self.get_head_block_index_inner().await?,
            ValidAfter::Index(i) => i,
        };

        let mut msg = DeployDataProto {
            term: deploy_data.term,
            timestamp: deploy_data.timestamp.timestamp_millis(),
            phlo_price: 1,
            phlo_limit: deploy_data.phlo_limit as _,
            valid_after_block_number: valid_after_block_number as _,
            shard_id: "root".into(),
            ..Default::default()
        };

        let secp = Secp256k1::new();

        let hash = Blake2b::<U32>::new()
            .chain_update(msg.encode_to_vec())
            .finalize();

        let signature = secp.sign_ecdsa(Message::from_digest(hash.into()), key);

        msg.sig = signature.serialize_der().to_vec().into();
        msg.sig_algorithm = "secp256k1".into();

        let public_key = key.public_key(&secp);
        msg.deployer = public_key.serialize_uncompressed().to_vec().into();

        let resp = self
            .deploy_client
            .do_deploy(msg)
            .await?
            .into_inner()
            .message
            .context("missing do_deploy responce")?;

        match resp {
            deploy_response::Message::Result(msg) => extract_deploy_id(&msg),
            deploy_response::Message::Error(err) => Err(anyhow!("do_deploy error: {err:?}")),
        }
    }

    pub async fn deploy_signed_contract(
        &mut self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        let timeout = self.request_timeout;
        Self::deadline(timeout, self.deploy_signed_contract_inner(contract)).await
    }

    async fn deploy_signed_contract_inner(
        &mut self,
        contract: SignedCode,
    ) -> anyhow::Result<DeployId> {
        let mut msg = DeployDataProto::decode(contract.contract.as_slice())?;

        msg.sig = contract.sig.into();
        msg.sig_algorithm = contract.sig_algorithm;
        msg.deployer = contract.deployer.into();

        let resp = self
            .deploy_client
            .do_deploy(msg)
            .await?
            .into_inner()
            .message
            .context("missing do_deploy responce")?;

        match resp {
            deploy_response::Message::Result(msg) => extract_deploy_id(&msg),
            deploy_response::Message::Error(err) => Err(anyhow!("do_deploy error: {err:?}")),
        }
    }

    pub async fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.deploy(key, deploy_data).await
    }

    pub async fn get_head_block_index(&mut self) -> anyhow::Result<u64> {
        let timeout = self.request_timeout;
        Self::deadline(timeout, self.get_head_block_index_inner()).await
    }

    async fn get_head_block_index_inner(&mut self) -> anyhow::Result<u64> {
        let mut stream = self
            .deploy_client
            .show_main_chain(BlocksQuery { depth: 1 })
            .await?
            .into_inner();

        stream
            .try_next()
            .await?
            .and_then(|block| block.message)
            .map_or(Ok(0), |m| match m {
                block_info_response::Message::Error(err) => {
                    Err(anyhow!("show_main_chain error: {err:?}"))
                }
                block_info_response::Message::BlockInfo(light_block_info) => {
                    Ok(light_block_info.block_number as _)
                }
            })
    }

    pub async fn get_channel_value<T>(
        &mut self,
        hash: BlockId,
        channel: String,
    ) -> anyhow::Result<T>
    where
        T: FromExpr,
    {
        let timeout = self.request_timeout;
        Self::deadline(timeout, self.get_channel_value_inner(hash, channel)).await
    }

    async fn get_channel_value_inner<T>(
        &mut self,
        hash: BlockId,
        channel: String,
    ) -> anyhow::Result<T>
    where
        T: FromExpr,
    {
        let mut par = Par::default();
        par.exprs.push(Expr {
            expr_instance: Some(ExprInstance::GString(channel)),
        });

        let resp = self
            .deploy_client
            .get_data_at_name(DataAtNameByBlockQuery {
                par: Some(par),
                block_hash: hash.into(),
                use_pre_state_hash: false,
            })
            .await
            .context("get_data_at_name grpc error")?
            .into_inner()
            .message
            .context("missing get_data_at_name responce")?;

        let payload = match resp {
            rho_data_response::Message::Payload(payload) => payload,
            rho_data_response::Message::Error(err) => {
                return Err(anyhow!("get_data_at_name error: {err:?}"));
            }
        };

        let par = payload
            .par
            .into_iter()
            .next_back()
            .context("missing par in get_data_at_name")?;
        let expr = par
            .exprs
            .into_iter()
            .next()
            .context("missing exprs in get_data_at_name")?;
        let expr = expr
            .expr_instance
            .context("missing expr_instance in get_data_at_name")?;

        T::from(expr)
    }
}

impl WriteNode for WriteNodeClient {
    async fn deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.deploy(key, deploy_data).await
    }

    async fn deploy_signed_contract(&mut self, contract: SignedCode) -> anyhow::Result<DeployId> {
        self.deploy_signed_contract(contract).await
    }

    async fn full_deploy(
        &mut self,
        key: &SecretKey,
        deploy_data: DeployData,
    ) -> anyhow::Result<DeployId> {
        self.full_deploy(key, deploy_data).await
    }

    async fn get_head_block_index(&mut self) -> anyhow::Result<u64> {
        self.get_head_block_index().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------
    // extract_deploy_id tests (pure function)
    // -------------------------------------------------------

    #[test]
    fn test_extract_deploy_id_scala_format() {
        let result = extract_deploy_id("Success! DeployId is: abc123def456").expect("should parse");
        assert_eq!(result, DeployId::from("abc123def456".to_owned()));
    }

    #[test]
    fn test_extract_deploy_id_rust_node_format() {
        let result =
            extract_deploy_id("Success!\nDeployId is: abc123def456").expect("should parse");
        assert_eq!(result, DeployId::from("abc123def456".to_owned()));
    }

    #[test]
    fn test_extract_deploy_id_error_response() {
        let result = extract_deploy_id("Error: something went wrong");
        assert!(result.is_err(), "expected error for error response");
    }

    #[test]
    fn test_extract_deploy_id_empty() {
        let result = extract_deploy_id("");
        assert!(result.is_err(), "expected error for empty string");
    }

    #[test]
    fn test_extract_deploy_id_partial_prefix_no_trailing_space() {
        // The prefix includes a trailing space: "Success! DeployId is: "
        // Without that space, the prefix doesn't match
        let result = extract_deploy_id("Success! DeployId is:");
        assert!(
            result.is_err(),
            "expected error when trailing space is missing"
        );
    }

    #[test]
    fn test_extract_deploy_id_with_whitespace() {
        let result = extract_deploy_id("Success! DeployId is:  spaced_id  ")
            .expect("should parse with surrounding spaces");
        assert_eq!(result, DeployId::from(" spaced_id  ".to_owned()));
    }

    // -------------------------------------------------------
    // DeployData construction tests
    // -------------------------------------------------------

    #[test]
    fn test_deploy_data_builder_defaults() {
        let now = chrono::Utc::now();
        let data = DeployData::builder("new Nil".into()).build();

        assert_eq!(data.term, "new Nil");
        assert_eq!(data.phlo_limit, 5_000_000);
        assert!(matches!(data.valid_after_block_number, ValidAfter::Head));
        // Timestamp should be very close to now
        let diff = (data.timestamp - now).num_milliseconds().unsigned_abs();
        assert!(
            diff < 1000,
            "timestamp should be close to now, diff={diff}ms"
        );
    }

    #[test]
    fn test_deploy_data_builder_custom_values() {
        let ts = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let data = DeployData::builder("code".into())
            .phlo_limit(1_000_000)
            .timestamp(ts)
            .valid_after_block_number(ValidAfter::Index(42))
            .build();

        assert_eq!(data.term, "code");
        assert_eq!(data.phlo_limit, 1_000_000);
        assert_eq!(data.timestamp, ts);
        assert!(matches!(
            data.valid_after_block_number,
            ValidAfter::Index(42)
        ));
    }

    // -------------------------------------------------------
    // SignedCode construction test
    // -------------------------------------------------------

    #[test]
    fn test_signed_code_fields() {
        let code = SignedCode {
            contract: vec![1, 2, 3],
            sig: vec![4, 5, 6],
            sig_algorithm: "secp256k1".into(),
            deployer: vec![7, 8, 9],
        };
        assert_eq!(code.contract, vec![1, 2, 3]);
        assert_eq!(code.sig, vec![4, 5, 6]);
        assert_eq!(code.sig_algorithm, "secp256k1");
        assert_eq!(code.deployer, vec![7, 8, 9]);
    }

    // -------------------------------------------------------
    // transport resilience (the "every */prepare hangs forever" wedge)
    // -------------------------------------------------------

    /// Every public gRPC call routes through `deadline`, so this is the guard
    /// that a stuck call fails fast instead of hanging forever — which is what
    /// parked the shared tower `Buffer` worker and wedged *all* callers.
    #[tokio::test]
    async fn test_deadline_bounds_a_hanging_call() {
        let result: anyhow::Result<()> = WriteNodeClient::deadline(
            Duration::from_millis(50),
            std::future::pending::<anyhow::Result<()>>(),
        )
        .await;

        let err = result.expect_err("a never-completing call must hit the deadline");
        assert!(
            err.to_string().contains("timed out"),
            "expected a timeout error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_deadline_passes_through_a_fast_call() {
        let result = WriteNodeClient::deadline(Duration::from_secs(5), async { Ok(42_u64) }).await;
        assert_eq!(result.expect("a fast call must not time out"), 42);
    }

    /// A peer that accepts TCP but never speaks HTTP/2 models the production
    /// wedge: the socket looks alive, so tonic's `Reconnect` never fires (it only
    /// re-establishes on a connection *error*), and with no deadline the call
    /// hangs forever — which is exactly how every `*/prepare` endpoint came to
    /// hang until the process was restarted.
    ///
    /// Note: `Endpoint::connect()` itself returns `Ok` here — it does not await
    /// the peer's HTTP/2 SETTINGS — so the connect bound is *not* what saves us.
    /// The per-request deadline is. This test hangs forever on the old code.
    // `held` is intentionally write-only: it exists purely to keep the accepted
    // sockets from being dropped (which would close them and defeat the test).
    #[allow(clippy::collection_is_never_read)]
    #[tokio::test]
    async fn test_call_fails_fast_against_peer_that_never_speaks_http2() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let addr = listener.local_addr().expect("local_addr");

        // Accept connections and then say nothing, ever.
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream); // hold it open; never write a byte
            }
        });

        let config = WriteNodeConfig {
            connect_timeout: Duration::from_millis(200),
            request_timeout: Duration::from_millis(300),
            ..WriteNodeConfig::default()
        };

        let mut client = WriteNodeClient::with_config(format!("http://{addr}"), config)
            .await
            .expect("connect() completes without awaiting the peer's HTTP/2 SETTINGS");

        let started = std::time::Instant::now();
        let result = client.get_head_block_index().await;

        // Either bound firing is a pass — the property under test is "bounded,
        // not hung". Here tonic's inner `GrpcTimeout` happens to win the race
        // (both are 300ms and nothing is blocking the buffer worker); the outer
        // `deadline` is the backstop for the two cases `GrpcTimeout` provably
        // cannot cover: queue-wait while the worker is head-of-line blocked, and
        // a streaming body that stalls after headers.
        let err = result.expect_err("a call to a peer that never speaks HTTP/2 must not hang");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("timed out") || msg.contains("timeout"),
            "expected a timeout, got: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must fail fast, took {:?}",
            started.elapsed()
        );
    }

    /// The resilience settings must actually be applied, and be internally
    /// consistent: a reconnect attempt inside a request must not be able to eat
    /// the entire request budget.
    #[test]
    fn test_default_config_is_internally_consistent() {
        let config = WriteNodeConfig::default();

        assert!(
            config.connect_timeout < config.request_timeout,
            "connect_timeout must be < request_timeout, so a reconnect attempt \
             inside a request cannot consume the whole budget"
        );

        // Regression guard for a real mistake: a 10s interval with a 5s ack window
        // was *measured* to spuriously kill a healthy connection against a live
        // node (`KeepAliveTimedOut`). Unlike a TCP keepalive, which the peer's
        // kernel answers, an h2 PING must be acked by the node's application task
        // — which stalls for seconds while processing a block. The ack window must
        // therefore tolerate a busy-but-alive node.
        if let Some(ack_window) = config.keep_alive_timeout {
            assert!(
                ack_window >= Duration::from_secs(20),
                "h2 PING ack window must tolerate a busy node; {ack_window:?} is too tight"
            );
        }

        assert!(
            config.tcp_keepalive.is_some(),
            "TCP keepalive is the false-positive-free self-heal (the kernel acks it) \
             and covers the idle case that keep_alive_while_idle = false leaves open"
        );
        assert!(!config.keep_alive_while_idle);
    }
}
