use crate::bus::redis_bus;
use crate::models::{
    location_user::{ConnectParams, LocationUpdate},
    state::AppState,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::time::{ Duration, interval, sleep};


pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<ConnectParams>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {

    ws.on_upgrade(move |socket| async move {
        match params.role.as_str() {
            "driver" => handle_driver(socket, state, params.parcel_id).await,
            _ => { /* unauthorized */ }
        }
    })
}

/// Driver side: receive location from socket, publish to Redis
async fn handle_driver(mut socket: WebSocket, state: Arc<AppState>, parcel_id: String) {
    tracing::info!("Driver connected for parcel {parcel_id}");
    let mut stream_tick = interval(Duration::from_secs(10));
    let idle_timeout = sleep(Duration::from_secs(120));
    tokio::pin!(idle_timeout);
    stream_tick.tick().await;

    loop {
        tokio::select! {
                msg = socket.recv() => {
                idle_timeout.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(120));
                // Validate the shape before publishing
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(update) = serde_json::from_str::<LocationUpdate>(&text) {
                            if let Err(e) = redis_bus::publish(&state, &parcel_id, &text, &update.latitude, &update.longitude, &update.driver_id).await {
                                tracing::error!(%parcel_id, "Redis publish error: {e}");
                                break;
                            }
                            socket.send(Message::Text(r#"{"status": "ok"}"#.into())).await.ok();
                            continue;
                        } else {
                            tracing::warn!("Invalid location payload, dropping {text}");
                            socket.send(Message::Text(r#"{"status": "close"}"#.into())).await.ok();
                            break;
                        }
                    },
                    Some(Err(e)) => { tracing::warn!("Invalid location payload , Error: {:?}", e); socket.send(Message::Text(r#"{"status": "close"}"#.into())).await.ok(); break; },
                    Some(Ok(Message::Ping(payload))) => {  tracing::info!("Location update received for parcel {parcel_id}"); socket.send(Message::Pong(payload)).await.ok(); continue; },
                    Some(Ok(Message::Pong(_))) => { continue; },
                    Some(Ok(Message::Close(_))) => {
                        tracing::info!("Client closed connection for parcel {parcel_id}");
                        break;
                    },
                    // Anything else — log but don't break
                    Some(Ok(_)) => {
                        tracing::warn!("Unknown message type for parcel {parcel_id}");
                        continue;
                    },
                    None => { tracing::warn!("Invalid location payload, dropping nothing"); socket.send(Message::Text(r#"{"status": "close"}"#.into())).await.ok(); break; }
                }//match msg

            }//msg
                _ = stream_tick.tick() => {
                    tracing::info!("Sending ping for parcel STREAM {parcel_id}");
                if let Err(e) = redis_bus::redis_stream_publish(&state, &parcel_id).await {
                    tracing::error!("Redis STREAM publish error: {e}");
                    socket.send(Message::Text(r#"{"status": "close"}"#.into())).await.ok();
                } else {
                    tracing::info!("Redis STREAM publish success");
                    socket.send(Message::Text(r#"{"status": "stream"}"#.into())).await.ok();
                }
                continue;
            }
            _ = &mut idle_timeout => {
                tracing::info!("No activity for 2 minutes, closing connection for parcel {parcel_id}");
                socket.send(Message::Text(r#"{"status": "close"}"#.into())).await.ok();
                break;
            }

        }//tokio: select

    }//loop

    tracing::info!("Driver disconnected for parcel {parcel_id}");
}
