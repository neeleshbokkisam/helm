use std::net::SocketAddr;

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::BROADCAST_CAPACITY;

#[derive(Clone)]
struct AppState {
    broadcast: broadcast::Sender<String>,
}

pub struct StartedServer {
    pub tx: broadcast::Sender<String>,
    pub addr: SocketAddr,
}

pub async fn try_start_server(
    port: u16,
    shutdown: CancellationToken,
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
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
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

    info!("dashboard listening on http://{addr}");
    Ok(StartedServer { tx, addr })
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let mut rx = state.broadcast.subscribe();
    let (mut sender, mut receiver) = socket.split();

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if sender.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

    #[tokio::test]
    async fn websocket_receives_broadcast_json() {
        let shutdown = CancellationToken::new();
        let StartedServer { tx, addr } = try_start_server(0, shutdown.clone())
            .await
            .unwrap();

        let url = format!("ws://{addr}/ws");
        let connect = tokio::spawn(async move {
            let (mut ws, _) = connect_async(&url).await.unwrap();
            tokio::task::yield_now().await;
            ws
        });

        let mut ws = connect.await.unwrap();
        tx.send(r#"{"tick":1}"#.to_string()).unwrap();
        let msg = ws.next().await.unwrap().unwrap();

        match msg {
            WsMessage::Text(text) => assert!(text.contains("\"tick\":1")),
            other => panic!("unexpected message: {other:?}"),
        }

        shutdown.cancel();
    }
}
