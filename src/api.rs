use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};

#[derive(Clone)]
pub struct AppState {
    pub input_tx: mpsc::Sender<Bytes>,
    pub output_rx: broadcast::Sender<Bytes>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn input(State(state): State<AppState>, body: Bytes) -> StatusCode {
    match state.input_tx.send(body).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!("Failed to send input to PTY: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn ws_raw(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_raw(socket, state))
}

async fn handle_ws_raw(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let mut output_rx = state.output_rx.subscribe();
    let input_tx = state.input_tx.clone();

    // Task: broadcast PTY output -> WebSocket
    let mut tx_task = tokio::spawn(async move {
        while let Ok(data) = output_rx.recv().await {
            if ws_tx.send(Message::Binary(data.to_vec())).await.is_err() {
                break;
            }
        }
    });

    // Task: WebSocket input -> PTY
    let mut rx_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            let data = match msg {
                Message::Binary(data) => Bytes::from(data),
                Message::Text(text) => Bytes::from(text),
                Message::Close(_) => break,
                _ => continue,
            };
            if input_tx.send(data).await.is_err() {
                break;
            }
        }
    });

    // Wait for either task to finish, then abort the other
    tokio::select! {
        _ = &mut tx_task => rx_task.abort(),
        _ = &mut rx_task => tx_task.abort(),
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/input", post(input))
        .route("/ws/raw", get(ws_raw))
        .with_state(state)
}
