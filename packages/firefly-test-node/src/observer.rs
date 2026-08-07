//! The fake observer: a `poem` server on loopback exposing the read + event
//! surface the client uses.
//!
//! * `POST /api/explore-deploy` — scripted `{"expr":[…]}` responses, so a test
//!   can drive the empty→populated visibility-lag path the retrying client must
//!   tolerate. Matches the current client, which posts a JSON `{"term":…}` body.
//! * `GET /status` — a small health body.
//! * `GET /ws/events` — sends `started` on connect, then forwards every injected
//!   event; a `Close` command drops the socket to exercise client reconnect.

use anyhow::Context as _;
use futures::{SinkExt, StreamExt};
use poem::listener::TcpAcceptor;
use poem::web::websocket::{Message, WebSocket};
use poem::web::{Data, Json};
use poem::{EndpointExt, IntoResponse, Route, Server, get, handler, post};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, oneshot};

use crate::Spawned;
use crate::control::{EventControl, ExploreControl, WsCommand, started_json};

#[derive(Clone)]
struct ObserverState {
    explore: ExploreControl,
    events: EventControl,
}

/// Bind an ephemeral loopback port, start the fake observer on it, and return
/// its address plus shutdown/join handles.
pub(crate) async fn spawn(
    explore: ExploreControl,
    events: EventControl,
) -> anyhow::Result<Spawned> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("failed to bind fake observer listener")?;
    let addr = listener
        .local_addr()
        .context("failed to read fake observer address")?;
    let acceptor =
        TcpAcceptor::from_tokio(listener).context("failed to build fake observer acceptor")?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let state = ObserverState { explore, events };
    let app = Route::new()
        .at("/api/explore-deploy", post(explore_deploy))
        .at("/status", get(status))
        .at("/ws/events", get(ws_events))
        .data(state);

    let handle = tokio::spawn(async move {
        let shutdown = async move {
            let _ = shutdown_rx.await;
        };
        if let Err(err) = Server::new_with_acceptor(acceptor)
            .run_with_graceful_shutdown(app, shutdown, None)
            .await
        {
            tracing::debug!("fake observer server ended: {err}");
        }
    });

    Ok(Spawned {
        addr,
        shutdown: shutdown_tx,
        handle,
    })
}

/// Pull the Rholang term out of the client's JSON body, falling back to the raw
/// body for any other shape.
fn extract_term(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("term")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.to_owned())
}

// poem supplies handler extractors by value, so the by-value parameters below
// are required by the framework rather than a needless move.
#[handler]
#[allow(clippy::needless_pass_by_value)]
fn explore_deploy(state: Data<&ObserverState>, body: String) -> Json<Value> {
    let term = extract_term(&body);
    Json(state.0.explore.next_response(term))
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn status(state: Data<&ObserverState>) -> Json<Value> {
    Json(state.0.explore.status_body())
}

#[handler]
#[allow(clippy::needless_pass_by_value)]
fn ws_events(ws: WebSocket, state: Data<&ObserverState>) -> impl IntoResponse {
    let events = state.0.events.clone();
    // Subscribe BEFORE marking the connection live, so any event injected once a
    // test observes the connection count is guaranteed to be buffered for this
    // receiver rather than racing the forward loop.
    let receiver = events.subscribe();
    events.on_connect();
    ws.on_upgrade(move |socket| ws_loop(socket, receiver, events))
}

async fn ws_loop(
    mut socket: poem::web::websocket::WebSocketStream,
    mut receiver: broadcast::Receiver<WsCommand>,
    events: EventControl,
) {
    if socket.send(Message::text(started_json())).await.is_err() {
        events.on_disconnect();
        return;
    }

    loop {
        tokio::select! {
            command = receiver.recv() => match command {
                Ok(WsCommand::Send(text)) => {
                    if socket.send(Message::text(text)).await.is_err() {
                        break;
                    }
                }
                Ok(WsCommand::Close) => {
                    let _ = socket.send(Message::close()).await;
                    break;
                }
                // A lag just means we dropped buffered commands; loop and re-poll.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.next() => match incoming {
                // Drain and ignore client-originated frames (pings, etc.).
                Some(Ok(_)) => {}
                // Error or clean close from the client side: end this task.
                _ => break,
            },
        }
    }

    events.on_disconnect();
}
