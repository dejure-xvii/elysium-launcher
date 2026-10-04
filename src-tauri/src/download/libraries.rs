//! Library resolution (Mojang + Maven-style), classpath and natives.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::manifest::{rules_allow, Features, Library, VersionJson};
use super::DownloadTask;
use crate::error::Result;

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

pub struct ResolvedLibraries {
    pub downloads: Vec<DownloadTask>,
    pub classpath: Vec<PathBuf>,
    /// Jars whose native libraries must be unpacked before launch.
    pub native_jars: Vec<PathBuf>,
}

/// `group:artifact:version[:classifier][@ext]` → `group/path/artifact/version/artifact-version[-classifier].ext`
pub fn maven_path(name: &str) -> String {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    let group = parts.first().copied().unwrap_or_default().replace('.', "/");
    let artifact = parts.get(1).copied().unwrap_or_default();
    let version = parts.get(2).copied().unwrap_or_default();
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    format!("{group}/{artifact}/{version}/{file}")
}

/// Identity of a library ignoring its version, used to drop duplicates.
fn library_key(name: &str) -> String {
    let coords = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = coords.split(':').collect();
    match parts.as_slice() {
        [g, a, _v, c, ..] => format!("{g}:{a}:{c}"),
        [g, a, ..] => format!("{g}:{a}"),
        _ => coords.to_string(),
    }
}

fn join_url(base: &str, path: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

fn add(out: &mut ResolvedLibraries, path: PathBuf, url: String, sha1: Option<String>, size: Option<u64>) {
    if !url.is_empty() {
        out.downloads.push(DownloadTask { url, path: path.clone(), sha1, sha512: None, size });
    }
}

pub fn resolve(version: &VersionJson, game_dir: &Path) -> ResolvedLibraries {
    let lib_dir = game_dir.join("libraries");
    let features = Features::default();
    let mut seen = HashSet::new();
    let mut out = ResolvedLibraries { downloads: vec![], classpath: vec![], native_jars: vec![] };

    for lib in &version.libraries {
        if let Some(rules) = &lib.rules {
            if !rules_allow(rules, &features) {
                continue;
            }
        }
        if !seen.insert(library_key(&lib.name)) {
            continue;
        }
        resolve_one(lib, &lib_dir, &mut out);
    }
    out
}

fn resolve_one(lib: &Library, lib_dir: &Path, out: &mut ResolvedLibraries) {
    let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());

    // Main artifact.
    if let Some(a) = artifact {
        let rel = a.path.clone().unwrap_or_else(|| maven_path(&lib.name));
        let path = lib_dir.join(&rel);
        add(out, path.clone(), a.url.clone(), a.sha1.clone(), a.size);
        if lib.name.contains(":natives-") {
            out.native_jars.push(path.clone());
        }
        out.classpath.push(path);
    } else if lib.natives.is_none() || lib.downloads.is_none() {
        let rel = maven_path(&lib.name);
        let base = lib.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
        let path = lib_dir.join(&rel);
        add(out, path.clone(), join_url(base, &rel), lib.sha1.clone(), lib.size);
        out.classpath.push(path);
    }

    // Legacy natives (`natives` + `classifiers`).
    if let Some(natives) = &lib.natives {
        if let Some(classifier) = natives.get("windows") {
            let classifier = classifier.replace("${arch}", "64");
            let native = lib
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&classifier));
            if let Some(a) = native {
                let rel = a
                    .path
                    .clone()
                    .unwrap_or_else(|| maven_path(&format!("{}:{classifier}", lib.name)));
                let path = lib_dir.join(rel);
                add(out, path.clone(), a.url.clone(), a.sha1.clone(), a.size);
                out.native_jars.push(path);
            }
        }
    }
}

/// Unpacks `.dll` files from native jars into `dir`.
pub fn extract_natives(jars: &[PathBuf], dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for jar in jars {
        let file = std::fs::File::open(jar)?;
        let mut zip = zip::ZipArchive::new(file)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir() || name.starts_with("META-INF") || !name.ends_with(".dll") {
                continue;
            }
            let file_name = Path::new(&name).file_name().map(|n| n.to_owned());
            let Some(file_name) = file_name else { continue };
            let target = dir.join(file_name);
            if target.exists() && std::fs::metadata(&target)?.len() == entry.size() {
                continue;
            }
            let mut out = std::fs::File::create(&target)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.19.5"),
            "net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows"),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(maven_path("a.b:c:1@zip"), "a/b/c/1/c-1.zip");
    }

    #[test]
    fn keys() {
        assert_eq!(library_key("org.ow2.asm:asm:9.8"), "org.ow2.asm:asm");
        assert_eq!(library_key("org.lwjgl:lwjgl:3.3.3:natives-windows"), "org.lwjgl:lwjgl:natives-windows");
    }
}
