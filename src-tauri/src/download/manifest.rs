//! Mojang version manifest and version JSON model, plus rule evaluation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{fetch, get_json, is_valid, DownloadTask, Verify};
use crate::config;
use crate::error::{Error, Result};

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    id: String,
    url: String,
    sha1: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub asset_index: Option<Artifact>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default, rename = "type")]
    pub version_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Ruled { rules: Vec<Rule>, value: StringOrVec },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

impl StringOrVec {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            StringOrVec::One(s) => vec![s.clone()],
            StringOrVec::Many(v) => v.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base (Fabric-style libraries).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Downloads {
    #[serde(default)]
    pub client: Option<Artifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

/// Feature flags used by argument rules.
#[derive(Default, Clone)]
pub struct Features {
    pub custom_resolution: bool,
}

pub fn os_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x86_64"
    }
}

fn os_rule_matches(os: &OsRule) -> bool {
    if let Some(name) = &os.name {
        if name != "windows" {
            return false;
        }
    }
    if let Some(arch) = &os.arch {
        if arch != os_arch() {
            return false;
        }
    }
    true
}

/// Mojang rule semantics: start with "disallow", last matching rule wins.
pub fn rules_allow(rules: &[Rule], features: &Features) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_ok = rule.os.as_ref().map(os_rule_matches).unwrap_or(true);
        let features_ok = rule.features.as_ref().map_or(true, |f| {
            f.iter().all(|(k, v)| {
                let have = match k.as_str() {
                    "has_custom_resolution" => features.custom_resolution,
                    _ => false,
                };
                have == *v
            })
        });
        if os_ok && features_ok {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

pub fn version_dir(game_dir: &Path, id: &str) -> PathBuf {
    game_dir.join("versions").join(id)
}

/// Loads the vanilla version JSON, downloading it when needed.
/// Falls back to the cached copy when offline.
pub async fn fetch_vanilla(client: &reqwest::Client, game_dir: &Path) -> Result<VersionJson> {
    let id = config::MINECRAFT_VERSION;
    let path = version_dir(game_dir, id).join(format!("{id}.json"));

    let online: Result<()> = async {
        let manifest: Manifest = get_json(client, config::VERSION_MANIFEST_URL).await?;
        let entry = manifest
            .versions
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| Error::msg(format!("Версия {id} не найдена на серверах Mojang")))?;
        let task = DownloadTask { url: entry.url, path: path.clone(), sha1: entry.sha1, sha512: None, size: None };
        if !is_valid(&task, Verify::Full) {
            fetch(client, &task).await?;
        }
        Ok(())
    }
    .await;

    match (online, path.exists()) {
        (Ok(()), _) | (Err(_), true) => {
            let text = tokio::fs::read_to_string(&path).await?;
            Ok(serde_json::from_str(&text)?)
        }
        (Err(e), false) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(action: &str, os: Option<(&str, Option<&str>)>) -> Rule {
        Rule {
            action: action.into(),
            os: os.map(|(n, a)| OsRule { name: Some(n.into()), arch: a.map(Into::into) }),
            features: None,
        }
    }

    #[test]
    fn rules() {
        let f = Features::default();
        assert!(rules_allow(&[], &f));
        assert!(rules_allow(&[rule("allow", Some(("windows", None)))], &f));
        assert!(!rules_allow(&[rule("allow", Some(("osx", None)))], &f));
        assert!(!rules_allow(&[rule("allow", None), rule("disallow", Some(("windows", None)))], &f));
        assert!(!rules_allow(&[rule("allow", Some(("windows", Some("arm64"))))], &f) || os_arch() == "arm64");
    }

    #[test]
    fn parses_ruled_argument() {
        let a: Argument = serde_json::from_str(
            r#"{"rules":[{"action":"allow","features":{"has_custom_resolution":true}}],"value":["--width","${resolution_width}"]}"#,
        )
        .unwrap();
        match a {
            Argument::Ruled { rules, value } => {
                assert!(!rules_allow(&rules, &Features::default()));
                assert!(rules_allow(&rules, &Features { custom_resolution: true }));
                assert_eq!(value.to_vec().len(), 2);
            }
            _ => panic!(),
        }
    }
}
