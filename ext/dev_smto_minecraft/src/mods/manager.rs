//! Installing, updating and auto-updating mods. Shared by the routes and the
//! background task.

use shared::{
    State,
    models::{ByUuid, server::Server},
};
use tracing::{info, warn};

use crate::{
    mods::{InstalledMod, Loader, MANIFEST_FILE, Manifest, Target, modrinth},
    server_file,
};

pub struct UpdateReport {
    pub project_id: String,
    pub target: Target,
    pub from: String,
    pub to: String,
}

/// Downloads a mod and puts it in place. Never leaves a half-written jar: the file is
/// written next to the target and renamed.
pub async fn install_one(
    state: &State,
    server: &Server,
    manifest: &mut Manifest,
    project_id: &str,
    version_id: Option<&str>,
    target: Target,
) -> anyhow::Result<InstalledMod> {
    let loader = manifest.detection.loader.map(Loader::as_str);
    let mc_version = manifest.detection.mc_version.clone();

    let version = match version_id {
        Some(version_id) => modrinth::version(version_id).await?,
        None => {
            let mc_version = mc_version
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("the Minecraft version of this server is not known"))?;

            modrinth::versions(project_id, loader, Some(mc_version))
                .await?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("no compatible version found for {project_id}"))?
        }
    };

    let file = version
        .primary_file()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("this version has no downloadable file"))?;

    let (bytes, sha512) = modrinth::download(&file.url, 200 * 1024 * 1024).await?;

    if let Some(expected) = file.sha512()
        && !expected.eq_ignore_ascii_case(&sha512)
    {
        anyhow::bail!("the downloaded file for {project_id} failed hash verification");
    }

    let target_dir = target.dir();

    if target == Target::Client {
        ensure_client_dir(state, server).await?;
    }

    let previous = manifest.find(project_id, target).cloned();
    let path = format!("{target_dir}/{}", file.filename);

    let temp_path = format!("{target_dir}/.smto-{}.tmp", uuid::Uuid::new_v4());
    server_file::write_bytes(state, server, &temp_path, bytes).await?;
    server_file::rename(state, server, &temp_path, &path).await?;

    if let Some(previous) = &previous
        && previous.filename != file.filename
    {
        let _ =
            server_file::delete(state, server, format!("{target_dir}/{}", previous.filename)).await;
    }

    let entry = InstalledMod {
        provider: "modrinth".to_string(),
        project_id: project_id.to_string(),
        project_name: previous
            .as_ref()
            .map(|entry| entry.project_name.clone())
            .unwrap_or_else(|| project_id.to_string()),
        version_id: version.id.clone(),
        version_name: version.version_number.clone(),
        filename: file.filename.clone(),
        target,
        // A pinned mod keeps its pin, updating it explicitly is allowed.
        pinned: previous.as_ref().map(|entry| entry.pinned).unwrap_or(false),
        sha512: Some(sha512),
        updated: Some(chrono::Utc::now().to_rfc3339()),
    };

    manifest.upsert(entry.clone());

    Ok(entry)
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

    let loader = manifest.detection.loader.map(Loader::as_str);
    let Some(mc_version) = manifest.detection.mc_version.clone() else {
        anyhow::bail!("the Minecraft version of this server is not known");
    };

    let Some(latest) = modrinth::versions(project_id, loader, Some(&mc_version))
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };

    if latest.id == current.version_id {
        return Ok(None);
    }

    let entry = install_one(state, server, manifest, project_id, Some(&latest.id), target).await?;

    Ok(Some(UpdateReport {
        project_id: project_id.to_string(),
        target,
        from: current.version_name,
        to: entry.version_name,
    }))
}

pub struct UpdateAvailable {
    pub project_id: String,
    pub target: Target,
    pub current: String,
    pub latest: String,
    pub version_id: String,
}

/// Lists managed mods that have a newer compatible version. Pinned mods are skipped.
pub async fn check_updates(
    _state: &State,
    server: &Server,
    manifest: &Manifest,
) -> Vec<UpdateAvailable> {
    let loader = manifest.detection.loader.map(Loader::as_str);
    let mc_version = manifest.detection.mc_version.clone();
    let mut available = Vec::new();

    for entry in manifest.mods.iter().filter(|entry| !entry.pinned) {
        let Some(mc_version) = mc_version.as_deref() else {
            break;
        };

        let latest = match modrinth::versions(&entry.project_id, loader, Some(mc_version)).await {
            Ok(versions) => versions.into_iter().next(),
            Err(err) => {
                warn!(server = %server.uuid, project_id = %entry.project_id, "cannot check for updates: {err:#}");
                continue;
            }
        };

        let Some(latest) = latest else {
            continue;
        };

        if latest.id != entry.version_id {
            available.push(UpdateAvailable {
                project_id: entry.project_id.clone(),
                target: entry.target,
                current: entry.version_name.clone(),
                latest: latest.version_number,
                version_id: latest.id,
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
    let server_uuids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT uuid FROM servers WHERE suspended = false",
    )
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
