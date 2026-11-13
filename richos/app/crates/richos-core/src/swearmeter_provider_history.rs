//! External input history is read locally. A numeric projection is all the app keeps.
use crate::{swearmeter, swearmeter_claude_history, swearmeter_periods::Dates};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
};

#[derive(Default, Debug)]
pub struct History {
    pub count: u64,
    pub dates: Dates,
    pub incomplete: bool,
    pub ambiguous_matches: u64,
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct Submission {
    at: u64,
    text_digest: [u8; 32],
    pub(crate) count: u64,
    pub(crate) session: Option<String>,
}

impl Submission {
    pub(crate) fn new(at: u64, text: &str) -> Self {
        let text = text.trim();
        Self {
            at,
            text_digest: Sha256::digest(text.as_bytes()).into(),
            count: swearmeter::count(text),
            session: None,
        }
    }
}

pub fn read(claude: Option<&Path>, codex: Option<&Path>) -> History {
    Reader::default().read_through(claude, codex, crate::util::now_millis())
}

pub fn read_through(claude: Option<&Path>, codex: Option<&Path>, through: u64) -> History {
    Reader::default().read_through(claude, codex, through)
}

#[derive(Default)]
pub struct Reader {
    transcripts: HashMap<PathBuf, Transcript>,
    rollouts: crate::swearmeter_codex_history::Rollouts,
}

struct Transcript {
    stamp: crate::swearmeter_checkpoint::SourceStamp,
    complete: bool,
    rows: Vec<(String, Submission)>,
}

impl Reader {
    pub fn read_through(
        &mut self,
        claude: Option<&Path>,
        codex: Option<&Path>,
        through: u64,
    ) -> History {
        let mut result = History::default();
        let mut claude_rows =
            claude.and_then(
                |root| match swearmeter_claude_history::read_submissions(root) {
                    Ok(Some((summary, rows))) => {
                        result.incomplete |=
                            summary.unreadable_records != 0 || summary.incomplete_submissions != 0;
                        Some(rows)
                    }
                    Ok(None) => None,
                    Err(_) => {
                        result.incomplete = true;
                        None
                    }
                },
            );
        if let Some(root) = claude {
            let rows = claude_rows.get_or_insert_with(Vec::new);
            result.incomplete |= !self.read_claude_transcripts(root, rows, through);
        }
        let mut codex_rows = HashSet::new();
        if let Some(root) = codex {
            let mut indexed = HashSet::new();
            match read_codex(root, &mut codex_rows, &mut indexed) {
                Ok(complete) => result.incomplete |= !complete,
                Err(_) => result.incomplete = true,
            }
            result.incomplete |= !self.rollouts.read(root, &indexed, &mut codex_rows);
        }
        let mut codex_times = HashMap::<[u8; 32], Vec<u64>>::new();
        for row in codex_rows.into_iter().filter(|row| row.at <= through) {
            if row.count > 0 {
                codex_times.entry(row.text_digest).or_default().push(row.at);
            }
            result.dates.add(Some(row.at), row.count);
            result.count += row.count;
        }
        // Time and text do not prove a shared submission across two independent apps.
        // Use the lower subtotal and disclose the withheld matches. A shared ID is
        // needed before either representation can be declared the original.
        for row in claude_rows
            .into_iter()
            .flatten()
            .filter(|row| row.at <= through)
        {
            if row.count != 0
                && codex_times
                    .get(&row.text_digest)
                    .is_some_and(|times| times.iter().any(|at| at.abs_diff(row.at) < 120_000))
            {
                result.ambiguous_matches += row.count;
                result.incomplete = true;
                continue;
            }
            result.dates.add(Some(row.at), row.count);
            result.count += row.count;
        }
        result
    }

    fn read_claude_transcripts(
        &mut self,
        root: &Path,
        submissions: &mut Vec<Submission>,
        through: u64,
    ) -> bool {
        use std::fs;
        let mut complete = true;
        let projects = root.join("projects");
        if !projects.is_dir() {
            return !projects.exists();
        }
        let mut directories = vec![projects];
        let mut seen = HashSet::new();
        let mut visited = HashSet::new();
        let mut recall = HashMap::<&str, Vec<(&Submission, bool)>>::new();
        for row in submissions.iter().filter(|row| row.at <= through) {
            if let Some(session) = row.session.as_deref() {
                recall.entry(session).or_default().push((row, false));
            }
        }
        let mut additional = Vec::new();
        while let Some(directory) = directories.pop() {
            let Ok(entries) = fs::read_dir(directory) else {
                complete = false;
                continue;
            };
            for entry in entries {
                let Ok(entry) = entry else {
                    complete = false;
                    continue;
                };
                let Ok(kind) = entry.file_type() else {
                    complete = false;
                    continue;
                };
                let path = entry.path();
                if kind.is_dir()
                    && !matches!(
                        entry.file_name().to_str(),
                        Some("subagents" | "tool-results")
                    )
                {
                    directories.push(path);
                } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
                    let Ok(Some(stamp)) = crate::swearmeter_checkpoint::SourceStamp::at(&path)
                    else {
                        complete = false;
                        continue;
                    };
                    visited.insert(path.clone());
                    if self
                        .transcripts
                        .get(&path)
                        .is_none_or(|entry| entry.stamp != stamp)
                    {
                        match parse_transcript(&path, stamp) {
                            Ok(transcript) => {
                                self.transcripts.insert(path.clone(), transcript);
                            }
                            Err(_) => {
                                complete = false;
                                continue;
                            }
                        }
                    }
                    let transcript = &self.transcripts[&path];
                    complete &= transcript.complete;
                    for (uuid, row) in &transcript.rows {
                        let at = row.at;
                        if at > through || !seen.insert(uuid.clone()) {
                            continue;
                        }
                        let session = row.session.as_deref().unwrap_or_default();
                        if let Some(candidates) = recall.get_mut(session) {
                            if let Some((_, matched)) = candidates
                                .iter_mut()
                                .filter(|(prior, matched)| {
                                    !*matched
                                        && prior.at.abs_diff(at) < 120_000
                                        && prior.text_digest == row.text_digest
                                })
                                .min_by_key(|(prior, _)| prior.at.abs_diff(at))
                            {
                                *matched = true;
                                continue;
                            }
                            // The recall file can differ from the provider frame (attachments,
                            // slash-command expansion). A nearby row is not a second submission.
                            if candidates.iter().any(|(prior, _)| {
                                prior.at.abs_diff(at) < 120_000
                                    && prior.text_digest != row.text_digest
                            }) {
                                complete &= row.count == 0;
                                continue;
                            }
                        }
                        additional.push(row.clone());
                    }
                }
            }
        }
        self.transcripts.retain(|path, _| visited.contains(path));
        submissions.extend(additional);
        complete
    }
}

fn parse_transcript(
    path: &Path,
    stamp: crate::swearmeter_checkpoint::SourceStamp,
) -> io::Result<Transcript> {
    use std::io::BufReader;
    let file = swearmeter_claude_history::open_regular(path)?;
    let mut complete = true;
    let mut rows = Vec::new();
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        match swearmeter_claude_history::read_record(&mut reader, &mut line) {
            Ok(0) => break,
            Ok(_) if line.is_empty() => {
                complete = false;
                continue;
            }
            Ok(_) => {}
            Err(_) => {
                complete = false;
                break;
            }
        }
        // Most lines are assistant or tool frames and need no decoding.
        if !line
            .windows(b"\"user\"".len())
            .any(|part| part == b"\"user\"")
        {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(&line) else {
            complete = false;
            continue;
        };
        if record["type"] != "user"
            || record["isMeta"] == true
            || record["isCompactSummary"] == true
            || record["isSidechain"] == true
            || record.get("toolUseResult").is_some()
        {
            continue;
        }
        if record["origin"]["kind"] != "human" || record["promptSource"] == "sdk" {
            if record["origin"].is_null() && record["promptSource"] == "sdk" {
                complete = false;
            }
            continue;
        }
        let Some(uuid) = record["uuid"].as_str() else {
            complete = false;
            continue;
        };
        let Some(session) = record["sessionId"].as_str() else {
            complete = false;
            continue;
        };
        let Some(at) = record["timestamp"]
            .as_str()
            .and_then(|stamp| {
                time::OffsetDateTime::parse(stamp, &time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .and_then(|date| u64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok())
        else {
            complete = false;
            continue;
        };
        let content = &record["message"]["content"];
        let text = match content {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Array(items) => items
                .iter()
                .filter(|item| item["type"] == "text")
                .filter_map(|item| item["text"].as_str())
                .filter(|text| is_user_text(text))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => continue,
        };
        if text.trim().is_empty() || !is_user_text(&text) {
            continue;
        }
        let mut row = Submission::new(at, &text);
        row.session = Some(session.to_owned());
        rows.push((uuid.to_owned(), row));
    }
    if crate::swearmeter_checkpoint::SourceStamp::at(path)? != Some(stamp.clone()) {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "transcript changed during scan",
        ));
    }
    Ok(Transcript {
        stamp,
        complete,
        rows,
    })
}

#[cfg(target_os = "macos")]
fn read_codex(
    root: &Path,
    rows: &mut HashSet<Submission>,
    indexed: &mut HashSet<String>,
) -> io::Result<bool> {
    use rusqlite::{Connection, OpenFlags};
    use serde::Deserialize;
    use std::time::Duration;

    #[derive(Deserialize)]
    struct Message {
        content: Vec<Content>,
    }
    #[derive(Deserialize)]
    struct Content {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        text: String,
    }

    let path = root.join("thread_history_1.sqlite");
    match path.symlink_metadata() {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Err(io::Error::other("Codex history is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error),
    }
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(io::Error::other)?;
    db.busy_timeout(Duration::from_millis(250))
        .map_err(io::Error::other)?;
    // This projection contains submitted user items and omits generated prompt wrappers.
    // Forks can carry copies at the original timestamp, even with a synthesized item ID.
    let mut query = db
        .prepare("SELECT created_at_ms, item_json, thread_id FROM thread_items WHERE item_type='userMessage'")
        .map_err(io::Error::other)?;
    let mut items = query.query([]).map_err(io::Error::other)?;
    let mut incomplete = false;
    while let Some(row) = items.next().map_err(io::Error::other)? {
        let (Ok(at), Ok(raw)) = (row.get::<_, u64>(0), row.get_ref(1)) else {
            incomplete = true;
            continue;
        };
        if let Ok(thread) = row.get::<_, String>(2) {
            indexed.insert(thread);
        }
        let Some(message) = raw
            .as_str()
            .ok()
            .and_then(|raw| serde_json::from_str::<Message>(raw).ok())
        else {
            incomplete = true;
            continue;
        };
        let text = message
            .content
            .into_iter()
            .filter(|item| item.kind == "text" && is_user_text(&item.text))
            .map(|item| item.text)
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            rows.insert(Submission::new(at, &text));
        }
    }
    Ok(!incomplete)
}

#[cfg(not(target_os = "macos"))]
fn read_codex(
    root: &Path,
    _rows: &mut HashSet<Submission>,
    _indexed: &mut HashSet<String>,
) -> io::Result<bool> {
    Ok(!root.join("thread_history_1.sqlite").try_exists()?)
}

pub(crate) fn parse_timestamp(stamp: &str) -> Option<u64> {
    let date =
        time::OffsetDateTime::parse(stamp, &time::format_description::well_known::Rfc3339).ok()?;
    u64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok()
}

pub(crate) fn is_user_text(text: &str) -> bool {
    let text = text.trim_start();
    if text.starts_with("# AGENTS.md instructions") {
        return false;
    }
    let Some(tag) = text
        .strip_prefix('<')
        .and_then(|text| text.split(['>', ' ', '\n']).next())
    else {
        return true;
    };
    !matches!(
        tag,
        "heartbeat"
            | "environment_context"
            | "external_codex_apps_open_page"
            | "external_codex_apps_selected_page_part_1_of_1"
            | "subagent_notification"
            | "collaboration_message"
            | "task-notification"
            | "developer"
            | "user_instructions"
            | "app-context"
            | "skills_instructions"
            | "permissions"
            | "system-reminder"
            | "local-command-caveat"
            | "local-command-stdout"
            | "command-name"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{fs, path::PathBuf};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("swearmeter-provider-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(root.join("projects/sample")).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                eprintln!("SwearMeter fixture cleanup: {error}");
            }
        }
    }

    #[test]
    fn recalls_expired_transcripts_counts_new_identical_submissions_and_excludes_sdk_copies() {
        let fixture = Fixture::new();
        let base = 1_700_000_000_000;
        let recall = [
            json!({"display":"shit","timestamp":base - 30 * 86_400_000,"sessionId":"expired"}),
            json!({"display":"fuck-shit","timestamp":base,"sessionId":"live"}),
        ];
        fs::write(
            fixture.0.join("history.jsonl"),
            recall.map(|row| row.to_string()).join("\n"),
        )
        .unwrap();
        let transcript = [
            json!({"type":"user","origin":{"kind":"human"},"promptSource":"typed","sessionId":"live","uuid":"a","timestamp":"2023-11-14T22:13:20Z","message":{"content":"fuck-shit"}}),
            json!({"type":"user","origin":{"kind":"human"},"promptSource":"typed","sessionId":"live","uuid":"b","timestamp":"2023-11-14T22:13:21Z","message":{"content":"fuck-shit"}}),
            json!({"type":"user","promptSource":"sdk","sessionId":"live","uuid":"c","timestamp":"2023-11-14T22:13:22Z","message":{"content":"shit shit shit"}}),
            json!({"type":"assistant","message":{"content":"shit shit shit"}}),
        ];
        fs::write(
            fixture.0.join("projects/sample/live.jsonl"),
            transcript.map(|row| row.to_string()).join("\n"),
        )
        .unwrap();
        let result = read_through(Some(&fixture.0), None, base + 2_000);
        assert_eq!(result.count, 3);
        assert!(
            result.incomplete,
            "unattributed SDK input qualifies the count"
        );
        let periods = Dates::report(&[&result.dates], base + 2_000, |at| {
            Some(if at < base { 202310 } else { 202311 })
        });
        assert_eq!(periods.last_30_days, 2);
        assert_eq!(
            periods.months.iter().map(|month| month.count).sum::<u64>(),
            3
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reads_codex_user_messages_once_and_leaves_the_database_unchanged() {
        let fixture = Fixture::new();
        let path = fixture.0.join("thread_history_1.sqlite");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE thread_items(created_at_ms INTEGER, item_type TEXT, item_json TEXT, thread_id TEXT);",
        )
        .unwrap();
        let message = json!({"content":[{"type":"text","text":"fuck-shit"}]}).to_string();
        let generated =
            json!({"content":[{"type":"text","text":"<heartbeat>shit shit</heartbeat>"}]})
                .to_string();
        for (at, kind, raw) in [
            (1000, "userMessage", &message),
            (1000, "userMessage", &message),
            (2000, "userMessage", &message),
            (2000, "agentMessage", &message),
            (2000, "userMessage", &generated),
        ] {
            db.execute(
                "INSERT INTO thread_items VALUES(?1,?2,?3,'fixture')",
                rusqlite::params![at, kind, raw],
            )
            .unwrap();
        }
        drop(db);
        let before = fs::read(&path).unwrap();
        let result = read_through(None, Some(&fixture.0), 2000);
        assert_eq!(result.count, 2);
        assert!(!result.incomplete);
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
