use serde::{Deserialize, Serialize};
use shared::{
    ApiError, GetState,
    models::{
        server::{GetServer, GetServerActivityLogger},
        user::GetPermissionManager,
    },
    response::{ApiResponse, ApiResponseResult},
};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{properties::Properties, server_file};

const FILE: &str = "server.properties";

#[derive(ToSchema, Serialize)]
pub struct PropertyEntry {
    pub key: String,
    pub value: String,
}

#[derive(ToSchema, Serialize)]
pub struct PropertiesResponse {
    /// Current file contents, verbatim.
    pub content: String,
    /// Parsed entries, in file order.
    pub entries: Vec<PropertyEntry>,
}

impl PropertiesResponse {
    fn new(content: String) -> Self {
        let entries = Properties::parse(&content)
            .entries()
            .into_iter()
            .map(|entry| PropertyEntry {
                key: entry.key,
                value: entry.value,
            })
            .collect();

        Self { content, entries }
    }
}

mod get {
    use super::*;

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(PropertiesResponse)),
        (status = UNAUTHORIZED, body = ApiError),
        (status = FORBIDDEN, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
    ), params(
        (
            "server" = uuid::Uuid,
            description = "The server ID",
            example = "123e4567-e89b-12d3-a456-426614174000",
        ),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        server: GetServer,
    ) -> ApiResponseResult {
        permissions.has_server_permission("minecraft-properties.read")?;

        let content = server_file::read(&state, &server, FILE).await?;

        ApiResponse::new_serialized(PropertiesResponse::new(content)).ok()
    }
}

#[derive(ToSchema, Deserialize)]
pub struct PropertyEdit {
    pub key: String,
    /// `None` removes the key.
    pub value: Option<String>,
}

#[derive(ToSchema, Deserialize)]
pub struct UpdatePropertiesRequest {
    pub edits: Vec<PropertyEdit>,
}

mod put {
    use super::*;

    #[utoipa::path(put, path = "/", request_body = UpdatePropertiesRequest, responses(
        (status = OK, body = inline(PropertiesResponse)),
        (status = UNAUTHORIZED, body = ApiError),
        (status = FORBIDDEN, body = ApiError),
        (status = BAD_REQUEST, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
    ), params(
        (
            "server" = uuid::Uuid,
            description = "The server ID",
            example = "123e4567-e89b-12d3-a456-426614174000",
        ),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        server: GetServer,
        activity_logger: GetServerActivityLogger,
        axum::Json(data): axum::Json<UpdatePropertiesRequest>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("minecraft-properties.update")?;

        for edit in &data.edits {
            if edit.key.trim().is_empty() || edit.key.contains('\n') {
                return ApiResponse::error("invalid property key").ok();
            }

            if let Some(value) = &edit.value
                && value.contains('\n')
            {
                return ApiResponse::error("property values cannot contain newlines").ok();
            }
        }

        let content = server_file::read(&state, &server, FILE).await?;
        let mut properties = Properties::parse(&content);

        let mut changed = Vec::new();
        for edit in &data.edits {
            let did_change = match &edit.value {
                Some(value) => properties.set(&edit.key, value),
                None => properties.remove(&edit.key),
            };

            if did_change {
                changed.push(edit.key.clone());
            }
        }

        if changed.is_empty() {
            let content = properties.render();
            return ApiResponse::new_serialized(PropertiesResponse::new(content)).ok();
        }

        let rendered = properties.render();
        server_file::write(&state, &server, FILE, rendered.clone()).await?;

        activity_logger
            .log(
                "server:dev.smto.minecraft.properties.update",
                serde_json::json!({
                    "file": FILE,
                    "changed": changed,
                }),
            )
            .await;

        ApiResponse::new_serialized(PropertiesResponse::new(rendered)).ok()
    }
}

pub fn router(state: &shared::State) -> OpenApiRouter<shared::State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(put::route))
        .with_state(state.clone())
}
