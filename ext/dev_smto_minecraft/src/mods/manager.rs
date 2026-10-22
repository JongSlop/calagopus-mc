//! Installing, updating and auto-updating mods. Shared by the routes and the
//! background task.

use serde::{Deserialize, Serialize};
use shared::{
    State,
    models::{ByUuid, server::Server},
};
use tracing::{info, warn};

use crate::{
    mods::{InstalledMod, Loader, MANIFEST_FILE, Manifest, Target, curseforge, modrinth},
    server_file,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Modrinth,
    CurseForge,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::CurseForge => "curseforge",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "modrinth" => Some(Self::Modrinth),
            "curseforge" => Some(Self::CurseForge),
            _ => None,
        }
    }
}

pub struct UpdateReport {
    pub project_id: String,
    pub target: Target,
    pub from: String,
    pub to: String,
}

pub struct UpdateAvailable {
    pub project_id: String,
    pub target: Target,
    pub current: String,
    pub latest: String,
    pub version_id: String,
}

/// A mod file resolved from a provider, ready to be downloaded.
struct ResolvedFile {
    url: String,
    filename: String,
    version_name: String,
    version_id: String,
    /// sha512 hex, when the provider gives us one to verify.
    sha512: Option<String>,
    /// sha1 hex, when the provider only offers that.
    sha1: Option<String>,
    /// Required dependency project ids.
    dependencies: Vec<String>,
}

async fn curseforge_key(state: &State) -> anyhow::Result<String> {
    crate::settings::curseforge_key(state)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("no CurseForge API key configured, add one in the extension settings")
        })
}

/// Resolves a specific version, or the newest matching one when `version_id` is `None`.
async fn resolve(
    state: &State,
    manifest: &Manifest,
    provider: Provider,
    project_id: &str,
    version_id: Option<&str>,
) -> anyhow::Result<ResolvedFile> {
    let loader = manifest.detection.loader.map(Loader::as_str);
    let mc_version = manifest.detection.mc_version.clone();
    let mc_version = mc_version.as_deref();

    match provider {
        Provider::Modrinth => {
            let version = match version_id {
                Some(version_id) => modrinth::version(version_id).await?,
                None => modrinth::versions(project_id, loader, mc_version)
                    .await?
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no compatible version found for {project_id}"))?,
            };

            let file = version
                .primary_file()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("this version has no downloadable file"))?;

            let sha512 = file.sha512().map(str::to_string);

            let dependencies = version
                .dependencies
                .iter()
                .filter(|dependency| dependency.dependency_type == "required")
                .filter_map(|dependency| dependency.project_id.clone())
                .collect();

            Ok(ResolvedFile {
                url: file.url,
                filename: file.filename,
                version_name: version.version_number,
                version_id: version.id,
                sha512,
                sha1: None,
                dependencies,
            })
        }
        Provider::CurseForge => {
            let api_key = curseforge_key(state).await?;
            let mod_id: u32 = project_id
                .parse()
                .map_err(|_| anyhow::anyhow!("'{project_id}' is not a CurseForge mod id"))?;

            let file = match version_id {
                Some(version_id) => {
                    let file_id: u32 = version_id.parse().map_err(|_| {
                        anyhow::anyhow!("'{version_id}' is not a CurseForge file id")
                    })?;
                    curseforge::file(&api_key, mod_id, file_id).await?
                }
                None => curseforge::files(&api_key, mod_id, loader, mc_version)
                    .await?
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no compatible file found for {project_id}"))?,
            };

            let url = file.download_url.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "the author of this mod does not allow API downloads, download it manually"
                )
            })?;

            let sha1 = file.sha1().map(str::to_string);

            let dependencies = file
                .dependencies
                .iter()
                .filter(|dependency| dependency.is_required())
                .map(|dependency| dependency.mod_id.to_string())
                .collect();

            Ok(ResolvedFile {
                url,
                filename: file.file_name,
                version_name: file.display_name,
                version_id: file.id.to_string(),
                sha512: None,
                sha1,
                dependencies,
            })
        }
    }
}

async fn ensure_client_dir(state: &State, server: &Server) -> anyhow::Result<()> {
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

/// Downloads a mod and puts it in place. Never leaves a half-written jar: the file is
/// written next to the target and renamed.
pub async fn install_one(
    state: &State,
    server: &Server,
    manifest: &mut Manifest,
    provider: Provider,
    project_id: &str,
    version_id: Option<&str>,
    target: Target,
) -> anyhow::Result<InstalledMod> {
    let resolved = resolve(state, manifest, provider, project_id, version_id).await?;
    let (bytes, sha512) = crate::mods::download(&resolved.url, 200 * 1024 * 1024).await?;

    if let Some(expected) = &resolved.sha512
        && !expected.eq_ignore_ascii_case(&sha512)
    {
        anyhow::bail!("the downloaded file for {project_id} failed hash verification");
    }

    if let Some(expected) = &resolved.sha1
        && !expected.eq_ignore_ascii_case(&crate::mods::sha1_hex(&bytes))
    {
        anyhow::bail!("the downloaded file for {project_id} failed hash verification");
    }

    let target_dir = target.dir();

    if target == Target::Client {
        ensure_client_dir(state, server).await?;
    }

    let previous = manifest.find(project_id, target).cloned();
    let path = format!("{target_dir}/{}", resolved.filename);

    let temp_path = format!("{target_dir}/.smto-{}.tmp", uuid::Uuid::new_v4());
    server_file::write_bytes(state, server, &temp_path, bytes).await?;
    server_file::rename(state, server, &temp_path, &path).await?;

    // Another provider entry may own the same file name, drop it so the manifest has
    // exactly one owner per file.
    manifest.mods.retain(|entry| {
        !(entry.target == target
            && entry.filename == resolved.filename
            && !(entry.project_id == project_id && provider_of(entry) == provider))
    });

    if let Some(previous) = &previous
        && previous.filename != resolved.filename
    {
        let _ =
            server_file::delete(state, server, format!("{target_dir}/{}", previous.filename)).await;
    }

    let project_name = match &previous {
        Some(entry) => entry.project_name.clone(),
        None => project_name_for(state, provider, project_id).await,
    };

    let entry = InstalledMod {
        provider: provider.as_str().to_string(),
        project_id: project_id.to_string(),
        project_name,
        version_id: resolved.version_id,
        version_name: resolved.version_name,
        filename: resolved.filename,
        target,
        // A pinned mod keeps its pin, updating it explicitly is allowed.
        pinned: previous.as_ref().map(|entry| entry.pinned).unwrap_or(false),
        sha512: Some(sha512),
        updated: Some(chrono::Utc::now().to_rfc3339()),
    };

    manifest.upsert(entry.clone());

    Ok(entry)
}

/// Installs a mod plus its required dependencies that are not managed yet. Only one
/// dependency level is followed, deeper graphs are rare for Minecraft mods.
/// ponytail: single level, walk recursively if nested requirements show up in practice
pub async fn install_with_dependencies(
    state: &State,
    server: &Server,
    manifest: &mut Manifest,
    provider: Provider,
    project_id: &str,
    version_id: Option<&str>,
    target: Target,
) -> anyhow::Result<InstalledMod> {
    let entry = install_one(state, server, manifest, provider, project_id, version_id, target).await?;

    let resolved = resolve(state, manifest, provider, project_id, version_id).await?;

    for dependency in resolved.dependencies {
        if manifest
            .mods
            .iter()
            .any(|entry| entry.project_id == dependency)
        {
            continue;
        }

        if let Err(err) = install_one(state, server, manifest, provider, &dependency, None, target).await
        {
            warn!(
                server = %server.uuid,
                dependency,
                "cannot install dependency: {err:#}"
            );
        }
    }

    Ok(entry)
}

async fn project_name_for(state: &State, provider: Provider, project_id: &str) -> String {
    let name = match provider {
        Provider::Modrinth => modrinth::project_title(project_id).await,
        Provider::CurseForge => match (curseforge_key(state).await, project_id.parse::<u32>()) {
            (Ok(api_key), Ok(mod_id)) => curseforge::mod_info(&api_key, mod_id)
                .await
                .map(|info| info.name),
            _ => Err(anyhow::anyhow!("no CurseForge key")),
        },
    };

    name.unwrap_or_else(|err| {
        warn!(provider = provider.as_str(), project_id, "cannot look up project name: {err:#}");
        project_id.to_string()
    })
}

fn provider_of(entry: &InstalledMod) -> Provider {
    Provider::parse(&entry.provider).unwrap_or(Provider::Modrinth)
}

/// Brings one managed mod to the newest version that matches the server. Returns
/// `None` when it is already current or pinned.
pub async fn update_one(
    state: &State,
    server: &Server,
    manifest: &mut Manifest,
    project_id: &str,
    target: Target,
) -> anyhow::Result<Option<UpdateReport>> {
    let Some(current) = manifest.find(project_id, target).cloned() else {
        anyhow::bail!("this mod is not managed by the extension");
    };

    if current.pinned {
        return Ok(None);
    }

    let latest = resolve(state, manifest, provider_of(&current), project_id, None).await?;

    if latest.version_id == current.version_id {
        return Ok(None);
    }

    let entry = install_one(
        state,
        server,
        manifest,
        provider_of(&current),
        project_id,
        Some(&latest.version_id),
        target,
    )
    .await?;

    Ok(Some(UpdateReport {
        project_id: project_id.to_string(),
        target,
        from: current.version_name,
        to: entry.version_name,
    }))
}

/// Lists managed mods that have a newer compatible version. Pinned mods are skipped.
pub async fn check_updates(
    state: &State,
    server: &Server,
    manifest: &Manifest,
) -> Vec<UpdateAvailable> {
    let mut available = Vec::new();

    for entry in manifest.mods.iter().filter(|entry| !entry.pinned) {
        let latest = match resolve(
            state,
            manifest,
            provider_of(entry),
            &entry.project_id,
            None,
        )
        .await
        {
            Ok(latest) => latest,
            Err(err) => {
                warn!(server = %server.uuid, project_id = %entry.project_id, "cannot check for updates: {err:#}");
                continue;
            }
        };

        if latest.version_id != entry.version_id {
            available.push(UpdateAvailable {
                project_id: entry.project_id.clone(),
                target: entry.target,
                current: entry.version_name.clone(),
                latest: latest.version_name,
                version_id: latest.version_id,
            });
        }
    }

    available
}

/// Updates every unpinned, managed mod of a server.
pub async fn update_all(
    state: &State,
    server: &Server,
    manifest: &mut Manifest,
) -> anyhow::Result<Vec<UpdateReport>> {
    let managed: Vec<(String, Target)> = manifest
        .mods
        .iter()
        .filter(|entry| !entry.pinned)
        .map(|entry| (entry.project_id.clone(), entry.target))
        .collect();

    let mut reports = Vec::new();

    for (project_id, target) in managed {
        match update_one(state, server, manifest, &project_id, target).await {
            Ok(Some(report)) => reports.push(report),
            Ok(None) => {}
            Err(err) => warn!(
                server = %server.uuid,
                project_id,
                "skipping mod update: {err:#}"
            ),
        }
    }

    Ok(reports)
}

/// One auto-update pass over every server. Runs from the cron task, so it must not
/// assume a request context.
pub async fn auto_update_pass(state: &State) -> anyhow::Result<()> {
    let server_uuids: Vec<uuid::Uuid> =
        sqlx::query_scalar("SELECT uuid FROM servers WHERE suspended = false")
            .fetch_all(state.database.write())
            .await?;

    for uuid in server_uuids {
        let server = match Server::by_uuid_cached(&state.database, uuid).await {
            Ok(server) => server,
            Err(err) => {
                warn!(server = %uuid, "cannot load server for mod updates: {err:#}");
                continue;
            }
        };

        let manifest = match server_file::read_optional(state, &server, MANIFEST_FILE).await {
            Ok(Some(content)) => Manifest::parse(&content),
            Ok(None) => continue,
            Err(err) => {
                warn!(server = %uuid, "cannot read {MANIFEST_FILE}: {err:#}");
                continue;
            }
        };

        if !manifest.auto_update || manifest.mods.is_empty() {
            continue;
        }

        // Jars are only ever touched while the server is stopped, so a running
        // server never sees a swapped jar and AutoModpack never serves a half one.
        let node = server.node.fetch_cached(&state.database).await?;
        let api = node.api_client(&state.database).await?;
        let is_offline = match api.get_servers_server(uuid).await {
            Ok(info) => matches!(info.state, wings_api::ServerState::Offline),
            Err(err) => {
                warn!(server = %uuid, "cannot read server state: {err:?}");
                continue;
            }
        };

        if !is_offline {
            info!(server = %uuid, "skipping mod auto update, server is not offline");
            continue;
        }

        let mut manifest = manifest;
        let reports = update_all(state, &server, &mut manifest).await?;

        if reports.is_empty() {
            continue;
        }

        server_file::write(state, &server, MANIFEST_FILE, manifest.render()).await?;

        for report in &reports {
            info!(
                server = %uuid,
                mod = %report.project_id,
                from = %report.from,
                to = %report.to,
                "auto updated mod"
            );
        }
    }

    Ok(())
}
