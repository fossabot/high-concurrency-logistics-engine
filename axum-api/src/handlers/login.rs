use crate::models::{login_user::LoginUser, user::User, error::AuthError};
use argon2::{Argon2, PasswordVerifier};
use axum::{
    extract::State, http::header::SET_COOKIE, http::StatusCode, response::IntoResponse, Json,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use time::{Duration, OffsetDateTime};
use std::sync::Arc;
use crate::AppState;
use crate::models::login_user::Claims;



fn create_token(user: User, encoding_key: &EncodingKey) -> Result<impl IntoResponse, AuthError> {
    let exp = OffsetDateTime::now_utc() + Duration::seconds(86400);
    let claims = Claims {
        sub: user.id.to_string(),
        role: user.role,
        exp: exp.unix_timestamp() as u64,
        aud: "parcel-api".to_string(), //for restricting token usage to this API
    };
    let token = encode(
        &Header::default(),
        &claims,
        encoding_key,
    )
    .map_err(|e| {
        tracing::error!("Failed to encode JWT token: {}", e);
        AuthError::InternalServerError
    })?;
    let cookie = format!("token={}; HttpOnly; Secure; SameSite=Strict; Path=/", token);
    Ok(([(SET_COOKIE, cookie)], "Authenticated").into_response())
}

pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(login_user): Json<LoginUser>,
) -> impl IntoResponse {
    if login_user.email.is_empty() || login_user.password.is_empty() {
        return (StatusCode::BAD_REQUEST, "Bad Request").into_response();
    }

    let user = sqlx::query_as::<_, User>(
        "SELECT id, name, email, created_at, password, role, customer_profile AS TEXT, driver_profile AS TEXT FROM users WHERE email = $1",
    )
    .bind(login_user.email)
    .fetch_one(&state.pool) // Use fetch_optional if the user might not exist
    .await;
    match user {
        Ok(user) => {
            let user_clone = user.clone();
            match tokio::task::spawn_blocking(move || {
                let parsed_password = argon2::password_hash::PasswordHash::new(&user.password)
                    .expect("Failed to parse password hash");
                Argon2::default()
                    .verify_password(login_user.password.as_bytes(), &parsed_password)
            })
            .await
            {
                Ok(_) => {
                     match create_token(user_clone, &state.jwt_encoding_key) {
                        Ok(response) => response.into_response(),
                        Err(error) => error.into_response(),
                    }
                }
                Err(_) =>  (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
            }
        }
        Err(_) =>  (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    }
}
