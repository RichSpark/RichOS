//! **THE COLLECTORS: A SMALL INCIDENT PACKET, NOT A MACHINE DUMP** (PRD §5).
//!
//! Six categories, one read-only collector each, shared by Claude's support tool ([`crate::mcp`])
//! and the no-AI path. The V1 source map, and nothing else:
//!
//! | Category | Sources |
//! |---|---|
//! | installation | RichDoctor's version; RichOS's Info.plist in `/Applications` and `~/Applications`; macOS version (`SystemVersion.plist`), CPU kind, memory class |
//! | startup_crash | `~/Library/Logs/RichOS/startup.log` and `.log.1`; `launches.json` under `~/Library/Application Support/com.richos.app/`; matching `.ips` reports under `~/Library/Logs/DiagnosticReports/` |
//! | conversation_failure | `conversation-ledger.jsonl` under the app data root: allowlisted fields of `UpstreamFailure`, `TurnInterruptionCause` and `TurnInterrupted` only |
//! | configuration | `config.json`'s schema version and a few current settings, labeled as current |
//! | richscribe | `dictation.log`'s lines in the window (model, timing, errors; it never holds words) |
//! | claude_availability | whether the CLI is found, its version, and whether the default sign-in answers |
//!
//! Nothing under `~/Library/Application Support/RichOS/` (the engine, corpus and loro root) is
//! ever read. Every source is opened read-only, never through RichOS's own store openers
//! (`Ledger::open`, `LaunchStore::open` create and repair), with no migration, repair, rotation
//! or truncation; a link out of its root is refused; a missing root stays missing. Each source
//! has a 5-second read bound and a 2 MiB cap, the whole draft a 10 MiB cap, and every result
//! says what it is: collected, absent, denied, unreadable, unsupported or truncated.
use crate::evidence::{Coverage, Item, Status, REPORT_INPUT_CAP, SOURCE_INPUT_CAP, SOURCE_READ_BOUND};
use crate::scrub::{self, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Installation,
    StartupCrash,
    ConversationFailure,
    Configuration,
    Richscribe,
    ClaudeAvailability,
}

impl Category {
    pub const ALL: [Category; 6] =
        [Category::Installation, Category::StartupCrash, Category::ConversationFailure, Category::Configuration, Category::Richscribe, Category::ClaudeAvailability];

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.as_str() == s)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Installation => "installation",
            Category::StartupCrash => "startup_crash",
            Category::ConversationFailure => "conversation_failure",
            Category::Configuration => "configuration",
            Category::Richscribe => "richscribe",
            Category::ClaudeAvailability => "claude_availability",
        }
    }
    /// Round 39's progress line while this runs.
    pub fn progress(self) -> &'static str {
        match self {
            Category::Installation => "Checking which RichOS is installed…",
            Category::StartupCrash => "Checking what happened when RichOS last opened…",
            Category::ConversationFailure => "Checking what happened in the conversation that stopped…",
            Category::Configuration => "Checking a few RichOS settings…",
            Category::Richscribe => "Checking what RichScribe recorded…",
            Category::ClaudeAvailability => "Checking whether Claude Code is working…",
        }
    }
    /// The folded line once it finished.
    pub fn done(self) -> &'static str {
        match self {
            Category::Installation => "Checked which RichOS is installed",
            Category::StartupCrash => "Checked what happened when RichOS last opened",
            Category::ConversationFailure => "Checked the conversation that stopped",
            Category::Configuration => "Checked a few RichOS settings",
            Category::Richscribe => "Checked what RichScribe recorded",
            Category::ClaudeAvailability => "Checked whether Claude Code is working",
        }
    }
}

/// The incident window (PRD §5): the 30 minutes around the time the person gave, or, when the
/// time is unknown, the last day with the most recent failed attempt preferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start: u64,
    pub end: u64,
    /// Whether the person gave the time.
    pub known: bool,
}

impl Window {
    pub const HALF: u64 = 30 * 60 * 1000;
    pub fn around(at: u64) -> Window {
        Window { start: at.saturating_sub(Self::HALF), end: at + Self::HALF, known: true }
    }
    pub fn unknown(now: u64) -> Window {
        Window { start: now.saturating_sub(24 * 3600 * 1000), end: now + 60_000, known: false }
    }
    /// "Earlier today": from local midnight to now.
    pub fn today(now: u64) -> Window {
        let off = crate::util::local_offset_seconds(now) * 1000;
        let local = now as i64 + off;
        let midnight = (local - local.rem_euclid(86_400_000) - off).max(0) as u64;
        Window { start: midnight, end: now + 60_000, known: true }
    }
    pub fn contains(&self, t: u64) -> bool {
        (self.start..=self.end).contains(&t)
    }
}

/// What a collector may read and for which incident. Written by the host into the draft
/// (`scope.json`) and read by the support server: the model can name a category, never a place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub home: PathBuf,
    pub window: Window,
    /// The resolved Claude Code CLI, when found.
    #[serde(default)]
    pub claude_bin: Option<PathBuf>,
    /// The person allowed diagnostic collection. `false` ("Do not collect diagnostic information
    /// from this Mac") refuses every category but `installation`, whatever Claude asks for.
    #[serde(default = "allowed")]
    pub diagnostics: bool,
}

fn allowed() -> bool {
    true
}

impl Scope {
    fn data_root(&self) -> PathBuf {
        self.home.join("Library/Application Support").join(crate::RICHOS_IDENTIFIER)
    }
    fn logs_root(&self) -> PathBuf {
        self.home.join("Library/Logs/RichOS")
    }
    fn reports_root(&self) -> PathBuf {
        self.home.join("Library/Logs/DiagnosticReports")
    }
}

/// **A SOURCE INSIDE ITS ROOT, OR NOTHING.** The root's real location must be inside the home
/// folder it is named under, and the source's folder inside the root's real location: a link
/// that leads out of either is refused (PRD §5). The last component is opened without following
/// a link ([`crate::util::bounded_tail`]).
fn contained(root: &Path, source: &Path, home: &Path) -> Result<PathBuf, Status> {
    let real_home = std::fs::canonicalize(home).map_err(|e| Status::from_io(&e))?;
    let real_root = std::fs::canonicalize(root).map_err(|e| Status::from_io(&e))?;
    if !real_root.starts_with(&real_home) || !real_root.ends_with(root.strip_prefix(home).unwrap_or(root)) {
        return Err(Status::Unsupported);
    }
    let parent = source.parent().ok_or(Status::Unreadable)?;
    let real_parent = std::fs::canonicalize(parent).map_err(|e| Status::from_io(&e))?;
    if !real_parent.starts_with(&real_root) {
        return Err(Status::Unsupported);
    }
    Ok(real_parent.join(source.file_name().ok_or(Status::Unreadable)?))
}

/// A bounded read of one source, counted against the draft's remaining budget.
struct Read {
    text: String,
    truncated: bool,
    bytes: u64,
}

fn read_source(root: &Path, source: &Path, scope: &Scope, budget: &mut u64, from_end: bool) -> Result<Read, Status> {
    let path = contained(root, source, &scope.home)?;
    let cap = SOURCE_INPUT_CAP.min(*budget);
    if cap == 0 {
        return Err(Status::Truncated);
    }
    let read = if from_end { crate::util::bounded_tail(&path, cap, SOURCE_READ_BOUND) } else { crate::util::bounded_head(&path, cap, SOURCE_READ_BOUND) }.map_err(|e| {
        if e.raw_os_error() == Some(libc::ELOOP) {
            Status::Unsupported
        } else {
            Status::from_io(&e)
        }
    })?;
    *budget -= read.bytes.len() as u64;
    Ok(Read { truncated: read.truncated || cap < SOURCE_INPUT_CAP, bytes: read.bytes.len() as u64, text: String::from_utf8_lossy(&read.bytes).into_owned() })
}

/// A plain sentence for a source that could not be read.
fn missing_note(status: Status, what: &str) -> String {
    match status {
        Status::Absent => format!("{what} was not on this Mac."),
        Status::Denied => format!("macOS did not let RichDoctor read {}.", what.to_lowercase()),
        Status::Unsupported => format!("{what} was in a form RichDoctor cannot read safely, so it was left out."),
        Status::Truncated => format!("{what} was left out: the report's size limit for diagnostic information was reached."),
        _ => format!("{what} could not be read."),
    }
}

fn unread(category: Category, source: &str, status: Status) -> Item {
    let mut item = Item::new(category, source).expect("random source");
    item.status = status;
    item.note = missing_note(status, source);
    item.summary = item.note.clone();
    item
}

/// Finish an item: scrub its text, count what was removed, set its status.
fn finish(mut item: Item, text: &str, ctx: &Context, truncated: bool) -> Item {
    let (clean, removed) = scrub::scrub(text.trim_end(), ctx);
    item.text = clean;
    item.removed = removed;
    item.status = if truncated { Status::Truncated } else { Status::Collected };
    if truncated && item.note.is_empty() {
        item.note = "Only the most recent part was read, to stay within the size limit.".into();
    }
    item
}

/// **COLLECT ONE CATEGORY.** `used` is the input the draft has already read; the return value
/// is the items, every one sanitized.
pub fn collect(category: Category, scope: &Scope, ctx: &Context, used: u64) -> Vec<Item> {
    let mut budget = REPORT_INPUT_CAP.saturating_sub(used);
    match category {
        Category::Installation => vec![installation(scope, ctx)],
        Category::StartupCrash => startup_crash(scope, ctx, &mut budget),
        Category::ConversationFailure => vec![conversation_failure(scope, ctx, &mut budget)],
        Category::Configuration => vec![configuration(scope, ctx, &mut budget)],
        Category::Richscribe => vec![richscribe(scope, ctx, &mut budget)],
        Category::ClaudeAvailability => vec![claude_availability(scope)],
    }
}

// ---------------------------------------------------------------------------------------
// installation
// ---------------------------------------------------------------------------------------

fn sysctl_u64(name: &str) -> Option<u64> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: sysctlbyname writes at most `size` bytes into `value`.
    let rc = unsafe { libc::sysctlbyname(name.as_ptr(), &mut value as *mut u64 as *mut libc::c_void, &mut size, std::ptr::null_mut(), 0) };
    (rc == 0).then_some(if size == 4 { value & 0xffff_ffff } else { value })
}

/// "Apple silicon" or "Intel": the Mac's own CPU, also when RichDoctor runs translated.
pub fn cpu_kind() -> &'static str {
    if sysctl_u64("hw.optional.arm64") == Some(1) || std::env::consts::ARCH == "aarch64" { "Apple silicon" } else { "Intel" }
}

/// "16 GB": the memory, rounded to a size class, never an exact figure.
pub fn memory_class() -> Option<String> {
    let bytes = sysctl_u64("hw.memsize")?;
    let gb = (bytes as f64 / (1u64 << 30) as f64).round() as u64;
    Some(format!("{gb} GB"))
}

/// macOS's version, from its own `SystemVersion.plist` (no program run).
pub fn macos_version() -> Option<String> {
    let read = crate::util::bounded_head(Path::new("/System/Library/CoreServices/SystemVersion.plist"), 64 * 1024, SOURCE_READ_BOUND).ok()?;
    let dict = plist::from_bytes::<plist::Value>(&read.bytes).ok()?.into_dictionary()?;
    dict.get("ProductVersion")?.as_string().map(str::to_string)
}

fn installation(scope: &Scope, ctx: &Context) -> Item {
    let mut item = Item::new(Category::Installation, "Installation").expect("random source");
    item.label = "which RichOS is installed".into();
    item.coverage = Coverage::Current;
    let found = crate::install::richos(&scope.home);
    let macos = macos_version();
    let memory = memory_class();
    let mut lines = vec![format!("RichDoctor {}", crate::VERSION)];
    lines.push(format!("macOS {} · {}{}", macos.as_deref().unwrap_or("version unknown"), cpu_kind(), memory.as_ref().map(|m| format!(" · {m} memory")).unwrap_or_default()));
    if found.is_empty() {
        lines.push("RichOS: not found in Applications or in the user Applications folder".into());
    }
    for f in &found {
        let what = match (&f.version, f.identity_ok) {
            (Some(v), true) => format!("RichOS {v}{}", f.build.as_ref().filter(|b| *b != v).map(|b| format!(" (build {b})")).unwrap_or_default()),
            (Some(v), false) => format!("an app named RichOS.app, version {v}, that does not identify as RichOS"),
            (None, _) => "RichOS, version unreadable".into(),
        };
        lines.push(format!("{what} in {}", f.place.words()));
    }
    item.facts = serde_json::json!({
        "richdoctor": crate::VERSION, "macos": macos, "cpu": cpu_kind(), "memory": memory,
        "richos": found, "line": crate::install::installation_line(&found), "ambiguous": crate::install::ambiguous(&found),
    });
    item.summary = lines[1..].join("; ");
    finish(item, &lines.join("\n"), ctx, false)
}

// ---------------------------------------------------------------------------------------
// startup_crash
// ---------------------------------------------------------------------------------------

/// One `startup.log` entry (`startup_alert.rs::log_entry`'s shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupEntry {
    pub at: Option<u64>,
    pub header: String,
    pub body: String,
}

/// The entries of a startup log, in order. Lines before the first header are kept as an entry
/// with no time, so nothing is silently dropped.
pub fn startup_entries(text: &str) -> Vec<StartupEntry> {
    let mut out: Vec<StartupEntry> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("==== ").and_then(|l| l.strip_suffix(" ====")) {
            let at = h.split_whitespace().next().and_then(crate::util::parse_utc_stamp);
            out.push(StartupEntry { at, header: h.to_string(), body: String::new() });
        } else if !line.trim().is_empty() {
            if out.is_empty() {
                out.push(StartupEntry { at: None, header: String::new(), body: String::new() });
            }
            let e = out.last_mut().unwrap();
            if !e.body.is_empty() {
                e.body.push('\n');
            }
            e.body.push_str(line);
        }
    }
    out
}

/// Entries inside the window, or, when none are, the last `fallback` of them.
fn pick<T: Clone>(all: &[T], at: impl Fn(&T) -> Option<u64>, window: &Window, fallback: usize) -> (Vec<T>, Coverage) {
    let inside: Vec<T> = all.iter().filter(|e| at(e).is_some_and(|t| window.contains(t))).cloned().collect();
    if !inside.is_empty() {
        return (inside, Coverage::InWindow);
    }
    let n = all.len();
    (all[n.saturating_sub(fallback)..].to_vec(), if n == 0 { Coverage::Unknown } else { Coverage::MostRecent })
}

fn when_words(times: &[u64]) -> String {
    let clocks: Vec<String> = times.iter().map(|t| crate::util::local_clock(*t)).collect();
    match clocks.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn startup_crash(scope: &Scope, ctx: &Context, budget: &mut u64) -> Vec<Item> {
    let mut items = Vec::new();
    // The startup log and its one older generation.
    let logs = scope.logs_root();
    let mut entries = Vec::new();
    let (mut collected, mut worst) = (false, None);
    let (mut bytes, mut truncated) = (0u64, false);
    for name in ["startup.log.1", "startup.log"] {
        match read_source(&logs, &logs.join(name), scope, budget, true) {
            Ok(r) => {
                bytes += r.bytes;
                truncated |= r.truncated;
                entries.extend(startup_entries(&r.text));
                collected = true;
            }
            // A missing older generation is normal; any other failure is what is reported.
            Err(Status::Absent) => {}
            Err(s) => worst = Some(s),
        }
    }
    if collected {
        let (picked, coverage) = pick(&entries, |e| e.at, &scope.window, 2);
        let mut item = Item::new(Category::StartupCrash, "Startup log").expect("random source");
        item.coverage = coverage;
        item.input_bytes = bytes;
        item.observed_at = picked.iter().filter_map(|e| e.at).max();
        let times: Vec<u64> = picked.iter().filter_map(|e| e.at).collect();
        item.label = if times.is_empty() { "the startup log".into() } else { format!("the startup log from {}", when_words(&times)) };
        item.summary = match (picked.len(), coverage) {
            (0, _) => "The startup log holds no recorded failures.".into(),
            (n, Coverage::InWindow) => format!("{n} recorded startup failure(s) in the incident window, at {}.", when_words(&times)),
            (n, _) => format!("No startup failure was recorded in the incident window; the {n} most recent are shown, at {}.", when_words(&times)),
        };
        let text: Vec<String> = picked.iter().map(|e| if e.header.is_empty() { e.body.clone() } else { format!("{}\n{}", e.header, e.body) }).collect();
        if picked.is_empty() {
            item.note = "The startup log was read and holds no failure records. It is written only when a start fails before the window opens.".into();
        }
        items.push(finish(item, &text.join("\n"), ctx, truncated));
    } else {
        items.push(unread(Category::StartupCrash, "Startup log", worst.unwrap_or(Status::Absent)));
    }
    items.push(launches(scope, ctx, budget));
    items.extend(crash_reports(scope, ctx, budget));
    items
}

fn launches(scope: &Scope, ctx: &Context, budget: &mut u64) -> Item {
    let root = scope.data_root();
    let source = "Launch record";
    let r = match read_source(&root, &root.join("launches.json"), scope, budget, false) {
        Ok(r) => r,
        Err(s) => return unread(Category::StartupCrash, source, s),
    };
    let v: serde_json::Value = match serde_json::from_str(&r.text) {
        Ok(v) if !r.truncated => v,
        _ => {
            let mut item = unread(Category::StartupCrash, source, Status::Unsupported);
            item.input_bytes = r.bytes;
            item.note = "The launch record could not be read: it is not in a form RichDoctor knows.".into();
            item.summary = item.note.clone();
            return item;
        }
    };
    let mut item = Item::new(Category::StartupCrash, source).expect("random source");
    item.input_bytes = r.bytes;
    item.label = "the launch record".into();
    if v["schema_version"].as_u64() != Some(1) {
        item.status = Status::Unsupported;
        item.note = format!("The launch record is format {}, which this RichDoctor does not read.", v["schema_version"]);
        item.summary = item.note.clone();
        return item;
    }
    let starts: Vec<u64> = v["starts"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64()).collect()).unwrap_or_default();
    let (picked, coverage) = pick(&starts, |t| Some(*t), &scope.window, 3);
    item.coverage = coverage;
    item.observed_at = picked.last().copied();
    let mut lines = Vec::new();
    lines.push(match coverage {
        Coverage::InWindow => format!("Recorded starts in the incident window: {}", picked.iter().map(|t| crate::util::utc_stamp(*t)).collect::<Vec<_>>().join(", ")),
        _ if starts.is_empty() => "No start has been recorded.".into(),
        _ => format!("No start recorded in the incident window. Most recent recorded starts: {}", picked.iter().map(|t| crate::util::utc_stamp(*t)).collect::<Vec<_>>().join(", ")),
    });
    match v["open_run"]["started_at"].as_u64() {
        Some(t) => lines.push(format!("The run that started at {} did not record a clean quit (it may still be running or it ended without quitting).", crate::util::utc_stamp(t))),
        None => lines.push("The last run recorded a clean quit.".into()),
    }
    lines.push("A start is recorded only after a quit; a restart after a crash is not counted.".into());
    item.summary = lines[0].clone();
    finish(item, &lines.join("\n"), ctx, false)
}

/// The crash report's own two documents: the one-line header and the body.
fn ips_documents(text: &str) -> Option<(serde_json::Value, serde_json::Value)> {
    let (head, body) = text.split_once('\n')?;
    Some((serde_json::from_str(head).ok()?, serde_json::from_str(body).ok()?))
}

/// "2026-10-22 08:43:11.00 -0700" -> epoch millis.
fn ips_time(s: &str) -> Option<u64> {
    let mut parts = s.split_whitespace();
    let (date, time, zone) = (parts.next()?, parts.next()?, parts.next()?);
    let time = time.split('.').next()?;
    let utc = crate::util::parse_utc_stamp(&format!("{date}T{time}Z"))? as i64;
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let z = zone.trim_start_matches(['+', '-']);
    let (h, m) = (z.get(0..2)?.parse::<i64>().ok()?, z.get(2..4)?.parse::<i64>().ok()?);
    Some((utc - sign * (h * 3600 + m * 60) * 1000) as u64)
}

/// The fields of one RichOS crash report worth an engineer's time, and none of its paths,
/// other processes' images or registers.
pub fn crash_summary(header: &serde_json::Value, body: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} {} (build {}) on {}",
        header["app_name"].as_str().unwrap_or("RichOS"),
        header["app_version"].as_str().unwrap_or("?"),
        header["build_version"].as_str().unwrap_or("?"),
        header["os_version"].as_str().unwrap_or("?")
    ));
    if let Some(t) = header["timestamp"].as_str() {
        lines.push(format!("Crashed: {t}"));
    }
    let ex = &body["exception"];
    if !ex.is_null() {
        lines.push(format!("Exception: {} {}", ex["type"].as_str().unwrap_or(""), ex["signal"].as_str().unwrap_or("")).trim_end().to_string());
    }
    let term = &body["termination"];
    if !term.is_null() {
        lines.push(format!("Termination: {} {}", term["namespace"].as_str().unwrap_or(""), term["indicator"].as_str().unwrap_or("")).trim_end().to_string());
    }
    if let Some(asi) = body["asi"].as_object() {
        for (image, messages) in asi.iter().take(4) {
            for m in messages.as_array().into_iter().flatten().filter_map(|m| m.as_str()).take(3) {
                lines.push(format!("{image}: {}", m.chars().take(500).collect::<String>()));
            }
        }
    }
    let images: Vec<String> = body["usedImages"].as_array().map(|a| a.iter().map(|i| i["name"].as_str().unwrap_or("?").to_string()).collect()).unwrap_or_default();
    let faulting = body["faultingThread"].as_u64().unwrap_or(0) as usize;
    if let Some(frames) = body["threads"].get(faulting).and_then(|t| t["frames"].as_array()) {
        lines.push(format!("Crashed thread {faulting}, top frames:"));
        for f in frames.iter().take(12) {
            let image = f["imageIndex"].as_u64().and_then(|i| images.get(i as usize)).cloned().unwrap_or_else(|| "?".into());
            let symbol = f["symbol"].as_str().map(|s| s.chars().take(160).collect::<String>()).unwrap_or_else(|| format!("+{}", f["imageOffset"]));
            lines.push(format!("  {image}  {symbol}"));
        }
    }
    lines.join("\n")
}

fn crash_reports(scope: &Scope, ctx: &Context, budget: &mut u64) -> Vec<Item> {
    let root = scope.reports_root();
    let source = "Crash record";
    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(e) => {
            let status = Status::from_io(&e);
            let mut item = unread(Category::StartupCrash, source, status);
            if status == Status::Absent {
                item.note = "No crash record for RichOS was on this Mac.".into();
                item.summary = item.note.clone();
            }
            return vec![item];
        }
    };
    // Only RichOS's own reports are opened: a name pre-filter, then the report's own identity.
    let mut candidates: Vec<(u64, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            (n.starts_with("richos") || n.starts_with("richos-tauri")) && n.ends_with(".ips")
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64, e.path())))
        .collect();
    candidates.sort();
    let mut reports = Vec::new();
    for (_, path) in candidates.iter().rev().take(10) {
        let Ok(r) = read_source(&root, path, scope, budget, false) else { continue };
        let Some((header, body)) = ips_documents(&r.text) else { continue };
        if header["bundleID"].as_str() != Some(crate::RICHOS_IDENTIFIER) {
            continue;
        }
        let at = header["timestamp"].as_str().and_then(ips_time);
        reports.push((at, crash_summary(&header, &body), r.bytes));
    }
    reports.reverse();
    let (picked, coverage) = pick(&reports, |r| r.0, &scope.window, 1);
    if picked.is_empty() {
        let mut item = unread(Category::StartupCrash, source, Status::Absent);
        item.note = "No crash record for RichOS was on this Mac.".into();
        item.summary = item.note.clone();
        return vec![item];
    }
    picked
        .into_iter()
        .map(|(at, text, bytes)| {
            let mut item = Item::new(Category::StartupCrash, source).expect("random source");
            item.coverage = coverage;
            item.observed_at = at;
            item.input_bytes = bytes;
            item.label = at.map(|t| format!("the crash record from {}", crate::util::local_clock(t))).unwrap_or_else(|| "the crash record".into());
            item.summary = format!("A RichOS crash record{}.", at.map(|t| format!(" from {}", crate::util::local_clock(t))).unwrap_or_default());
            finish(item, &text, ctx, false)
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// conversation_failure
// ---------------------------------------------------------------------------------------

/// One allowlisted ledger record, as it may be shown. Only these fields ever leave the
/// ledger: no message, prompt, tool payload, conversation name, turn or request id.
pub fn ledger_line(line: &str) -> Option<(u64, String)> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let at = v["at"].as_u64()?;
    let bounded = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let shown = match v["event"].as_str()? {
        "UpstreamFailure" => {
            let r = &v["record"];
            let mut s = format!("upstream failure: {}", r["fault"].as_str().unwrap_or("unknown"));
            if let Some(status) = r["status"].as_u64() {
                s.push_str(&format!(", status {status}"));
            }
            if let Some(m) = r["ceo_message"].as_str() {
                s.push_str(&format!(" · shown: \"{}\"", bounded(m, 300)));
            }
            s
        }
        "TurnInterruptionCause" => {
            let r = &v["record"];
            format!("interruption cause: {}{}", r["cause"].as_str().unwrap_or("unknown"), if r["offers_retry"].as_bool() == Some(true) { " (retry offered)" } else { "" })
        }
        "TurnInterrupted" => format!("turn interrupted: {}{}", bounded(v["reason"].as_str().unwrap_or(""), 300), if v["recovered_after_restart"].as_bool() == Some(true) { " (found after a restart)" } else { "" }),
        _ => return None,
    };
    Some((at, format!("{} {shown}", crate::util::utc_stamp(at))))
}

fn conversation_failure(scope: &Scope, ctx: &Context, budget: &mut u64) -> Item {
    let root = scope.data_root();
    let source = "Conversation record";
    let r = match read_source(&root, &root.join("conversation-ledger.jsonl"), scope, budget, true) {
        Ok(r) => r,
        Err(s) => return unread(Category::ConversationFailure, source, s),
    };
    let records: Vec<(u64, String)> = r.text.lines().filter_map(ledger_line).collect();
    let (picked, coverage) = pick(&records, |x| Some(x.0), &scope.window, 6);
    let mut item = Item::new(Category::ConversationFailure, source).expect("random source");
    item.input_bytes = r.bytes;
    item.coverage = coverage;
    item.observed_at = picked.last().map(|x| x.0);
    item.label = "the conversation's failure records".into();
    item.summary = match (picked.len(), coverage) {
        (0, _) => "No failure or interruption is recorded in the conversation record.".into(),
        (n, Coverage::InWindow) => format!("{n} failure or interruption record(s) in the incident window."),
        (n, _) => format!("None in the incident window; the {n} most recent failure or interruption records are shown."),
    };
    if picked.is_empty() {
        item.note = "The conversation record was read: it holds no failure or interruption record in what was read.".into();
    }
    let text: Vec<String> = picked.into_iter().map(|x| x.1).collect();
    finish(item, &text.join("\n"), ctx, r.truncated)
}

// ---------------------------------------------------------------------------------------
// configuration
// ---------------------------------------------------------------------------------------

fn configuration(scope: &Scope, ctx: &Context, budget: &mut u64) -> Item {
    let root = scope.data_root();
    let source = "RichOS settings";
    let r = match read_source(&root, &root.join("config.json"), scope, budget, false) {
        Ok(r) => r,
        Err(s) => return unread(Category::Configuration, source, s),
    };
    let mut item = Item::new(Category::Configuration, source).expect("random source");
    item.input_bytes = r.bytes;
    item.coverage = Coverage::Current;
    item.label = "a few RichOS settings".into();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&r.text) else {
        item.status = Status::Unreadable;
        item.note = "RichOS's settings file could not be read: it is damaged or incomplete.".into();
        item.summary = item.note.clone();
        return item;
    };
    let onoff = |b: Option<bool>| b.map(|b| if b { "on" } else { "off" }).unwrap_or("not set");
    let lines = [
        format!("Settings format: {}", v["schema_version"].as_u64().map(|n| n.to_string()).unwrap_or_else(|| "not stated (an early format)".into())),
        format!("Technical view, default for conversations: {}", onoff(v["techy_default"].as_bool())),
        format!("Theme: {}", v["theme"].as_str().unwrap_or("not set (follows the Mac)")),
        format!("Text size: {}%", v["font_scale"].as_u64().unwrap_or(100)),
        format!("Opening screen: {}", onoff(v["splash_enabled"].as_bool().or(Some(true)))),
        "These are the settings as they are now. They do not show what they were when the problem happened.".to_string(),
    ];
    item.summary = "Current settings (not a history).".into();
    finish(item, &lines.join("\n"), ctx, false)
}

// ---------------------------------------------------------------------------------------
// richscribe
// ---------------------------------------------------------------------------------------

fn richscribe(scope: &Scope, ctx: &Context, budget: &mut u64) -> Item {
    let root = scope.data_root();
    let source = "RichScribe log";
    let r = match read_source(&root, &root.join("dictation.log"), scope, budget, true) {
        Ok(r) => r,
        Err(s) => return unread(Category::Richscribe, source, s),
    };
    let lines: Vec<(Option<u64>, String)> = r.text.lines().filter(|l| !l.trim().is_empty()).map(|l| (l.split_whitespace().next().and_then(crate::util::parse_utc_stamp), l.chars().take(400).collect())).collect();
    let (picked, coverage) = pick(&lines, |x| x.0, &scope.window, 20);
    let picked: Vec<(Option<u64>, String)> = picked.into_iter().rev().take(200).rev().collect();
    let mut item = Item::new(Category::Richscribe, source).expect("random source");
    item.input_bytes = r.bytes;
    item.coverage = coverage;
    item.observed_at = picked.iter().filter_map(|x| x.0).max();
    item.label = "the RichScribe log".into();
    item.summary = format!("{} RichScribe log line(s){}.", picked.len(), if coverage == Coverage::InWindow { " in the incident window" } else { ", the most recent" });
    let text: Vec<String> = picked.into_iter().map(|x| x.1).collect();
    finish(item, &text.join("\n"), ctx, r.truncated)
}

// ---------------------------------------------------------------------------------------
// claude_availability
// ---------------------------------------------------------------------------------------

/// Claude Code's own version line, run with an environment built from nothing and bounded.
fn claude_version(bin: &Path) -> Option<String> {
    use std::io::Read as _;
    let mut command = std::process::Command::new(bin);
    command.arg("--version").env_clear().envs(crate::adapter::environment(|k| std::env::var(k).ok())).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    richos_core::owned_process::OwnedChild::configure(&mut command);
    let mut raw = command.spawn().ok()?;
    let mut out = raw.stdout.take()?;
    let mut child = richos_core::owned_process::OwnedChild::new(raw);
    let reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        (&mut out).take(4096).read_to_end(&mut s).ok();
        s
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(20)),
            _ => {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
        }
    }
    let s = String::from_utf8_lossy(&reader.join().ok()?).trim().to_string();
    (!s.is_empty()).then(|| s.chars().take(80).collect())
}

/// The default sign-in, as `claude auth status` answers it, in RichDoctor's words. Only a
/// confirmed negative is "signed out" (PRD §4).
pub fn sign_in_words(state: &richos_core::provider_auth::AuthState) -> &'static str {
    use richos_core::provider_auth::AuthState;
    match state {
        AuthState::Connected => "the default sign-in answered as signed in",
        AuthState::SignedOut => "the default sign-in answered as signed out",
        AuthState::Unavailable => "Claude Code could not be run",
        _ => "RichDoctor could not check the default sign-in",
    }
}

fn claude_availability(scope: &Scope) -> Item {
    let mut item = Item::new(Category::ClaudeAvailability, "Claude Code").expect("random source");
    item.coverage = Coverage::Current;
    item.label = "whether Claude Code is working".into();
    let Some(bin) = scope.claude_bin.as_ref().filter(|b| b.is_file()) else {
        item.status = Status::Collected;
        item.text = "Claude Code: not found in the places it is normally installed.".into();
        item.summary = item.text.clone();
        return item;
    };
    let version = claude_version(bin);
    // The CLI's DEFAULT configuration: `None` folder, and RichDoctor removed CLAUDE_CONFIG_DIR
    // from its own environment at startup, so status_in cannot inherit one.
    let auth = richos_core::provider_auth::status_in(bin, None);
    let lines = [format!("Claude Code: found, version {}", version.as_deref().unwrap_or("unknown")), format!("Sign-in: {}", sign_in_words(&auth.state))];
    item.summary = lines.join("; ");
    item.text = lines.join("\n");
    item.status = Status::Collected;
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Home(PathBuf);
    impl Drop for Home {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }
    fn home() -> Home {
        let h = std::env::temp_dir().join(format!("rd-collect-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap())));
        std::fs::create_dir_all(&h).unwrap();
        Home(std::fs::canonicalize(&h).unwrap())
    }
    fn scope(home: &Path, window: Window) -> Scope {
        Scope { home: home.to_path_buf(), window, claude_bin: None, diagnostics: true }
    }
    const T: u64 = 1_792_658_590_000; // 2026-10-22T08:43:10Z

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn startup_log_entries_in_the_window_are_collected_and_scrubbed() {
        let h = home();
        let log = format!(
            "==== {} RichOS 1.2.0 pid 10 ====\nexecutable: /Applications/RichOS.app/Contents/MacOS/richos-tauri\nfailure: old\n\n==== {} RichOS 1.2.1 pid 11 ====\nexecutable: {}/Applications/RichOS.app/Contents/MacOS/richos-tauri\nfailure: the engine did not start\n\n",
            crate::util::utc_stamp(T - 86_400_000),
            crate::util::utc_stamp(T),
            h.0.display()
        );
        write(&h.0.join("Library/Logs/RichOS/startup.log"), &log);
        let mut ctx = Context::with(h.0.file_name().unwrap().to_string_lossy().into(), vec![]);
        ctx.home = h.0.display().to_string();
        let items = collect(Category::StartupCrash, &scope(&h.0, Window::around(T)), &ctx, 0);
        let log = &items[0];
        assert_eq!(log.status, Status::Collected);
        assert_eq!(log.coverage, Coverage::InWindow);
        assert!(log.text.contains("failure: the engine did not start") && !log.text.contains("failure: old"), "{}", log.text);
        assert!(!log.text.contains(&h.0.display().to_string()) && log.text.contains("[path removed]"), "{}", log.text);
        assert_eq!(log.removed.paths, 1);
        // No launch record and no crash report: explicit omissions, never "no errors".
        assert_eq!(items[1].status, Status::Absent);
        assert_eq!(items[2].status, Status::Absent);
        assert!(items[2].note.contains("No crash record"));
    }

    #[test]
    fn a_missing_root_stays_missing_and_nothing_is_created() {
        let h = home();
        for c in Category::ALL {
            let items = collect(c, &scope(&h.0, Window::unknown(T)), &Context::default(), 0);
            assert!(!items.is_empty(), "{c:?}");
        }
        assert!(!h.0.join("Library/Application Support").exists(), "a collector created a folder");
        assert!(!h.0.join("Library/Logs/RichOS").exists());
    }

    #[test]
    fn a_link_out_of_the_root_is_refused() {
        let h = home();
        let outside = h.0.join("outside");
        write(&outside.join("startup.log"), "==== 2026-10-22T08:43:10Z RichOS 1 pid 1 ====\nfailure: secret place\n");
        std::fs::create_dir_all(h.0.join("Library/Logs")).unwrap();
        std::os::unix::fs::symlink(&outside, h.0.join("Library/Logs/RichOS")).unwrap();
        let items = collect(Category::StartupCrash, &scope(&h.0, Window::around(T)), &Context::default(), 0);
        assert_eq!(items[0].status, Status::Unsupported);
        assert!(items[0].text.is_empty());
        // And a link as the file itself.
        std::fs::remove_file(h.0.join("Library/Logs/RichOS")).unwrap();
        std::fs::create_dir_all(h.0.join("Library/Logs/RichOS")).unwrap();
        std::os::unix::fs::symlink(outside.join("startup.log"), h.0.join("Library/Logs/RichOS/startup.log")).unwrap();
        let items = collect(Category::StartupCrash, &scope(&h.0, Window::around(T)), &Context::default(), 0);
        assert_eq!(items[0].status, Status::Unsupported);
    }

    #[test]
    fn the_ledger_yields_only_allowlisted_fields() {
        let h = home();
        let lines = [
            format!(r#"{{"event":"PromptSubmitted","turn_id":"t1","text":"my private plan for Acme","at":{T}}}"#),
            format!(r#"{{"event":"UpstreamFailure","turn_id":"t1","record":{{"fault":"overloaded","status":529,"request_id":"req_123","model":"x","ceo_message":"Anthropic is overloaded.","loss_message":"Your words are saved."}},"at":{T}}}"#),
            format!(r#"{{"event":"TurnInterruptionCause","turn_id":"t1","record":{{"cause":"upstream","ceo_message":"m","offers_retry":true}},"at":{T}}}"#),
            "not json at all".to_string(),
        ];
        write(&h.0.join("Library/Application Support/com.richos.app/conversation-ledger.jsonl"), &lines.join("\n"));
        let before = std::fs::read(h.0.join("Library/Application Support/com.richos.app/conversation-ledger.jsonl")).unwrap();
        let item = &collect(Category::ConversationFailure, &scope(&h.0, Window::around(T)), &Context::default(), 0)[0];
        assert_eq!(item.status, Status::Collected);
        assert!(item.text.contains("upstream failure: overloaded, status 529") && item.text.contains("interruption cause: upstream (retry offered)"), "{}", item.text);
        for leaked in ["private plan", "Acme", "req_123", "t1", "Your words are saved"] {
            assert!(!item.text.contains(leaked), "{leaked} leaked: {}", item.text);
        }
        assert_eq!(std::fs::read(h.0.join("Library/Application Support/com.richos.app/conversation-ledger.jsonl")).unwrap(), before, "the ledger is byte-identical");
    }

    #[test]
    fn settings_are_current_not_history_and_a_damaged_file_says_so() {
        let h = home();
        let path = h.0.join("Library/Application Support/com.richos.app/config.json");
        write(&path, r#"{"schema_version":1,"techy_default":true,"theme":"dark","company_name":"SecretCo","user_name":"Jane Doe"}"#);
        let item = &collect(Category::Configuration, &scope(&h.0, Window::unknown(T)), &Context::default(), 0)[0];
        assert!(item.text.contains("Technical view, default for conversations: on") && item.text.contains("as they are now"), "{}", item.text);
        assert!(!item.text.contains("SecretCo") && !item.text.contains("Jane"));
        write(&path, "{\"schema_version\":1,");
        assert_eq!(collect(Category::Configuration, &scope(&h.0, Window::unknown(T)), &Context::default(), 0)[0].status, Status::Unreadable);
    }

    #[test]
    fn the_report_input_cap_is_honored() {
        let h = home();
        write(&h.0.join("Library/Application Support/com.richos.app/dictation.log"), &format!("{} dictation: model small.en\n", crate::util::utc_stamp(T)).repeat(10));
        let item = &collect(Category::Richscribe, &scope(&h.0, Window::around(T)), &Context::default(), REPORT_INPUT_CAP)[0];
        assert_eq!(item.status, Status::Truncated);
        assert!(item.text.is_empty());
        let item = &collect(Category::Richscribe, &scope(&h.0, Window::around(T)), &Context::default(), 0)[0];
        assert_eq!(item.status, Status::Collected);
        assert_eq!(item.text.lines().count(), 10);
    }

    #[test]
    fn a_crash_report_is_read_only_when_it_is_richos_own() {
        let h = home();
        let dir = h.0.join("Library/Logs/DiagnosticReports");
        let header = r#"{"app_name":"RichOS","timestamp":"2026-10-22 08:43:11.00 +0000","app_version":"1.2.1","build_version":"1.2.1","bundleID":"com.richos.app","os_version":"macOS 15.6 (24G84)"}"#;
        let body = r#"{"exception":{"type":"EXC_CRASH","signal":"SIGABRT"},"termination":{"namespace":"SIGNAL","indicator":"Abort trap: 6"},"asi":{"libsystem_c.dylib":["abort() called"]},"faultingThread":0,"threads":[{"frames":[{"imageIndex":0,"symbol":"__pthread_kill"},{"imageIndex":1,"symbol":"richos_tauri::main"}]}],"usedImages":[{"name":"libsystem_kernel.dylib","path":"/usr/lib/system/libsystem_kernel.dylib"},{"name":"richos-tauri","path":"/Users/jdoe/Applications/RichOS.app/Contents/MacOS/richos-tauri"}]}"#;
        write(&dir.join("RichOS-2026-10-22-084311.ips"), &format!("{header}\n{body}"));
        write(&dir.join("RichOS-2026-10-22-084400.ips"), &format!("{}\n{body}", header.replace("com.richos.app", "com.other.app")));
        write(&dir.join("Safari-2026-10-22-084311.ips"), "x");
        let items = collect(Category::StartupCrash, &scope(&h.0, Window::around(T)), &Context::default(), 0);
        let crash: Vec<&Item> = items.iter().filter(|i| i.source == "Crash record").collect();
        assert_eq!(crash.len(), 1);
        assert_eq!(crash[0].observed_at, Some(T + 1000));
        assert!(crash[0].text.contains("Exception: EXC_CRASH SIGABRT") && crash[0].text.contains("richos-tauri  richos_tauri::main"), "{}", crash[0].text);
        assert!(!crash[0].text.contains("/Users/jdoe"));
    }
}
