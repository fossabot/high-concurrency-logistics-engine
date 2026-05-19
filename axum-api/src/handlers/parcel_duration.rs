use crate::bus::postgres_bus::read_delivery_time;
use crate::models::{location_user::ConnectParams, state::AppState};
use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;

pub async fn handle_parcel_duration(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ConnectParams>,
) -> impl IntoResponse {
    if params.role != "customer" {
        return StatusCode::CONFLICT.into_response();
    }
    let parcel_id = params.parcel_id;
    match read_delivery_time(&state, &parcel_id).await {
        Ok(parcel_duration_history) => Json(parcel_duration_history).into_response(),
        Err(e) => e.into_response(),
    }
}
