use axum::http::StatusCode;
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
    mods::{self, InstalledMod, Loader, MANIFEST_FILE, Manifest, Target, modrinth},
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
    pub mods: Vec<InstalledModResponse>,
}

impl ModsResponse {
    fn new(manifest: &Manifest) -> Self {
        Self {
            detection: DetectionResponse {
                loader: manifest.detection.loader,
                mc_version: manifest.detection.mc_version.clone(),
            },
            mods: manifest.mods.iter().map(Into::into).collect(),
        }
    }
}

pub(crate) async fn read_manifest(
    state: &shared::State,
    server: &GetServer,
) -> anyhow::Result<Manifest> {
    let content = server_file::read_optional(state, server, MANIFEST_FILE).await?;
    Ok(content
        .as_deref()
        .map(Manifest::parse)
        .unwrap_or_default())
}

pub(crate) async fn write_manifest(
    state: &shared::State,
    server: &GetServer,
    manifest: &Manifest,
) -> anyhow::Result<()> {
    server_file::write(state, server, MANIFEST_FILE, manifest.render()).await
}

/// Fills in loader and version from the server directory when the manifest has none.
pub(crate) async fn detect_and_store(
    state: &shared::State,
    server: &GetServer,
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

    let mut manifest = read_manifest(&state, &server).await?;
    if detect_and_store(&state, &server, &mut manifest).await? {
        write_manifest(&state, &server, &manifest).await?;
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

    let mut manifest = read_manifest(&state, &server).await?;
    manifest.detection.loader = data.loader;
    manifest.detection.mc_version = data.mc_version.filter(|version| !version.trim().is_empty());
    write_manifest(&state, &server, &manifest).await?;

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

    let manifest = read_manifest(&state, &server).await?;
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

    let hits = modrinth::search(
        params.query.as_deref().unwrap_or(""),
        Some(loader),
        Some(&mc_version),
        20,
    )
    .await
    .map_err(|err| ApiResponse::error(err.to_string()))?;

    let hits = hits
        .into_iter()
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
        .collect::<Vec<_>>();

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
) -> ApiResponseResult {
    permissions.has_server_permission("minecraft-mods.read")?;

    let manifest = read_manifest(&state, &server).await?;
    let loader = manifest.detection.loader.map(Loader::as_str);

    let versions = modrinth::versions(
        &project_id,
        loader,
        manifest.detection.mc_version.as_deref(),
    )
    .await
    .map_err(|err| ApiResponse::error(err.to_string()))?;

    let versions = versions
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
        .collect::<Vec<_>>();

    ApiResponse::new_serialized(versions).ok()
}

#[derive(ToSchema, Deserialize)]
pub struct InstallRequest {
    /// Modrinth project id or slug.
    pub project_id: String,
    /// Version id. Omitted picks the newest compatible version.
    pub version_id: Option<String>,
    pub target: Target,
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

    let mut manifest = read_manifest(&state, &server).await?;
    let loader = manifest.detection.loader.map(Loader::as_str);
    let mc_version = manifest.detection.mc_version.clone();

    let version = match &data.version_id {
        Some(version_id) => modrinth::version(version_id).await,
        None => {
            let Some(mc_version) = mc_version.as_deref() else {
                return ApiResponse::error("the Minecraft version of this server is not known").ok();
            };

            match modrinth::versions(&data.project_id, loader, Some(mc_version)).await {
                Ok(versions) => versions
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no compatible version found")),
                Err(err) => Err(err),
            }
        }
    }
    .map_err(|err| ApiResponse::error(err.to_string()))?;

    let file = version
        .primary_file()
        .cloned()
        .ok_or_else(|| ApiResponse::error("this version has no downloadable file"))?;

    let (bytes, sha512) = modrinth::download(&file.url, 200 * 1024 * 1024)
        .await
        .map_err(|err| ApiResponse::error(err.to_string()))?;

    if let Some(expected) = file.sha512()
        && !expected.eq_ignore_ascii_case(&sha512)
    {
        return ApiResponse::error("the downloaded file failed hash verification")
            .with_status(StatusCode::BAD_GATEWAY)
            .ok();
    }

    let target_dir = data.target.dir();

    if data.target == Target::Client {
        ensure_client_dir(&state, &server).await?;
    }

    let previous = manifest.find(&data.project_id, data.target).cloned();
    let path = format!("{target_dir}/{}", file.filename);

    // Write next to the target and rename, so AutoModpack and Minecraft never see a
    // half-written jar.
    let temp_path = format!("{target_dir}/.smto-{}.tmp", uuid::Uuid::new_v4());
    server_file::write_bytes(&state, &server, &temp_path, bytes).await?;
    server_file::rename(&state, &server, &temp_path, &path).await?;

    if let Some(previous) = &previous
        && previous.filename != file.filename
    {
        let _ = server_file::delete(
            &state,
            &server,
            format!("{target_dir}/{}", previous.filename),
        )
        .await;
    }

    manifest.upsert(InstalledMod {
        provider: "modrinth".to_string(),
        project_id: data.project_id.clone(),
        project_name: data.project_id.clone(),
        version_id: version.id.clone(),
        version_name: version.version_number.clone(),
        filename: file.filename.clone(),
        target: data.target,
        pinned: previous.map(|entry| entry.pinned).unwrap_or(false),
        sha512: Some(sha512),
        updated: Some(chrono::Utc::now().to_rfc3339()),
    });
    write_manifest(&state, &server, &manifest).await?;

    activity_logger
        .log(
            "server:dev.smto.minecraft.mods.install",
            serde_json::json!({
                "project_id": data.project_id,
                "version_id": version.id,
                "target": target_dir,
                "file": file.filename,
            }),
        )
        .await;

    ApiResponse::new_serialized(ModsResponse::new(&manifest)).ok()
}

async fn ensure_client_dir(state: &shared::State, server: &GetServer) -> anyhow::Result<()> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    // Fails harmlessly when the directory already exists.
    let _ = api
        .post_servers_server_files_create_directory(
            server.uuid,
            &wings_api::servers_server_files_create_directory::post::RequestBody {
                root: "automodpack/host-modpack/main".into(),
                name: "mods".into(),
            },
        )
        .await;

    Ok(())
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

    let mut manifest = read_manifest(&state, &server).await?;
    let entry = manifest
        .remove(&data.project_id, data.target)
        .ok_or_else(|| ApiResponse::error("this mod is not managed by the extension"))?;

    server_file::delete(
        &state,
        &server,
        format!("{}/{}", data.target.dir(), entry.filename),
    )
    .await?;

    write_manifest(&state, &server, &manifest).await?;

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

pub fn router(state: &shared::State) -> OpenApiRouter<shared::State> {
    OpenApiRouter::new()
        .routes(routes!(list))
        .routes(routes!(search))
        .routes(routes!(versions))
        .routes(routes!(set_detection))
        .routes(routes!(install))
        .routes(routes!(uninstall))
        .with_state(state.clone())
}
