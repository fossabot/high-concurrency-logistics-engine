use std::sync::Arc;
use crate::models::state::AppState;
use fred::prelude::*;
use tokio::sync::broadcast::error::RecvError;

pub async fn start_global_redis(state: Arc<AppState>) {
    tracing::info!("Global Switchboard Active: Listening to Redis...");
       let client = state.redis_subscriber.clone();
       // 1. Get the Broadcast Receiver
       let mut global_stream = client.message_rx();

       // 2. Use loop + match (Best for Result types)
       loop {
           match global_stream.recv().await {
               // CASE A: Success - We got a message
               Ok(msg) => {
                      let channel_name = msg.channel.to_string();
                       let payload = msg.value.convert::<String>().unwrap_or_default();


                   if let Some(tx) = state.parcels.get(&channel_name) {

                             let _ = tx.send(payload);
                        // Send to the user's room

                   }
               }

               // CASE B: Lag - The Switchboard is too slow!
               Err(RecvError::Lagged(skipped_count)) => {
                   // CRITICAL WARNING: This means Redis is pushing faster than this loop can process.
                   // We lost data here.
                   tracing::warn!("CRITICAL: Switchboard skipped {} messages from Redis.", skipped_count);
               }

               // CASE C: Closed - Connection died
               Err(RecvError::Closed) => {
                   tracing::error!("Redis Global Connection Closed. Stopping Switchboard.");
                   break;
               }
           }
       }
}
