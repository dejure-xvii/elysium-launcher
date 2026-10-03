//! Spawns the game, streams its output and reports how it exited.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::time::SystemTime;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

use super::LaunchState;
use crate::error::{Error, Result};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub line: String,
    pub stderr: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitEvent {
    pub code: Option<i32>,
    pub crashed: bool,
    pub crash_report: Option<String>,
    pub log_file: Option<String>,
}

fn pump<R: AsyncRead + Unpin + Send + 'static>(app: AppHandle, reader: R, stderr: bool, secret: String) {
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(reader).split(b'\n');
        while let Ok(Some(bytes)) = lines.next_segment().await {
            let mut line = String::from_utf8_lossy(&bytes).trim_end_matches('\r').to_string();
            if !secret.is_empty() && line.contains(&secret) {
                line = line.replace(&secret, "<скрыто>");
            }
            let _ = app.emit("game-log", LogLine { line, stderr });
        }
    });
}

/// Newest crash report created after `since`.
fn find_crash_report(game_dir: &Path, since: SystemTime) -> Option<PathBuf> {
    std::fs::read_dir(game_dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            (modified >= since).then(|| (modified, e.path()))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

pub async fn spawn(
    app: &AppHandle,
    java: &Path,
    game_dir: &Path,
    jvm: Vec<String>,
    game: Vec<String>,
    secret: &str,
    piped: bool,
) -> Result<()> {
    let mut cmd = Command::new(java);
    cmd.args(&jvm)
        .args(&game)
        .current_dir(game_dir)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(false);
    if piped {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let started = SystemTime::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::msg(format!("Не удалось запустить Java ({}): {e}", java.display())))?;

    let _ = app.emit(
        "game-log",
        LogLine { line: format!("[Launcher] Запуск: {}", java.display()), stderr: false },
    );
    if let Some(out) = child.stdout.take() {
        pump(app.clone(), out, false, secret.to_string());
    }
    if let Some(err) = child.stderr.take() {
        pump(app.clone(), err, true, secret.to_string());
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    *app.state::<LaunchState>().kill.lock().unwrap() = Some(tx);

    let app = app.clone();
    let game_dir = game_dir.to_path_buf();
    tauri::async_runtime::spawn(async move {
        let status = tokio::select! {
            status = child.wait() => status.ok(),
            _ = rx.recv() => {
                let _ = child.kill().await;
                None
            }
        };
        let killed = status.is_none();
        let code = status.and_then(|s| s.code());
        let crash_report = find_crash_report(&game_dir, started);
        let crashed = !killed && (code.unwrap_or(0) != 0 || crash_report.is_some());
        let log_file = game_dir.join("logs").join("latest.log");

        let state = app.state::<LaunchState>();
        *state.kill.lock().unwrap() = None;
        state.busy.store(false, Ordering::SeqCst);

        let _ = app.emit(
            "game-exit",
            ExitEvent {
                code,
                crashed,
                crash_report: crash_report.map(|p| p.to_string_lossy().into_owned()),
                log_file: log_file.is_file().then(|| log_file.to_string_lossy().into_owned()),
            },
        );
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            if crashed {
                let _ = window.set_focus();
            }
        }
    });
    Ok(())
}
