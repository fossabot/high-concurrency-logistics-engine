use crate::bus::redis_bus;
use crate::models::location_user::ConnectParams;
use crate::models::state::AppState;
use axum::extract::ws::Message;
use axum::extract::Query;
use axum::extract::{ws::WebSocketUpgrade, State};
use axum::response::IntoResponse;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub async fn customer_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<ConnectParams>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        if params.role.as_str() == "customer" {
            let parcel_id = params.parcel_id;
            tracing::info!("Customer connected for parcel {parcel_id}");

            // Send last known position immediately so the map isn't blank
            if let Ok(Some(last)) = redis_bus::last_position(&state, &parcel_id).await {
                tracing::info!("{parcel_id}: last position: {last}");
                let _ = socket.send(Message::Text(last.into())).await;
            }

            // Ensure a Redis subscriber task is running for this parcel
            let tx = state.channel_for(&parcel_id);
            let value = parcel_id.clone();
            if tx.receiver_count() == 0 {
                         // First customer — spawn the Redis subscriber
                         tracing::info!("Spawning subscribe_parcel for {}", parcel_id);
                         tokio::spawn(redis_bus::subscribe_parcel(
                             value.clone(),
                             state
                         ));
            }
            let idle_timeout = sleep(Duration::from_secs(120));
            tokio::pin!(idle_timeout);
            let mut rx = tx.subscribe();

            loop {
                tokio::select! {
                    // Forward Redis messages to this customer's WebSocket
                    Ok(msg) = rx.recv() => {
                        idle_timeout.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(120));
                         tracing::info!("{parcel_id}: {msg}");
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break; // customer disconnected
                        }

                    }


                // Handle ping / close from customer side

                msg = socket.recv() => {
                            match msg {
                                Some(Ok(Message::Close(_))) => break,
                                Some(Ok(Message::Ping(p))) => {
                                    let _ = socket.send(Message::Pong(p)).await;
                                }
                                Some(Ok(_)) => { /* Handle other messages */ }
                                Some(Err(e)) => {
                                   tracing::error!("Error: {}", e);
                                    break;
                                }
                                None => break, // Stream closed
                            }
                        }
                        _ =    &mut idle_timeout => {
                           tracing::warn!("No message received for 120s, timing out.");
                           break;
                        }
                }//tokio
            }//loop
        }//params
    })
}
