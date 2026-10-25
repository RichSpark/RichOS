//! **YOUR INSTRUCTIONS** — the user's own standing instructions file, edited from Settings and
//! filled by Rich's interview (CEO 2026-10-21, "Implement this simplified version for the app";
//! richos-hq `design/mockups/rounds/round-36/README.md` section 1, build plan
//! `docs/plans/2026-10-21-everything-for-every-user.md` item 1b).
//!
//! The file is `<central folder>/instructions.md` ([`crate::user_settings::INSTRUCTIONS`]), the
//! one `user_settings::standing_plugin` appends in full to Rich's and Stu's definitions at every
//! session start. Claude Code keeps the instructions a conversation started with, so an edit is
//! followed from the user's next NEW conversation, not in one already open (measured; the CEO's
//! "OK" of 2026-10-21 12:17:30Z to that default, rather than reloading open conversations). This
//! module never decides what Rich does with the words: it reads and writes the file he already
//! follows, and nothing else.
//!
//! Three writers, one file:
//!
//! - **The sheet** (Settings → Working with Rich → Your instructions) saves the whole text as the
//!   user types ([`save`]).
//! - **Rich**, through `mcp__richos_onboarding__save_your_instructions` ([`add_rules`]): the
//!   interview's last step, or a standing instruction the user gives in any conversation. It adds
//!   rules at the end and never rewrites what is there.
//! - **The user's own editor**, through "Open the file".
//!
//! # The card
//!
//! Each save of Rich's is written down ([`record`]) so the conversation can show it as the "Your
//! instructions · Saved" card under his reply, with "See your instructions". The record sits in
//! the folder of the files Rich puts in front of the user (`rich_files::DIR`), whose watcher
//! already shows a new row at once (`src-tauri/src/rich_files_host.rs`).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The tool's name on the onboarding server, and as the model sees it.
pub const TOOL: &str = "save_your_instructions";
pub const QUALIFIED_TOOL: &str = "mcp__richos_onboarding__save_your_instructions";

/// The timeline item kind the window reads.
pub const ITEM_KIND: &str = "instructions_saved";

const RECORD: &str = "instructions.jsonl";
const SCHEMA: u32 = 1;
/// The most rules one save takes, and the longest one rule may be. Generous: a rule is one line
/// in the user's words, and the file itself has no length limit (round 36: "there is no length
/// limit").
pub const MAX_RULES: usize = 40;
pub const MAX_RULE_CHARS: usize = 2000;

/// The file's place.
pub fn path(folder: &Path) -> PathBuf {
    folder.join(crate::user_settings::INSTRUCTIONS)
}

/// The text as it is on disk now, or empty when there is no file yet.
pub fn read(folder: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path(folder)) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("Your instructions could not be read: {e}")),
    }
}

/// Write the whole text, atomically, and read it back. A file that is a link (the user keeps it
/// somewhere else) is written where the link points, so the link stays the user's.
pub fn save(folder: &Path, text: &str) -> Result<(), String> {
    let mut target = path(folder);
    if std::fs::symlink_metadata(&target).is_ok_and(|m| m.file_type().is_symlink()) {
        target = std::fs::canonicalize(&target).map_err(|e| format!("Your instructions could not be saved: {e}"))?;
    }
    crate::doctrine::write_verified(&target, text).map_err(|e| match e {
        crate::doctrine::DoctrineError::Unwritable { why, .. } => format!("Your instructions could not be saved: {why}"),
        crate::doctrine::DoctrineError::NoHome => "Your instructions could not be saved: there is no home folder".into(),
    })
}

/// A line's words without its list mark: `- Ask me first.` and `Ask me first.` are one rule.
pub fn bare(line: &str) -> &str {
    let t = line.trim();
    for mark in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(mark) {
            return rest.trim();
        }
    }
    t
}

/// **Rich adds rules**: each one, in the user's words, as a `- ` line at the end of the file,
/// after everything already in it. A rule already in the file (same words) is not added twice.
/// Returns the rules as saved, in the order given; nothing is written when a rule is refused.
pub fn add_rules(folder: &Path, rules: &[String]) -> Result<Vec<String>, String> {
    if rules.is_empty() {
        return Err("Give at least one rule. Nothing was saved.".into());
    }
    if rules.len() > MAX_RULES {
        return Err(format!("Save at most {MAX_RULES} rules at a time. Nothing was saved."));
    }
    let mut clean = Vec::new();
    for rule in rules {
        let rule = bare(rule);
        if rule.is_empty() {
            return Err("A rule was empty. Nothing was saved.".into());
        }
        if rule.contains('\n') || rule.contains('\r') {
            return Err("Write each rule on one line, as its own item. Nothing was saved.".into());
        }
        if rule.chars().count() > MAX_RULE_CHARS {
            return Err(format!("A rule is longer than {MAX_RULE_CHARS} characters. Shorten it in the user's words. Nothing was saved."));
        }
        if !clean.contains(&rule.to_string()) {
            clean.push(rule.to_string());
        }
    }
    let mut text = read(folder)?;
    let present: Vec<String> = text.lines().map(|l| bare(l).to_string()).collect();
    let mut added = false;
    for rule in &clean {
        if present.contains(rule) {
            continue;
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("- ");
        text.push_str(rule);
        text.push('\n');
        added = true;
    }
    if added {
        save(folder, &text)?;
    }
    Ok(clean)
}

/// One save of Rich's, as the conversation shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub schema: u32,
    pub id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub rules: Vec<String>,
    /// Epoch millis.
    pub at: u64,
}

/// The record's file under an engine state root.
pub fn record_path(root: &Path) -> PathBuf {
    root.join(crate::rich_files::DIR).join(RECORD)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Write down one save for a turn. One line, appended in one write.
pub fn record(root: &Path, entity_id: &str, thread_id: &str, turn_id: &str, rules: &[String]) -> Result<Card, String> {
    use sha2::{Digest, Sha256};
    let at = now_ms();
    let digest = Sha256::digest(format!("{thread_id}\n{turn_id}\n{at}\n{}", rules.join("\n")).as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let card = Card {
        schema: SCHEMA,
        id: format!("yi_{hex}"),
        entity_id: entity_id.into(),
        thread_id: thread_id.into(),
        turn_id: turn_id.into(),
        rules: rules.to_vec(),
        at,
    };
    let file = record_path(root);
    std::fs::create_dir_all(file.parent().unwrap_or(root)).map_err(|e| e.to_string())?;
    let mut line = serde_json::to_string(&card).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut out = std::fs::OpenOptions::new().create(true).append(true).open(&file).map_err(|e| e.to_string())?;
    out.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    Ok(card)
}

/// Every card the record holds, in the order they were saved. A line that does not parse is
/// skipped, never half-read.
pub fn all(root: &Path) -> Vec<Card> {
    let Ok(text) = std::fs::read_to_string(record_path(root)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Card>(line).ok())
        .filter(|card| card.schema == SCHEMA)
        .collect()
}

/// One card as a timeline item, the shape `rich_files::item` uses.
pub fn item(card: &Card) -> Value {
    json!({
        "kind": ITEM_KIND,
        "id": card.id,
        "entityId": card.entity_id,
        "threadId": card.thread_id,
        "turnId": card.turn_id,
        "visibility": "ceo",
        "createdAt": card.at,
        "sequence": card.at,
        "rules": card.rules,
    })
}

/// The cards of one conversation of one company, as timeline items.
pub fn items(root: &Path, entity_id: &str, thread_id: &str) -> Vec<Value> {
    all(root)
        .iter()
        .filter(|card| card.entity_id == entity_id && card.thread_id == thread_id)
        .map(item)
        .collect()
}

/// The tool's entry in the onboarding server's list.
pub fn tool() -> Value {
    json!({"name": TOOL,
        "description": "Save how the user wants you to work: their standing instructions, which you and Stu follow in full in every company, starting with the user's next new conversation. Each rule is one short line in the user's own words. The rules are added at the end of the user's instructions file, after everything already in it and the user sees them as a card under your reply. Use it for the interview's question about how they want you to work and whenever they give you a new standing instruction. Never use Write, Edit or a shell for this file.",
        "inputSchema": {"type": "object", "properties": {
            "rules": {"type": "array", "minItems": 1, "maxItems": MAX_RULES,
                "items": {"type": "string", "minLength": 1, "maxLength": MAX_RULE_CHARS},
                "description": "The new rules, one per item, each in the user's words."}
        }, "required": ["rules"], "additionalProperties": false},
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let p = std::env::temp_dir().join(format!("richos-your-instructions-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4().simple()));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                eprintln!("test scratch {} was not removed: {e}", self.0.display());
            }
        }
    }

    #[test]
    fn rules_go_at_the_end_after_everything_already_there_and_never_twice() {
        let dir = Dir::new("add");
        std::fs::write(path(&dir.0), "Write in plain English.\n- Ask me first.").unwrap();
        let saved = add_rules(&dir.0, &["- Ask me first.".into(), "Mornings are for deep work.".into(), "Mornings are for deep work.".into()]).unwrap();
        assert_eq!(saved, vec!["Ask me first.", "Mornings are for deep work."]);
        assert_eq!(read(&dir.0).unwrap(), "Write in plain English.\n- Ask me first.\n- Mornings are for deep work.\n");
    }

    #[test]
    fn a_refused_rule_writes_nothing() {
        let dir = Dir::new("refuse");
        assert!(add_rules(&dir.0, &[]).is_err());
        assert!(add_rules(&dir.0, &["one".into(), "two\nthree".into()]).is_err());
        assert!(add_rules(&dir.0, &["  ".into()]).is_err());
        assert!(!path(&dir.0).exists(), "a refused save created the file");
    }

    #[test]
    fn the_sheet_saves_the_whole_text_and_a_link_stays_a_link() {
        let dir = Dir::new("save");
        assert_eq!(read(&dir.0).unwrap(), "", "no file reads as nothing written yet");
        save(&dir.0, "- Ask me before spending more than $500.\n").unwrap();
        assert_eq!(read(&dir.0).unwrap(), "- Ask me before spending more than $500.\n");
        let elsewhere = dir.0.join("dotfiles");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("mine.md"), "old").unwrap();
        std::fs::remove_file(path(&dir.0)).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("mine.md"), path(&dir.0)).unwrap();
        save(&dir.0, "new").unwrap();
        assert!(std::fs::symlink_metadata(path(&dir.0)).unwrap().file_type().is_symlink(), "the link was replaced");
        assert_eq!(std::fs::read_to_string(elsewhere.join("mine.md")).unwrap(), "new");
    }

    #[test]
    fn a_card_belongs_to_its_conversation_and_turn() {
        let dir = Dir::new("card");
        let root = dir.0.join("engine-state");
        let card = record(&root, "acme", "t1", "turn-1", &["Ask me first.".into()]).unwrap();
        record(&root, "acme", "t2", "turn-9", &["Elsewhere.".into()]).unwrap();
        let shown = items(&root, "acme", "t1");
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0]["kind"], ITEM_KIND);
        assert_eq!(shown[0]["id"], card.id);
        assert_eq!(shown[0]["turnId"], "turn-1");
        assert_eq!(shown[0]["rules"], json!(["Ask me first."]));
        assert!(items(&root, "other", "t1").is_empty(), "another company's conversation of the same id");
    }
}
