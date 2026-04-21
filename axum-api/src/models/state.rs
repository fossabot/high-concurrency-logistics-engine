use dashmap::DashMap;
use redis::{aio::ConnectionManager, Client};
use tokio::sync::broadcast;
use sqlx::PgPool;
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use jsonwebtoken::{EncodingKey, DecodingKey};


/// One in-process broadcast sender per parcel.
/// Customers subscribe to this; the Redis listener feeds it.
#[derive(Clone)]
pub struct AppState {
    pub redis_manager : ConnectionManager,
    pub redis_client: Client,
    pub mailer: AsyncSmtpTransport<Tokio1Executor>,
    pub pool: PgPool,
    pub parcels: DashMap<String, broadcast::Sender<String>>,/// parcel_id → sender for that parcel's location stream
    pub jwt_encoding_key: EncodingKey,
    pub jwt_decoding_key: DecodingKey,
}

impl AppState {
    pub async fn new(redis_manager: ConnectionManager, redis_client: redis::Client, pool: PgPool, mailer: AsyncSmtpTransport<Tokio1Executor>, jwt_encoding_key: EncodingKey, jwt_decoding_key: DecodingKey) -> Self {
        Self {
            redis_manager,
            redis_client,
            mailer,
            pool,
            parcels: DashMap::new(),
            jwt_encoding_key,
            jwt_decoding_key,
        }
    }

    /// Get or create the in-process channel for a parcel.
    pub fn channel_for(&self, parcel_id: &str) -> broadcast::Sender<String> {       //Only for subsriber pubsub of redis
        self.parcels
            .entry(parcel_id.to_string())
            .or_insert_with(|| broadcast::channel::<String>(32).0)
            .clone()
    }
}
