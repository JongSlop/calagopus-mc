//! Minimal Modrinth API client: search and version listing.

use serde::{Deserialize, Serialize};

const BASE: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub client_side: String,
    pub server_side: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub hashes: serde_json::Value,
    pub size: u64,
}

impl VersionFile {
    pub fn sha512(&self) -> Option<&str> {
        self.hashes.get("sha512").and_then(|value| value.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub project_id: Option<String>,
    pub dependency_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|file| file.primary)
            .or_else(|| self.files.first())
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("dev.smto.minecraft/0.1.0 (Calagopus panel extension)")
        .build()
        .expect("reqwest client")
}

pub async fn search(
    query: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
    limit: u32,
) -> anyhow::Result<Vec<SearchHit>> {
    let mut facets: Vec<String> = vec!["[\"project_type:mod\"]".to_string()];

    if let Some(loader) = loader {
        facets.push(format!("[\"categories:{loader}\"]"));
    }
    if let Some(mc_version) = mc_version {
        facets.push(format!("[\"versions:{mc_version}\"]"));
    }

    let response = client()
        .get(format!("{BASE}/search"))
        .query(&[
            ("query", query.to_string()),
            ("limit", limit.to_string()),
            ("facets", format!("[{}]", facets.join(","))),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<SearchResponse>()
        .await?;

    Ok(response.hits)
}

pub async fn versions(
    project_id: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
) -> anyhow::Result<Vec<Version>> {
    let mut request = client().get(format!("{BASE}/project/{project_id}/version"));

    if let Some(loader) = loader {
        request = request.query(&[("loaders", format!("[\"{loader}\"]"))]);
    }
    if let Some(mc_version) = mc_version {
        request = request.query(&[("game_versions", format!("[\"{mc_version}\"]"))]);
    }

    Ok(request
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Version>>()
        .await?)
}

pub async fn version(version_id: &str) -> anyhow::Result<Version> {
    Ok(client()
        .get(format!("{BASE}/version/{version_id}"))
        .send()
        .await?
        .error_for_status()?
        .json::<Version>()
        .await?)
}
