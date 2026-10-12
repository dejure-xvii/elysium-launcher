//! Built-in mods from `default-mods.json` (in the mod repository), downloaded
//! from Modrinth with sha512 checks, or from a GitHub release pinned by tag,
//! file name and sha256. Only files the launcher installed itself (tracked in
//! `.elysium-mods.json`) are ever replaced or removed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config;
use crate::download::{self, sha256_file, sha512_file, DownloadTask, Reporter, Verify};
use crate::error::{Error, Result};
use crate::fabric;
use crate::paths;
use crate::settings::{self, Settings, SettingsState};
use crate::updates::{jar_mod_json, jar_mod_version};

// ---------------------------------------------------------------- the list

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModList {
    pub schema: u32,
    pub minecraft: String,
    pub loader: String,
    pub mods: Vec<ListEntry>,
    /// Mods that aren't on Modrinth, from GitHub releases. A separate field
    /// (not entries in `mods`) so that launchers up to 1.3.0, which require
    /// Modrinth ids for every entry, keep reading the same list.
    #[serde(default)]
    pub github: Vec<GithubEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListEntry {
    pub name: String,
    /// Modrinth project id.
    pub project: String,
    /// Modrinth version id.
    pub version: String,
    #[serde(default = "yes")]
    pub required: bool,
}

fn yes() -> bool {
    true
}

impl ListEntry {
    fn enabled(&self, disabled: &[String]) -> bool {
        self.required || !disabled.contains(&self.project)
    }
}

/// A mod file attached to a GitHub release: only ever downloaded from that
/// repository's release and installed if its sha256 matches.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubEntry {
    pub name: String,
    /// `owner/repo`.
    pub repo: String,
    /// Release tag, e.g. `v1.0.3`.
    pub tag: String,
    /// Asset file name in that release.
    pub file: String,
    pub sha256: String,
    /// `id` from the jar's `fabric.mod.json`, to recognise a user's own copy.
    pub mod_id: String,
    /// Shown in the Mods tab next to a link to the repository.
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default = "yes")]
    pub required: bool,
}

impl GithubEntry {
    /// Stands in for the Modrinth project id: in `disabledMods`, the state file and the UI.
    pub fn key(&self) -> String {
        format!("github:{}", self.repo)
    }

    fn enabled(&self, disabled: &[String]) -> bool {
        self.required || !disabled.contains(&self.key())
    }

    fn page(&self) -> String {
        format!("{}{}", config::GITHUB_URL, self.repo)
    }

    fn url(&self) -> String {
        format!("{}/releases/download/{}/{}", self.page(), self.tag, self.file)
    }

    fn is_valid(&self) -> bool {
        let label = |s: &str| !s.trim().is_empty() && s.chars().count() <= 64 && !s.chars().any(char::is_control);
        let mod_id = !self.mod_id.is_empty()
            && self.mod_id.len() <= 64
            && self.mod_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
        valid_repo(&self.repo)
            && valid_path_part(&self.tag)
            && valid_path_part(&self.file)
            && self.file.ends_with(".jar")
            && self.sha256.len() == 64
            && self.sha256.chars().all(|c| c.is_ascii_hexdigit())
            && mod_id
            && label(&self.name)
            && self.author.as_deref().is_none_or(label)
    }
}

/// Modrinth ids are short alphanumeric strings; anything else is rejected so
/// list contents can't be smuggled into API paths.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// A tag or file name that goes into a URL path and (file) into `mods/` as is.
fn valid_path_part(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

/// `owner/repo` as GitHub allows them.
fn valid_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else { return false };
    let owner_ok = !owner.is_empty() && owner.len() <= 39 && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let name_ok = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    owner_ok && name_ok
}

fn parse_list(text: &str) -> Result<ModList> {
    let list: ModList = serde_json::from_str(text)?;
    if list.schema != 1 {
        return Err(Error::msg(format!("Неизвестный формат списка модов ({})", list.schema)));
    }
    if list.minecraft != config::MINECRAFT_VERSION || list.loader != "fabric" {
        return Err(Error::msg(format!(
            "Список модов для {} {}, а лаунчер ставит Fabric {}",
            list.loader,
            list.minecraft,
            config::MINECRAFT_VERSION
        )));
    }
    if let Some(bad) = list.mods.iter().find(|m| !valid_id(&m.project) || !valid_id(&m.version)) {
        return Err(Error::msg(format!("Некорректный ID в списке модов: {}", bad.name)));
    }
    if let Some(bad) = list.github.iter().find(|g| !g.is_valid()) {
        return Err(Error::msg(format!("Некорректный мод с GitHub в списке модов: {}", bad.name)));
    }
    let mut repos = HashSet::new();
    if let Some(dup) = list.github.iter().find(|g| !repos.insert(g.repo.to_lowercase())) {
        return Err(Error::msg(format!("Репозиторий {} указан в списке модов дважды", dup.repo)));
    }
    Ok(list)
}

fn cached_list_file() -> PathBuf {
    paths::launcher_dir().join("default-mods.json")
}

/// Debug builds read the list from the sibling mod project when it exists.
#[cfg(debug_assertions)]
fn dev_list() -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../elysium-visuals-template-26.2/default-mods.json");
    std::fs::read_to_string(path).ok()
}

#[cfg(not(debug_assertions))]
fn dev_list() -> Option<String> {
    None
}

fn cached_list() -> Option<ModList> {
    let text = dev_list().or_else(|| std::fs::read_to_string(cached_list_file()).ok())?;
    parse_list(&text).ok()
}

/// Downloads the list (cached for offline use); falls back to the cache.
pub async fn load_list(client: &reqwest::Client) -> Result<ModList> {
    if let Some(text) = dev_list() {
        return parse_list(&text);
    }
    let fresh = async {
        let text = client
            .get(config::DEFAULT_MODS_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let list = parse_list(&text)?;
        std::fs::create_dir_all(paths::launcher_dir())?;
        std::fs::write(cached_list_file(), text)?;
        Ok::<_, Error>(list)
    }
    .await;
    match fresh {
        Ok(list) => Ok(list),
        Err(e) => cached_list().ok_or_else(|| Error::msg(format!("Не удалось получить список модов: {e}"))),
    }
}

// ---------------------------------------------------------------- Modrinth

#[derive(Clone, Debug, Deserialize)]
struct MrVersion {
    id: String,
    project_id: String,
    version_number: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: Vec<MrDep>,
}

#[derive(Clone, Debug, Deserialize)]
struct MrFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    size: u64,
    hashes: MrHashes,
}

#[derive(Clone, Debug, Deserialize)]
struct MrHashes {
    sha512: String,
}

#[derive(Clone, Debug, Deserialize)]
struct MrDep {
    project_id: Option<String>,
    version_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct MrProject {
    id: String,
    title: String,
}

fn ids_param(ids: &[String]) -> String {
    serde_json::to_string(ids).unwrap_or_default()
}

async fn versions_by_id(client: &reqwest::Client, ids: &[String]) -> Result<Vec<MrVersion>> {
    let url = format!("{}/versions", config::MODRINTH_API_URL);
    Ok(client
        .get(url)
        .query(&[("ids", ids_param(ids))])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Newest release (or newest of any type) for Fabric on our Minecraft version.
async fn latest_version(client: &reqwest::Client, project: &str) -> Result<MrVersion> {
    let url = format!("{}/project/{project}/version", config::MODRINTH_API_URL);
    let list: Vec<MrVersion> = client
        .get(url)
        .query(&[
            ("loaders", r#"["fabric"]"#.to_string()),
            ("game_versions", format!(r#"["{}"]"#, config::MINECRAFT_VERSION)),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let pick = list.iter().find(|v| v.version_type == "release").or(list.first());
    pick.cloned()
        .ok_or_else(|| Error::msg(format!("На Modrinth нет версии {project} для Fabric {}", config::MINECRAFT_VERSION)))
}

async fn project_titles(client: &reqwest::Client, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let url = format!("{}/projects", config::MODRINTH_API_URL);
    let list: Vec<MrProject> = client
        .get(url)
        .query(&[("ids", ids_param(ids))])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list.into_iter().map(|p| (p.id, p.title)).collect())
}

/// Maps sha512 hashes of local files to Modrinth project ids.
async fn projects_by_hash(client: &reqwest::Client, hashes: &[String]) -> Result<HashMap<String, String>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let url = format!("{}/version_files", config::MODRINTH_API_URL);
    let map: HashMap<String, MrVersion> = client
        .post(url)
        .json(&serde_json::json!({ "hashes": hashes, "algorithm": "sha512" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(map.into_iter().map(|(h, v)| (h, v.project_id)).collect())
}

// ---------------------------------------------------------------- resolving

/// A mod file the launcher wants in `mods/`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Planned {
    pub project: String,
    pub version: String,
    pub version_number: String,
    pub name: String,
    pub filename: String,
    pub url: String,
    /// Empty for GitHub mods, which are checked by `sha256` instead.
    pub sha512: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// 0 when unknown (GitHub mods).
    pub size: u64,
    /// Listed as required (always true for dependencies).
    pub required: bool,
    /// Top-level list entries that pulled this mod in; empty for list entries.
    #[serde(default)]
    pub parents: Vec<String>,
    /// `fabric.mod.json` id (GitHub mods), to recognise a user's own copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_id: Option<String>,
}

impl Planned {
    /// The hash `plan` compares against: sha256 for GitHub mods, sha512 otherwise.
    fn expected_hash(&self) -> &str {
        self.sha256.as_deref().unwrap_or(&self.sha512)
    }
}

fn planned_from_github(g: &GithubEntry) -> Planned {
    Planned {
        project: g.key(),
        version: g.tag.clone(),
        version_number: g.tag.trim_start_matches('v').to_string(),
        name: g.name.clone(),
        filename: g.file.clone(),
        url: g.url(),
        sha512: String::new(),
        sha256: Some(g.sha256.to_lowercase()),
        size: 0,
        required: g.required,
        parents: Vec::new(),
        mod_id: Some(g.mod_id.clone()),
    }
}

fn planned_from(v: &MrVersion, name: &str, required: bool) -> Result<Planned> {
    if !v.loaders.iter().any(|l| l == "fabric") || !v.game_versions.iter().any(|g| g == config::MINECRAFT_VERSION) {
        return Err(Error::msg(format!(
            "Версия {name} {} не подходит для Fabric {}",
            v.version_number,
            config::MINECRAFT_VERSION
        )));
    }
    let file = v
        .files
        .iter()
        .find(|f| f.primary)
        .or(v.files.first())
        .ok_or_else(|| Error::msg(format!("У {name} {} нет файлов", v.version_number)))?;
    let safe_name = file.filename.ends_with(".jar")
        && !file.filename.contains(['/', '\\', ':'])
        && !file.filename.starts_with('.');
    if !safe_name || !file.url.starts_with(config::MODRINTH_CDN_PREFIX) || file.hashes.sha512.len() != 128 {
        return Err(Error::msg(format!("Некорректный файл {name} на Modrinth")));
    }
    Ok(Planned {
        project: v.project_id.clone(),
        version: v.id.clone(),
        version_number: v.version_number.clone(),
        name: name.to_string(),
        filename: file.filename.clone(),
        url: file.url.clone(),
        sha512: file.hashes.sha512.to_lowercase(),
        sha256: None,
        size: file.size,
        required,
        parents: Vec::new(),
        mod_id: None,
    })
}

/// Turns the list into concrete files: enabled entries at their pinned
/// versions plus their required dependencies (except Fabric API, which the
/// launcher installs from Maven). A version pinned in the list wins over the
/// one a dependency asks for. Enabled GitHub mods come last, as pinned.
pub async fn resolve(client: &reqwest::Client, list: &ModList, disabled: &[String]) -> Result<Vec<Planned>> {
    let pinned: HashMap<&str, &ListEntry> = list.mods.iter().map(|m| (m.project.as_str(), m)).collect();
    let top: Vec<&ListEntry> = list
        .mods
        .iter()
        .filter(|m| m.enabled(disabled) && m.project != config::FABRIC_API_PROJECT)
        .collect();

    let ids: Vec<String> = top.iter().map(|m| m.version.clone()).collect();
    let fetched: HashMap<String, MrVersion> =
        versions_by_id(client, &ids).await?.into_iter().map(|v| (v.id.clone(), v)).collect();

    let mut out: Vec<Planned> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new(); // project -> out index
    // (version, root project) whose dependencies still need resolving.
    let mut queue: Vec<(MrVersion, String)> = Vec::new();

    for m in &top {
        let v = fetched
            .get(&m.version)
            .ok_or_else(|| Error::msg(format!("Версия {} не найдена на Modrinth", m.name)))?;
        if v.project_id != m.project {
            return Err(Error::msg(format!("Версия {} относится к другому проекту", m.name)));
        }
        index.insert(m.project.clone(), out.len());
        out.push(planned_from(v, &m.name, m.required)?);
        queue.push((v.clone(), m.project.clone()));
    }

    let mut deps: Vec<String> = Vec::new();
    while let Some((v, root)) = queue.pop() {
        for d in v.dependencies.iter().filter(|d| d.dependency_type == "required") {
            // Resolve the dependency's project (and version, if it names one).
            let (project, dep_version) = match (&d.project_id, &d.version_id) {
                (Some(p), _) if !valid_id(p) => continue,
                (Some(p), vid) => (p.clone(), vid.clone()),
                (None, Some(vid)) if valid_id(vid) => {
                    let dv = versions_by_id(client, &[vid.clone()]).await?.into_iter().next();
                    match dv {
                        Some(dv) => (dv.project_id, Some(vid.clone())),
                        None => continue,
                    }
                }
                _ => continue,
            };
            if project == config::FABRIC_API_PROJECT {
                continue;
            }
            if let Some(&i) = index.get(&project) {
                if !out[i].parents.is_empty() && !out[i].parents.contains(&root) {
                    out[i].parents.push(root.clone());
                }
                continue;
            }
            let dv = if let Some(entry) = pinned.get(project.as_str()) {
                versions_by_id(client, &[entry.version.clone()]).await?.into_iter().next()
            } else if let Some(vid) = dep_version.filter(|v| valid_id(v)) {
                versions_by_id(client, &[vid]).await?.into_iter().next()
            } else {
                Some(latest_version(client, &project).await?)
            };
            let dv = dv.ok_or_else(|| Error::msg(format!("Зависимость {project} не найдена на Modrinth")))?;
            let name = pinned.get(project.as_str()).map(|e| e.name.clone()).unwrap_or_else(|| project.clone());
            let mut p = planned_from(&dv, &name, true)?;
            p.parents.push(root.clone());
            if !pinned.contains_key(project.as_str()) {
                deps.push(project.clone());
            }
            index.insert(project, out.len());
            out.push(p);
            queue.push((dv, root.clone()));
        }
    }

    // Human-readable names for dependencies that aren't in the list.
    if !deps.is_empty() {
        if let Ok(titles) = project_titles(client, &deps).await {
            for p in out.iter_mut() {
                if let Some(t) = titles.get(&p.project) {
                    p.name = t.clone();
                }
            }
        }
    }

    for g in list.github.iter().filter(|g| g.enabled(disabled)) {
        if out.iter().any(|p| p.filename.eq_ignore_ascii_case(&g.file)) {
            return Err(Error::msg(format!("Файл {} уже есть среди модов с Modrinth", g.file)));
        }
        out.push(planned_from_github(g));
    }
    Ok(out)
}

// ---------------------------------------------------------------- state

/// What the launcher put into `mods/` last time.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManagedState {
    pub mods: Vec<Planned>,
    /// Built-in mods skipped because the user already has their own copy.
    pub replaced: Vec<Planned>,
}

fn state_file(game_dir: &Path) -> PathBuf {
    game_dir.join(".elysium-mods.json")
}

pub fn load_state(game_dir: &Path) -> ManagedState {
    std::fs::read_to_string(state_file(game_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(game_dir: &Path, state: &ManagedState) -> Result<()> {
    let file = state_file(game_dir);
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(tmp, file)?;
    Ok(())
}

fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

/// Jars the launcher manages through other means.
fn is_core_jar(name: &str) -> bool {
    name == config::MOD_FILE_NAME || name.starts_with("fabric-api-")
}

#[derive(Debug, Default, PartialEq)]
pub struct Actions {
    pub download: Vec<Planned>,
    /// File names to delete from `mods/`.
    pub remove: Vec<String>,
}

/// Decides what to download and delete. `hash_of(mod)` is the hash of the
/// mod's file currently in `mods/` (sha256 for GitHub mods, sha512 otherwise;
/// None when missing). A managed file that is no longer wanted is deleted only
/// if it is still exactly what we installed.
pub fn plan(state: &[Planned], desired: &[Planned], hash_of: impl Fn(&Planned) -> Option<String>) -> Actions {
    let mut actions = Actions::default();
    for d in desired {
        if hash_of(d).as_deref() != Some(d.expected_hash()) {
            actions.download.push(d.clone());
        }
    }
    for s in state {
        if desired.iter().any(|d| d.filename == s.filename) {
            continue;
        }
        if hash_of(s).as_deref() == Some(s.expected_hash()) {
            actions.remove.push(s.filename.clone());
        }
    }
    actions
}

/// Offline: keep what was installed, minus switched-off mods and the
/// dependencies only they needed.
fn offline_desired(state: &ManagedState, disabled: &[String]) -> Vec<Planned> {
    let on = |p: &str| !disabled.iter().any(|d| d == p);
    state
        .mods
        .iter()
        .filter(|m| {
            if m.parents.is_empty() {
                m.required || on(&m.project)
            } else {
                m.parents.iter().any(|p| on(p))
            }
        })
        .cloned()
        .collect()
}

/// Drops built-in mods the user already has as their own jar (matched by
/// Modrinth project via file hash, or for GitHub mods by the `fabric.mod.json`
/// id), so nothing is loaded twice.
async fn drop_user_duplicates(
    client: &reqwest::Client,
    game_dir: &Path,
    state: &ManagedState,
    desired: Vec<Planned>,
) -> (Vec<Planned>, Vec<Planned>) {
    let managed: HashSet<&str> = state.mods.iter().map(|m| m.filename.as_str()).collect();
    let mut user_hashes = Vec::new();
    let mut user_files = HashSet::new();
    let mut user_mod_ids = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(mods_dir(game_dir)) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".jar") || is_core_jar(&name) || managed.contains(name.as_str()) {
                continue;
            }
            if let Ok(h) = sha512_file(&e.path()) {
                // A user jar that is byte-identical to ours is simply adopted.
                let ours = desired.iter().any(|d| {
                    d.filename == name
                        && match &d.sha256 {
                            Some(expected) => sha256_file(&e.path()).is_ok_and(|h| h.eq_ignore_ascii_case(expected)),
                            None => d.sha512 == h,
                        }
                });
                if !ours {
                    user_hashes.push(h);
                    user_files.insert(name);
                    if let Some(id) = jar_mod_json(&e.path()).and_then(|j| j.get("id")?.as_str().map(str::to_string)) {
                        user_mod_ids.insert(id);
                    }
                }
            }
        }
    }
    let user_projects: HashSet<String> = projects_by_hash(client, &user_hashes)
        .await
        .map(|m| m.into_values().collect())
        .unwrap_or_default();
    desired.into_iter().partition(|d| {
        !user_projects.contains(&d.project)
            && !user_files.contains(&d.filename)
            && !d.mod_id.as_ref().is_some_and(|id| user_mod_ids.contains(id))
    })
}

/// Brings `mods/` in line with the list: downloads new/changed built-in mods,
/// removes ones that were switched off, dropped or replaced by a new version.
pub async fn sync(client: &reqwest::Client, game_dir: &Path, disabled: &[String], reporter: &Reporter) -> Result<()> {
    let mods = mods_dir(game_dir);
    std::fs::create_dir_all(&mods)?;
    let state = load_state(game_dir);

    let online = async {
        let list = load_list(client).await?;
        resolve(client, &list, disabled).await
    }
    .await;
    let (desired, replaced) = match online {
        Ok(desired) => drop_user_duplicates(client, game_dir, &state, desired).await,
        Err(e) if !state.mods.is_empty() => {
            let _ = e;
            (offline_desired(&state, disabled), state.replaced.clone())
        }
        Err(e) => return Err(Error::msg(format!("Не удалось установить моды: {e}"))),
    };

    let hash_of = |p: &Planned| {
        let path = mods.join(&p.filename);
        let hash = if p.sha256.is_some() { sha256_file(&path) } else { sha512_file(&path) };
        hash.ok().map(|h| h.to_lowercase())
    };
    let actions = plan(&state.mods, &desired, hash_of);

    let tasks: Vec<DownloadTask> = actions
        .download
        .iter()
        .map(|p| DownloadTask {
            url: p.url.clone(),
            path: mods.join(&p.filename),
            sha1: None,
            sha512: p.sha256.is_none().then(|| p.sha512.clone()),
            sha256: p.sha256.clone(),
            size: (p.size > 0).then_some(p.size),
        })
        .collect();
    if !tasks.is_empty() {
        let r = reporter.clone();
        download::download_all(client, tasks, Verify::Full, move |n, t| {
            r.report("mods", format!("Скачивание модов {n}/{t}"), n, t)
        })
        .await
        .map_err(|e| Error::msg(format!("Не удалось скачать моды: {e}")))?;
    }
    for f in &actions.remove {
        std::fs::remove_file(mods.join(f))?;
    }
    save_state(game_dir, &ManagedState { mods: desired, replaced })
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinMod {
    /// Modrinth project id, `github:owner/repo`, or a fixed key for Elysium Visuals / Fabric API.
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub required: bool,
    pub enabled: bool,
    pub installed: bool,
    /// Names of the mods that need this one (dependencies only).
    pub dependency_of: Vec<String>,
    /// The user has their own copy, so the built-in one isn't installed.
    pub replaced_by_user: bool,
    /// Credit and project page for third-party mods from GitHub.
    pub author: Option<String>,
    pub page: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMod {
    pub file: String,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsView {
    pub builtin: Vec<BuiltinMod>,
    pub user: Vec<UserMod>,
}

fn build_view(game_dir: &Path, list: Option<&ModList>, disabled: &[String]) -> ModsView {
    let mods = mods_dir(game_dir);
    let state = load_state(game_dir);
    let installed = |f: &str| mods.join(f).is_file();
    let name_of = |project: &str| {
        list.and_then(|l| l.mods.iter().find(|m| m.project == project).map(|m| m.name.clone()))
            .or_else(|| state.mods.iter().find(|m| m.project == project).map(|m| m.name.clone()))
            .unwrap_or_else(|| project.to_string())
    };

    let mut builtin = vec![
        BuiltinMod {
            id: "elysium-visuals".into(),
            name: "Elysium Visuals".into(),
            version: fabric::mod_source().and_then(|p| jar_mod_version(&p)),
            required: true,
            enabled: true,
            installed: installed(config::MOD_FILE_NAME),
            dependency_of: Vec::new(),
            replaced_by_user: false,
            author: None,
            page: None,
        },
        BuiltinMod {
            id: config::FABRIC_API_PROJECT.into(),
            name: "Fabric API".into(),
            version: Some(config::FABRIC_API_VERSION.into()),
            required: true,
            enabled: true,
            installed: installed(&format!("fabric-api-{}.jar", config::FABRIC_API_VERSION)),
            dependency_of: Vec::new(),
            replaced_by_user: false,
            author: None,
            page: None,
        },
    ];

    let mut seen = HashSet::new();
    for m in list.map(|l| l.mods.as_slice()).unwrap_or_default() {
        if m.project == config::FABRIC_API_PROJECT || !seen.insert(m.project.clone()) {
            continue;
        }
        let managed = state.mods.iter().find(|s| s.project == m.project);
        let replaced = state.replaced.iter().any(|s| s.project == m.project);
        builtin.push(BuiltinMod {
            id: m.project.clone(),
            name: m.name.clone(),
            version: managed.map(|s| s.version_number.clone()),
            required: m.required,
            enabled: m.enabled(disabled),
            installed: managed.is_some_and(|s| installed(&s.filename)),
            dependency_of: managed.map(|s| s.parents.iter().map(|p| name_of(p)).collect()).unwrap_or_default(),
            replaced_by_user: replaced,
            author: None,
            page: None,
        });
    }
    for g in list.map(|l| l.github.as_slice()).unwrap_or_default() {
        let key = g.key();
        if !seen.insert(key.clone()) {
            continue;
        }
        let managed = state.mods.iter().find(|s| s.project == key);
        builtin.push(BuiltinMod {
            id: key.clone(),
            name: g.name.clone(),
            version: managed.map(|s| s.version_number.clone()),
            required: g.required,
            enabled: g.enabled(disabled),
            installed: managed.is_some_and(|s| installed(&s.filename)),
            dependency_of: Vec::new(),
            replaced_by_user: state.replaced.iter().any(|s| s.project == key),
            author: g.author.clone(),
            page: Some(g.page()),
        });
    }
    // Dependencies that aren't in the list themselves.
    for s in state.mods.iter().chain(state.replaced.iter()) {
        if !seen.insert(s.project.clone()) {
            continue;
        }
        builtin.push(BuiltinMod {
            id: s.project.clone(),
            name: s.name.clone(),
            version: Some(s.version_number.clone()),
            required: true,
            enabled: true,
            installed: installed(&s.filename),
            dependency_of: s.parents.iter().map(|p| name_of(p)).collect(),
            replaced_by_user: state.replaced.iter().any(|r| r.project == s.project),
            author: None,
            page: None,
        });
    }

    let managed: HashSet<&str> = state.mods.iter().map(|m| m.filename.as_str()).collect();
    let mut user = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mods) {
        for e in entries.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            if !file.ends_with(".jar") || is_core_jar(&file) || managed.contains(file.as_str()) {
                continue;
            }
            let json = jar_mod_json(&e.path());
            let field = |k: &str| json.as_ref().and_then(|j| j.get(k)?.as_str().map(str::to_string));
            user.push(UserMod {
                name: field("name").unwrap_or_else(|| file.trim_end_matches(".jar").to_string()),
                version: field("version"),
                file,
            });
        }
    }
    user.sort_by_key(|m| m.name.to_lowercase());
    ModsView { builtin, user }
}

#[tauri::command]
pub async fn list_mods(settings: State<'_, SettingsState>) -> Result<ModsView> {
    let s = settings.0.lock().unwrap().clone();
    let list = match cached_list() {
        Some(l) => Some(l),
        None => load_list(&download::http_client()).await.ok(),
    };
    Ok(build_view(&s.game_dir(), list.as_ref(), &s.disabled_mods))
}

/// Switches an optional built-in mod on or off; applied on the next launch.
#[tauri::command]
pub async fn set_mod_enabled(settings: State<'_, SettingsState>, id: String, enabled: bool) -> Result<Settings> {
    let list = match cached_list() {
        Some(l) => l,
        None => load_list(&download::http_client()).await?,
    };
    let (name, required) = list
        .mods
        .iter()
        .find(|m| m.project == id)
        .map(|m| (&m.name, m.required))
        .or_else(|| list.github.iter().find(|g| g.key() == id).map(|g| (&g.name, g.required)))
        .ok_or_else(|| Error::msg("Это не встроенный мод"))?;
    if required {
        return Err(Error::msg(format!("{name} — обязательный мод, его нельзя выключить")));
    }
    let mut s = settings.0.lock().unwrap().clone();
    s.disabled_mods.retain(|p| p != &id);
    if !enabled {
        s.disabled_mods.push(id);
    }
    settings::save(&s)?;
    *settings.0.lock().unwrap() = s.clone();
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(project: &str, file: &str, hash: &str) -> Planned {
        Planned {
            project: project.into(),
            version: format!("{project}-v"),
            version_number: "1".into(),
            name: project.into(),
            filename: file.into(),
            url: format!("https://cdn.modrinth.com/{file}"),
            sha512: hash.into(),
            sha256: None,
            size: 1,
            required: true,
            parents: Vec::new(),
            mod_id: None,
        }
    }

    /// `hash_of` for `plan`: the hash of each file present in `mods/`.
    fn files<'a>(list: &'a [(&'a str, &'a str)]) -> impl Fn(&Planned) -> Option<String> + 'a {
        move |p: &Planned| list.iter().find(|(n, _)| *n == p.filename).map(|(_, h)| h.to_string())
    }

    const ZAKO_SHA256: &str = "72c3b9d04fefa3ea4bf01a667736508e03bd60d987c390696cface9853e2bf0d";

    fn zako() -> GithubEntry {
        GithubEntry {
            name: "ZakoOpt".into(),
            repo: "3ako/ZakoOpt".into(),
            tag: "v1.0.3".into(),
            file: "zakoopt-1.0.3+26.2.jar".into(),
            sha256: ZAKO_SHA256.into(),
            mod_id: "zakoopt".into(),
            author: Some("Zako".into()),
            required: false,
        }
    }

    #[test]
    fn fresh_install_downloads_everything() {
        let desired = vec![p("a", "a.jar", "1"), p("b", "b.jar", "2")];
        let a = plan(&[], &desired, files(&[]));
        assert_eq!(a.download, desired);
        assert!(a.remove.is_empty());
    }

    #[test]
    fn up_to_date_does_nothing() {
        let desired = vec![p("a", "a.jar", "1")];
        let a = plan(&desired, &desired, files(&[("a.jar", "1")]));
        assert_eq!(a, Actions::default());
    }

    #[test]
    fn version_change_replaces_old_file() {
        let old = vec![p("a", "a-1.jar", "1")];
        let new = vec![p("a", "a-2.jar", "2")];
        let a = plan(&old, &new, files(&[("a-1.jar", "1")]));
        assert_eq!(a.download, new);
        assert_eq!(a.remove, vec!["a-1.jar".to_string()]);
    }

    #[test]
    fn same_file_name_new_hash_is_redownloaded() {
        let old = vec![p("a", "a.jar", "1")];
        let new = vec![p("a", "a.jar", "2")];
        let a = plan(&old, &new, files(&[("a.jar", "1")]));
        assert_eq!(a.download, new);
        assert!(a.remove.is_empty());
    }

    #[test]
    fn disabled_mod_is_removed() {
        let old = vec![p("a", "a.jar", "1"), p("iris", "iris.jar", "2")];
        let new = vec![p("a", "a.jar", "1")];
        let a = plan(&old, &new, files(&[("a.jar", "1"), ("iris.jar", "2")]));
        assert!(a.download.is_empty());
        assert_eq!(a.remove, vec!["iris.jar".to_string()]);
    }

    #[test]
    fn user_files_are_never_removed() {
        // user.jar is not in the state; a.jar was modified by the user.
        let old = vec![p("a", "a.jar", "1")];
        let a = plan(&old, &[], files(&[("a.jar", "changed"), ("user.jar", "9")]));
        assert!(a.remove.is_empty());
    }

    #[test]
    fn offline_drops_disabled_and_orphaned_deps() {
        let mut iris = p("iris", "iris.jar", "1");
        iris.required = false;
        let mut dep = p("dep", "dep.jar", "2");
        dep.parents = vec!["iris".into()];
        let mut shared = p("shared", "shared.jar", "3");
        shared.parents = vec!["iris".into(), "menu".into()];
        let state = ManagedState { mods: vec![iris, dep, shared, p("menu", "menu.jar", "4")], replaced: vec![] };
        let names: Vec<String> =
            offline_desired(&state, &["iris".into()]).into_iter().map(|m| m.project).collect();
        assert_eq!(names, vec!["shared", "menu"]);
    }

    #[test]
    fn list_validation() {
        let ok = r#"{"schema":1,"minecraft":"26.2","loader":"fabric","mods":[{"name":"S","project":"AANobbMI","version":"xJZxADzI"}]}"#;
        let list = parse_list(ok).unwrap();
        assert!(list.mods[0].required, "required defaults to true");
        assert!(parse_list(&ok.replace("26.2", "1.21")).is_err());
        assert!(parse_list(&ok.replace("xJZxADzI", "../evil")).is_err());
        assert!(list.github.is_empty(), "lists without GitHub mods still parse");
    }

    #[test]
    fn github_list_validation() {
        let with = |g: &GithubEntry| {
            let entry = serde_json::to_string(g).unwrap();
            format!(r#"{{"schema":1,"minecraft":"26.2","loader":"fabric","mods":[],"github":[{entry}]}}"#)
        };
        let list = parse_list(&with(&zako())).unwrap();
        assert_eq!(list.github[0].key(), "github:3ako/ZakoOpt");
        assert!(!list.github[0].required);
        let required_default = with(&zako()).replace(r#","required":false"#, "");
        assert!(parse_list(&required_default).unwrap().github[0].required, "required defaults to true");

        let bad = |f: fn(&mut GithubEntry)| {
            let mut g = zako();
            f(&mut g);
            parse_list(&with(&g)).is_err()
        };
        assert!(bad(|g| g.repo = "3ako".into()));
        assert!(bad(|g| g.repo = "3ako/ZakoOpt/releases".into()));
        assert!(bad(|g| g.repo = "../ZakoOpt".into()));
        assert!(bad(|g| g.repo = "3ako/..".into()));
        assert!(bad(|g| g.tag = "v1.0.3/../../x".into()));
        assert!(bad(|g| g.tag = "v1.0.3?x=1".into()));
        assert!(bad(|g| g.file = "../zakoopt.jar".into()));
        assert!(bad(|g| g.file = "zakoopt.zip".into()));
        assert!(bad(|g| g.file = "zako opt.jar".into()));
        assert!(bad(|g| g.file = ".jar".into()));
        assert!(bad(|g| g.sha256 = ZAKO_SHA256[..63].into()));
        assert!(bad(|g| g.sha256 = ZAKO_SHA256.replace('7', "z")));
        assert!(bad(|g| g.mod_id = "Zako Opt".into()));
        assert!(bad(|g| g.name = " ".into()));
        assert!(bad(|g| g.author = Some("Zako\n".into())));

        let twice = with(&zako()).replace("]}", &format!(",{}]}}", serde_json::to_string(&zako()).unwrap()));
        assert!(parse_list(&twice).is_err(), "same repository twice");
    }

    #[test]
    fn github_mod_is_planned_from_its_release() {
        let p = planned_from_github(&zako());
        assert_eq!(p.url, "https://github.com/3ako/ZakoOpt/releases/download/v1.0.3/zakoopt-1.0.3+26.2.jar");
        assert_eq!(p.project, "github:3ako/ZakoOpt");
        assert_eq!(p.version_number, "1.0.3");
        assert_eq!(p.sha256.as_deref(), Some(ZAKO_SHA256));
        assert_eq!(p.mod_id.as_deref(), Some("zakoopt"));
        assert!(!p.required);

        let mut upper = zako();
        upper.sha256 = ZAKO_SHA256.to_uppercase();
        assert_eq!(planned_from_github(&upper).sha256.as_deref(), Some(ZAKO_SHA256));
    }

    #[test]
    fn github_mod_is_checked_by_sha256() {
        let zako = planned_from_github(&zako());
        let file = zako.filename.as_str();
        // Present with the pinned hash: nothing to do.
        let a = plan(&[zako.clone()], &[zako.clone()], files(&[(file, ZAKO_SHA256)]));
        assert_eq!(a, Actions::default());
        // Missing or changed: downloaded again.
        assert_eq!(plan(&[], &[zako.clone()], files(&[])).download, vec![zako.clone()]);
        assert_eq!(plan(&[zako.clone()], &[zako.clone()], files(&[(file, "other")])).download, vec![zako.clone()]);
        // Switched off: removed only if it is still our file.
        assert_eq!(plan(&[zako.clone()], &[], files(&[(file, ZAKO_SHA256)])).remove, vec![file.to_string()]);
        assert!(plan(&[zako.clone()], &[], files(&[(file, "edited")])).remove.is_empty());
    }

    #[test]
    fn offline_drops_disabled_github_mod() {
        let zako = planned_from_github(&zako());
        let state = ManagedState { mods: vec![p("menu", "menu.jar", "1"), zako.clone()], replaced: vec![] };
        assert_eq!(offline_desired(&state, &[]).len(), 2);
        let kept: Vec<String> =
            offline_desired(&state, &[zako.project.clone()]).into_iter().map(|m| m.project).collect();
        assert_eq!(kept, vec!["menu"]);
    }

    #[test]
    fn github_mod_in_mods_tab() {
        let dir = std::env::temp_dir().join(format!("elysium-mods-view-{}", std::process::id()));
        let list = ModList {
            schema: 1,
            minecraft: config::MINECRAFT_VERSION.into(),
            loader: "fabric".into(),
            mods: vec![],
            github: vec![zako()],
        };
        let view = build_view(&dir, Some(&list), &["github:3ako/ZakoOpt".into()]);
        let m = view.builtin.iter().find(|m| m.id == "github:3ako/ZakoOpt").expect("ZakoOpt listed");
        assert_eq!(m.name, "ZakoOpt");
        assert!(!m.required && !m.enabled && !m.installed);
        assert_eq!(m.author.as_deref(), Some("Zako"));
        assert_eq!(m.page.as_deref(), Some("https://github.com/3ako/ZakoOpt"));
    }

    /// Debug builds read the sibling mod project's list.
    #[test]
    fn dev_list_is_valid() {
        if let Some(text) = dev_list() {
            let list = parse_list(&text).unwrap();
            assert!(list.mods.iter().any(|m| m.name == "Iris Shaders" && !m.required));
        }
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;

    /// `cargo test resolve_default_list -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn resolve_default_list() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = download::http_client();
        let list = rt.block_on(load_list(&client)).expect("list");
        let all = rt.block_on(resolve(&client, &list, &[])).expect("resolve");
        for m in &all {
            println!("{} {} {} parents={:?}", m.name, m.version_number, m.filename, m.parents);
            if m.sha256.is_some() {
                assert!(m.url.starts_with(config::GITHUB_URL));
            } else {
                assert!(m.url.starts_with(config::MODRINTH_CDN_PREFIX));
                assert_eq!(m.sha512.len(), 128);
            }
        }
        assert!(all.iter().all(|m| m.project != config::FABRIC_API_PROJECT));
        assert!(all.iter().any(|m| m.project == "eXts2L7r"), "Text Placeholder API pulled in by Mod Menu");
        assert_eq!(all.len(), list.mods.len() + list.github.len() + 1);

        let without_iris = rt.block_on(resolve(&client, &list, &["YL57xq9U".into()])).expect("resolve");
        assert!(without_iris.iter().all(|m| m.project != "YL57xq9U"));
    }

    /// Downloads ZakoOpt from its GitHub release and checks the pinned sha256;
    /// a wrong hash must leave nothing behind.
    /// `cargo test github_mod_download -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn github_mod_download() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = download::http_client();
        let dir = std::env::temp_dir().join(format!("elysium-github-mod-{}", std::process::id()));
        let entry = GithubEntry {
            name: "ZakoOpt".into(),
            repo: "3ako/ZakoOpt".into(),
            tag: "v1.0.3".into(),
            file: "zakoopt-1.0.3+26.2.jar".into(),
            sha256: "72c3b9d04fefa3ea4bf01a667736508e03bd60d987c390696cface9853e2bf0d".into(),
            mod_id: "zakoopt".into(),
            author: Some("Zako".into()),
            required: false,
        };
        let p = planned_from_github(&entry);
        let task = |sha256: &str| DownloadTask {
            url: p.url.clone(),
            path: dir.join(&p.filename),
            sha1: None,
            sha512: None,
            sha256: Some(sha256.into()),
            size: None,
        };

        rt.block_on(download::fetch(&client, &task(&entry.sha256))).expect("download");
        let jar = dir.join(&p.filename);
        assert_eq!(sha256_file(&jar).unwrap(), entry.sha256);
        assert_eq!(jar_mod_json(&jar).unwrap()["id"], "zakoopt");
        assert!(download::is_valid(&task(&entry.sha256), Verify::Full));
        assert!(!download::is_valid(&task(&"0".repeat(64)), Verify::Full));

        std::fs::remove_file(&jar).unwrap();
        assert!(rt.block_on(download::fetch(&client, &task(&"0".repeat(64)))).is_err());
        assert!(!jar.exists() && !jar.with_extension("part").exists(), "rejected file not kept");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    /// Runs against an installed game dir (`ELYSIUM_E2E_DIR`):
    /// `cargo test sync_scenarios -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn sync_scenarios() {
        let game_dir = PathBuf::from(std::env::var("ELYSIUM_E2E_DIR").expect("ELYSIUM_E2E_DIR"));
        let mods = mods_dir(&game_dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = download::http_client();
        let reporter = Reporter::from_fn(|_| {});
        let run = |disabled: &[String]| rt.block_on(sync(&client, &game_dir, disabled, &reporter)).expect("sync");
        let file_of = |project: &str| load_state(&game_dir).mods.into_iter().find(|m| m.project == project).map(|m| m.filename);

        run(&[]);
        let iris = file_of("YL57xq9U").expect("iris installed");

        // A user's own mod survives every sync.
        let user_jar = mods.join("my-own-mod.jar");
        std::fs::write(&user_jar, b"not really a jar").unwrap();

        // Switching Iris off removes it; Sodium stays (required).
        run(&["YL57xq9U".into()]);
        assert!(!mods.join(&iris).exists(), "iris removed");
        assert!(file_of("AANobbMI").is_some_and(|f| mods.join(f).is_file()), "sodium kept");
        assert!(user_jar.is_file(), "user jar kept");

        // Back on: downloaded again.
        run(&[]);
        assert!(mods.join(&iris).is_file(), "iris restored");

        // Version change in the list: the previously installed file is replaced.
        let mut state = load_state(&game_dir);
        let sodium = state.mods.iter_mut().find(|m| m.project == "AANobbMI").unwrap();
        let old = mods.join("sodium-old-version.jar");
        std::fs::copy(mods.join(&sodium.filename), &old).unwrap();
        std::fs::remove_file(mods.join(&sodium.filename)).unwrap();
        sodium.filename = "sodium-old-version.jar".into();
        save_state(&game_dir, &state).unwrap();
        run(&[]);
        assert!(!old.exists(), "old sodium version removed");
        assert!(mods.join(file_of("AANobbMI").unwrap()).is_file(), "new sodium installed");

        // A user copy of a built-in mod (same Modrinth project) is not duplicated.
        let menu = file_of("mOgUt4GM").unwrap();
        let user_menu = mods.join("modmenu-from-user.jar");
        std::fs::rename(mods.join(&menu), &user_menu).unwrap();
        run(&[]);
        assert!(user_menu.is_file() && !mods.join(&menu).exists(), "no duplicate Mod Menu");
        assert!(load_state(&game_dir).replaced.iter().any(|m| m.project == "mOgUt4GM"));

        // Clean up: user removes their copy, built-in comes back.
        std::fs::remove_file(&user_menu).unwrap();
        std::fs::remove_file(&user_jar).unwrap();
        run(&[]);
        assert!(mods.join(&menu).is_file(), "built-in Mod Menu restored");
        assert!(mods.join(format!("fabric-api-{}.jar", config::FABRIC_API_VERSION)).is_file());
        let fabric_api_count = std::fs::read_dir(&mods)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("fabric-api"))
            .count();
        assert_eq!(fabric_api_count, 1, "single Fabric API");

        // Optional GitHub mods (ZakoOpt): removed when switched off, back with the pinned file.
        if let Some(g) = cached_list().and_then(|l| l.github.into_iter().find(|g| !g.required)) {
            let jar = mods.join(&g.file);
            assert!(jar.is_file(), "{} installed", g.name);
            run(&[g.key()]);
            assert!(!jar.exists(), "{} removed", g.name);
            run(&[]);
            assert_eq!(sha256_file(&jar).unwrap(), g.sha256.to_lowercase());
        }

        // What the Mods tab shows.
        let view = build_view(&game_dir, cached_list().as_ref(), &["YL57xq9U".into()]);
        println!("{}", serde_json::to_string_pretty(&view).unwrap());
        let get = |id: &str| view.builtin.iter().find(|m| m.id == id).unwrap();
        assert!(!get("YL57xq9U").required && !get("YL57xq9U").enabled);
        assert!(get("AANobbMI").required && get("AANobbMI").installed);
        assert_eq!(get("eXts2L7r").dependency_of, vec!["Mod Menu".to_string()]);
        assert!(view.user.is_empty());
    }
}
