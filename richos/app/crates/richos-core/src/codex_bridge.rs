//! Native channel boundary: host scope, existing inbox acceptance, then position.
//! Channel framing, discovery and writes live exclusively in the shared engine.
use crate::{session_tools, status_tools::StatusToolScope};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

pub const TOOL: &str = "codex_channel";
pub const SETUP: &str = "Codex coordination is waiting. In the chat you coordinate from, tell Rich to take over the Codex channel.";

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

pub fn setup_notice(state: &Path, binding: &crate::entity::ThreadBinding) -> Result<(), String> {
    let scope = StatusToolScope {
        version: 1,
        state_root: state.into(),
        entity_id: binding.entity_id().to_string(),
        thread_id: binding.thread_id().into(),
    };
    session_tools::observe_channel(
        &scope,
        SETUP,
        session_tools::BridgeReference {
            connection_id: "setup".into(),
            message_id: "coordinator-required".into(),
            payload_digest: digest(&json!(SETUP)),
            locator: String::new(),
            reply_to: None,
        },
    )
    .map(|_| ())
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
    let locator = coordination
        .join("connections")
        .join(entity)
        .join(thread)
        .join(id);
    if event["locator"].as_str() != locator.to_str()
        || locator.canonicalize().map_err(|e| e.to_string())? != locator
    {
        return Err("Channel is outside its registered location".into());
    }
    let through = event["through"]
        .as_u64()
        .ok_or("Missing accepted position")?;
    let identity = event["source_identity"]
        .as_str()
        .ok_or("Missing source identity")?;
    let record = &event["record"];
    let scope = StatusToolScope {
        version: 1,
        state_root: state.into(),
        entity_id: entity.into(),
        thread_id: thread.into(),
    };
    if record["kind"] != "task_context" {
        let text = record["text"]
            .as_str()
            .ok_or("Channel record has no text")?;
        let message_id = record["message_id"]
            .as_str()
            .ok_or("Channel record has no ID")?;
        component(message_id)?;
        let legacy = record["kind"] == "legacy_import";
        if !legacy || !session_tools::legacy_channel_saved(state, text)? {
            let text = if legacy {
                format!("Pre-cutover legacy observation; verify the complete original request before acting.\n{text}")
            } else {
                text.into()
            };
            session_tools::observe_channel_from(
                &scope,
                &text,
                session_tools::BridgeReference {
                    connection_id: id.into(),
                    message_id: message_id.into(),
                    payload_digest: digest(record),
                    locator: locator.to_string_lossy().into_owned(),
                    reply_to: record["reply_to"].as_str().map(str::to_string),
                },
                if record["kind"] == "review_result" {
                    "review-watch"
                } else {
                    "codex-watch"
                },
            )?;
        }
    }
    // Only reached after the exact inbox row was synced (or non-actionable context).
    // This fixed notice is deduplicated by its inbox reference, independently of
    // the preserved spool's position. Raw growth never moves the spool cursor.
    if record["kind"] == "legacy_notice" {
        return Ok(());
    }
    let path = state
        .join("codex-watch/accepted")
        .join(format!("{id}.json"));
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
    if !matches!(action, "designate" | "connect" | "send" | "read") {
        return Err("Unknown channel action".into());
    }
    if action == "designate" && (sender.role != "rich" || sender.instruction.is_none()) {
        return Err(
            "Ask Rich to designate the coordinator in the user's requested conversation".into(),
        );
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
    if action == "designate" {
        request.insert(
            "legacy".into(),
            serde_json::to_value(crate::codex_watch::installed_channel_folders())
                .map_err(|e| e.to_string())?,
        );
    }

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
            let mut ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
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
    fn codex_setup_and_legacy_notices_deduplicate_without_advancing_spool() {
        let f = Fixture::new();
        setup_notice(&f.state, &f.binding).unwrap();
        setup_notice(&f.state, &f.binding).unwrap();
        assert_eq!(f.rows().len(), 1);
        let mut notice = f.event.clone();
        notice["record"]["kind"] = json!("legacy_notice");
        accept(&f.state, &f.binding, &notice, &f.coord).unwrap();
        accept(&f.state, &f.binding, &notice, &f.coord).unwrap();
        assert_eq!(f.rows().len(), 2);
        assert!(!f.state.join("codex-watch/accepted/pair.json").exists());
        let mut imported = f.event.clone();
        imported["record"]["kind"] = json!("legacy_import");
        imported["record"]["message_id"] = json!("import-1");
        // A pre-cutover save in a different conversation is existing history.
        let old = StatusToolScope {
            thread_id: "archived-chat".into(),
            ..f.scope()
        };
        session_tools::observe_codex(&old, "Original request").unwrap();
        accept(&f.state, &f.binding, &imported, &f.coord).unwrap();
        assert_eq!(f.rows().len(), 2);
        assert!(f.state.join("codex-watch/accepted/pair.json").exists());
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
            drop(writer); // always release our owned lock before asserting/joining
            reader.join().unwrap();
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
        let ledger = crate::ledger::Ledger::open(root.join("ledger.jsonl")).unwrap();
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
                    f.event["record"]["kind"] = json!("legacy_import");
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
