use crate::bus::redis_bus::channel;
use dashmap::DashMap;
use fred::clients::{Pool, SubscriberClient};
use jsonwebtoken::{DecodingKey, EncodingKey};
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::PgPool;
use tokio::sync::{mpsc, watch};
/// One in-process broadcast sender per parcel.
/// Customers subscribe to this; the Redis listener feeds it.
#[derive(Clone)]
pub struct AppState {
    pub redis_client: Pool,
    pub redis_subscriber: SubscriberClient,
    pub mailer: AsyncSmtpTransport<Tokio1Executor>,
    pub pool: PgPool,
    pub parcels: DashMap<String, watch::Sender<String>>,
    pub jwt_encoding_key: EncodingKey,
    pub jwt_decoding_key: DecodingKey,
    pub redis_channel: mpsc::Sender<StreamEvent>,
}

impl AppState {
    pub async fn new(
        redis_client: Pool,
        redis_subscriber: SubscriberClient,
        pool: PgPool,
        mailer: AsyncSmtpTransport<Tokio1Executor>,
        jwt_encoding_key: EncodingKey,
        jwt_decoding_key: DecodingKey,
        redis_channel: mpsc::Sender<StreamEvent>,
    ) -> Self {
        Self {
            redis_client,
            redis_subscriber,
            mailer,
            pool,
            parcels: DashMap::new(),
            jwt_encoding_key,
            jwt_decoding_key,
            redis_channel,
        }
    }

    /// Get or create the in-process channel for a parcel.
    pub fn channel_for(&self, parcel_id: &str) -> watch::Sender<String> {
        //Only for subsriber pubsub of redis
        self.parcels
            .entry(channel(parcel_id))
            .or_insert_with(|| watch::channel::<String>("None".to_string()).0)
            .clone()
    }
}

#[allow(dead_code)]
#[allow(clippy::useless_format)]
#[derive(Debug, Clone)]
pub enum StreamEvent {
    ParcelStream { stream_key: String },
    ParcelDelivered { parcel_id: String },
}

#[allow(dead_code)]
#[allow(clippy::useless_format)]
impl StreamEvent {
    pub fn parcel_stream(stream_key: &str) -> Self {
        Self::ParcelStream {
            stream_key: stream_key.to_string(),
        }
    }
    pub fn parcel_stream_duplicate(stream_key: &str) -> Self {
        Self::ParcelStream {
            stream_key: stream_key.to_string(),
        }
    }
    pub fn parcel_delivered(parcel_id: &str) -> Self {
        Self::ParcelDelivered {
            parcel_id: parcel_id.to_string(),
        }
    }
    pub fn parcel_stream_key(&self) -> Option<&str> {
        match self {
            Self::ParcelStream { stream_key } => Some(stream_key.as_str()),
            _ => None,
        }
    }
}
