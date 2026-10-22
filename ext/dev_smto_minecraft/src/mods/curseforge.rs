//! CurseForge API client. Needs an API key, which the panel admin enters in the
//! extension settings.

use serde::Deserialize;

const BASE: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: u32 = 432;
const MODS_CLASS_ID: u32 = 6;

/// Loader ids used by CurseForge. NeoForge shares Forge's mod loader filter in most
/// cases, but has its own id since 1.20.2.
pub fn mod_loader_id(loader: &str) -> u32 {
    match loader {
        "fabric" => 4,
        "quilt" => 5,
        "forge" => 1,
        "neoforge" => 6,
        _ => 1,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mod {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub summary: String,
    #[serde(default)]
    pub logo: Option<Logo>,
    #[serde(default)]
    pub downloads: DownloadCount,
    #[serde(default)]
    pub links: Links,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Logo {
    #[serde(default)]
    pub thumbnail_url: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DownloadCount {
    #[serde(default)]
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Links {
    #[serde(default)]
    pub website_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub id: u32,
    pub mod_id: u32,
    pub display_name: String,
    pub file_name: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub hashes: Vec<FileHash>,
    #[serde(default)]
    pub file_length: u64,
    #[serde(default)]
    pub dependencies: Vec<FileDependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileHash {
    pub value: String,
    pub algo: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileDependency {
    pub mod_id: u32,
    pub relation_type: u32,
}

impl FileDependency {
    /// Relation type 3 is `RequiredDependency`.
    pub fn is_required(&self) -> bool {
        self.relation_type == 3
    }
}

impl File {
    /// CurseForge uses algo 1 for sha1 and 2 for md5.
    pub fn has_download(&self) -> bool {
        self.download_url.is_some()
    }
}

fn client(api_key: &str) -> reqwest::Client {
    use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(api_key).expect("valid api key header"),
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("dev.smto.minecraft/0.1.0 (Calagopus panel extension)")
        .build()
        .expect("reqwest client")
}

#[derive(Deserialize)]
struct DataResponse<T> {
    data: T,
}

pub async fn search(
    api_key: &str,
    query: &str,
    loader: Option<&str>,
    mc_version: Option<&str>,
) -> anyhow::Result<Vec<Mod>> {
    let mut request = client(api_key)
        .get(format!("{BASE}/mods/search"))
        .query(&[
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("classId", MODS_CLASS_ID.to_string()),
            ("searchFilter", query.to_string()),
            ("sortField", "2".to_string()),
            ("sortOrder", "desc".to_string()),
            ("pageSize", "20".to_string()),
        ]);

    if let Some(loader) = loader {
        request = request.query(&[("modLoaderType", mod_loader_id(loader).to_string())]);
    }
    if let Some(mc_version) = mc_version {
        request = request.query(&[("gameVersion", mc_version.to_string())]);
    }

    let response = request
        .send()
        .await?
        .error_for_status()?
        .json::<DataResponse<Vec<Mod>>>()
        .await?;

    Ok(response.data)
}

pub async fn files(
    api_key: &str,
    mod_id: u32,
    loader: Option<&str>,
    mc_version: Option<&str>,
) -> anyhow::Result<Vec<File>> {
    let mut request = client(api_key)
        .get(format!("{BASE}/mods/{mod_id}/files"))
        .query(&[("pageSize", "30".to_string())]);

    if let Some(loader) = loader {
        request = request.query(&[("modLoaderType", mod_loader_id(loader).to_string())]);
    }
    if let Some(mc_version) = mc_version {
        request = request.query(&[("gameVersion", mc_version.to_string())]);
    }

    let response = request
        .send()
        .await?
        .error_for_status()?
        .json::<DataResponse<Vec<File>>>()
        .await?;

    Ok(response.data)
}

pub async fn file(api_key: &str, mod_id: u32, file_id: u32) -> anyhow::Result<File> {
    let response = client(api_key)
        .get(format!("{BASE}/mods/{mod_id}/files/{file_id}"))
        .send()
        .await?
        .error_for_status()?
        .json::<DataResponse<File>>()
        .await?;

    Ok(response.data)
}

pub async fn mod_info(api_key: &str, mod_id: u32) -> anyhow::Result<Mod> {
    let response = client(api_key)
        .get(format!("{BASE}/mods/{mod_id}"))
        .send()
        .await?
        .error_for_status()?
        .json::<DataResponse<Mod>>()
        .await?;

    Ok(response.data)
}
