use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures::SinkExt;
use futures::StreamExt;

use super::AppState;
use crate::watcher::WatcherState;

pub fn ws_routes() -> Router<Arc<AppState>> {
    Router::new().route("/ws", get(ws_handler))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state.watcher.clone()))
}

async fn handle_socket(socket: WebSocket, watcher: WatcherState) {
    let (mut sender, mut receiver) = socket.split();

    let mut rx = watcher.subscribe();

    // Forward watcher events to WebSocket client
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Consume incoming messages (keep connection alive)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {
            // Just consume client messages
        }
    });

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
