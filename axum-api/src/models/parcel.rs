use crate::models::location_user::DriverStatus;

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Parcel {
    //This struct represents a parcel in the system when Customer books and sends it to the database
    pub id: String,
    pub sender: String,
    pub recipient: String,
    pub weight: f64,
    pub status: DriverStatus,
    pub created_at: String,
    pub dest_lat: f64,
    pub dest_lon: f64,
    pub from_lat: f64,
    pub from_lon: f64,
}

impl Parcel {
    pub fn validate(&self) -> bool {
        if self.id.is_empty()
            || self.sender.is_empty()
            || self.recipient.is_empty()
            || self.weight <= 0.0
            || self.status == DriverStatus::Unknown
            || self.created_at.is_empty()
            || self.dest_lat.is_nan()
            || self.dest_lon.is_nan()
            || self.from_lat.is_nan()
            || self.from_lon.is_nan()
        {
            return false;
        }
        true
    }
}
