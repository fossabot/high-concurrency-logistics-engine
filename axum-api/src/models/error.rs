use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    #[error("Redis failure: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Database failure: {0}")]
    Postgres(#[from] sqlx::Error),

    #[error("JSON failure: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl IntoResponse for SyncError {
    fn into_response(self) -> Response {
        // Log the actual error for the server admin
        tracing::error!("Sync Error: {:?}", self);

        // Map your internal errors to external HTTP statuses
        let (status, error_message) = match self {
            SyncError::Postgres(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            SyncError::Redis(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            SyncError::Json(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            SyncError::Other(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
        };

        // Return a JSON response or just a status code
        (status, error_message).into_response()
    }
}

#[derive(thiserror::Error, Debug)]                             //Never Used I am thinking so restricting Users Not implimented
pub enum AuthError {
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Internal server error")]
    InternalServerError,
    #[error("IO error")]
    IoError(#[from] std::io::Error),
    #[error("Token expired")]
    Expired,
    #[error("Missing token")]
    Missing,
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AuthError::InternalServerError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
            AuthError::IoError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error".to_string()),
            AuthError::Expired => (StatusCode::UNAUTHORIZED, "Token expired".to_string()),
            AuthError::Missing => (StatusCode::UNAUTHORIZED, "Missing token".to_string()),
        };
        (status, error_message).into_response()
    }
}
