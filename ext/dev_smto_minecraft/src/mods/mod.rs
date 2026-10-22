//! Mod manager state: the per-server manifest, loader/version detection and the
//! Modrinth client.
//!
//! The manifest lives in the server directory (`smto-modmanager.json`) instead of the
//! panel database, so it travels with the server on transfers and restores and does
//! not tie the extension to a database schema.

pub mod curseforge;
pub mod manager;
pub mod modrinth;

use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "smto-modmanager.json";
pub const SERVER_MODS_DIR: &str = "mods";
pub const CLIENT_MODS_DIR: &str = "automodpack/host-modpack/main/mods";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Fabric,
    NeoForge,
    Forge,
}

impl Loader {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::NeoForge => "neoforge",
            Self::Forge => "forge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// `/mods`, loaded by the server.
    Server,
    /// `/automodpack/host-modpack/main/mods`, served to players by AutoModpack.
    Client,
}

impl Target {
    pub fn dir(self) -> &'static str {
        match self {
            Self::Server => SERVER_MODS_DIR,
            Self::Client => CLIENT_MODS_DIR,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub provider: String,
    pub project_id: String,
    pub project_name: String,
    pub version_id: String,
    pub version_name: String,
    pub filename: String,
    pub target: Target,
    pub pinned: bool,
    #[serde(default)]
    pub sha512: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    /// The version that was replaced by the last install, so it can be restored.
    #[serde(default)]
    pub previous: Option<PreviousVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviousVersion {
    pub version_id: String,
    pub version_name: String,
}

fn default_interval_hours() -> u32 {
    6
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Detection {
    pub loader: Option<Loader>,
    pub mc_version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub detection: Detection,
    /// Opt-in per server: the cron task updates unpinned mods while the server is offline.
    #[serde(default)]
    pub auto_update: bool,
    /// How many hours have to pass between two auto update runs.
    #[serde(default = "default_interval_hours")]
    pub auto_update_interval_hours: u32,
    #[serde(default)]
    pub auto_update_last_run: Option<String>,
    #[serde(default)]
    pub mods: Vec<InstalledMod>,
}

impl Manifest {
    pub fn parse(input: &str) -> Self {
        serde_json::from_str(input).unwrap_or_default()
    }

    pub fn render(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        json.push('\n');
        json
    }

    pub fn find(&self, project_id: &str, target: Target) -> Option<&InstalledMod> {
        self.mods
            .iter()
            .find(|entry| entry.project_id == project_id && entry.target == target)
    }

    pub fn upsert(&mut self, entry: InstalledMod) {
        match self
            .mods
            .iter_mut()
            .find(|existing| existing.project_id == entry.project_id && existing.target == entry.target)
        {
            Some(existing) => *existing = entry,
            None => self.mods.push(entry),
        }
    }

    pub fn remove(&mut self, project_id: &str, target: Target) -> Option<InstalledMod> {
        let index = self
            .mods
            .iter()
            .position(|entry| entry.project_id == project_id && entry.target == target)?;

        Some(self.mods.remove(index))
    }
}

/// Downloads a file and returns its bytes plus the sha512 hash of what arrived.
pub async fn download(url: &str, max_bytes: u64) -> anyhow::Result<(Vec<u8>, String)> {
    use sha2::{Digest, Sha512};

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("dev.smto.minecraft/0.1.0 (Calagopus panel extension)")
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?;

    if let Some(length) = response.content_length()
        && length > max_bytes
    {
        anyhow::bail!("file is {length} bytes, above the {max_bytes} byte limit");
    }

    let bytes = response.bytes().await?;

    if bytes.len() as u64 > max_bytes {
        anyhow::bail!("file exceeded the {max_bytes} byte limit");
    }

    Ok((bytes.to_vec(), hex::encode(Sha512::digest(&bytes))))
}

/// sha1 hex of a file, used by CurseForge which does not publish sha512.
pub fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};

    hex::encode(Sha1::digest(bytes))
}

/// A directory entry as returned by wings, reduced to what detection needs.
pub struct DirEntry {
    pub name: String,
    pub directory: bool,
}

/// NeoForge versions its jars after the Minecraft version, but only from 1.20.2 on:
/// `21.1.72` is Minecraft 1.21.1, `20.4.237` is 1.20.4. The 1.20.1 line predates the
/// rename and kept Forge-style numbers, where `47` stands for 1.20.1.
fn neoforge_mc_version(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;

    match major {
        47 => Some("1.20.1".to_string()),
        20..=99 => Some(format!("1.{major}.{minor}")),
        _ => None,
    }
}

/// Works out loader and Minecraft version from the files on disk. Anything it cannot
/// see can be overridden by the user and is stored in the manifest.
pub fn detect(entries: &[DirEntry]) -> Detection {
    let mut detection = Detection::default();

    for entry in entries {
        let name = entry.name.to_lowercase();

        if entry.directory {
            continue;
        }

        if name.starts_with("fabric-server") {
            detection.loader.get_or_insert(Loader::Fabric);

            // fabric-server-mc.1.21.1-loader.0.16.9-launcher.1.0.1.jar
            if let Some(rest) = name.split("mc.").nth(1)
                && let Some(version) = rest.split('-').next()
            {
                detection.mc_version.get_or_insert_with(|| version.to_string());
            }
        }

        if let Some(version) = name
            .strip_prefix("minecraft_server.")
            .or_else(|| name.strip_prefix("server-"))
            .and_then(|rest| rest.strip_suffix(".jar"))
        {
            detection.mc_version.get_or_insert_with(|| version.to_string());
        }

        if name.starts_with("forge-") || name.starts_with("neoforge-") {
            // forge-1.21.1-52.0.1.jar / neoforge-21.1.72.jar
            let neoforge = name.starts_with("neoforge-");

            detection
                .loader
                .get_or_insert(if neoforge {
                    Loader::NeoForge
                } else {
                    Loader::Forge
                });

            if let Some(version) = name.split('-').nth(1) {
                let mc_version = if neoforge {
                    neoforge_mc_version(version)
                } else {
                    version
                        .starts_with("1.")
                        .then(|| version.to_string())
                };

                if let Some(mc_version) = mc_version {
                    detection.mc_version.get_or_insert(mc_version);
                }
            }
        }
    }

    detection
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            directory: false,
        }
    }

    #[test]
    fn detects_fabric_from_launcher_jar() {
        let detection = detect(&[file("fabric-server-mc.1.21.1-loader.0.16.9-launcher.1.0.1.jar")]);
        assert_eq!(detection.loader, Some(Loader::Fabric));
        assert_eq!(detection.mc_version.as_deref(), Some("1.21.1"));
    }

    #[test]
    fn detects_forge_and_neoforge() {
        let forge = detect(&[file("forge-1.20.1-47.2.0.jar")]);
        assert_eq!(forge.loader, Some(Loader::Forge));
        assert_eq!(forge.mc_version.as_deref(), Some("1.20.1"));

        let neoforge = detect(&[file("neoforge-21.1.72.jar")]);
        assert_eq!(neoforge.loader, Some(Loader::NeoForge));
        assert_eq!(neoforge.mc_version.as_deref(), Some("1.21.1"));
    }

    #[test]
    fn maps_neoforge_versions_to_minecraft() {
        let modern = detect(&[file("neoforge-21.1.72.jar")]);
        assert_eq!(modern.mc_version.as_deref(), Some("1.21.1"), "21.1 is 1.21.1");

        let older = detect(&[file("neoforge-20.4.237.jar")]);
        assert_eq!(older.mc_version.as_deref(), Some("1.20.4"), "20.4 is 1.20.4");

        // The 1.20.1 line kept Forge numbering, where 47 means 1.20.1.
        let legacy = detect(&[file("neoforge-47.1.106-universal.jar")]);
        assert_eq!(legacy.loader, Some(Loader::NeoForge));
        assert_eq!(legacy.mc_version.as_deref(), Some("1.20.1"));

        assert_eq!(neoforge_mc_version("19.0.1"), None, "unmapped must stay unknown");
    }

    #[test]
    fn detects_vanilla_server_jar() {
        assert_eq!(
            detect(&[file("minecraft_server.1.20.1.jar")]).mc_version.as_deref(),
            Some("1.20.1")
        );
        assert_eq!(
            detect(&[file("server-1.21.1.jar")]).mc_version.as_deref(),
            Some("1.21.1")
        );
    }

    #[test]
    fn manifest_round_trip_and_upsert() {
        let mut manifest = Manifest::default();
        let entry = InstalledMod {
            provider: "modrinth".into(),
            project_id: "sodium".into(),
            project_name: "Sodium".into(),
            version_id: "abc".into(),
            version_name: "1.0".into(),
            filename: "sodium.jar".into(),
            target: Target::Client,
            pinned: false,
            sha512: None,
            updated: None,
            previous: None,
        };

        manifest.upsert(entry.clone());
        manifest.upsert(entry.clone());
        assert_eq!(manifest.mods.len(), 1);

        let parsed = Manifest::parse(&manifest.render());
        assert_eq!(parsed.mods.len(), 1);
        assert_eq!(parsed.mods[0].filename, "sodium.jar");

        assert!(manifest.remove("sodium", Target::Client).is_some());
        assert!(manifest.mods.is_empty());
    }
}
