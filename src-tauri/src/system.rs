//! Host information used by the settings page.

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub total_memory_mb: u64,
    /// Upper bound for the RAM slider (leaves room for the OS).
    pub max_memory_mb: u64,
    pub recommended_memory_mb: u64,
    pub launcher_version: String,
    pub ms_login_available: bool,
}

pub fn total_memory_mb() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory() / 1024 / 1024
}

pub fn recommended_memory_mb(total_mb: u64) -> u64 {
    // Half the RAM, clamped to 4..8 GB, but never above the slider max.
    let half = total_mb / 2;
    let rec = half.clamp(4096, 8192);
    let rec = rec.min(max_memory_mb(total_mb));
    rec / 512 * 512
}

pub fn max_memory_mb(total_mb: u64) -> u64 {
    let max = total_mb.saturating_sub(2048).max(2048);
    max / 512 * 512
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let total = total_memory_mb();
    SystemInfo {
        total_memory_mb: total,
        max_memory_mb: max_memory_mb(total),
        recommended_memory_mb: recommended_memory_mb(total),
        launcher_version: crate::config::LAUNCHER_VERSION.to_string(),
        ms_login_available: crate::config::ms_client_configured(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_bounds() {
        assert_eq!(max_memory_mb(16384), 14336);
        assert_eq!(recommended_memory_mb(16384), 8192);
        assert_eq!(recommended_memory_mb(8192), 4096);
        assert_eq!(max_memory_mb(4096), 2048);
        assert_eq!(recommended_memory_mb(4096), 2048);
    }
}
