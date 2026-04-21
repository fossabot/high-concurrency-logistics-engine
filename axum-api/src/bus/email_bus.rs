use lettre::{Message, AsyncTransport}; // AsyncTransport trait is required for .send()
use std::sync::Arc;
use crate::models::error::SyncError;
use crate::models::state::AppState;
use axum::http::StatusCode;


pub async fn send_verification_email(state: Arc<AppState>, email: String, otp: &u32) -> Result<StatusCode, SyncError> {
    let smtp_username = std::env::var("SMTP_USERNAME").expect("SMTP_USERNAME must be set in .env");
    // 1. Build the message

    let email = Message::builder()
        .from(smtp_username.parse().unwrap())
        .to(email.parse().unwrap())
        .subject("Verification otp")
        .body(format!("Your verification otp is: {otp}"))
        .expect("Failed to build email");

    // 2. Send it using the mailer in your AppState
    if let Err(e) = state.mailer.send(email).await {
        println!("Could not send email: {e}");
        return Err(SyncError::Other(e.to_string()));
    };
    Ok(StatusCode::OK)
}
