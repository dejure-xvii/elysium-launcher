//! Persistent launcher settings (`%APPDATA%/ElysiumLauncher/settings.json`).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::Result;
use crate::paths;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AfterLaunch {
    Minimize,
    Close,
    Keep,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub memory_mb: u64,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub game_dir: String,
    pub jvm_args: String,
    pub after_launch: AfterLaunch,
    pub selected_account: Option<String>,
    /// Modrinth project ids of optional built-in mods the user switched off.
    pub disabled_mods: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let total = crate::system::total_memory_mb();
        Settings {
            memory_mb: crate::system::recommended_memory_mb(total),
            width: 1280,
            height: 720,
            fullscreen: false,
            game_dir: paths::default_game_dir().to_string_lossy().into_owned(),
            jvm_args: String::new(),
            after_launch: AfterLaunch::Minimize,
            selected_account: None,
            disabled_mods: Vec::new(),
        }
    }
}

impl Settings {
    /// Game folder; relative or empty paths fall back to the default.
    pub fn game_dir(&self) -> PathBuf {
        let path = PathBuf::from(self.game_dir.trim());
        if path.is_absolute() {
            path
        } else {
            paths::default_game_dir()
        }
    }
}

pub struct SettingsState(pub Mutex<Settings>);

fn file() -> PathBuf {
    paths::launcher_dir().join("settings.json")
}

pub fn load() -> Settings {
    let mut settings: Settings = std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    settings.game_dir = settings.game_dir().to_string_lossy().into_owned();
    settings
}

pub fn save(settings: &Settings) -> Result<()> {
    std::fs::create_dir_all(paths::launcher_dir())?;
    let tmp = file().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(tmp, file())?;
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<SettingsState>) -> Settings {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(state: State<SettingsState>, settings: Settings) -> Result<Settings> {
    let mut settings = settings;
    let total = crate::system::total_memory_mb();
    settings.memory_mb = settings
        .memory_mb
        .clamp(2048, crate::system::max_memory_mb(total));
    settings.width = settings.width.clamp(320, 7680);
    settings.height = settings.height.clamp(240, 4320);
    save(&settings)?;
    *state.0.lock().unwrap() = settings.clone();
    Ok(settings)
}
