//! Codex rollouts are the durable source when no indexed view covers the thread.
use crate::{
    swearmeter_checkpoint::SourceStamp,
    swearmeter_claude_history::{open_regular, read_record},
    swearmeter_provider_history::{is_user_text, Submission},
};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(crate) struct Rollouts {
    files: HashMap<PathBuf, (SourceStamp, Vec<Submission>, bool)>,
}

impl Rollouts {
    pub(crate) fn read(
        &mut self,
        root: &Path,
        indexed: &HashSet<String>,
        rows: &mut HashSet<Submission>,
    ) -> bool {
        let mut directories = vec![root.join("sessions"), root.join("archived_sessions")];
        let mut visited = HashSet::new();
        let mut complete = true;
        while let Some(directory) = directories.pop() {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(_) => {
                    complete = false;
                    continue;
                }
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
                if kind.is_dir() {
                    directories.push(path);
                    continue;
                }
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let bare = name.strip_suffix(".zst").unwrap_or(name);
                let Some(stem) = bare.strip_suffix(".jsonl") else {
                    continue;
                };
                if !kind.is_file() {
                    complete = false;
                    continue;
                }
                if stem.len() >= 36
                    && stem
                        .get(stem.len() - 36..)
                        .is_some_and(|id| indexed.contains(id))
                {
                    continue;
                }
                let Ok(Some(stamp)) = SourceStamp::at(&path) else {
                    complete = false;
                    continue;
                };
                visited.insert(path.clone());
                if self.files.get(&path).is_none_or(|cached| cached.0 != stamp) {
                    match read_rollout(&path, name.ends_with(".zst")) {
                        Ok((found, clean))
                            if SourceStamp::at(&path).ok() == Some(Some(stamp.clone())) =>
                        {
                            self.files.insert(path.clone(), (stamp, found, clean));
                        }
                        _ => {
                            complete = false;
                            continue;
                        }
                    }
                }
                let (_, found, clean) = &self.files[&path];
                rows.extend(found.iter().cloned());
                complete &= clean;
            }
        }
        self.files.retain(|path, _| visited.contains(path));
        complete
    }
}

fn read_rollout(path: &Path, compressed: bool) -> io::Result<(Vec<Submission>, bool)> {
    let file = open_regular(path)?;
    if compressed {
        let mut decoder = zstd::stream::read::Decoder::new(file)?;
        decoder.window_log_max(26)?;
        parse(BufReader::new(decoder))
    } else {
        parse(BufReader::new(file))
    }
}

fn parse(mut reader: impl BufRead) -> io::Result<(Vec<Submission>, bool)> {
    let mut line = Vec::new();
    let mut complete = true;
    let mut session_seen = false;
    let mut items = Vec::new();
    loop {
        line.clear();
        if read_record(&mut reader, &mut line)? == 0 {
            break;
        }
        if line.is_empty() {
            complete = false;
            continue;
        }
        if ![
            &b"\"session_meta\""[..],
            &b"\"input_text\""[..],
            &b"\"user_message\""[..],
        ]
        .iter()
        .any(|key| line.windows(key.len()).any(|piece| piece == *key))
        {
            continue;
        }
        let Ok(row) = serde_json::from_slice::<Value>(&line) else {
            complete = false;
            continue;
        };
        let payload = &row["payload"];
        if row["type"] == "session_meta" {
            if payload["source"].get("subagent").is_some()
                || payload["thread_source"]
                    .as_str()
                    .is_some_and(|value| value.contains("subagent"))
            {
                return Ok((Vec::new(), true));
            }
            session_seen = true;
            continue;
        }
        let (priority, text) = if row["type"] == "response_item"
            && payload["type"] == "message"
            && payload["role"] == "user"
        {
            let Some(content) = payload["content"].as_array() else {
                continue;
            };
            let kinds = payload["internal_chat_message_metadata_passthrough"]["content_item_kinds"]
                .as_array();
            let typed = kinds.is_some_and(|kinds| kinds.iter().any(user_kind));
            if kinds.is_some_and(|kinds| !kinds.is_empty()) && !typed {
                continue;
            }
            let text = content
                .iter()
                .enumerate()
                .filter(|(i, part)| {
                    part["type"] == "input_text"
                        && kinds
                            .filter(|kinds| kinds.len() == content.len())
                            .is_none_or(|kinds| user_kind(&kinds[*i]))
                })
                .filter_map(|(_, part)| part["text"].as_str())
                .filter(|text| is_user_text(text))
                .collect::<Vec<_>>()
                .join("\n");
            (if typed { 3 } else { 1 }, text)
        } else if row["type"] == "event_msg" && payload["type"] == "user_message" {
            (
                2,
                payload["message"]
                    .as_str()
                    .filter(|text| is_user_text(text))
                    .unwrap_or_default()
                    .to_owned(),
            )
        } else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let Some(at) = row["timestamp"]
            .as_str()
            .and_then(crate::swearmeter_provider_history::parse_timestamp)
        else {
            complete = false;
            continue;
        };
        items.push((priority, Submission::new(at, &text)));
    }
    // Each format stores several renderings of one input. Use the structured source
    // where present, retaining legacy response items in older sessions.
    let priority = items.iter().map(|(priority, _)| *priority).max();
    Ok((
        items
            .into_iter()
            .filter(|(p, _)| Some(*p) == priority)
            .map(|(_, row)| row)
            .collect(),
        complete && session_seen,
    ))
}

fn user_kind(kind: &Value) -> bool {
    matches!(
        kind.as_str(),
        Some("user.text" | "user.dictated_text" | "user.voice" | "user.transcription")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_messages_are_read_once_and_generated_items_are_excluded() {
        let raw = r#"{"type":"session_meta","payload":{"source":"vscode"}}
{"timestamp":"2026-02-04T21:13:20.618Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"fuck-shit"}]}}
{"timestamp":"2026-02-04T21:13:20.618Z","type":"event_msg","payload":{"type":"user_message","message":"fuck-shit"}}
{"timestamp":"2026-02-04T21:13:21.618Z","type":"event_msg","payload":{"type":"user_message","message":"fuck-shit"}}
{"timestamp":"2026-02-04T21:13:22.618Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<heartbeat>shit shit</heartbeat>"}]}}"#;
        let (items, complete) = parse(raw.as_bytes()).unwrap();
        assert!(complete);
        assert_eq!(items.len(), 2);
        assert_eq!(items.iter().map(|item| item.count).sum::<u64>(), 2);
    }

    #[test]
    fn compressed_archive_is_read_without_an_external_decompressor() {
        let fixture = std::env::temp_dir().join(format!(
            "swearmeter-codex-{}.jsonl.zst",
            uuid::Uuid::new_v4()
        ));
        let raw = b"{\"type\":\"session_meta\",\"payload\":{\"source\":\"vscode\"}}\n";
        fs::write(&fixture, zstd::stream::encode_all(&raw[..], 0).unwrap()).unwrap();
        let result = read_rollout(&fixture, true).unwrap();
        fs::remove_file(fixture).unwrap();
        assert!(result.0.is_empty() && result.1);
    }
}
