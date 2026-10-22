//! **A new RichOS version's improved Rich and Stu instructions reach every existing user**
//! (CEO, 2026-10-22: "If the user never changed their `rich.md`/`stu.md`, the app updates it by
//! itself. If they did, the app asks once").
//!
//! The user's `rich.md` and `stu.md` (`user_settings.rs`) are copies of the shipped files. The
//! app keeps a [`RECORD`] beside them of the shipped text each copy was made from, and when the
//! shipped text moves on ([`check`], at every lease's start, so at the first start of a new
//! version):
//!
//! - **a copy the user never changed** is replaced by the new shipped file, silently;
//! - **a changed copy** is asked about, once per new version, in the CEO's words (the shell's
//!   sheet, `ui/instructions-update.js`). **OK** ([`accept`]): Rich updates it, not a line merge
//!   (CEO, 2026-10-22: a mechanical merge "could easily defeat the whole update"). A Claude Code
//!   session ([`rewrite_with_rich`]) is given the three texts and the engine's
//!   `update-instructions` skill, which says how (CEO §124: never in code): everything from the
//!   update is kept, and a change of the user's that conflicts with it is edited out (CEO,
//!   2026-10-22). No copy of the previous file is kept: a user who wants their wording back edits
//!   the file again (CEO, 2026-10-22). What is returned ([`Updated`]) carries the before and
//!   after text of each file and the edits between them, for the before-and-after screen the
//!   CEO asked for, with the session's own one or two lines.
//!   **No** ([`decline`]): nothing changes, and the question is not asked again until a later
//!   version changes the instructions again.
//!
//! A copy made before the record existed has none. One that is exactly a shipped version (the
//! current one or one of [`EARLIER`]) was not changed, and that version is its starting point;
//! any other was changed, and its starting point is the shipped version nearest to it.
//!
//! What the user's file is used for from then on is unchanged: every session of Rich and Stu
//! starts from it, so the result applies from the next session (`user_settings::digest`).
use crate::runtime::RuntimeError;
use crate::user_settings::{RICH, STU};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The record of the shipped text each of the user's copies was made from, in the user's folder.
pub const RECORD: &str = ".richos/instructions-record.json";
/// Every version of `rich.md` and `stu.md` shipped before the record existed, under the shipped
/// plugin root: `earlier/rich/<n>.txt` and `earlier/stu/<n>.txt`, oldest first, byte for byte
/// as shipped (`.txt`, because the commit's Oxford comma fixer rewrites every `.md` it is
/// given, and a copy is recognized only by matching exactly). A copy made by a build without
/// the record can only be one of these, so the set never grows.
pub const EARLIER: &str = "earlier";
/// The skill that says how a changed copy is updated, in the engine.
pub const SKILL: &str = "app/skills/update-instructions/SKILL.md";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Entry {
    /// The shipped text the user's copy was made from.
    from: String,
    /// The SHA-256 of a shipped text the user said No to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    declined: Option<String>,
}

type Record = BTreeMap<String, Entry>;

fn sha(text: &str) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(text.as_bytes()))
}

/// The record as written; a missing one is empty, and so is one that cannot be read as a
/// record (said, then read as no record: every copy then gets its starting point again).
fn read_record(folder: &Path) -> Record {
    let path = folder.join(RECORD);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("[richos] {} is not a record of the instructions ({e}); it is written again", path.display());
            Record::new()
        }),
        Err(_) => Record::new(),
    }
}

/// Write `body` to `path` through a file beside it, so a crash leaves the old file or the new.
fn write_atomic(path: &Path, body: &str) -> Result<(), RuntimeError> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| RuntimeError(format!("{} has no folder", path.display())))?;
    std::fs::create_dir_all(parent).map_err(|e| RuntimeError(format!("{} could not be created: {e}", parent.display())))?;
    let temporary = parent.join(format!(".{}.{}", path.file_name().unwrap_or_default().to_string_lossy(), uuid::Uuid::new_v4()));
    let written = std::fs::File::create(&temporary)
        .and_then(|mut file| file.write_all(body.as_bytes()).and_then(|_| file.sync_all()))
        .and_then(|_| std::fs::rename(&temporary, path));
    if let Err(e) = written {
        if let Err(left) = std::fs::remove_file(&temporary) {
            eprintln!("[richos] {} could not be removed: {left}", temporary.display());
        }
        return Err(RuntimeError(format!("{} could not be written: {e}", path.display())));
    }
    Ok(())
}

fn write_record(folder: &Path, record: &Record) -> Result<(), RuntimeError> {
    let text = serde_json::to_string_pretty(record).map_err(|e| RuntimeError(e.to_string()))?;
    write_atomic(&folder.join(RECORD), &(text + "\n"))
}

fn stem(name: &str) -> &str { name.strip_suffix(".md").unwrap_or(name) }


fn shipped_text(shipped: &Path, name: &str) -> Result<String, RuntimeError> {
    let path = shipped.join("agents").join(name);
    std::fs::read_to_string(&path).map_err(|e| RuntimeError(format!("the shipped {name} could not be read: {e}")))
}

/// [`EARLIER`]'s versions of `name`, oldest first.
fn earlier(shipped: &Path, name: &str) -> Vec<String> {
    let folder = shipped.join(EARLIER).join(stem(name));
    let mut numbered: Vec<(u32, PathBuf)> = std::fs::read_dir(&folder).into_iter().flatten().flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let n = path.file_stem()?.to_str()?.parse().ok()?;
            (path.extension()? == "txt").then_some((n, path))
        }).collect();
    numbered.sort();
    numbered.into_iter().filter_map(|(_, path)| std::fs::read_to_string(path).ok()).collect()
}

/// The user's own copy, or `None` when there is none to update: no file, or a link (a link is
/// the user's choice of file, never written through).
fn own_copy(folder: &Path, name: &str) -> Option<String> {
    let path = folder.join(name);
    if std::fs::symlink_metadata(&path).ok()?.file_type().is_symlink() { return None; }
    std::fs::read_to_string(path).ok()
}

/// One line of the difference between a before and an after text, for showing it: kept as it
/// was, removed from the before text, or added in the after text. Nothing is ever merged from
/// these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "line")]
pub enum Edit {
    Same(String),
    Removed(String),
    Added(String),
}

/// The lines of `after` against `before`, in order: the longest run of lines the two share is
/// kept, every other line of `before` is removed and every other line of `after` added.
pub fn edits(before: &str, after: &str) -> Vec<Edit> {
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    // shared[i][j]: the longest run of lines a[i..] and b[j..] share.
    let mut shared = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            shared[i][j] = if a[i] == b[j] { shared[i + 1][j + 1] + 1 } else { shared[i + 1][j].max(shared[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push(Edit::Same(a[i].to_string())); i += 1; j += 1;
        } else if i < a.len() && (j == b.len() || shared[i + 1][j] >= shared[i][j + 1]) {
            out.push(Edit::Removed(a[i].to_string())); i += 1;
        } else {
            out.push(Edit::Added(b[j].to_string())); j += 1;
        }
    }
    out
}

/// Lines that differ between two texts: added plus removed.
fn distance(a: &str, b: &str) -> usize {
    edits(a, b).iter().filter(|edit| !matches!(edit, Edit::Same(_))).count()
}

/// The shipped text a copy with no record was made from: the version it is exactly, or else the
/// one nearest to it (the later one of two as near).
fn starting_point(yours: &str, new: &str, earlier: Vec<String>) -> String {
    let mut versions = earlier;
    versions.push(new.to_string());
    if let Some(exact) = versions.iter().find(|version| version.as_str() == yours) { return exact.clone(); }
    versions.into_iter().rev().min_by_key(|version| distance(version, yours)).unwrap_or_else(|| new.to_string())
}

/// **Record a copy the app just made from the shipped text** (`user_settings::provision`).
pub fn record_copy(folder: &Path, name: &str, from: &str) -> Result<(), RuntimeError> {
    let mut record = read_record(folder);
    record.insert(name.to_string(), Entry { from: from.to_string(), declined: None });
    write_record(folder, &record)
}

/// **The check at a lease's start.** Copies the user never changed are brought to the shipped
/// version, silently; returns the names of the changed copies the user is to be asked about
/// (none when the shipped files have not moved on, or when the user said No to this version).
pub fn check(folder: &Path, shipped: &Path) -> Result<Vec<&'static str>, RuntimeError> {
    let mut record = read_record(folder);
    let before = record.clone();
    let mut ask = Vec::new();
    for name in [RICH, STU] {
        let Some(yours) = own_copy(folder, name) else { continue };
        let new = shipped_text(shipped, name)?;
        let entry = record.entry(name.to_string())
            .or_insert_with(|| Entry { from: starting_point(&yours, &new, earlier(shipped, name)), declined: None });
        if entry.from == new { continue; }
        if yours == new {
            *entry = Entry { from: new, declined: None };
        } else if yours == entry.from {
            write_atomic(&folder.join(name), &new)?;
            *entry = Entry { from: new, declined: None };
        } else if entry.declined.as_deref() != Some(sha(&new).as_str()) {
            ask.push(name);
        }
    }
    if record != before { write_record(folder, &record)?; }
    Ok(ask)
}

/// **No**: nothing changes, and the question is not asked again for these shipped versions.
pub fn decline(folder: &Path, shipped: &Path) -> Result<(), RuntimeError> {
    let ask = check(folder, shipped)?;
    if ask.is_empty() { return Ok(()); }
    let mut record = read_record(folder);
    for name in ask {
        if let Some(entry) = record.get_mut(name) { entry.declined = Some(sha(&shipped_text(shipped, name)?)); }
    }
    write_record(folder, &record)
}

/// One changed copy to update: the three texts the session reads.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Update {
    pub name: &'static str,
    /// The shipped version the user's copy was made from.
    pub started_from: String,
    /// The user's copy as it is now.
    pub yours: String,
    /// The new shipped version.
    pub new: String,
}

/// One file as OK left it: what the before-and-after screen shows (CEO, 2026-10-22: "the user is
/// presented with "before" and "after" with all the edits highlighted").
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdatedFile {
    pub name: &'static str,
    pub before: String,
    pub after: String,
    /// Every line, kept, removed or added, from `before` to `after`.
    pub edits: Vec<Edit>,
}

/// What OK did: each file, and the session's own one or two lines about it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Updated {
    pub said: String,
    pub files: Vec<UpdatedFile>,
}

/// **OK**: `rewrite` (in the app, [`rewrite_with_rich`]) is given every changed copy and answers
/// with each file's new text, in the same order, and its own lines about it. Each new file is
/// written and recorded, and each file's before, after and edits are returned. Nothing is written when `rewrite` fails, or when a copy changed
/// while it ran.
pub fn accept(folder: &Path, shipped: &Path,
              rewrite: impl FnOnce(&[Update]) -> Result<(Vec<String>, String), String>) -> Result<Updated, String> {
    let ask = check(folder, shipped).map_err(|e| e.0)?;
    let record = read_record(folder);
    let mut updates = Vec::new();
    for name in ask {
        let (Some(yours), Some(entry)) = (own_copy(folder, name), record.get(name)) else { continue };
        updates.push(Update { name, started_from: entry.from.clone(), yours, new: shipped_text(shipped, name).map_err(|e| e.0)? });
    }
    if updates.is_empty() { return Ok(Updated::default()); }
    let (texts, said) = rewrite(&updates)?;
    if texts.len() != updates.len() || texts.iter().any(|text| text.trim().is_empty()) {
        return Err("the update did not write every file; nothing was changed".into());
    }
    for update in &updates {
        if own_copy(folder, update.name).as_deref() != Some(update.yours.as_str()) {
            return Err(format!("{} was changed while it was being updated; nothing was changed", update.name));
        }
    }
    let mut record = read_record(folder);
    let mut files = Vec::new();
    for (update, text) in updates.iter().zip(texts) {
        write_atomic(&folder.join(update.name), &text).map_err(|e| e.0)?;
        record.insert(update.name.to_string(), Entry { from: update.new.clone(), declined: None });
        files.push(UpdatedFile { name: update.name, edits: edits(&update.yours, &text), before: update.yours.clone(), after: text });
    }
    write_record(folder, &record).map_err(|e| e.0)?;
    Ok(Updated { said, files })
}

/// How long the session may take. A careful rewrite of two files on the user's own model is a
/// minute or two; this only stops a stalled session being waited on for ever.
const WAIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// **The session that updates the user's changed copies**: Claude Code on the model Rich runs on,
/// with the engine's [`SKILL`] as its whole instruction and only the Write tool, given each file's
/// three texts and where to write the result. Returns the files it wrote and its answer.
pub fn rewrite_with_rich(bin: &Path, profile: &crate::engine_profile::EngineProfile, updates: &[Update])
    -> Result<(Vec<String>, String), String> {
    let work = profile.state.join("instructions-update").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = (|| {
        let files: Vec<_> = updates.iter().map(|update| serde_json::json!({
            "name": update.name, "started_from": update.started_from, "yours": update.yours, "new": update.new,
            "write_to": work.join(update.name)})).collect();
        let body = serde_json::json!({"type": "user", "message": {"role": "user",
            "content": serde_json::json!({"files": files}).to_string()}}).to_string();
        let mut live = crate::request_classifier::Provider::new(bin, profile, SKILL, "instructions-update")
            .writing(profile.lease_models.rich.cli_alias(), "Write").start()?;
        let said = live.exchange(body, std::time::Instant::now() + WAIT, || false, |frame| {
            let said = frame["result"].as_str().unwrap_or_default().trim();
            if said.is_empty() { Err("the update said nothing".to_string()) } else { Ok(said.to_string()) }
        })?;
        drop(live);
        let texts = updates.iter().map(|update| std::fs::read_to_string(work.join(update.name))
            .map_err(|_| format!("the update did not write {}", update.name))).collect::<Result<Vec<_>, _>>()?;
        Ok((texts, said))
    })();
    if let Err(e) = std::fs::remove_dir_all(&work) {
        eprintln!("[richos] the update's work folder {} could not be removed: {e}", work.display());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_distance_counts_added_and_removed_lines() {
        assert_eq!(distance("a\nb\nc\n", "a\nb\nc\n"), 0);
        assert_eq!(distance("a\nb\nc\n", "a\nb\nc\nmine\n"), 1);
        assert_eq!(distance("a\nb\nc\n", "a\nB\nc\n"), 2);
        assert_eq!(distance("", "a\n"), 1);
    }

    #[test]
    fn the_edits_show_every_line_kept_removed_or_added_in_order() {
        assert_eq!(edits("a\nb\nc\n", "a\nB\nc\nd\n"), [
            Edit::Same("a".into()), Edit::Removed("b".into()), Edit::Added("B".into()), Edit::Same("c".into()), Edit::Added("d".into())]);
        assert_eq!(serde_json::to_value(Edit::Removed("x".into())).unwrap(), serde_json::json!({"kind": "removed", "line": "x"}));
    }

    #[test]
    fn a_copy_without_a_record_starts_from_the_version_it_is_or_the_nearest_one() {
        let earlier = vec!["one\n".to_string(), "one\ntwo\n".to_string()];
        assert_eq!(starting_point("one\n", "one\ntwo\nthree\n", earlier.clone()), "one\n");
        assert_eq!(starting_point("one\ntwo\nthree\n", "one\ntwo\nthree\n", earlier.clone()), "one\ntwo\nthree\n");
        assert_eq!(starting_point("one\ntwo\nmine\n", "one\ntwo\nthree\nfour\n", earlier), "one\ntwo\n");
    }

    /// **OK is done by Rich, from the skill** (CEO, 2026-10-22): the session is started with the
    /// engine's `update-instructions` skill as its instruction, on Rich's model, with only the
    /// Write tool, and is given the three texts; what it writes becomes the user's file, and the
    /// before, after and edits are returned for the before-and-after screen.
    #[test]
    fn ok_starts_a_session_with_the_three_texts_and_the_skill_and_returns_before_and_after() {
        let (root, bin, profile, _) = crate::request_classifier::tests::fixture(r#"
open('argv.json','w').write(json.dumps(sys.argv))
print(json.dumps({'type':'system','subtype':'init','tools':['Write']}),flush=True)
for line in sys.stdin:
    data=json.loads(json.loads(line)['message']['content'])
    open('asked.json','w').write(json.dumps(data))
    for f in data['files']:
        open(f['write_to'],'w').write(f['new']+f['yours'].splitlines()[-1]+'\n')
    said='Your instructions now have the new section, and your own line is kept.'
    print(json.dumps({'type':'assistant','message':{'model':'claude-opus-5-5','content':[{'type':'tool_use','name':'Write','input':{}}]}}),flush=True)
    print(json.dumps({'type':'result','is_error':False,'result':said}),flush=True)
"#);
        let folder = root.join("central");
        let shipped = root.join("shipped");
        std::fs::create_dir_all(shipped.join("agents")).unwrap();
        let older = "---\nname: rich\n---\n\nYou are Rich.\n";
        let newer = "---\nname: rich\n---\n\nYou are Rich.\n\nA new section.\n";
        std::fs::write(shipped.join("agents").join(RICH), older).unwrap();
        std::fs::write(shipped.join("agents").join(STU), "Stu.\n").unwrap();
        crate::user_settings::provision(&folder, &shipped).unwrap();
        let yours = format!("{older}My own line.\n");
        std::fs::write(folder.join(RICH), &yours).unwrap();
        std::fs::write(shipped.join("agents").join(RICH), newer).unwrap();
        assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH]);

        let updated = accept(&folder, &shipped, |updates| rewrite_with_rich(&bin, &profile, updates)).unwrap();
        assert_eq!(updated.said, "Your instructions now have the new section, and your own line is kept.");
        // What the before-and-after screen shows: the file before, after, and every edit between.
        let file = &updated.files[0];
        assert_eq!((file.name, file.before.as_str(), file.after.as_str()), (RICH, yours.as_str(), format!("{newer}My own line.\n").as_str()));
        let changed: Vec<&Edit> = file.edits.iter().filter(|e| !matches!(e, Edit::Same(_))).collect();
        assert_eq!(changed, [&Edit::Added("".into()), &Edit::Added("A new section.".into())], "the edits are the lines the update added");
        assert_eq!(file.edits.last(), Some(&Edit::Same("My own line.".into())));
        let session = profile.state.join("instructions-update");
        let argv: Vec<String> = serde_json::from_str(&std::fs::read_to_string(session.join("argv.json")).unwrap()).unwrap();
        let after = |flag: &str| argv[argv.iter().position(|a| a == flag).unwrap() + 1].clone();
        let skill = std::fs::read_to_string(profile.engine.join(SKILL)).unwrap();
        assert!(skill.contains("is edited out"), "the skill is not the one that says how");
        assert_eq!(after("--system-prompt"), skill, "how to update comes from the engine's skill");
        assert_eq!(after("--tools"), "Write");
        assert_eq!(after("--model"), profile.lease_models.rich.cli_alias(), "the update runs on Rich's model");
        let asked: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(session.join("asked.json")).unwrap()).unwrap();
        let file = &asked["files"][0];
        assert_eq!((file["name"].as_str(), file["started_from"].as_str(), file["yours"].as_str(), file["new"].as_str()),
            (Some(RICH), Some(older), Some(yours.as_str()), Some(newer)), "the session must read the three texts");
        assert_eq!(asked["files"].as_array().unwrap().len(), 1, "an unchanged stu.md is not the session's");

        assert_eq!(std::fs::read_to_string(folder.join(RICH)).unwrap(), format!("{newer}My own line.\n"));
        let mut kept: Vec<String> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        kept.sort();
        assert_eq!(kept, [".richos", "instructions.md", "rich.md", "stu.md"], "OK left a file beside the user's");
        assert!(check(&folder, &shipped).unwrap().is_empty(), "the question came back after OK");
        assert!(std::fs::read_dir(&session).unwrap().flatten().all(|entry| entry.path().is_file()), "the session's work folder was left behind");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A session that fails writes nothing, and the question stays to be answered.
    #[test]
    fn a_session_that_fails_changes_nothing() {
        let (root, bin, profile, _) = crate::request_classifier::tests::fixture(r#"
print(json.dumps({'type':'system','subtype':'init','tools':['Write']}),flush=True)
for line in sys.stdin:
    print(json.dumps({'type':'result','is_error':True,'result':'Not signed in'}),flush=True)
"#);
        let folder = root.join("central");
        let shipped = root.join("shipped");
        std::fs::create_dir_all(shipped.join("agents")).unwrap();
        std::fs::write(shipped.join("agents").join(RICH), "Old.\n").unwrap();
        std::fs::write(shipped.join("agents").join(STU), "Stu.\n").unwrap();
        crate::user_settings::provision(&folder, &shipped).unwrap();
        std::fs::write(folder.join(RICH), "Old.\nMine.\n").unwrap();
        std::fs::write(shipped.join("agents").join(RICH), "New.\n").unwrap();
        assert!(accept(&folder, &shipped, |updates| rewrite_with_rich(&bin, &profile, updates)).is_err());
        assert_eq!(std::fs::read_to_string(folder.join(RICH)).unwrap(), "Old.\nMine.\n");
        assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH], "a failed update must ask again");
        std::fs::remove_dir_all(root).unwrap();
    }
}
