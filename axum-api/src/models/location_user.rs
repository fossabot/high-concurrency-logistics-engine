use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgTypeInfo, Postgres};
use sqlx::{Decode, FromRow, Type};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, FromRow)]
pub struct LocationUpdate {
    pub parcel_id: String,
    pub driver_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timestamp: i64,
    pub status: DriverStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DriverStatus {
    Unknown,
    PickedUp,
    InTransit,
    DroppedOff,
    NotAvailable,
    Nearby,
}

impl Type<Postgres> for DriverStatus {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("status") // Matches your DB: CREATE TYPE display_name_style ...
    }
}

impl<'r> Decode<'r, Postgres> for DriverStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let s = <&str as Decode<'r, Postgres>>::decode(value)?;
        match s {
            "unknown" => Ok(DriverStatus::Unknown),
            "picked_up" => Ok(DriverStatus::PickedUp),
            "in_transit" => Ok(DriverStatus::InTransit),
            "dropped_off" => Ok(DriverStatus::DroppedOff),
            "not_available" => Ok(DriverStatus::NotAvailable),
            "nearby" => Ok(DriverStatus::Nearby),
            _ => Err(format!("Unknown display_name_style variant: {s}").into()),
        }
    }
}
#[derive(serde::Deserialize)]
pub struct ConnectParams {
    pub parcel_id: String,
    pub role: String, // "driver" | "customer"
}
