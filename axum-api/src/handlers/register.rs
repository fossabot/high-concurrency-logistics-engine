use crate::models::user::{CreateUser, User};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};
use axum::{extract::State, http::StatusCode, Json};
use sqlx::{self};
use crate::bus::redis_bus;
use std::sync::Arc;
use crate::models::state::AppState;
use crate::models::error::SyncError;
use crate::bus::email_bus;

pub async fn register_handler(
    State(state): State<Arc<AppState>>,
    Json(cre): Json<CreateUser>,
) -> Result<StatusCode, SyncError> {
    if cre.email.is_empty() || cre.name.is_empty() {
        return Ok(StatusCode::BAD_REQUEST);
    }
    let existing_user: Option<User> =
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(&cre.email)
            .fetch_optional(&state.pool)
            .await?;
    if existing_user.is_some() {
        return Ok(StatusCode::CONFLICT);
    }
    let argon2 = Argon2::default();
    let salt = SaltString::generate(&mut OsRng);
    let hashed_password = argon2
        .hash_password(cre.password.as_bytes(), &salt)
        .expect("Failed to hash password")
        .to_string();
    let user = User::new(&cre, hashed_password);
    let email = user.email.clone();
    let otp: u32 = rand::random_range(100_000..999_999);
    redis_bus::publish_otp(&otp, &user, &state).await?;
    match email_bus::send_verification_email(state, email, &otp).await {
        Ok(_) => {
            Ok(StatusCode::OK)
        }
        Err(e) => {
            println!("Could not send email: {e}");
            return Err(SyncError::Other(e.to_string()));
        }
    }
}
