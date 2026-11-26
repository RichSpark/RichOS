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
    if kind != Some("task_context") {
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
    // Only reached after the exact inbox row was synced (or non-actionable context).
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

/// **What the app knew when it opened, and which conversations the user has asked something in
/// since**, read from the conversation ledger: a `PromptReceived` row written after that whose source is
/// his own words (`text` or `jam`), by whichever road they came (the composer, the phone,
/// voice). An agent's message, a re-prime and Rich speaking unprompted are not his asking.
///
/// The first look reads the ledger once; every later one reads only what was appended since, so
/// a look costs nothing while nothing was written. Once a conversation is in, it stays in.
pub struct Asked {
    since_ms: u64,
    read: std::sync::Mutex<(u64, std::collections::HashSet<String>)>,
    /// How long each connection's journal was when the app opened ([`Self::nobody_wrote_since`]).
    journals: std::collections::HashMap<PathBuf, u64>,
    /// The same for each old Markdown channel file the watcher reads, and, once asked for, how
    /// many lines that length holds.
    markdown: std::collections::HashMap<PathBuf, u64>,
    markdown_lines: std::sync::Mutex<std::collections::HashMap<PathBuf, u64>>,
}

/// Where the app's watcher keeps its place in each old Markdown channel file
/// (`codex_watch.py` `_legacy_tick`, the app host's session directory).
const MARKDOWN_PLACES: &str = "codex-watch/watch/app-host/live-offsets.json";

impl Asked {
    /// **The app is opening now.** Neither the journals nor the Markdown channels carry a
    /// clock, so how long each file is at this moment is what tells a report from before from
    /// one written while the app is open: a few dozen `stat` calls, once. `state` is the app's
    /// engine state folder.
    pub fn at_launch(coordination: &Path, state: &Path) -> Self {
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
        Self { since_ms: crate::util::now_millis(), read: std::sync::Mutex::new((0, Default::default())), journals,
            markdown, markdown_lines: Default::default() }
    }

    /// **Is this a report nobody has followed up since the app opened?**
    ///
    /// A report that ends within its journal's length at launch was written while the app was
    /// closed. So was, in effect, a notice the app's own review watcher writes about one at
    /// startup (`review_result`): neither starts a turn. Both wait only while Codex has written
    /// nothing on that connection since launch: the watcher offers a connection's reports one
    /// at a time, in order, so a report that stayed held would hold back every report Codex
    /// writes there while the app is open. Once Codex writes one, what was held is handed over
    /// ahead of it. A journal that did not exist at launch holds nothing from before.
    ///
    /// A reply on an old Markdown channel (`legacy_live`) names the lines it came from, and it
    /// is from before when it starts within the file's length at launch. It waits only while
    /// that file has not grown, for the same reason.
    fn nobody_wrote_since(&self, event: &Value) -> bool {
        let kind = event["record"]["kind"].as_str();
        match kind {
            Some("legacy_live") => return self.markdown_reply_from_before(event["record"]["text"].as_str().unwrap_or_default()),
            Some(kind) if kind.starts_with("legacy_") => return false,
            _ => {}
        }
        let Some(journal) = event["locator"].as_str().and_then(|locator| Path::new(locator).join(JOURNAL).canonicalize().ok())
        else { return false };
        let (Some(length), Some(through)) = (self.journals.get(&journal), event["through"].as_u64()) else { return false };
        (through <= *length || kind == Some("review_result")) && !codex_wrote_after(&journal, *length)
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
        if std::fs::metadata(&file).is_ok_and(|now| now.len() > *length) {
            return false;
        }
        let mut lines = self.markdown_lines.lock().unwrap();
        let before = *lines.entry(file.clone()).or_insert_with(|| {
            use std::io::Read;
            let mut bytes = Vec::new();
            match std::fs::File::open(&file).map(|opened| opened.take(*length).read_to_end(&mut bytes)) {
                Ok(Ok(_)) => bytes.iter().filter(|byte| **byte == b'\n').count() as u64,
                _ => u64::MAX,
            }
        });
        first <= before
    }

    /// An unreadable ledger answers no: the report waits and stays in its journal.
    pub fn in_thread(&self, ledger: &Path, thread: &str) -> bool {
        use std::io::{BufRead, Seek, SeekFrom};
        let mut read = self.read.lock().unwrap();
        if read.1.contains(thread) {
            return true;
        }
        let Ok(mut file) = std::fs::File::open(ledger) else { return false };
        let length = file.metadata().map(|file| file.len()).unwrap_or(0);
        // A ledger shorter than what was read was rewritten: read it again from its start.
        if length < read.0 {
            read.0 = 0;
        }
        if length == read.0 || file.seek(SeekFrom::Start(read.0)).is_err() {
            return false;
        }
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        loop {
            line.clear();
            // A concurrent append may not have completed its last record yet.
            match reader.read_line(&mut line) {
                Ok(count) if count > 0 && line.ends_with('\n') => read.0 += count as u64,
                _ => break,
            }
            if !line.contains("PromptReceived") {
                continue;
            }
            let Ok(row) = serde_json::from_str::<Value>(&line) else { continue };
            if row["event"] == "PromptReceived"
                && matches!(row["source"].as_str(), Some("text" | "jam"))
                && row["at"].as_u64().is_some_and(|at| at >= self.since_ms)
            {
                if let Some(asked) = row["thread_id"].as_str() {
                    read.1.insert(asked.to_string());
                }
            }
        }
        read.1.contains(thread)
    }
}

/// The file in a connection's folder Codex and the review watcher write to Rich's team in.
const JOURNAL: &str = "to-rich.jsonl";

/// Has Codex written a report in this journal past `length`? A review watcher's notice
/// (`review_result`) is the app's own writing, not Codex's.
fn codex_wrote_after(journal: &Path, length: u64) -> bool {
    use std::io::{BufRead, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(journal) else { return false };
    if file.seek(SeekFrom::Start(length)).is_err() {
        return false;
    }
    std::io::BufReader::new(file).lines().map_while(Result::ok).any(|line| {
        serde_json::from_str::<Value>(&line).is_ok_and(|record| record.is_object() && record["kind"] != "review_result")
    })
}

/// What the watcher's log says once for a report that waits ([`accept_when_asked`]).
pub const WAITS_FOR_THE_USER: &str =
    "nothing newer was written since RichOS opened, for a conversation with no open job in which nothing was asked since; the report stays in its journal";

/// Does this conversation have a job that has not ended? A record that cannot be read is not
/// counted: a report that waits can still be given later, a model turn cannot be taken back.
fn has_open_job(state: &Path, binding: &crate::entity::ThreadBinding) -> bool {
    crate::assignment::scan_all(state, binding.entity_id().as_str(), binding.thread_id())
        .is_ok_and(|scan| scan.rows.iter().any(|job| job.reported_state().is_some_and(|state| state.is_open())))
}

/// **[`accept`], unless the report is from before the app opened and nobody is waiting for it**
/// (CEO 2026-11-26: "Everything was either stopped, finished, ended or paused in the RichOS
/// app. [...] there should have been absolutely zero usage after me just opening the app.").
///
/// Every accepted report is a turn of Stu's, on his whole session, and usually one of Rich's
/// after it. On 2026-11-26 the app opened after two days closed, with no job open, and the
/// watcher handed Stu six Codex and review reports written meanwhile: 23 requests and 5.19
/// million tokens on Fable before the user had typed a word. So a report that was already in
/// its journal when the app opened, for a conversation with no open job, in which the user has
/// asked nothing since, is refused here and stays in that journal, where it already waited
/// while the app was closed. The watcher offers it again at every scan, and the first offer
/// after the user's next message in that conversation accepts it, in order. `coordination` is
/// the folder `asked` was made from.
///
/// Nothing else changes for a connection's journal: a report written while the app is open
/// reaches Stu by itself, with the held ones before it, and so does one from before for a
/// conversation whose job has not ended. A reply on an old Markdown channel is held and handed
/// over by the same rule ([`Asked::nobody_wrote_since`]).
pub fn accept_when_asked(
    state: &Path,
    binding: &crate::entity::ThreadBinding,
    event: &Value,
    coordination: &Path,
    asked: &Asked,
) -> Result<(), String> {
    // The app's conversation ledger sits beside its engine state (`work_host::verified_instruction`).
    let ledger = state.parent().unwrap_or(state).join("conversation-ledger.jsonl");
    if asked.nobody_wrote_since(event) && !asked.in_thread(&ledger, binding.thread_id()) && !has_open_job(state, binding) {
        return Err(WAITS_FOR_THE_USER.into());
    }
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
        Asked::at_launch(&f.coord, &f.state)
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
    /// with no open job, in which he has asked nothing since the app opened, is not handed to
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
        assert!(asked.in_thread(&f.root.join("conversation-ledger.jsonl"), other.thread_id()), "his spoken words were not read as asking");
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
        // handed over now, and the new one after it.
        written(&f, json!({"version":1,"message_id":"message-2","text":"A second report"}));
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        let mut later = f.event.clone();
        later["record"]["message_id"] = json!("message-2");
        later["through"] = json!(200);
        accept_when_asked(&f.state, &f.binding, &later, &f.coord, &asked).unwrap();
        assert_eq!(f.rows().len(), 2, "a report written after the app opened, or the one before it, was held");

        let new = f.coord.join("connections/company-a").join(f.binding.thread_id()).join("new");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join(JOURNAL), [b'\n'; 100]).unwrap();
        let mut fresh = f.event.clone();
        fresh["locator"] = json!(new);
        fresh["connection"]["connection_id"] = json!("new");
        fresh["record"]["message_id"] = json!("message-3");
        accept_when_asked(&f.state, &f.binding, &fresh, &f.coord, &asked).unwrap();
        assert_eq!(f.rows().len(), 3, "a report on a connection made after the app opened was held");
    }

    /// **The app's own review watcher writes a notice at startup** about a report from while
    /// the app was closed. That notice is not Codex writing: the report stays held, and the
    /// notice starts no turn either. A report Codex then writes hands all three over, in order.
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
        assert_eq!(f.rows().len(), 2);
    }

    /// **The old Markdown channel.** A reply written while the app was closed starts no turn at
    /// launch; one written while it is open reaches Stu by itself, and the held one goes ahead
    /// of it. Each block names the lines it came from, as the engine's watcher writes it.
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
        assert_eq!(f.rows().len(), 2, "a reply written while the app is open, or the one before it, was held");
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

    /// A job of his that has not ended is waiting for its reports: one from before the app
    /// opened arrives at once, as before, with no message from him.
    #[test]
    fn a_chat_with_an_open_job_gets_its_codex_reports_at_once() {
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
        accept_when_asked(&f.state, &f.binding, &f.event, &f.coord, &asked).unwrap();
        assert_eq!(f.rows().len(), 1);
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
