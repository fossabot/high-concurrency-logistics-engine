use crate::models::login_user::Claims;
use crate::models::state::AppState;
use axum::extract::State;
use axum::{
    http::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use metrics;
use std::sync::Arc;

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {

    let start = std::time::Instant::now();

    let Some(cookie_headers) = req
        .headers()
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = auth_header // Extract token from Authorization header first, then from cookie
        .and_then(|h| {
            let mut parts = h.trim().splitn(2, ' ');
            let scheme = parts.next()?;
            if scheme != "Bearer" {
                return None;
            }

            Some(parts.next()?)
        })
        .or_else(|| {
            cookie_headers.split(';').find_map(|c| {
                let mut parts = c.trim().splitn(2, '=');
                let name = parts.next()?;
                let value = parts.next()?;
                if name == "token" {
                    Some(value)
                } else {
                    None
                }
            })
        })
        .ok_or(StatusCode::UNAUTHORIZED)?;



    if let Err(e) = decode_jwt_token(&token, &state.jwt_decoding_key) {
        return Err(e);
    }
    // In Grafana, you'll see this in microseconds/milliseconds
    metrics::histogram!("jwt_sign_duration_seconds").record(start.elapsed().as_secs_f64());

    Ok(next.run(req).await)
}

pub fn decode_jwt_token(token: &str, jwt_decoding_key: &DecodingKey) -> Result<Claims, StatusCode> {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&["parcel-api"]);
    decode::<Claims>(&token, &jwt_decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|e| {
            tracing::warn!("token decode error: {:?}", e);
            StatusCode::UNAUTHORIZED
        })
}
