//! Timestamped evidence attached to a Rich answer. The app supplies the company,
//! conversation and turn. Playback resolves the recorded source again by card id.

use crate::{output, question_tools, rich_files};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const TOOL: &str = "show_media_moments";
const RECORD: &str = "media-moments.jsonl";
const POINT_LIMIT: usize = 8;
const QUOTE_LIMIT: usize = 420;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileVersion {
    device: u64,
    inode: u64,
    size: u64,
    modified: i64,
    modified_ns: i64,
}

impl FileVersion {
    fn of(meta: &Metadata) -> Self {
        Self {
            device: meta.dev(), inode: meta.ino(), size: meta.len(),
            modified: meta.mtime(), modified_ns: meta.mtime_nsec(),
        }
    }

    pub fn matches(&self, meta: &Metadata) -> bool {
        meta.is_file() && *self == Self::of(meta)
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn still_at(&self, path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok_and(|meta| self.matches(&meta))
            && fs::canonicalize(path).is_ok_and(|canonical| canonical == path)
    }

    pub fn capture_recording(path: &Path) -> Result<(PathBuf, Self), String> {
        let supported = |path: &Path| matches!(output::kind_of(&path.to_string_lossy()), "mp4" | "audio");
        if !path.is_absolute() || !supported(path) {
            return Err("Give the absolute path of an MP4, MOV, M4V, WebM, M4A, MP3 or WAV recording.".into());
        }
        let file = OpenOptions::new().read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path).map_err(|_| "The recording could not be opened.".to_string())?;
        let meta = file.metadata().map_err(|_| "The recording could not be read.".to_string())?;
        if !meta.is_file() || meta.len() == 0 {
            return Err("The recording must be a nonempty regular file.".into());
        }
        let path = fs::canonicalize(path).map_err(|_| "The recording could not be located.".to_string())?;
        let version = Self::of(&meta);
        if !supported(&path) || !version.still_at(&path) {
            return Err("The recording changed or is in an unsupported format.".into());
        }
        Ok((path, version))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    Local { path: PathBuf, version: FileVersion },
    Youtube { video_id: String },
}

impl Source {
    pub fn youtube_url(&self, start_ms: u32) -> Option<String> {
        let Self::Youtube { video_id } = self else { return None };
        valid_video_id(video_id).then(|| {
            format!("https://www.youtube.com/watch?v={video_id}&t={}s", start_ms / 1000)
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    pub start_ms: u32,
    pub label: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Moments {
    pub duration_ms: u32,
    pub points: Vec<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waveform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub name: String,
    pub at: u64,
    pub source: Source,
    pub moments: Moments,
}

impl Row {
    pub fn item(&self) -> Value {
        let (source, exists, youtube_id, media_type, bytes, download_name) = match &self.source {
            Source::Local { path, version } => {
                let media_type = if output::kind_of(&path.to_string_lossy()) == "mp4" { "video" } else { "audio" };
                ("local", version.still_at(path), None, media_type, Some(version.size()), Some(download_name(path)))
            }
            Source::Youtube { video_id } => ("youtube", true, Some(video_id), "video", None, None),
        };
        json!({
            "kind": rich_files::ITEM_KIND, "id": self.id, "entityId": self.entity_id,
            "threadId": self.thread_id, "turnId": self.turn_id, "visibility": "ceo",
            "createdAt": self.at, "sequence": self.at, "text": self.name,
            "file": {"id": self.id, "name": self.name, "source": source,
                     "exists": exists, "moments": self.moments, "youtubeId": youtube_id, "mediaType": media_type,
                     "bytes": bytes, "downloadName": download_name}
        })
    }
}

fn download_name(path: &Path) -> String {
    let extension = path.extension().and_then(|part| part.to_str()).unwrap_or_default().to_ascii_lowercase();
    format!("recording.{extension}")
}

pub fn read_for_phone(root: &Path, entity: &str, thread: &str, id: &str) -> Result<(String, Vec<u8>), rich_files::Fetch> {
    use rich_files::{Fetch, PHONE_LIMIT};
    let row = locate(root, entity, thread, id).ok_or(Fetch::NotAttached)?;
    let Source::Local { path, version } = row.source else { return Err(Fetch::NotAttached) };
    if !version.still_at(&path) {
        return Err(Fetch::Gone);
    }
    if version.size() > PHONE_LIMIT {
        return Err(Fetch::TooLarge(version.size()));
    }
    let mut file = OpenOptions::new().read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => Fetch::Gone,
            _ => Fetch::Unreadable(error.to_string()),
        })?;
    let read_error = |error: std::io::Error| Fetch::Unreadable(error.to_string());
    if !version.matches(&file.metadata().map_err(read_error)?) {
        return Err(Fetch::Gone);
    }
    let mut body = Vec::new();
    (&mut file).take(version.size() + 1).read_to_end(&mut body).map_err(read_error)?;
    if body.len() as u64 != version.size() || !version.matches(&file.metadata().map_err(read_error)?) || !version.still_at(&path) {
        return Err(Fetch::Gone);
    }
    Ok((download_name(&path), body))
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum InputSource {
    Local { path: PathBuf },
    Youtube { video_id: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    name: String,
    source: InputSource,
    duration_ms: u32,
    points: Vec<Point>,
    waveform: Option<String>,
}

fn valid_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

impl InputSource {
    fn resolve(self) -> Result<Source, String> {
        match self {
            Self::Youtube { video_id } if valid_video_id(&video_id) => Ok(Source::Youtube { video_id }),
            Self::Youtube { .. } => Err("Give the 11-character YouTube video ID.".into()),
            Self::Local { path } => {
                let (path, version) = FileVersion::capture_recording(&path)?;
                Ok(Source::Local { path, version })
            }
        }
    }
}

pub fn definition() -> Value {
    json!({
        "name": TOOL,
        "description": "Show timestamped audio or video evidence below Rich’s reply. Supply the verified duration and moments from the original source. A local source must remain available after this turn.",
        "inputSchema": {
            "type": "object", "additionalProperties": false,
            "required": ["name", "source", "durationMs", "points"],
            "properties": {
                "name": {"type": "string", "maxLength": 120},
                "source": {"oneOf": [
                    {"type":"object","additionalProperties":false,"required":["kind","path"],
                     "properties":{"kind":{"const":"local"},"path":{"type":"string"}}},
                    {"type":"object","additionalProperties":false,"required":["kind","video_id"],
                     "properties":{"kind":{"const":"youtube"},"video_id":{"type":"string","pattern":"^[A-Za-z0-9_-]{11}$"}}}
                ]},
                "durationMs": {"type": "integer", "minimum": 1, "maximum": u32::MAX},
                "waveform": {"type":"string","pattern":"^[0-9]{96}$","description":"Optional 96 peak levels from the waveform helper, exactly as it returned them."},
                "points": {"type":"array", "minItems":1,"maxItems": POINT_LIMIT, "items":{
                    "type":"object","additionalProperties":false,"required":["startMs","label","quote"],
                    "properties":{"startMs":{"type":"integer","minimum":0},
                                  "label":{"type":"string","maxLength":72},
                                  "quote":{"type":"string","description":
                                      "A source excerpt of up to 420 characters after spaces, tabs and line breaks are collapsed. Other control characters are refused."}}
                }}
            }
        }
    })
}

pub fn call(scope: &question_tools::Scope, args: Value) -> Result<Value, String> {
    let Input { name, source, duration_ms, mut points, waveform } =
        serde_json::from_value(args).map_err(|_| "Give the source, duration and its timestamped moments.".to_string())?;
    if !text(&name, 120) || points.is_empty() || points.len() > POINT_LIMIT
        || points.iter().any(|p| p.start_ms >= duration_ms || !text(&p.label, 72))
    {
        return Err("Give a recording name and one to eight moments with short labels and times inside the recording.".into());
    }
    for (index, point) in points.iter_mut().enumerate() {
        let unsupported = point.quote.chars().any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'));
        let quote = point.quote.split_whitespace().collect::<Vec<_>>().join(" ");
        if unsupported || !text(&quote, QUOTE_LIMIT) {
            return Err(format!("Moment {} needs a source excerpt of 1 to {QUOTE_LIMIT} characters after whitespace is collapsed. Tabs and line breaks are allowed. Remove other control characters.", index + 1));
        }
        point.quote = quote;
    }
    points.sort_by_key(|point| point.start_ms);
    if points.windows(2).any(|pair| pair[0].start_ms == pair[1].start_ms) {
        return Err("Give each moment a distinct starting time.".into());
    }
    if waveform.as_ref().is_some_and(|levels| levels.len() != 96 || !levels.bytes().all(|b| b.is_ascii_digit())) {
        return Err("Use the 96 peak levels from the waveform helper or leave the waveform out.".into());
    }
    let source = source.resolve()?;
    let moments = Moments { duration_ms, points, waveform };
    let ctx = &scope.context;
    let turn_id = ctx.obligation_id.as_ref().unwrap_or(&ctx.turn_id);
    let payload = serde_json::to_vec(&(&ctx.entity_id, &ctx.thread_id, turn_id, &name, &source, &moments))
        .map_err(|_| "The moments could not be prepared.".to_string())?;
    let digest = Sha256::digest(&payload);
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    let row = Row {
        id: format!("mm_{hex}"), entity_id: ctx.entity_id.clone(), thread_id: ctx.thread_id.clone(),
        turn_id: turn_id.clone(), name, at: crate::util::now_millis(), source, moments,
    };
    save(&ctx.root, &row).map_err(|_| "The moments could not be saved.".to_string())?;
    Ok(json!({"shown": true, "id": row.id}))
}

fn path(root: &Path) -> PathBuf {
    root.join(rich_files::DIR).join(RECORD)
}

fn rows(reader: impl BufRead) -> impl Iterator<Item = Row> {
    reader.split(b'\n').map_while(Result::ok).filter_map(|line| serde_json::from_slice(&line).ok())
}

fn save(root: &Path, row: &Row) -> std::io::Result<()> {
    fs::create_dir_all(root.join(rich_files::DIR))?;
    let mut file = OpenOptions::new().create(true).read(true).append(true).mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path(root))?;
    // Closing the file releases the lock, including on an error.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if rows(BufReader::new(&mut file)).any(|existing| existing.id == row.id) {
        return file.sync_data();
    }
    // A partial line from a previous interrupted append cannot swallow the next record.
    crate::util::ensure_line_boundary(&mut file)?;
    let mut line = serde_json::to_vec(row).map_err(std::io::Error::other)?;
    line.push(b'\n');
    file.write_all(&line)?;
    file.sync_data()
}

pub fn all(root: &Path) -> impl Iterator<Item = Row> {
    File::open(path(root)).into_iter().flat_map(|file| rows(BufReader::new(file)))
}

pub fn items(root: &Path, entity: &str, thread: &str) -> Vec<Value> {
    all(root).filter(|row| row.entity_id == entity && row.thread_id == thread).map(|row| row.item()).collect()
}

pub fn locate(root: &Path, entity: &str, thread: &str, id: &str) -> Option<Row> {
    if id.len() != 19 || !id.starts_with("mm_") || !id[3..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    all(root).find(|row| row.id == id && row.entity_id == entity && row.thread_id == thread)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interrupted_unicode_write_cannot_hide_later_moments() {
        let row = Row {
            id: "mm_0000000000000001".into(),
            entity_id: "alpha".into(),
            thread_id: "conversation".into(),
            turn_id: "turn".into(),
            name: "An interview".into(),
            at: 1,
            source: Source::Youtube { video_id: "ab-cDe_F123".into() },
            moments: Moments {
                duration_ms: 100,
                waveform: None,
                points: vec![Point { start_ms: 10, label: "Evidence".into(), quote: "The source.".into() }],
            },
        };
        let mut bytes = b"{\"partial\":\"\xe2\x80\n".to_vec();
        bytes.extend(serde_json::to_vec(&row).unwrap());
        bytes.push(b'\n');
        let ids: Vec<_> = rows(bytes.as_slice()).map(|row| row.id).collect();
        assert_eq!(ids, [row.id]);
    }
}
