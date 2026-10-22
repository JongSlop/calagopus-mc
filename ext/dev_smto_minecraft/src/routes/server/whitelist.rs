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

use crate::{properties::Properties, server_file, whitelist};

const FILE: &str = "whitelist.json";
const PROPERTIES_FILE: &str = "server.properties";

#[derive(ToSchema, serde::Serialize, serde::Deserialize, Clone)]
pub struct WhitelistEntry {
    pub name: String,
    /// Resolved by the backend when missing.
    pub uuid: Option<String>,
}

#[derive(ToSchema, serde::Serialize)]
pub struct WhitelistResponse {
    pub entries: Vec<WhitelistEntry>,
    /// `white-list` from server.properties.
    pub enforced: bool,
    pub online_mode: bool,
}

async fn read_properties(state: &shared::State, server: &GetServer) -> anyhow::Result<Properties> {
    let content = server_file::read(state, server, PROPERTIES_FILE).await?;
    Ok(Properties::parse(&content))
}

async fn load(
    state: &shared::State,
    server: &GetServer,
) -> anyhow::Result<(Vec<WhitelistEntry>, bool, bool)> {
    let properties = read_properties(state, server).await?;
    let enforced = properties.get("white-list") == Some("true");
    let online_mode = properties.get("online-mode") != Some("false");

    let content = server_file::read_optional(state, server, FILE).await?;
    let entries = content
        .as_deref()
        .map(|content| whitelist::parse(content).unwrap_or_default())
        .unwrap_or_default()
        .into_iter()
        .map(|entry| WhitelistEntry {
            name: entry.name,
            uuid: Some(entry.uuid),
        })
        .collect();

    Ok((entries, enforced, online_mode))
}

mod get {
    use super::*;

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(WhitelistResponse)),
        (status = UNAUTHORIZED, body = ApiError),
        (status = FORBIDDEN, body = ApiError),
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
        permissions.has_server_permission("minecraft-whitelist.read")?;

        let (entries, enforced, online_mode) = load(&state, &server).await?;

        ApiResponse::new_serialized(WhitelistResponse {
            entries,
            enforced,
            online_mode,
        })
        .ok()
    }
}

#[derive(ToSchema, serde::Deserialize)]
pub struct UpdateWhitelistRequest {
    pub entries: Vec<WhitelistEntry>,
}

mod put {
    use super::*;

    #[utoipa::path(put, path = "/", request_body = UpdateWhitelistRequest, responses(
        (status = OK, body = inline(WhitelistResponse)),
        (status = UNAUTHORIZED, body = ApiError),
        (status = FORBIDDEN, body = ApiError),
        (status = BAD_REQUEST, body = ApiError),
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
        axum::Json(data): axum::Json<UpdateWhitelistRequest>,
    ) -> ApiResponseResult {
        permissions.has_server_permission("minecraft-whitelist.update")?;

        let properties = read_properties(&state, &server).await?;
        let online_mode = properties.get("online-mode") != Some("false");
        let enforced = properties.get("white-list") == Some("true");

        let mut entries: Vec<whitelist::Entry> = Vec::with_capacity(data.entries.len());
        let mut seen = std::collections::HashSet::new();
        let mut added = Vec::new();

        for entry in &data.entries {
            let name = entry.name.trim().to_string();

            if !seen.insert(name.to_lowercase()) {
                return ApiResponse::error(format!("'{name}' is listed twice")).ok();
            }

            if let Err(message) = whitelist::validate_name(&name) {
                return ApiResponse::error(message).ok();
            }

            let uuid = match &entry.uuid {
                Some(uuid) if !uuid.is_empty() => uuid.to_lowercase(),
                _ if online_mode => match resolve_online_uuid(&name).await {
                    Ok(uuid) => uuid,
                    Err(message) => return ApiResponse::error(message).ok(),
                },
                _ => whitelist::offline_uuid(&name),
            };

            added.push(name.clone());
            entries.push(whitelist::Entry { uuid, name });
        }

        server_file::write(&state, &server, FILE, whitelist::render(&entries)).await?;
        server_file::reload_whitelist(&state, &server).await?;

        activity_logger
            .log(
                "server:dev.smto.minecraft.whitelist.update",
                serde_json::json!({
                    "file": FILE,
                    "players": added,
                }),
            )
            .await;

        let (entries, _, _) = load(&state, &server).await?;

        ApiResponse::new_serialized(WhitelistResponse {
            entries,
            enforced,
            online_mode,
        })
        .ok()
    }

    async fn resolve_online_uuid(name: &str) -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct Profile {
            id: String,
        }

        let response = reqwest::Client::new()
            .get(format!(
                "https://api.mojang.com/users/profiles/minecraft/{name}"
            ))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|err| format!("Mojang lookup for '{name}' failed: {err}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "'{name}' does not exist on Mojang (the server runs in online mode, so the account must exist)"
            ));
        }

        let profile: Profile = response
            .json()
            .await
            .map_err(|err| format!("unexpected Mojang response for '{name}': {err}"))?;

        // Mojang returns the UUID without hyphens.
        if profile.id.len() != 32 {
            return Err(format!("Mojang returned an unexpected UUID for '{name}'"));
        }

        Ok(format!(
            "{}-{}-{}-{}-{}",
            &profile.id[0..8],
            &profile.id[8..12],
            &profile.id[12..16],
            &profile.id[16..20],
            &profile.id[20..32]
        ))
    }
}

pub fn router(state: &shared::State) -> OpenApiRouter<shared::State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(put::route))
        .with_state(state.clone())
}
