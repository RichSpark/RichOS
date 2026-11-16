//! Optional native animations are authenticated before they become selectable.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;

const BASE: &str = "https://github.com/RichSpark/RichOS/releases/download/splash-packs-v2/";
const FOLDER: &str = "splash-packs-v2";
const BUDGET: u64 = 150_000_000;
const STARTERS: [&str; 2] = ["ruled-standard", "midnight-suede"];
static DOWNLOAD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Deserialize)]
struct Asset {
    bytes: u64,
    sha256: String,
    bar: [f64; 4],
}

fn catalog() -> &'static HashMap<String, Asset> {
    static ASSETS: OnceLock<HashMap<String, Asset>> = OnceLock::new();
    ASSETS.get_or_init(|| serde_json::from_str(include_str!("../../native/splash/packs.json"))
        .expect("the splash asset manifest is validated at build time"))
}

fn bundled(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().resource_dir().ok()?.join("Splash"))
}

fn filename(id: &str) -> String {
    format!("{id}.mp4")
}

fn edition(app: &AppHandle) -> String {
    format!("edition-update-{}", app.package_info().version)
}

pub fn local(app: &AppHandle, data: &Path, id: &str) -> Option<PathBuf> {
    let asset_id = if id == "edition-update" { edition(app) } else { id.to_string() };
    let file = filename(&asset_id);
    if STARTERS.contains(&id) || id == "edition-update" {
        let path = bundled(app)?.join(&file);
        return path.is_file().then_some(path);
    }
    let asset = catalog().get(id)?;
    let path = data.join(FOLDER).join(file);
    (fs::metadata(&path).ok()?.len() == asset.bytes).then_some(path)
}

pub fn bar(id: &str, path: &Path) -> Option<[f64; 4]> {
    if STARTERS.contains(&id) || id == "edition-update" {
        serde_json::from_slice(&fs::read(path.with_extension("bar.json")).ok()?).ok()
    } else {
        catalog().get(id).map(|asset| asset.bar)
    }
}

fn bundle_bytes(app: &AppHandle) -> u64 {
    bundled(app)
        .into_iter()
        .flat_map(|path| fs::read_dir(path).into_iter().flatten())
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .map(|file| file.len())
        .sum()
}

fn make_room(dir: &Path, bundled: u64, keep: &[&str], required: u64) -> std::io::Result<()> {
    let files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let pack = path.file_stem()?.to_str()?.to_string();
            let file = entry.metadata().ok()?;
            if path.extension()? != "mp4" || !file.is_file() { return None; }
            let touched = file.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((touched, file.len(), pack, path))
        });
    let mut files: Vec<_> = files.collect();
    files.sort_unstable();
    let mut total = bundled + required + files.iter().map(|row| row.1).sum::<u64>();
    for (_, bytes, id, file) in files {
        if total <= BUDGET { break; }
        if !keep.contains(&id.as_str()) {
            fs::remove_file(file)?;
            total -= bytes;
        }
    }
    if total > BUDGET {
        return Err(std::io::Error::other("the splash storage budget is full"));
    }
    Ok(())
}

pub async fn prepare(app: &AppHandle, data: &Path, packs: &[String], keep: Option<&str>) -> Result<(), String> {
    if packs.iter().all(|id| local(app, data, id).is_some()) { return Ok(()); }
    let _lock = DOWNLOAD.lock().await;
    let mut protected: Vec<&str> = keep.into_iter().collect();
    for id in packs {
        download(app, data, id, &protected).await?;
        protected.push(id);
    }
    Ok(())
}

async fn download(app: &AppHandle, data: &Path, id: &str, keep: &[&str]) -> Result<(), String> {
    if local(app, data, id).is_some() { return Ok(()); }
    let asset = catalog().get(id).ok_or("This opening is unavailable in this build.")?;
    let cache = data.join(FOLDER);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let temporary = tempfile::NamedTempFile::new_in(&cache).map_err(|e| e.to_string())?;
    let mut file = tokio::fs::File::from_std(temporary.reopen().map_err(|e| e.to_string())?);
    let name = filename(id);
    crate::voice_provision::ensure_crypto_provider();
    let mut response = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?
        .get(format!("{BASE}{name}"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?;
    let mut bytes = 0;
    let mut hash = Sha256::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        bytes += chunk.len() as u64;
        if bytes > asset.bytes { return Err("The opening download was larger than expected.".into()); }
        hash.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    if bytes != asset.bytes || format!("{:x}", hash.finalize()) != asset.sha256 {
        return Err("The opening download could not be verified.".into());
    }
    file.sync_all().await.map_err(|e| e.to_string())?;
    drop(file);
    let destination = cache.join(name);
    if destination.exists() {
        fs::remove_file(&destination).map_err(|e| e.to_string())?;
    }
    make_room(&cache, bundle_bytes(app), keep, bytes)
        .map_err(|e| e.to_string())?;
    temporary.persist(destination).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_byte_budget_includes_bundled_assets_and_keeps_the_active_selection() {
        let cache = tempfile::tempdir().unwrap();
        for (id, len) in [("active", 30_000_000), ("older", 30_000_000), ("recent", 30_000_000)] {
            fs::File::create(cache.path().join(filename(id))).unwrap().set_len(len).unwrap();
        }
        make_room(cache.path(), 30_000_000, &["active"], 40_000_000).unwrap();
        assert!(cache.path().join(filename("active")).is_file());
        let stored = fs::read_dir(cache.path()).unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len()).sum::<u64>();
        assert_eq!(stored, 60_000_000);
    }
}
