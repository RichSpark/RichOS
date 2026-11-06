//! Native RichCleaner filing probe. No GUI, no provider, no personal files or installed settings.
// This file-only probe reuses the adapter; global-focus and bundle discovery run in VM walks.
#[path = "../src/cleaner_metadata.rs"]
mod cleaner_metadata;
#[allow(dead_code)]
#[path = "../src/cleaner_native.rs"]
mod cleaner_native;
use cleaner_native::Native;
use richos_core::cleaner::{self, Catalog, Entry, Files, Identity, Staged};
use serde_json::json;
use std::path::{Path, PathBuf};
fn main() {
    if let Err(error) = run() {
        eprintln!("RichCleaner probe failed: {error}");
        std::process::exit(1)
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let resources = PathBuf::from(args.next().ok_or("Give the resources directory")?);
    let fixtures = PathBuf::from(args.next().ok_or("Give a fixture directory")?);
    let report = PathBuf::from(args.next().ok_or("Give an empty report directory")?);
    if report.exists() {
        return Err("Preserve earlier evidence and give a fresh report directory".into());
    }
    std::fs::create_dir_all(&report).map_err(|e| e.to_string())?;
    let native = Native {
        directory: resources,
    };
    let mut results = vec![];
    for input in std::fs::read_dir(&fixtures).map_err(|e| e.to_string())? {
        let input = input.map_err(|e| e.to_string())?.path();
        if !input.is_file() {
            continue;
        }
        let name = input.file_name().ok_or("Missing fixture name")?;
        let case = report.join(name);
        std::fs::create_dir(&case).map_err(|e| e.to_string())?;
        let desktop = case.join("Desktop");
        let library = case.join("Library");
        std::fs::create_dir(&desktop).map_err(|e| e.to_string())?;
        std::fs::create_dir(&library).map_err(|e| e.to_string())?;
        let source = desktop.join(name);
        std::fs::copy(&input, &source).map_err(|e| e.to_string())?;
        let db_path = case.join("catalog.sqlite");
        let mut db = Catalog::open(&db_path)?;
        let e = Entry::new(
            source.clone(),
            native.inspect(&source)?,
            "O'Brien's kitchen quote. Compare the café renovation 📷".into(),
        )?;
        db.save(&e)?;
        let filed = cleaner::file(
            &mut db,
            &native,
            &e.id,
            &library,
            "Home/Kitchen",
            vec!["renovation".into()],
            false,
        )?;
        let ext = input
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if cleaner::EXTENSIONS.contains(&ext.as_str()) && filed.projection.mode != "embedded" {
            return Err(format!(
                "Ordinary {ext} fixture did not embed its note: {}",
                filed.projection.reason
            ));
        }
        if filed.projection.mode == "embedded"
            && !cleaner_metadata::search_text(&filed.path, Some(&native.directory))?.contains(&filed.note)
        {
            return Err(format!("The portable search reader did not recover the {ext} note"));
        }
        if source.exists() {
            return Err("The filed Desktop source still exists".into());
        }
        let projected = case.join(format!("annotated.{ext}"));
        std::fs::copy(&filed.path, &projected).map_err(|e| e.to_string())?;
        let edited = cleaner::edit(
            &mut db,
            &native,
            &filed.id,
            "Corrected kitchen quote. Compare the other estimate.".into(),
        )?;
        if edited.projection.mode != "embedded" && cleaner::EXTENSIONS.contains(&ext.as_str()) {
            return Err(format!(
                "The {ext} note edit did not embed: {:?}",
                edited.problem
            ));
        }
        drop(db);
        let mut db = Catalog::open(&db_path)?;
        std::fs::write(&source, b"another original occupies this name")
            .map_err(|e| e.to_string())?;
        let undone = cleaner::undo(&mut db, &native, &e.id)?;
        if std::fs::read(&source).map_err(|e| e.to_string())?
            != b"another original occupies this name"
        {
            return Err("Undo replaced an occupant".into());
        }
        if undone.path == source
            || undone.note != edited.note
            || !native.inspect(&undone.path)?.same_object(&undone.identity)
        {
            return Err("Undo did not preserve its original and note".into());
        }
        results.push(json!({"fixture":name.to_string_lossy(),"mode":filed.projection.mode,"annotated":projected,"restored":undone.path,"note":e.note,"editedNote":undone.note,"checks":["saved-before-mutation","embedded","single-moved-original","persistent-search-record","edit","restart","undo-collision"]}));
        std::fs::write(
            report.join("partial-results.json"),
            serde_json::to_vec_pretty(&results).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    // One real mutation at each gap between the filesystem and the catalog.
    for (label, step, inbox) in [
        ("after-replace", "replace", false),
        ("after-annotation", "annotate", false),
        ("after-rename", "rename", false),
        ("after-inbox-rename", "rename", true),
    ] {
        let input = std::fs::read_dir(&fixtures)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.is_file())
            .ok_or("Missing fault fixture")?;
        let area = report.join(label);
        std::fs::create_dir(&area).map_err(|e| e.to_string())?;
        let source = area.join(input.file_name().ok_or("Missing fault name")?);
        std::fs::copy(&input, &source).map_err(|e| e.to_string())?;
        let library = area.join("Library");
        std::fs::create_dir(&library).map_err(|e| e.to_string())?;
        let db_path = area.join("catalog.sqlite");
        let mut db = Catalog::open(&db_path)?;
        let entry = Entry::new(
            source.clone(),
            native.inspect(&source)?,
            "A note survives the interrupted move".into(),
        )?;
        db.save(&entry)?;
        let fault = Fault {
            native: &native,
            step,
        };
        let result = if inbox {
            cleaner::store_inbox(&mut db, &fault, &entry.id, &library)
        } else {
            cleaner::file(
                &mut db,
                &fault,
                &entry.id,
                &library,
                "Home/Kitchen",
                vec![],
                false,
            )
        };
        // Unsupported types have no replacement, so the replace fault is not reached.
        if matches!(step,"replace"|"annotate") && result.is_ok() {
            continue;
        }
        if result.is_ok() {
            return Err("The injected interruption did not run".into());
        }
        let pending_stage = db.get(&entry.id)?.staged;
        drop(db);
        let mut db = Catalog::open(&db_path)?;
        cleaner::recover(&mut db, &native)?;
        let recovered = db.get(&entry.id)?;
        if recovered.note != entry.note
            || !matches!(
                recovered.phase,
                cleaner::Phase::Draft | cleaner::Phase::Filed
            )
            || !native
                .inspect(&recovered.path)?
                .same_content(&recovered.identity)
            || recovered.staged.is_some()
        {
            return Err(format!(
                "{label} failed to settle its original and note: {:?}",
                recovered.phase
            ));
        }
        if step == "rename" && source.exists() {
            return Err("Recovery left a second source original".into());
        }
        if let Some(stage) = pending_stage {
            if stage.path.parent().is_some_and(Path::exists) {
                return Err("An interrupted writer left a staging directory behind".into());
            }
            native.cleanup(&stage)?;
        }
        results.push(json!({"fault":label,"checks":["real-native-mutation","restart","original-located","note-retained","staging-cleaned"],"path":recovered.path}));
    }
    if let Some(input) = std::fs::read_dir(&fixtures).map_err(|e| e.to_string())?
        .filter_map(std::result::Result::ok).map(|e| e.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "png"))
    {
        for step in ["annotate", "replace"] {
            let case = report.join(format!("edit-after-{step}"));
            std::fs::create_dir(&case).map_err(|e| e.to_string())?;
            std::fs::create_dir(case.join("Library")).map_err(|e| e.to_string())?;
            let source = case.join("original.png");
            std::fs::copy(&input, &source).map_err(|e| e.to_string())?;
            let path = case.join("catalog.sqlite");
            let mut db = Catalog::open(&path)?;
            let entry = Entry::new(source, native.inspect(&case.join("original.png"))?, "Initial note".into())?;
            db.save(&entry)?;
            let filed = cleaner::file(&mut db, &native, &entry.id, &case.join("Library"), "Home/Kitchen", vec![], false)?;
            let saved = cleaner::edit(&mut db, &Fault { native: &native, step }, &entry.id, "Corrected note survives the native error".into())?;
            if saved.path != filed.path || saved.phase != cleaner::Phase::Filed
                || !native.inspect(&saved.path)?.same_content(&saved.identity)
                || saved.staged.is_some()
                || saved.projection.mode != if step == "replace" { "embedded" } else { "older" }
            {
                return Err(format!("Edit {step} error lost the current original identity or note state"));
            }
            drop(db);
            let db = Catalog::open(&path)?;
            if db.get(&entry.id)?.note != "Corrected note survives the native error"
                || db.search("Corrected", 0, 65536)?.total != 1
            {
                return Err("The corrected note was lost after reopening".into());
            }
            let mut db = Catalog::open(&path)?;
            let retried = cleaner::edit(&mut db, &native, &entry.id, "A later correction is current".into())?;
            if retried.projection.mode != "embedded" || retried.problem.is_some() || retried.path != filed.path {
                return Err("A successful later edit retained an obsolete error or moved the file".into());
            }
            results.push(json!({"fault":format!("edit-after-{step}"),"checks":["real-native-mutation","error-reconciled-before-cleanup","correct-original-identity","corrected-note-retained","restart","searchable","folder-unchanged"],"path":saved.path}));
        }
    }
    if results.is_empty() {
        return Err("No fixtures were exercised".into());
    }
    std::fs::write(
        report.join("results.json"),
        serde_json::to_vec_pretty(&results).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{}",
        json!({"cases":results.len(),"report":report.join("results.json")})
    );
    Ok(())
}

// The error stands in for process loss after the actual native mutation.
struct Fault<'a> {
    native: &'a Native,
    step: &'static str,
}
impl Files for Fault<'_> {
    fn inspect(&self, p: &Path) -> Result<Identity, String> {
        self.native.inspect(p)
    }
    fn resolve(&self, p: &Path, i: &Identity) -> Result<PathBuf, String> {
        self.native.resolve(p, i)
    }
    fn destination(&self, r: &Path, p: &[String], n: &std::ffi::OsStr) -> Result<PathBuf, String> {
        self.native.destination(r, p, n)
    }
    fn stage(&self, e: &Entry) -> Result<Option<Staged>, String> {
        self.native.stage(e)
    }
    fn annotate(&self, e: &Entry, s: &Staged) -> Result<Staged, String> {
        let staged = self.native.annotate(e, s)?;
        if self.step == "annotate" {
            std::fs::write(
                staged.path.with_extension("writer_tmp"),
                b"interrupted writer output",
            )
            .map_err(|e| e.to_string())?;
            return Err("Injected interruption after metadata annotation".into());
        }
        Ok(staged)
    }
    fn replace(&self, e: &Entry, s: &Staged) -> Result<Identity, String> {
        let actual = self.native.replace(e, s)?;
        if self.step == "replace" {
            Err("Injected interruption after native replacement".into())
        } else {
            Ok(actual)
        }
    }
    fn rename(&self, a: &Path, b: &Path, i: &Identity) -> Result<Identity, String> {
        let actual = self.native.rename(a, b, i)?;
        if self.step == "rename" {
            Err("Injected interruption after native rename".into())
        } else {
            Ok(actual)
        }
    }
    fn cleanup(&self, s: &Staged) -> Result<(), String> {
        self.native.cleanup(s)
    }
}
