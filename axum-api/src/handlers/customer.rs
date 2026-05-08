use crate::bus::redis_bus;
use crate::models::location_user::ConnectParams;
use crate::models::state::AppState;
use axum::extract::ws::Message;
use axum::extract::Query;
use axum::extract::{ws::WebSocketUpgrade, State};
use axum::response::IntoResponse;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use fred::prelude::*;
use redis_bus::channel;

pub async fn customer_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<ConnectParams>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
         if params.role.as_str() == "customer" {
        let parcel_id = params.parcel_id;
        // 1. Send "Initial State" immediately (UX best practice)
        if let Ok(Some(last)) = redis_bus::last_position(&state, &parcel_id).await {
            let _ = socket.send(Message::Text(last.into())).await;
        }

        let tx = state.channel_for(&parcel_id);
        tracing::info!("the tx for each {:?}", tx);

         if tx.receiver_count() <= 1 {
             if let Err(e) = redis_bus::subscribe_parcel(&state, &parcel_id).await {
                 tracing::error!("Redis SSUBSCRIBE failed: {}", e);
                 return;
             }
         }
         let mut rx = tx.subscribe();
        // It doesn't matter if the Switchboard is running; if we don't SSUBSCRIBE, Redis won't send anything.
        // This is idempotent. Safe to call 100 times.
        // 3. Get the Internal Channel
        // We assume state.channel_for uses the .or_insert_with() pattern we discussed.

       // <--- SUBSCRIBE ONLY ONCE

        tracing::info!("User connected to Internal Room: {}", parcel_id);

        // 4. Setup Timeout
        let idle_timeout = sleep(Duration::from_secs(120));
        tokio::pin!(idle_timeout);

        loop {
            tokio::select! {
                // A: Handle Internal Messages (From Switchboard)
                res = rx.changed() => {
                    match res {
                        Ok(_) => {
                            // Reset timeout on activity
                            let msg_cloned = rx.borrow_and_update().clone();
                            idle_timeout.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(120));

                            // Send to User
                            if let Err(_) = socket.send(Message::Text(msg_cloned.clone().into())).await {
                                                  break; // Connection closed
                                              }



                             continue // User disconnected

                        }
                        Err(_) => break, // Channel closed
                    }
                }

                // B: Handle Incoming Socket Data (Ping/Close)
                res = socket.recv() => {
                    match res {
                        Some(Ok(Message::Close(_))) => break,
                        Some(Ok(Message::Ping(p))) => {
                            idle_timeout.as_mut().reset(tokio::time::Instant::now() + tokio::time::Duration::from_secs(120));
                            let _ = socket.send(Message::Pong(p)).await;
                        }

                        Some(Err(e)) => {
                            tracing::error!("Socket Error: {}", e);
                            break;
                        }
                        None => break, // Stream closed
                        _ =>  continue // Ignore other messages
                    }
                }

                // C: Handle Timeout
                _ = &mut idle_timeout => {
                    tracing::warn!("Client idle for 120s. Disconnecting.");
                    break;
                }
            }
        }




        if tx.receiver_count() == 0 {
            tracing::info!("Last user left {}. Stopping Redis stream.", parcel_id);
            let _ = state.redis_subscriber.sunsubscribe(channel(&parcel_id)).await;
             state.parcels.remove(&channel(&parcel_id));
            }
         }
    })


}
