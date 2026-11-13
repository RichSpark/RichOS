//! Numeric projection of the user's Claude Code input recall. Session transcripts are
//! a separate representation and can expire before this history does.
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub submissions: u64,
    pub matches: u64,
    pub unreadable_records: u64,
    pub incomplete_submissions: u64,
    pub first_ms: Option<u64>,
    pub last_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    display: String,
    timestamp: u64,
    session_id: String,
    #[serde(default)]
    pasted_contents: HashMap<u64, Paste>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Paste {
    #[serde(rename = "type")]
    kind: String,
    content: Option<String>,
    content_hash: Option<String>,
}

/// `None` means the recall file is absent. A read failure means it is unavailable.
/// The caller supplies the Claude Code configuration directory for the current user.
pub fn read(root: &Path) -> io::Result<Option<Summary>> {
    read_submissions(root).map(|value| value.map(|(summary, _)| summary))
}

pub(crate) fn read_submissions(
    root: &Path,
) -> io::Result<Option<(Summary, Vec<crate::swearmeter_provider_history::Submission>)>> {
    let file = match open_regular(&root.join("history.jsonl")) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut rows = Vec::new();
    let summary = summarize(BufReader::new(file), root, |row| rows.push(row))?;
    Ok(Some((summary, rows)))
}

pub(crate) fn open_regular(path: &Path) -> io::Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected a regular history file",
        ));
    }
    Ok(file)
}

pub(crate) fn read_record(reader: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<usize> {
    let bytes = reader
        .by_ref()
        .take(MAX_RECORD_BYTES as u64 + 1)
        .read_until(b'\n', line)?;
    if bytes > MAX_RECORD_BYTES {
        if !line.ends_with(b"\n") {
            reader.skip_until(b'\n')?;
        }
        line.clear();
    }
    Ok(bytes)
}

fn summarize(
    mut reader: impl BufRead,
    root: &Path,
    mut emit: impl FnMut(crate::swearmeter_provider_history::Submission),
) -> io::Result<Summary> {
    use sha2::{Digest, Sha256};
    let mut summary = Summary::default();
    let mut identities = HashSet::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let bytes = read_record(&mut reader, &mut line)?;
        if bytes == 0 {
            return Ok(summary);
        }
        if line.is_empty() {
            summary.unreadable_records += 1;
            continue;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let Ok(entry) = serde_json::from_slice::<Entry>(&line) else {
            summary.unreadable_records += 1;
            continue;
        };
        if entry.session_id.is_empty() || entry.timestamp == 0 {
            summary.unreadable_records += 1;
            continue;
        }
        let text = entry.display.trim();
        // Claude's slash commands and local shell commands are not submitted prompts.
        if text.is_empty()
            || text.starts_with('!')
            || text.starts_with('/')
                && text
                    .as_bytes()
                    .get(1)
                    .is_some_and(|b| !b.is_ascii_whitespace())
        {
            continue;
        }
        let (expanded, incomplete) = authored_text(text, &entry.pasted_contents, root);
        let key = (
            entry.session_id.clone(),
            entry.timestamp,
            Sha256::digest(expanded.as_bytes()),
        );
        if !identities.insert(key) {
            continue;
        }
        summary.submissions += 1;
        let mut row =
            crate::swearmeter_provider_history::Submission::new(entry.timestamp, &expanded);
        row.session = Some(entry.session_id);
        summary.matches += row.count;
        emit(row);
        summary.incomplete_submissions += u64::from(incomplete);
        summary.first_ms = Some(
            summary
                .first_ms
                .map_or(entry.timestamp, |first| first.min(entry.timestamp)),
        );
        summary.last_ms = Some(
            summary
                .last_ms
                .map_or(entry.timestamp, |last| last.max(entry.timestamp)),
        );
    }
}

fn authored_text(text: &str, pastes: &HashMap<u64, Paste>, root: &Path) -> (String, bool) {
    let mut expanded = String::with_capacity(text.len());
    let mut rest = text;
    let mut incomplete = false;
    while let Some(open) = rest.find('[') {
        expanded.push_str(&rest[..open]);
        rest = &rest[open + 1..];
        let placeholder = rest
            .find([']', '[', '\n'])
            .filter(|end| rest.as_bytes()[*end] == b']')
            .and_then(|end| parse_placeholder(&rest[..end]).map(|reference| (end, reference)));
        let Some((end, reference)) = placeholder else {
            expanded.push('[');
            continue;
        };
        rest = &rest[end + 1..];
        if let Some(id) = reference {
            match pastes.get(&id).and_then(|paste| paste_text(paste, root)) {
                Some(text)
                    if expanded
                        .len()
                        .saturating_add(text.len())
                        .saturating_add(rest.len())
                        <= MAX_RECORD_BYTES =>
                {
                    expanded.push_str(&text)
                }
                _ => {
                    incomplete = true;
                    expanded.push(' ');
                }
            }
        } else {
            expanded.push(' ');
        }
    }
    expanded.push_str(rest);
    (expanded, incomplete)
}

/// `Some(None)` is a generated image marker. `Some(Some(id))` is text to recover.
fn parse_placeholder(value: &str) -> Option<Option<u64>> {
    if let Some(id) = value.strip_prefix("Image #") {
        return id.parse::<u64>().ok().map(|_| None);
    }
    let token = value.strip_prefix("Pasted text #")?;
    let id = if let Some((id, suffix)) = token.split_once(" +") {
        let lines = suffix
            .strip_suffix(" lines")
            .or_else(|| suffix.strip_suffix(" line"))?;
        lines.parse::<u64>().ok()?;
        id
    } else {
        token
    };
    id.parse::<u64>().ok().map(Some)
}

fn paste_text(paste: &Paste, root: &Path) -> Option<String> {
    if paste.kind != "text" {
        return None;
    }
    if let Some(text) = &paste.content {
        return Some(text.clone());
    }
    let hash = paste.content_hash.as_deref()?;
    if hash.len() != 16 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let dir = root.join("paste-cache");
    if !dir.symlink_metadata().ok()?.file_type().is_dir() {
        return None;
    }
    let mut file = open_regular(&dir.join(format!("{hash}.txt")))
        .ok()?
        .take(MAX_RECORD_BYTES as u64 + 1);
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    (text.len() <= MAX_RECORD_BYTES).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_submission_counts_once_but_repeated_text_counts_again() {
        let rows = r#"{"display":"fuck-shit","timestamp":1,"sessionId":"a"}
{"display":"fuck-shit","timestamp":1,"sessionId":"a"}
{"display":"fuck-shit","timestamp":2,"sessionId":"a"}
{"display":"fuck-shit","timestamp":1,"sessionId":"b"}
{"display":"/rename fuck","timestamp":3,"sessionId":"a"}
{"display":"! echo shit","timestamp":4,"sessionId":"a"}
{"display":"Wow hell!","timestamp":5,"sessionId":"a"}"#;
        let actual = summarize(rows.as_bytes(), Path::new("unused"), |_| {}).unwrap();
        assert_eq!(
            actual,
            Summary {
                submissions: 4,
                matches: 4,
                first_ms: Some(1),
                last_ms: Some(5),
                ..Summary::default()
            }
        );
    }

    #[test]
    fn uses_submitted_pastes_and_reports_absent_text_and_malformed_history() {
        let rows = r#"{"display":"[Image #2] [Pasted text #1 +3 lines] fu[Pasted text #3]ck [Pasted text #bogus]","timestamp":1,"sessionId":"a","pastedContents":{"1":{"type":"text","content":"shit fuck-shit"},"3":{"type":"text","contentHash":"../shit"}}}
bogus
{"display":"hell","timestamp":2,"sessionId":"a"}"#;
        let actual = summarize(rows.as_bytes(), Path::new("unused"), |_| {}).unwrap();
        assert_eq!(
            actual,
            Summary {
                submissions: 2,
                matches: 3,
                unreadable_records: 1,
                incomplete_submissions: 1,
                first_ms: Some(1),
                last_ms: Some(2)
            }
        );
    }
}
