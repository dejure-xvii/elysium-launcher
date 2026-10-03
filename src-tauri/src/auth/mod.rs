//! Accounts: Microsoft (MSA → Xbox Live → XSTS → Minecraft) and offline.

pub mod microsoft;
pub mod minecraft;
pub mod offline;
pub mod store;
pub mod xbox;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, State};

use crate::config;
use crate::download::http_client;
use crate::error::{Error, Result};
use crate::settings::{self, SettingsState};
pub use store::{Account, AccountKind};

struct Session {
    access_token: String,
    expires_at: Instant,
}

#[derive(Default)]
pub struct AuthState {
    pub accounts: Mutex<Vec<Account>>,
    /// Minecraft access tokens are kept in memory only.
    sessions: Mutex<HashMap<String, Session>>,
    pending: Mutex<Option<microsoft::DeviceCode>>,
    cancel: AtomicBool,
}

impl AuthState {
    pub fn load() -> Self {
        AuthState { accounts: Mutex::new(store::load()), ..Default::default() }
    }

    fn upsert(&self, account: Account) -> Result<()> {
        let mut list = self.accounts.lock().unwrap();
        match list.iter_mut().find(|a| a.id == account.id) {
            Some(existing) => *existing = account,
            None => list.push(account),
        }
        store::save(&list)
    }
}

/// Credentials passed to the game.
pub struct LaunchAuth {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
    pub user_type: &'static str,
}

/// Full chain from a Microsoft token to a Minecraft session + profile.
async fn authenticate(client: &reqwest::Client, ms: &microsoft::MsTokens) -> Result<(Session, minecraft::Profile)> {
    let xbl = xbox::user_token(client, &ms.access_token).await?;
    let xsts = xbox::xsts_token(client, &xbl.token).await?;
    let mc = minecraft::login(client, &xsts).await?;
    let profile = minecraft::profile(client, &mc.access_token).await?;
    let session = Session {
        access_token: mc.access_token,
        expires_at: Instant::now() + Duration::from_secs(mc.expires_in),
    };
    Ok((session, profile))
}

fn account_from_profile(p: &minecraft::Profile) -> Account {
    Account {
        id: p.id.clone(),
        kind: AccountKind::Microsoft,
        username: p.name.clone(),
        uuid: p.id.clone(),
        skin_url: p.active_skin(),
    }
}

fn select(app: &AppHandle, id: Option<String>) -> Result<()> {
    let state = app.state::<SettingsState>();
    let mut s = state.0.lock().unwrap();
    s.selected_account = id;
    settings::save(&s)
}

/// Resolves the selected account into launch credentials, refreshing MS sessions.
pub async fn prepare_launch(app: &AppHandle, selected: Option<&str>) -> Result<LaunchAuth> {
    let state = app.state::<AuthState>();
    let account = {
        let list = state.accounts.lock().unwrap();
        selected
            .and_then(|id| list.iter().find(|a| a.id == id))
            .or_else(|| list.first())
            .cloned()
    }
    .ok_or_else(|| Error::msg("Добавьте аккаунт, чтобы играть"))?;

    if account.kind == AccountKind::Offline {
        return Ok(LaunchAuth {
            username: account.username,
            uuid: account.uuid,
            access_token: "0".into(),
            xuid: "0".into(),
            user_type: "msa",
        });
    }

    let cached = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .get(&account.id)
            .filter(|s| s.expires_at > Instant::now() + Duration::from_secs(300))
            .map(|s| s.access_token.clone())
    };
    let (token, account) = match cached {
        Some(t) => (t, account),
        None => {
            let refresh_token = store::load_refresh_token(&account.id)?
                .ok_or_else(|| Error::msg("Вход Microsoft не сохранён — войдите в аккаунт заново"))?;
            let client = http_client();
            let ms = microsoft::refresh(&client, &refresh_token).await?;
            store::store_refresh_token(&account.id, &ms.refresh_token)?;
            let (session, profile) = authenticate(&client, &ms).await?;
            let token = session.access_token.clone();
            state.sessions.lock().unwrap().insert(account.id.clone(), session);
            let updated = account_from_profile(&profile);
            state.upsert(updated.clone())?;
            let _ = tauri::Emitter::emit(app, "accounts-changed", ());
            (token, updated)
        }
    };

    Ok(LaunchAuth {
        username: account.username,
        uuid: account.uuid,
        access_token: token,
        xuid: "0".into(),
        user_type: "msa",
    })
}

#[tauri::command]
pub fn list_accounts(state: State<AuthState>) -> Vec<Account> {
    state.accounts.lock().unwrap().clone()
}

#[tauri::command]
pub fn add_offline_account(app: AppHandle, state: State<AuthState>, username: String) -> Result<Account> {
    let username = username.trim().to_string();
    if !offline::is_valid_name(&username) {
        return Err(Error::msg("Ник: 3–16 символов, только латиница, цифры и _"));
    }
    let uuid = offline::offline_uuid(&username);
    let account = Account {
        id: format!("offline-{uuid}"),
        kind: AccountKind::Offline,
        username,
        uuid,
        skin_url: None,
    };
    state.upsert(account.clone())?;
    select(&app, Some(account.id.clone()))?;
    Ok(account)
}

#[tauri::command]
pub fn remove_account(app: AppHandle, state: State<AuthState>, id: String) -> Result<()> {
    let removed = {
        let mut list = state.accounts.lock().unwrap();
        let before = list.len();
        let kind = list.iter().find(|a| a.id == id).map(|a| a.kind);
        list.retain(|a| a.id != id);
        store::save(&list)?;
        (before != list.len()).then_some(kind).flatten()
    };
    if removed == Some(AccountKind::Microsoft) {
        store::delete_refresh_token(&id)?;
        state.sessions.lock().unwrap().remove(&id);
    }
    let selected = app.state::<SettingsState>().0.lock().unwrap().selected_account.clone();
    if selected.as_deref() == Some(id.as_str()) {
        let first = state.accounts.lock().unwrap().first().map(|a| a.id.clone());
        select(&app, first)?;
    }
    Ok(())
}

#[tauri::command]
pub fn select_account(app: AppHandle, id: String) -> Result<()> {
    select(&app, Some(id))
}

/// Starts device-code sign-in; the UI shows the code and opens the browser.
#[tauri::command]
pub async fn ms_login_start(state: State<'_, AuthState>) -> Result<microsoft::DeviceCode> {
    if !config::ms_client_configured() {
        return Err(Error::msg("Вход через Microsoft не настроен: укажите Client ID (см. README)"));
    }
    let code = microsoft::device_code(&http_client()).await?;
    state.cancel.store(false, Ordering::SeqCst);
    *state.pending.lock().unwrap() = Some(code.clone());
    Ok(code)
}

/// Waits for the user to confirm the code, then completes the chain.
#[tauri::command]
pub async fn ms_login_finish(app: AppHandle, state: State<'_, AuthState>) -> Result<Account> {
    let code = state
        .pending
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| Error::msg("Вход не начат"))?;
    let client = http_client();
    let ms = microsoft::poll_token(&client, &code, &state.cancel).await?;
    let (session, profile) = authenticate(&client, &ms).await?;
    let account = account_from_profile(&profile);
    store::store_refresh_token(&account.id, &ms.refresh_token)?;
    state.sessions.lock().unwrap().insert(account.id.clone(), session);
    state.upsert(account.clone())?;
    select(&app, Some(account.id.clone()))?;
    Ok(account)
}

#[tauri::command]
pub fn ms_login_cancel(state: State<AuthState>) {
    state.cancel.store(true, Ordering::SeqCst);
    state.pending.lock().unwrap().take();
}
