//! Update checks for the Elysium Visuals mod. The mod is served by the Elysium server only to
//! launcher sessions with an active subscription. The launcher itself updates through `launcher_update`.

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::elysium;
use crate::elysium::api::{self, Authed, ModRelease};
use crate::error::{Error, Result};
use crate::fabric;

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
    pub mod_version: Option<String>,
    pub mod_update: Option<ReleaseInfo>,
    /// Why the mod check failed, if it did.
    pub mod_error: Option<String>,
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

fn signed_out() -> Error {
    Error::msg("Сессия аккаунта Elysium завершена — войдите снова")
}

fn token() -> Result<String> {
    elysium::load_token()?.ok_or_else(|| Error::msg("Войдите в аккаунт Elysium, чтобы скачать мод"))
}

/// The current release from the Elysium server (launcher session with an active subscription).
async fn latest() -> Result<(String, ModRelease)> {
    let token = token()?;
    match api::mod_release(&token).await? {
        Authed::Ok(r) => Ok((token, r)),
        Authed::SignedOut => Err(signed_out()),
    }
}

fn info(r: &ModRelease) -> ReleaseInfo {
    ReleaseInfo { version: r.version.clone(), notes: r.notes.clone(), page: api::site_url("/account") }
}

/// Downloads the release's mod jar into the launcher folder.
async fn download_mod(token: &str, r: &ModRelease) -> Result<()> {
    match api::download_mod(token, r, &fabric::downloaded_mod()).await? {
        Authed::Ok(()) => Ok(()),
        Authed::SignedOut => Err(signed_out()),
    }
}

/// Makes sure the latest mod release is downloaded. Offline, an already
/// downloaded copy is kept (the subscription itself is checked before launch).
pub async fn sync_mod(_client: &reqwest::Client) -> Result<()> {
    let local = fabric::downloaded_mod();
    let have = local.is_file();
    let result = async {
        let (token, r) = latest().await?;
        let local_version = if have { jar_mod_version(&local) } else { None };
        if !have || is_newer(&r.version, local_version.as_deref().unwrap_or("0")) {
            download_mod(&token, &r).await?;
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

#[tauri::command]
pub async fn check_updates() -> Result<UpdateStatus> {
    let mod_version = fabric::mod_source().and_then(|p| jar_mod_version(&p));
    let mut status = UpdateStatus { mod_version: mod_version.clone(), mod_update: None, mod_error: None };
    match latest().await {
        Ok((_, r)) if is_newer(&r.version, mod_version.as_deref().unwrap_or("0")) => status.mod_update = Some(info(&r)),
        Ok(_) => {}
        Err(e) => status.mod_error = Some(format!("Не удалось проверить обновления мода: {e}")),
    }
    Ok(status)
}

/// Downloads the newest mod jar into the launcher folder; it is copied into
/// `mods/` on the next launch.
#[tauri::command]
pub async fn update_mod() -> Result<String> {
    let (token, r) = latest().await?;
    download_mod(&token, &r).await?;
    Ok(info(&r).version)
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

