use crate::models::{location_user::LocationUpdate, error::SyncError};
use fred::prelude::*;
use metrics;
use sqlx::PgPool;

pub fn parse_entry(entry: &Value) -> Result<(LocationUpdate,String),SyncError> {
    let entry_parts = match entry {
        Value::Array(arr) => arr,
        _ => return Err(SyncError::Other("None".to_string())),
    };

    let redis_id = match entry_parts.get(0){
        Some(Value::String(s)) => s.to_string(),
        _ => return Err(SyncError::Other("None".to_string()))
    };
    // Redis Stream structure: [ID, [Field, Value]]

    let fields_array = match entry_parts.get(1) {
        Some(Value::Array(fields)) => fields,
        _ => return Err(SyncError::Other("None".to_string())),
    };


    let json_payload = match fields_array.get(1) {
        Some(Value::String(s)) => s,
        _ => return Err(SyncError::Other("None".to_string())),
    };


    let location = match serde_json::from_str::<LocationUpdate>(&json_payload) {
        Ok(loc) => loc,
        Err(e) => {
            tracing::error!("Failed to parse location payload: {e} — payload: {json_payload}");
            metrics::counter!("batch_parse_errors").increment(1);
            return Err(SyncError::Json(e)) // skip this entry, process the rest
        }

    };
    return Ok((location, redis_id))
}

pub async fn insert_batch(db: &PgPool, batch: &[LocationUpdate]) -> Result<(), sqlx::Error> {
    let start = std::time::Instant::now();

    let parcel_ids: Vec<String> = batch.iter().map(|u| u.parcel_id.clone()).collect();
    let driver_ids: Vec<String> = batch.iter().map(|u| u.driver_id.clone()).collect();
    let latitudes: Vec<f64> = batch.iter().map(|u| u.latitude).collect();
    let longitudes: Vec<f64> = batch.iter().map(|u| u.longitude).collect();
    let timestamps: Vec<i64> = batch.iter().map(|u| u.timestamp).collect();
    let statuses: Vec<String> = batch.iter().map(|u| format!("{:?}", u.status)).collect();

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
        }
        Err(e) => {
            // Error! Record a failure metric for your Grafana dashboard
            metrics::counter!("postgres_errors_total").increment(1);
            tracing::warn!("Database error: {:?}", e);
            return Err(e);
        }
    }
}
