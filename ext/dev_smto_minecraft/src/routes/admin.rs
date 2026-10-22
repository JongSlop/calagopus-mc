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
    /// Panel-wide switch for the mod auto update task.
    pub auto_updates_enabled: bool,
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

    ApiResponse::new_serialized(settings(&state).await?).ok()
}

#[derive(ToSchema, Deserialize)]
pub struct UpdateSettingsRequest {
    /// Omitted leaves the stored key alone, an empty string removes it.
    pub curseforge_api_key: Option<String>,
    /// Omitted leaves the switch alone.
    pub auto_updates_enabled: Option<bool>,
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

    if let Some(key) = data.curseforge_api_key {
        crate::settings::set_curseforge_key(&state, Some(&key)).await?;
    }

    if let Some(enabled) = data.auto_updates_enabled {
        crate::settings::set_auto_updates_enabled(&state, enabled).await?;
    }

    ApiResponse::new_serialized(settings(&state).await?).ok()
}

async fn settings(state: &shared::State) -> anyhow::Result<SettingsResponse> {
    Ok(SettingsResponse {
        curseforge_configured: crate::settings::curseforge_key(state).await?.is_some(),
        auto_updates_enabled: crate::settings::auto_updates_enabled(state).await?,
    })
}

pub fn router(state: &shared::State) -> OpenApiRouter<shared::State> {
    OpenApiRouter::new()
        .routes(routes!(get_settings))
        .routes(routes!(update_settings))
        .with_state(state.clone())
}
