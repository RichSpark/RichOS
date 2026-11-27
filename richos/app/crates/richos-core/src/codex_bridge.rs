//! Native channel boundary: host scope, existing inbox acceptance, then position.
//! Channel framing, discovery and writes live exclusively in the shared engine.
use crate::{session_tools, status_tools::StatusToolScope};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

pub mod ownership;

pub const TOOL: &str = "codex_channel";
const AUTOMATIC: &str = "_automatic";

pub fn root() -> PathBuf {
    let path = std::env::var_os("CODEX_COORDINATION_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".richos-coordination")
        });
    path.canonicalize().unwrap_or(path)
}

fn component(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Invalid channel identity".into());
    }
    Ok(())
}

fn digest(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

/// `binding` was re-resolved from the ledger for this attempt, never the visible chat.
pub fn accept(
    state: &Path,
    binding: &crate::entity::ThreadBinding,
    event: &Value,
    coordination: &Path,
) -> Result<(), String> {
    take(state, binding, event, coordination, Take::Accept)
}

/// What [`take`] does with a report it has checked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Take {
    /// Into Stu's inbox, and the watcher's place moved past it: [`accept`].
    Accept,
    /// Set aside in [`HELD`] for when the user acts in its chat, and the watcher's place moved
    /// past it, so what Codex wrote behind it on the connection is offered next.
    SetAside,
    /// Into Stu's inbox from [`HELD`]: the watcher's place is already past it.
    Release,
}

fn take(
    state: &Path,
    binding: &crate::entity::ThreadBinding,
    event: &Value,
    coordination: &Path,
    how: Take,
) -> Result<(), String> {
    let c = &event["connection"];
    let entity = c["entity_id"].as_str().ok_or("Missing channel company")?;
    let thread = c["thread_id"]
        .as_str()
        .ok_or("Missing channel conversation")?;
    let id = c["connection_id"].as_str().ok_or("Missing connection ID")?;
    for part in [entity, thread, id] {
        component(part)?;
    }
    if c["version"] != 1 || entity != binding.entity_id().as_str() || thread != binding.thread_id()
    {
        return Err("Channel endpoint disagrees with its ledger binding".into());
    }
    let base = coordination.join("connections");
    let direct = base.join(entity).join(thread).join(id);
    let queued = base.join(AUTOMATIC).join("codex").join(id);
    let locator = match event["locator"].as_str() {
        path if path == direct.to_str() => direct,
        path if path == queued.to_str() => {
            let file = std::fs::File::open(queued.join("connection.json")).map_err(|e| e.to_string())?;
            let saved: Value = serde_json::from_reader(file.take(256 * 1024)).map_err(|e| e.to_string())?;
            if saved["version"] != 1 || saved["entity_id"] != AUTOMATIC
                || saved["thread_id"] != "codex" || saved["connection_id"] != id
                || saved["binding"]["entity_id"] != entity || saved["binding"]["thread_id"] != thread
            {
                return Err("Queued Codex connection disagrees with its permanent binding".into());
            }
            queued
        }
        _ => return Err("Channel is outside its registered location".into()),
    };
    if locator.canonicalize().map_err(|e| e.to_string())? != locator {
        return Err("Channel is outside its registered location".into());
    }
    let record = &event["record"];
    let kind = record["kind"].as_str();
    // An upgrade can start the previous verified engine first.
    if matches!(kind, Some("legacy_import" | "legacy_notice")) {
        return Ok(());
    }
    if kind == Some("legacy_live")
        && ownership::unresolved_legacy(coordination, c).map_err(|e| e.to_string())?
    {
        return Err("Legacy Codex work is held until its registered company can be verified".into());
    }
    let through = event["through"]
        .as_u64()
        .ok_or("Missing accepted position")?;
    let identity = event["source_identity"]
        .as_str()
        .ok_or("Missing source identity")?;
    let scope = StatusToolScope {
        version: 1,
        state_root: state.into(),
        entity_id: entity.into(),
        thread_id: thread.into(),
    };
    if kind != Some("task_context") && how != Take::SetAside {
        let text = record["text"]
            .as_str()
            .ok_or("Channel record has no text")?;
        let message_id = record["message_id"]
            .as_str()
            .ok_or("Channel record has no ID")?;
        component(message_id)?;
        let legacy_text;
        let text = if kind == Some("legacy_live") {
            legacy_text = format!("Legacy Markdown observation; verify the complete original request before acting.\n{text}");
            &legacy_text
        } else {
            text
        };
        let receipt_locator = if locator.starts_with(base.join(AUTOMATIC)) {
            locator
        } else {
            ownership::historic_locator(coordination, &locator, c).map_err(|e| e.to_string())?
        };
        session_tools::observe_channel_from(
            &scope,
            text,
            session_tools::BridgeReference {
                connection_id: id.into(),
                message_id: message_id.into(),
                payload_digest: digest(record),
                locator: receipt_locator.to_string_lossy().into_owned(),
                reply_to: record["reply_to"].as_str().map(str::to_string),
            },
            if kind == Some("review_result") { "review-watch" } else { "codex-watch" },
        )?;
    }
    match how {
        Take::Accept => {}
        // Saved before the place moves past it: it is never dropped.
        Take::SetAside => set_aside(state, id, event)?,
        Take::Release => return Ok(()),
    }
    // Only reached after the exact inbox row was synced (or non-actionable context), or the
    // report was set aside.
    let suffix = if kind == Some("legacy_live") { ".live.json" } else { ".json" };
    let path = state
        .join("codex-watch/accepted")
        .join(format!("{id}{suffix}"));
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    #[cfg(test)]
    crash_boundary("before-position-write");
    let temporary = path.with_extension(format!("{}.incoming", uuid::Uuid::new_v4()));
    {
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(
            json!({"through":through,"source_identity":identity})
                .to_string()
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        #[cfg(test)]
        crash_boundary("before-position-sync");
        file.sync_all().map_err(|e| e.to_string())?;
        #[cfg(test)]
        crash_boundary("after-position-sync");
    }
    std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    #[cfg(test)]
    crash_boundary("after-position-rename");
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    #[cfg(test)]
    crash_boundary("after-position-directory-sync");
    Ok(())
}

/// **Which Codex reports were written before the app opened.** Neither the journals nor the
/// Markdown channels carry a clock, so how long each file was at launch is what tells a report
/// from before from one written while the app is open. Whether one from before may start a
/// turn is the one rule's to say ([`crate::turn_gate`]); this only knows which is which.
pub struct Asked {
    gate: std::sync::Arc<crate::turn_gate::TurnGate>,
    /// How long each connection's journal was when the app opened ([`Self::nobody_wrote_since`]).
    journals: std::collections::HashMap<PathBuf, u64>,
    /// The same for each old Markdown channel file the watcher reads, and, once asked for, how
    /// many lines that length holds.
    markdown: std::collections::HashMap<PathBuf, u64>,
    markdown_lines: std::sync::Mutex<std::collections::HashMap<PathBuf, u64>>,
    /// Held while a report is set aside, released or accepted ([`accept_when_asked`],
    /// [`Self::release_held`]): the watcher's thread and the one that releases what was set
    /// aside when the user acts never hand Stu one connection's reports out of order.
    aside: std::sync::Mutex<()>,
}

/// Where the app's watcher keeps its place in each old Markdown channel file
/// (`codex_watch.py` `_legacy_tick`, the app host's session directory).
const MARKDOWN_PLACES: &str = "codex-watch/watch/app-host/live-offsets.json";

/// **How long each connection's journal was when the app opened**, for the app's review
/// watcher, a process of its own (`review_watch.py` `AppWorld.held_by_the_bridge`): a READY
/// written before then is reviewed only once this bridge has handed it to Stu, which it does
/// once the user has acted in its chat (review rv-20261127T025408Z-0159fa0b-818a, finding 4).
/// In the app's engine state folder, replaced whole at every launch.
pub const JOURNALS_AT_LAUNCH: &str = "codex-watch/journals-at-launch.json";

fn write_journals_at_launch(state: &Path, journals: &std::collections::HashMap<PathBuf, u64>) {
    let path = state.join(JOURNALS_AT_LAUNCH);
    let partial = path.with_extension("json.partial");
    let body = json!({"version": 1, "journals": journals.iter()
        .map(|(journal, length)| (journal.to_string_lossy().into_owned(), json!(length))).collect::<serde_json::Map<_, _>>()});
    let written = std::fs::create_dir_all(path.parent().unwrap_or(state))
        .and_then(|()| std::fs::write(&partial, body.to_string()))
        .and_then(|()| std::fs::rename(&partial, &path));
    if let Err(error) = written {
        eprintln!("[richos] the Codex journals' lengths at launch could not be written for the review watcher: {error}");
    }
}

impl Asked {
    /// **The app is opening now.** Neither the journals nor the Markdown channels carry a
    /// clock, so how long each file is at this moment is what tells a report from before from
    /// one written while the app is open: a few dozen `stat` calls, once. `state` is the app's
    /// engine state folder.
    pub fn at_launch(coordination: &Path, state: &Path, gate: std::sync::Arc<crate::turn_gate::TurnGate>) -> Self {
        let mut journals = std::collections::HashMap::new();
        let mut folders = vec![coordination.join("connections")];
        // connections/<company>/<conversation>/<connection>/to-rich.jsonl
        for _ in 0..3 {
            folders = folders.iter().filter_map(|folder| std::fs::read_dir(folder).ok())
                .flat_map(|entries| entries.flatten().map(|entry| entry.path())).collect();
        }
        for folder in folders {
            let journal = folder.join(JOURNAL);
            if let (Ok(length), Ok(journal)) = (std::fs::metadata(&journal).map(|file| file.len()), journal.canonicalize()) {
                journals.insert(journal, length);
            }
        }
        write_journals_at_launch(state, &journals);
        // The files the watcher already has a place in. One it has never read starts at its
        // end (`look_one`), so it holds nothing from before.
        let mut markdown = std::collections::HashMap::new();
        let places: Value = std::fs::read(state.join(MARKDOWN_PLACES)).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
        for file in places.as_object().into_iter().flat_map(|places| places.keys()) {
            if let Ok(length) = std::fs::metadata(file).map(|file| file.len()) {
                markdown.insert(PathBuf::from(file), length);
            }
        }
        Self { gate, journals,
            markdown, markdown_lines: Default::default(), aside: Default::default() }
    }

    /// **Was this report written before the app opened?**
    ///
    /// A report that ends within its journal's length at launch was written while the app was
    /// closed. So was, in effect, a notice the app's own review watcher writes (`review_result`)
    /// about one of those: it waits with the report it answers. A verdict on a report Codex
    /// wrote while the app is open reaches Stu by itself, as that report did (review
    /// rv-20261127T025408Z-0159fa0b-818a, finding 3); one whose report is not among those
    /// waits. A journal that did not exist at launch holds nothing from before. A reply on an
    /// old Markdown channel (`legacy_live`) names the lines it came from, and it is from before
    /// when it starts within the file's length at launch.
    fn written_before_launch(&self, event: &Value) -> bool {
        let kind = event["record"]["kind"].as_str();
        match kind {
            Some("legacy_live") => return self.markdown_reply_from_before(event["record"]["text"].as_str().unwrap_or_default()),
            Some(kind) if kind.starts_with("legacy_") => return false,
            _ => {}
        }
        let Some(journal) = Self::journal(event) else { return false };
        let (Some(length), Some(through)) = (self.journals.get(&journal), event["through"].as_u64()) else { return false };
        through <= *length || kind == Some("review_result")
            && !event["record"]["reply_to"].as_str().is_some_and(|report| codex_wrote_after(&journal, *length, Some(report)))
    }

    /// **Has Codex written behind this report since the app opened?** The watcher offers a
    /// connection's reports one at a time, in order, so a report left in its journal holds back
    /// every one Codex writes there while the app is open: it is set aside instead
    /// ([`accept_when_asked`]). The old Markdown channels share one spool, so a reply written on
    /// ANY of them is behind this one.
    fn written_behind(&self, event: &Value) -> bool {
        if event["record"]["kind"] == "legacy_live" {
            return self.markdown.iter().any(|(file, length)| std::fs::metadata(file).is_ok_and(|now| now.len() > *length));
        }
        let Some(journal) = Self::journal(event) else { return false };
        self.journals.get(&journal).is_some_and(|length| codex_wrote_after(&journal, *length, None))
    }

    fn journal(event: &Value) -> Option<PathBuf> {
        event["locator"].as_str().and_then(|locator| Path::new(locator).join(JOURNAL).canonicalize().ok())
    }

    /// **What was set aside on this report's connection goes to Stu first**, oldest first,
    /// once the user has been active in its chat since launch; until then it stays set aside.
    /// The caller holds [`Self::aside`].
    fn release_connection(&self, state: &Path, binding: &crate::entity::ThreadBinding, event: &Value, coordination: &Path) -> Result<(), String> {
        let Some(id) = event["connection"]["connection_id"].as_str() else { return Ok(()) };
        component(id)?;
        let path = state.join(HELD).join(format!("{id}.jsonl"));
        if !path.exists() || !self.gate.may_start(binding, crate::turn_gate::Origin::FromBefore) {
            return Ok(());
        }
        for held in held_events(&path)? {
            take(state, binding, &held, coordination, Take::Release)?;
        }
        // Each is in Stu's inbox now; one given again after a crash here is already there.
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        std::fs::File::open(state.join(HELD)).and_then(|folder| folder.sync_all()).map_err(|e| e.to_string())
    }

    /// **The user has just acted in a chat** ([`crate::turn_gate::TurnGate::when_a_chat_opens`]):
    /// every report set aside for a chat he has been active in since launch goes to Stu now,
    /// oldest first; the rest stay set aside. `binding_for` resolves a conversation the ledger
    /// and the company registry still hold. Every connection is tried; the first failure is
    /// returned, and what failed stays set aside.
    pub fn release_held(&self, state: &Path, coordination: &Path,
        binding_for: &dyn Fn(&str) -> Option<crate::entity::ThreadBinding>) -> Result<(), String> {
        let _one = self.aside.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(entries) = std::fs::read_dir(state.join(HELD)) else { return Ok(()) };
        let mut failure = None;
        for path in entries.flatten().map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|e| e == "jsonl")) {
            let result = held_events(&path).and_then(|held| {
                let Some(first) = held.first() else { return Ok(()) };
                let Some(binding) = first["connection"]["thread_id"].as_str().and_then(binding_for) else { return Ok(()) };
                self.release_connection(state, &binding, first, coordination)
            });
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// The watcher's block for a Markdown entry opens with where it came from:
    /// `... (lines 12-15 of /path/to-rich.md); every new line follows:` (`codex_watch.py`
    /// `look_one`). A block that says nothing of the kind is held: it cannot be shown to be new.
    fn markdown_reply_from_before(&self, text: &str) -> bool {
        let head = text.lines().next().unwrap_or_default();
        let Some((first, file)) = head.rsplit_once("(lines ").and_then(|(_, rest)| rest.split_once(" of "))
            .and_then(|(lines, rest)| Some((lines.split('-').next()?.parse::<u64>().ok()?, rest.rsplit_once(");")?.0)))
        else { return true };
        let file = PathBuf::from(file);
        let Some(length) = self.markdown.get(&file) else { return false };
        let mut lines = self.markdown_lines.lock().unwrap();
        let before = *lines.entry(file.clone()).or_insert_with(|| {
            use std::io::Read;
            let mut bytes = Vec::new();
            match std::fs::File::open(&file).map(|opened| opened.take(*length).read_to_end(&mut bytes)) {
                // A last line with no newline yet is a line the file already held at launch:
                // the watcher offers it as one (review rv-20261126T112554Z-6b571438-ca3d, #1).
                Ok(Ok(_)) => bytes.iter().filter(|byte| **byte == b'\n').count() as u64
                    + u64::from(bytes.last().is_some_and(|last| *last != b'\n')),
                _ => u64::MAX,
            }
        });
        first <= before
    }

}

/// The file in a connection's folder Codex and the review watcher write to Rich's team in.
const JOURNAL: &str = "to-rich.jsonl";

/// Has Codex written a report in this journal past `length`, or, given `report`, that report?
/// A review watcher's notice (`review_result`) is the app's own writing, not Codex's.
fn codex_wrote_after(journal: &Path, length: u64, report: Option<&str>) -> bool {
    use std::io::{BufRead, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(journal) else { return false };
    if file.seek(SeekFrom::Start(length)).is_err() {
        return false;
    }
    std::io::BufReader::new(file).lines().map_while(Result::ok).any(|line| {
        serde_json::from_str::<Value>(&line).is_ok_and(|record| record.is_object() && record["kind"] != "review_result"
            && report.is_none_or(|report| record["message_id"] == report))
    })
}

/// Where a report from before waits once Codex has written behind it on its connection
/// ([`accept_when_asked`]): `<state>/codex-watch/held/<connection>.jsonl`, each report as the
/// watcher offered it, one per line, oldest first.
const HELD: &str = "codex-watch/held";

/// **Set this report aside**, on disk before the watcher's place moves past it. Offered again
/// after a crash between the two, it is not saved twice.
fn set_aside(state: &Path, id: &str, event: &Value) -> Result<(), String> {
    use std::io::Write;
    let folder = state.join(HELD);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join(format!("{id}.jsonl"));
    if held_events(&path)?.iter().any(|held| held["record"] == event["record"] && held["through"] == event["through"]) {
        return Ok(());
    }
    // A last line a crash left half written is cut off first, or this report would be joined
    // to it; the watcher's place never moved past that report, so it is offered again now.
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.last().is_some_and(|last| *last != b'\n') {
            let whole = bytes.iter().rposition(|byte| *byte == b'\n').map_or(0, |end| end + 1);
            std::fs::OpenOptions::new().write(true).open(&path).and_then(|file| file.set_len(whole as u64))
                .map_err(|e| e.to_string())?;
        }
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| e.to_string())?;
    file.write_all(format!("{event}\n").as_bytes()).and_then(|()| file.sync_all()).map_err(|e| e.to_string())?;
    std::fs::File::open(&folder).and_then(|folder| folder.sync_all()).map_err(|e| e.to_string())
}

/// The reports set aside in `path`, oldest first; none when there is no such file. A last line
/// with no newline was never completely written, and its report is still in its journal.
fn held_events(path: &Path) -> Result<Vec<Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let complete = text.rfind('\n').map_or("", |end| &text[..end]);
    complete.lines().filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| format!("{}: {e}", path.display()))).collect()
}

/// What the watcher's log says once for a report that waits ([`accept_when_asked`]).
pub const WAITS_FOR_THE_USER: &str =
    "written before RichOS opened, for a conversation the user has not been active in since; the report stays in its journal";

/// What the app's log says when a report from before is set aside so the one behind it can go.
pub const SET_ASIDE: &str =
    "[richos] a Codex report written before RichOS opened is set aside until the user is active in its conversation; the newer one behind it goes ahead";

/// **[`accept`], unless the report is from before the app opened and the one rule holds it**
/// (CEO 2026-11-26: "Everything was either stopped, finished, ended or paused in the RichOS
/// app. [...] there should have been absolutely zero usage after me just opening the app.").
///
/// Every accepted report is a turn of Stu's, on his whole session, and usually one of Rich's
/// after it. On 2026-11-26 the app opened after two days closed and the watcher handed Stu six
/// Codex and review reports written meanwhile: 23 requests and 5.19 million tokens on Fable
/// before the user had typed a word. A report that was already in its journal when the app
/// opened is saved input from before ([`crate::turn_gate::Origin::FromBefore`]): refused here
/// until the user is active in that conversation, and it stays in the journal, where it
/// already waited while the app was closed. The watcher offers it again at every scan, and the
/// first offer after that accepts it, in order. A job record that still says it is open opens
/// nothing (a quit never rewrites it).
///
/// A report written while the app is open reaches Stu by itself. The watcher offers a
/// connection's reports one at a time, so once Codex writes behind a held report, the held one
/// is set aside ([`HELD`]) and the newer one is offered and goes; the one set aside still waits
/// for the user (review rv-20261127T005026Z-12b7c7e4-015b, finding 1). When he acts in that
/// chat it goes to Stu ([`Asked::release_held`]), and anything that connection offers after
/// that goes only behind it. `coordination` is the folder `asked` was made from.
pub fn accept_when_asked(
    state: &Path,
    binding: &crate::entity::ThreadBinding,
    event: &Value,
    coordination: &Path,
    asked: &Asked,
) -> Result<(), String> {
    let _one = asked.aside.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if asked.written_before_launch(event) && !asked.gate.may_start(binding, crate::turn_gate::Origin::FromBefore) {
        if !asked.written_behind(event) {
            return Err(WAITS_FOR_THE_USER.into());
        }
        take(state, binding, event, coordination, Take::SetAside)?;
        eprintln!("{SET_ASIDE}");
        return Ok(());
    }
    asked.release_connection(state, binding, event, coordination)?;
    accept(state, binding, event, coordination)
}

// Test-only process-death probes at the real persistence calls. Nothing is
// compiled into the delivered app, and only the explicitly launched child has
// the stage variable. No timer simulates a durability boundary.
#[cfg(test)]
pub(crate) fn crash_boundary(stage: &str) {
    if std::env::var("RICHOS_CHANNEL_TEST_CRASH").as_deref() == Ok(stage) {
        unsafe {
            libc::kill(libc::getpid(), libc::SIGKILL);
        }
    }
}

/// Invoke the existing engine implementation. The model supplies no destination IDs.
pub fn call(scope_path: &Path, scope: &StatusToolScope, arguments: Value) -> Result<Value, String> {
    let sender: session_tools::Session = serde_json::from_slice(
        &std::fs::read(scope_path.with_extension("session.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if !matches!(sender.role.as_str(), "rich" | "stu")
        || sender.entity_id != scope.entity_id
        || sender.thread_id != scope.thread_id
    {
        return Err("Channel caller does not match the host session".into());
    }
    let allowed = [
        "action",
        "locator",
        "message_id",
        "text",
        "provenance",
        "reply_to",
        "direction",
        "offset",
    ];
    if arguments
        .as_object()
        .is_none_or(|m| m.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err("Unknown channel arguments; endpoint IDs are host-owned".into());
    }
    let action = arguments["action"]
        .as_str()
        .ok_or("A channel action is required")?;
    if !matches!(action, "connect" | "send" | "read" | "quota") {
        return Err("Unknown channel action".into());
    }
    let engine = PathBuf::from(
        std::env::var_os("RICHOS_CHANNEL_ENGINE").ok_or("No host engine for channel helper")?,
    );
    let python = PathBuf::from(
        std::env::var_os("RICHOS_CHANNEL_PYTHON").ok_or("No host runtime for channel helper")?,
    );
    let mut request = arguments
        .as_object()
        .ok_or("Expected channel arguments")?
        .clone();
    request.remove("action");
    let mut command = crate::runtime::interpreter_command(&python);
    command
        .arg(engine.join("scripts/lib/codex_channels.py"))
        .arg(action)
        .arg("--scope")
        .arg(scope_path)
        .arg("--request")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::owned_process::OwnedChild::configure(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("No helper input")?;
    let stdout = child.stdout.take().ok_or("No helper output")?;
    let stderr = child.stderr.take().ok_or("No helper diagnostic")?;
    let mut child = crate::owned_process::OwnedChild::new(child);
    let input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    fn bounded(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
        let mut out = Vec::new();
        pipe.by_ref().take(512 * 1024 + 1).read_to_end(&mut out)?;
        Ok(out)
    }
    let output = std::thread::spawn(move || bounded(stdout));
    let errors = std::thread::spawn(move || bounded(stderr));
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            None => {
                child.kill().map_err(|e| e.to_string())?;
                child.wait().map_err(|e| e.to_string())?;
                return Err("Channel helper timed out; read sent history before retrying the same message ID".into());
            }
        }
    };
    writer
        .join()
        .map_err(|_| "Channel input stopped")?
        .map_err(|e| e.to_string())?;
    let out = output
        .join()
        .map_err(|_| "Channel output stopped")?
        .map_err(|e| e.to_string())?;
    let err = errors
        .join()
        .map_err(|_| "Channel diagnostic stopped")?
        .map_err(|e| e.to_string())?;
    if out.len() > 512 * 1024 || err.len() > 512 * 1024 {
        return Err("Channel helper response too large".into());
    }
    if !status.success() {
        return Err(String::from_utf8_lossy(&err).into_owned());
    }
    serde_json::from_slice(&out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        state: PathBuf,
        coord: PathBuf,
        binding: crate::entity::ThreadBinding,
        event: Value,
    }
    impl Fixture {
        fn new() -> Self {
            // `accept` demands a canonical locator; TMPDIR is a symlink on macOS
            // (/var -> /private/var), so resolve the fixture root once.
            let root =
                std::env::temp_dir().join(format!("richos-channel-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let mut ledger = crate::ledger::Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
            let thread = ledger
                .create_thread("Same title", &crate::EntityId::parse("company-a").unwrap())
                .unwrap();
            let binding = ledger.thread_binding(&thread).unwrap();
            let coord = root.join("coord");
            let state = root.join("state");
            let locator = coord
                .join("connections/company-a")
                .join(&thread)
                .join("pair");
            std::fs::create_dir_all(&locator).unwrap();
            let event = json!({"type":"channel","locator":locator,"connection":{"version":1,"entity_id":"company-a","thread_id":thread,"connection_id":"pair"},
                "source_identity":"1:2","through":100,"record":{"version":1,"message_id":"message-1","text":"Original request"}});
            Self {
                root,
                state,
                coord,
                binding,
                event,
            }
        }
        fn scope(&self) -> StatusToolScope {
            StatusToolScope {
                version: 1,
                state_root: self.state.clone(),
                entity_id: self.binding.entity_id().to_string(),
                thread_id: self.binding.thread_id().into(),
            }
        }
        fn rows(&self) -> Vec<session_tools::Message> {
            std::fs::read_to_string(session_tools::inbox(&self.scope(), "stu").unwrap())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
        /// The message IDs in Stu's inbox, in order; none before it exists.
        fn delivered(&self) -> Vec<String> {
            if !session_tools::inbox(&self.scope(), "stu").unwrap().exists() {
                return Vec::new();
            }
            self.rows().into_iter().map(|row| row.bridge.unwrap().message_id).collect()
        }
        /// The app's own release once the user acts in a chat (`main.rs` `codex_sink`).
        fn release(&self, asked: &Asked) {
            let binding = self.binding.clone();
            asked.release_held(&self.state, &self.coord, &|thread| (thread == binding.thread_id()).then(|| binding.clone())).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn codex_inbox_sync_before_failed_position_write_replays_once_after_ack() {
        let f = Fixture::new();
        std::fs::create_dir_all(f.state.join("codex-watch")).unwrap();
        std::fs::write(
            f.state.join("codex-watch/accepted"),
            "blocked position directory",
        )
        .unwrap();
        assert!(accept(&f.state, &f.binding, &f.event, &f.coord).is_err());
        assert_eq!(
            f.rows().len(),
            1,
            "inbox was saved before the position failure"
        );
        let inbox = session_tools::inbox(&f.scope(), "stu").unwrap();
        session_tools::acknowledge(&f.scope(), "stu", std::fs::metadata(inbox).unwrap().len())
            .unwrap();
        std::fs::remove_file(f.state.join("codex-watch/accepted")).unwrap();
        accept(&f.state, &f.binding, &f.event, &f.coord).unwrap();
        assert_eq!(
            f.rows().len(),
            1,
            "acknowledged history still reconciles the retry"
        );
        let mut conflict = f.event.clone();
        conflict["record"]["text"] = json!("Different request");
        assert!(accept(&f.state, &f.binding, &conflict, &f.coord)
            .unwrap_err()
            .contains("conflict"));
        assert_eq!(f.rows().len(), 1);
    }

    /// The app as it opened: the ledger already holds the chat, the connection's journal
    /// already holds the fixture's report (it ends at byte 100), and nothing since.
    fn opened(f: &Fixture) -> Asked {
        std::fs::write(Path::new(f.event["locator"].as_str().unwrap()).join(JOURNAL), [b'\n'; 100]).unwrap();
        // The ledger's clock is in milliseconds: what was written before is strictly earlier.
        std::thread::sleep(std::time::Duration::from_millis(3));
        Asked::at_launch(&f.coord, &f.state, std::sync::Arc::new(crate::turn_gate::TurnGate::at_launch(&f.state)))
    }

    /// One line of a journal, as the engine writes a record.
    fn written(f: &Fixture, record: Value) {
        use std::io::Write;
        let journal = Path::new(f.event["locator"].as_str().unwrap()).join(JOURNAL);
        let mut file = std::fs::OpenOptions::new().append(true).open(journal).unwrap();
        writeln!(file, "{record}").unwrap();
    }

    fn prompt(f: &Fixture, text: &str, source: crate::ledger::Source) {
        let mut ledger = crate::ledger::Ledger::open(f.root.join("conversation-ledger.jsonl")).unwrap();
        ledger.record_prompt_received(&f.binding, text, source).unwrap();
    }

    /// **Opening the app spends nothing** (CEO 2026-11-26): a Codex report for a conversation
    /// in which he has asked nothing since the app opened is not handed to
    /// Stu. It stays in its journal, unaccepted, and is taken at the first offer after his next
    /// message there. What he typed before the app opened, and an agent's message, open nothing.
    #[test]
    fn a_codex_report_nobody_waits_for_stays_in_its_journal_until_the_user_asks_in_that_chat() {
        let f = Fixture::new();
        prompt(&f, "Yesterday's request", crate::ledger::Source::Text);
        let asked = opened(&f);
        for _ in 0..2 {
            assert_eq!(
                accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(),
                WAITS_FOR_THE_USER
            );
        }
        prompt(&f, "{\"from\":\"stu\",\"text\":\"a report\"}", crate::ledger::Source::Agent);
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER,
            "an agent's message is not the user asking"
        );
        assert!(!session_tools::inbox(&f.scope(), "stu").unwrap().exists(), "a held report reached Stu's inbox");
        assert!(!f.state.join("codex-watch/accepted").exists(), "a held report was marked accepted");

        prompt(&f, "What did Codex say?", crate::ledger::Source::Text);
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        assert_eq!(f.rows().len(), 1, "the report was not handed over after he asked");
        assert!(f.state.join("codex-watch/accepted/pair.json").exists());
    }

    /// He asked in one chat: another chat's reports still wait.
    #[test]
    fn asking_in_one_chat_releases_no_other_chats_reports() {
        let f = Fixture::new();
        let asked = opened(&f);
        let mut ledger = crate::ledger::Ledger::open(f.root.join("conversation-ledger.jsonl")).unwrap();
        let other = ledger.create_thread("Another chat", &crate::EntityId::parse("company-a").unwrap()).unwrap();
        let other = ledger.thread_binding(&other).unwrap();
        ledger.record_prompt_received(&other, "Hello", crate::ledger::Source::Jam).unwrap();
        drop(ledger);
        assert!(asked.gate.may_start(&other, crate::turn_gate::Origin::FromBefore), "his spoken words were not read as his acting");
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
    }

    /// Codex's entries still reach Stu by themselves while the app is open: a report written
    /// after it opened is handed over at once, with no job open and no message from him. So is
    /// one on a connection made after it opened.
    #[test]
    fn a_report_written_while_the_app_is_open_reaches_stu_by_itself() {
        let f = Fixture::new();
        let asked = opened(&f);
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
        // Codex writes a second report on the same connection while the app is open. The
        // watcher offers a connection's reports one at a time, in order: the one from before is
        // set aside, still held, and the new one goes.
        written(&f, json!({"version":1,"message_id":"message-2","text":"A second report"}));
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        let mut later = f.event.clone();
        later["record"]["message_id"] = json!("message-2");
        later["through"] = json!(200);
        accept_when_asked(&f.state, &f.binding, &later, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["message-2"], "a report written after the app opened was held, or the one before it went");

        let new = f.coord.join("connections/company-a").join(f.binding.thread_id()).join("new");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join(JOURNAL), [b'\n'; 100]).unwrap();
        let mut fresh = f.event.clone();
        fresh["locator"] = json!(new);
        fresh["connection"]["connection_id"] = json!("new");
        fresh["record"]["message_id"] = json!("message-3");
        accept_when_asked(&f.state, &f.binding, &fresh, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["message-2", "message-3"], "a report on a connection made after the app opened was held");
    }

    /// **A fresh Codex report never releases the held one** (review
    /// rv-20261127T005026Z-12b7c7e4-015b, finding 1: "Fresh Codex report: held report delivered
    /// first; user messages: 0"). Once Codex writes behind the report from before, the newer
    /// report reaches Stu by itself and the one from before stays held: through a restart, and
    /// through more reports, until the user acts in that chat. It is then delivered, never
    /// dropped, and before anything the connection offers after that.
    #[test]
    fn a_fresh_codex_report_goes_alone_and_the_one_from_before_waits_for_the_user() {
        let f = Fixture::new();
        let asked = opened(&f);
        assert_eq!(accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(), WAITS_FOR_THE_USER);
        let report = |id: &str, through: u64| {
            written(&f, json!({"version":1,"message_id":id,"text":format!("Report {id}")}));
            let mut event = f.event.clone();
            event["record"] = json!({"version":1,"message_id":id,"text":format!("Report {id}")});
            event["through"] = json!(through);
            event
        };
        let second = report("message-2", 200);
        // The watcher offers the held report again, now with one behind it, then the new one.
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        assert!(f.delivered().is_empty(), "the report from before went to Stu with no message from the user");
        accept_when_asked(&f.state, &f.binding, &second, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["message-2"], "the fresh report did not reach Stu by itself");
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-2"], "the report from before went with no message from the user");

        // RichOS opens again: what was set aside is still from before.
        // load-bound: this only moves the millisecond clock past what was saved; host load makes the gap larger, never smaller.
        std::thread::sleep(std::time::Duration::from_millis(3));
        let asked = Asked::at_launch(&f.coord, &f.state, std::sync::Arc::new(crate::turn_gate::TurnGate::at_launch(&f.state)));
        f.release(&asked);
        let third = report("message-3", 10_000);
        accept_when_asked(&f.state, &f.binding, &third, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["message-2", "message-3"], "a reopening released the report from before, or held a fresh one");

        // He writes in that chat. The next report Codex writes waits behind the one from before,
        // even when it is offered before the app's release has run.
        prompt(&f, "What did Codex say?", crate::ledger::Source::Text);
        let fourth = report("message-4", 20_000);
        accept_when_asked(&f.state, &f.binding, &fourth, &f.coord, &asked).unwrap();
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-2", "message-3", "message-1", "message-4"]);
        assert!(!f.state.join(HELD).join("pair.jsonl").exists(), "a delivered report is still set aside");
    }

    /// **A crash in the middle of setting a report aside loses nothing** (review
    /// rv-20261127T020027Z-5e9ff3e9-1d7e, finding 3): the half-written line is cut, the report
    /// is saved whole when the watcher offers it again, and it goes once he acts.
    #[test]
    fn a_half_written_set_aside_line_is_cut_before_the_report_is_saved_again() {
        let f = Fixture::new();
        let asked = opened(&f);
        written(&f, json!({"version":1,"message_id":"message-2","text":"A second report"}));
        let held = f.state.join(HELD).join("pair.jsonl");
        std::fs::create_dir_all(held.parent().unwrap()).unwrap();
        std::fs::write(&held, &f.event.to_string().as_bytes()[..26]).unwrap();
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        assert_eq!(held_events(&held).unwrap(), std::slice::from_ref(&f.event), "the set-aside file is not one whole report");
        prompt(&f, "Hello", crate::ledger::Source::Text);
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-1"]);
    }

    /// **Set aside the moment he acts**: the app's release hands it to Stu with nothing more
    /// from Codex, once, and only for the chat he acted in.
    #[test]
    fn the_report_set_aside_goes_once_when_the_user_acts_in_its_chat_and_not_another() {
        let f = Fixture::new();
        let asked = opened(&f);
        written(&f, json!({"version":1,"message_id":"message-2","text":"A second report"}));
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        let mut ledger = crate::ledger::Ledger::open(f.root.join("conversation-ledger.jsonl")).unwrap();
        let other = ledger.create_thread("Another chat", &crate::EntityId::parse("company-a").unwrap()).unwrap();
        let other = ledger.thread_binding(&other).unwrap();
        ledger.record_prompt_received(&other, "Hello", crate::ledger::Source::Text).unwrap();
        drop(ledger);
        f.release(&asked);
        assert!(f.delivered().is_empty(), "his words in another chat released this one's report");
        prompt(&f, "Hello", crate::ledger::Source::Jam);
        f.release(&asked);
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-1"]);
    }

    /// **The app's own review watcher writes a notice at startup** about a report from while
    /// the app was closed. That notice is not Codex writing: the report stays held, and the
    /// notice starts no turn either. A report Codex then writes goes alone; the two from before
    /// go once the user acts in that chat, in their order.
    #[test]
    fn a_review_notice_written_at_startup_releases_nothing_and_starts_no_turn() {
        let f = Fixture::new();
        let asked = opened(&f);
        written(&f, json!({"version":1,"kind":"review_result","message_id":"review-1","reply_to":"message-1","text":"no review started"}));
        let mut notice = f.event.clone();
        notice["record"] = json!({"version":1,"kind":"review_result","message_id":"review-1","reply_to":"message-1","text":"no review started"});
        notice["through"] = json!(250);
        for event in [&f.event, &notice] {
            assert_eq!(
                accept_when_asked(&f.state, &f.binding, event, &f.coord, &asked).unwrap_err(),
                WAITS_FOR_THE_USER
            );
        }
        assert!(!session_tools::inbox(&f.scope(), "stu").unwrap().exists(), "a startup notice reached Stu's inbox");

        written(&f, json!({"version":1,"message_id":"message-2","text":"A second report"}));
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        accept_when_asked(&f.state, &f.binding, &notice, &f.coord, &asked).unwrap();
        let mut later = f.event.clone();
        later["record"]["message_id"] = json!("message-2");
        later["through"] = json!(300);
        accept_when_asked(&f.state, &f.binding, &later, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["message-2"]);
        prompt(&f, "What did Codex say?", crate::ledger::Source::Text);
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-2", "message-1", "review-1"]);
    }

    /// **The verdict on a report that reached Stu during this run reaches Stu by itself** (review
    /// rv-20261127T025408Z-0159fa0b-818a, finding 3: "Stu inbox = [\"fresh\"], held = [\"old\",
    /// \"verdict\"]"), as it did before the one rule. Only a verdict on the held report waits
    /// with it, and goes behind it once the user acts in that chat.
    #[test]
    fn a_verdict_on_a_fresh_report_reaches_stu_by_itself_and_one_on_the_held_report_waits_with_it() {
        let f = Fixture::new();
        let asked = opened(&f);
        let offered = |record: Value, through: u64| {
            written(&f, record.clone());
            let mut event = f.event.clone();
            event["record"] = record;
            event["through"] = json!(through);
            event
        };
        assert_eq!(accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(), WAITS_FOR_THE_USER);
        let fresh = offered(json!({"version":1,"message_id":"message-2","text":"READY TO LAND: codex/fresh"}), 200);
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        accept_when_asked(&f.state, &f.binding, &fresh, &f.coord, &asked).unwrap();
        let verdict = offered(json!({"version":1,"kind":"review_result","message_id":"review-2","reply_to":"message-2",
            "text":"SECOND REVIEW VERDICT\npassed"}), 300);
        accept_when_asked(&f.state, &f.binding, &verdict, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["message-2", "review-2"], "the verdict on a report Stu got during this run did not reach him by itself");

        let held_verdict = offered(json!({"version":1,"kind":"review_result","message_id":"review-1","reply_to":"message-1",
            "text":"SECOND REVIEW VERDICT\npassed"}), 400);
        accept_when_asked(&f.state, &f.binding, &held_verdict, &f.coord, &asked).unwrap();
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-2", "review-2"], "the verdict on the held report went with no message from the user");
        prompt(&f, "What did Codex say?", crate::ledger::Source::Text);
        f.release(&asked);
        assert_eq!(f.delivered(), ["message-2", "review-2", "message-1", "review-1"]);
    }

    /// **The review watcher is told how long each journal was when the app opened** (review
    /// rv-20261127T025408Z-0159fa0b-818a, finding 4): it reviews a READY from before only once
    /// this bridge has handed it to Stu (`review_watch.py` `AppWorld.held_by_the_bridge`, whose
    /// own test reads this record's shape).
    #[test]
    fn the_journals_lengths_at_launch_are_written_for_the_review_watcher() {
        let f = Fixture::new();
        let _asked = opened(&f);
        let record: Value = serde_json::from_slice(&std::fs::read(f.state.join(JOURNALS_AT_LAUNCH)).unwrap()).unwrap();
        let journal = Path::new(f.event["locator"].as_str().unwrap()).join(JOURNAL).canonicalize().unwrap();
        assert_eq!(record, json!({"version": 1, "journals": {journal.to_str().unwrap(): 100}}));
    }

    /// **The old Markdown channel.** A reply written while the app was closed starts no turn at
    /// launch; one written while it is open reaches Stu by itself, and the held one is set
    /// aside until the user acts in that chat. Each block names the lines it came from, as the
    /// engine's watcher writes it.
    #[test]
    fn a_markdown_reply_from_before_waits_and_one_written_while_open_arrives_by_itself() {
        let f = Fixture::new();
        let channel = f.root.join("rich-codex/to-rich.md");
        std::fs::create_dir_all(channel.parent().unwrap()).unwrap();
        std::fs::write(&channel, "## read long ago\nold\n## written while closed\nbacklog\n").unwrap();
        // The watcher had read the first two lines when the app last quit.
        let places = f.state.join(MARKDOWN_PLACES);
        std::fs::create_dir_all(places.parent().unwrap()).unwrap();
        std::fs::write(&places, json!({channel.to_str().unwrap(): {"offset": 21, "inode": 1, "seen": 21}}).to_string()).unwrap();
        let asked = opened(&f);
        let block = |lines: &str, body: &str, id: &str, through: u64| {
            let mut event = f.event.clone();
            event["record"] = json!({"version":1,"kind":"legacy_live","message_id":id,
                "text":format!("CODEX-WATCH 10:04Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines {lines} of {}); every new line follows:\n{body}", channel.display())});
            event["through"] = json!(through);
            event
        };
        let backlog = block("3-4", "## written while closed\nbacklog", "legacy-live-a", 5000);
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &backlog, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
        assert!(!session_tools::inbox(&f.scope(), "stu").unwrap().exists(), "a held reply reached Stu's inbox");

        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(&channel).unwrap().write_all(b"## written while open\nlive\n").unwrap();
        let live = block("5-6", "## written while open\nlive", "legacy-live-b", 6000);
        accept_when_asked(&f.state, &f.binding, &backlog, &f.coord, &asked).unwrap();
        accept_when_asked(&f.state, &f.binding, &live, &f.coord, &asked).unwrap();
        assert_eq!(f.delivered(), ["legacy-live-b"], "a reply written while the app is open was held, or the one before it went");
        prompt(&f, "Anything from Codex?", crate::ledger::Source::Text);
        f.release(&asked);
        assert_eq!(f.delivered(), ["legacy-live-b", "legacy-live-a"]);
    }

    /// The two default Markdown channels share one spool: a reply held on one never holds back
    /// a reply written on the other while the app is open.
    #[test]
    fn a_markdown_reply_held_on_one_channel_never_blocks_a_live_reply_on_the_other() {
        let f = Fixture::new();
        let (first, second) = (f.root.join("rich-codex/to-rich.md"), f.root.join("rich-codex-questions/to-rich.md"));
        for (channel, text) in [(&first, "## written while closed\nbacklog\n"), (&second, "## read long ago\nold\n")] {
            std::fs::create_dir_all(channel.parent().unwrap()).unwrap();
            std::fs::write(channel, text).unwrap();
        }
        let places = f.state.join(MARKDOWN_PLACES);
        std::fs::create_dir_all(places.parent().unwrap()).unwrap();
        std::fs::write(&places, json!({
            first.to_str().unwrap(): {"offset": 0, "inode": 1, "seen": 0},
            second.to_str().unwrap(): {"offset": 21, "inode": 2, "seen": 21},
        }).to_string()).unwrap();
        let asked = opened(&f);
        let mut backlog = f.event.clone();
        backlog["record"] = json!({"version":1,"kind":"legacy_live","message_id":"legacy-live-a",
            "text":format!("CODEX-WATCH 10:04Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 1-2 of {}); every new line follows:\n## written while closed\nbacklog", first.display())});
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &backlog, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(&second).unwrap().write_all(b"## a question, written while open\nlive\n").unwrap();
        accept_when_asked(&f.state, &f.binding, &backlog, &f.coord, &asked).unwrap();
        assert!(f.delivered().is_empty(), "the reply from before went to Stu with nothing asked");
        assert!(f.state.join("codex-watch/accepted/pair.live.json").exists(), "the held reply still blocks the other channel's live reply");
    }

    /// **A reply written while the app was closed whose last line has no newline is from
    /// before too** (review rv-20261126T112554Z-6b571438-ca3d, finding 1): the file's length
    /// at launch decides, not its newlines.
    #[test]
    fn a_markdown_reply_from_before_without_a_final_newline_waits() {
        let f = Fixture::new();
        let channel = f.root.join("rich-codex/to-rich.md");
        std::fs::create_dir_all(channel.parent().unwrap()).unwrap();
        std::fs::write(&channel, "# Existing channel\n## Offline without newline").unwrap();
        let places = f.state.join(MARKDOWN_PLACES);
        std::fs::create_dir_all(places.parent().unwrap()).unwrap();
        std::fs::write(&places, json!({channel.to_str().unwrap(): {"offset": 19, "inode": 1, "seen": 19}}).to_string()).unwrap();
        let asked = opened(&f);
        let mut backlog = f.event.clone();
        backlog["record"] = json!({"version":1,"kind":"legacy_live","message_id":"legacy-live-a",
            "text":format!("CODEX-WATCH 10:04Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 2-2 of {}); every new line follows:\n## Offline without newline", channel.display())});
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &backlog, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
        assert!(!session_tools::inbox(&f.scope(), "stu").unwrap().exists(), "a reply from before reached Stu's inbox");
    }

    /// With nothing waiting at launch, the first Markdown reply written while the app is open
    /// arrives by itself, hours later or not.
    #[test]
    fn the_first_markdown_reply_written_while_open_is_never_held() {
        let f = Fixture::new();
        let channel = f.root.join("rich-codex/to-rich.md");
        std::fs::create_dir_all(channel.parent().unwrap()).unwrap();
        std::fs::write(&channel, "## read long ago\nold\n").unwrap();
        let places = f.state.join(MARKDOWN_PLACES);
        std::fs::create_dir_all(places.parent().unwrap()).unwrap();
        std::fs::write(&places, json!({channel.to_str().unwrap(): {"offset": 21, "inode": 1, "seen": 21}}).to_string()).unwrap();
        let asked = opened(&f);
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(&channel).unwrap().write_all(b"## written while open\nlive\n").unwrap();
        let mut live = f.event.clone();
        live["record"] = json!({"version":1,"kind":"legacy_live","message_id":"legacy-live-b",
            "text":format!("CODEX-WATCH 13:00Z: Codex wrote a new entry to you in rich-codex/to-rich.md (lines 3-4 of {}); every new line follows:\n## written while open\nlive", channel.display())});
        accept_when_asked(&f.state, &f.binding, &live, &f.coord, &asked).unwrap();
        assert_eq!(f.rows().len(), 1);
    }

    /// A job record that says the job is open (registered, running, paused for quota, waiting
    /// on him) is what a quit leaves behind: it is never rewritten, so it opens nothing. The
    /// report waits (review rv-20261126T140329Z-15e71cf5-6caa, finding 1).
    #[test]
    fn a_job_record_that_still_says_open_releases_nothing_from_before() {
        let f = Fixture::new();
        let asked = opened(&f);
        let scope = f.scope();
        crate::assignment::register(&f.state, &crate::assignment::Registration {
            entity_id: scope.entity_id.clone(),
            thread_id: scope.thread_id.clone(),
            obligation_id: "obligation-1".into(),
            instruction_ledger_ref: format!("ledger:{}:turn-1", scope.thread_id),
            instruction_sha256: "0".repeat(64),
            title: "the Desktop fix".into(),
            repositories: Vec::new(),
            needs_screen: false,
        })
        .unwrap();
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
        assert!(!session_tools::inbox(&scope, "stu").unwrap().exists(), "an open job record released a report from before");
    }

    /// The same report, the job ended: it waits.
    #[test]
    fn a_chat_whose_job_has_ended_waits_like_one_that_never_had_a_job() {
        let f = Fixture::new();
        let asked = opened(&f);
        let scope = f.scope();
        let job = crate::assignment::register(&f.state, &crate::assignment::Registration {
            entity_id: scope.entity_id.clone(),
            thread_id: scope.thread_id.clone(),
            obligation_id: "obligation-1".into(),
            instruction_ledger_ref: format!("ledger:{}:turn-1", scope.thread_id),
            instruction_sha256: "0".repeat(64),
            title: "the Desktop fix".into(),
            repositories: Vec::new(),
            needs_screen: false,
        })
        .unwrap();
        crate::assignment::advance(&f.state, &scope.entity_id, &scope.thread_id, &job.id,
            crate::assignment::AssignmentState::Settled, "done").unwrap();
        assert_eq!(
            accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap_err(),
            WAITS_FOR_THE_USER
        );
        assert!(!session_tools::inbox(&scope, "stu").unwrap().exists());
    }

    #[test]
    fn codex_bad_binding_or_missing_metadata_never_creates_an_inbox() {
        let f = Fixture::new();
        let mut wrong = f.event.clone();
        wrong["connection"]["entity_id"] = json!("company-b");
        assert!(accept(&f.state, &f.binding, &wrong, &f.coord).is_err());
        let mut missing = f.event.clone();
        missing.as_object_mut().unwrap().remove("through");
        assert!(accept(&f.state, &f.binding, &missing, &f.coord).is_err());
        assert!(!session_tools::inbox(&f.scope(), "stu").unwrap().exists());
    }

    #[test]
    fn old_engine_events_never_stage_history_or_move_a_live_cursor() {
        let f = Fixture::new();
        let mut event = f.event.clone();
        event["record"]["kind"] = json!("legacy_live");
        accept(&f.state, &f.binding, &event, &f.coord).unwrap();
        let position = f.state.join("codex-watch/accepted/pair.live.json");
        let saved = std::fs::read(&position).unwrap();
        for kind in ["legacy_import", "legacy_notice"] {
            event["record"]["kind"] = json!(kind);
            event["record"]["message_id"] = json!(kind);
            event["through"] = json!(400);
            accept(&f.state, &f.binding, &event, &f.coord).unwrap();
        }
        assert_eq!(f.rows().len(), 1);
        assert_eq!(std::fs::read(position).unwrap(), saved);
        assert!(!f.state.join("codex-watch/accepted/pair.json").exists());
    }

    #[test]
    fn upgrade_skips_archived_inbox_rows_and_keeps_new_input() {
        let f = Fixture::new();
        let scope = f.scope();
        let inbox = session_tools::inbox(&scope, "stu").unwrap();
        let mut reference = session_tools::BridgeReference {
            connection_id: "legacy-channel".into(),
            message_id: format!("legacy-{}", "a".repeat(64)),
            payload_digest: "digest".into(),
            locator: "locator".into(),
            reply_to: None,
        };
        let (old, _) = session_tools::observe_channel(&scope, "Old archive row", reference.clone()).unwrap();
        session_tools::accept_live_handoff(&scope, "stu", old).unwrap();
        reference.message_id = "legacy-outbox".into();
        session_tools::observe_channel(&scope, "Old notice", reference.clone()).unwrap();
        assert!(!session_tools::undelivered(&scope, "stu").unwrap());
        assert!(session_tools::pending(&scope, "stu").unwrap().is_none());
        reference.message_id = format!("legacy-live-{}", "b".repeat(64));
        session_tools::observe_channel(&scope, "New Markdown append", reference).unwrap();
        accept(&f.state, &f.binding, &f.event, &f.coord).unwrap();
        for text in ["New Markdown append", "Original request"] {
            let (message, through) = session_tools::pending(&scope, "stu").unwrap().unwrap();
            assert_eq!(message.text, text);
            session_tools::acknowledge(&scope, "stu", through).unwrap();
        }
        assert_eq!(std::fs::read_to_string(inbox).unwrap().lines().count(), 4);
    }

    #[test]
    fn codex_review_observations_have_their_actual_source_and_no_user_authority() {
        let f = Fixture::new();
        let mut result = f.event.clone();
        result["record"]["kind"] = json!("review_result");
        accept(&f.state, &f.binding, &result, &f.coord).unwrap();
        let rows = f.rows();
        assert_eq!(rows[0].from, "review-watch");
        assert!(rows[0].instruction.is_none());
        assert!(!rows[0].handoff);
    }
    #[test]
    fn codex_complete_unsynced_inbox_frame_is_held_until_the_writer_releases_it() {
        use std::os::fd::AsRawFd;
        use std::sync::mpsc;
        for retained in [false, true] {
            let f = Fixture::new();
            let path = session_tools::inbox(&f.scope(), "stu").unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path.with_extension("cursor"),
                json!({"through":0,"handoffs_retained":retained}).to_string(),
            )
            .unwrap();
            let mut writer = std::fs::OpenOptions::new()
                .create_new(true)
                .append(true)
                .read(true)
                .open(&path)
                .unwrap();
            assert_eq!(unsafe { libc::flock(writer.as_raw_fd(), libc::LOCK_EX) }, 0);
            let record = session_tools::Message {
                handoff: false,
                from: "codex-watch".into(),
                text: "Original request".into(),
                instruction: None,
                work: None,
                bridge: Some(session_tools::BridgeReference {
                    connection_id: "pair".into(),
                    message_id: "message-1".into(),
                    payload_digest: digest(&f.event["record"]),
                    locator: f.event["locator"].as_str().unwrap().into(),
                    reply_to: None,
                }),
            };
            writer
                .write_all(format!("{}\n", serde_json::to_string(&record).unwrap()).as_bytes())
                .unwrap();
            // It looks complete but the producer still holds its unsynced write.
            let state = f.state.clone();
            let binding = f.binding.clone();
            let event = f.event.clone();
            let coord = f.coord.clone();
            let scope = f.scope();
            let (tx, rx) = mpsc::channel();
            let reader = std::thread::spawn(move || {
                let accepted = accept(&state, &binding, &event, &coord);
                let pending = session_tools::pending(&scope, "stu");
                tx.send((accepted, pending)).unwrap();
            });
            // load-bound: hang guard for an OS-lock admission answer; assertions check held results, never measured speed. Our fixture lock is always released before joining.
            let result = rx.recv_timeout(Duration::from_secs(10));
            // A concurrently forked test can briefly inherit the descriptor. Unlock the
            // shared file description explicitly before checking the post-release read.
            let unlocked = unsafe { libc::flock(writer.as_raw_fd(), libc::LOCK_UN) };
            drop(writer);
            reader.join().unwrap();
            assert_eq!(unlocked, 0);
            let (accepted, pending) = result.expect("one busy inbox blocked the receiver");
            assert!(accepted.is_err());
            assert!(pending.is_err());
            accept(&f.state, &f.binding, &f.event, &f.coord).unwrap();
            assert_eq!(f.rows().len(), 1);
            assert_eq!(
                session_tools::pending(&f.scope(), "stu")
                    .unwrap()
                    .unwrap()
                    .0
                    .text,
                "Original request"
            );
        }
    }

    #[test]
    fn codex_crash_probe_child() {
        let Ok(root) = std::env::var("RICHOS_CHANNEL_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let event: Value =
            serde_json::from_slice(&std::fs::read(root.join("event.json")).unwrap()).unwrap();
        let ledger = crate::ledger::Ledger::open(root.join("conversation-ledger.jsonl")).unwrap();
        let binding = ledger
            .thread_binding(event["connection"]["thread_id"].as_str().unwrap())
            .unwrap();
        accept(&root.join("state"), &binding, &event, &root.join("coord")).unwrap();
        panic!("declared crash boundary was never reached");
    }

    #[test]
    fn codex_receiver_killed_at_real_persistence_boundaries_replays_one_inbox_row() {
        use std::os::unix::process::ExitStatusExt;
        for legacy in [false, true] {
            for stage in [
                "before-inbox-write",
                "before-inbox-sync",
                "after-inbox-sync",
                "before-position-write",
                "before-position-sync",
                "after-position-sync",
                "after-position-rename",
                "after-position-directory-sync",
            ] {
                let mut f = Fixture::new();
                if legacy {
                    f.event["record"]["kind"] = json!("legacy_live");
                }
                std::fs::write(f.root.join("event.json"), f.event.to_string()).unwrap();
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "codex_bridge::tests::codex_crash_probe_child",
                        "--nocapture",
                    ])
                    .env("RICHOS_CHANNEL_TEST_ROOT", &f.root)
                    .env("RICHOS_CHANNEL_TEST_CRASH", stage)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .unwrap();
                assert_eq!(
                    status.signal(),
                    Some(libc::SIGKILL),
                    "{stage}: probe did not die at its actual boundary"
                );
                accept(&f.state, &f.binding, &f.event, &f.coord).unwrap();
                accept(&f.state, &f.binding, &f.event, &f.coord).unwrap();
                assert_eq!(
                    f.rows().len(),
                    1,
                    "{stage}: lost or duplicated accepted input (legacy={legacy})"
                );
            }
        }
    }
}
