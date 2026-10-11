//! Elysium account (site account), separate from the Minecraft accounts in `auth`.
//! Optional: the game launches without it. The session token lives in Windows Credential Manager
//! (like Microsoft refresh tokens) and never reaches the webview; only the public profile is cached on disk.

pub mod api;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{Error, Result};
use crate::paths;
use api::{Authed, User};

const KEYRING_SERVICE: &str = "ElysiumLauncher";
const KEYRING_USER: &str = "elysium:session";

#[derive(Default)]
pub struct ElysiumState {
    user: Mutex<Option<User>>,
}

impl ElysiumState {
    pub fn load() -> Self {
        let user = load_token().ok().flatten().and_then(|_| load_profile());
        ElysiumState { user: Mutex::new(user) }
    }

    fn set(&self, user: Option<User>) {
        *self.user.lock().unwrap() = user;
    }
}

// ---------- secure token storage ----------

fn entry() -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?)
}

fn store_token(token: &str) -> Result<()> {
    entry()?.set_secret(token.as_bytes())?;
    Ok(())
}

/// Session token for authorized API calls (future: config sync, friends).
pub fn load_token() -> Result<Option<String>> {
    match entry()?.get_secret() {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn delete_token() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

// ---------- cached public profile (shown instantly and offline) ----------

fn profile_file() -> PathBuf {
    paths::launcher_dir().join("elysium-account.json")
}

fn load_profile() -> Option<User> {
    std::fs::read_to_string(profile_file()).ok().and_then(|s| serde_json::from_str(&s).ok())
}

fn save_profile(user: &User) -> Result<()> {
    std::fs::create_dir_all(paths::launcher_dir())?;
    std::fs::write(profile_file(), serde_json::to_vec_pretty(user)?)?;
    Ok(())
}

fn forget(state: &ElysiumState) -> Result<()> {
    state.set(None);
    let _ = std::fs::remove_file(profile_file());
    delete_token()
}

/// A one-time ticket for the game, or `None` when the launcher is not signed in.
/// A revoked session signs the launcher out, like `elysium_refresh`.
pub async fn game_ticket(state: &ElysiumState) -> Result<Option<String>> {
    let Some(token) = load_token()? else {
        return Ok(None);
    };
    match api::game_ticket(&token).await? {
        Authed::Ok(t) => Ok(Some(t.ticket)),
        Authed::SignedOut => {
            forget(state)?;
            Ok(None)
        }
    }
}

// ---------- commands ----------

/// Cached account, without a network call.
#[tauri::command]
pub fn elysium_account(state: State<ElysiumState>) -> Option<User> {
    state.user.lock().unwrap().clone()
}

/// Re-checks the session with the server. Offline keeps the cached account;
/// a revoked session (logout everywhere, password reset) signs the launcher out.
#[tauri::command]
pub async fn elysium_refresh(state: State<'_, ElysiumState>) -> Result<Option<User>> {
    let Some(token) = load_token()? else {
        state.set(None);
        return Ok(None);
    };
    match api::me(&token).await {
        Ok(Authed::Ok(user)) => {
            save_profile(&user)?;
            state.set(Some(user.clone()));
            Ok(Some(user))
        }
        Ok(Authed::SignedOut) => {
            forget(&state)?;
            Ok(None)
        }
        Err(_) => Ok(state.user.lock().unwrap().clone()),
    }
}

#[tauri::command]
pub async fn elysium_login(state: State<'_, ElysiumState>, login: String, password: String) -> Result<User> {
    let login = login.trim();
    if login.is_empty() || password.is_empty() {
        return Err(Error::msg("Введите логин или почту и пароль"));
    }
    let res = api::login(login, &password).await?;
    store_token(&res.token)?;
    save_profile(&res.user)?;
    state.set(Some(res.user.clone()));
    Ok(res.user)
}

/// Ends the session on the server (best effort) and removes it from this computer.
#[tauri::command]
pub async fn elysium_logout(state: State<'_, ElysiumState>) -> Result<()> {
    if let Some(token) = load_token()? {
        let _ = api::logout(&token).await;
    }
    forget(&state)
}

/// Opens a page of the Elysium site in the default browser (registration and captcha live there).
#[tauri::command]
pub fn elysium_open_page(app: AppHandle, page: String) -> Result<()> {
    let path = match page.as_str() {
        "register" => "/register",
        "forgot" => "/forgot",
        "account" => "/account",
        "privacy" => "/privacy",
        _ => return Err(Error::msg("Неизвестная страница")),
    };
    app.opener()
        .open_url(api::site_url(path), None::<&str>)
        .map_err(|e| Error::msg(e.to_string()))
}
