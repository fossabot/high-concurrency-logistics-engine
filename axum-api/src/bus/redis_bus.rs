use futures::stream::{StreamExt, FuturesUnordered};
use redis::{Script, AsyncCommands, pipe, streams:: {StreamRangeReply}};
use std::{sync::Arc, collections::HashSet};
use crate::models::error::SyncError;
use crate::models::{user::User, state::{AppState, StreamEvent}};
use crate::models::location_user::LocationUpdate;
use crate::components::batch_postgres::{parse_entry, insert_batch};
/// Channel name convention: one channel per parcel
fn channel(parcel_id: &str) -> String {
    format!("{{{parcel_id}}}:channel:parcel:")
}

/// Redis key for last known position hash
fn position_key(parcel_id: &str) -> String {
    format!("{{{parcel_id}}}:parcel")
}

fn geo_key(parcel_id: &str) -> String {
    format!("{{{parcel_id}}}:active_drivers")
}

fn history_key(parcel_id: &str) -> String {
    format!("{{{parcel_id}}}:history")
}

fn stream_registry() -> String {
    format!("parcel:global:stream_registry")
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
    let mut conn = state.redis_manager.clone();

    // Broadcast to all subscribers on this channel
    let _: () = pipe()
        .spublish(channel(parcel_id), payload)
        .geo_add(geo_key(parcel_id), (lat, lon, driver_id))
        .hset(position_key(parcel_id), "data", payload)// Persist last known position (customer joining late gets this immediately)
        .query_async(&mut conn)
        .await?;


    // Expire after 6 hours — parcel delivered, no need to keep forever
    conn.expire::<_, ()>(position_key(parcel_id), 6 * 3600)
        .await?;

    Ok(())
}

/// Fetch last known position for a parcel (for customers connecting mid-delivery).
pub async fn last_position(
    state: &Arc<AppState>,
    parcel_id: &str,
) -> redis::RedisResult<Option<String>> {
    let mut conn = state.redis_manager.clone();
    let temp = conn.hget(position_key(parcel_id), "data").await?;
    tracing::info!("{parcel_id}: last position: {:?}", temp);
    Ok(temp)
}

/// Subscribe to a parcel channel and fan-out into the in-process broadcast.
/// Spawned once per parcel when the first customer connects.
pub async fn subscribe_parcel(parcel_id: String, state: Arc<AppState>) {
    let mut stream = state.redis_manager.clone();
    let mut payload = Vec::new();
    let result: redis::RedisResult<StreamRangeReply> = redis::cmd("XREVRANGE")
                    .arg(&history_key(&parcel_id)).arg("+").arg("-").arg("COUNT").arg(1)
                    .query_async(&mut stream).await;
    if let Ok(entries)= result {
        if let Some(entry) = entries.ids.first(){
            let x = parse_entry(&entry);
            if let Some(data)= x{
                payload.push(data);
            }
        }
    }


    let tx = state.channel_for(&parcel_id);
    // No active WebSocket customers — clean up and stop
    if tx.receiver_count() == 0 {
        state.parcels.remove_if(&parcel_id, |_, tx_entry| tx_entry.receiver_count() == 0);
        tracing::info!("No customers left for {parcel_id}, stopping subscriber");
        return;
    }
    if let Some(update) = payload.first() {
        if let Ok(json_string) = serde_json::to_string(update) {
            let _ = tx.send(json_string);
        }
    }

}

/// Publish a message to the Redis stream for the given parcel after a delay
pub async fn redis_stream_publish(state: &Arc<AppState>, parcel_id: &str) -> Result<(), SyncError> {
    let mut stream = state.redis_manager.clone();
    let stream_tx = state.redis_channel.clone();
    let script = Script::new(r#"
        -- 1. Read from Hash internally (Atomic)
               local current_val = redis.call('HGET', KEYS[2], 'data')
               if  current_val == false then return -1 end -- Hash not found

               -- 2. Check Stream Tail
               local last_entry = redis.call('XREVRANGE', KEYS[1], '+', '-', 'COUNT', 1)
               if #last_entry > 0 then
                   local fields = last_entry[1][2]
                   for i = 1, #fields, 2 do
                       if fields[i] == 'payload' and fields[i+1] == current_val then
                           return 0 -- Duplicate
                       end
                   end
               end

               -- 3. Write to Stream
               redis.call('XADD', KEYS[1], 'MAXLEN', '~', 1000,  '*' , 'payload', current_val)
               return 1
        "#
    );
    let result: i64 = script
        .key(history_key(parcel_id)) // KEYS[1] = history
        .key(position_key(parcel_id)) // KEYS[2] = position_key
        .invoke_async(&mut stream)
        .await?;
    if result == 0 {
        tracing::debug!("Skipping duplicate history for {}", parcel_id);
        return Ok(());
    }
    else if result == -1 {
        tracing::warn!("Position hash missing for {}", parcel_id);
        return Ok(());
    }
    else if result == 1 {
        redis::cmd("SADD")
                .arg(stream_registry())
                .arg(history_key(parcel_id))
                .query_async::<()>(&mut stream)
                .await?;
        stream_tx.send(StreamEvent::parcel_stream(&history_key(parcel_id))).await?;
        return Ok(());
    } else {
        return Ok(());
    }



}

/// Send batch message from Redis stream to the postgres history table
pub async fn redis_stream_to_postgres(parcel_stream_keys: &mut HashSet<String>, state: &Arc<AppState>) -> Result<(), SyncError> {
    let keys_vec: Vec<String> = parcel_stream_keys.drain().collect();
    if keys_vec.is_empty() {
        return Ok(());
    }
    let mut tasks = FuturesUnordered::new();
    let mut finished_keys_vec = Vec::new();
    let mut location_entries:Vec<LocationUpdate> = Vec::new();
    for key in keys_vec{
        let mut stream = state.redis_manager.clone();
        tasks.push(async move{
            let res: redis::RedisResult<StreamRangeReply> = redis::cmd("XREVRANGE")
                            .arg(&key).arg("+").arg("-").arg("COUNT").arg(1)
                            .query_async(&mut stream).await;
            (key, res)
        })
    }

    while let Some((key, result )) = tasks.next().await {
        finished_keys_vec.push(key);
        if let Ok(entries)= result {
            if let Some(entry) = entries.ids.first(){
                let x = parse_entry(&entry);
                if let Some(data)= x{
                    location_entries.push(data);
                }
            }
        }
    }
        insert_batch(&state.pool, &location_entries).await?;

        tracing::info!("Batching Postgres Success ");

    Ok(())
}


pub async fn publish_otp(otp: &u32, user: &User, state: &Arc<AppState>) -> Result<(), SyncError> {
    let otp_str = otp.to_string();
    let user_string = serde_json::to_string(user).unwrap_or_default();
    let mut stream = state.redis_manager.clone();
    let _: ((), ()) = pipe()
        .set_ex(format!("otp for {}", user.email), otp_str, 300)
        .set_ex(format!("pending for {}", user.email), user_string, 900)
        .query_async(&mut stream)
        .await?;
    Ok(())
}

pub async fn read_otp(otp: &u32, email: &str, state: &Arc<AppState>) -> Result<(), SyncError> {
    let mut stream = state.redis_manager.clone();
    let otp_str: Option<String> = stream.get(format!("otp for {}", email)).await?;
    let otp_user: Option<u32> = otp_str.map(|s| s.parse().ok()).flatten();
    println!("otp_user: {:?}, otp: {:?}", otp_user, otp);
    if otp_user == Some(*otp) {
        let user_str: Option<String> = stream.get(format!("pending for {}", email)).await?;
        let user: User = serde_json::from_str(&user_str.unwrap_or_default()).map_err(|e| SyncError::Json(e))?;
        let user_role: String = user.role.to_string();
        tracing::info!("OTP verified for email: {:?}", user);
        let _: ((), ()) = pipe()
            .del(format!("otp for {}", email))
            .del(format!("pending for {}", email))
            .query_async(&mut stream)
            .await?;
        let x = sqlx::query("INSERT INTO users (id, name, email, password, created_at, role AS text) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(&user.id)
            .bind(&user.name)
            .bind(&user.email)
            .bind(&user.password)
            .bind(&user.created_at)
            .bind(&user_role)
            .execute(&state.pool)
            .await?;
        tracing::info!("User inserted into database: {:?}", x);

    }
    Ok(())
}
