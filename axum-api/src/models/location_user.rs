use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LocationUpdate {
    pub parcel_id: String,
    pub driver_id: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timestamp: u64,
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

#[derive(serde::Deserialize)]
pub struct ConnectParams {
    pub parcel_id: String,
    pub role: String, // "driver" | "customer"
}
