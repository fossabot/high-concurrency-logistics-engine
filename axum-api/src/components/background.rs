use crate::bus::redis_bus::redis_stream_to_postgres;
use crate::models::state::AppState;
use crate::models::{error::SyncError, state::StreamEvent};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

pub async fn background_to_postgres(
    state: &Arc<AppState>,
    mut stream_rx: mpsc::Receiver<StreamEvent>,
) -> Result<(), SyncError> {
    tracing::info!("Start");
    let mut interval = interval(Duration::from_secs(4));
    let mut parcel_stream_keys: HashSet<String> = HashSet::new();
    loop {
        tokio::select! {
            _ = interval.tick() => {
            if !parcel_stream_keys.is_empty() {
                redis_stream_to_postgres(&mut parcel_stream_keys, &state).await?;
                }
            },
            key = stream_rx.recv() => {
                if let Some(key) = key {
                if let StreamEvent::ParcelStream { stream_key } = key {
                    parcel_stream_keys.insert(stream_key);
                    }
                }
                if parcel_stream_keys.len() > 1000 {
                    redis_stream_to_postgres(&mut parcel_stream_keys, &state).await?;
                }
            }

        }
    }
}
