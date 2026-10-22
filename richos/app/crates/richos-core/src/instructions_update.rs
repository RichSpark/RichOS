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
//!   the file again (CEO, 2026-10-22). Each file is updated on its own, and what is returned
//!   ([`UpdatedFile`]) is the file before and after, paragraph by paragraph, with every edit
//!   marked ([`rows`]), for the before-and-after screen the CEO asked for (round 37).
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

/// Lines that differ between two texts: added plus removed.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    steps(&a, &b).iter().filter(|(i, j)| i.is_none() || j.is_none()).count()
}

/// The shipped text a copy with no record was made from: the version it is exactly, or else the
/// one nearest to it (the later one of two as near). The current version is taken only when it
/// alone is the nearest: a copy as near to an earlier version as to the current one may be that
/// earlier version with the user's edits, so it is not guessed to be up to date, and the user
/// is asked (second review rv-20261022T102758Z-81c4f463-539d, finding 1).
fn starting_point(yours: &str, new: &str, earlier: Vec<String>) -> String {
    if yours == new { return new.to_string(); }
    if let Some(exact) = earlier.iter().find(|version| version.as_str() == yours) { return exact.clone(); }
    let to_new = distance(new, yours);
    // The current version is often among the earlier ones too (the set is frozen as of the
    // version that introduced the record); it is the current one, not an earlier one.
    let nearest_earlier = earlier.into_iter().rev().filter(|version| version != new)
        .map(|version| (distance(&version, yours), version)).min_by_key(|(d, _)| *d);
    match nearest_earlier {
        Some((d, version)) if d <= to_new => version,
        _ => new.to_string(),
    }
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
        // A copy with no record is placed against the shipped history, and an engine without
        // that history (the one an upgrade starts on, before setup puts the new one in place) can
        // only guess: nothing is recorded or asked until the engine that carries it is installed
        // (second review rv-20261022T143030Z-c953badd-7eab, finding 1: the guess was saved and
        // an untouched copy was then asked about instead of updated).
        if !record.contains_key(name) && !shipped.join(EARLIER).is_dir() { continue; }
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

/// A changed copy to update: the three texts the session reads.
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

/// One part of a paragraph on the before-and-after screen (round 37), as `[kind, text]`: `fm` the
/// file's front matter, `s` the same before and after, `a` added from the RichOS update, `d`
/// older RichOS wording the update took out, `y` one of the user's own instructions, edited out
/// because it conflicts with the update.
pub type Segment = (&'static str, String);

/// One file as OK left it: what the before-and-after screen shows (CEO, 2026-10-22: "the user is
/// presented with "before" and "after" with all the edits highlighted").
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UpdatedFile {
    pub name: &'static str,
    /// Where the file is, the home folder written `~`.
    pub path: String,
    /// The file paragraph by paragraph, before and after side by side ([`rows`]).
    pub rows: Vec<Vec<Segment>>,
    /// How many paragraphs have an edit.
    pub edits: usize,
    /// The session's own line about what changed.
    pub said: String,
}

/// A changed copy the user is asked about: its name and where it is, the home folder written `~`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Asked {
    pub name: &'static str,
    pub path: String,
}

/// `path` with the home folder written `~`, the way the screen names a file.
pub fn shown_path(path: &Path) -> String {
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(home) if !home.as_os_str().is_empty() => match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        _ => path.display().to_string(),
    }
}

/// [`check`], with where each file to ask about is.
pub fn question(folder: &Path, shipped: &Path) -> Result<Vec<Asked>, RuntimeError> {
    Ok(check(folder, shipped)?.into_iter().map(|name| Asked { name, path: shown_path(&folder.join(name)) }).collect())
}

/// **OK, for one file** (the screen updates them one after another): `rewrite` (in the app,
/// [`rewrite_with_rich`]) is given the three texts and answers with the file's new text and its
/// own line about it. The new file is written and recorded and the before and after returned.
/// Nothing is written when `rewrite` fails, when its file is empty, or when the user's copy
/// changed while it ran. No copy of the previous file is kept (CEO, 2026-10-22).
pub fn accept(folder: &Path, shipped: &Path, name: &str,
              rewrite: impl FnOnce(&Update) -> Result<(String, String), String>) -> Result<UpdatedFile, String> {
    let name = *check(folder, shipped).map_err(|e| e.0)?.iter().find(|asked| **asked == name)
        .ok_or_else(|| format!("{name} has no update waiting"))?;
    let entry = read_record(folder).remove(name).ok_or_else(|| format!("{name} has no record"))?;
    let yours = own_copy(folder, name).ok_or_else(|| format!("{name} is not there"))?;
    let update = Update { name, started_from: entry.from, yours, new: shipped_text(shipped, name).map_err(|e| e.0)? };
    let (text, said) = rewrite(&update)?;
    if text.trim().is_empty() { return Err(format!("the update did not write {name}; nothing was changed")); }
    if own_copy(folder, name).as_deref() != Some(update.yours.as_str()) {
        return Err(format!("{name} was changed while it was being updated; nothing was changed"));
    }
    write_atomic(&folder.join(name), &text).map_err(|e| e.0)?;
    let mut record = read_record(folder);
    record.insert(name.to_string(), Entry { from: update.new.clone(), declined: None });
    write_record(folder, &record).map_err(|e| e.0)?;
    let rows = rows(&update.started_from, &update.yours, &text);
    let edits = rows.iter().filter(|row| row.iter().any(|(kind, _)| matches!(*kind, "a" | "d" | "y"))).count();
    Ok(UpdatedFile { name, path: shown_path(&folder.join(name)), rows, edits, said })
}

/// The paragraphs of a file: its blocks between blank lines.
fn paragraphs(text: &str) -> Vec<&str> {
    text.split("\n\n").map(|block| block.trim_matches('\n')).filter(|block| !block.is_empty()).collect()
}

/// The longest run two sequences share, as same / removed / added steps, in order.
fn steps<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut shared = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            shared[i][j] = if a[i] == b[j] { shared[i + 1][j + 1] + 1 } else { shared[i + 1][j].max(shared[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.push((Some(i), Some(j))); i += 1; j += 1;
        } else if i < a.len() && (j == b.len() || shared[i + 1][j] >= shared[i][j + 1]) {
            out.push((Some(i), None)); i += 1;
        } else {
            out.push((None, Some(j))); j += 1;
        }
    }
    out
}

/// Removed text is the user's own (`y`) when it is nowhere in the shipped version the user's copy
/// was made from; otherwise it is older RichOS wording (`d`).
fn removed(text: &str, started_from: &str) -> Segment {
    (if started_from.contains(text.trim()) { "d" } else { "y" }, text.to_string())
}

/// **The before and after, paragraph by paragraph**, for showing it (nothing is ever merged from
/// these). Paragraphs the two share are kept; between two kept ones, the removed and the added
/// paragraphs are paired in order, and a pair is compared word by word, a run of one or two shared
/// words inside a change counting as part of the change so the edit reads as a whole phrase.
pub fn rows(started_from: &str, before: &str, after: &str) -> Vec<Vec<Segment>> {
    let (b, a) = (paragraphs(before), paragraphs(after));
    let mut out = Vec::new();
    let (mut gone, mut new): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let flush = |gone: &mut Vec<&str>, new: &mut Vec<&str>, out: &mut Vec<Vec<Segment>>| {
        for k in 0..gone.len().max(new.len()) {
            out.push(match (gone.get(k), new.get(k)) {
                (Some(g), Some(n)) => words(started_from, g, n),
                (Some(g), None) => vec![removed(g, started_from)],
                (None, Some(n)) => vec![("a", n.to_string())],
                (None, None) => continue,
            });
        }
        gone.clear();
        new.clear();
    };
    for (i, j) in steps(&b, &a) {
        match (i, j) {
            (Some(i), Some(_)) => {
                flush(&mut gone, &mut new, &mut out);
                out.push(vec![(if out.is_empty() && b[i].starts_with("---") { "fm" } else { "s" }, b[i].to_string())]);
            }
            (Some(i), None) => gone.push(b[i]),
            (None, Some(j)) => new.push(a[j]),
            (None, None) => {}
        }
    }
    flush(&mut gone, &mut new, &mut out);
    out
}

/// One paragraph before and after, word by word ([`rows`]).
fn words(started_from: &str, before: &str, after: &str) -> Vec<Segment> {
    let split = |text: &str| -> Vec<String> {
        let mut tokens: Vec<String> = Vec::new();
        for c in text.chars() {
            match tokens.last_mut() {
                Some(last) if !c.is_whitespace() && last.chars().last().is_some_and(|l| !l.is_whitespace()) => last.push(c),
                Some(last) if c.is_whitespace() => last.push(c),
                _ => tokens.push(c.to_string()),
            }
        }
        tokens
    };
    let (b, a) = (split(before), split(after));
    // Each step as (kind, token): 's' same, 'r' removed, 'a' added.
    let mut marked: Vec<(char, String)> = steps(&b, &a).into_iter().map(|step| match step {
        (Some(i), Some(_)) => ('s', b[i].clone()),
        (Some(i), None) => ('r', b[i].clone()),
        (None, Some(j)) => ('a', a[j].clone()),
        (None, None) => unreachable!(),
    }).collect();
    // A run of one or two shared words with a change on both sides is part of the change.
    loop {
        let mut changed = false;
        let mut k = 0;
        while k < marked.len() {
            if marked[k].0 != 's' { k += 1; continue; }
            let end = (k..marked.len()).find(|&e| marked[e].0 != 's').unwrap_or(marked.len());
            if k > 0 && end < marked.len() && end - k <= 2 {
                let run: Vec<String> = marked.drain(k..end).map(|(_, t)| t).collect();
                let at = k;
                for (n, token) in run.iter().enumerate() { marked.insert(at + n, ('r', token.clone())); }
                for (n, token) in run.into_iter().enumerate() { marked.insert(end + n, ('a', token)); }
                changed = true;
                break;
            }
            k = end;
        }
        if !changed { break; }
    }
    // Between two shared runs, what was removed comes first and what was added after it.
    let mut out: Vec<Segment> = Vec::new();
    let (mut gone, mut new) = (String::new(), String::new());
    let push = |out: &mut Vec<Segment>, kind: &'static str, text: String| {
        if text.is_empty() { return; }
        match out.last_mut() {
            Some((last, prior)) if *last == kind => prior.push_str(&text),
            _ => out.push((kind, text)),
        }
    };
    for (kind, token) in marked {
        match kind {
            'r' => gone.push_str(&token),
            'a' => new.push_str(&token),
            _ => {
                if !gone.is_empty() { let (k, t) = removed(&gone, started_from); push(&mut out, k, t); gone.clear(); }
                push(&mut out, "a", std::mem::take(&mut new));
                push(&mut out, "s", token);
            }
        }
    }
    if !gone.is_empty() { let (k, t) = removed(&gone, started_from); push(&mut out, k, t); }
    push(&mut out, "a", new);
    out
}

/// How long the session may take. A careful rewrite of one file on the user's own model is a
/// minute or two; this only stops a stalled session being waited on for ever.
const WAIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// **The session that updates one changed copy**: Claude Code on the model Rich runs on, with the
/// engine's [`SKILL`] as its whole instruction and only the Write tool, given the three texts and
/// where to write the result. Returns the file it wrote and its answer.
pub fn rewrite_with_rich(bin: &Path, profile: &crate::engine_profile::EngineProfile, update: &Update)
    -> Result<(String, String), String> {
    let work = profile.state.join("instructions-update").join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let write_to = work.join(update.name);
    let result = (|| {
        let asked = serde_json::json!({"name": update.name, "started_from": update.started_from,
            "yours": update.yours, "new": update.new, "write_to": write_to});
        let body = serde_json::json!({"type": "user", "message": {"role": "user", "content": asked.to_string()}}).to_string();
        let mut live = crate::request_classifier::Provider::new(bin, profile, SKILL, "instructions-update")
            .writing(profile.lease_models.rich.cli_alias(), "Write").start()?;
        let said = live.exchange(body, std::time::Instant::now() + WAIT, || false, |frame| {
            let said = frame["result"].as_str().unwrap_or_default().trim();
            if said.is_empty() { Err("the update said nothing".to_string()) } else { Ok(said.to_string()) }
        })?;
        drop(live);
        let text = std::fs::read_to_string(&write_to).map_err(|_| format!("the update did not write {}", update.name))?;
        Ok((text, said))
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
    fn a_copy_without_a_record_starts_from_the_version_it_is_or_the_nearest_one() {
        let earlier = vec!["one\n".to_string(), "one\ntwo\n".to_string()];
        assert_eq!(starting_point("one\n", "one\ntwo\nthree\n", earlier.clone()), "one\n");
        assert_eq!(starting_point("one\ntwo\nthree\n", "one\ntwo\nthree\n", earlier.clone()), "one\ntwo\nthree\n");
        assert_eq!(starting_point("one\ntwo\nmine\n", "one\ntwo\nthree\nfour\n", earlier), "one\ntwo\n");
        // As near to the current version as to an earlier one: the earlier one, so it is asked.
        let earlier = vec!["a\nold\n".to_string()];
        assert_eq!(starting_point("a\nmine\n", "a\nnew\n", earlier.clone()), "a\nold\n");
        // Nearer to the current version alone: the user edited the current version.
        assert_eq!(starting_point("a\nnew\nmine\n", "a\nnew\n", earlier), "a\nnew\n");
    }

    /// The before and after the round-37 screen draws: the front matter, paragraphs kept, a user's
    /// sentence edited out and tagged as theirs (`y`), older RichOS wording taken out (`d`), and
    /// what the update added (`a`), a whole new paragraph among it.
    #[test]
    fn the_rows_show_each_paragraph_with_the_users_own_lines_told_apart() {
        let started_from = "---\nname: rich\n---\n\nYou are Rich.\n\nDo things quickly.\n\nOld rule.\n";
        let before = "---\nname: rich\n---\n\nYou are Rich.\n\nDo things quickly. Hand it to Stu always.\n\nOld rule.\n";
        let after = "---\nname: rich\n---\n\nYou are Rich.\n\nDo things quickly. When asked yourself, do it yourself.\n\nNew rule.\n\nAnother new paragraph.\n";
        let s = |k: &'static str, t: &str| (k, t.to_string());
        assert_eq!(rows(started_from, before, after), vec![
            vec![s("fm", "---\nname: rich\n---")],
            vec![s("s", "You are Rich.")],
            vec![s("s", "Do things quickly. "), s("y", "Hand it to Stu always."), s("a", "When asked yourself, do it yourself.")],
            vec![s("d", "Old "), s("a", "New "), s("s", "rule.")],
            vec![s("a", "Another new paragraph.")],
        ]);
        let json = serde_json::to_value(rows(started_from, before, after)).unwrap();
        assert_eq!(json[0][0], serde_json::json!(["fm", "---\nname: rich\n---"]), "a segment is [kind, text]");
    }

    /// **OK is done by Rich, from the skill** (CEO, 2026-10-22): the session is started with the
    /// engine's `update-instructions` skill as its instruction, on Rich's model, with only the
    /// Write tool, and is given the three texts; what it writes becomes the user's file, and the
    /// before and after is returned for the screen.
    #[test]
    fn ok_starts_a_session_with_the_three_texts_and_the_skill_and_returns_the_before_and_after() {
        let (root, bin, profile, _) = crate::request_classifier::tests::fixture(r#"
open('argv.json','w').write(json.dumps(sys.argv))
print(json.dumps({'type':'system','subtype':'init','tools':['Write']}),flush=True)
for line in sys.stdin:
    f=json.loads(json.loads(line)['message']['content'])
    open('asked.json','w').write(json.dumps(f))
    open(f['write_to'],'w').write(f['new']+'\n'+f['yours'].splitlines()[-1]+'\n')
    said='The update added a new section; your own line is kept.'
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
        let yours = format!("{older}\nMy own line.\n");
        std::fs::write(folder.join(RICH), &yours).unwrap();
        std::fs::write(shipped.join("agents").join(RICH), newer).unwrap();
        assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH]);

        let updated = accept(&folder, &shipped, RICH, |update| rewrite_with_rich(&bin, &profile, update)).unwrap();
        let session = profile.state.join("instructions-update");
        let argv: Vec<String> = serde_json::from_str(&std::fs::read_to_string(session.join("argv.json")).unwrap()).unwrap();
        let after = |flag: &str| argv[argv.iter().position(|a| a == flag).unwrap() + 1].clone();
        let skill = std::fs::read_to_string(profile.engine.join(SKILL)).unwrap();
        assert!(skill.contains("is edited out"), "the skill is not the one that says how");
        assert_eq!(after("--system-prompt"), skill, "how to update comes from the engine's skill");
        assert_eq!(after("--tools"), "Write");
        assert_eq!(after("--model"), profile.lease_models.rich.cli_alias(), "the update runs on Rich's model");
        let asked: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(session.join("asked.json")).unwrap()).unwrap();
        assert_eq!((asked["name"].as_str(), asked["started_from"].as_str(), asked["yours"].as_str(), asked["new"].as_str()),
            (Some(RICH), Some(older), Some(yours.as_str()), Some(newer)), "the session must read the three texts");

        let now = format!("{newer}\nMy own line.\n");
        assert_eq!(std::fs::read_to_string(folder.join(RICH)).unwrap(), now);
        assert_eq!(updated.said, "The update added a new section; your own line is kept.");
        assert_eq!(updated.rows, rows(older, &yours, &now));
        assert!(updated.rows.contains(&vec![("a", "A new section.".to_string())]), "{:?}", updated.rows);
        assert!(updated.rows.contains(&vec![("s", "My own line.".to_string())]), "{:?}", updated.rows);
        assert_eq!(updated.edits, 1);
        let mut kept: Vec<String> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        kept.sort();
        assert_eq!(kept, [".richos", "instructions.md", "rich.md", "stu.md"], "OK left a file beside the user's");
        assert!(check(&folder, &shipped).unwrap().is_empty(), "the question came back after OK");
        assert!(std::fs::read_dir(&session).unwrap().flatten().all(|entry| entry.path().is_file()), "the session's work folder was left behind");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **A conflict is with what the update changed, never with a line it left alone** (second
    /// review rv-20261022T102758Z-81c4f463-539d, finding 2, as the lead scoped it: the update's
    /// text wins over a user's change only where the update changed something). The rule lives
    /// in the skill (§124); this pins that the skill says it.
    #[test]
    fn the_skill_counts_a_conflict_only_against_what_the_update_changed() {
        let skill = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine").join(SKILL)).unwrap();
        assert!(skill.contains("A conflict is only with what the update changed"), "{skill}");
        assert!(skill.contains("to a part the update left exactly as it was is never a conflict"), "{skill}");
        assert!(skill.contains("is edited out: the update's text wins"), "the CEO's rule for a real conflict is gone: {skill}");
    }

    /// **The same rule, on the real model** (one live session; `#[ignore]` because it spends a
    /// real turn on the signed-in `claude` and needs the network):
    ///
    /// `cargo test -p richos-core --lib instructions_update::tests::live -- --ignored --nocapture`
    ///
    /// The user changed paragraph A ("English" to "German"); the update changed only paragraph
    /// B. A keeps the user's text and B gets the update's.
    #[test]
    #[ignore]
    fn live_a_users_change_to_a_paragraph_the_update_left_alone_is_kept() {
        let (root, _, profile, _) = crate::request_classifier::tests::fixture("");
        let bin = std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|dir| dir.join("claude")).find(|bin| bin.is_file()))
            .expect("no claude on PATH; nothing was verified");
        let head = "---\nname: rich\ndescription: Fixture chief of staff.\n---\n\nYou are Rich.\n\n";
        let update = Update {
            name: RICH,
            started_from: format!("{head}Always answer in English.\n\nKeep replies short.\n"),
            yours: format!("{head}Always answer in German.\n\nKeep replies short.\n"),
            new: format!("{head}Always answer in English.\n\nKeep replies short and lead with the answer, then the detail.\n"),
        };
        let (text, said) = rewrite_with_rich(&bin, &profile, &update).unwrap();
        eprintln!("--- written ---\n{text}--- said ---\n{said}");
        assert!(text.contains("German"), "the user's change to a paragraph the update did not touch was edited out:\n{text}");
        assert!(!text.contains("answer in English"), "the shipped wording the user changed came back:\n{text}");
        assert!(text.contains("lead with the answer"), "the update's change is missing:\n{text}");
        assert!(text.starts_with("---\nname: rich"), "the front matter was not kept first:\n{text}");
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
        assert!(accept(&folder, &shipped, RICH, |update| rewrite_with_rich(&bin, &profile, update)).is_err());
        assert_eq!(std::fs::read_to_string(folder.join(RICH)).unwrap(), "Old.\nMine.\n");
        assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH], "a failed update must ask again");
        assert!(accept(&folder, &shipped, STU, |_| panic!("an unchanged stu.md has nothing to update")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
