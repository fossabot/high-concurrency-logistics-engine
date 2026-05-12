use crate::components::batch_postgres::{insert_batch, parse_entry};
use crate::models::error::SyncError;
use crate::models::location_user::LocationUpdate;
use crate::models::{
    state::{AppState, StreamEvent},
    user::User,
};
use fred::prelude::*;
use fred::types::geo::{GeoPosition, GeoValue};
use fred::types::{streams::XCap, Value};
use futures::stream::FuturesUnordered;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio_stream::StreamExt;
/// Channel name convention: one channel per parcel
pub fn channel(parcel_id: &str) -> String {
    format!("{{{}}}:channel:parcel", parcel_id)
}

/// Redis key for last known position hash
fn position_key(parcel_id: &str) -> String {
    format!("{{{}}}:parcel", parcel_id)
}

fn geo_key(parcel_id: &str) -> String {
    format!("{{{}}}:active_drivers", parcel_id)
}

fn history_key(parcel_id: &str) -> String {
    format!("{{{}}}:history", parcel_id)
}

#[allow(dead_code)]
#[allow(clippy::useless_format)]
fn stream_registry() -> String {
    format!("parcel:global:stream_registry")
}

fn otp_key(email: &str) -> String {
    format!("{{{}}}:otp_email", email)
}

fn pending_user(email: &str) -> String {
    format!("{{{}}}:pending_user", email)
}

/// Publish a location update to Redis.
/// Also persists it as the last known position.
pub async fn publish(
    state: &Arc<AppState>,
    parcel_id: &str,
    payload: &str,
    lat: &f64,
    lon: &f64,
    driver_id: &str,
) -> Result<(), SyncError> {
    let client = state.redis_client.next();
    let lon_val = *lon;
    let lat_val = *lat;
    let geo_pos = GeoPosition::from((lon_val, lat_val));
    let geo_val = GeoValue {
        coordinates: geo_pos,
        member: driver_id.to_string().into(),
    };
    client
        .spublish::<(), _, _>(channel(parcel_id), payload)
        .await?;
    let pipe = client.pipeline();

    // Broadcast to all subscribers on this channel

    pipe.geoadd::<(), _, _>(geo_key(parcel_id), None, false, geo_val)
        .await?;
    pipe.hset::<(), _, _>(position_key(parcel_id), ("data", payload))
        .await?;
    pipe.expire::<(), _>(position_key(parcel_id), (60 * 3600) as i64, None)
        .await?;
    match pipe.all::<()>().await {
        Ok(_) => tracing::info!("Pipeline executed for {}", parcel_id),
        Err(e) => tracing::error!("Pipeline FAILED for {}: {:?}", parcel_id, e),
    }

    Ok(())
}

/// Fetch last known position for a parcel (for customers connecting mid-delivery).
pub async fn last_position(
    state: &Arc<AppState>,
    parcel_id: &str,
) -> Result<Option<String>, SyncError> {
    let conn = state.redis_client.next();
    tracing::info!("Start for last postion");
    match conn
        .hget::<Option<String>, _, _>(position_key(parcel_id), "data")
        .await
    {
        Ok(Some(val)) => {
            tracing::info!("Got last position: {}", val);
            Ok(Some(val))
        }
        Ok(None) => {
            tracing::info!("No last position for {}", parcel_id);
            Ok(None)
        }
        Err(e) => {
            tracing::error!("hget error for {}: {:?}", parcel_id, e);
            Err(e.into())
        }
    }
}

pub async fn subscribe_parcel(state: &Arc<AppState>, parcel_id: &str) -> Result<(), SyncError> {
    let client = state.redis_subscriber.clone();
    client.ssubscribe(channel(parcel_id)).await?;
    Ok(())
}

/// Publish a message to the Redis stream for the given parcel after a delay
pub async fn redis_stream_publish(state: &Arc<AppState>, parcel_id: &str) -> Result<(), SyncError> {
    let stream_tx = state.redis_channel.clone();
    const SCRIPT: &str = r#"
        -- 1. Read from Hash internally (Atomic)
               local current_val = redis.call('HGET', KEYS[2], 'data')
               if  current_val == false then return -1 end -- Hash not found
               local data = cjson.decode(current_val)
               local lat = data.latitude
               local long = data.longitude


               -- 2. Check Stream Tail
               local last_entry = redis.call('XREVRANGE', KEYS[1], '+', '-', 'COUNT', 1)
               if #last_entry > 0 then
                   local fields = last_entry[1][2]
                   for i = 1, #fields, 2 do
                       if fields[i] == 'payload' then
                           local val = fields[i+1]
                           local data_val = cjson.decode(val)
                           local val_lat = data_val.latitude
                           local val_long = data_val.longitude
                           if val_lat == lat and val_long == long then
                               return 0 -- Duplicate
                           end
                       end
                   end
               end

               -- 3. Write to Stream
               redis.call('XADD', KEYS[1], 'MAXLEN', '~', 1000,  '*' , 'payload', current_val)
               return 1
        "#;
    let result: i64 = state
        .redis_client
        .next()
        .eval(
            SCRIPT,
            vec![history_key(parcel_id), position_key(parcel_id)],
            vec![] as Vec<String>, // Use hashtags for Cluster!
        )
        .await?;
    if result == 0 {
        tracing::debug!("Skipping duplicate history for {}", parcel_id);
        Ok(())
    } else if result == -1 {
        tracing::warn!("Position hash missing for {}", parcel_id);
        Ok(())
    } else if result == 1 {
        let _: i64 = state
            .redis_client
            .next()
            .sadd("online_drivers", vec![parcel_id])
            .await?;
        stream_tx
            .send(StreamEvent::parcel_stream(&history_key(parcel_id)))
            .await?;
        tracing::info!("Lua is USED HEREE");
        Ok(())
    } else {
        Ok(())
    }
}

/// Send batch message from Redis stream to the postgres history table
pub async fn redis_stream_to_postgres(
    parcel_stream_keys: &mut HashSet<String>,
    state: &Arc<AppState>,
) -> Result<(), SyncError> {
    let keys_vec: Vec<String> = parcel_stream_keys.drain().collect();
    if keys_vec.is_empty() {
        return Ok(());
    }
    let mut tasks = FuturesUnordered::new();
    let mut finished_id_vec = HashMap::new();
    let mut location_entries: Vec<LocationUpdate> = Vec::with_capacity(keys_vec.len());
    for key in keys_vec {
        let client = state.redis_client.next();
        tasks.push(async move {
            let result: Result<Vec<Value>, Error> =
                client.xrevrange(key.clone(), "+", "-", Some(1)).await;
            (key, result)
        })
    }

    while let Some((key, result)) = tasks.next().await {
        if let Ok(entries) = result {
            if let Some(entry_value) = entries.first() {
                if let Ok((data, id)) = parse_entry(entry_value) {
                    location_entries.push(data);
                    finished_id_vec.insert(key, id);
                }
            }
        }
    }

    let mut trim_tasks = FuturesUnordered::new();
    insert_batch(&state.pool, &location_entries).await?;
    tracing::info!("Batching Postgres Success ");
    for (key, id) in finished_id_vec {
        let client = state.redis_client.next();
        trim_tasks.push(async move {
            let _: () = client
                .xtrim(key, XCap::try_from(("MINID", "~", id.clone()))?)
                .await?;
            Ok::<(), fred::error::Error>(())
        })
    }

    while let Some(result) = trim_tasks.next().await {
        if let Err(e) = result {
            tracing::error!("Redis xtrim failed: {e}");
        }
    }

    Ok(())
}

pub async fn publish_otp(otp: &u32, user: &User, state: &Arc<AppState>) -> Result<(), SyncError> {
    let otp_str = otp.to_string();
    let user_string = serde_json::to_string(user).unwrap_or_default();
    let client = state.redis_client.next();
    let pipe = client.pipeline();
    pipe.set::<(), _, _>(
        otp_key(&user.email),
        otp_str,
        Some(Expiration::EX(300)),
        None,
        false,
    )
    .await?;
    pipe.set::<(), _, _>(
        pending_user(&user.email),
        user_string,
        Some(Expiration::EX(300)),
        None,
        false,
    )
    .await?;
    let _: () = pipe.all().await?;
    Ok(())
}

pub async fn read_otp(otp: &u32, email: &str, state: &Arc<AppState>) -> Result<(), SyncError> {
    let client = state.redis_client.next();
    let otp_str: Option<String> = client.get(otp_key(email)).await?;
    let otp_user: Option<u32> = otp_str.and_then(|s| s.parse().ok());
    println!("otp_user: {:?}, otp: {:?}", otp_user, otp);
    if otp_user == Some(*otp) {
        let user_str: Option<String> = client.get(pending_user(email)).await?;
        let user: User = serde_json::from_str(&user_str.unwrap_or_default())?;
        let user_role: String = user.role.to_string();
        tracing::info!("OTP verified for email: {:?}", user);
        let pipe = client.pipeline();
        pipe.del::<(), _>(otp_key(email)).await?;
        pipe.del::<(), _>(pending_user(email)).await?;
        let _: () = pipe.all().await?;
        let x = sqlx::query("INSERT INTO users (id, name, email, password, created_at, role AS text) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(user.id)
            .bind(user.name)
            .bind(user.email)
            .bind(user.password)
            .bind(user.created_at)
            .bind(user_role)
            .execute(&state.pool)
            .await?;
        tracing::info!("User inserted into database: {:?}", x);
    }
    Ok(())
}
