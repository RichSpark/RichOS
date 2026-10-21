//! THE LEAD CLAIM, APP SIDE — his terminal and the app never run his team at once (operator
//! back-end spec r3 (e) "The claim", items 1-7, with Frank's G11 and r4 §2.4; the engine side is
//! zach-opus-openg1's `scripts/lib/operator_leads.py`, and the file contract both sides build to
//! is richos-hq `docs/verification/2026-09-24-operator-fences/engine-rest-2026-09-25.md` §2.1).
//!
//! **The file** is `<claude dir>/state/operator-lead.json`, written only under `flock` on
//! `operator-lead.lock` beside it, by temporary file and rename:
//!
//! ```json
//! {"schema": 1, "owner": "app", "claim_id": "<RICHOS_OPERATOR_LEAD>", "claimed_at": "<ISO UTC>",
//!  "processes": [{"role": "app|supervisor|lead", "pid": 123, "start": 1790300000}],
//!  "leads": [{"pid": 124, "start": 1790300001, "session_id": "<uuid>", "title": "<conversation>"}]}
//! ```
//!
//! `start` is the kernel's process start in whole epoch seconds (`proc_pidinfo`), never the
//! `procStart` string. A claim is **live** while any listed process runs with its recorded start.
//!
//! **The terminal test is G11's, as the engine side built it** (contract §2.1): a live session
//! record whose `entrypoint` is non-empty and does not start with `sdk-`, whose `cwd` is the
//! entity root or under it, and not under `<root>/.claude/worktrees/`. A print-mode `claude` —
//! every app lead, and one started from a terminal's own tool shell — records `sdk-cli` (P2),
//! so it is never the terminal. It is never `kind` (r3 F6): `kind` is `interactive` for print
//! mode too.
//!
//! **What this side never does.** It never writes into `<claude dir>/sessions/`, which Claude
//! Code owns (r3 F6). It never signals anything. It never takes a claim from a live terminal, and
//! it treats an unreadable claim as held (r3 (e) item 6), with a sentence naming the way through.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const CLAIM_SCHEMA: u64 = 1;
/// The engine side's wait for the same lock (`operator_leads.py` `CLAIM_LOCK_WAIT`).
pub const CLAIM_LOCK_WAIT: Duration = Duration::from_secs(5);
/// The engine side refuses a larger claim as unreadable; so does this one.
pub const MAX_CLAIM_BYTES: u64 = 64 * 1024;
/// A session record is a few hundred bytes.
const MAX_SESSION_BYTES: u64 = 64 * 1024;

/// r3 (e) item 1, verbatim.
pub const TERMINAL_RUNNING: &str = "Your team is running in the terminal. End that session there, then ask me again.";

/// A process by identity: its pid AND its kernel start time, so a recycled pid is never it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessId {
    pub pid: u32,
    pub start: u64,
}

/// Where process start times come from. The kernel in production; a table in tests.
pub trait ProcessTable {
    /// The start time of a live, non-zombie `pid`, in whole epoch seconds.
    fn start_of(&self, pid: u32) -> Option<u64>;
    fn alive(&self, id: ProcessId) -> bool {
        self.start_of(id.pid) == Some(id.start)
    }
}

/// `proc_pidinfo(PROC_PIDTBSDINFO)`, the same call the engine side reads (`operator_fences.proc`).
pub struct Kernel;

impl ProcessTable for Kernel {
    #[cfg(target_os = "macos")]
    fn start_of(&self, pid: u32) -> Option<u64> {
        if pid == 0 {
            return None;
        }
        // SAFETY: `info` is a plain C struct the kernel fills, sized as the call is told.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = unsafe {
            libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, (&mut info as *mut libc::proc_bsdinfo).cast(), size)
        };
        // SZOMB is 5 in <sys/proc.h>: a zombie is not a running process.
        (n == size && info.pbi_status != 5).then_some(info.pbi_start_tvsec)
    }
    #[cfg(not(target_os = "macos"))]
    fn start_of(&self, _pid: u32) -> Option<u64> {
        None
    }
}

/// This process, by identity.
pub fn this_process(table: &dyn ProcessTable) -> Option<ProcessId> {
    let pid = std::process::id();
    table.start_of(pid).map(|start| ProcessId { pid, start })
}

/// Days since 1970-01-01 for a proleptic Gregorian date, and back (Howard Hinnant's
/// `days_from_civil` / `civil_from_days`, public domain). Kept here rather than borrowed from
/// `launch.rs`: that module holds his usage history, and `launch_no_outbound_tests` refuses any
/// other module that reaches into it.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 }.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `procStart` as `ps` prints it under `TZ=UTC0` (`Thu Sep 24 17:49:33 2026`), to epoch seconds.
pub fn parse_proc_start(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let [_weekday, month, day, clock, year] = parts.as_slice() else { return None };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.iter().position(|m| m == month)? as u32 + 1;
    let day: u32 = day.parse().ok().filter(|d| (1..=31).contains(d))?;
    let year: i64 = year.parse().ok()?;
    let hms: Vec<u64> = clock.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let [h, m, s] = hms.as_slice() else { return None };
    if *h > 23 || *m > 59 || *s > 60 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    u64::try_from(days * 86_400).ok().map(|base| base + h * 3600 + m * 60 + s)
}

/// Epoch seconds as ISO 8601 UTC.
pub fn iso_utc(epoch: u64) -> String {
    let days = (epoch / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    let rest = epoch % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, (rest / 60) % 60, rest % 60)
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// =============================================================================================
// session records (read only)
// =============================================================================================

/// What the app reads from one `<claude dir>/sessions/<pid>.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecord {
    pub pid: u32,
    /// `procStart`, as epoch seconds.
    pub start: Option<u64>,
    pub entrypoint: String,
    pub cwd: PathBuf,
    pub session_id: String,
}

/// Every readable `*.json` record. The directory also holds other files (`<pid>.<hex>.key`);
/// only `.json` is read, and nothing is ever written here.
pub fn read_sessions(dir: &Path) -> Result<Vec<SessionRecord>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("the session records at {} could not be read ({e})", dir.display())),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let small = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_SESSION_BYTES);
        let Some(value) = small.then(|| std::fs::read_to_string(&path).ok()).flatten()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
        let Some(pid) = value.get("pid").and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok()) else { continue };
        out.push(SessionRecord {
            pid,
            start: value.get("procStart").and_then(Value::as_str).and_then(parse_proc_start),
            entrypoint: value.get("entrypoint").and_then(Value::as_str).unwrap_or("").to_string(),
            cwd: PathBuf::from(value.get("cwd").and_then(Value::as_str).unwrap_or("")),
            session_id: value.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

fn lexically_inside(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

/// G11's terminal test, exactly as the engine side decides it (`operator_leads.is_terminal`).
/// Paths are compared as written in the record and as canonical, so a link does not hide it.
pub fn is_terminal(record: &SessionRecord, entity_root: &Path) -> bool {
    let entry = record.entrypoint.trim();
    if entry.is_empty() || entry.starts_with("sdk-") {
        return false;
    }
    let root = std::fs::canonicalize(entity_root).unwrap_or_else(|_| entity_root.to_path_buf());
    let cwd = std::fs::canonicalize(&record.cwd).unwrap_or_else(|_| record.cwd.clone());
    if record.cwd.as_os_str().is_empty() || !(lexically_inside(&cwd, &root) || lexically_inside(&record.cwd, entity_root)) {
        return false;
    }
    let worktrees = root.join(".claude").join("worktrees");
    !(lexically_inside(&cwd, &worktrees) || lexically_inside(&record.cwd, &entity_root.join(".claude").join("worktrees")))
}

/// Live terminal sessions in his entity: the terminal test, and alive by pid AND start (the
/// record's `procStart` within one second of the kernel's, as the engine side checks).
pub fn live_terminals(sessions: &Path, entity_root: &Path, table: &dyn ProcessTable, own: &[u32])
                      -> Result<Vec<SessionRecord>, String> {
    Ok(read_sessions(sessions)?.into_iter()
        .filter(|r| !own.contains(&r.pid) && is_terminal(r, entity_root))
        .filter(|r| match (r.start, table.start_of(r.pid)) {
            (Some(recorded), Some(kernel)) => recorded.abs_diff(kernel) <= 1,
            _ => false,
        })
        .collect())
}

// =============================================================================================
// the claim file
// =============================================================================================

/// What the claim file says right now.
#[derive(Clone, Debug, PartialEq)]
pub enum ClaimState {
    /// No file, or a file whose every listed process has ended.
    Free(Option<Value>),
    /// A live claim, owned by `app` or `terminal`.
    Live { owner: String, record: Value },
    /// Unreadable means held (r3 (e) item 6).
    Unreadable(String),
}

/// Read and judge the claim, the way `operator_leads.read_claim` / `claim_state` do.
pub fn claim_state(path: &Path, table: &dyn ProcessTable) -> ClaimState {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ClaimState::Free(None),
        Err(e) => return ClaimState::Unreadable(format!("it could not be read ({e})")),
    };
    if !metadata.file_type().is_file() {
        return ClaimState::Unreadable("it is not a plain file".into());
    }
    if metadata.len() > MAX_CLAIM_BYTES {
        return ClaimState::Unreadable("it is too large to be a claim".into());
    }
    let Some(record) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object) else {
        return ClaimState::Unreadable("it is not a JSON object".into());
    };
    let owner = record.get("owner").and_then(Value::as_str).unwrap_or("").to_string();
    let processes = record.get("processes").and_then(Value::as_array);
    if record.get("schema").and_then(Value::as_u64) != Some(CLAIM_SCHEMA) || !["app", "terminal"].contains(&owner.as_str())
        || processes.is_none() {
        return ClaimState::Unreadable(format!("it is not a schema-{CLAIM_SCHEMA} claim with an owner and a process list"));
    }
    let mut ids = Vec::new();
    for p in processes.into_iter().flatten() {
        match (p.get("pid").and_then(Value::as_u64).and_then(|x| u32::try_from(x).ok()), p.get("start").and_then(Value::as_u64)) {
            (Some(pid), Some(start)) => ids.push(ProcessId { pid, start }),
            _ => return ClaimState::Unreadable("a listed process has no integer pid and start".into()),
        }
    }
    if ids.iter().any(|id| table.alive(*id)) {
        ClaimState::Live { owner, record }
    } else {
        ClaimState::Free(Some(record))
    }
}

/// `flock` on the lock file, held for one check-and-write. Busy past [`CLAIM_LOCK_WAIT`] is a
/// refusal, never a guess.
struct ClaimLock {
    _file: std::fs::File,
}

impl ClaimLock {
    fn take(path: &Path, wait: Duration) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(parent).map_err(|e| format!("its folder could not be made ({e})"))?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path).map_err(|e| format!("the claim lock could not be opened ({e})"))?;
        let deadline = Instant::now() + wait;
        loop {
            #[cfg(unix)]
            {
                use std::os::unix::io::AsRawFd;
                // SAFETY: flock on a descriptor this struct owns; released when it closes.
                // lock-busy: one poll inside the caller's `wait`; busy past it says in words that the claim lock stayed busy.
                if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                    return Ok(ClaimLock { _file: file });
                }
            }
            #[cfg(not(unix))]
            return Ok(ClaimLock { _file: file });
            #[cfg(unix)]
            if Instant::now() >= deadline {
                return Err(format!("the claim lock stayed busy for {} s", wait.as_secs()));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn write_claim(path: &Path, record: &Value) -> Result<(), String> {
    let tmp = path.with_file_name(format!(".{}.{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("claim"),
                                          std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())? + "\n";
    let written = options.open(&tmp).and_then(|mut f| {
        use std::io::Write;
        f.write_all(text.as_bytes()).and_then(|_| f.sync_all())
    });
    // The claim itself is untouched on either failure; a temporary file that also cannot be
    // removed is overwritten by the next write, so its own error adds nothing to the sentence.
    if let Err(e) = written {
        std::fs::remove_file(&tmp).ok();
        return Err(format!("the claim could not be written ({e})"));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        std::fs::remove_file(&tmp).ok();
        format!("the claim could not be written ({e})")
    })
}

/// Where the claim lives and who it guards, from a declaration that passed the gate.
#[derive(Clone, Debug)]
pub struct ClaimPlace {
    pub file: PathBuf,
    pub lock: PathBuf,
    /// `<claude dir>/sessions`, beside `<claude dir>/state`.
    pub sessions: PathBuf,
    pub entity_root: PathBuf,
}

impl ClaimPlace {
    pub fn from_declaration(d: &crate::operator_declaration::Declaration) -> Self {
        let claude = d.claim.file.parent().and_then(Path::parent).map(Path::to_path_buf)
            .unwrap_or_else(|| d.home.join(".claude"));
        ClaimPlace { file: d.claim.file.clone(), lock: d.claim.lock.clone(), sessions: claude.join("sessions"),
                     entity_root: d.entity_root.clone() }
    }
}

/// Why his team cannot open here, in one sentence to him.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimRefusal(pub String);

/// The app's claim, once taken. Every change is one flock'd check-and-write.
#[derive(Clone, Debug)]
pub struct AppClaim {
    place: ClaimPlace,
    claim_id: String,
    app: ProcessId,
}

fn unreadable_sentence(place: &ClaimPlace, why: &str) -> String {
    format!("The file that says who runs your team ({}) is unreadable ({why}), so your team does not start here. \
             Rich checks that no terminal session runs your team, then removes that file.", place.file.display())
}

impl AppClaim {
    pub fn claim_id(&self) -> &str {
        &self.claim_id
    }

    /// Take the claim for this app before its first lead opens (r3 (e) items 1-3). Idempotent:
    /// a live claim this very process already holds is adopted, never rewritten.
    pub fn acquire(place: &ClaimPlace, app: ProcessId, own_leads: &[u32], table: &dyn ProcessTable)
                   -> Result<Self, ClaimRefusal> {
        let _lock = ClaimLock::take(&place.lock, CLAIM_LOCK_WAIT).map_err(|why| ClaimRefusal(format!(
            "RichOS could not check who runs your team ({why}). Ask me again in a moment.")))?;
        // Inside the critical section: the claim, then the session table (item 2).
        let state = claim_state(&place.file, table);
        match &state {
            ClaimState::Unreadable(why) => return Err(ClaimRefusal(unreadable_sentence(place, why))),
            ClaimState::Live { owner, .. } if owner == "terminal" => return Err(ClaimRefusal(TERMINAL_RUNNING.into())),
            ClaimState::Live { record, .. } => {
                let ours = listed(record).contains(&app);
                let id = record.get("claim_id").and_then(Value::as_str).unwrap_or("");
                if ours && !id.is_empty() {
                    return Ok(AppClaim { place: place.clone(), claim_id: id.to_string(), app });
                }
                return Err(ClaimRefusal("Your team is already running in another copy of RichOS on this Mac. \
                                         Quit that one first, then ask me again.".into()));
            }
            ClaimState::Free(_) => {}
        }
        let terminals = live_terminals(&place.sessions, &place.entity_root, table, own_leads)
            .map_err(|why| ClaimRefusal(format!("RichOS could not check whether your terminal runs your team ({why}), \
                                                 so your team does not start here. Rich can fix it.")))?;
        if !terminals.is_empty() {
            return Err(ClaimRefusal(TERMINAL_RUNNING.into()));
        }
        let claim_id = format!("app-{}", uuid::Uuid::new_v4());
        let record = json!({"schema": CLAIM_SCHEMA, "owner": "app", "claim_id": claim_id,
                            "claimed_at": iso_utc(now_epoch()),
                            "processes": [{"role": "app", "pid": app.pid, "start": app.start}], "leads": []});
        write_claim(&place.file, &record).map_err(|why| ClaimRefusal(format!("{why}. Rich can fix it.")))?;
        Ok(AppClaim { place: place.clone(), claim_id, app })
    }

    fn update(&self, table: &dyn ProcessTable, change: impl FnOnce(&mut Value)) -> Result<(), ClaimRefusal> {
        let _lock = ClaimLock::take(&self.place.lock, CLAIM_LOCK_WAIT)
            .map_err(|why| ClaimRefusal(format!("RichOS could not update who runs your team ({why}).")))?;
        match claim_state(&self.place.file, table) {
            ClaimState::Live { owner, mut record }
                if owner == "app" && record.get("claim_id").and_then(Value::as_str) == Some(self.claim_id.as_str()) => {
                change(&mut record);
                write_claim(&self.place.file, &record).map_err(ClaimRefusal)
            }
            ClaimState::Unreadable(why) => Err(ClaimRefusal(unreadable_sentence(&self.place, &why))),
            _ => Err(ClaimRefusal("This app no longer holds the claim on your team, so no lead opens. \
                                    Rich can fix it.".into())),
        }
    }

    /// Record a lead and its supervisor before the lead takes work (item 3). Its conversation's
    /// title is what the land lease names it by (`operator_fences._app_conversation_title`).
    pub fn add_lead(&self, supervisor: ProcessId, lead: ProcessId, session_id: &str, title: &str,
                    table: &dyn ProcessTable) -> Result<(), ClaimRefusal> {
        let title: String = title.chars().filter(|c| !c.is_control()).take(200).collect();
        self.update(table, |record| {
            let processes = record["processes"].as_array_mut().expect("checked by claim_state");
            for (role, id) in [("supervisor", supervisor), ("lead", lead)] {
                if !processes.iter().any(|p| p["pid"] == json!(id.pid) && p["start"] == json!(id.start)) {
                    processes.push(json!({"role": role, "pid": id.pid, "start": id.start}));
                }
            }
            if !record["leads"].is_array() {
                record["leads"] = json!([]);
            }
            let leads = record["leads"].as_array_mut().expect("just set");
            leads.retain(|l| !(l["pid"] == json!(lead.pid) && l["start"] == json!(lead.start)));
            leads.push(json!({"pid": lead.pid, "start": lead.start, "session_id": session_id, "title": title}));
        })
    }

    /// Drop a lead and its supervisor once the supervisor has exited (item 3: the claim goes
    /// dead only after that). Dead entries of other leads are dropped with it.
    pub fn remove_lead(&self, supervisor: ProcessId, lead: ProcessId, table: &dyn ProcessTable) -> Result<(), ClaimRefusal> {
        let app = self.app;
        self.update(table, |record| {
            let gone = |p: &Value| {
                let id = ProcessId { pid: p["pid"].as_u64().unwrap_or(0) as u32, start: p["start"].as_u64().unwrap_or(0) };
                id == supervisor || id == lead || (id != app && !table.alive(id))
            };
            if let Some(processes) = record["processes"].as_array_mut() {
                processes.retain(|p| !gone(p));
            }
            if let Some(leads) = record["leads"].as_array_mut() {
                leads.retain(|p| !gone(p));
            }
        })
    }

    /// Give the claim up at quit, after every supervisor has exited. Only this claim is removed.
    pub fn release(&self, table: &dyn ProcessTable) -> Result<(), ClaimRefusal> {
        let _lock = ClaimLock::take(&self.place.lock, CLAIM_LOCK_WAIT)
            .map_err(|why| ClaimRefusal(format!("RichOS could not release the claim on your team ({why}).")))?;
        let ours = |record: &Value| record.get("claim_id").and_then(Value::as_str) == Some(self.claim_id.as_str());
        match claim_state(&self.place.file, table) {
            ClaimState::Live { record, .. } | ClaimState::Free(Some(record)) if ours(&record) => {
                std::fs::remove_file(&self.place.file).map_err(|e| ClaimRefusal(format!("the claim could not be removed ({e})")))
            }
            _ => Ok(()),
        }
    }
}

fn listed(record: &Value) -> Vec<ProcessId> {
    record.get("processes").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|p| Some(ProcessId { pid: u32::try_from(p.get("pid")?.as_u64()?).ok()?, start: p.get("start")?.as_u64()? }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A process table the test writes.
    #[derive(Default)]
    struct Table(Mutex<HashMap<u32, u64>>);
    impl Table {
        fn with(rows: &[(u32, u64)]) -> Self {
            Table(Mutex::new(rows.iter().copied().collect()))
        }
        fn end(&self, pid: u32) {
            self.0.lock().unwrap().remove(&pid);
        }
    }
    impl ProcessTable for Table {
        fn start_of(&self, pid: u32) -> Option<u64> {
            self.0.lock().unwrap().get(&pid).copied()
        }
    }

    struct Fixture {
        root: PathBuf,
        place: ClaimPlace,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("operator-claim-{}", uuid::Uuid::new_v4()));
        let root = { std::fs::create_dir_all(&root).unwrap(); std::fs::canonicalize(&root).unwrap() };
        let claude = root.join("home/.claude");
        let entity = root.join("home/ab/femcboost");
        std::fs::create_dir_all(entity.join(".claude/worktrees/agent-x")).unwrap();
        std::fs::create_dir_all(claude.join("sessions")).unwrap();
        let place = ClaimPlace { file: claude.join("state/operator-lead.json"), lock: claude.join("state/operator-lead.lock"),
                                 sessions: claude.join("sessions"), entity_root: entity };
        Fixture { root, place }
    }

    /// 1790300000 = 2026-09-25T01:33:20Z (measured: python3 datetime.fromtimestamp, UTC); procStart
    /// is written the way `ps` prints it in UTC.
    const T0: u64 = 1_790_300_000;
    const T0_PS: &str = "Fri Sep 25 01:33:20 2026";

    fn session(f: &Fixture, pid: u32, entrypoint: &str, cwd: &Path, start: &str) {
        let body = json!({"pid": pid, "sessionId": format!("s-{pid}"), "cwd": cwd, "entrypoint": entrypoint,
                          "kind": "interactive", "procStart": start});
        std::fs::write(f.place.sessions.join(format!("{pid}.json")), body.to_string()).unwrap();
    }

    const APP: ProcessId = ProcessId { pid: 100, start: T0 };

    #[test]
    fn proc_start_is_read_as_utc_seconds_and_written_back_as_iso() {
        assert_eq!(parse_proc_start(T0_PS), Some(T0));
        // His live terminal's record (2026-09-24 18:49 BST is 17:49 UTC).
        assert_eq!(iso_utc(parse_proc_start("Thu Sep 24 17:49:33 2026").unwrap()), "2026-09-24T17:49:33Z");
        for bad in ["", "yesterday", "Thu Sep 24 25:49:33 2026", "Thu Foo 24 17:49:33 2026"] {
            assert_eq!(parse_proc_start(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_calendar_round_trips_across_leap_days_and_century_rules() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2024, 2, 29), (2026, 9, 25), (2100, 3, 1)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 9, 24), 20_720, "1790272173 / 86400 = 20720 whole days (python3)");
    }

    #[test]
    fn the_terminal_test_is_g11_s_and_never_kind() {
        let f = fixture();
        let e = &f.place.entity_root;
        let rec = |entry: &str, cwd: PathBuf| SessionRecord { pid: 1, start: Some(T0), entrypoint: entry.into(), cwd,
                                                             session_id: "s".into() };
        assert!(is_terminal(&rec("cli", e.clone()), e));
        assert!(is_terminal(&rec("claude-vscode", e.join("docs")), e), "G11: an IDE entrypoint counts");
        assert!(!is_terminal(&rec("sdk-cli", e.clone()), e), "P2: every print-mode lead records sdk-cli");
        assert!(!is_terminal(&rec("", e.clone()), e), "no entrypoint is not the terminal");
        assert!(!is_terminal(&rec("cli", e.join(".claude/worktrees/agent-x")), e), "a teammate's worktree");
        assert!(!is_terminal(&rec("cli", f.root.join("elsewhere")), e));
    }

    #[test]
    fn a_live_terminal_refuses_the_app_and_an_ended_one_does_not() {
        let f = fixture();
        session(&f, 7, "cli", &f.place.entity_root, T0_PS);
        let table = Table::with(&[(7, T0), (APP.pid, APP.start)]);
        assert_eq!(AppClaim::acquire(&f.place, APP, &[], &table).unwrap_err().0, TERMINAL_RUNNING);
        assert!(!f.place.file.exists(), "a refusal writes nothing");
        // The same pid, recycled by another process with another start, is not his terminal.
        let recycled = Table::with(&[(7, T0 + 500), (APP.pid, APP.start)]);
        assert!(AppClaim::acquire(&f.place, APP, &[], &recycled).is_ok());
    }

    #[test]
    fn a_print_mode_session_in_the_entity_is_not_the_terminal() {
        let f = fixture();
        session(&f, 8, "sdk-cli", &f.place.entity_root, T0_PS);
        let table = Table::with(&[(8, T0), (APP.pid, APP.start)]);
        let claim = AppClaim::acquire(&f.place, APP, &[], &table).unwrap();
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&f.place.file).unwrap()).unwrap();
        assert_eq!(written["owner"], "app");
        assert_eq!(written["claim_id"], claim.claim_id());
        assert_eq!(written["processes"], json!([{"role": "app", "pid": 100, "start": T0}]));
        assert!(!f.place.sessions.join("100.json").exists(), "nothing is written into the sessions directory");
    }

    #[test]
    fn a_live_terminal_claim_refuses_and_an_unreadable_one_is_held_with_the_way_through() {
        let f = fixture();
        std::fs::create_dir_all(f.place.file.parent().unwrap()).unwrap();
        std::fs::write(&f.place.file, json!({"schema": 1, "owner": "terminal", "claim_id": "terminal-7-1",
            "processes": [{"role": "terminal", "pid": 7, "start": T0}], "leads": []}).to_string()).unwrap();
        let table = Table::with(&[(7, T0), (APP.pid, APP.start)]);
        assert_eq!(AppClaim::acquire(&f.place, APP, &[], &table).unwrap_err().0, TERMINAL_RUNNING);
        for body in ["{ not json", "[]", "{\"schema\":2,\"owner\":\"app\",\"processes\":[]}",
                     "{\"schema\":1,\"owner\":\"app\",\"processes\":[{\"pid\":\"7\",\"start\":1}]}"] {
            std::fs::write(&f.place.file, body).unwrap();
            let why = AppClaim::acquire(&f.place, APP, &[], &table).unwrap_err().0;
            assert!(why.contains("unreadable") && why.contains("removes that file"), "{body}: {why}");
        }
    }

    #[test]
    fn a_dead_claim_is_replaced_and_the_app_s_own_live_claim_is_adopted() {
        let f = fixture();
        std::fs::create_dir_all(f.place.file.parent().unwrap()).unwrap();
        std::fs::write(&f.place.file, json!({"schema": 1, "owner": "terminal", "claim_id": "terminal-7-1",
            "processes": [{"role": "terminal", "pid": 7, "start": T0}], "leads": []}).to_string()).unwrap();
        let table = Table::with(&[(APP.pid, APP.start)]);
        let first = AppClaim::acquire(&f.place, APP, &[], &table).expect("the terminal's claim is dead");
        let again = AppClaim::acquire(&f.place, APP, &[], &table).expect("idempotent");
        assert_eq!(first.claim_id(), again.claim_id(), "adopted, not rewritten");
        let other = ProcessId { pid: 200, start: T0 };
        let table = Table::with(&[(APP.pid, APP.start), (200, T0)]);
        assert!(AppClaim::acquire(&f.place, other, &[], &table).unwrap_err().0.contains("another copy of RichOS"));
    }

    #[test]
    fn leads_are_listed_with_their_titles_and_the_claim_dies_only_with_its_processes() {
        let f = fixture();
        let table = Table::with(&[(APP.pid, APP.start), (300, T0 + 1), (301, T0 + 2)]);
        let claim = AppClaim::acquire(&f.place, APP, &[], &table).unwrap();
        let (sup, lead) = (ProcessId { pid: 300, start: T0 + 1 }, ProcessId { pid: 301, start: T0 + 2 });
        claim.add_lead(sup, lead, "session-1", "Landing the phone fix", &table).unwrap();
        claim.add_lead(sup, lead, "session-1", "Landing the phone fix", &table).unwrap();
        let record: Value = serde_json::from_str(&std::fs::read_to_string(&f.place.file).unwrap()).unwrap();
        assert_eq!(record["processes"].as_array().unwrap().len(), 3, "idempotent: {record}");
        assert_eq!(record["leads"], json!([{"pid": 301, "start": T0 + 2, "session_id": "session-1", "title": "Landing the phone fix"}]));
        // The app ends; the supervisor lives: the claim is still live (item 3).
        table.end(APP.pid);
        assert!(matches!(claim_state(&f.place.file, &table), ClaimState::Live { .. }));
        table.end(300);
        table.end(301);
        assert!(matches!(claim_state(&f.place.file, &table), ClaimState::Free(Some(_))));
    }

    #[test]
    fn remove_lead_and_release_touch_only_this_claim() {
        let f = fixture();
        let table = Table::with(&[(APP.pid, APP.start), (300, T0), (301, T0)]);
        let claim = AppClaim::acquire(&f.place, APP, &[], &table).unwrap();
        let (sup, lead) = (ProcessId { pid: 300, start: T0 }, ProcessId { pid: 301, start: T0 });
        claim.add_lead(sup, lead, "s", "t", &table).unwrap();
        claim.remove_lead(sup, lead, &table).unwrap();
        let record: Value = serde_json::from_str(&std::fs::read_to_string(&f.place.file).unwrap()).unwrap();
        assert_eq!(record["processes"].as_array().unwrap().len(), 1);
        assert!(record["leads"].as_array().unwrap().is_empty());
        claim.release(&table).unwrap();
        assert!(!f.place.file.exists());
        // A claim someone else wrote since is never removed by this one.
        std::fs::write(&f.place.file, json!({"schema": 1, "owner": "terminal", "claim_id": "terminal-9-1",
            "processes": [{"role": "terminal", "pid": 9, "start": T0}]}).to_string()).unwrap();
        claim.release(&table).unwrap();
        assert!(f.place.file.exists());
    }

    #[test]
    fn a_busy_lock_is_a_refusal_never_a_guess() {
        let f = fixture();
        let table = Table::with(&[(APP.pid, APP.start)]);
        let _held = ClaimLock::take(&f.place.lock, Duration::from_secs(1)).unwrap();
        // flock is per open file description, so a second open in this process conflicts.
        let err = ClaimLock::take(&f.place.lock, Duration::from_millis(200)).err().expect("busy");
        assert!(err.contains("busy"), "{err}");
        let _ = table;
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_kernel_answers_for_this_process_and_not_for_a_recycled_start() {
        let me = this_process(&Kernel).expect("this process has a start time");
        assert!(Kernel.alive(me));
        assert!(!Kernel.alive(ProcessId { pid: me.pid, start: me.start + 1 }));
    }
}
