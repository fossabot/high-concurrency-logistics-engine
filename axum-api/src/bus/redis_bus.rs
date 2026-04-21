use futures::stream::StreamExt;
use redis::{Script, AsyncCommands, pipe};
use std::sync::Arc;
use crate::WORKER_ID;
use crate::models::error::SyncError;
use crate::models::{user::User, state::AppState};
use crate::models::location_user::LocationUpdate;
/// Channel name convention: one channel per parcel
fn channel(parcel_id: &str) -> String {
    format!("channel:parcel:{parcel_id}")
}

/// Redis key for last known position hash
fn position_key(parcel_id: &str) -> String {
    format!("parcel:{parcel_id}")
}

fn geo_key() -> String {
    format!("active_drivers")
}

fn history_key() -> String {
    format!("parcel:history")
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
        .publish(channel(parcel_id), payload)
        .geo_add(geo_key(), (lat, lon, driver_id))
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
    let state_red = state.redis_client.clone();
    let mut pubsub = match state_red.get_async_pubsub().await {
           Ok(c) => c,
           Err(e) => {
               eprintln!("Redis connection error: {}", e);
               return;
           }
       };

    if let Err(e) = pubsub.subscribe(channel(&parcel_id)).await {
        tracing::error!("Redis subscribe failed: {e}");
        return;
    }

    tracing::info!("Redis subscriber started for parcel {parcel_id}");

    let mut stream = pubsub.on_message();
    loop {
        match stream.next().await {
            Some(msg) => {
                let payload: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(_) => {
                        tracing::warn!("Skipping malformed Redis payload");
                        continue;
                    },
                };
                let tx = state.channel_for(&parcel_id);
                // No active WebSocket customers — clean up and stop
                if tx.receiver_count() == 0 {
                    state.parcels.remove_if(&parcel_id, |_, tx_entry| tx_entry.receiver_count() == 0);
                    tracing::info!("No customers left for {parcel_id}, stopping subscriber");
                    break;
                }
                if let Ok(_valid_data) = serde_json::from_str::<LocationUpdate>(&payload) {
                    // Validation passed! Now pass the ORIGINAL string to the channel
                    let _ = tx.send(payload);
                } else {
                    tracing::warn!("Discarding invalid payload: {}", payload);
                }
            }
            None => break, // connection closed
        }
    }
}

/// Publish a message to the Redis stream for the given parcel after a delay
pub async fn redis_stream_publish(state: &Arc<AppState>, parcel_id: &str) -> Result<(), SyncError> {
    let mut stream = state.redis_manager.clone();
    let script = Script::new(r#"
        -- 1. Read from Hash internally (Atomic)
               local current_val = redis.call('HGET', KEYS[2], 'data')
               if not current_val then return -1 end -- Hash not found

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
               redis.call('XADD', KEYS[1], '*', 'payload', current_val)
               return 1
        "#
    );
    let result: i64 = script
        .key(history_key()) // KEYS[1] = history
        .key(position_key(parcel_id)) // KEYS[2] = position_key
        .invoke_async(&mut stream)
        .await?;
    if result == 0 {
        return Ok(());
    }
    else if result == -1 {
        return Ok(());
    }
    else if result == 1 {
        return Ok(());
    } else {
        return Ok(());
    }



}

/// Send batch message from Redis stream to the postgres history table
pub async fn redis_stream_to_postgres(state: &Arc<AppState>) -> Result<(), SyncError> {
    let worker_id = WORKER_ID.get().expect("worker id not set");
    tracing::info!("Start");
    let mut stream = state.redis_manager.clone();
    let stream_response: redis::streams::StreamReadReply = redis::cmd("XREADGROUP")
        .arg("GROUP")
        .arg("history-processor")
        .arg(&worker_id)
        .arg("COUNT")
        .arg(10000)
        .arg("BLOCK")
        .arg(100)
        .arg("STREAMS")
        .arg(history_key()) // The Key
        .arg(">")           // The ID
        .query_async(&mut stream)
        .await?;
    let mut parcel_ids = Vec::new();
    let mut driver_ids = Vec::new();
    let mut latitudes = Vec::new();
    let mut longitudes = Vec::new();
    let mut timestamps = Vec::new();
    let mut statuses = Vec::new();
    if stream_response.keys.is_empty() {
        tracing::warn!("no history entries found for in redis stream");
        return Ok(());
    }

    for stream_key in stream_response.keys {
        let mut processed_ids = Vec::new();
        for entry in stream_key.ids {
            // 1. Get the payload as a String first
            if let Some(val) = entry.map.get("payload") {
                // This is the "Best Way" to use FromRedisValue
                let json_str: String = redis::from_redis_value(val.clone()).unwrap_or_default();

                // 2. Immediately turn that string into your Rust Struct
                if let Ok(update) = serde_json::from_str::<LocationUpdate>(&json_str) {
                    parcel_ids.push(update.parcel_id);
                    driver_ids.push(update.driver_id);
                    latitudes.push(update.latitude);
                    longitudes.push(update.longitude);
                    timestamps.push(update.timestamp as i64);// sqlx cannot take Vec<u64>
                    statuses.push(format!("{:?}", update.status));
                    processed_ids.push(entry.id);
                }
            }
        }
        if !processed_ids.is_empty() {
            // Manually build the command: XACKDEL <key> <group> <ID>
            // Note: XACKDEL syntax is: XACKDEL key group id [id ...]
            // 1. Acknowledge (XACK) - Tells Redis the group is done with these
            let _: i32 = redis::cmd("XACK")
                .arg(history_key())
                .arg("history-processor")
                .arg(&processed_ids) // This can be a Vec of IDs
                .query_async(&mut stream)
                .await?;

            // 2. Delete (XDEL) - Removes the actual data from the stream
            let result: i32 = redis::cmd("XDEL")
                .arg(history_key())
                .arg(&processed_ids)
                .query_async(&mut stream)
                .await?;
            // result will be:
              //  1: Acknowledged and deleted
              // -1: ID not found
              //  2: Acknowledged but not deleted (if other groups still need it)
              tracing::info!(processed_ids = %processed_ids[0], result = %result, "XACKDEL result for {}: {}", processed_ids[0], result);
        }
    }

    if !parcel_ids.is_empty() {
        sqlx::query!(
            r#"
            INSERT INTO parcel_history (parcel_id, driver_id, latitude, longitude, timestamp, status)
             SELECT u.p_id, u.d_id, u.lat, u.lon, u.ts, u.status
             FROM UNNEST($1::text[], $2::text[], $3::float[], $4::float[], $5::bigint[], $6::text[]) AS u(p_id, d_id, lat, lon, ts, status)"#,
             &parcel_ids as &[String], // Rust knows this must be a Vec<String>
             &driver_ids as &[String],  // If types don't match the DB, it won't compile
             &latitudes as &[f64],
             &longitudes as &[f64],
             &timestamps ,
             &statuses as &[String],
        )
        .execute(&state.pool)
        .await?;
        tracing::info!("Inserted {:?} parcel history entries", parcel_ids);
    }

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
