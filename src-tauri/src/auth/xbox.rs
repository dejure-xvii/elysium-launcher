//! Xbox Live user token and XSTS token for Minecraft services.

use serde::Deserialize;
use serde_json::json;

use crate::error::{Error, Result};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxResponse {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: String,
}

#[derive(Deserialize)]
struct XstsError {
    #[serde(rename = "XErr")]
    xerr: u64,
}

pub struct XboxToken {
    pub token: String,
    pub user_hash: String,
}

fn into_token(r: XboxResponse) -> Result<XboxToken> {
    let user_hash = r
        .display_claims
        .xui
        .into_iter()
        .next()
        .map(|x| x.uhs)
        .ok_or_else(|| Error::msg("Xbox Live не вернул user hash"))?;
    Ok(XboxToken { token: r.token, user_hash })
}

pub async fn user_token(client: &reqwest::Client, ms_access_token: &str) -> Result<XboxToken> {
    let resp = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(Error::msg(format!("Xbox Live: ошибка {}", resp.status())));
    }
    into_token(resp.json().await?)
}

fn xsts_message(code: u64) -> String {
    match code {
        2148916233 => "У этого аккаунта Microsoft нет профиля Xbox. Зайдите на xbox.com и создайте его.".into(),
        2148916235 => "Xbox Live недоступен в вашей стране.".into(),
        2148916236 | 2148916237 => "Аккаунту нужна верификация возраста на xbox.com.".into(),
        2148916238 => "Это детский аккаунт — его нужно добавить в семейную группу Microsoft.".into(),
        _ => format!("Xbox Live отказал во входе (код {code})"),
    }
}

pub async fn xsts_token(client: &reqwest::Client, user_token: &str) -> Result<XboxToken> {
    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Accept", "application/json")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [user_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let err: XstsError = resp.json().await?;
        return Err(Error::msg(xsts_message(err.xerr)));
    }
    if !resp.status().is_success() {
        return Err(Error::msg(format!("XSTS: ошибка {}", resp.status())));
    }
    into_token(resp.json().await?)
}
