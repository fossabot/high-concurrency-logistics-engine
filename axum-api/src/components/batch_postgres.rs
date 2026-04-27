use crate::models::location_user::LocationUpdate;
use redis::streams::StreamId;
use sqlx::PgPool;
use metrics;
use redis::FromRedisValue;

pub fn parse_entry(
    entry: &StreamId
) -> Option<LocationUpdate> {
    // Parse payload JSON stored by your Lua script
    let value =  entry.map.get("payload")?;
    let payload:String =  redis::from_redis_value(value.clone()).ok()?;
    let update: LocationUpdate = serde_json::from_str(&payload).ok()?;
    Some(LocationUpdate {
        parcel_id: update.parcel_id,
        driver_id: update.driver_id,
        latitude: update.latitude,
        longitude: update.longitude,
        timestamp: update.timestamp,
        status: update.status,
    })
}

pub async fn insert_batch(
    db: &PgPool,
    batch: &[LocationUpdate],
) -> Result<(), sqlx::Error> {
    let start = std::time::Instant::now();


    let parcel_ids: Vec<String> = batch
        .iter().map(|u| u.parcel_id.clone()).collect();
    let driver_ids: Vec<String> = batch
        .iter().map(|u| u.driver_id.clone()).collect();
    let latitudes: Vec<f64> = batch
        .iter().map(|u| u.latitude).collect();
    let longitudes: Vec<f64> = batch
        .iter().map(|u| u.longitude).collect();
    let timestamps: Vec<i64> = batch
        .iter().map(|u| u.timestamp).collect();
    let statuses: Vec<String> = batch
        .iter().map(|u| format!("{:?}", u.status)).collect();

    let result = sqlx::query!(
        r#"
        INSERT INTO parcel_history
            (parcel_id, driver_id, latitude, longitude,
             timestamp, status)
        SELECT * FROM UNNEST(
            $1::text[],
            $2::text[],
            $3::float8[],
            $4::float8[],
            $5::bigint[],
            $6::text[]
        )
        ON CONFLICT (parcel_id, timestamp) DO NOTHING
        "#,
        &parcel_ids,
        &driver_ids,
        &latitudes,
        &longitudes,
        &timestamps,
        &statuses,
    )
    .execute(db)
    .await;

    // Record how long the DB took to process the batch
    metrics::histogram!("postgres_batch_flush_seconds").record(start.elapsed().as_secs_f64());

    match result {
        Ok(query_result) => {
            // Success! Record the number of rows actually written
            let rows = query_result.rows_affected();
            metrics::counter!("postgres_records_written_total").increment(rows);
             Ok(())
        },
        Err(e) => {
            // Error! Record a failure metric for your Grafana dashboard
            metrics::counter!("postgres_errors_total").increment(1);
            tracing::warn!("Database error: {:?}", e);
            return Err(e)
        }
    }
}
