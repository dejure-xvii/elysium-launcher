//! Account list (`accounts.json`) and refresh tokens (Windows Credential Manager).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

const KEYRING_SERVICE: &str = "ElysiumLauncher";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Microsoft,
    Offline,
}

/// Public account data. Secrets never live here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub kind: AccountKind,
    pub username: String,
    pub uuid: String,
    #[serde(default)]
    pub skin_url: Option<String>,
}

fn file() -> PathBuf {
    paths::launcher_dir().join("accounts.json")
}

pub fn load() -> Vec<Account> {
    std::fs::read_to_string(file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(accounts: &[Account]) -> Result<()> {
    std::fs::create_dir_all(paths::launcher_dir())?;
    let tmp = file().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(accounts)?)?;
    std::fs::rename(tmp, file())?;
    Ok(())
}

fn entry(account_id: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(KEYRING_SERVICE, &format!("msa:{account_id}"))?)
}

pub fn store_refresh_token(account_id: &str, token: &str) -> Result<()> {
    entry(account_id)?.set_secret(token.as_bytes())?;
    Ok(())
}

pub fn load_refresh_token(account_id: &str) -> Result<Option<String>> {
    match entry(account_id)?.get_secret() {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_refresh_token(account_id: &str) -> Result<()> {
    match entry(account_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
