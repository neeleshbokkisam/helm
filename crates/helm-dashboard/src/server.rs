use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::shared::DashboardShared;
use crate::wire::{hello_json, history_json};
use crate::BROADCAST_CAPACITY;

#[derive(Clone)]
struct AppState {
    broadcast: broadcast::Sender<String>,
    shared: DashboardShared,
}

pub struct StartedServer {
    pub tx: broadcast::Sender<String>,
    pub addr: SocketAddr,
}

pub async fn try_start_server(
    port: u16,
    static_dir: PathBuf,
    shutdown: CancellationToken,
    shared: DashboardShared,
) -> Result<StartedServer, String> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .map_err(|e| format!("bind failed: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr failed: {e}"))?;

    let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
    let state = AppState {
        broadcast: tx.clone(),
        shared,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws_handler))
        .fallback_service(crate::static_files::service(static_dir))
        .with_state(state);

    tokio::spawn(async move {
        let shutdown_signal = async move {
            shutdown.cancelled().await;
        };
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            tracing::error!("dashboard server exited: {e}");
        }
    });

    info!("dashboard listening on http://{addr} (GET /health, WS /ws)");
    Ok(StartedServer { tx, addr })
}

#[derive(Serialize)]
struct HealthBody {
    ok: bool,
    ws_path: &'static str,
    mode: &'static str,
    backend: &'static str,
    loops: bool,
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let session = state.shared.session();
    Json(HealthBody {
        ok: true,
        ws_path: "/ws",
        mode: session.mode,
        backend: session.backend,
        loops: session.loops,
    })
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    info!("ws: client connected");
    let mut rx = state.broadcast.subscribe();
    let (mut sender, mut receiver) = socket.split();

    if let Some(json) = hello_json(state.shared.session()) {
        if sender.send(Message::Text(json.into())).await.is_err() {
            warn!("ws: failed to send hello");
            return;
        }
        debug!("ws: sent hello");
    }
    let history = state.shared.history();
    if !history.is_empty() {
        if let Some(json) = history_json(&history) {
            if sender.send(Message::Text(json.into())).await.is_err() {
                warn!("ws: failed to send history");
                return;
            }
            debug!("ws: sent history ({} snapshots)", history.len());
        }
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("ws: client lagged, skipped {n} messages");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        warn!("ws: read error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    info!("ws: client disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::TickSnapshot;
    use crate::wire::SessionInfo;
    use futures_util::StreamExt;
    use helm_core::{CartPoleState, SafetyStatus, Timestamp};
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

    #[tokio::test]
    async fn websocket_receives_hello_history_and_tick() {
        let shutdown = CancellationToken::new();
        let shared = DashboardShared::new(SessionInfo::live_sim(0.01));
        let snap = TickSnapshot::new(
            Timestamp {
                tick: 1,
                dt_secs: 0.01,
            },
            CartPoleState::INITIAL,
            0.5,
            SafetyStatus::INITIAL,
        );
        shared.push_history(snap);

        let StartedServer { tx, addr } =
            try_start_server(0, PathBuf::from("/nonexistent"), shutdown.clone(), shared)
                .await
                .unwrap();

        let url = format!("ws://{addr}/ws");
        let connect = tokio::spawn(async move {
            let (mut ws, _) = connect_async(&url).await.unwrap();
            tokio::task::yield_now().await;
            ws
        });

        let mut ws = connect.await.unwrap();

        let hello = ws.next().await.unwrap().unwrap();
        match hello {
            WsMessage::Text(text) => assert!(text.contains("\"type\":\"hello\"")),
            other => panic!("unexpected message: {other:?}"),
        }

        let history = ws.next().await.unwrap().unwrap();
        match history {
            WsMessage::Text(text) => assert!(text.contains("\"type\":\"history\"")),
            other => panic!("unexpected message: {other:?}"),
        }

        tx.send(
            crate::wire::tick_json(&TickSnapshot::new(
                Timestamp {
                    tick: 2,
                    dt_secs: 0.01,
                },
                CartPoleState::INITIAL,
                0.25,
                SafetyStatus::INITIAL,
            ))
            .unwrap(),
        )
        .unwrap();

        let tick = ws.next().await.unwrap().unwrap();
        match tick {
            WsMessage::Text(text) => {
                assert!(text.contains("\"type\":\"tick\""));
                assert!(text.contains("\"tick\":2"));
            }
            other => panic!("unexpected message: {other:?}"),
        }

        shutdown.cancel();
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let shutdown = CancellationToken::new();
        let shared = DashboardShared::new(SessionInfo::live_sim(0.01));
        let StartedServer { addr, .. } =
            try_start_server(0, PathBuf::from("/nonexistent"), shutdown.clone(), shared)
                .await
                .unwrap();

        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut buf = vec![0u8; 2048];
        let n = stream.read(&mut buf).await.unwrap();
        let body = String::from_utf8_lossy(&buf[..n]);
        assert!(body.contains("200"));
        assert!(body.contains("\"ok\":true"));
        assert!(body.contains("\"ws_path\":\"/ws\""));

        shutdown.cancel();
    }
}
