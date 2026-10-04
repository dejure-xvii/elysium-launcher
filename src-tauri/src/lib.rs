mod auth;
mod config;
mod download;
mod error;
mod fabric;
mod launch;
mod mods;
mod paths;
mod settings;
mod system;
mod updates;

use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use error::{Error, Result};
use settings::SettingsState;

fn open(app: &AppHandle, path: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::msg(e.to_string()))
}

#[tauri::command]
fn open_game_dir(app: AppHandle, settings: State<SettingsState>) -> Result<()> {
    let dir = settings.0.lock().unwrap().game_dir();
    open(&app, &dir)
}

#[tauri::command]
fn open_mods_dir(app: AppHandle, settings: State<SettingsState>) -> Result<()> {
    let dir = settings.0.lock().unwrap().game_dir().join("mods");
    open(&app, &dir)
}

#[tauri::command]
fn open_logs_dir(app: AppHandle, settings: State<SettingsState>) -> Result<()> {
    let dir = settings.0.lock().unwrap().game_dir().join("logs");
    open(&app, &dir)
}

/// Opens a log or crash report in the default text editor.
#[tauri::command]
fn open_file(app: AppHandle, settings: State<SettingsState>, path: String) -> Result<()> {
    let game_dir = settings.0.lock().unwrap().game_dir();
    let path = std::path::PathBuf::from(path);
    if !path.starts_with(&game_dir) {
        return Err(Error::msg("Можно открывать только файлы из папки игры"));
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| Error::msg(e.to_string()))
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<()> {
    if !url.starts_with("https://") {
        return Err(Error::msg("Разрешены только https-ссылки"));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| Error::msg(e.to_string()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(SettingsState(Mutex::new(settings::load())))
        .manage(auth::AuthState::load())
        .manage(launch::LaunchState::default())
        .setup(|app| {
            std::fs::create_dir_all(paths::launcher_dir())?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("Elysium Launcher");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::set_settings,
            system::system_info,
            auth::list_accounts,
            auth::add_offline_account,
            auth::remove_account,
            auth::select_account,
            auth::ms_login_start,
            auth::ms_login_finish,
            auth::ms_login_cancel,
            launch::launch_game,
            launch::reinstall_files,
            launch::stop_game,
            launch::is_busy,
            mods::list_mods,
            mods::set_mod_enabled,
            updates::check_updates,
            updates::update_mod,
            updates::update_launcher,
            open_game_dir,
            open_mods_dir,
            open_logs_dir,
            open_file,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Elysium Launcher");
}
