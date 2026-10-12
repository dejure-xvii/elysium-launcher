//! HTTP client for the Elysium accounts API (`<site>/api/v1`).
//! Future features (config sync, friends) add their calls here and use `get`/`post` with the session token.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::ELYSIUM_SITE_URL;
use crate::download::http_client;
use crate::error::{Error, Result};

/// Public profile returned by the API. No secrets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    /// Optional on the site; `None` when the account has no e-mail.
    #[serde(default)]
    pub email: Option<String>,
    pub email_verified: bool,
    /// Account number by registration date ("Вы пользователь #N"); missing from servers before the subscription.
    #[serde(default)]
    pub uid: Option<u64>,
}

/// Subscription as the server reports it. `expires_at` is Unix ms, `None` with `forever`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Subscription {
    pub active: bool,
    pub forever: bool,
    pub plan: Option<String>,
    pub expires_at: Option<u64>,
    pub days_left: Option<u64>,
}

impl Subscription {
    /// Whether it still runs at `now` (Unix seconds), judged by the end date alone. Used offline, when
    /// the server can't say `active`, so the offline grace never outlives the subscription.
    pub fn valid_at(&self, now: u64) -> bool {
        self.forever || self.expires_at.is_some_and(|ms| ms / 1000 > now)
    }
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
    #[serde(default)]
    pub subscription: Subscription,
}

/// `/me`: the profile and the subscription.
#[derive(Deserialize)]
pub struct Me {
    pub user: User,
    #[serde(default)]
    pub subscription: Subscription,
}

/// Outcome of a request that needs a session.
pub enum Authed<T> {
    Ok(T),
    /// The token is expired or revoked (logout everywhere, password reset, account deleted).
    SignedOut,
}

pub fn site_url(path: &str) -> String {
    format!("{}{path}", ELYSIUM_SITE_URL.trim_end_matches('/'))
}

/// API endpoint. Release builds talk to the server over HTTPS only (the token travels in every request).
fn endpoint(path: &str) -> Result<String> {
    let url = site_url(&format!("/api/v1/{path}"));
    let local = url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:");
    if url.starts_with("https://") || (cfg!(debug_assertions) && local) {
        Ok(url)
    } else {
        Err(Error::msg("Адрес сервера Elysium должен начинаться с https://"))
    }
}

fn network_error(e: reqwest::Error) -> Error {
    if e.is_connect() || e.is_timeout() {
        Error::msg("Нет связи с сервером Elysium. Проверьте интернет.")
    } else {
        Error::Http(e)
    }
}

/// Turns a non-2xx answer into the server's own Russian message.
async fn api_error(res: reqwest::Response) -> Error {
    let status = res.status();
    let body: Value = res.json().await.unwrap_or(Value::Null);
    match body.get("message").and_then(Value::as_str) {
        Some(msg) => Error::msg(msg),
        None => Error::msg(format!("Сервер Elysium ответил ошибкой {}", status.as_u16())),
    }
}

async fn send<T: DeserializeOwned>(req: reqwest::RequestBuilder) -> Result<Authed<T>> {
    let res = req.send().await.map_err(network_error)?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(Authed::SignedOut);
    }
    if !res.status().is_success() {
        return Err(api_error(res).await);
    }
    Ok(Authed::Ok(res.json().await?))
}

pub async fn get<T: DeserializeOwned>(path: &str, token: &str) -> Result<Authed<T>> {
    send(http_client().get(endpoint(path)?).bearer_auth(token)).await
}

pub async fn post<T: DeserializeOwned>(path: &str, token: &str, body: &Value) -> Result<Authed<T>> {
    send(http_client().post(endpoint(path)?).bearer_auth(token).json(body)).await
}

pub async fn login(login: &str, password: &str) -> Result<LoginResponse> {
    let body = serde_json::json!({ "login": login, "password": password, "client": "launcher" });
    let res = http_client()
        .post(endpoint("login")?)
        .json(&body)
        .send()
        .await
        .map_err(network_error)?;
    if !res.status().is_success() {
        return Err(api_error(res).await);
    }
    Ok(res.json().await?)
}

pub async fn me(token: &str) -> Result<Authed<Me>> {
    get::<Me>("me", token).await
}

/// The current mod release; served only to launcher sessions with an active subscription.
#[derive(Clone, Deserialize)]
pub struct ModRelease {
    pub version: String,
    pub sha256: String,
    #[serde(default)]
    pub notes: String,
}

pub async fn mod_release(token: &str) -> Result<Authed<ModRelease>> {
    get::<ModRelease>("releases/mod", token).await
}

/// Downloads the mod jar to `path`, checked against the release's SHA-256 (a partial or foreign
/// file never replaces the working one: it is written next to it and moved only when it matches).
pub async fn download_mod(token: &str, release: &ModRelease, path: &std::path::Path) -> Result<Authed<()>> {
    let res = http_client().get(endpoint("download/mod")?).bearer_auth(token).send().await.map_err(network_error)?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(Authed::SignedOut);
    }
    if !res.status().is_success() {
        return Err(api_error(res).await);
    }
    let bytes = res.bytes().await.map_err(network_error)?;
    use sha2::Digest;
    if hex::encode(sha2::Sha256::digest(&bytes)) != release.sha256.to_lowercase() {
        return Err(Error::msg("Файл мода повреждён при загрузке — попробуйте ещё раз"));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("jar.part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(Authed::Ok(()))
}

#[derive(Deserialize)]
pub struct GameTicket {
    pub ticket: String,
}

/// One-time ticket (2 minutes) that signs the game in to Elysium Social. The launcher token itself
/// never reaches the game.
pub async fn game_ticket(token: &str) -> Result<Authed<GameTicket>> {
    post::<GameTicket>("game-ticket", token, &serde_json::json!({})).await
}

pub async fn logout(token: &str) -> Result<()> {
    post::<Value>("logout", token, &serde_json::json!({})).await?;
    Ok(())
}

/// Against a local `npm run dev` of elysium-site:
/// `ELYSIUM_SITE_URL=http://127.0.0.1:8790 cargo test elysium_session_lifecycle -- --ignored`
/// (run one test at a time: failed logins from 127.0.0.1 add up and the IP limit then asks for a captcha).
#[cfg(test)]
mod tests {
    use super::*;

    async fn register(name: &str, password: &str) {
        let body = serde_json::json!({
            "username": name, "email": format!("{}@example.com", name.to_lowercase()),
            "password": password, "password2": password, "turnstileToken": "XXXX.DUMMY.TOKEN.XXXX",
        });
        let res = http_client()
            .post(site_url("/api/v1/register"))
            .header("Origin", site_url(""))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert!(res.status().is_success(), "register: {}", res.status());
    }

    fn rand_name() -> String {
        format!("Launcher_{}", &uuid::Uuid::new_v4().simple().to_string()[..6])
    }

    #[tokio::test]
    #[ignore]
    async fn elysium_session_lifecycle() {
        let name = rand_name();
        register(&name, "Launcher#Pass-2026").await;

        let wrong = login(&name, "wrong-password").await.err().unwrap().to_string();
        assert_eq!(wrong, "Неверный логин или пароль");

        let res = login(&name.to_uppercase(), "Launcher#Pass-2026").await.unwrap();
        assert_eq!(res.user.username, name);
        let Authed::Ok(m) = me(&res.token).await.unwrap() else { panic!("me: signed out") };
        assert_eq!(m.user.id, res.user.id);
        assert!(m.user.uid.is_some(), "UID from the server");
        assert!(!m.subscription.active, "a new account has no subscription");
        // Without a subscription the server gives no mod.
        let no_mod = mod_release(&res.token).await.err().expect("402 without a subscription").to_string();
        assert_eq!(no_mod, "Подписка не активна. Купите или активируйте ключ в кабинете.");

        logout(&res.token).await.unwrap();
        assert!(matches!(me(&res.token).await.unwrap(), Authed::SignedOut));
        assert!(matches!(me("garbage-token").await.unwrap(), Authed::SignedOut));
    }

    #[tokio::test]
    #[ignore]
    async fn elysium_brute_force_message() {
        let name = rand_name();
        register(&name, "Launcher#Pass-2026").await;
        for i in 0..5 {
            assert!(login(&name, &format!("wrong-{i}-pass")).await.is_err());
        }
        let msg = login(&name, "Launcher#Pass-2026").await.err().unwrap().to_string();
        assert_eq!(msg, "Слишком много попыток — подождите 15 минут или войдите на сайте");
    }
}
