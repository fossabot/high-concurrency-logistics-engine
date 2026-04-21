
use axum::{
    extract::{State, Json},
    http::StatusCode,
};
use serde::Deserialize;

use crate::models::parcel::Parcel;
use crate::models::{state::AppState, error::SyncError};


pub async fn handle_parcel(State(state): State<AppState>, Json(parcel): Json<Parcel>) -> Result<(), SyncError>{
    let result = parcel.0.validate();
    if let Err(err) = result {
        return Err(SyncError::Other(StatusCode::BAD_REQUEST, err.to_string()));
    }

    Ok(())
}
