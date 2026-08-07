//! A loopback, in-process fake `ATProto` Personal Data Server (PDS).
//!
//! It answers the exact XRPC calls the `publish_to_firesky` / `unpublish` clients
//! (atrium over reqwest) make, with canned responses shaped to deserialize
//! through atrium's generated output types. It lets the publish/unpublish flows
//! be driven end to end without touching a real PDS or the network.
//!
//! ## Covered XRPC (`POST|GET {origin}/xrpc/{nsid}`)
//!
//! | NSID | Method | Canned behaviour |
//! | ---- | ------ | ---------------- |
//! | `com.atproto.server.createAccount` | POST | Refused with a `400` XRPC error, so the client falls back to signing in — the ordinary "publish again" path. |
//! | `com.atproto.server.createSession` | POST | Returns a session for the configured handle + DID. |
//! | `com.atproto.server.listAppPasswords` | GET | Returns the configured set of existing app-password names. |
//! | `com.atproto.server.createAppPassword` | POST | Mints a fixed app password. |
//! | `com.atproto.repo.uploadBlob` | POST | Returns a typed blob ref with a valid CID. |
//! | `com.atproto.repo.putRecord` | POST | `200` with a `{uri, cid}`, or a `400` XRPC error when rejection is armed. |
//! | `com.atproto.repo.deleteRecord` | POST | `200` (supports the unpublish path too). |
//! | `GET /logo.png` | GET | Serves an opaque blob asset the profile-avatar upload fetches. |
//!
//! ## Fidelity boundary
//!
//! Not a PDS. It does **not**: verify credentials or app-password auth (any
//! `Authorization` is accepted); resolve DIDs or handles; validate Lexicon
//! records or blob bytes; expire sessions or refresh tokens; or persist anything
//! across calls beyond the counters a test arms. Sessions are immortal and the
//! served `/logo.png` is opaque bytes, not a decodable image.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Context as _;
use poem::http::StatusCode;
use poem::listener::TcpAcceptor;
use poem::web::{Data, Json};
use poem::{EndpointExt, IntoResponse, Response, Route, Server, get, handler, post};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// A DID the fake returns for the published account. Valid per atproto's DID
/// syntax (`did:plc:` + base32) so atrium's `Did` newtype accepts it.
const FAKE_DID: &str = "did:plc:z72i7hdynmk6r22z27h6tvur";

/// A valid `CIDv1` (raw, sha2-256) used for every returned blob ref and record CID.
/// Taken from atrium's own test vectors, so atrium's `Cid` newtype parses it.
const FAKE_CID: &str = "bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy";

/// A Lexicon datetime the fake stamps on app passwords. Valid ISO-8601/RFC-3339.
const FAKE_DATETIME: &str = "2024-01-01T00:00:00.000Z";

/// Opaque bytes served at `/logo.png`. The upload path only needs fetchable
/// bytes; the fake's `uploadBlob` returns a canned ref regardless of content.
const LOGO_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n-embers-fake-pds-logo-";

fn guard<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct FakePdsInner {
    /// Handle returned by `createSession` (must equal the requested handle for a
    /// publish to succeed, since the client verifies it).
    handle: String,
    /// App-password names `listAppPasswords` reports as already taken.
    existing_app_passwords: Mutex<Vec<String>>,
    /// When set, `putRecord` responds with a `400` XRPC error.
    reject_put_record: AtomicBool,
    /// Ordered log of the NSIDs the fake has been asked to serve.
    calls: Mutex<Vec<String>>,
}

/// Cloneable handler state; every clone shares the same inner via `Arc`.
#[derive(Clone)]
struct FakePdsState {
    inner: Arc<FakePdsInner>,
}

impl FakePdsState {
    fn record(&self, nsid: &str) {
        guard(&self.inner.calls).push(nsid.to_owned());
    }
}

/// A running fake PDS. Dropping it (or [`FakePds::shutdown`]) stops the server
/// and releases the loopback port.
pub(crate) struct FakePds {
    origin: String,
    state: FakePdsState,
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl FakePds {
    /// Start a fake PDS on an ephemeral loopback port, returning sessions for
    /// `handle`.
    pub(crate) async fn start(handle: &str) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .context("failed to bind fake PDS listener")?;
        let addr: SocketAddr = listener
            .local_addr()
            .context("failed to read fake PDS address")?;
        let origin = format!("http://{addr}");
        let acceptor =
            TcpAcceptor::from_tokio(listener).context("failed to build fake PDS acceptor")?;

        let state = FakePdsState {
            inner: Arc::new(FakePdsInner {
                handle: handle.to_owned(),
                existing_app_passwords: Mutex::new(Vec::new()),
                reject_put_record: AtomicBool::new(false),
                calls: Mutex::new(Vec::new()),
            }),
        };

        let app = Route::new()
            .at(
                "/xrpc/com.atproto.server.createAccount",
                post(create_account),
            )
            .at(
                "/xrpc/com.atproto.server.createSession",
                post(create_session),
            )
            .at(
                "/xrpc/com.atproto.server.listAppPasswords",
                get(list_app_passwords),
            )
            .at(
                "/xrpc/com.atproto.server.createAppPassword",
                post(create_app_password),
            )
            .at("/xrpc/com.atproto.repo.uploadBlob", post(upload_blob))
            .at("/xrpc/com.atproto.repo.putRecord", post(put_record))
            .at("/xrpc/com.atproto.repo.deleteRecord", post(delete_record))
            .at("/logo.png", get(logo))
            .data(state.clone());

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let shutdown = async move {
                let _ = shutdown_rx.await;
            };
            if let Err(err) = Server::new_with_acceptor(acceptor)
                .run_with_graceful_shutdown(app, shutdown, None)
                .await
            {
                tracing::debug!("fake PDS server ended: {err}");
            }
        });

        Ok(Self {
            origin,
            state,
            shutdown: Some(shutdown_tx),
            handle,
        })
    }

    /// `http://<addr>` — pass as the request's `pds_url`.
    pub(crate) fn origin(&self) -> &str {
        &self.origin
    }

    /// `http://<addr>/logo.png` — a fetchable blob asset for a team logo.
    pub(crate) fn logo_url(&self) -> String {
        format!("{}/logo.png", self.origin)
    }

    /// Arm `listAppPasswords` to report these names as already taken.
    pub(crate) fn set_existing_app_passwords(&self, names: Vec<String>) {
        *guard(&self.state.inner.existing_app_passwords) = names;
    }

    /// Arm `putRecord` to reject with a `400` XRPC error.
    pub(crate) fn set_reject_put_record(&self, reject: bool) {
        self.state
            .inner
            .reject_put_record
            .store(reject, Ordering::SeqCst);
    }

    /// The NSIDs the fake has served, in order.
    pub(crate) fn calls(&self) -> Vec<String> {
        guard(&self.state.inner.calls).clone()
    }

    /// Gracefully stop the server and await its task.
    pub(crate) async fn shutdown(mut self) {
        if let Some(trigger) = self.shutdown.take() {
            let _ = trigger.send(());
        }
        let _ = (&mut self.handle).await;
    }
}

impl Drop for FakePds {
    fn drop(&mut self) {
        if let Some(trigger) = self.shutdown.take() {
            let _ = trigger.send(());
        }
        self.handle.abort();
    }
}

/// A standard XRPC error body (`{error, message}`) with a status.
fn xrpc_error(status: StatusCode, error: &str, message: &str) -> Response {
    (status, Json(json!({ "error": error, "message": message }))).into_response()
}

// poem supplies handler extractors by value; the by-value `Data` params are
// framework-required rather than a needless move.

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn create_account(state: Data<&FakePdsState>) -> Response {
    state.0.record("com.atproto.server.createAccount");
    // Refuse creation so the client signs in instead — the reversible
    // publish-again path that `prepare_publish_to_firesky_contract` relies on.
    xrpc_error(
        StatusCode::BAD_REQUEST,
        "HandleNotAvailable",
        "the fake PDS does not create accounts; sign in to the existing one",
    )
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn create_session(state: Data<&FakePdsState>) -> Json<Value> {
    state.0.record("com.atproto.server.createSession");
    Json(json!({
        "accessJwt": "fake-access-jwt",
        "refreshJwt": "fake-refresh-jwt",
        "handle": state.0.inner.handle,
        "did": FAKE_DID,
        "active": true,
    }))
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn list_app_passwords(state: Data<&FakePdsState>) -> Json<Value> {
    state.0.record("com.atproto.server.listAppPasswords");
    let passwords: Vec<Value> = guard(&state.0.inner.existing_app_passwords)
        .iter()
        .map(|name| json!({ "name": name, "createdAt": FAKE_DATETIME }))
        .collect();
    Json(json!({ "passwords": passwords }))
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn create_app_password(state: Data<&FakePdsState>) -> Json<Value> {
    state.0.record("com.atproto.server.createAppPassword");
    Json(json!({
        "name": "Embers",
        "password": "fake-app-password-value",
        "createdAt": FAKE_DATETIME,
        "privileged": false,
    }))
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn upload_blob(state: Data<&FakePdsState>) -> Json<Value> {
    state.0.record("com.atproto.repo.uploadBlob");
    Json(json!({
        "blob": {
            "$type": "blob",
            "ref": { "$link": FAKE_CID },
            "mimeType": "image/png",
            "size": LOGO_BYTES.len(),
        }
    }))
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn put_record(state: Data<&FakePdsState>) -> Response {
    state.0.record("com.atproto.repo.putRecord");
    if state.0.inner.reject_put_record.load(Ordering::SeqCst) {
        return xrpc_error(
            StatusCode::BAD_REQUEST,
            "InvalidRecord",
            "the fake PDS rejected this record",
        );
    }
    Json(json!({
        "uri": format!("at://{FAKE_DID}/com.f1r3sky.agentsteam.config/self"),
        "cid": FAKE_CID,
        "validationStatus": "valid",
    }))
    .into_response()
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn delete_record(state: Data<&FakePdsState>) -> Json<Value> {
    state.0.record("com.atproto.repo.deleteRecord");
    Json(json!({}))
}

#[handler]
fn logo() -> Response {
    Response::builder()
        .content_type("image/png")
        .body(LOGO_BYTES.to_vec())
}
