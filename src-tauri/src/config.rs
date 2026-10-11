//! Build-time constants: game versions, endpoints, OAuth client.

pub const LAUNCHER_NAME: &str = "ElysiumLauncher";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const MINECRAFT_VERSION: &str = "26.2";
pub const FABRIC_LOADER_VERSION: &str = "0.19.5";
pub const FABRIC_API_VERSION: &str = "0.161.0+26.2";
pub const JAVA_MAJOR: u32 = 25;

/// Version id of the merged Fabric profile inside `versions/`.
pub fn fabric_version_id() -> String {
    format!("fabric-loader-{FABRIC_LOADER_VERSION}-{MINECRAFT_VERSION}")
}

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
pub const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2";
pub const FABRIC_MAVEN_URL: &str = "https://maven.fabricmc.net";
pub const ADOPTIUM_API_URL: &str = "https://api.adoptium.net/v3";

/// Azure application (client) ID used for Microsoft sign-in.
/// Register your own app (see README) and paste its ID here, or set the
/// `ELYSIUM_MS_CLIENT_ID` environment variable at build time.
pub const MS_CLIENT_ID: &str = match option_env!("ELYSIUM_MS_CLIENT_ID") {
    Some(id) if !id.is_empty() => id,
    _ => "00000000-0000-0000-0000-000000000000",
};

pub fn ms_client_configured() -> bool {
    MS_CLIENT_ID != "00000000-0000-0000-0000-000000000000"
}

/// Elysium site: accounts API (`/api/v1`) and the pages opened in the browser (register, forgot…).
/// Override at build time with `ELYSIUM_SITE_URL` (debug builds may use http://127.0.0.1 for `wrangler dev`).
pub const ELYSIUM_SITE_URL: &str = match option_env!("ELYSIUM_SITE_URL") {
    Some(url) if !url.is_empty() => url,
    _ => "https://elysium-visuals.elysium-site.workers.dev",
};

/// GitHub repository ("owner/repo") whose latest release provides the mod jar.
/// Launcher updates come from the updater endpoints in `tauri.conf.json`.
pub const MOD_REPO: &str = "dejure-xvii/elysium-visuals";

/// File name of the mod inside `mods/`.
pub const MOD_FILE_NAME: &str = "elysium-visuals.jar";
pub const MOD_FILE_PREFIX: &str = "elysium-visuals";

pub const USER_AGENT: &str = concat!("ElysiumLauncher/", env!("CARGO_PKG_VERSION"));

/// List of mods installed alongside the client (Modrinth project/version ids).
pub const DEFAULT_MODS_URL: &str =
    "https://raw.githubusercontent.com/dejure-xvii/elysium-visuals/main/default-mods.json";
pub const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";
/// Only files from this host are downloaded for Modrinth mods.
pub const MODRINTH_CDN_PREFIX: &str = "https://cdn.modrinth.com/";
/// Fabric API is installed from Maven (`FABRIC_API_VERSION`), never from Modrinth.
pub const FABRIC_API_PROJECT: &str = "P7dR8mSH";
/// ffmpeg for the mod's Replay (installed only when the mod asks for it):
/// the official Windows essentials build by gyan.dev, checked against its published SHA-256.
pub const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
pub const FFMPEG_SHA256_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip.sha256";
