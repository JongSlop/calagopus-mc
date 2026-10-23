//! Adopting mods that were installed before this extension was: every jar in the mod
//! directories is hashed by wings and looked up at the providers, then recorded in the
//! manifest so it can be pinned, updated and reverted like anything else.

use std::collections::{HashMap, HashSet};

use shared::{
    State,
    models::{server::Server},
};
use tracing::warn;

use crate::mods::{InstalledMod, Manifest, Target, curseforge, modrinth};

/// How many files go into one provider lookup. Wings takes the list in the query
/// string, so keep it well below anything that could produce a URL that is too long.
const LOOKUP_CHUNK: usize = 100;

pub struct AdoptedEntry {
    pub project_name: String,
    pub provider: String,
    pub version_name: String,
    pub filename: String,
    pub target: Target,
}

#[derive(Default)]
pub struct AdoptReport {
    pub adopted: Vec<AdoptedEntry>,
    pub already_tracked: usize,
    /// Jars no provider recognises. Left alone, they stay unmanaged.
    pub unmatched: Vec<String>,
}

struct Candidate {
    target: Target,
    /// Path inside the server, e.g. `mods/sodium.jar`.
    path: String,
    filename: String,
}

fn is_mod_jar(name: &str) -> bool {
    name.to_lowercase().ends_with(".jar")
}

/// Lists the jars in both mod directories. A missing directory is not an error, a
/// server without AutoModpack simply has nothing there.
async fn candidates(
    state: &State,
    server: &Server,
    tracked: &HashSet<(Target, String)>,
) -> anyhow::Result<(Vec<Candidate>, usize)> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let mut candidates = Vec::new();
    let mut already_tracked = 0;

    for target in [Target::Server, Target::Client] {
        let directory = target.dir();

        let entries = match api
            .get_servers_server_files_list_directory(
                server.uuid,
                &wings_api::servers_server_files_list_directory::get::Query {
                    directory: Some(directory.into()),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(entries) => entries,
            Err(err) => {
                warn!(directory, "cannot list mod directory: {err:?}");
                continue;
            }
        };

        for entry in entries {
            if entry.directory || !is_mod_jar(&entry.name) {
                continue;
            }

            let filename = entry.name.to_string();

            if tracked.contains(&(target, filename.clone())) {
                already_tracked += 1;
                continue;
            }

            candidates.push(Candidate {
                target,
                path: format!("{directory}/{filename}"),
                filename,
            });
        }
    }

    Ok((candidates, already_tracked))
}

/// Hashes per path, chunked so the query string stays reasonable. Wings computes them
/// on the node, so no jar is ever sent through the panel.
async fn hashes(
    state: &State,
    server: &Server,
    candidates: &[Candidate],
    algorithm: wings_api::Algorithm,
) -> anyhow::Result<HashMap<String, String>> {
    let node = server.node.fetch_cached(&state.database).await?;
    let api = node.api_client(&state.database).await?;

    let mut hashes = HashMap::new();

    for chunk in candidates.chunks(LOOKUP_CHUNK) {
        let files = chunk
            .iter()
            .map(|candidate| candidate.path.clone().into())
            .collect();

        let response = api
            .get_servers_server_files_fingerprints(
                server.uuid,
                &wings_api::servers_server_files_fingerprints::get::Query {
                    algorithm: Some(algorithm),
                    files: Some(files),
                    ..Default::default()
                },
            )
            .await?;

        hashes.extend(
            response
                .fingerprints
                .into_iter()
                .map(|(path, hash)| (path.to_string(), hash.to_string())),
        );
    }

    Ok(hashes)
}

pub async fn adopt(
    state: &State,
    server: &Server,
    manifest: &mut Manifest,
) -> anyhow::Result<AdoptReport> {
    let tracked: HashSet<(Target, String)> = manifest
        .mods
        .iter()
        .map(|entry| (entry.target, entry.filename.clone()))
        .collect();

    let (candidates, already_tracked) = candidates(state, server, &tracked).await?;

    if candidates.is_empty() {
        return Ok(AdoptReport {
            already_tracked,
            ..Default::default()
        });
    }

    // Modrinth first: no API key needed and it hands back the project id we store.
    let sha512 = hashes(state, server, &candidates, wings_api::Algorithm::Sha512).await?;

    let unique: Vec<String> = sha512
        .values()
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut modrinth_versions = HashMap::new();
    for chunk in unique.chunks(LOOKUP_CHUNK) {
        modrinth_versions.extend(modrinth::versions_by_hashes(chunk).await?);
    }

    // CurseForge is matched by fingerprint, so the jars are hashed once more.
    let cf_hashes = match crate::settings::curseforge_key(state).await? {
        Some(_) => hashes(state, server, &candidates, wings_api::Algorithm::Curseforge).await?,
        None => HashMap::new(),
    };

    let mut cf_files = HashMap::new();
    if let Some(key) = crate::settings::curseforge_key(state).await? {
        let unique: Vec<u32> = cf_hashes
            .values()
            .filter_map(|hash| hash.parse::<u32>().ok())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        for chunk in unique.chunks(LOOKUP_CHUNK) {
            for file in curseforge::match_fingerprints(&key, chunk).await? {
                // CurseForge does not echo the fingerprint back. Match on the sha1 it
                // publishes instead, so a jar the user renamed still resolves.
                if let Some(sha1) = file.sha1() {
                    cf_files.insert(sha1.to_lowercase(), file);
                }
            }
        }
    }

    let sha1 = if cf_files.is_empty() {
        HashMap::new()
    } else {
        hashes(state, server, &candidates, wings_api::Algorithm::Sha1).await?
    };

    // Names, in one request per provider.
    let modrinth_ids: Vec<String> = modrinth_versions
        .values()
        .map(|version| version.project_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let modrinth_titles = modrinth::project_titles(&modrinth_ids).await.unwrap_or_default();

    let cf_ids: Vec<u32> = cf_files
        .values()
        .map(|file| file.mod_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let cf_names = match crate::settings::curseforge_key(state).await? {
        Some(key) => curseforge::mods(&key, &cf_ids)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|project| (project.id, project.name))
            .collect::<HashMap<_, _>>(),
        None => HashMap::new(),
    };

    let mut report = AdoptReport {
        already_tracked,
        ..Default::default()
    };

    for candidate in candidates {
        let entry = sha512
            .get(&candidate.path)
            .and_then(|hash| modrinth_versions.get(hash))
            .map(|version| InstalledMod {
                provider: "modrinth".to_string(),
                project_id: version.project_id.clone(),
                project_name: modrinth_titles
                    .get(&version.project_id)
                    .cloned()
                    .unwrap_or_else(|| version.project_id.clone()),
                version_id: version.id.clone(),
                version_name: version.version_number.clone(),
                filename: candidate.filename.clone(),
                target: candidate.target,
                pinned: false,
                sha512: sha512.get(&candidate.path).cloned(),
                updated: None,
                previous: None,
            })
            .or_else(|| {
                cf_hashes
                    .get(&candidate.path)
                    .and_then(|_| {
                        sha1
                            .get(&candidate.path)
                            .and_then(|sum| cf_files.get(sum))
                    })
                    .map(|file| InstalledMod {
                        provider: "curseforge".to_string(),
                        project_id: file.mod_id.to_string(),
                        project_name: cf_names
                            .get(&file.mod_id)
                            .cloned()
                            .unwrap_or_else(|| file.mod_id.to_string()),
                        version_id: file.id.to_string(),
                        version_name: file.display_name.clone(),
                        filename: candidate.filename.clone(),
                        target: candidate.target,
                        pinned: false,
                        sha512: None,
                        updated: None,
                        previous: None,
                    })
            });

        match entry {
            Some(entry) => {
                report.adopted.push(AdoptedEntry {
                    project_name: entry.project_name.clone(),
                    provider: entry.provider.clone(),
                    version_name: entry.version_name.clone(),
                    filename: entry.filename.clone(),
                    target: entry.target,
                });

                manifest.upsert(entry);
            }
            None => report.unmatched.push(candidate.filename),
        }
    }

    Ok(report)
}
