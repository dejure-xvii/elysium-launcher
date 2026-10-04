//! Fabric Loader profile, Fabric API and the Elysium Visuals mod.

use std::path::{Path, PathBuf};

use crate::config;
use crate::download::manifest::{version_dir, Arguments, VersionJson};
use crate::download::{fetch, get_json, is_valid, sha1_file, DownloadTask, Verify};
use crate::error::{Error, Result};
use crate::paths;

/// Downloads (or loads cached) Fabric profile JSON for the configured loader.
pub async fn fetch_profile(client: &reqwest::Client, game_dir: &Path) -> Result<VersionJson> {
    let id = config::fabric_version_id();
    let path = version_dir(game_dir, &id).join(format!("{id}.json"));
    let url = format!(
        "{}/versions/loader/{}/{}/profile/json",
        config::FABRIC_META_URL,
        config::MINECRAFT_VERSION,
        config::FABRIC_LOADER_VERSION
    );
    match get_json::<serde_json::Value>(client, &url).await {
        Ok(json) => {
            tokio::fs::create_dir_all(path.parent().unwrap()).await?;
            tokio::fs::write(&path, serde_json::to_vec_pretty(&json)?).await?;
            Ok(serde_json::from_value(json)?)
        }
        Err(e) if path.exists() => {
            let _ = e;
            Ok(serde_json::from_str(&tokio::fs::read_to_string(&path).await?)?)
        }
        Err(e) => Err(Error::msg(format!("Не удалось получить профиль Fabric: {e}"))),
    }
}

/// Applies a Fabric profile on top of the vanilla version (`inheritsFrom`).
/// Fabric libraries come first so they win over duplicates.
pub fn merge(vanilla: VersionJson, fabric: VersionJson) -> VersionJson {
    let mut libraries = fabric.libraries;
    libraries.extend(vanilla.libraries);

    let arguments = match (vanilla.arguments, fabric.arguments) {
        (Some(mut v), Some(f)) => {
            v.jvm.extend(f.jvm);
            v.game.extend(f.game);
            Some(v)
        }
        (v, f) => v.or(f).or_else(|| Some(Arguments::default())),
    };

    VersionJson {
        id: fabric.id,
        inherits_from: Some(vanilla.id),
        main_class: fabric.main_class,
        arguments,
        minecraft_arguments: vanilla.minecraft_arguments,
        libraries,
        asset_index: vanilla.asset_index,
        assets: vanilla.assets,
        downloads: vanilla.downloads,
        java_version: vanilla.java_version,
        version_type: vanilla.version_type,
    }
}

fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

/// Removes `<prefix>*.jar` files in `mods/` except `keep`.
fn remove_other_versions(mods: &Path, prefix: &str, keep: &str) -> Result<()> {
    if let Ok(entries) = std::fs::read_dir(mods) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && name.ends_with(".jar") && name != keep {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

/// Download task for Fabric API (hash taken from the Maven `.sha1` file).
pub async fn fabric_api_task(client: &reqwest::Client, game_dir: &Path) -> Result<DownloadTask> {
    let v = config::FABRIC_API_VERSION;
    let file = format!("fabric-api-{v}.jar");
    let url = format!(
        "{}/net/fabricmc/fabric-api/fabric-api/{}/{}",
        config::FABRIC_MAVEN_URL,
        v.replace('+', "%2B"),
        file.replace('+', "%2B")
    );
    let path = mods_dir(game_dir).join(&file);
    let sha1 = match client.get(format!("{url}.sha1")).send().await {
        Ok(r) if r.status().is_success() => r.text().await.ok().map(|s| s.trim().to_string()),
        _ => None,
    };
    remove_other_versions(&mods_dir(game_dir), "fabric-api-", &file)?;
    Ok(DownloadTask { url, path, sha1, sha512: None, size: None })
}

pub async fn install_fabric_api(client: &reqwest::Client, game_dir: &Path) -> Result<()> {
    let task = fabric_api_task(client, game_dir).await?;
    if is_valid(&task, Verify::Full) {
        return Ok(());
    }
    if let Err(e) = fetch(client, &task).await {
        // Offline with an existing copy: keep going.
        if !task.path.exists() {
            return Err(e);
        }
    }
    Ok(())
}

/// Mod jar downloaded from the latest GitHub release.
pub fn downloaded_mod() -> PathBuf {
    paths::launcher_dir().join("mod").join(config::MOD_FILE_NAME)
}

/// Debug builds use the sibling mod project's build output when it exists.
#[cfg(debug_assertions)]
fn dev_mod() -> Option<PathBuf> {
    let libs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../elysium-visuals-template-26.2/build/libs");
    std::fs::read_dir(&libs).ok()?.flatten().map(|e| e.path()).find(|p| {
        let n = p.file_name().unwrap_or_default().to_string_lossy();
        n.starts_with(config::MOD_FILE_PREFIX) && n.ends_with(".jar") && !n.contains("sources")
    })
}

#[cfg(not(debug_assertions))]
fn dev_mod() -> Option<PathBuf> {
    None
}

/// The mod jar that gets installed into `mods/`, if one is available.
pub fn mod_source() -> Option<PathBuf> {
    dev_mod().or_else(|| Some(downloaded_mod()).filter(|p| p.is_file()))
}

/// Downloads the latest mod release when needed and copies it into `mods/`
/// when it differs from the installed copy.
pub async fn install_mod(client: &reqwest::Client, game_dir: &Path) -> Result<()> {
    if dev_mod().is_none() {
        crate::updates::sync_mod(client).await?;
    }
    let source = mod_source().ok_or_else(|| Error::msg("Файл мода Elysium Visuals не найден"))?;
    let mods = mods_dir(game_dir);
    std::fs::create_dir_all(&mods)?;
    let target = mods.join(config::MOD_FILE_NAME);
    let same = target.is_file() && sha1_file(&target).ok() == sha1_file(&source).ok();
    if !same {
        std::fs::copy(&source, &target)?;
    }
    remove_other_versions(&mods, config::MOD_FILE_PREFIX, config::MOD_FILE_NAME)?;
    Ok(())
}
