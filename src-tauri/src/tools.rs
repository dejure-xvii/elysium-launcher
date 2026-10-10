//! Extra tools the mod asks for through `<game dir>/launcher-requests.json`.
//!
//! The mod never ships third-party binaries. When a feature needs one (Replay
//! needs ffmpeg), the mod writes a request such as
//! `{"schema":1,"tools":{"ffmpeg":{"required":true,"dir":"tools/ffmpeg"}}}`
//! and the launcher fetches the official build before the next start, into
//! `<game dir>/tools/ffmpeg/bin`, where the mod looks for it. The fulfilled
//! request is removed from the file.

use std::io::Read;
use std::path::{Path, PathBuf};

use futures::TryStreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::config;
use crate::download::Reporter;
use crate::error::{Error, Result};

const REQUESTS_FILE: &str = "launcher-requests.json";
/// Files taken from the ffmpeg archive (the rest of it is not needed).
const FFMPEG_FILES: [&str; 2] = ["ffmpeg.exe", "ffprobe.exe"];

fn requests_path(game_dir: &Path) -> PathBuf {
    game_dir.join(REQUESTS_FILE)
}

pub fn ffmpeg_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("tools").join("ffmpeg").join("bin")
}

pub fn ffmpeg_installed(game_dir: &Path) -> bool {
    ffmpeg_dir(game_dir).join("ffmpeg.exe").is_file()
}

/// Whether the mod asked for ffmpeg (a missing or broken file means no).
pub fn ffmpeg_requested(game_dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(requests_path(game_dir)) else { return false };
    let Ok(root) = serde_json::from_str::<Value>(&text) else { return false };
    root.pointer("/tools/ffmpeg/required").and_then(Value::as_bool).unwrap_or(false)
}

/// Drops the ffmpeg request; deletes the file when nothing else is asked for.
fn clear_ffmpeg_request(game_dir: &Path) -> Result<()> {
    let path = requests_path(game_dir);
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(()) };
    let mut root: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let empty = match root.get_mut("tools").and_then(Value::as_object_mut) {
        Some(tools) => {
            tools.remove("ffmpeg");
            tools.is_empty()
        }
        None => true,
    };
    if empty {
        std::fs::remove_file(&path)?;
    } else {
        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    }
    Ok(())
}

/// Fulfils the mod's tool requests. Never fails the launch: problems are
/// shown as a status line and the request stays for the next start.
pub async fn sync(client: &reqwest::Client, game_dir: &Path, reporter: &Reporter) {
    if !ffmpeg_requested(game_dir) {
        return;
    }
    if ffmpeg_installed(game_dir) {
        let _ = clear_ffmpeg_request(game_dir);
        return;
    }
    match install_ffmpeg(client, game_dir, reporter).await {
        Ok(()) => {
            let _ = clear_ffmpeg_request(game_dir);
            reporter.status("tools", "ffmpeg установлен — Replay готов к записи");
        }
        Err(e) => {
            crate::launcher_update::log(&format!("ffmpeg install failed: {e}"));
            reporter.status("tools", format!("ffmpeg не скачан ({e}), попробуем при следующем запуске"));
        }
    }
}

/// Downloads the official essentials build, checks its SHA-256 against the
/// one published next to it, and unpacks ffmpeg.exe and ffprobe.exe.
async fn install_ffmpeg(client: &reqwest::Client, game_dir: &Path, reporter: &Reporter) -> Result<()> {
    reporter.status("tools", "Получение ffmpeg для Replay");
    let expected = client
        .get(config::FFMPEG_SHA256_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let expected = expected.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    if expected.len() != 64 {
        return Err(Error::msg("нет контрольной суммы ffmpeg"));
    }

    let tools = game_dir.join("tools");
    tokio::fs::create_dir_all(&tools).await?;
    let zip_path = tools.join("ffmpeg-download.zip");
    let resp = client.get(config::FFMPEG_URL).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
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
                "tools",
                format!("Скачивание ffmpeg — {mb}/{} МБ", total / 1_048_576),
                done as usize / 1024,
                (total as usize / 1024).max(1),
            );
        }
    }
    file.flush().await?;
    drop(file);
    if hex::encode(hasher.finalize()) != expected {
        let _ = tokio::fs::remove_file(&zip_path).await;
        return Err(Error::msg("контрольная сумма ffmpeg не совпала"));
    }

    reporter.status("tools", "Распаковка ffmpeg");
    let target = ffmpeg_dir(game_dir);
    let zip_clone = zip_path.clone();
    let result = tokio::task::spawn_blocking(move || extract_ffmpeg(&zip_clone, &target))
        .await
        .map_err(|e| Error::msg(e.to_string()))?;
    let _ = tokio::fs::remove_file(&zip_path).await;
    result
}

/// Copies `bin/ffmpeg.exe` and `bin/ffprobe.exe` out of the archive (whatever
/// its top folder is called) into `target`, via temporary names so a broken
/// unpack never leaves a half-written ffmpeg.exe behind.
fn extract_ffmpeg(zip_path: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    let mut found = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        let Some(file_name) = FFMPEG_FILES.iter().find(|f| name.ends_with(&format!("bin/{f}"))) else { continue };
        let tmp = target.join(format!("{file_name}.part"));
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        std::fs::write(&tmp, &buf)?;
        std::fs::rename(&tmp, target.join(file_name))?;
        found.push(*file_name);
    }
    if !found.contains(&"ffmpeg.exe") {
        return Err(Error::msg("в архиве нет ffmpeg.exe"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elysium-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_and_clears_requests() {
        let dir = temp_dir("requests");
        assert!(!ffmpeg_requested(&dir));
        std::fs::write(
            requests_path(&dir),
            r#"{"schema":1,"tools":{"ffmpeg":{"required":true,"dir":"tools/ffmpeg"},"other":{"required":true}}}"#,
        )
        .unwrap();
        assert!(ffmpeg_requested(&dir));
        clear_ffmpeg_request(&dir).unwrap();
        assert!(!ffmpeg_requested(&dir));
        // Other requests stay.
        let left: Value = serde_json::from_str(&std::fs::read_to_string(requests_path(&dir)).unwrap()).unwrap();
        assert!(left.pointer("/tools/other").is_some());

        std::fs::write(requests_path(&dir), r#"{"schema":1,"tools":{"ffmpeg":{"required":true}}}"#).unwrap();
        clear_ffmpeg_request(&dir).unwrap();
        assert!(!requests_path(&dir).exists(), "file removed when nothing else is requested");

        std::fs::write(requests_path(&dir), "not json").unwrap();
        assert!(!ffmpeg_requested(&dir), "a broken file is ignored");
    }

    #[test]
    fn extracts_only_the_binaries() {
        let dir = temp_dir("extract");
        let zip_path = dir.join("ffmpeg.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in [
                ("ffmpeg-9.0-essentials_build/bin/ffmpeg.exe", "ffmpeg"),
                ("ffmpeg-9.0-essentials_build/bin/ffprobe.exe", "ffprobe"),
                ("ffmpeg-9.0-essentials_build/bin/ffplay.exe", "ffplay"),
                ("ffmpeg-9.0-essentials_build/doc/readme.txt", "doc"),
            ] {
                w.start_file(name, opts).unwrap();
                w.write_all(data.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        let target = ffmpeg_dir(&dir);
        extract_ffmpeg(&zip_path, &target).unwrap();
        assert_eq!(std::fs::read_to_string(target.join("ffmpeg.exe")).unwrap(), "ffmpeg");
        assert_eq!(std::fs::read_to_string(target.join("ffprobe.exe")).unwrap(), "ffprobe");
        assert!(!target.join("ffplay.exe").exists());
        assert!(ffmpeg_installed(&dir));
    }

    /// Network (~115 MB): `cargo test ffmpeg_download -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn ffmpeg_download() {
        let dir = temp_dir("download");
        std::fs::write(requests_path(&dir), r#"{"schema":1,"tools":{"ffmpeg":{"required":true}}}"#).unwrap();
        let reporter = Reporter::from_fn(|e| {
            if e.total == 0 {
                println!("{}", e.label)
            }
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(sync(&crate::download::http_client(), &dir, &reporter));
        assert!(ffmpeg_installed(&dir));
        assert!(!requests_path(&dir).exists());
        let out = std::process::Command::new(ffmpeg_dir(&dir).join("ffmpeg.exe")).arg("-version").output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("ffmpeg version"));
    }
}
