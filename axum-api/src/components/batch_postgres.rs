use crate::models::location_user::LocationUpdate;
use fred::prelude::*;
use metrics;
use sqlx::PgPool;

pub fn parse_entry(entry: &Value) -> Option<LocationUpdate> {
    // 1. Match on the Value enum to find the Array (VecDeque in Fred)
    let entry_parts = match entry {
        Value::Array(arr) => arr,
        _ => return None,
    };

    // 2. Redis Stream structure: [ID, [Field, Value]]
    // entry_parts[1] is the fields array
    let fields_array = match entry_parts.get(1) {
        Some(Value::Array(fields)) => fields,
        _ => return None,
    };

    // 3. fields_array[1] is the actual JSON string
    let json_payload = match fields_array.get(1) {
        Some(Value::String(s)) => s,
        _ => return None,
    };

    // 4. Finally, parse the JSON into your struct
    serde_json::from_str::<LocationUpdate>(&json_payload).ok()
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
