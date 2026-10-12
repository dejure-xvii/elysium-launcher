//! Installation pipeline and game launch.

pub mod args;
pub mod process;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth::{self, LaunchAuth};
use crate::config;
use crate::elysium::{self, ElysiumState};
use crate::download::manifest::{version_dir, Features, VersionJson};
use crate::download::{self, assets, java, libraries, DownloadTask, Reporter, Verify};
use crate::error::{Error, Result};
use crate::fabric;
use crate::mods;
use crate::settings::{AfterLaunch, Settings, SettingsState};

#[derive(Default)]
pub struct LaunchState {
    /// Installing or running.
    pub busy: AtomicBool,
    /// A launcher update is being downloaded/installed; launching waits for it.
    pub updating: AtomicBool,
    pub kill: Mutex<Option<tokio::sync::mpsc::Sender<()>>>,
}

/// Clears `busy` when dropped, unless the game process took over.
struct BusyGuard<'a> {
    flag: &'a AtomicBool,
    armed: bool,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.flag.store(false, Ordering::SeqCst);
        }
    }
}

fn acquire(state: &LaunchState) -> Result<BusyGuard<'_>> {
    if state.updating.load(Ordering::SeqCst) {
        return Err(Error::msg("Устанавливается обновление лаунчера, подождите немного"));
    }
    if state.busy.swap(true, Ordering::SeqCst) {
        return Err(Error::msg("Игра уже устанавливается или запущена"));
    }
    Ok(BusyGuard { flag: &state.busy, armed: true })
}

/// Everything needed to start the game after installation.
pub struct Prepared {
    pub java: PathBuf,
    pub version: VersionJson,
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_index: String,
}

/// Downloads/verifies Java, Minecraft, Fabric, libraries, assets and mods.
pub async fn prepare(
    reporter: &Reporter,
    game_dir: &Path,
    disabled_mods: &[String],
    force: bool,
) -> Result<Prepared> {
    let client = download::http_client();
    let verify_assets = if force { Verify::Full } else { Verify::SizeOnly };
    std::fs::create_dir_all(game_dir)?;

    reporter.status("manifest", format!("Получение Minecraft {}", config::MINECRAFT_VERSION));
    let vanilla = download::manifest::fetch_vanilla(&client, game_dir).await?;
    let java_major = vanilla
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(config::JAVA_MAJOR);

    let java = java::ensure_java(&client, java_major, reporter).await?;

    // Client jar.
    let client_task = vanilla
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
        .map(|a| DownloadTask {
            url: a.url.clone(),
            path: version_dir(game_dir, &vanilla.id).join(format!("{}.jar", vanilla.id)),
            sha1: a.sha1.clone(),
            size: a.size,
            sha512: None,
            sha256: None,
        })
        .ok_or_else(|| Error::msg("В описании версии нет клиента"))?;
    let client_jar = client_task.path.clone();
    reporter.status("client", "Скачивание клиента Minecraft");
    download::download_all(&client, vec![client_task], Verify::Full, |_, _| {}).await?;

    reporter.status("fabric", "Установка Fabric");
    let profile = fabric::fetch_profile(&client, game_dir).await?;
    let version = fabric::merge(vanilla, profile);

    let libs = libraries::resolve(&version, game_dir);
    {
        let r = reporter.clone();
        download::download_all(&client, libs.downloads, Verify::Full, move |n, t| {
            r.report("libraries", format!("Скачивание библиотек {n}/{t}"), n, t)
        })
        .await?;
    }

    reporter.status("assets", "Проверка ресурсов");
    let (assets_index, asset_tasks) = assets::resolve(&client, &version, game_dir).await?;
    {
        let r = reporter.clone();
        download::download_all(&client, asset_tasks, verify_assets, move |n, t| {
            r.report("assets", format!("Скачивание ресурсов {n}/{t}"), n, t)
        })
        .await?;
    }

    reporter.status("natives", "Распаковка нативных библиотек");
    let natives_dir = version_dir(game_dir, &version.id).join("natives");
    if force && natives_dir.exists() {
        std::fs::remove_dir_all(&natives_dir)?;
    }
    let jars = libs.native_jars.clone();
    let nd = natives_dir.clone();
    tokio::task::spawn_blocking(move || libraries::extract_natives(&jars, &nd))
        .await
        .map_err(|e| Error::msg(e.to_string()))??;

    reporter.status("mods", "Установка Fabric API и Elysium Visuals");
    fabric::install_fabric_api(&client, game_dir).await?;
    fabric::install_mod(&client, game_dir).await?;
    reporter.status("mods", "Установка модов");
    mods::sync(&client, game_dir, disabled_mods, reporter).await?;
    // Tools the mod asked for (ffmpeg for Replay); never blocks the launch.
    crate::tools::sync(&client, game_dir, reporter).await;

    let mut classpath = libs.classpath;
    classpath.push(client_jar);
    Ok(Prepared { java, version, classpath, natives_dir, assets_index })
}

fn build_command(p: &Prepared, s: &Settings, auth: &LaunchAuth, game_dir: &Path) -> (Vec<String>, Vec<String>) {
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let classpath = p.classpath.iter().map(|c| path(c)).collect::<Vec<_>>().join(";");
    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("auth_player_name", auth.username.clone());
    vars.insert("auth_uuid", auth.uuid.clone());
    vars.insert("auth_access_token", auth.access_token.clone());
    vars.insert("auth_session", auth.access_token.clone());
    vars.insert("auth_xuid", auth.xuid.clone());
    vars.insert("clientid", String::new());
    vars.insert("user_type", auth.user_type.to_string());
    vars.insert("user_properties", "{}".into());
    vars.insert("version_name", p.version.id.clone());
    vars.insert("version_type", "Elysium".into());
    vars.insert("game_directory", path(game_dir));
    vars.insert("assets_root", path(&game_dir.join("assets")));
    vars.insert("game_assets", path(&game_dir.join("assets")));
    vars.insert("assets_index_name", p.assets_index.clone());
    vars.insert("library_directory", path(&game_dir.join("libraries")));
    vars.insert("natives_directory", path(&p.natives_dir));
    vars.insert("classpath", classpath);
    vars.insert("classpath_separator", ";".into());
    vars.insert("launcher_name", config::LAUNCHER_NAME.into());
    vars.insert("launcher_version", config::LAUNCHER_VERSION.into());
    vars.insert("resolution_width", s.width.to_string());
    vars.insert("resolution_height", s.height.to_string());

    let features = Features { custom_resolution: !s.fullscreen };
    let cmd = args::build(&p.version, &features, &vars);

    let mut jvm = vec![
        format!("-Xmx{}M", s.memory_mb),
        format!("-Xms{}M", s.memory_mb.min(1024)),
        "-Dstdout.encoding=UTF-8".into(),
        "-Dstderr.encoding=UTF-8".into(),
        "-Dfile.encoding=UTF-8".into(),
    ];
    jvm.extend(args::split_user_args(&s.jvm_args));
    jvm.extend(cmd.jvm);
    jvm.push(p.version.main_class.clone());

    let mut game = cmd.game;
    if s.fullscreen {
        game.push("--fullscreen".into());
    }
    (jvm, game)
}

/// Elysium Social sign-in for the game: a one-time ticket in the environment (and the server address in
/// dev builds). Without an Elysium account, or when the site is unreachable, the game starts without it.
async fn social_env(app: &AppHandle) -> (Vec<(&'static str, String)>, Vec<String>) {
    let mut env = Vec::new();
    let mut secrets = Vec::new();
    match elysium::game_ticket(&app.state::<ElysiumState>()).await {
        Ok(Some(ticket)) => {
            secrets.push(ticket.clone());
            env.push((process::SOCIAL_TICKET_ENV, ticket));
        }
        Ok(None) => {}
        Err(e) => {
            let _ = app.emit(
                "game-log",
                process::LogLine { line: format!("[Launcher] Социальные функции Elysium недоступны: {e}"), stderr: true },
            );
        }
    }
    if let Some(url) = config::ELYSIUM_SOCIAL_URL {
        env.push((process::SOCIAL_URL_ENV, url.to_string()));
    }
    (env, secrets)
}

#[tauri::command]
pub async fn launch_game(
    app: AppHandle,
    launch: State<'_, LaunchState>,
    settings: State<'_, SettingsState>,
) -> Result<()> {
    let mut guard = acquire(&launch)?;
    let s = settings.0.lock().unwrap().clone();
    let game_dir = s.game_dir();
    let reporter = Reporter::new(app.clone());

    reporter.status("auth", "Проверка аккаунта");
    let auth = auth::prepare_launch(&app, s.selected_account.as_deref()).await?;

    let prepared = prepare(&reporter, &game_dir, &s.disabled_mods, false).await?;

    reporter.status("launch", "Запуск…");
    let (jvm, game) = build_command(&prepared, &s, &auth, &game_dir);
    let piped = s.after_launch != AfterLaunch::Close;
    let (env, mut secrets) = social_env(&app).await;
    secrets.push(auth.access_token.clone());
    process::spawn(&app, &prepared.java, &game_dir, jvm, game, env, secrets, piped).await?;
    guard.armed = false; // the process watcher clears `busy` on exit

    if let Some(window) = app.get_webview_window("main") {
        match s.after_launch {
            AfterLaunch::Minimize => {
                let _ = window.minimize();
            }
            AfterLaunch::Close => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    app.exit(0);
                });
            }
            AfterLaunch::Keep => {}
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn reinstall_files(
    app: AppHandle,
    launch: State<'_, LaunchState>,
    settings: State<'_, SettingsState>,
) -> Result<()> {
    let _guard = acquire(&launch)?;
    let s = settings.0.lock().unwrap().clone();
    let game_dir = s.game_dir();
    let reporter = Reporter::new(app.clone());
    prepare(&reporter, &game_dir, &s.disabled_mods, true).await?;
    reporter.status("done", "Файлы проверены и переустановлены");
    Ok(())
}

#[tauri::command]
pub async fn stop_game(launch: State<'_, LaunchState>) -> Result<()> {
    let tx = launch.kill.lock().unwrap().clone();
    if let Some(tx) = tx {
        let _ = tx.send(()).await;
    }
    Ok(())
}

#[tauri::command]
pub fn is_busy(launch: State<'_, LaunchState>) -> bool {
    launch.busy.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Fabric mod ids that must be loaded after a clean install.
    const EXPECTED_MODS: &[&str] = &[
        "elysium-visuals",
        "fabric-api",
        "sodium",
        "iris",
        "entityculling",
        "immediatelyfast",
        "modmenu",
        "placeholder-api",
        "punchy",
    ];

    /// Full install + boot with an offline account until the game finishes
    /// loading. Uses `ELYSIUM_E2E_DIR` (an empty folder for a clean install)
    /// or the default game dir. Network-heavy:
    /// `cargo test e2e -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn e2e_install_and_boot() {
        let mut settings = Settings::default();
        if let Ok(dir) = std::env::var("ELYSIUM_E2E_DIR") {
            settings.game_dir = dir;
        }
        let game_dir = settings.game_dir();
        println!("game dir: {}", game_dir.display());
        let mod_downloads = Arc::new(AtomicUsize::new(0));
        let reporter = {
            let last = std::sync::Mutex::new(String::new());
            let mod_downloads = mod_downloads.clone();
            Reporter::from_fn(move |e| {
                if e.stage == "mods" && e.total > 0 {
                    mod_downloads.store(e.total, Ordering::SeqCst);
                }
                let mut last = last.lock().unwrap();
                if *last != e.stage || e.current == e.total {
                    println!("[{}] {}", e.stage, e.label);
                    *last = e.stage.to_string();
                }
            })
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let prepared = rt.block_on(prepare(&reporter, &game_dir, &[], false)).expect("prepare");
        println!("java: {}", prepared.java.display());
        println!("classpath entries: {}", prepared.classpath.len());

        let state = crate::mods::load_state(&game_dir);
        for m in &state.mods {
            println!("mod: {} {} ({})", m.name, m.version_number, m.filename);
            assert!(game_dir.join("mods").join(&m.filename).is_file(), "{} missing", m.filename);
        }
        assert!(state.mods.len() >= 7, "expected 6 listed mods + Text Placeholder API");

        // A second run finds everything in place.
        mod_downloads.store(0, Ordering::SeqCst);
        rt.block_on(prepare(&reporter, &game_dir, &[], false)).expect("prepare again");
        assert_eq!(mod_downloads.load(Ordering::SeqCst), 0, "mods re-downloaded on second run");

        let name = "ElysiumTest";
        let auth = LaunchAuth {
            username: name.into(),
            uuid: crate::auth::offline::offline_uuid(name),
            access_token: "0".into(),
            xuid: "0".into(),
            user_type: "msa",
        };
        let (jvm, game) = build_command(&prepared, &settings, &auth, &game_dir);
        let mut child = std::process::Command::new(&prepared.java)
            .args(&jvm)
            .args(&game)
            .current_dir(&game_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn java");

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        for out in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(|l| l.ok()) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        // Fabric prints "Loading N mods:" followed by "	- id version" lines.
        let mut loaded: Vec<String> = Vec::new();
        let mut in_list = false;
        let mut booted = false;
        let mut failure = None;
        let deadline = Instant::now() + Duration::from_secs(240);
        while let Ok(line) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            println!("{line}");
            if line.contains("Loading") && line.contains("mods:") {
                in_list = true;
                continue;
            }
            if in_list {
                match line.trim_start().strip_prefix("- ") {
                    Some(rest) => loaded.push(rest.split_whitespace().next().unwrap_or("").to_string()),
                    None if line.trim_start().starts_with("|--") || line.trim_start().starts_with(r"\--") => {}
                    None => in_list = false,
                }
            }
            if line.contains("---- Minecraft Crash Report ----")
                || line.contains("Mixin apply failed")
                || line.contains("Incompatible mods found")
                || line.contains("Mod resolution failed")
            {
                failure = Some(line.clone());
                break;
            }
            if line.contains("Sound engine started") || line.contains("Created:") && line.contains("atlas") {
                booted = true;
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        let _ = child.kill();
        println!("loaded mods: {loaded:?}");
        assert!(failure.is_none(), "game failed: {failure:?}");
        for id in EXPECTED_MODS {
            assert!(loaded.iter().any(|m| m == id), "Fabric did not load {id}");
        }
        assert!(booted, "game did not finish loading in time");
    }
}
