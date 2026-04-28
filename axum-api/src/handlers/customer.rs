use std::sync::Arc;
use axum::extract::ws::Message;
use tokio::time::{sleep, Duration};
use crate::bus::redis_bus;
use axum::extract::{ws::WebSocketUpgrade, State};
use crate::models::state::AppState;
use axum::extract::Query;
use crate::models::location_user::ConnectParams;
use axum::response::IntoResponse;


pub async fn customer_handler(ws: WebSocketUpgrade, Query(params): Query<ConnectParams>, State(state): State<Arc<AppState>>) -> impl IntoResponse {
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
            if tx.receiver_count() == 0 {
                // First customer — spawn the Redis subscriber
                tokio::spawn(redis_bus::subscribe_parcel(
                    parcel_id.clone(),
                    state
                ));
            }
            let mut rx = tx.subscribe();

            loop {
                tokio::select! {
                    // Forward Redis messages to this customer's WebSocket
                    Ok(msg) = rx.recv() => {
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
                _ = sleep(Duration::from_secs(60)) => {
                           eprintln!("No message received for 60s, timing out.");
                           break;
                        }
                }//tokio
            }//loop
        }//params
    })


}
