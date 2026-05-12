use axum_api::middlewares::auth::decode_jwt_token;
use axum_api::models::{error::AuthError, login_user::Claims, user::UserRole};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use time::{Duration, OffsetDateTime};

#[tokio::test]
async fn test_jwt_token() {
    dotenvy::dotenv().ok();
    let jwt_private_key =
        std::env::var("JWT_PRIVATE_KEY").expect("JWT_PRIVATE_KEY must be set in .env");
    let jwt_public_key =
        std::env::var("JWT_PUBLIC_KEY").expect("JWT_PRIVATE_KEY must be set in .env");
    let priv_bytes = hex::decode(&jwt_private_key).expect("invalid hex");
    let pub_bytes = hex::decode(&jwt_public_key).expect("invalid hex");
    let jwt_encoding_key = EncodingKey::from_ed_der(&priv_bytes);
    let jwt_decoding_key = DecodingKey::from_ed_der(&pub_bytes);
    let user_id = "abcd 568a djfn";
    let exp = OffsetDateTime::now_utc() + Duration::seconds(86400);
    let claims = Claims {
        sub: user_id.to_string(),
        role: UserRole::Driver,
        exp: exp.unix_timestamp() as u64,
        aud: "parcel-api".to_string(), //for restricting token usage to this API
    };
    let token = encode(&Header::new(Algorithm::EdDSA), &claims, &jwt_encoding_key)
        .map(|data| data.to_string())
        .map_err(|e| {
            tracing::error!("Failed to encode JWT token: {}", e);
            AuthError::InternalServerError
        });
    if let Ok(token_string) = token {
        let data = decode_jwt_token(&token_string, &jwt_decoding_key)
            .expect("token should decode successfully");
        assert_eq!(data, claims);
        tracing::info!("JWT is working properly")
    }
}

#[tokio::test]
async fn test_jwt_rejects_tampered_token() {
    dotenvy::dotenv().ok();
    let jwt_public_key = std::env::var("JWT_PUBLIC_KEY").expect("JWT_PUBLIC_KEY must be set");
    let pub_bytes = hex::decode(&jwt_public_key).expect("invalid hex");
    let jwt_decoding_key = DecodingKey::from_ed_der(&pub_bytes);

    let tampered_token = "eyJhbGciOiJFZERTQSJ9.tampered.signature";

    let result = decode_jwt_token(tampered_token, &jwt_decoding_key);
    assert!(result.is_err());
}
