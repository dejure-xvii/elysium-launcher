//! Minecraft services: login with Xbox and profile lookup.

use serde::Deserialize;
use serde_json::json;

use super::xbox::XboxToken;
use crate::error::{Error, Result};

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Clone, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<Skin>,
}

#[derive(Clone, Deserialize)]
pub struct Skin {
    pub url: String,
    #[serde(default)]
    pub state: String,
}

impl Profile {
    pub fn active_skin(&self) -> Option<String> {
        self.skins
            .iter()
            .find(|s| s.state == "ACTIVE")
            .or_else(|| self.skins.first())
            .map(|s| s.url.replacen("http://", "https://", 1))
    }
}

pub struct McToken {
    pub access_token: String,
    pub expires_in: u64,
}

pub async fn login(client: &reqwest::Client, xsts: &XboxToken) -> Result<McToken> {
    let resp = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={};{}", xsts.user_hash, xsts.token) }))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(Error::msg(
            "Minecraft отклонил вход: Client ID приложения ещё не одобрен Mojang (см. README)",
        ));
    }
    if !resp.status().is_success() {
        return Err(Error::msg(format!("Minecraft: ошибка входа {}", resp.status())));
    }
    let r: LoginResponse = resp.json().await?;
    Ok(McToken { access_token: r.access_token, expires_in: r.expires_in })
}

pub async fn profile(client: &reqwest::Client, mc_token: &str) -> Result<Profile> {
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_token)
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::msg("На этом аккаунте не куплен Minecraft: Java Edition"));
    }
    if !resp.status().is_success() {
        return Err(Error::msg(format!("Minecraft: ошибка профиля {}", resp.status())));
    }
    Ok(resp.json().await?)
}
