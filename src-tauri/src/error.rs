use serde::{Serialize, Serializer};

/// Error type returned by every command; serialized to the UI as a plain message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error("Ошибка сети: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Ошибка файловой системы: {0}")]
    Io(#[from] std::io::Error),
    #[error("Некорректные данные: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Ошибка распаковки: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Хранилище учётных данных: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("{0}")]
    Tauri(#[from] tauri::Error),
    #[error("Обновление лаунчера: {0}")]
    Updater(#[from] tauri_plugin_updater::Error),
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Error::Msg(s.into())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
