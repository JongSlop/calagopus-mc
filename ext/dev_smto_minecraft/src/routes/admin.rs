//! Management routes for the extension itself, mounted under `/api/admin`.

use serde::{Deserialize, Serialize};
use shared::{
    ApiError, GetState,
    models::user::GetPermissionManager,
    response::{ApiResponse, ApiResponseResult},
};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(ToSchema, Serialize)]
pub struct SettingsResponse {
    pub curseforge_configured: bool,
}

#[utoipa::path(get, path = "/settings", responses(
    (status = OK, body = inline(SettingsResponse)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
))]
pub async fn get_settings(
    state: GetState,
    permissions: GetPermissionManager,
) -> ApiResponseResult {
    permissions.has_admin_permission("extensions.read")?;

    let configured = crate::settings::curseforge_key(&state).await?.is_some();

    ApiResponse::new_serialized(SettingsResponse {
        curseforge_configured: configured,
    })
    .ok()
}

#[derive(ToSchema, Deserialize)]
pub struct UpdateSettingsRequest {
    /// `None` or an empty string removes the key.
    pub curseforge_api_key: Option<String>,
}

#[utoipa::path(put, path = "/settings", request_body = UpdateSettingsRequest, responses(
    (status = OK, body = inline(SettingsResponse)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
))]
pub async fn update_settings(
    state: GetState,
    permissions: GetPermissionManager,
    axum::Json(data): axum::Json<UpdateSettingsRequest>,
) -> ApiResponseResult {
    permissions.has_admin_permission("extensions.manage")?;

    crate::settings::set_curseforge_key(&state, data.curseforge_api_key.as_deref()).await?;

    ApiResponse::new_serialized(SettingsResponse {
        curseforge_configured: crate::settings::curseforge_key(&state).await?.is_some(),
    })
    .ok()
}

pub fn router(state: &shared::State) -> OpenApiRouter<shared::State> {
    OpenApiRouter::new()
        .routes(routes!(get_settings))
        .routes(routes!(update_settings))
        .with_state(state.clone())
}
