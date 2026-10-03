//! Microsoft OAuth 2.0 device code flow (consumer accounts).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::{Error, Result};

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    #[serde(rename(deserialize = "user_code"))]
    pub user_code: String,
    #[serde(rename(deserialize = "device_code"))]
    pub device_code: String,
    #[serde(rename(deserialize = "verification_uri"))]
    pub verification_uri: String,
    #[serde(rename(deserialize = "expires_in"))]
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Deserialize)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

pub async fn device_code(client: &reqwest::Client) -> Result<DeviceCode> {
    let resp = client
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", config::MS_CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await?;
    if !resp.status().is_success() {
        let err: OAuthError = resp.json().await?;
        return Err(Error::msg(format!("Microsoft: {}", err.error_description)));
    }
    Ok(resp.json().await?)
}

/// Polls the token endpoint until the user confirms the code in the browser.
pub async fn poll_token(
    client: &reqwest::Client,
    code: &DeviceCode,
    cancel: &AtomicBool,
) -> Result<MsTokens> {
    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = Duration::from_secs(code.interval.max(1));
    loop {
        tokio::time::sleep(interval).await;
        if cancel.load(Ordering::SeqCst) {
            return Err(Error::msg("Вход отменён"));
        }
        if Instant::now() > deadline {
            return Err(Error::msg("Код истёк, начните вход заново"));
        }
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", config::MS_CLIENT_ID),
                ("device_code", code.device_code.as_str()),
            ])
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(resp.json().await?);
        }
        let err: OAuthError = resp.json().await?;
        match err.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += Duration::from_secs(5),
            "authorization_declined" => return Err(Error::msg("Вход отклонён в браузере")),
            "expired_token" => return Err(Error::msg("Код истёк, начните вход заново")),
            _ => return Err(Error::msg(format!("Microsoft: {}", err.error_description))),
        }
    }
}

pub async fn refresh(client: &reqwest::Client, refresh_token: &str) -> Result<MsTokens> {
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", config::MS_CLIENT_ID),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(Error::msg("Сессия Microsoft истекла — войдите в аккаунт заново"));
    }
    Ok(resp.json().await?)
}
