use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use crate::models::ServerEvent;
use crate::state::AppState;

pub async fn events(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| socket_loop(socket, state))
}

async fn socket_loop(socket: WebSocket, state: AppState) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.events.subscribe();

    let snap = state.devices.snapshot();
    let hello = ServerEvent::Devices {
        devices: snap.devices,
        scanning: snap.scanning,
        last_error: snap.last_error,
    };
    if send(&mut sink, &hello).await.is_err() {
        return;
    }
    let _ = send(
        &mut sink,
        &ServerEvent::Playback {
            status: state.sessions.status(),
        },
    )
    .await;

    loop {
        tokio::select! {
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        if sink.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        if send(&mut sink, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send<S>(sink: &mut S, event: &ServerEvent) -> Result<(), ()>
where
    S: SinkExt<Message> + Unpin,
{
    let payload = serde_json::to_string(event).map_err(|_| ())?;
    sink.send(Message::Text(payload.into()))
        .await
        .map_err(|_| ())
}
