use axum::{http::Request, http::{header,StatusCode}, middleware::Next, response::Response};
use std::sync::Arc;
use crate::AppState;
use axum::extract::State;
use jsonwebtoken::{ Validation, Algorithm, decode};
use crate::models::login_user::Claims;

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(cookie_headers) = req.headers()
        .get(header::COOKIE)
        .and_then(|h| h.to_str().ok())
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let auth_header = req.headers().get(header::AUTHORIZATION).and_then(|h| h.to_str().ok());

    let token = auth_header             // Extract token from Authorization header first, then from cookie
        .and_then(|h| {
            let mut parts = h.trim().splitn(2, ' ');
            let scheme = parts.next()?;
            if scheme != "Bearer" { return None; }

            Some(parts.next()?)
        })
        .or_else(|| {
            cookie_headers
                .split(';')
                .find_map(|c| {
                    let mut parts = c.trim().splitn(2,'=');
                    let name = parts.next()?;
                    let value = parts.next()?;
                    if name == "token" { Some(value) } else { None }
                })
        })
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_audience(&["parcel-api"]);           // Restrict token usage to this API
    let _ = decode::<Claims>(&token, &state.jwt_decoding_key, &validation).map_err(|e| {
        tracing::warn!("token decode error: {:?}", e);
        StatusCode::UNAUTHORIZED
    })?;


    Ok(next.run(req).await)
}
