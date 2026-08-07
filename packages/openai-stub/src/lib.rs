//! # openai-stub
//!
//! A loopback-only, OpenAI-compatible HTTP stub. It stands in for the real
//! OpenAI API on a machine that cannot (or must not) reach `api.openai.com`, so
//! that a f1r3node whose Rholang interpreter is configured to call OpenAI can be
//! booted and driven entirely offline.
//!
//! The stub can be used two ways:
//!
//! * as a **library** — [`OpenAiStub::start`] binds an ephemeral loopback port
//!   (`127.0.0.1:0`) and returns the bound address, so a test can point a client
//!   at it and shut it down deterministically; and
//! * as a **binary** (`openai-stub`) — binds the address in `OPENAI_STUB_ADDR`
//!   (default `127.0.0.1:8571`) and runs until `Ctrl-C`.
//!
//! ## Endpoints
//!
//! | Method & path | Purpose |
//! | ------------- | ------- |
//! | `GET /v1/models` | Lists `gpt-4o-mini`, `dall-e-3`, `tts-1` (keeps a node's startup `list_models` validation green). |
//! | `POST /v1/chat/completions` | Echoes the last user message as `"[stub:gpt-4o-mini] " + <message>`; `id = chatcmpl-stub-<sha256(prompt)[..16]>`; zeroed usage. |
//! | `POST /v1/images/generations` | Returns a loopback URL `http://127.0.0.1:{port}/stub-assets/img-<hash>.png`. |
//! | `POST /v1/audio/speech` | Returns a minimal valid WAV (header + SHA-256-derived payload). |
//! | `GET /stub-assets/{file}` | Serves the constant valid 1×1 PNG the image endpoint points at. |
//! | `GET /stub/requests` | Returns the ring buffer of received requests, for assertions / stub-uptake checks. |
//!
//! ## Fidelity boundary
//!
//! This is a behavioural double, not OpenAI. It deliberately does **not**:
//!
//! * run any model — chat/image/audio responses are canned/echoed, not generated;
//! * enforce authentication — the `Authorization` header is accepted and ignored;
//! * emit rate-limit (`429`), quota, or other error statuses;
//! * support streaming (`stream: true`) or Server-Sent Events;
//! * count tokens — `usage` is always zero.
//!
//! Its purpose is to exercise a client's request encoding, endpoint routing, and
//! response deserialization deterministically and offline. The
//! [contract tests](https://docs.rs) drive all four endpoints with the real
//! `openai-api-rs` client (the same version f1r3node uses) so that the canned
//! JSON is guaranteed to deserialize through that client — fidelity by
//! construction.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::Context as _;
use poem::listener::TcpAcceptor;
use poem::web::{Data, Json};
use poem::{EndpointExt, Response, Route, Server, get, handler, post};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

mod assets;

pub use assets::{one_by_one_png, wav_from_input};

/// Capacity of the received-request ring buffer. Older entries are evicted once
/// the buffer is full, bounding memory for a long-lived stub.
const REQUEST_LOG_CAPACITY: usize = 256;

/// A request the stub received, retained for test assertions.
#[derive(Clone, Debug, Serialize)]
pub struct RecordedRequest {
    /// HTTP method (`"GET"` / `"POST"`).
    pub method: String,
    /// Request path (e.g. `"/v1/chat/completions"`).
    pub path: String,
    /// Raw request body as received (empty for bodiless requests).
    pub body: String,
}

/// A cloneable, bounded log of received requests shared by every handler.
#[derive(Clone, Default)]
struct RequestLog {
    inner: Arc<Mutex<VecDeque<RecordedRequest>>>,
}

impl RequestLog {
    fn record(&self, method: &str, path: &str, body: String) {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.len() == REQUEST_LOG_CAPACITY {
            guard.pop_front();
        }
        guard.push_back(RecordedRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            body,
        });
    }

    fn snapshot(&self) -> Vec<RecordedRequest> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

/// Handler-shared state: the stub's own origin (for building loopback asset URLs)
/// and the request log.
#[derive(Clone)]
struct StubState {
    origin: String,
    requests: RequestLog,
}

/// A running loopback OpenAI stub. Holds the server task alive; dropping it (or
/// calling [`OpenAiStub::shutdown`]) stops the server and releases the port.
pub struct OpenAiStub {
    addr: SocketAddr,
    origin: String,
    requests: RequestLog,
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

impl OpenAiStub {
    /// Start the stub on an ephemeral loopback port (`127.0.0.1:0`).
    pub async fn start() -> anyhow::Result<Self> {
        Self::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await
    }

    /// Start the stub on a specific address, which must be loopback.
    ///
    /// # Errors
    ///
    /// Returns an error if `addr` is not a loopback address, or if the port
    /// cannot be bound.
    pub async fn start_on(addr: SocketAddr) -> anyhow::Result<Self> {
        anyhow::ensure!(
            addr.ip().is_loopback(),
            "openai-stub refuses to bind non-loopback address {addr}; the stub is loopback-only"
        );
        Self::bind(addr).await
    }

    async fn bind(addr: SocketAddr) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind openai-stub listener on {addr}"))?;
        let addr = listener
            .local_addr()
            .context("failed to read openai-stub bound address")?;
        let origin = format!("http://{addr}");
        let acceptor =
            TcpAcceptor::from_tokio(listener).context("failed to build openai-stub acceptor")?;

        let requests = RequestLog::default();
        let state = StubState {
            origin: origin.clone(),
            requests: requests.clone(),
        };
        let app = Route::new()
            .at("/v1/models", get(list_models))
            .at("/v1/chat/completions", post(chat_completions))
            .at("/v1/images/generations", post(images_generations))
            .at("/v1/audio/speech", post(audio_speech))
            .at("/stub-assets/:file", get(stub_asset))
            .at("/stub/requests", get(stub_requests))
            .data(state);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let shutdown = async move {
                let _ = shutdown_rx.await;
            };
            if let Err(err) = Server::new_with_acceptor(acceptor)
                .run_with_graceful_shutdown(app, shutdown, None)
                .await
            {
                tracing::debug!("openai-stub server ended: {err}");
            }
        });

        Ok(Self {
            addr,
            origin,
            requests,
            shutdown: Some(shutdown_tx),
            handle,
        })
    }

    /// The bound socket address (always loopback).
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://<addr>` — the server origin (serves `/stub-assets/*` and `/stub/*`).
    #[must_use]
    pub fn origin(&self) -> String {
        self.origin.clone()
    }

    /// `http://<addr>/v1` — the value to set `OPENAI_API_BASE` to (or pass to a
    /// client's `with_endpoint`) so its `/v1/...` calls reach this stub.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("{}/v1", self.origin)
    }

    /// A snapshot of the requests the stub has received, oldest first.
    #[must_use]
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.snapshot()
    }

    /// Gracefully stop the server and await its task.
    pub async fn shutdown(mut self) {
        if let Some(trigger) = self.shutdown.take() {
            let _ = trigger.send(());
        }
        let _ = (&mut self.handle).await;
    }
}

impl Drop for OpenAiStub {
    fn drop(&mut self) {
        // Resolve the graceful-shutdown future if it is still armed, then abort
        // as a hard backstop so the ephemeral port is released even mid-request.
        if let Some(trigger) = self.shutdown.take() {
            let _ = trigger.send(());
        }
        self.handle.abort();
    }
}

impl StubState {
    fn record(&self, method: &str, path: &str, body: String) {
        self.requests.record(method, path, body);
    }
}

/// First 16 hex characters of `SHA-256(text)`.
fn sha256_hex16(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let hex = hex::encode(digest);
    hex[..16].to_owned()
}

/// The last message with role `user` in a chat-completions request body, or the
/// empty string if there is none.
fn last_user_message(request: &Value) -> String {
    request
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages.iter().rev().find_map(|message| {
                match message.get("role").and_then(Value::as_str) {
                    Some("user") => message
                        .get("content")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    _ => None,
                }
            })
        })
        .unwrap_or_default()
}

/// A single string field from a JSON body, or the empty string.
fn string_field(request: &Value, field: &str) -> String {
    request
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn parse_body(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

fn model_entry(id: &str) -> Value {
    json!({ "id": id, "object": "model", "created": 0, "owned_by": "openai" })
}

fn models_body() -> Value {
    json!({
        "object": "list",
        "data": [
            model_entry("gpt-4o-mini"),
            model_entry("dall-e-3"),
            model_entry("tts-1"),
        ],
    })
}

fn chat_body(request_body: &str) -> Value {
    let request = parse_body(request_body);
    let model = request
        .get("model")
        .and_then(Value::as_str)
        .map_or_else(|| "gpt-4o-mini".to_owned(), ToOwned::to_owned);
    let last_user = last_user_message(&request);
    let content = format!("[stub:gpt-4o-mini] {last_user}");
    let id = format!("chatcmpl-stub-{}", sha256_hex16(&last_user));

    json!({
        "id": id,
        "object": "chat.completion",
        "created": 0,
        "model": model,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop",
            "finish_details": null,
        }],
        "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 },
        "system_fingerprint": null,
    })
}

fn image_body(origin: &str, request_body: &str) -> Value {
    let request = parse_body(request_body);
    let hash = sha256_hex16(&string_field(&request, "prompt"));
    let url = format!("{origin}/stub-assets/img-{hash}.png");
    json!({ "created": 0, "data": [{ "url": url }] })
}

// poem supplies handler extractors by value, so the by-value parameters below are
// required by the framework rather than a needless move.

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn list_models(state: Data<&StubState>) -> Json<Value> {
    state.0.record("GET", "/v1/models", String::new());
    Json(models_body())
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn chat_completions(state: Data<&StubState>, body: String) -> Json<Value> {
    let response = chat_body(&body);
    state.0.record("POST", "/v1/chat/completions", body);
    Json(response)
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn images_generations(state: Data<&StubState>, body: String) -> Json<Value> {
    let response = image_body(&state.0.origin, &body);
    state.0.record("POST", "/v1/images/generations", body);
    Json(response)
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn audio_speech(state: Data<&StubState>, body: String) -> Response {
    let input = string_field(&parse_body(&body), "input");
    let wav = assets::wav_from_input(&input);
    state.0.record("POST", "/v1/audio/speech", body);
    Response::builder().content_type("audio/wav").body(wav)
}

#[handler]
fn stub_asset() -> Response {
    Response::builder()
        .content_type("image/png")
        .body(assets::one_by_one_png().to_vec())
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn stub_requests(state: Data<&StubState>) -> Json<Value> {
    Json(json!(state.0.requests.snapshot()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_body_echoes_last_user_message() {
        let request = json!({
            "model": "gpt-4o-mini",
            "messages": [
                { "role": "system", "content": "be terse" },
                { "role": "user", "content": "first" },
                { "role": "assistant", "content": "ok" },
                { "role": "user", "content": "second" },
            ],
        })
        .to_string();

        let response = chat_body(&request);
        assert_eq!(
            response["choices"][0]["message"]["content"],
            "[stub:gpt-4o-mini] second"
        );
        assert_eq!(response["object"], "chat.completion");
        assert_eq!(response["usage"]["total_tokens"], 0);
        let id = response["id"].as_str().expect("id is a string");
        assert!(id.starts_with("chatcmpl-stub-"));
        assert_eq!(id.len(), "chatcmpl-stub-".len() + 16);
    }

    #[test]
    fn chat_id_is_deterministic_in_the_prompt() {
        let make = |content: &str| {
            chat_body(&json!({ "messages": [{ "role": "user", "content": content }] }).to_string())
                ["id"]
                .as_str()
                .expect("id")
                .to_owned()
        };
        assert_eq!(make("same"), make("same"));
        assert_ne!(make("a"), make("b"));
    }

    #[test]
    fn image_body_points_at_a_loopback_asset() {
        let response = image_body("http://127.0.0.1:65535", &json!({ "prompt": "cat" }).to_string());
        let url = response["data"][0]["url"].as_str().expect("url");
        assert!(url.starts_with("http://127.0.0.1:65535/stub-assets/img-"));
        assert!(
            std::path::Path::new(url)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        );
    }

    #[test]
    fn models_body_lists_the_three_advertised_models() {
        let body = models_body();
        let ids: Vec<&str> = body["data"]
            .as_array()
            .expect("data array")
            .iter()
            .filter_map(|entry| entry["id"].as_str())
            .collect();
        assert!(ids.contains(&"gpt-4o-mini"));
        assert!(ids.contains(&"dall-e-3"));
        assert!(ids.contains(&"tts-1"));
    }
}
