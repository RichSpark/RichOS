//! **A FILE PUT IN FRONT OF THE USER** — the reverse of the user's attachments (§138; build plan
//! item 12, round 36 screen 3 "A file from Rich").
//!
//! Rich and Stu call one tool, `richos_files.attach_file`, with the absolute path of a file on
//! this Mac. The file is written down here, and the conversation shows it as a card under Rich's
//! words: on the Mac a click opens it with the Mac's default app (`src-tauri/src/opener.rs`),
//! and on the phone a tap brings it over from the Mac.
//!
//! # What is recorded, and where
//!
//! One append-only file, `<engine state>/rich-files/files.jsonl`, one [`Row`] per attachment:
//! the conversation, the turn, who attached it and the file's CANONICAL path as it was when it
//! was attached. No content is copied: the card points at the user's own file, so a file that is
//! later moved or deleted shows as gone ("No longer on this Mac") rather than as a stale copy.
//!
//! # The scope is the app's, never the model's
//!
//! The tool reads the same per-turn scope file the question tool reads (`question_tools.rs`):
//! the company, the conversation, the turn and whether this turn is open for actions. The model
//! supplies a path and nothing else, so it cannot put a file in another conversation.
//!
//! # What the page holds is an id
//!
//! The window and the phone are handed an attachment id, never a path to act on. Opening,
//! showing in Finder and downloading resolve the id here ([`locate`]) against the record of the
//! conversation the request names, and then check what is at the recorded path now.

use crate::question_tools;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// The MCP server's name, and the tool's qualified name as the model sees it.
pub const SERVER: &str = "richos_files";
pub const TOOL: &str = "attach_file";
pub const QUALIFIED_TOOL: &str = "mcp__richos_files__attach_file";

/// The timeline item kind both the window and the phone rows read.
pub const ITEM_KIND: &str = "rich_file";

/// The folder under the engine state that holds the record.
pub const DIR: &str = "rich-files";
const RECORD: &str = "files.jsonl";
const SCHEMA: u32 = 1;
/// One request frame at most; a path is short.
const MAX_FRAME_BYTES: usize = 64 * 1024;

/// One file put in front of the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub schema: u32,
    pub id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub turn_id: String,
    /// `front_desk` (Rich) or `stu`. The conversation shows both as Rich's.
    pub asker: String,
    /// Canonical at the moment it was attached.
    pub path: String,
    pub name: String,
    /// Epoch millis.
    pub at: u64,
}

/// The record's file under an engine state root.
pub fn record_path(root: &Path) -> PathBuf {
    root.join(DIR).join(RECORD)
}

/// The stable id of one file in one turn: attaching the same file twice in a turn is one card.
pub fn attachment_id(thread_id: &str, turn_id: &str, canonical: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{thread_id}\n{turn_id}\n{canonical}").as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("rf_{hex}")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Every row the record holds, in the order they were attached. A line that does not parse is
/// skipped, never half-read.
pub fn all(root: &Path) -> Vec<Row> {
    let Ok(text) = std::fs::read_to_string(record_path(root)) else { return Vec::new() };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Row>(line).ok())
        .filter(|row| row.schema == SCHEMA)
        .collect()
}

/// The rows of one conversation of one company.
pub fn list(root: &Path, entity_id: &str, thread_id: &str) -> Vec<Row> {
    all(root)
        .into_iter()
        .filter(|row| row.entity_id == entity_id && row.thread_id == thread_id)
        .collect()
}

/// Write down one file for a turn. The path must be absolute and name a regular file that is
/// there now; a symbolic link is refused, because the card would open whatever it points at
/// later. Returns the row, or the existing one when this file is already on this turn.
pub fn attach(root: &Path, entity_id: &str, thread_id: &str, turn_id: &str, asker: &str, path: &str) -> Result<Row, String> {
    let given = Path::new(path.trim());
    if !given.is_absolute() {
        return Err("Give the file's absolute path, starting with /.".into());
    }
    let meta = match std::fs::symlink_metadata(given) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(format!("There is no file at {}. Nothing was attached.", given.display()))
        }
        Err(e) => return Err(format!("I could not look at {}: {e}. Nothing was attached.", given.display())),
    };
    if meta.file_type().is_symlink() {
        return Err("That path is a link. Attach the file it points to, by its own path.".into());
    }
    if !meta.is_file() {
        return Err("That is a folder or a special file. Attach one regular file at a time.".into());
    }
    let canonical = std::fs::canonicalize(given).map_err(|e| format!("I could not resolve that path: {e}."))?;
    let canonical = canonical.to_string_lossy().into_owned();
    let id = attachment_id(thread_id, turn_id, &canonical);
    if let Some(existing) = all(root).into_iter().find(|row| row.id == id) {
        return Ok(existing);
    }
    let name = Path::new(&canonical)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| canonical.clone());
    let row = Row {
        schema: SCHEMA,
        id,
        entity_id: entity_id.into(),
        thread_id: thread_id.into(),
        turn_id: turn_id.into(),
        asker: asker.into(),
        path: canonical,
        name,
        at: now_ms(),
    };
    let file = record_path(root);
    std::fs::create_dir_all(file.parent().unwrap_or(root)).map_err(|e| e.to_string())?;
    let mut line = serde_json::to_string(&row).map_err(|e| e.to_string())?;
    line.push('\n');
    // One `write` of one line in append mode: two attachments at once cannot interleave.
    let mut out = std::fs::OpenOptions::new().create(true).append(true).open(&file).map_err(|e| e.to_string())?;
    out.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    Ok(row)
}

/// An attachment id, found in the record of the conversation named: its canonical path.
/// `None` for an id that conversation does not hold.
pub fn locate(root: &Path, thread_id: &str, id: &str) -> Option<Row> {
    all(root).into_iter().find(|row| row.id == id && row.thread_id == thread_id)
}

/// What is at a recorded path now: the file's size, or why it cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Now {
    Here { bytes: u64 },
    /// Moved or deleted.
    Gone,
    /// A link, a folder, or a different file than the one attached.
    Refused,
}

/// Re-stat the recorded path, refusing a link or anything swapped in under it.
pub fn now(row: &Row) -> Now {
    let path = Path::new(&row.path);
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(_) => return Now::Gone,
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Now::Refused;
    }
    match std::fs::canonicalize(path) {
        Ok(canonical) if canonical == path => Now::Here { bytes: meta.len() },
        Ok(_) => Now::Refused,
        Err(_) => Now::Gone,
    }
}

/// The largest file the Mac sends to the phone in one answer. Over it, the phone says to open it
/// on the Mac rather than holding the whole of it in memory on both ends.
pub const PHONE_LIMIT: u64 = 100 * 1000 * 1000;

/// Why a file could not be sent to the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetch {
    /// The conversation's record does not hold that id.
    NotAttached,
    /// Moved or deleted.
    Gone,
    /// A link, a folder, or another file swapped in under the recorded path.
    Refused,
    /// Larger than [`PHONE_LIMIT`].
    TooLarge(u64),
    /// The OS would not let it be read; its own words.
    Unreadable(String),
}

/// The bytes of an attached file, for the phone: resolved by id in the conversation named,
/// checked as it is now, and read without following a link at the last component.
pub fn read_for_phone(root: &Path, thread_id: &str, id: &str) -> Result<(Row, Vec<u8>), Fetch> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let row = locate(root, thread_id, id).ok_or(Fetch::NotAttached)?;
    match now(&row) {
        Now::Here { bytes } if bytes > PHONE_LIMIT => return Err(Fetch::TooLarge(bytes)),
        Now::Here { .. } => {}
        Now::Gone => return Err(Fetch::Gone),
        Now::Refused => return Err(Fetch::Refused),
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&row.path)
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ELOOP) => Fetch::Refused,
            _ if e.kind() == io::ErrorKind::NotFound => Fetch::Gone,
            _ => Fetch::Unreadable(e.to_string()),
        })?;
    let mut body = Vec::new();
    (&mut file).take(PHONE_LIMIT + 1).read_to_end(&mut body).map_err(|e| Fetch::Unreadable(e.to_string()))?;
    if body.len() as u64 > PHONE_LIMIT {
        return Err(Fetch::TooLarge(body.len() as u64));
    }
    Ok((row, body))
}

/// The type word the card prints before the size: `PDF · 184 KB`, `Spreadsheet · 46 KB`.
pub fn type_label(name: &str) -> String {
    match crate::output::kind_of(name) {
        "pdf" => "PDF".into(),
        "xlsx" | "csv" => "Spreadsheet".into(),
        "docx" => "Document".into(),
        "pptx" => "Presentation".into(),
        "png" => "Image".into(),
        "mp4" => "Video".into(),
        "audio" => "Audio".into(),
        "md" | "txt" => "Text".into(),
        _ => match extension(name) {
            Some(ext) => format!("{ext} file"),
            None => "File".into(),
        },
    }
}

/// The extension as the page glyph prints it (`PDF`, `XLSX`), or `None`.
pub fn extension(name: &str) -> Option<String> {
    let ext = Path::new(name).extension()?.to_str()?;
    if ext.is_empty() || ext.len() > 5 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(ext.to_ascii_uppercase())
}

/// Decimal units, as Finder shows them: `96 KB`, `14.2 MB`.
pub fn size_label(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes < 1000 {
        format!("{bytes} bytes")
    } else if b < 1e6 {
        format!("{} KB", (b / 1e3).round() as u64)
    } else if b < 1e9 {
        format!("{:.1} MB", b / 1e6)
    } else {
        format!("{:.1} GB", b / 1e9)
    }
}

/// The card's own description of one file, shared by the window and the phone.
pub fn file_value(row: &Row) -> Value {
    let state = now(row);
    let (exists, bytes) = match state {
        Now::Here { bytes } => (true, Some(bytes)),
        _ => (false, None),
    };
    let label = type_label(&row.name);
    let meta = match bytes {
        Some(bytes) => format!("{label} · {}", size_label(bytes)),
        None => label.clone(),
    };
    json!({
        "id": row.id,
        "name": row.name,
        "ext": extension(&row.name).unwrap_or_else(|| "FILE".into()),
        "type": label,
        "meta": meta,
        "bytes": bytes,
        "exists": exists,
        "refused": state == Now::Refused,
    })
}

/// A PDF's page count on its card (round 36 screen 3, "PDF · 184 KB · 2 pages"): `pages` and the
/// meta line's last part. Counted by the window's host, which has the Mac's own PDF reader; a
/// file that is gone, or a count of none, leaves the card as it was.
pub fn add_pages(file: &mut Value, pages: usize) {
    if pages == 0 || file["exists"] != true {
        return;
    }
    let meta = file["meta"].as_str().unwrap_or_default().to_string();
    let count = if pages == 1 { "1 page".to_string() } else { format!("{pages} pages") };
    file["pages"] = json!(pages);
    file["meta"] = json!(if meta.is_empty() { count } else { format!("{meta} · {count}") });
}

/// One attachment as a timeline item, the shape `question_host.rs` merges questions in.
pub fn item(row: &Row) -> Value {
    json!({
        "kind": ITEM_KIND,
        "id": row.id,
        "entityId": row.entity_id,
        "threadId": row.thread_id,
        "turnId": row.turn_id,
        "visibility": "ceo",
        "createdAt": row.at,
        "sequence": row.at,
        "text": row.name,
        "file": file_value(row),
    })
}

/// The conversation's attachments as timeline items.
pub fn items(root: &Path, entity_id: &str, thread_id: &str) -> Vec<Value> {
    list(root, entity_id, thread_id).iter().map(item).collect()
}

// ---- the MCP endpoint ---------------------------------------------------------------------

pub fn tools() -> Value {
    json!({"tools":[{
        "name": TOOL,
        "description": "Put a file from this Mac in front of the user. It appears as a card under your reply in this conversation, on the Mac and on the phone and the user opens it with one click or tap. Give the absolute path of one existing regular file; call once per file. Use it whenever the user asks you to send, show or give them a file and whenever a file is the answer.",
        "inputSchema": {"type":"object","properties":{"path":{"type":"string","description":"Absolute path of the file, starting with /."}},"required":["path"],"additionalProperties":false}
    }]})
}

/// One tool call against the turn's scope file.
pub fn call(scope_path: &Path, name: &str, args: Value) -> Result<Value, String> {
    if name != TOOL {
        return Err("Unknown file tool.".into());
    }
    let scope = question_tools::read_scope(scope_path).map_err(|_| "This turn is not open for files.".to_string())?;
    if !scope.actions_allowed {
        return Err("Files can be attached only while you are answering the user or working on their request.".into());
    }
    let keys = args.as_object().ok_or("Tool arguments must be an object.")?;
    if keys.keys().any(|key| key != "path") {
        return Err("Give only the path. The conversation is supplied by the app.".into());
    }
    let path = args["path"].as_str().ok_or("Give the file's absolute path.")?;
    let s = &scope.context;
    // A background job's file belongs to its job, the way its question does.
    let turn = s.obligation_id.as_deref().unwrap_or(&s.turn_id);
    let row = attach(&s.root, &s.entity_id, &s.thread_id, turn, &s.asker, path)?;
    Ok(json!({"attached": true, "id": row.id, "name": row.name,
        "shown": "The user sees this file as a card under your reply and can open it."}))
}

pub fn run_stdio(scope_path: &Path) -> io::Result<()> {
    serve(scope_path, io::stdin().lock(), io::stdout().lock())
}

/// JSON-RPC over stdio, one frame per line, the framing `question_tools::serve` uses.
pub fn serve(scope_path: &Path, mut reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
    let mut initialized = false;
    loop {
        let mut frame = Vec::new();
        let mut oversized = false;
        loop {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            let n = buf.iter().position(|b| *b == b'\n').map_or(buf.len(), |i| i + 1);
            let end = buf[n - 1] == b'\n';
            if frame.len() + n <= MAX_FRAME_BYTES && !oversized {
                frame.extend_from_slice(&buf[..n]);
            } else {
                oversized = true;
            }
            reader.consume(n);
            if end {
                break;
            }
        }
        if frame.is_empty() && !oversized {
            break;
        }
        let reply = match serde_json::from_slice::<Value>(&frame) {
            Ok(r) if !oversized => {
                let Some(id) = r.get("id") else { continue };
                let result = match r["method"].as_str().unwrap_or("") {
                    "initialize" => {
                        initialized = true;
                        Ok(json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":SERVER,"version":"1"}}))
                    }
                    "ping" => Ok(json!({})),
                    "tools/list" if initialized => Ok(tools()),
                    "tools/call" if initialized => {
                        let result = call(scope_path, r["params"]["name"].as_str().unwrap_or(""), r["params"]["arguments"].clone());
                        let error = result.is_err();
                        let text = match result {
                            Ok(v) => v.to_string(),
                            Err(e) => e,
                        };
                        Ok(json!({"isError":error,"content":[{"type":"text","text":text}]}))
                    }
                    _ => Err("Initialize before calling an available method."),
                };
                match result {
                    Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                    Err(e) => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":e}}),
                }
            }
            _ => json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid or oversized file request"}}),
        };
        serde_json::to_writer(&mut writer, &reply)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::question_tools::{write_scope, Scope};
    use crate::questions::AskScope;

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let p = std::env::temp_dir().join(format!("richos-rich-files-{tag}-{}-{}", std::process::id(), now_ms()));
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

    fn scope(dir: &Dir, asker: &str, allowed: bool, obligation: Option<&str>) -> PathBuf {
        let root = dir.0.join("engine-state");
        let path = dir.0.join("assignments.questions.json");
        write_scope(&path, &Scope {
            context: AskScope { root, entity_id: "acme".into(), thread_id: "t1".into(), turn_id: "turn-1".into(),
                asker: asker.into(), obligation_id: obligation.map(str::to_string), session_id: "s".into(),
                engine: None, entity_root: None, app_run: None },
            actions_allowed: allowed, answer_method: "typed".into(), surface: "mac".into(),
        }).unwrap();
        path
    }

    #[test]
    fn a_file_attached_in_a_turn_becomes_one_item_of_that_conversation() {
        let dir = Dir::new("attach");
        let file = dir.0.join("Acme counteroffer.pdf");
        std::fs::write(&file, vec![0u8; 184_000]).unwrap();
        let path = scope(&dir, "front_desk", true, None);
        let out = call(&path, TOOL, json!({"path": file.to_string_lossy()})).unwrap();
        assert_eq!(out["attached"], true);
        // Twice in the same turn is one card.
        let again = call(&path, TOOL, json!({"path": file.to_string_lossy()})).unwrap();
        assert_eq!(again["id"], out["id"]);
        let root = dir.0.join("engine-state");
        let items = items(&root, "acme", "t1");
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it["kind"], ITEM_KIND);
        assert_eq!(it["turnId"], "turn-1");
        assert_eq!(it["file"]["name"], "Acme counteroffer.pdf");
        assert_eq!(it["file"]["ext"], "PDF");
        assert_eq!(it["file"]["meta"], "PDF · 184 KB");
        assert_eq!(it["file"]["exists"], true);
        // Another conversation or company sees none of it.
        assert!(items_of(&root, "acme", "t2").is_empty());
        assert!(items_of(&root, "other", "t1").is_empty());
        // Gone from the Mac: still listed, and says so.
        std::fs::remove_file(&file).unwrap();
        let gone = &super::items(&root, "acme", "t1")[0];
        assert_eq!(gone["file"]["exists"], false);
        assert_eq!(gone["file"]["meta"], "PDF");
    }

    #[test]
    fn a_pdf_card_says_its_pages_as_the_round_writes_them() {
        let mut two = json!({"exists": true, "meta": "PDF · 184 KB"});
        add_pages(&mut two, 2);
        assert_eq!(two["meta"], "PDF · 184 KB · 2 pages");
        assert_eq!(two["pages"], 2);
        let mut one = json!({"exists": true, "meta": "PDF · 3 KB"});
        add_pages(&mut one, 1);
        assert_eq!(one["meta"], "PDF · 3 KB · 1 page");
        // A file that is gone, or a count of none, leaves the card as it was.
        let mut gone = json!({"exists": false, "meta": "PDF"});
        add_pages(&mut gone, 4);
        assert_eq!(gone, json!({"exists": false, "meta": "PDF"}));
        let mut none = json!({"exists": true, "meta": "PDF · 3 KB"});
        add_pages(&mut none, 0);
        assert_eq!(none, json!({"exists": true, "meta": "PDF · 3 KB"}));
    }

    fn items_of(root: &Path, entity: &str, thread: &str) -> Vec<Value> {
        super::items(root, entity, thread)
    }

    #[test]
    fn a_background_job_files_its_attachment_under_its_job() {
        let dir = Dir::new("stu");
        let file = dir.0.join("comps.xlsx");
        std::fs::write(&file, b"x").unwrap();
        let path = scope(&dir, "stu", true, Some("obl-7"));
        call(&path, TOOL, json!({"path": file.to_string_lossy()})).unwrap();
        let row = &list(&dir.0.join("engine-state"), "acme", "t1")[0];
        assert_eq!(row.turn_id, "obl-7");
        assert_eq!(row.asker, "stu");
        assert_eq!(type_label(&row.name), "Spreadsheet");
    }

    #[test]
    fn what_is_refused_and_why() {
        let dir = Dir::new("refuse");
        let file = dir.0.join("a.txt");
        std::fs::write(&file, b"x").unwrap();
        let link = dir.0.join("link.txt");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let open = scope(&dir, "front_desk", true, None);
        for (args, why) in [
            (json!({"path": "a.txt"}), "absolute"),
            (json!({"path": dir.0.join("missing.pdf").to_string_lossy()}), "no file"),
            (json!({"path": link.to_string_lossy()}), "link"),
            (json!({"path": dir.0.to_string_lossy()}), "folder"),
            (json!({"path": file.to_string_lossy(), "thread_id": "t9"}), "Give only the path"),
        ] {
            let error = call(&open, TOOL, args).unwrap_err();
            assert!(error.contains(why), "{error}");
        }
        assert!(call(&open, "other", json!({})).is_err());
        let closed = scope(&dir, "front_desk", false, None);
        assert!(call(&closed, TOOL, json!({"path": file.to_string_lossy()})).is_err());
        assert!(all(&dir.0.join("engine-state")).is_empty(), "a refusal writes nothing");
    }

    #[test]
    fn locate_answers_only_for_the_conversation_named() {
        let dir = Dir::new("locate");
        let file = dir.0.join("b.md");
        std::fs::write(&file, b"x").unwrap();
        let root = dir.0.join("engine-state");
        let row = attach(&root, "acme", "t1", "turn-1", "front_desk", &file.to_string_lossy()).unwrap();
        assert_eq!(locate(&root, "t1", &row.id).unwrap().path, std::fs::canonicalize(&file).unwrap().to_string_lossy());
        assert!(locate(&root, "t2", &row.id).is_none());
        assert!(locate(&root, "t1", "rf_0000000000000000").is_none());
        // Swapped for a link after it was attached: refused, never followed.
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", &file).unwrap();
        assert_eq!(now(&row), Now::Refused);
    }

    #[test]
    fn the_phone_gets_the_bytes_of_a_file_this_conversation_holds_and_nothing_else() {
        let dir = Dir::new("phone");
        let file = dir.0.join("comps.csv");
        std::fs::write(&file, b"a,b\n1,2\n").unwrap();
        let root = dir.0.join("engine-state");
        let row = attach(&root, "acme", "t1", "turn-1", "front_desk", &file.to_string_lossy()).unwrap();
        let (got, body) = read_for_phone(&root, "t1", &row.id).unwrap();
        assert_eq!(got.name, "comps.csv");
        assert_eq!(body, b"a,b\n1,2\n");
        assert_eq!(read_for_phone(&root, "t2", &row.id).unwrap_err(), Fetch::NotAttached);
        assert_eq!(read_for_phone(&root, "t1", "rf_0").unwrap_err(), Fetch::NotAttached);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(read_for_phone(&root, "t1", &row.id).unwrap_err(), Fetch::Gone);
        std::os::unix::fs::symlink("/etc/hosts", &file).unwrap();
        assert_eq!(read_for_phone(&root, "t1", &row.id).unwrap_err(), Fetch::Refused);
    }

    #[test]
    fn the_server_lists_one_tool_and_answers_a_call() {
        let dir = Dir::new("serve");
        let file = dir.0.join("c.pdf");
        std::fs::write(&file, b"x").unwrap();
        let path = scope(&dir, "front_desk", true, None);
        let input = format!(
            "{}\n{}\n{}\n",
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":TOOL,"arguments":{"path":file.to_string_lossy()}}}),
        );
        let mut out = Vec::new();
        serve(&path, input.as_bytes(), &mut out).unwrap();
        let replies: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(replies[1]["result"]["tools"][0]["name"], TOOL);
        assert_eq!(replies[2]["result"]["isError"], false);
        assert_eq!(QUALIFIED_TOOL, format!("mcp__{SERVER}__{TOOL}"));
    }
}
