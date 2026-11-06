//! Lazy inventory for the RichFinder folder map. Folder IDs refer only to rows
//! the app offered or to a prior choice. They are not open-file capabilities.
use richos_search::corpus::{directory_exclusion, source_id, validate_root, DirectoryExclusion};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PAGE_SIZE: usize = 256;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowState {
    Eligible,
    Locked,
    Unavailable,
}

#[derive(Serialize)]
pub struct FolderRow {
    id: String,
    name: String,
    path: String,
    state: RowState,
    reason: Option<&'static str>,
}

#[derive(Serialize)]
pub struct FolderPage {
    rows: Vec<FolderRow>,
    next_offset: Option<usize>,
}

#[derive(Default)]
pub struct FolderMap {
    known: Mutex<BTreeMap<String, PathBuf>>,
}

fn locked_reason(path: &Path, excluded: &[PathBuf]) -> Option<&'static str> {
    if excluded.iter().any(|prefix| path.starts_with(prefix)) {
        return Some("App data or search cache");
    }
    match directory_exclusion(path)? {
        DirectoryExclusion::System => Some("System folders"),
        DirectoryExclusion::AppBundle => Some("Applications"),
        DirectoryExclusion::Hidden => Some("Hidden folders"),
        DirectoryExclusion::DependencyOrGenerated => Some("Code dependencies or generated output"),
    }
}

fn eligible(path: &Path, excluded: &[PathBuf]) -> Result<(), String> {
    if path.to_str().is_none() || locked_reason(path, excluded).is_some() {
        return Err("This folder is excluded from search.".into());
    }
    let canonical = validate_root(path)?;
    if canonical != path {
        return Err("The selected path reaches a different folder through a link.".into());
    }
    Ok(())
}

impl FolderMap {
    pub fn path(&self, id: &str) -> Result<PathBuf, String> {
        self.known.lock().unwrap().get(id).cloned().ok_or_else(|| "That search folder is no longer in the folder map.".into())
    }

    pub fn allowed_path(&self, id: &str, excluded: &[PathBuf]) -> Result<PathBuf, String> {
        let path = self.path(id)?;
        eligible(&path, excluded)?;
        Ok(path)
    }

    fn row(&self, path: PathBuf, excluded: &[PathBuf], unavailable: bool) -> FolderRow {
        let display = path.to_string_lossy().into_owned();
        let id = source_id(&display);
        let name = path.file_name().map_or_else(|| display.clone(), |name| name.to_string_lossy().into_owned());
        let reason = locked_reason(&path, excluded);
        let state = if reason.is_some() { RowState::Locked }
            else if unavailable || path.to_str().is_none() { RowState::Unavailable }
            else { RowState::Eligible };
        self.known.lock().unwrap().insert(id.clone(), path);
        FolderRow { id, name, path: display, state, reason }
    }

    pub fn saved_rows(&self, selected: &[PathBuf], excluded: &[PathBuf]) -> Vec<FolderRow> {
        selected.iter().map(|path| self.row(path.clone(), excluded, eligible(path, excluded).is_err())).collect()
    }

    pub fn roots(&self, home: &Path, excluded: &[PathBuf]) -> FolderPage {
        let mut paths = vec![home.to_path_buf()];
        #[cfg(target_os = "macos")]
        {
            if let Ok(volumes) = std::fs::read_dir("/Volumes") {
                paths.extend(volumes.flatten().filter_map(|entry| {
                    if !entry.file_type().ok()?.is_dir() { return None; }
                    let path = entry.path();
                    let mounted = richos_core::machine_settings::validate_existing_storage(&path).ok()?;
                    (mounted == path).then_some(mounted)
                }));
            }
            paths.extend(["/System", "/Applications", "/Library"].into_iter().map(PathBuf::from));
        }
        FolderPage { rows: paths.into_iter().map(|path| self.row(path, excluded, false)).collect(), next_offset: None }
    }

    pub fn children(&self, parent: &str, offset: usize, excluded: &[PathBuf]) -> Result<FolderPage, String> {
        let path = self.allowed_path(parent, excluded)?;
        let entries = std::fs::read_dir(&path).map_err(|_| "RichOS could not read that folder.")?;
        let mut folders: Vec<_> = entries.flatten().filter_map(|entry| entry.file_type().ok()?.is_dir().then(|| entry.path())).collect();
        folders.sort_unstable();
        let end = offset.saturating_add(PAGE_SIZE);
        let next_offset = (end < folders.len()).then_some(end);
        let rows = folders.into_iter().skip(offset).take(PAGE_SIZE)
            .map(|path| self.row(path, excluded, false)).collect();
        Ok(FolderPage { rows, next_offset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_folder_ids_can_select_only_reachable_eligible_folders() {
        let base = std::env::temp_dir().join(richos_core::util::new_id("folder-map"));
        let home = base.join("home");
        let chosen = home.join("Documents");
        let locked = home.join("app-data");
        let outside = base.join("outside");
        for dir in [&chosen, &locked, &outside] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let home = home.canonicalize().unwrap();
        let chosen = chosen.canonicalize().unwrap();
        let excluded = vec![locked.canonicalize().unwrap()];
        let map = FolderMap::default();
        let home_id = source_id(&home.to_string_lossy());
        // A plausible ID for a path the app has never shown grants nothing.
        let guessed = source_id(&outside.canonicalize().unwrap().to_string_lossy());
        assert!(map.allowed_path(&guessed, &excluded).is_err());
        map.roots(&home, &excluded);
        let page = map.children(&home_id, 0, &excluded).unwrap();
        let documents = page.rows.iter().find(|row| row.name == "Documents").unwrap();
        let app_data = page.rows.iter().find(|row| row.name == "app-data").unwrap();
        assert!(matches!(app_data.state, RowState::Locked));
        assert!(map.allowed_path(&app_data.id, &excluded).is_err());
        assert_eq!(map.allowed_path(&documents.id, &excluded).unwrap(), chosen);
        std::fs::remove_dir(&chosen).unwrap();
        assert!(map.allowed_path(&documents.id, &excluded).is_err());
        assert_eq!(map.path(&documents.id).unwrap(), chosen, "an unplugged choice can still be removed");
        std::fs::remove_dir_all(base).unwrap();
    }
}
