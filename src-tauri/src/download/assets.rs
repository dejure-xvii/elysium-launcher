//! Asset index and content-addressed asset objects.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use super::manifest::VersionJson;
use super::{fetch, is_valid, DownloadTask, Verify};
use crate::config;
use crate::error::{Error, Result};

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

/// Downloads the asset index (if needed) and returns tasks for every object.
pub async fn resolve(
    client: &reqwest::Client,
    version: &VersionJson,
    game_dir: &Path,
) -> Result<(String, Vec<DownloadTask>)> {
    let index_ref = version
        .asset_index
        .as_ref()
        .ok_or_else(|| Error::msg("В описании версии нет assetIndex"))?;
    let id = index_ref
        .id
        .clone()
        .or_else(|| version.assets.clone())
        .unwrap_or_else(|| "legacy".into());
    let assets_dir = game_dir.join("assets");
    let index_path = assets_dir.join("indexes").join(format!("{id}.json"));

    let task = DownloadTask {
        url: index_ref.url.clone(),
        path: index_path.clone(),
        sha1: index_ref.sha1.clone(),
        size: index_ref.size,
        sha512: None,
        sha256: None,
    };
    if !is_valid(&task, Verify::Full) {
        // Keep working offline if a cached index exists.
        if let Err(e) = fetch(client, &task).await {
            if !index_path.exists() {
                return Err(e);
            }
        }
    }

    let index: AssetIndex = serde_json::from_str(&tokio::fs::read_to_string(&index_path).await?)?;
    let objects_dir = assets_dir.join("objects");
    let mut seen = std::collections::HashSet::new();
    let tasks = index
        .objects
        .into_values()
        .filter(|o| seen.insert(o.hash.clone()))
        .map(|o| {
            let prefix = &o.hash[..2];
            DownloadTask {
                url: format!("{}/{prefix}/{}", config::RESOURCES_URL, o.hash),
                path: objects_dir.join(prefix).join(&o.hash),
                sha1: Some(o.hash.clone()),
                size: Some(o.size),
                sha512: None,
                sha256: None,
            }
        })
        .collect();
    Ok((id, tasks))
}
