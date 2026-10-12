//! Elysium account (site account), separate from the Minecraft accounts in `auth`.
//! Required to play since 1.4.0. The session token lives in Windows Credential Manager
//! (like Microsoft refresh tokens) and never reaches the webview; only the public profile is cached on disk.
//! When the accounts server can't be reached, a session checked within `OFFLINE_GRACE` still lets the game start.

pub mod api;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{Error, Result};
use crate::paths;
use api::{Authed, User};

const KEYRING_SERVICE: &str = "ElysiumLauncher";
const KEYRING_USER: &str = "elysium:session";

/// How long the launcher keeps playing without reaching the accounts server, since the last successful check.
pub const OFFLINE_GRACE: u64 = 7 * 24 * 60 * 60;

/// Event for the UI when the session turned out to be revoked (e.g. while asking for a game ticket).
pub const SIGNED_OUT_EVENT: &str = "elysium-signed-out";

/// Profile cached on disk, with the time of the last successful server check.
/// Files from 1.3.0 hold just the profile and read as never checked.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cached {
    #[serde(flatten)]
    user: User,
    #[serde(default)]
    last_verified_at: u64,
}

/// Whether the launcher may be used, as shown by the UI.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    /// Checked with the server just now.
    #[serde(rename_all = "camelCase")]
    Online { user: User },
    /// The server is unreachable, the session was checked within `OFFLINE_GRACE`.
    #[serde(rename_all = "camelCase")]
    Offline { user: User, last_verified_at: u64 },
    /// No session (never signed in, signed out or revoked): the sign-in screen.
    SignedOut,
    /// A session exists, but the server has been unreachable for longer than `OFFLINE_GRACE`.
    Unreachable,
}

impl Status {
    pub fn can_play(&self) -> bool {
        matches!(self, Status::Online { .. } | Status::Offline { .. })
    }
}

/// Answer of the accounts server to `/me`.
enum Probe {
    Ok(User),
    SignedOut,
    Unavailable,
}

fn decide(cached: Option<&Cached>, probe: Probe, now: u64) -> Status {
    match probe {
        Probe::Ok(user) => Status::Online { user },
        Probe::SignedOut => Status::SignedOut,
        Probe::Unavailable => match cached {
            Some(c) if c.last_verified_at > 0 && now.saturating_sub(c.last_verified_at) <= OFFLINE_GRACE => {
                Status::Offline { user: c.user.clone(), last_verified_at: c.last_verified_at }
            }
            _ => Status::Unreachable,
        },
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub struct ElysiumState {
    status: Mutex<Status>,
}

impl ElysiumState {
    /// Starts signed out; the UI asks for `elysium_refresh` right away.
    pub fn load() -> Self {
        ElysiumState { status: Mutex::new(Status::SignedOut) }
    }

    fn set(&self, status: Status) -> Status {
        *self.status.lock().unwrap() = status.clone();
        status
    }

    pub fn can_play(&self) -> bool {
        self.status.lock().unwrap().can_play()
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

fn load_profile() -> Option<Cached> {
    std::fs::read_to_string(profile_file()).ok().and_then(|s| serde_json::from_str(&s).ok())
}

/// Saves the profile as just checked with the server.
fn save_profile(user: &User) -> Result<()> {
    let cached = Cached { user: user.clone(), last_verified_at: now() };
    std::fs::create_dir_all(paths::launcher_dir())?;
    std::fs::write(profile_file(), serde_json::to_vec_pretty(&cached)?)?;
    Ok(())
}

fn forget(state: &ElysiumState) -> Result<()> {
    state.set(Status::SignedOut);
    let _ = std::fs::remove_file(profile_file());
    delete_token()
}

/// A one-time ticket for the game, or `None` when the server is unreachable (the game then starts
/// without Elysium Social). A revoked session signs the launcher out and tells the UI.
pub async fn game_ticket(app: &AppHandle, state: &ElysiumState) -> Result<Option<String>> {
    let Some(token) = load_token()? else {
        return Ok(None);
    };
    match api::game_ticket(&token).await? {
        Authed::Ok(t) => Ok(Some(t.ticket)),
        Authed::SignedOut => {
            forget(state)?;
            let _ = app.emit(SIGNED_OUT_EVENT, ());
            Err(Error::msg("Сессия аккаунта Elysium завершена — войдите снова"))
        }
    }
}

// ---------- commands ----------

/// Checks the session with the server (quietly, on every start). A revoked session
/// (logout everywhere, password reset) signs the launcher out; an unreachable server
/// keeps a recently checked session playable for `OFFLINE_GRACE`.
#[tauri::command]
pub async fn elysium_refresh(state: State<'_, ElysiumState>) -> Result<Status> {
    let Some(token) = load_token()? else {
        return Ok(state.set(Status::SignedOut));
    };
    let probe = match api::me(&token).await {
        Ok(Authed::Ok(user)) => {
            save_profile(&user)?;
            Probe::Ok(user)
        }
        Ok(Authed::SignedOut) => {
            forget(&state)?;
            Probe::SignedOut
        }
        Err(_) => Probe::Unavailable,
    };
    Ok(state.set(decide(load_profile().as_ref(), probe, now())))
}

#[tauri::command]
pub async fn elysium_login(state: State<'_, ElysiumState>, login: String, password: String) -> Result<Status> {
    let login = login.trim();
    if login.is_empty() || password.is_empty() {
        return Err(Error::msg("Введите логин или почту и пароль"));
    }
    let res = api::login(login, &password).await?;
    store_token(&res.token)?;
    save_profile(&res.user)?;
    Ok(state.set(Status::Online { user: res.user }))
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

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 60 * 60;
    const NOW: u64 = 1_800_000_000;

    fn user() -> User {
        User { id: "u1".into(), username: "dejure".into(), email: None, email_verified: false }
    }

    fn checked(ago: u64) -> Cached {
        Cached { user: user(), last_verified_at: NOW - ago }
    }

    #[test]
    fn server_answer_wins() {
        assert_eq!(decide(None, Probe::Ok(user()), NOW), Status::Online { user: user() });
        // 401: the sign-in screen even with a fresh cache.
        assert_eq!(decide(Some(&checked(60)), Probe::SignedOut, NOW), Status::SignedOut);
    }

    #[test]
    fn offline_for_seven_days() {
        let s = decide(Some(&checked(6 * DAY)), Probe::Unavailable, NOW);
        assert_eq!(s, Status::Offline { user: user(), last_verified_at: NOW - 6 * DAY });
        assert!(s.can_play());
        assert!(decide(Some(&checked(OFFLINE_GRACE)), Probe::Unavailable, NOW).can_play());
        let late = decide(Some(&checked(OFFLINE_GRACE + 1)), Probe::Unavailable, NOW);
        assert_eq!(late, Status::Unreachable);
        assert!(!late.can_play());
    }

    #[test]
    fn never_checked_is_not_offline() {
        assert_eq!(decide(None, Probe::Unavailable, NOW), Status::Unreachable);
        // A profile cached by 1.3.0 has no check time.
        let old: Cached = serde_json::from_str(r#"{"id":"u1","username":"dejure","emailVerified":false}"#).unwrap();
        assert_eq!(old.last_verified_at, 0);
        assert_eq!(decide(Some(&old), Probe::Unavailable, NOW), Status::Unreachable);
    }

    #[test]
    fn status_json_for_the_ui() {
        let json = serde_json::to_value(Status::Offline { user: user(), last_verified_at: 5 }).unwrap();
        assert_eq!(json["state"], "offline");
        assert_eq!(json["lastVerifiedAt"], 5);
        assert_eq!(json["user"]["username"], "dejure");
        assert_eq!(serde_json::to_value(Status::SignedOut).unwrap()["state"], "signedOut");
    }
}
