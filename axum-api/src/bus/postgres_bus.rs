use crate::models::{error::SyncError, location_user::LocationUpdate, state::AppState};

pub async fn read_delivery_time(
    state: &AppState,
    parcel_id: &str,
) -> Result<Vec<LocationUpdate>, SyncError> {
    let parcel_history: Vec<LocationUpdate> = sqlx::query_as::<_, LocationUpdate>(
        r#"
        (SELECT * FROM parcel_history WHERE parcel_id = $1 ORDER BY TIMESTAMP ASC LIMIT 1)
        UNION ALL
        (SELECT * FROM parcel_history WHERE parcel_id = $2 ORDER BY TIMESTAMP DESC LIMIT 1)
        "#,
    )
    .bind(parcel_id)
    .bind(parcel_id)
    .fetch_all(&state.pool)
    .await?;
    match parcel_history.len() {
        2 => Ok(parcel_history),
        1 => Err(SyncError::Other("Error".to_string())),
        _ => Err(SyncError::Other("Error".to_string())),
    }
}
