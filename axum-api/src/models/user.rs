use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub created_at: OffsetDateTime,
    pub password: String,
    pub role: UserRole,
    #[sqlx(flatten)]
    pub driver_profile: DriverProfile,
    #[sqlx(flatten)]
    pub customer_profile: CustomerProfile,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase", type_name = "text")]
pub enum UserRole {
    Admin,
    Driver,
    Customer,
}

impl UserRole {
    pub fn to_string(&self) -> String {
        match self {
            UserRole::Admin => "admin".to_string(),
            UserRole::Driver => "driver".to_string(),
            UserRole::Customer => "customer".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "lowercase")]
pub struct DriverProfile {
    pub is_available: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, FromRow)]
#[serde(rename_all = "lowercase")]
pub struct CustomerProfile {
    pub default_address: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CreateUser {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: UserRole,
}

#[derive(Deserialize, Debug, Clone)]
pub struct VerifyUser {
    pub email: String,
    pub otp: u32,
}

impl User {
    pub fn new(cre: &CreateUser, hash_password: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: cre.name.clone(),
            email: cre.email.clone(),
            created_at: OffsetDateTime::now_utc(),
            password: hash_password,
            role: cre.role.clone(),
            driver_profile: DriverProfile { is_available: Some(false) },
            customer_profile: CustomerProfile { default_address: None },
        }
    }
}
