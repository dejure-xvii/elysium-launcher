//! Update checks for the mod and the launcher via GitHub Releases.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::config;
use crate::download::{fetch, http_client, DownloadTask};
use crate::error::{Error, Result};
use crate::fabric;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    pub notes: String,
    pub page: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub launcher_version: String,
    pub mod_version: Option<String>,
    pub launcher_update: Option<ReleaseInfo>,
    pub mod_update: Option<ReleaseInfo>,
}

/// Numeric comparison of "v1.2.3"-style versions (build metadata ignored).
pub fn is_newer(remote: &str, local: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches(['v', 'V'])
            .split(['+', '-'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (r, l) = (parts(remote), parts(local));
    for i in 0..r.len().max(l.len()) {
        let (a, b) = (r.get(i).copied().unwrap_or(0), l.get(i).copied().unwrap_or(0));
        if a != b {
            return a > b;
        }
    }
    false
}

/// Parsed `fabric.mod.json` of a mod jar.
pub fn jar_mod_json(jar: &Path) -> Option<serde_json::Value> {
    let file = std::fs::File::open(jar).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut entry = zip.by_name("fabric.mod.json").ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

/// Reads `version` from `fabric.mod.json` inside a mod jar.
pub fn jar_mod_version(jar: &Path) -> Option<String> {
    jar_mod_json(jar)?.get("version")?.as_str().map(str::to_string)
}

async fn latest(client: &reqwest::Client, repo: &str) -> Result<GhRelease> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    Ok(client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

fn info(r: &GhRelease) -> ReleaseInfo {
    ReleaseInfo {
        version: r.tag_name.trim_start_matches('v').to_string(),
        notes: r.body.clone().unwrap_or_default(),
        page: r.html_url.clone(),
    }
}

/// Downloads the release's mod jar into the launcher folder.
async fn download_mod(client: &reqwest::Client, r: &GhRelease) -> Result<()> {
    let asset = r
        .assets
        .iter()
        .find(|a| a.name.ends_with(".jar") && !a.name.contains("sources"))
        .ok_or_else(|| Error::msg("В релизе мода нет .jar файла"))?;
    let task = DownloadTask {
        url: asset.browser_download_url.clone(),
        path: fabric::downloaded_mod(),
        sha1: None,
        size: Some(asset.size),
        sha512: None,
    };
    fetch(client, &task).await
}

/// Makes sure the latest mod release is downloaded. Offline, an already
/// downloaded copy is kept.
pub async fn sync_mod(client: &reqwest::Client) -> Result<()> {
    let local = fabric::downloaded_mod();
    let have = local.is_file();
    let result = async {
        let r = latest(client, config::MOD_REPO).await?;
        let local_version = if have { jar_mod_version(&local) } else { None };
        if !have || is_newer(&r.tag_name, local_version.as_deref().unwrap_or("0")) {
            download_mod(client, &r).await?;
        }
        Ok(())
    }
    .await;
    match result {
        Err(_) if have => Ok(()),
        Err(e) => Err(Error::msg(format!("Не удалось скачать мод Elysium Visuals: {e}"))),
        ok => ok,
    }
}

async fn launcher_update(app: &AppHandle) -> Result<Option<Update>> {
    Ok(app.updater()?.check().await?)
}

#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<UpdateStatus> {
    let mod_version = fabric::mod_source().and_then(|p| jar_mod_version(&p));
    let mut status = UpdateStatus {
        launcher_version: config::LAUNCHER_VERSION.to_string(),
        mod_version: mod_version.clone(),
        launcher_update: None,
        mod_update: None,
    };
    if let Some(u) = launcher_update(&app).await? {
        status.launcher_update = Some(ReleaseInfo {
            page: format!("https://github.com/{}/releases/tag/v{}", config::LAUNCHER_REPO, u.version),
            notes: u.body.clone().unwrap_or_default(),
            version: u.version,
        });
    }
    let r = latest(&http_client(), config::MOD_REPO).await?;
    if is_newer(&r.tag_name, mod_version.as_deref().unwrap_or("0")) {
        status.mod_update = Some(info(&r));
    }
    Ok(status)
}

/// Downloads the newest mod jar into the launcher folder; it is copied into
/// `mods/` on the next launch.
#[tauri::command]
pub async fn update_mod() -> Result<String> {
    let client = http_client();
    let r = latest(&client, config::MOD_REPO).await?;
    download_mod(&client, &r).await?;
    Ok(info(&r).version)
}

/// Downloads the signed launcher update, verifies it and runs the installer.
#[tauri::command]
pub async fn update_launcher(app: AppHandle) -> Result<()> {
    let update = launcher_update(&app)
        .await?
        .ok_or_else(|| Error::msg("У вас последняя версия лаунчера"))?;
    update.download_and_install(|_, _| {}, || {}).await?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn versions() {
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(is_newer("1.1", "1.0.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("1.0.0+26.2", "1.0.0"));
    }
}

#[cfg(test)]
mod net_tests {
    /// `cargo test sync_mod -- --ignored`
    #[test]
    #[ignore]
    fn sync_mod_downloads_latest() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(super::sync_mod(&crate::download::http_client())).expect("sync");
        let jar = crate::fabric::downloaded_mod();
        println!("{} -> {:?}", jar.display(), super::jar_mod_version(&jar));
        assert!(jar.is_file());
    }
}
