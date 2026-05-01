use crate::models::user::UserRole;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Deserialize, Debug, Clone)]
pub struct LoginUser {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Debug, Clone, FromRow, Serialize, PartialEq)]
pub struct Claims {
    pub sub: String,
    pub role: UserRole,
    pub exp: u64,
    pub aud: String,
}
