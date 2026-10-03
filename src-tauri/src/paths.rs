//! Well-known directories.

use std::path::PathBuf;

/// `%APPDATA%` (Roaming).
pub fn appdata() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Launcher's own data: settings, accounts, Java runtime, downloaded mod updates.
pub fn launcher_dir() -> PathBuf {
    appdata().join("ElysiumLauncher")
}

pub fn default_game_dir() -> PathBuf {
    appdata().join(".elysium")
}

pub fn runtime_dir() -> PathBuf {
    launcher_dir().join("runtime")
}
