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

use crate::{
    mods::{
        self, InstalledMod, Loader, MANIFEST_FILE, Manifest, Target, curseforge, manager,
        manager::Provider, modrinth,
    },
    server_file,
};

#[derive(ToSchema, Serialize)]
pub struct InstalledModResponse {
    pub provider: String,
    pub project_id: String,
    pub project_name: String,
    pub version_id: String,
    pub version_name: String,
    pub filename: String,
    pub target: Target,
    pub pinned: bool,
    pub sha512: Option<String>,
    pub updated: Option<String>,
}

impl From<&InstalledMod> for InstalledModResponse {
    fn from(entry: &InstalledMod) -> Self {
        Self {
            provider: entry.provider.clone(),
            project_id: entry.project_id.clone(),
            project_name: entry.project_name.clone(),
            version_id: entry.version_id.clone(),
            version_name: entry.version_name.clone(),
            filename: entry.filename.clone(),
            target: entry.target,
            pinned: entry.pinned,
            sha512: entry.sha512.clone(),
            updated: entry.updated.clone(),
        }
    }
}

#[derive(ToSchema, Serialize)]
pub struct DetectionResponse {
    pub loader: Option<Loader>,
    pub mc_version: Option<String>,
}

#[derive(ToSchema, Serialize)]
pub struct ModsResponse {
    pub detection: DetectionResponse,
    pub auto_update: bool,
    pub mods: Vec<InstalledModResponse>,
}

impl ModsResponse {
    fn new(manifest: &Manifest) -> Self {
        Self {
            detection: DetectionResponse {
                loader: manifest.detection.loader,
                mc_version: manifest.detection.mc_version.clone(),
            },
            auto_update: manifest.auto_update,
            mods: manifest.mods.iter().map(Into::into).collect(),
        }
    }
}

pub(crate) async fn read_manifest(
    state: &shared::State,
    server: &shared::models::server::Server,
) -> anyhow::Result<Manifest> {
    let content = server_file::read_optional(state, server, MANIFEST_FILE).await?;
    Ok(content
        .as_deref()
        .map(Manifest::parse)
        .unwrap_or_default())
}

pub(crate) async fn write_manifest(
    state: &shared::State,
    server: &shared::models::server::Server,
    manifest: &Manifest,
) -> anyhow::Result<()> {
    server_file::write(state, server, MANIFEST_FILE, manifest.render()).await
}

/// Fills in loader and version from the server directory when the manifest has none.
pub(crate) async fn detect_and_store(
    state: &shared::State,
    server: &shared::models::server::Server,
    manifest: &mut Manifest,
) -> anyhow::Result<bool> {
    if manifest.detection.loader.is_some() && manifest.detection.mc_version.is_some() {
        return Ok(false);
    }

    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let listing = api
        .get_servers_server_files_list(
            server.uuid,
            &wings_api::servers_server_files_list::get::Query {
                directory: Some("/".into()),
                per_page: Some(200),
                ..Default::default()
            },
        )
        .await?;

    let entries: Vec<mods::DirEntry> = listing
        .entries
        .iter()
        .map(|entry| mods::DirEntry {
            name: entry.name.to_string(),
            directory: entry.directory,
        })
        .collect();

    let detected = mods::detect(&entries);
    let mut changed = false;

    if manifest.detection.loader.is_none() && detected.loader.is_some() {
        manifest.detection.loader = detected.loader;
        changed = true;
    }
    if manifest.detection.mc_version.is_none() && detected.mc_version.is_some() {
        manifest.detection.mc_version = detected.mc_version;
        changed = true;
    }

    Ok(changed)
}

#[utoipa::path(get, path = "/", responses(
    (status = OK, body = inline(ModsResponse)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
))]
pub async fn list(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.read")?;

    let mut manifest = read_manifest(&state, &server.0).await?;
    if detect_and_store(&state, &server.0, &mut manifest).await? {
        write_manifest(&state, &server.0, &manifest).await?;
    }

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

#[derive(ToSchema, Deserialize)]
pub struct SetDetectionRequest {
    pub loader: Option<Loader>,
    pub mc_version: Option<String>,
}

#[utoipa::path(post, path = "/detection", request_body = SetDetectionRequest, responses(
    (status = OK, body = inline(ModsResponse)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
))]
pub async fn set_detection(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    axum::Json(data): axum::Json<SetDetectionRequest>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.update")?;

    let mut manifest = read_manifest(&state, &server.0).await?;
    manifest.detection.loader = data.loader;
    manifest.detection.mc_version = data.mc_version.filter(|version| !version.trim().is_empty());
    write_manifest(&state, &server.0, &manifest).await?;

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

#[derive(ToSchema, Serialize)]
pub struct SearchHitResponse {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub client_side: String,
    pub server_side: String,
    /// Suggested install directory.
    pub suggested_target: Target,
}

#[derive(ToSchema, Deserialize)]
pub struct SearchParams {
    pub query: Option<String>,
    pub loader: Option<Loader>,
    pub mc_version: Option<String>,
    pub provider: Option<Provider>,
}

#[utoipa::path(get, path = "/search", responses(
    (status = OK, body = inline(Vec<SearchHitResponse>)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
    (status = BAD_REQUEST, body = ApiError),
    (status = BAD_GATEWAY, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
))]
pub async fn search(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.read")?;

    let manifest = read_manifest(&state, &server.0).await?;
    let loader = params
        .loader
        .or(manifest.detection.loader)
        .map(Loader::as_str);
    let mc_version = params
        .mc_version
        .filter(|version| !version.trim().is_empty())
        .or(manifest.detection.mc_version.clone());

    let (Some(loader), Some(mc_version)) = (loader, mc_version) else {
        return ApiResponse::error(
            "loader and Minecraft version have to be known before searching mods",
        )
        .ok();
    };

    let hits = match params.provider.unwrap_or(Provider::Modrinth) {
        Provider::Modrinth => {
            let hits = modrinth::search(
                params.query.as_deref().unwrap_or(""),
                Some(loader),
                Some(&mc_version),
                20,
            )
            .await
            .map_err(|err| ApiResponse::error(err.to_string()))?;

            hits.into_iter()
                .map(|hit| {
                    // Mods with no server-side support can only go to AutoModpack.
                    let suggested_target = if hit.server_side == "unsupported" {
                        Target::Client
                    } else {
                        Target::Server
                    };

                    SearchHitResponse {
                        project_id: hit.project_id,
                        slug: hit.slug,
                        title: hit.title,
                        description: hit.description,
                        icon_url: hit.icon_url,
                        downloads: hit.downloads,
                        client_side: hit.client_side,
                        server_side: hit.server_side,
                        suggested_target,
                    }
                })
                .collect::<Vec<_>>()
        }
        Provider::CurseForge => {
            let api_key = crate::settings::curseforge_key(&state)
                .await
                .map_err(|err| ApiResponse::error(err.to_string()))?
                .ok_or_else(|| {
                    ApiResponse::error(
                        "no CurseForge API key configured, add one in the extension settings",
                    )
                })?;

            curseforge::search(
                &api_key,
                params.query.as_deref().unwrap_or(""),
                Some(loader),
                Some(&mc_version),
            )
            .await
            .map_err(|err| ApiResponse::error(err.to_string()))?
            .into_iter()
            .map(|hit| SearchHitResponse {
                project_id: hit.id.to_string(),
                slug: hit.slug,
                title: hit.name,
                description: hit.summary,
                icon_url: Some(hit.logo.map(|logo| logo.thumbnail_url).unwrap_or_default()),
                downloads: hit.downloads.total,
                // CurseForge does not report per-side support in search results.
                client_side: "unknown".to_string(),
                server_side: "unknown".to_string(),
                suggested_target: Target::Server,
            })
            .collect::<Vec<_>>()
        }
    };

    ApiResponse::new_serialized(hits).ok()
}

#[derive(ToSchema, Serialize)]
pub struct VersionResponse {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub dependencies: Vec<String>,
}

#[utoipa::path(get, path = "/versions/{project_id}", responses(
    (status = OK, body = inline(Vec<VersionResponse>)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
    (status = BAD_GATEWAY, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
    (
        "project_id" = String, Path,
        description = "Modrinth project ID or slug",
        example = "sodium",
    ),
))]
pub async fn versions(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    // The server uuid is part of the path too, so the extraction is a pair.
    axum::extract::Path((_server, project_id)): axum::extract::Path<(uuid::Uuid, String)>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.read")?;

    let manifest = read_manifest(&state, &server.0).await?;
    let loader = manifest.detection.loader.map(Loader::as_str);

    let versions = match params.provider.unwrap_or(Provider::Modrinth) {
        Provider::Modrinth => {
            let versions = modrinth::versions(
                &project_id,
                loader,
                manifest.detection.mc_version.as_deref(),
            )
            .await
            .map_err(|err| ApiResponse::error(err.to_string()))?;

            versions
                .into_iter()
                .take(30)
                .map(|version| {
                    let file = version.primary_file().cloned();

                    VersionResponse {
                        id: version.id,
                        name: version.name,
                        version_number: version.version_number,
                        game_versions: version.game_versions,
                        loaders: version.loaders,
                        filename: file.as_ref().map(|file| file.filename.clone()),
                        size: file.as_ref().map(|file| file.size),
                        dependencies: version
                            .dependencies
                            .iter()
                            .filter(|dependency| dependency.dependency_type == "required")
                            .filter_map(|dependency| dependency.project_id.clone())
                            .collect(),
                    }
                })
                .collect::<Vec<_>>()
        }
        Provider::CurseForge => {
            let api_key = crate::settings::curseforge_key(&state)
                .await
                .map_err(|err| ApiResponse::error(err.to_string()))?
                .ok_or_else(|| {
                    ApiResponse::error(
                        "no CurseForge API key configured, add one in the extension settings",
                    )
                })?;

            let mod_id: u32 = project_id
                .parse()
                .map_err(|_| ApiResponse::error("not a CurseForge mod id"))?;

            curseforge::files(
                &api_key,
                mod_id,
                loader,
                manifest.detection.mc_version.as_deref(),
            )
            .await
            .map_err(|err| ApiResponse::error(err.to_string()))?
            .into_iter()
            .map(|file| VersionResponse {
                id: file.id.to_string(),
                name: file.display_name.clone(),
                version_number: file.display_name,
                game_versions: Vec::new(),
                loaders: Vec::new(),
                filename: Some(file.file_name),
                size: Some(file.file_length),
                dependencies: file
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.is_required())
                    .map(|dependency| dependency.mod_id.to_string())
                    .collect(),
            })
            .collect::<Vec<_>>()
        }
    };

    ApiResponse::new_serialized(versions).ok()
}

#[derive(ToSchema, Deserialize)]
pub struct InstallRequest {
    /// Modrinth project id or slug, or the numeric CurseForge mod id.
    pub project_id: String,
    /// Version id (Modrinth) or file id (CurseForge). Omitted picks the newest compatible one.
    pub version_id: Option<String>,
    pub target: Target,
    #[serde(default)]
    pub provider: Option<Provider>,
}

#[utoipa::path(post, path = "/install", request_body = InstallRequest, responses(
    (status = OK, body = inline(ModsResponse)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
    (status = BAD_REQUEST, body = ApiError),
    (status = BAD_GATEWAY, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
))]
pub async fn install(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    activity_logger: GetServerActivityLogger,
    axum::Json(data): axum::Json<InstallRequest>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.update")?;

    let mut manifest = read_manifest(&state, &server.0).await?;
    let entry = manager::install_one(
        &state,
        &server,
        &mut manifest,
        data.provider.unwrap_or(Provider::Modrinth),
        &data.project_id,
        data.version_id.as_deref(),
        data.target,
    )
    .await
    .map_err(|err| ApiResponse::error(err.to_string()))?;

    write_manifest(&state, &server.0, &manifest).await?;

    activity_logger
        .log(
            "server:dev.smto.minecraft.mods.install",
            serde_json::json!({
                "project_id": data.project_id,
                "version_id": entry.version_id,
                "target": data.target.dir(),
                "file": entry.filename,
            }),
        )
        .await;

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

#[derive(ToSchema, Deserialize)]
pub struct UninstallRequest {
    pub project_id: String,
    pub target: Target,
}

#[utoipa::path(post, path = "/uninstall", request_body = UninstallRequest, responses(
    (status = OK, body = inline(ModsResponse)),
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
pub async fn uninstall(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    activity_logger: GetServerActivityLogger,
    axum::Json(data): axum::Json<UninstallRequest>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.update")?;

    let mut manifest = read_manifest(&state, &server.0).await?;
    let entry = manifest
        .remove(&data.project_id, data.target)
        .ok_or_else(|| ApiResponse::error("this mod is not managed by the extension"))?;

    server_file::delete(
        &state,
        &server.0,
        format!("{}/{}", data.target.dir(), entry.filename),
    )
    .await?;

    write_manifest(&state, &server.0, &manifest).await?;

    activity_logger
        .log(
            "server:dev.smto.minecraft.mods.uninstall",
            serde_json::json!({
                "project_id": data.project_id,
                "file": entry.filename,
            }),
        )
        .await;

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

#[derive(ToSchema, Serialize)]
pub struct UpdateAvailableResponse {
    pub project_id: String,
    pub target: Target,
    pub current: String,
    pub latest: String,
    pub version_id: String,
}

#[utoipa::path(get, path = "/updates", responses(
    (status = OK, body = inline(Vec<UpdateAvailableResponse>)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
))]
pub async fn updates(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.read")?;

    let manifest = read_manifest(&state, &server.0).await?;
    let available = manager::check_updates(&state, &server.0, &manifest).await;

    ApiResponse::new_serialized(
        available
            .into_iter()
            .map(|entry| UpdateAvailableResponse {
                project_id: entry.project_id,
                target: entry.target,
                current: entry.current,
                latest: entry.latest,
                version_id: entry.version_id,
            })
            .collect::<Vec<_>>(),
    )
    .ok()
}

#[derive(ToSchema, Deserialize)]
pub struct PinRequest {
    pub project_id: String,
    pub target: Target,
    pub pinned: bool,
}

#[utoipa::path(post, path = "/pin", request_body = PinRequest, responses(
    (status = OK, body = inline(ModsResponse)),
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
pub async fn pin(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    axum::Json(data): axum::Json<PinRequest>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.update")?;

    let mut manifest = read_manifest(&state, &server.0).await?;

    let Some(entry) = manifest
        .mods
        .iter_mut()
        .find(|entry| entry.project_id == data.project_id && entry.target == data.target)
    else {
        return ApiResponse::error("this mod is not managed by the extension").ok();
    };

    entry.pinned = data.pinned;
    write_manifest(&state, &server.0, &manifest).await?;

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

#[derive(ToSchema, Deserialize)]
pub struct UpdateRequest {
    /// Omitted updates every unpinned mod.
    pub project_id: Option<String>,
    pub target: Option<Target>,
}

#[derive(ToSchema, Serialize)]
pub struct UpdateResponse {
    pub updated: Vec<UpdateEntryResponse>,
    pub mods: ModsResponse,
}

#[derive(ToSchema, Serialize)]
pub struct UpdateEntryResponse {
    pub project_id: String,
    pub target: Target,
    pub from: String,
    pub to: String,
}

#[utoipa::path(post, path = "/update", request_body = UpdateRequest, responses(
    (status = OK, body = inline(UpdateResponse)),
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
pub async fn update(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    activity_logger: GetServerActivityLogger,
    axum::Json(data): axum::Json<UpdateRequest>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.update")?;

    let mut manifest = read_manifest(&state, &server.0).await?;

    let reports = match (&data.project_id, data.target) {
        (Some(project_id), Some(target)) => {
            match manager::update_one(&state, &server, &mut manifest, project_id, target).await {
                Ok(Some(report)) => vec![report],
                Ok(None) => Vec::new(),
                Err(err) => return ApiResponse::error(err.to_string()).ok(),
            }
        }
        (None, _) => {
            manager::update_all(&state, &server, &mut manifest)
                .await
                .map_err(|err| ApiResponse::error(err.to_string()))?
        }
        (Some(_), None) => {
            return ApiResponse::error("target is required when updating a single mod").ok();
        }
    };

    if !reports.is_empty() {
        write_manifest(&state, &server.0, &manifest).await?;

        activity_logger
            .log(
                "server:dev.smto.minecraft.mods.update",
                serde_json::json!({
                    "mods": reports
                        .iter()
                        .map(|report| format!("{} {} -> {}", report.project_id, report.from, report.to))
                        .collect::<Vec<_>>(),
                }),
            )
            .await;
    }

    ApiResponse::new_serialized(UpdateResponse {
        updated: reports
            .into_iter()
            .map(|report| UpdateEntryResponse {
                project_id: report.project_id,
                target: report.target,
                from: report.from,
                to: report.to,
            })
            .collect(),
        mods: ModsResponse::new(&manifest),
    })
    .ok()
}

#[derive(ToSchema, Deserialize)]
pub struct AutoUpdateRequest {
    pub enabled: bool,
}

#[utoipa::path(post, path = "/auto-update", request_body = AutoUpdateRequest, responses(
    (status = OK, body = inline(ModsResponse)),
    (status = UNAUTHORIZED, body = ApiError),
    (status = FORBIDDEN, body = ApiError),
), params(
    (
        "server" = uuid::Uuid,
        description = "The server ID",
        example = "123e4567-e89b-12d3-a456-426614174000",
    ),
))]
pub async fn set_auto_update(
    state: GetState,
    permissions: GetPermissionManager,
    server: GetServer,
    axum::Json(data): axum::Json<AutoUpdateRequest>,
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.update")?;

    let mut manifest = read_manifest(&state, &server.0).await?;
    manifest.auto_update = data.enabled;
    write_manifest(&state, &server.0, &manifest).await?;

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

pub fn router(state: &shared::State) -> OpenApiRouter<shared::State> {
    OpenApiRouter::new()
        .routes(routes!(list))
        .routes(routes!(search))
        .routes(routes!(versions))
        .routes(routes!(updates))
        .routes(routes!(set_detection))
        .routes(routes!(install))
        .routes(routes!(uninstall))
        .routes(routes!(pin))
        .routes(routes!(update))
        .routes(routes!(set_auto_update))
        .with_state(state.clone())
}
