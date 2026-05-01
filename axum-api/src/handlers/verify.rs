use crate::bus::redis_bus;
use crate::models::error::SyncError;
use crate::models::state::AppState;
use crate::models::user::VerifyUser;
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

pub async fn verify_handler(
    State(state): State<Arc<AppState>>,
    Json(ver): Json<VerifyUser>,
) -> Result<StatusCode, SyncError> {
    if ver.email.is_empty() {
        return Ok(StatusCode::CONFLICT);
    }
    match redis_bus::read_otp(&ver.otp, &ver.email, &state).await {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(e) => {
            println!("Could not send email: {e}");
            return Err(SyncError::Other(e.to_string()));
        }
    }
}
