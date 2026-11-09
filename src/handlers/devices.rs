use axum::{
    Json,
    extract::{Path, State},
};

use crate::{
    auth::AuthUser,
    error::{AppResult, ErrorBody},
    services::device::{DeviceResponse, DeviceResult, RegisterDeviceRequest, VoipTokenRequest},
    state::AppState,
};

/// Register (or re-assign) a mobile device and its push tokens.
#[utoipa::path(
    post, path = "/v1/devices/register", tag = "Devices",
    request_body = RegisterDeviceRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Device registered successfully", body = DeviceResult),
        (status = 400, description = "Invalid platform", body = ErrorBody),
    )
)]
pub async fn register(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<RegisterDeviceRequest>,
) -> AppResult<Json<DeviceResult>> {
    Ok(Json(s.device_service.register(au.user.id, req).await?))
}

/// Update the VoIP push token of one of the caller's devices.
#[utoipa::path(
    post, path = "/v1/devices/voip-token", tag = "Devices",
    request_body = VoipTokenRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "VoIP token updated successfully", body = DeviceResult),
        (status = 404, description = "Device not found", body = ErrorBody),
    )
)]
pub async fn voip_token(
    State(s): State<AppState>,
    au: AuthUser,
    Json(req): Json<VoipTokenRequest>,
) -> AppResult<Json<DeviceResult>> {
    Ok(Json(s.device_service.set_voip_token(au.user.id, req).await?))
}

/// Deactivate a device and clear its push tokens.
#[utoipa::path(
    delete, path = "/v1/devices/{device_id}", tag = "Devices",
    params(("device_id" = String, Path, description = "Client-generated device id")),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Device unregistered successfully", body = DeviceResult),
        (status = 404, description = "Device not found", body = ErrorBody),
    )
)]
pub async fn unregister(
    State(s): State<AppState>,
    au: AuthUser,
    Path(device_id): Path<String>,
) -> AppResult<Json<DeviceResult>> {
    Ok(Json(s.device_service.unregister(au.user.id, &device_id).await?))
}

/// The caller's active devices.
#[utoipa::path(
    get, path = "/v1/devices", tag = "Devices",
    security(("bearer" = [])),
    responses((status = 200, description = "List of active user devices", body = Vec<DeviceResponse>))
)]
pub async fn list(State(s): State<AppState>, au: AuthUser) -> AppResult<Json<Vec<DeviceResponse>>> {
    Ok(Json(s.device_service.list(au.user.id).await?))
}
