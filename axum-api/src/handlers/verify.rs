use axum::{extract::State, http::StatusCode, Json};
use crate::bus::redis_bus;
use std::sync::Arc;
use crate::models::state::AppState;
use crate::models::error::SyncError;
use crate::models::user::VerifyUser;


pub async fn verify_handler(
    State(state): State<Arc<AppState>>,
    Json(ver): Json<VerifyUser>,
) -> Result<StatusCode, SyncError> {


    if ver.email.is_empty() {
        return Ok(StatusCode::CONFLICT);
    }
    match redis_bus::read_otp(&ver.otp, &ver.email, &state).await {
        Ok(_) => {
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            println!("Could not send email: {e}");
            return Err(SyncError::Other(e.to_string()));
        }
    }
}
