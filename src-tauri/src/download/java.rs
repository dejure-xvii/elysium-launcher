//! Locates a suitable Java runtime or downloads portable Temurin.

use std::path::{Path, PathBuf};

use futures::TryStreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::{get_json, Reporter};
use crate::config;
use crate::error::{Error, Result};
use crate::paths;

fn runtime_home(major: u32) -> PathBuf {
    paths::runtime_dir().join(format!("temurin-{major}"))
}

/// Reads the major version from `<home>/release` (`JAVA_VERSION="25.0.1"`).
fn home_major(home: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(home.join("release")).ok()?;
    let line = text.lines().find(|l| l.starts_with("JAVA_VERSION="))?;
    let v = line.trim_start_matches("JAVA_VERSION=").trim_matches('"');
    let first = v.split(['.', '+', '-']).next()?;
    let major: u32 = first.parse().ok()?;
    // Old "1.8" style.
    if major == 1 {
        v.split('.').nth(1)?.parse().ok()
    } else {
        Some(major)
    }
}

/// `javaw.exe` (no console window) inside a Java home.
fn javaw(home: &Path) -> Option<PathBuf> {
    let p = home.join("bin").join("javaw.exe");
    p.is_file().then_some(p)
}

/// Finds a Java home directly inside `dir` or one level below (zip layout).
fn find_home_in(dir: &Path) -> Option<PathBuf> {
    if javaw(dir).is_some() {
        return Some(dir.to_path_buf());
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && javaw(p).is_some())
}

fn check_home(home: &Path, major: u32) -> Option<PathBuf> {
    match home_major(home) {
        Some(m) if m >= major => javaw(home),
        _ => None,
    }
}

/// Bundled runtime first, then JAVA_HOME, then PATH.
pub fn find_java(major: u32) -> Option<PathBuf> {
    if let Some(home) = find_home_in(&runtime_home(major)) {
        if let Some(j) = check_home(&home, major) {
            return Some(j);
        }
    }
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if let Some(j) = check_home(Path::new(&home), major) {
            return Some(j);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join("java.exe").is_file() {
                if let Some(home) = dir.parent() {
                    if let Some(j) = check_home(home, major) {
                        return Some(j);
                    }
                }
            }
        }
    }
    None
}

#[derive(Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: Option<String>,
    size: Option<u64>,
}

/// Returns `javaw.exe` for Java `major`, downloading Temurin JRE when missing.
pub async fn ensure_java(client: &reqwest::Client, major: u32, reporter: &Reporter) -> Result<PathBuf> {
    reporter.status("java", format!("Поиск Java {major}"));
    if let Some(j) = find_java(major) {
        return Ok(j);
    }
    download_temurin(client, major, reporter).await
}

/// Downloads Temurin JRE `major` into the launcher's runtime folder.
async fn download_temurin(client: &reqwest::Client, major: u32, reporter: &Reporter) -> Result<PathBuf> {
    reporter.status("java", format!("Получение Java {major}"));
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let url = format!(
        "{}/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os=windows&vendor=eclipse",
        config::ADOPTIUM_API_URL
    );
    let assets: Vec<AdoptiumAsset> = get_json(client, &url).await?;
    let pkg = assets
        .into_iter()
        .next()
        .map(|a| a.binary.package)
        .ok_or_else(|| Error::msg(format!("Не удалось найти Temurin {major} для Windows")))?;

    let runtime = paths::runtime_dir();
    tokio::fs::create_dir_all(&runtime).await?;
    let zip_path = runtime.join(format!("temurin-{major}.zip"));

    // Stream with byte progress and SHA-256 verification.
    let resp = client.get(&pkg.link).send().await?.error_for_status()?;
    let total = pkg.size.or(resp.content_length()).unwrap_or(0);
    let mut body = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&zip_path).await?;
    let mut hasher = Sha256::new();
    let mut done: u64 = 0;
    let mut last_mb = u64::MAX;
    while let Some(chunk) = body.try_next().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        let mb = done / 1_048_576;
        if mb != last_mb {
            last_mb = mb;
            reporter.report(
                "java",
                format!("Скачивание Java {major} — {mb}/{} МБ", total / 1_048_576),
                done as usize / 1024,
                (total as usize / 1024).max(1),
            );
        }
    }
    file.flush().await?;
    drop(file);
    if let Some(expected) = pkg.checksum {
        if !hex::encode(hasher.finalize()).eq_ignore_ascii_case(&expected) {
            let _ = tokio::fs::remove_file(&zip_path).await;
            return Err(Error::msg("Контрольная сумма Java не совпала, попробуйте ещё раз"));
        }
    }

    reporter.status("java", format!("Распаковка Java {major}"));
    let target = runtime_home(major);
    let zip_clone = zip_path.clone();
    let target_clone = target.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if target_clone.exists() {
            std::fs::remove_dir_all(&target_clone)?;
        }
        let file = std::fs::File::open(&zip_clone)?;
        zip::ZipArchive::new(file)?.extract(&target_clone)?;
        Ok(())
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;
    let _ = tokio::fs::remove_file(&zip_path).await;

    find_home_in(&target)
        .and_then(|h| javaw(&h))
        .ok_or_else(|| Error::msg("Java распакована, но javaw.exe не найден"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_file() {
        let dir = std::env::temp_dir().join("elysium-java-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("release"), "IMPLEMENTOR=\"Eclipse\"\nJAVA_VERSION=\"25.0.4.1\"\n").unwrap();
        assert_eq!(home_major(&dir), Some(25));
        std::fs::write(dir.join("release"), "JAVA_VERSION=\"1.8.0_402\"\n").unwrap();
        assert_eq!(home_major(&dir), Some(8));
    }

    /// Network: `cargo test temurin -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn temurin_download() {
        let reporter = Reporter::from_fn(|e| {
            if e.total == 0 {
                println!("{}", e.label)
            }
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        let java = rt
            .block_on(download_temurin(&super::super::http_client(), config::JAVA_MAJOR, &reporter))
            .expect("download");
        println!("{}", java.display());
        let home = java.parent().unwrap().parent().unwrap();
        assert_eq!(home_major(home), Some(config::JAVA_MAJOR));
        assert_eq!(find_java(config::JAVA_MAJOR).as_deref(), Some(java.as_path()));
    }
}
