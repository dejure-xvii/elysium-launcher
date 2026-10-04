//! Automatic launcher updates: checks on start and every few hours, then
//! downloads, verifies (signature from `tauri.conf.json`) and installs the
//! update by itself — after the game is closed if it is running. Progress is
//! sent to the UI as `launcher-update` events; every step goes to `updater.log`.

use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::config;
use crate::error::{Error, Result};
use crate::launch::LaunchState;
use crate::paths;

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(4);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const WAIT_FOR_GAME: Duration = Duration::from_secs(30);

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Idle,
    Checking,
    UpToDate,
    /// Found; installs as soon as the game is closed.
    WaitingForGame,
    Downloading,
    Installing,
    Error,
}

/// What the updater is doing, for the Settings page and toasts.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterStatus {
    pub stage: Stage,
    pub current_version: String,
    /// Version being installed / waiting to be installed.
    pub version: Option<String>,
    /// Error text for `Error`.
    pub message: Option<String>,
    /// 0..1 while downloading, if the size is known.
    pub progress: Option<f32>,
    /// Unix ms of the last finished check.
    pub checked_at: Option<u64>,
}

pub struct UpdaterState {
    status: Mutex<UpdaterStatus>,
    /// Serializes checks/installs (automatic and manual).
    running: tokio::sync::Mutex<()>,
}

impl Default for UpdaterState {
    fn default() -> Self {
        UpdaterState {
            status: Mutex::new(UpdaterStatus {
                stage: Stage::Idle,
                current_version: config::LAUNCHER_VERSION.to_string(),
                version: None,
                message: None,
                progress: None,
                checked_at: None,
            }),
            running: tokio::sync::Mutex::new(()),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Appends a line to `%APPDATA%/ElysiumLauncher/updater.log` (kept small).
pub fn log(line: &str) {
    let path = paths::launcher_dir().join("updater.log");
    if std::fs::metadata(&path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{}] v{} {line}", now_ms() / 1000, config::LAUNCHER_VERSION);
    }
}

fn set(app: &AppHandle, f: impl FnOnce(&mut UpdaterStatus)) {
    let state = app.state::<UpdaterState>();
    let status = {
        let mut s = state.status.lock().unwrap();
        f(&mut s);
        s.clone()
    };
    let _ = app.emit("launcher-update", status);
}

fn updater(app: &AppHandle) -> Result<Updater> {
    #[allow(unused_mut)]
    let mut builder = app.updater_builder().timeout(Duration::from_secs(60));
    // Debug builds can point at a test feed: ELYSIUM_UPDATE_ENDPOINT=http://127.0.0.1:8123/latest.json
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("ELYSIUM_UPDATE_ENDPOINT") {
        let url = url.parse::<tauri::Url>().map_err(|e| Error::msg(e.to_string()))?;
        builder = builder.endpoints(vec![url])?;
    }
    // ...and treat the announced version as newer, to test with a real signed release.
    #[cfg(debug_assertions)]
    if std::env::var("ELYSIUM_UPDATE_FORCE").is_ok() {
        builder = builder.version_comparator(|_, _| true);
    }
    Ok(builder.build()?)
}

/// Checks the update feed and records the result.
async fn check(app: &AppHandle) -> Result<Option<Update>> {
    set(app, |s| {
        s.stage = Stage::Checking;
        s.message = None;
    });
    match updater(app)?.check().await {
        Ok(update) => {
            match &update {
                Some(u) => log(&format!("update available: {}", u.version)),
                None => log("up to date"),
            }
            set(app, |s| {
                s.checked_at = Some(now_ms());
                s.stage = if update.is_some() { Stage::WaitingForGame } else { Stage::UpToDate };
                s.version = update.as_ref().map(|u| u.version.clone());
            });
            Ok(update)
        }
        Err(e) => {
            let message = format!("Не удалось проверить обновления лаунчера: {e}");
            log(&message);
            set(app, |s| {
                s.checked_at = Some(now_ms());
                s.stage = Stage::Error;
                s.message = Some(message.clone());
            });
            Err(Error::msg(message))
        }
    }
}

/// Downloads, verifies and installs; the installer restarts the launcher.
/// Waits while the game is running or being installed.
async fn install(app: &AppHandle, update: Update) -> Result<()> {
    let launch = app.state::<LaunchState>();
    loop {
        // Block launching first, then make sure nothing is running already.
        launch.updating.store(true, Ordering::SeqCst);
        if !launch.busy.load(Ordering::SeqCst) {
            break;
        }
        launch.updating.store(false, Ordering::SeqCst);
        set(app, |s| s.stage = Stage::WaitingForGame);
        tokio::time::sleep(WAIT_FOR_GAME).await;
    }

    let version = update.version.clone();
    log(&format!("downloading {version}"));
    set(app, |s| {
        s.stage = Stage::Downloading;
        s.version = Some(version.clone());
        s.progress = Some(0.0);
    });
    let mut done = 0u64;
    let mut last_emit = 0f32;
    let result = update
        .download(
            |chunk, total| {
                done += chunk as u64;
                if let Some(total) = total.filter(|t| *t > 0) {
                    let p = (done as f32 / total as f32).min(1.0);
                    if p - last_emit >= 0.02 || p >= 1.0 {
                        last_emit = p;
                        set(app, |s| s.progress = Some(p));
                    }
                }
            },
            || {},
        )
        .await;
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(e) => {
            launch.updating.store(false, Ordering::SeqCst);
            let message = format!("Не удалось скачать обновление {version}: {e}");
            log(&message);
            set(app, |s| {
                s.stage = Stage::Error;
                s.message = Some(message.clone());
                s.progress = None;
            });
            return Err(Error::msg(message));
        }
    };
    log(&format!("downloaded and verified {version} ({} bytes)", bytes.len()));

    #[cfg(debug_assertions)]
    if std::env::var("ELYSIUM_UPDATE_DRY_RUN").is_ok() {
        launch.updating.store(false, Ordering::SeqCst);
        log("dry run: not installing");
        set(app, |s| {
            s.stage = Stage::UpToDate;
            s.progress = None;
        });
        return Ok(());
    }

    set(app, |s| {
        s.stage = Stage::Installing;
        s.progress = None;
    });
    log(&format!("installing {version}"));
    if let Err(e) = update.install(bytes) {
        launch.updating.store(false, Ordering::SeqCst);
        let message = format!("Не удалось установить обновление {version}: {e}");
        log(&message);
        set(app, |s| {
            s.stage = Stage::Error;
            s.message = Some(message.clone());
        });
        return Err(Error::msg(message));
    }
    // On Windows the installer has taken over; make sure we restart into the new version.
    app.restart();
}

/// One automatic round: check, and install if there is an update.
async fn auto_round(app: &AppHandle) {
    let state = app.state::<UpdaterState>();
    let _running = state.running.lock().await;
    if let Ok(Some(update)) = check(app).await {
        let _ = install(app, update).await;
    }
}

/// Starts the background updater (called once from setup).
pub fn start(app: AppHandle) {
    log("launcher started");
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            auto_round(&app).await;
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

#[tauri::command]
pub fn launcher_update_status(state: State<UpdaterState>) -> UpdaterStatus {
    state.status.lock().unwrap().clone()
}

/// "Проверить обновления": checks now and, if there is an update, installs it
/// (after the game is closed). Returns the version found, if any.
#[tauri::command]
pub async fn check_launcher_update(app: AppHandle) -> Result<Option<String>> {
    let state = app.state::<UpdaterState>();
    let Ok(_running) = state.running.try_lock() else {
        // An automatic round is already busy; its progress arrives as events.
        return Ok(state.status.lock().unwrap().version.clone());
    };
    match check(&app).await? {
        Some(update) => {
            let version = update.version.clone();
            let app2 = app.clone();
            drop(_running);
            tauri::async_runtime::spawn(async move {
                let state = app2.state::<UpdaterState>();
                let _running = state.running.lock().await;
                let _ = install(&app2, update).await;
            });
            Ok(Some(version))
        }
        None => Ok(None),
    }
}
