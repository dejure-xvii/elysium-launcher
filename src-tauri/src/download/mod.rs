//! Parallel, hash-verified file downloads with progress reporting.

pub mod assets;
pub mod java;
pub mod libraries;
pub mod manifest;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{stream, StreamExt, TryStreamExt};
use serde::Serialize;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::error::{Error, Result};

const PARALLEL: usize = 16;
const RETRIES: usize = 3;

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(crate::config::USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("http client")
}

#[derive(Clone, Debug)]
pub struct DownloadTask {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub size: Option<u64>,
}

/// How strictly existing files are checked.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Verify {
    /// SHA-1 for files that have one.
    Full,
    /// Size only (used for content-addressed assets on normal launches).
    SizeOnly,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub stage: &'static str,
    pub label: String,
    pub current: usize,
    pub total: usize,
}

/// Progress sink; in the app it emits `launch-progress` events to the UI.
#[derive(Clone)]
pub struct Reporter {
    sink: Arc<dyn Fn(ProgressEvent) + Send + Sync>,
}

impl Reporter {
    pub fn new(app: AppHandle) -> Self {
        Reporter {
            sink: Arc::new(move |e| {
                let _ = app.emit("launch-progress", e);
            }),
        }
    }

    #[cfg(test)]
    pub fn from_fn(f: impl Fn(ProgressEvent) + Send + Sync + 'static) -> Self {
        Reporter { sink: Arc::new(f) }
    }

    pub fn report(&self, stage: &'static str, label: impl Into<String>, current: usize, total: usize) {
        (self.sink)(ProgressEvent { stage, label: label.into(), current, total });
    }

    pub fn status(&self, stage: &'static str, label: impl Into<String>) {
        self.report(stage, label, 0, 0);
    }
}

pub fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha512_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha512::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Whether `task.path` already holds the expected file.
pub fn is_valid(task: &DownloadTask, verify: Verify) -> bool {
    let Ok(meta) = std::fs::metadata(&task.path) else { return false };
    if !meta.is_file() {
        return false;
    }
    if let Some(size) = task.size {
        if meta.len() != size {
            return false;
        }
    }
    match (&task.sha512, &task.sha1, verify) {
        (Some(expected), _, Verify::Full) => sha512_file(&task.path)
            .map(|h| h.eq_ignore_ascii_case(expected))
            .unwrap_or(false),
        (None, Some(expected), Verify::Full) => sha1_file(&task.path)
            .map(|h| h.eq_ignore_ascii_case(expected))
            .unwrap_or(false),
        // Size (if known) already matched; otherwise only reject empty leftovers.
        _ => task.size.is_some() || meta.len() > 0,
    }
}

async fn fetch_once(client: &reqwest::Client, task: &DownloadTask) -> Result<()> {
    if let Some(parent) = task.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = task.path.with_extension("part");
    let resp = client.get(&task.url).send().await?.error_for_status()?;
    let mut body = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha1::new();
    let mut hasher512 = task.sha512.as_ref().map(|_| Sha512::new());
    let mut written: u64 = 0;
    while let Some(chunk) = body.try_next().await? {
        hasher.update(&chunk);
        if let Some(h) = hasher512.as_mut() {
            h.update(&chunk);
        }
        written += chunk.len() as u64;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    if let Some(size) = task.size {
        if written != size {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::msg(format!(
                "Размер не совпал для {} ({written} вместо {size})",
                task.url
            )));
        }
    }
    if let Some(expected) = &task.sha1 {
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::msg(format!("Хэш не совпал для {}", task.url)));
        }
    }
    if let (Some(expected), Some(h)) = (&task.sha512, hasher512) {
        let actual = hex::encode(h.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(Error::msg(format!("Хэш не совпал для {}", task.url)));
        }
    }
    tokio::fs::rename(&part, &task.path).await?;
    Ok(())
}

pub async fn fetch(client: &reqwest::Client, task: &DownloadTask) -> Result<()> {
    let mut last = None;
    for attempt in 0..RETRIES {
        match fetch_once(client, task).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
            }
        }
    }
    Err(last.unwrap())
}

/// Checks every task, downloads only missing/corrupted files.
/// `on_progress(done, total)` is called after each file.
pub async fn download_all<F>(
    client: &reqwest::Client,
    tasks: Vec<DownloadTask>,
    verify: Verify,
    on_progress: F,
) -> Result<()>
where
    F: Fn(usize, usize) + Send + Sync,
{
    let total = tasks.len();
    let done = Arc::new(AtomicUsize::new(0));
    on_progress(0, total);

    stream::iter(tasks)
        .map(|task| {
            let done = done.clone();
            let on_progress = &on_progress;
            async move {
                let check = task.clone();
                let valid = tokio::task::spawn_blocking(move || is_valid(&check, verify))
                    .await
                    .unwrap_or(false);
                if !valid {
                    fetch(client, &task).await?;
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                on_progress(n, total);
                Ok::<_, Error>(())
            }
        })
        .buffer_unordered(PARALLEL)
        .try_collect::<Vec<_>>()
        .await?;
    Ok(())
}

pub async fn get_json<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    Ok(client.get(url).send().await?.error_for_status()?.json().await?)
}
