//! **Several Claude subscriptions, used one at a time** ("fill-first", the CEO 2026-10-04:
//! *"when the weekly Claude quota in one subscription is exhausted auto-switch to the other
//! Claude subscription and so on"*).
//!
//! Plan: richos-hq `docs/plans/2026-10-04-multi-subscription-fill-first.md` (Sage), with
//! Frank's review beside it and the CEO's answers in its §15.
//!
//! - **One Claude Code configuration folder per subscription** (`CLAUDE_CONFIG_DIR`, the
//!   mechanism Anthropic documents as "Log in with multiple accounts"). Each folder is signed
//!   in with the stock `claude auth login`; RichOS never sees a credential.
//! - **Account 1 is the folder used today.** It has no folder of its own here (`folder: None`),
//!   so a lease on it inherits the app's environment exactly as before, and a user with one
//!   subscription sees no change.
//! - **Every chat's records are in ONE place, whichever account it ran on** (the CEO
//!   2026-11-01: removing an account broke the chats that had run on it). In the terminal there
//!   is one `~/.claude` and signing in to another account never touches its history; so Claude
//!   Code's records (`projects/`) are Account 1's folder's, and an added account's own
//!   `projects` is only a link to it ([`Accounts::share_records`]). An added account's folder
//!   holds its sign-in and nothing a chat needs: removing it removes only the sign-in, and a
//!   chat started on one account resumes on any other. An install that kept records inside an
//!   added account's folder has them moved into the shared place once, losing nothing
//!   ([`move_records`]); nothing is ever moved out of Account 1's folder.
//! - **Staying is fill-first:** the account in use stays in use until it must be left.
//! - **Left BEFORE the wall, never at it** (plan §15 answers 6-8; ruling §108). An account is
//!   left at a turn boundary, checked before EVERY turn from the freshest reading (the
//!   `rate_limit_event` the lease itself streamed when that is newer than the probe):
//!   - at normal speed, at **99% of its weekly window** (`resets::WEEKLY_THRESHOLD`), and at
//!     the pause threshold (**93%**) of its five-hour window when the setting is Switch (his
//!     answer 2; with Pause the five-hour window holds the workers exactly as before);
//!   - when usage speeds up a lot, EARLIER: the five-hour window whenever the measured speed
//!     would carry it to 100% before the next check, which is then one minute away instead
//!     of 5 (`quota::Reading`, plan answers 10 and 11; back to 93% once the speed is down);
//!     the weekly window 1 or 2 points earlier, at 98% or 97%, when the wait for the next
//!     check and a teammate's handoff would use more than 1 point (`Reading::weekly_point`,
//!     weekly-switch plan §1).
//! - **Choosing the next** (his answer 1): the one whose weekly window resets soonest, among
//!   accounts with room. No list-order option and no setting for it.
//! - **Choosing the first is his** (the CEO 2026-10-06, feedback item 8: *"I should be able to
//!   change/switch which account drains first"*): [`Accounts::use_first`] puts the account he
//!   picks in use. It is saved like any switch, so it survives a relaunch, and from then on it
//!   is left exactly as above: at 99% of its week, to the account whose week resets soonest.
//! - **All accounts gone:** work is held until the soonest reset among them (Frank's
//!   finding 2). A usage-limit refusal inside a turn (the backstop, for one turn that by itself
//!   uses up what remained) marks its account gone until its reset.
//!
//! This file decides and records; it spawns nothing. The quota service and the turn boundaries
//! call [`Accounts::evaluate`]; the lease factory reads [`Accounts::in_use`]; the spine and the
//! work host call [`Accounts::limit_reached`] when a lease is cut by a limit anyway.
use crate::quota::{Reading, Window};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{fs, io};

/// The account that is the folder RichOS used before this file existed.
pub const ACCOUNT_ONE: &str = "1";

/// The account list under the app's data folder. The app's review-watch hands this path to each
/// second review, which reads the account in use from it when its reviewer starts
/// (`review_watch::Mode::App`).
pub const LIST_FILE: &str = "claude-accounts.json";

/// What a lease error says when the provider refused the turn for a usage limit. The spine and
/// the work host read it back with [`parse_usage_limit`]; nothing else depends on the wording.
pub const USAGE_LIMIT: &str = "Claude usage limit reached";

/// When a limit is hit and nothing says when it resets, the account is retried after this long.
/// Five hours: the longest a session window can still be running.
const UNKNOWN_RESET_MS: u64 = 5 * 3_600_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    /// The user's own label. Rows show this and nothing else (his answer 4: no email).
    pub label: String,
    /// `None` for Account 1: the folder used today, inherited from the app's environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
}

/// The one setting (his answer 2): what happens when the five-hour window reaches the pause
/// threshold. Pause is the default and is today's behavior, unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AtThreshold {
    #[default]
    Pause,
    Switch,
}

/// **The last switch**, for the panel's card after it (round 16: who switched, from where,
/// when, and why). Ids, not labels, so a rename never leaves it stale.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSwitch {
    pub from: String,
    pub to: String,
    /// Epoch ms.
    pub at: u64,
    /// `fiveHour`, `weekly` or `limit`.
    pub why: String,
    /// The figure it was read at; `None` for a usage-limit refusal.
    pub used: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    accounts: Vec<Account>,
    in_use: String,
    #[serde(default)]
    at_threshold: AtThreshold,
    /// Account id -> until when a usage limit refused it (epoch ms).
    #[serde(default)]
    limited_until: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_switch: Option<LastSwitch>,
    /// Round 18's **Recent changes**, oldest first, the last [`CHANGES_KEPT`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    changes: Vec<Change>,
    /// Rich's one-time suggestion of a second account (round 18, `one-nudge`): said at most
    /// once on this Mac, ever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nudge: Option<Said>,
    /// Rich's "close to their weekly limit" line (round 18, `both-near-chat`): said once while
    /// every account is near its week, and again only after one of them is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    both_near: Option<Said>,
    /// Which of Rich's lines in the conversation are about accounts, so the conversation can
    /// put round 18's buttons under them. The last [`CHANGES_KEPT`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    notes: Vec<NoteTurn>,
}
impl Default for Stored {
    fn default() -> Self {
        Self {
            accounts: vec![Account { id: ACCOUNT_ONE.into(), label: "Account 1".into(), folder: None }],
            in_use: ACCOUNT_ONE.into(),
            at_threshold: AtThreshold::Pause,
            limited_until: BTreeMap::new(),
            last_switch: None,
            changes: Vec::new(),
            nudge: None,
            both_near: None,
            notes: Vec::new(),
        }
    }
}

/// How many Recent changes rows, and how many account lines in the conversation, are kept.
pub const CHANGES_KEPT: usize = 20;

/// Round 18's one-time suggestion of a second account, at this share of a single account's
/// week (the mockup's value; its NOTES.md calls it "a fixture value, not a rule").
pub const NUDGE_WEEKLY_PERCENT: f64 = 86.0;

/// Round 18's "close to the weekly limit": the share of the week at which a card's meter reads
/// "Almost used up", and at which, when every account is there, Rich says so.
pub const NEAR_WEEKLY_PERCENT: f64 = 90.0;

/// **One row of round 18's Recent changes.** Labels are kept as they were at the time, so a
/// removed account still reads by its name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// Epoch ms.
    pub at: u64,
    /// `added`, `removed`, `chose` or `switched`.
    pub kind: String,
    /// The account added, removed, chosen or switched to.
    pub id: String,
    pub label: String,
    /// For `switched`: the account that was left, and why (`weekly`, `fiveHour` or `limit`),
    /// with the figure it was read at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
}

/// A line Rich said once: when, the conversation turn it was written as, and (the suggestion
/// only) the answer, `notNow`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Said {
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// One of Rich's lines in the conversation that is about accounts: `nudge`, `switched` or
/// `bothNear`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteTurn {
    pub turn: String,
    pub kind: String,
}

/// "your **Work** account", or "**Account 1**" when the label already says account (round 18:
/// "I switched the team to your Work account").
pub fn account_phrase(label: &str) -> String {
    if label.to_lowercase().split_whitespace().any(|w| w == "account") {
        format!("**{label}**")
    } else {
        format!("your **{label}** account")
    }
}

/// One account's row as the settings panel shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub label: String,
    pub in_use: bool,
    /// **Who is working on this account right now** ([`Accounts::working_on`]): `"rich"` when
    /// a reply of Rich's, in any conversation, is running on it, `"team"` when a back end's
    /// lease (Stu's) runs on it. `in_use` is where the next session starts; during a switch the account being left
    /// still lists whoever has not moved yet (review rv-20261019T005929Z-e3d9ad03-3e7b, finding
    /// 3: the panel said the new account was "In use now" while Stu still ran on the old one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub working: Vec<String>,
    pub windows: Vec<Window>,
    pub checked_at: Option<u64>,
    /// Set while this account is exhausted, with when it comes back.
    pub exhausted_until: Option<u64>,
    /// Shown instead of the figures when the account could not be read.
    pub message: Option<String>,
    /// [`crate::quota::View::signed_in`] for this account.
    #[serde(default)]
    pub signed_in: Option<bool>,
    /// The weekly point this account is left at (`Reading::weekly_point` of its own weekly
    /// window), so a handoff order on it can record the point it acted on (`gate.rs`, follow-ups
    /// 7fed0bef item 3). `None` until the account has a weekly reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_point: Option<f64>,
    /// This account's own measured speeds, its speed before a hold and its last empty check
    /// (`crate::quota::View`'s fields of the same names), so the separate-process gate judges a
    /// call on the account it actually runs on (`quota::gate::decide`), not on the one in use.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub speeds: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pre_hold: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_at: Option<u64>,
}

/// **A move between accounts that has not happened yet** (Rich, 2026-10-29): the account in
/// use is `to`, where every new session starts, but work still runs on `from`. `blocked` when
/// Stu's session was found with work running on `from` that ending it would end
/// ([`Accounts::move_waits`]): the move waits for that work to finish.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Moving {
    pub from: String,
    pub to: String,
    pub blocked: bool,
}

/// Why an account is being left, for the one-line notice, with the figure it was read at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gone {
    Weekly(f64),
    FiveHour(f64),
    Limit,
}

/// **Must this account be left (or, with no account left, held), and until when?**
/// - a weekly window, its own week or one model's (`quota::is_weekly`), at its handoff point
///   (`Reading::weekly_point`, weekly-switch plan §1): 99% at
///   normal speed, 98% or 97% when the wait for the next check and a teammate's handoff
///   would use more than 1 or 2 points at the measured speed;
/// - with the setting on Switch AND the automatic switch on (`pause` is `Some(threshold)`),
///   the five-hour window at the pause threshold (93%), or earlier by the same projection;
/// - a usage-limit refusal that has not reset (the backstop).
///
/// `pause` is `None` while the automatic pause-or-switch is off. Round 16 (the design the CEO
/// chose for this panel, 2026-10-04) makes that one switch the subject of the sentence whose
/// verb is Pause or Switch: off, *"nothing happens at 93%; the line is only drawn"*, and only
/// the weekly 99% switch still happens (richos-hq `design/mockups/rounds/round-16/NOTES.md`).
pub fn gone(reading: &Reading, at_threshold: AtThreshold, pause: Option<u8>, limited_until: Option<u64>, now: u64) -> Option<(Gone, u64)> {
    if let Some(until) = limited_until.filter(|t| *t > now) {
        return Some((Gone::Limit, until));
    }
    // Its own week or one model's (`quota::is_weekly`); with several at their point, the one
    // that comes back last, since the account is fresh only when all of them are.
    let weekly = reading.windows.iter().filter(|w| crate::quota::is_weekly(&w.id) && reading.reaches_weekly(w))
        .filter_map(|w| match w.resets_at {
            Some(t) if t > now => Some((w.used_percent, t)),
            Some(_) => None,
            None => Some((w.used_percent, now + crate::quota::REFRESH_INTERVAL_MS)),
        })
        .max_by_key(|(_, until)| *until);
    if let Some((used, until)) = weekly {
        return Some((Gone::Weekly(used), until));
    }
    if let (AtThreshold::Switch, Some(pause_percent)) = (at_threshold, pause) {
        if let Some(five) = reading.windows.iter().find(|w| w.id == "five_hour"
            && reading.reaches(w, f64::from(pause_percent))) {
            if let Some(t) = five.resets_at.filter(|t| *t > now) {
                return Some((Gone::FiveHour(five.used_percent), t));
            }
        }
    }
    None
}

/// The weekly reset of an account with room, for the "soonest first" order.
fn weekly_reset(reading: &Reading) -> u64 {
    reading.windows.iter().find(|w| w.id == "seven_day").and_then(|w| w.resets_at).unwrap_or(u64::MAX)
}

/// **His answer 1:** among the candidates (accounts with room), the one whose weekly window
/// resets soonest. Ties keep list order.
pub fn next_account<'a>(candidates: impl IntoIterator<Item = (&'a str, &'a Reading)>) -> Option<&'a str> {
    candidates.into_iter().enumerate()
        .min_by_key(|(order, (_, reading))| (weekly_reset(reading), *order))
        .map(|(_, (id, _))| id)
}

/// What [`Accounts::limit_reached`] decided for a lease a usage limit cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AfterLimit {
    /// Another account is in use now: start the work again on it.
    Continue,
    /// No account has room. The work stops as it would have with one account.
    NoRoom,
}

/// A usage-limit lease error, written by the lease and read by the spine and the work host.
pub fn usage_limit_error(resets_at: Option<u64>, detail: &str) -> String {
    match resets_at {
        Some(t) => format!("{USAGE_LIMIT} (resets_at={t}): {detail}"),
        None => format!("{USAGE_LIMIT}: {detail}"),
    }
}

/// `Some(resets_at)` when `error` is a usage-limit refusal; the inner `None` means it did not
/// say when the limit resets.
pub fn parse_usage_limit(error: &str) -> Option<Option<u64>> {
    let at = error.find(USAGE_LIMIT)?;
    let rest = &error[at + USAGE_LIMIT.len()..];
    Some(rest.strip_prefix(" (resets_at=")
        .and_then(|r| r.split(')').next())
        .and_then(|n| n.parse().ok()))
}

/// A Recent changes row, keeping the last [`CHANGES_KEPT`].
fn push_change(state: &mut Stored, change: Change) {
    state.changes.push(change);
    let extra = state.changes.len().saturating_sub(CHANGES_KEPT);
    state.changes.drain(..extra);
}

/// Was `id`'s adding finished (its sign-in came back as a different account) since it was
/// last created? Ids are reused after a removal, so the latest row about it decides.
fn fully_added(state: &Stored, id: &str) -> bool {
    state.changes.iter().rev().find(|c| c.id == id && (c.kind == "added" || c.kind == "removed"))
        .is_some_and(|c| c.kind == "added")
}

/// The folder in a Claude Code folder that holds its chats' records (a session's `.jsonl`, its
/// helpers' transcripts and saved tool output beside it, and the project's memory).
pub const RECORDS: &str = "projects";

/// **`folder/projects` becomes a link to `shared`**, after every record in it was moved there.
/// A link that already points at `shared` is left as it is; a link to anywhere else is replaced
/// (a link is never followed, so nothing it points at is touched).
fn link_records(folder: &Path, shared: &Path) -> io::Result<()> {
    let own = folder.join(RECORDS);
    match fs::symlink_metadata(&own) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if fs::read_link(&own)? == shared { return Ok(()); }
            fs::remove_file(&own)?;
        }
        Ok(meta) if meta.is_dir() => move_own_records(folder, shared)?,
        Ok(_) => return Err(io::Error::other(format!("{} is not a folder", own.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    link(shared, &own)
}

/// **The record `path` names, in `shared`**: the same project folder and file name, when it is
/// there. For a path written under an account folder that is gone now.
pub fn record_in(path: &Path, shared: &Path) -> Option<PathBuf> {
    let (project, name) = (path.parent()?.file_name()?, path.file_name()?);
    Some(shared.join(project).join(name)).filter(|p| p.is_file())
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> io::Result<()> { std::os::unix::fs::symlink(target, at) }
#[cfg(windows)]
fn link(target: &Path, at: &Path) -> io::Result<()> { std::os::windows::fs::symlink_dir(target, at) }

/// Move every record in `folder/projects` (a real folder, never a link) into `shared`, then
/// remove the emptied `projects`. Anything that could not be moved stays, and so does the
/// folder holding it.
fn move_own_records(folder: &Path, shared: &Path) -> io::Result<()> {
    let own = folder.join(RECORDS);
    match fs::symlink_metadata(&own) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    }
    let tag = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    move_records(&own, shared, &tag)?;
    remove_empty(&own)
}

/// **Move one folder's records into another, losing nothing.** A file `to` does not have is
/// moved (renamed, or copied then removed across volumes). A file `to` has with the same bytes
/// is the same record twice, and the extra copy goes. Two different copies of one record (a
/// session an earlier version carried between accounts, then continued on both) are both kept:
/// the one written last keeps the record's name, so the chat resumes from its newest turn, and
/// the other is kept beside it as `<name>.from-account-<tag>`, a name no reader takes for a
/// transcript, so no turn is counted twice.
pub fn move_records(from: &Path, to: &Path, tag: &str) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            move_records(&source, &target, tag)?;
            continue;
        }
        match fs::symlink_metadata(&target) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => move_file(&source, &target)?,
            Err(e) => return Err(e),
            Ok(_) if same_bytes(&source, &target)? => fs::remove_file(&source)?,
            Ok(meta) => {
                let modified = |m: &fs::Metadata| m.modified().ok();
                let kept = kept_name(&target, tag);
                if modified(&entry.metadata()?) > modified(&meta) {
                    move_file(&target, &kept)?;
                    move_file(&source, &target)?;
                } else {
                    move_file(&source, &kept)?;
                }
            }
        }
    }
    Ok(())
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if fs::rename(from, to).is_ok() { return Ok(()); }
    fs::copy(from, to)?;
    fs::remove_file(from)
}

fn same_bytes(a: &Path, b: &Path) -> io::Result<bool> {
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() { return Ok(false); }
    Ok(fs::read(a)? == fs::read(b)?)
}

/// `<name>.from-account-<tag>`, numbered when that is taken too.
fn kept_name(target: &Path, tag: &str) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut candidate = target.with_file_name(format!("{name}.from-account-{tag}"));
    let mut n = 2;
    while candidate.symlink_metadata().is_ok() {
        candidate = target.with_file_name(format!("{name}.from-account-{tag}-{n}"));
        n += 1;
    }
    candidate
}

/// Remove `dir` and the folders under it once they are empty; an error when anything is left.
fn remove_empty(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() { remove_empty(&entry.path())?; }
    }
    fs::remove_dir(dir)
}

/// **Remove an account's sign-in folder, never a record in it**: its `projects` link is
/// unlinked (never followed), and a real `projects` still there (no shared place was named, so
/// nothing could be moved) is left with the folder around it; everything else goes.
fn remove_sign_in(folder: &Path) -> io::Result<()> {
    let own = folder.join(RECORDS);
    if fs::symlink_metadata(&own).is_ok_and(|m| m.file_type().is_symlink()) {
        fs::remove_file(&own)?;
    }
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        if path == own { continue; }
        if entry.file_type()?.is_dir() { fs::remove_dir_all(&path)?; } else { fs::remove_file(&path)?; }
    }
    match fs::remove_dir(folder) {
        Err(_) if own.exists() => Ok(()),
        done => done,
    }
}

/// A switch that is decided but has not happened yet: no conversation turn has run under `to`,
/// or a back end is still on another account.
struct PendingSwitch {
    /// The account the leases were on before this switch (before any chain of switches that
    /// nothing ran under).
    from: String,
    to: String,
    text: String,
    /// A conversation turn has run under `to` ([`Accounts::ran_on`]).
    ran: bool,
}

pub struct Accounts {
    path: PathBuf,
    root: PathBuf,
    /// **The one records folder every account's chats are kept in** (Account 1's `projects`,
    /// [`Accounts::share_records`]). `None` until the desktop entrypoint names Account 1's folder:
    /// tests and simulations never touch the developer's own.
    records: Mutex<Option<PathBuf>>,
    state: Mutex<Stored>,
    /// The switch notice waits here until a turn really runs under the new account
    /// ([`Accounts::ran_on`]) and no back end is still on another one
    /// ([`Accounts::back_end_on`]). A switch that waits on a running command shows nothing.
    pending: Mutex<Option<PendingSwitch>>,
    /// **The account each live back-end lease (Stu's) runs on**, by conversation, reported by
    /// the work host when it opens or ends one (nightly 65, 2026-10-18: "I switched the team
    /// to your Home account" was said while Stu's whole job ran on, and stayed on, Work).
    back_ends: Mutex<BTreeMap<String, String>>,
    /// **Every reply of Rich's running now, by turn, with the account it runs on**
    /// ([`Accounts::ran_on`] until [`Accounts::turn_ended`]). Each conversation has its own
    /// lease and their replies run side by side, so one conversation's reply on the new account
    /// says nothing about another's still running on the old one (review
    /// rv-20261019T030516Z-0a1a8728-e798, finding 2).
    replies: Mutex<BTreeMap<String, String>>,
    /// **The conversations whose back end could not move yet** ([`Accounts::move_waits`]):
    /// something of Stu's still runs on his old session. Cleared when that back end moves or ends.
    waiting: Mutex<std::collections::BTreeSet<String>>,
    notice: Mutex<Option<String>>,
    /// The webview's offset from UTC (`quota::Service::set_utc_offset`), so the switch line
    /// can say as a clock time when the account it left is fresh again.
    utc_offset_minutes: Mutex<Option<i32>>,
    /// Rich's lines about accounts that are queued and not yet written (`say_once`).
    queued: Mutex<std::collections::BTreeSet<String>>,
}

impl Accounts {
    /// `data_dir/claude-accounts.json` keeps the list; new folders go under
    /// `data_dir/claude-accounts/<id>/`.
    pub fn open(data_dir: &Path) -> io::Result<Self> {
        let path = data_dir.join(LIST_FILE);
        let state = match fs::read(&path) {
            Ok(bytes) => {
                let stored: Stored = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                if !stored.accounts.iter().any(|a| a.id == ACCOUNT_ONE) {
                    return Err(io::Error::other("the account list lost Account 1"));
                }
                stored
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Stored::default(),
            Err(e) => return Err(e),
        };
        Ok(Self { path, root: data_dir.join("claude-accounts"), records: Mutex::new(None), state: Mutex::new(state), pending: Mutex::new(None), back_ends: Mutex::new(BTreeMap::new()), replies: Mutex::new(BTreeMap::new()), waiting: Mutex::new(std::collections::BTreeSet::new()), notice: Mutex::new(None), utc_offset_minutes: Mutex::new(None), queued: Mutex::new(std::collections::BTreeSet::new()) })
    }

    fn save(&self, state: &Stored) -> io::Result<()> {
        crate::quota::atomic_write(&self.path, state)
    }

    pub fn list(&self) -> Vec<Account> { self.state.lock().unwrap().accounts.clone() }
    pub fn count(&self) -> usize { self.state.lock().unwrap().accounts.len() }
    pub fn in_use(&self) -> Account {
        let state = self.state.lock().unwrap();
        state.accounts.iter().find(|a| a.id == state.in_use).cloned()
            .unwrap_or_else(|| state.accounts[0].clone())
    }
    pub fn at_threshold(&self) -> AtThreshold { self.state.lock().unwrap().at_threshold }
    /// Round 18's Recent changes, oldest first.
    pub fn changes(&self) -> Vec<Change> { self.state.lock().unwrap().changes.clone() }
    /// Rich's one-time suggestion of a second account, once it has been said.
    pub fn nudge(&self) -> Option<Said> { self.state.lock().unwrap().nudge.clone() }
    /// Rich's lines in the conversation that are about accounts.
    pub fn notes(&self) -> Vec<NoteTurn> { self.state.lock().unwrap().notes.clone() }
    /// The webview's offset from UTC, minutes east positive (`quota::Service::set_utc_offset`).
    pub fn set_utc_offset(&self, minutes: i32) { *self.utc_offset_minutes.lock().unwrap() = Some(minutes); }
    pub fn utc_offset(&self) -> Option<i32> { *self.utc_offset_minutes.lock().unwrap() }

    /// **The added account's sign-in finished, as a different Claude account** (round 18: "Work
    /// is ready"). Recent changes says "You added Work." once per adding: a later Sign in on the
    /// same account adds no second row.
    pub fn signed_in(&self, id: &str, now: u64) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        let Some(account) = state.accounts.iter().find(|a| a.id == id).cloned() else { return Ok(()) };
        if fully_added(&state, id) { return Ok(()); }
        let mut next = state.clone();
        push_change(&mut next, Change { at: now, kind: "added".into(), id: id.into(), label: account.label, from: None, why: None, used: None });
        self.save(&next)?;
        *state = next;
        Ok(())
    }

    /// **Is a line about accounts due, and has it not been said?** Marks it said, so each is
    /// queued once: `nudge` once ever, `bothNear` once while it is true ([`Accounts::clear_both_near`]).
    ///
    /// It is recorded as said when it is WRITTEN into the conversation ([`Accounts::noted`]), not
    /// when it is queued: a line queued while no conversation is open, and lost to a relaunch,
    /// is queued again rather than never said. Until then it is queued at most once.
    pub fn say_once(&self, kind: &str, _now: u64) -> io::Result<bool> {
        let state = self.state.lock().unwrap();
        let said = match kind { "nudge" => state.nudge.is_some(), "bothNear" => state.both_near.is_some(), _ => return Ok(false) };
        if said { return Ok(false); }
        Ok(self.queued.lock().unwrap().insert(kind.to_string()))
    }

    /// Not every account is near its week any more, so the line may be said again next time.
    pub fn clear_both_near(&self) -> io::Result<()> {
        self.queued.lock().unwrap().remove("bothNear");
        let mut state = self.state.lock().unwrap();
        if state.both_near.is_none() { return Ok(()); }
        let mut next = state.clone();
        next.both_near = None;
        self.save(&next)?;
        *state = next;
        Ok(())
    }

    /// **A line about accounts was written into the conversation as `turn`**, so the
    /// conversation can draw round 18's buttons under it.
    pub fn noted(&self, kind: &str, turn: &str) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        next.notes.push(NoteTurn { turn: turn.into(), kind: kind.into() });
        let extra = next.notes.len().saturating_sub(CHANGES_KEPT);
        next.notes.drain(..extra);
        let said = || Said { at: crate::util::now_millis(), turn: Some(turn.into()), answer: None };
        match kind {
            "nudge" => next.nudge = Some(said()),
            "bothNear" => next.both_near = Some(said()),
            _ => {}
        }
        self.save(&next)?;
        *state = next;
        self.queued.lock().unwrap().remove(kind);
        Ok(())
    }

    /// **Not now** on Rich's suggestion (round 18): remembered, so the conversation answers "You
    /// can add one any time in Settings, under Claude accounts." and offers nothing again.
    pub fn answer_nudge(&self, answer: &str) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        let said = next.nudge.get_or_insert(Said { at: crate::util::now_millis(), turn: None, answer: None });
        said.answer = Some(answer.into());
        self.save(&next)?;
        *state = next;
        Ok(())
    }
    pub fn limited_until(&self, id: &str) -> Option<u64> { self.state.lock().unwrap().limited_until.get(id).copied() }
    pub fn last_switch(&self) -> Option<LastSwitch> { self.state.lock().unwrap().last_switch.clone() }
    pub fn folder(&self, id: &str) -> Option<PathBuf> {
        self.state.lock().unwrap().accounts.iter().find(|a| a.id == id).and_then(|a| a.folder.clone())
    }
    /// The Claude Code folder whose sign-in IS account `id` (round 31's "Show email"):
    /// `Some(None)` for Account 1, the app's own environment; `Some(Some(folder))` for an added
    /// account; `None` when no account has that id.
    pub fn sign_in_folder(&self, id: &str) -> Option<Option<PathBuf>> {
        self.state.lock().unwrap().accounts.iter().find(|a| a.id == id).map(|a| a.folder.clone())
    }

    pub fn set_at_threshold(&self, value: AtThreshold) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        next.at_threshold = value;
        self.save(&next)?;
        *state = next;
        Ok(())
    }

    /// **Name an account** (round 16: going from one account to two names both, because the
    /// first one never needed a label before). Empty keeps the label it has.
    pub fn rename(&self, id: &str, label: &str) -> io::Result<()> {
        let label: String = label.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        if label.is_empty() { return Ok(()); }
        let mut state = self.state.lock().unwrap();
        let mut next = state.clone();
        let Some(account) = next.accounts.iter_mut().find(|a| a.id == id) else { return Ok(()) };
        account.label = label;
        self.save(&next)?;
        *state = next;
        Ok(())
    }

    /// Add an account: a new, empty Claude Code configuration folder. Signing it in is the
    /// stock `claude auth login` with `CLAUDE_CONFIG_DIR` set to it (`provider_auth.rs`).
    pub fn add(&self, label: &str) -> io::Result<Account> {
        let mut state = self.state.lock().unwrap();
        let id = (state.accounts.iter().filter_map(|a| a.id.parse::<u32>().ok()).max().unwrap_or(1) + 1).to_string();
        let folder = self.root.join(&id);
        // The Keychain item Claude Code writes is named from a hash of this path string, and
        // the reset reader refuses non-ASCII custom paths (`reset_transport.rs`).
        if !folder.to_string_lossy().is_ascii() {
            return Err(io::Error::other("RichOS cannot create an account folder in a location whose name has non-ASCII characters."));
        }
        fs::create_dir_all(&folder)?;
        if let Some(shared) = self.records() {
            // Without the link (a system that refuses one) its chats stay in its folder, which
            // removing it never deletes (`remove_sign_in`).
            if let Err(error) = link_records(&folder, &shared) {
                eprintln!("[richos] claude accounts: a new account's chats could not be linked to the shared records ({error})");
            }
        }
        let label = label.trim();
        let label = if label.is_empty() { format!("Account {id}") } else { label.chars().filter(|c| !c.is_control()).take(40).collect() };
        let account = Account { id, label, folder: Some(folder) };
        let mut next = state.clone();
        next.accounts.push(account.clone());
        self.save(&next)?;
        *state = next;
        Ok(account)
    }

    /// Remove an added account: its sign-in, never a chat. Account 1 is the user's own Claude
    /// Code folder and is never removed. The caller signs the folder out first (`claude auth
    /// logout`). Any record still inside the folder is first moved into the shared place
    /// ([`move_own_records`]); one that cannot be moved is left where it is.
    pub fn remove(&self, id: &str) -> io::Result<()> {
        if id == ACCOUNT_ONE {
            return Err(io::Error::other("Account 1 is your own Claude Code sign-in and cannot be removed here."));
        }
        let mut state = self.state.lock().unwrap();
        let Some(account) = state.accounts.iter().find(|a| a.id == id).cloned() else { return Ok(()) };
        let own = account.folder.clone().filter(|f| f.starts_with(&self.root));
        if let (Some(folder), Some(shared)) = (&own, self.records()) {
            // Anything still inside joins the others; what could not be moved stays, with the
            // folder around it (`remove_sign_in`), and the next launch moves it (`share_records`).
            if let Err(error) = move_own_records(folder, &shared) {
                eprintln!("[richos] claude accounts: {}'s chats stay in its folder for now ({error})", account.label);
            }
        }
        let mut next = state.clone();
        // An account whose sign-in never finished (Cancel in round 18's add flow) leaves no row
        // in Recent changes; one that was added says it was removed.
        if fully_added(&state, id) {
            push_change(&mut next, Change { at: crate::util::now_millis(), kind: "removed".into(), id: id.into(), label: account.label.clone(), from: None, why: None, used: None });
        }
        next.accounts.retain(|a| a.id != id);
        next.limited_until.remove(id);
        if next.in_use == id { next.in_use = ACCOUNT_ONE.into(); }
        if next.last_switch.as_ref().is_some_and(|s| s.from == id || s.to == id) { next.last_switch = None; }
        self.save(&next)?;
        *state = next;
        drop(state);
        // Only a folder this file created is ever deleted, and never a chat's record in it.
        if let Some(folder) = own {
            let _best_effort = remove_sign_in(&folder);
        }
        Ok(())
    }

    /// The one records folder, once [`Accounts::share_records`] named it.
    pub fn records(&self) -> Option<PathBuf> { self.records.lock().unwrap().clone() }

    /// **Keep every account's chats in Account 1's records folder** (`home/projects`, `home`
    /// being Account 1's Claude Code folder: the app's `CLAUDE_CONFIG_DIR`, else `~/.claude`).
    /// Each added account's folder gets `projects` as a link to it; records an earlier version
    /// kept inside an added account's folder, a removed one's left behind included, are moved
    /// there first ([`move_records`]). Run at every launch; once everything is linked it moves
    /// nothing. An account whose records could not all be moved keeps them where they are and
    /// is not linked (the first such error is returned; every other account is still done).
    pub fn share_records(&self, home: &Path) -> io::Result<()> {
        if home.starts_with(&self.root) {
            return Err(io::Error::other("Account 1's folder is inside the added accounts' folder"));
        }
        let shared = home.join(RECORDS);
        fs::create_dir_all(&shared)?;
        *self.records.lock().unwrap() = Some(shared.clone());
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        let listed: Vec<PathBuf> = self.list().into_iter().filter_map(|a| a.folder).collect();
        let mut first_error = None;
        for entry in entries.flatten() {
            let folder = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) { continue; }
            let done = if listed.contains(&folder) {
                link_records(&folder, &shared)
            } else {
                // A folder no account owns any more (removed by a version that left its records):
                // its records join the others; the folder itself is not this launch's to delete.
                move_own_records(&folder, &shared)
            };
            if let Err(e) = done { first_error.get_or_insert(e); }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// `fresh_at` is when the account being left is fresh again (the reset of the window that
    /// made it go), when that is known.
    fn switch_to(&self, state: &mut Stored, to: &str, why: Gone, fresh_at: Option<u64>, now: u64) -> io::Result<()> {
        let from = state.accounts.iter().find(|a| a.id == state.in_use).map(|a| a.label.clone()).unwrap_or_default();
        let to_label = state.accounts.iter().find(|a| a.id == to).map(|a| a.label.clone()).unwrap_or_default();
        let (kind, used) = match why {
            Gone::Weekly(used) => ("weekly", Some(used)),
            Gone::FiveHour(used) => ("fiveHour", Some(used)),
            Gone::Limit => ("limit", None),
        };
        let leaving = state.in_use.clone();
        let mut next = state.clone();
        next.last_switch = Some(LastSwitch { from: next.in_use.clone(), to: to.into(), at: now, why: kind.into(), used });
        next.in_use = to.into();
        push_change(&mut next, Change { at: now, kind: "switched".into(), id: to.into(), label: to_label.clone(), from: Some(from.clone()), why: Some(kind.into()), used });
        self.save(&next)?;
        *state = next;
        // **Rich's line in the conversation, in round 18's words** ("I switched the team to your
        // Work account. Home had used 99% of its weekly limit, and it is fresh again on Thursday
        // at 9:00 AM." / "Nothing stopped. ..."). A switch at a turn boundary stops nothing; a
        // usage-limit refusal re-serves the step it cut on the next account (`spine.rs`,
        // `work_host.rs`). The clock time needs the webview's offset; before it has said one,
        // the span is said instead.
        let fresh = fresh_at.filter(|t| *t > now)
            .map(|t| format!(", and it is fresh again {}", crate::quota::when_long(t, *self.utc_offset_minutes.lock().unwrap(), now)))
            .unwrap_or_default();
        let lead = format!("I switched the team to {}.", account_phrase(&to_label));
        let carried = "Nothing stopped. The team carried on right where it was.";
        let text = match why {
            Gone::FiveHour(used) => format!("{lead} {from} had used {}% of its 5-hour limit{fresh}.\n\n{carried}", used.floor()),
            Gone::Weekly(used) => format!("{lead} {from} had used {}% of its weekly limit{fresh}.\n\n{carried}", used.floor()),
            Gone::Limit => format!("{lead} {from} reached a Claude usage limit, so the step it turned away runs again on {to_label}."),
        };
        // Deciding is not switching: running leases move at their next turn boundary, and one
        // with a command running waits longer (`spine.rs` / `work_host.rs`,
        // `account_switch_due`). The notice is held until a turn or job runs under `to`. A
        // switch back to the account nothing ever left is no switch at all.
        let mut pending = self.pending.lock().unwrap();
        let origin = pending.as_ref().map_or(leaving, |p| p.from.clone());
        *pending = (origin != to).then(|| PendingSwitch {
            from: origin,
            to: to.into(),
            text,
            ran: false,
        });
        Ok(())
    }

    /// **Has the decided switch not happened yet for every session?** While it has not, the
    /// view leaves out the record of it, so nothing says the team moved before it did.
    pub fn switch_pending(&self) -> bool { self.pending.lock().unwrap().is_some() }

    /// **The back end of conversation `thread` runs on `account` now** (`None`: it has no live
    /// lease). Reported by the work host whenever it opens or ends Stu's lease.
    pub fn back_end_on(&self, thread: &str, account: Option<&str>) {
        // A back end that ended or opened a session is no longer waiting to move.
        self.waiting.lock().unwrap().remove(thread);
        {
            let mut back_ends = self.back_ends.lock().unwrap();
            match account {
                Some(account) => { back_ends.insert(thread.into(), account.into()); }
                None => { back_ends.remove(thread); }
            }
        }
        self.release();
    }

    /// **The back end of conversation `thread` could not move to the account in use yet**:
    /// something of Stu's still runs on his session (a turn, a command or a helper), and ending
    /// it would end that work. Reported by the work host at `Renewal::Waits`. The accounts
    /// screen says so in plain words until it moves ([`Moving::blocked`]).
    pub fn move_waits(&self, thread: &str) {
        self.waiting.lock().unwrap().insert(thread.into());
    }

    /// **The account the work actually runs on** (Rich, 2026-10-29): the account in use once
    /// every live back end (Stu's session, where his helpers run) and every running reply of
    /// Rich's is on it; until then, the account a back end is still on, or failing that the one
    /// a reply is still on. Nothing running anywhere: the account in use, where the next work
    /// starts. The screen marks and lists this one first, and the top-level reading describes
    /// it, so nothing shows a switch that has not happened.
    pub fn running_on(&self) -> Account {
        let in_use = self.in_use();
        // One lock at a time: `release` holds `replies` while it takes `back_ends`.
        let away = |on: &Mutex<BTreeMap<String, String>>| on.lock().unwrap().values().find(|a| **a != in_use.id).cloned();
        let elsewhere = away(&self.back_ends).or_else(|| away(&self.replies));
        elsewhere.and_then(|id| self.list().into_iter().find(|a| a.id == id)).unwrap_or(in_use)
    }

    /// **The move under way**, while work still runs on another account than the one in use.
    pub fn moving(&self) -> Option<Moving> {
        let (to, from) = (self.in_use().id, self.running_on().id);
        if from == to { return None; }
        let waiting: Vec<String> = self.waiting.lock().unwrap().iter().cloned().collect();
        let blocked = {
            let back_ends = self.back_ends.lock().unwrap();
            waiting.iter().any(|thread| back_ends.get(thread).is_some_and(|on| *on != to))
        };
        Some(Moving { from, to, blocked })
    }

    /// **Who is working on `account` right now**, for the panel ([`AccountView::working`]):
    /// `"rich"` when a reply of his, in any conversation, is running on it, `"team"` when a back
    /// end's live lease runs on it.
    pub fn working_on(&self, account: &str) -> Vec<String> {
        let mut who = Vec::new();
        if self.replies.lock().unwrap().values().any(|on| on == account) { who.push("rich".to_string()); }
        if self.back_ends.lock().unwrap().values().any(|on| on == account) { who.push("team".to_string()); }
        who
    }

    /// The switch has happened once a conversation turn ran under `to`, every reply of Rich's
    /// still running is on `to` and every live back end is on `to` too; then its notice is
    /// released for [`Accounts::take_notice`].
    fn release(&self) {
        let mut pending = self.pending.lock().unwrap();
        let moved = pending.as_ref().is_some_and(|p| p.ran
            && self.replies.lock().unwrap().values().all(|account| *account == p.to)
            && self.back_ends.lock().unwrap().values().all(|account| *account == p.to));
        if moved {
            *self.notice.lock().unwrap() = pending.take().map(|p| p.text);
        }
    }

    /// **He chose the account that drains first** (feedback item 8, 2026-10-06). `id` is put in
    /// use now and saved, so the choice survives a relaunch. Running leases move to it at their
    /// next turn boundary, exactly as after an automatic switch (`account_switch_due` in
    /// `spine.rs` and `work_host.rs`); nothing inside a turn is touched. Nothing is announced:
    /// he made the change himself, so a pending automatic switch's line is dropped, and the
    /// record of the last automatic switch goes too, because it no longer describes the account
    /// in use. Choosing the account already in use changes nothing.
    pub fn use_first(&self, id: &str) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        if !state.accounts.iter().any(|a| a.id == id) {
            return Err(io::Error::other("That account is no longer on this Mac."));
        }
        if state.in_use == id { return Ok(()); }
        let mut next = state.clone();
        next.in_use = id.into();
        next.last_switch = None;
        // Round 18's Recent changes: "You chose Work."
        let label = next.accounts.iter().find(|a| a.id == id).map(|a| a.label.clone()).unwrap_or_default();
        push_change(&mut next, Change { at: crate::util::now_millis(), kind: "chose".into(), id: id.into(), label, from: None, why: None, used: None });
        self.save(&next)?;
        *state = next;
        *self.pending.lock().unwrap() = None;
        Ok(())
    }

    /// The account a switch would move to now: among the others with a reading and room, the
    /// one whose weekly window resets soonest (his answer 1). `None` when none has room.
    pub fn next(&self, readings: &BTreeMap<String, Reading>, pause: Option<u8>, now: u64) -> Option<Account> {
        let state = self.state.lock().unwrap();
        let limited = &state.limited_until;
        let id = next_account(state.accounts.iter().filter(|a| a.id != state.in_use)
            .filter_map(|a| readings.get(&a.id).map(|r| (a.id.as_str(), r)))
            .filter(|(id, r)| gone(r, state.at_threshold, pause, limited.get(*id).copied(), now).is_none()))?;
        state.accounts.iter().find(|a| a.id == id).cloned()
    }

    /// **Rich's reply `turn` is about to run on a lease under `account`**, and runs there until
    /// [`Accounts::turn_ended`]. If that is the account a switch went to, and no other reply of
    /// his and no back end is still on another account ([`Accounts::back_end_on`]), the switch
    /// has now actually happened and its notice is released for [`Accounts::take_notice`].
    /// Called by the conversation (`spine.rs`, `prepare_connection`).
    pub fn ran_on(&self, turn: &str, account: &str) {
        self.replies.lock().unwrap().insert(turn.into(), account.into());
        if let Some(pending) = self.pending.lock().unwrap().as_mut().filter(|p| p.to == account) {
            pending.ran = true;
        }
        self.release();
    }

    /// **Rich's reply `turn` has ended**, however it ended (`spine.rs`, `finish_delivery`): it
    /// runs on no account any more, and the switch may now be complete.
    pub fn turn_ended(&self, turn: &str) {
        self.replies.lock().unwrap().remove(turn);
        self.release();
    }

    /// **The switch decision, on the freshest readings.** `readings` holds every account that
    /// has a reading. Called before every turn (the spine and the work host) and after every
    /// probe. Returns whether the account in use changed. Running leases are NOT touched here:
    /// each one rotates at its own next turn boundary, because rotation never happens inside a
    /// turn (`spine.rs`).
    pub fn evaluate(&self, readings: &BTreeMap<String, Reading>, pause: Option<u8>, now: u64) -> io::Result<bool> {
        let mut state = self.state.lock().unwrap();
        if state.accounts.len() < 2 { return Ok(false); }
        let current = state.in_use.clone();
        let at = state.at_threshold;
        let limited: BTreeMap<String, u64> = state.limited_until.clone();
        let limit = |id: &str| limited.get(id).copied();
        let empty = Reading::default();
        let mine = readings.get(&current).unwrap_or(&empty);
        let Some((why, until)) = gone(mine, at, pause, limit(&current), now) else { return Ok(false) };
        let next = next_account(state.accounts.iter().filter(|a| a.id != current)
            .filter_map(|a| readings.get(&a.id).map(|r| (a.id.as_str(), r)))
            .filter(|(id, r)| gone(r, at, pause, limit(id), now).is_none()));
        let Some(next) = next.map(str::to_string) else { return Ok(false) };
        // When the account being left is fresh again: the reset of the window that made it go
        // (`gone` returns that reset; a weekly window with no reset time has none to say).
        let fresh_at = match why {
            Gone::Limit => limit(&current),
            _ => mine.windows.iter().filter_map(|w| w.resets_at).find(|t| *t == until),
        };
        self.switch_to(&mut state, &next, why, fresh_at, now)?;
        Ok(true)
    }

    /// **The backstop.** A lease on `account` was refused by a usage limit inside a turn. The
    /// account is marked gone until `resets_at` (or, when the refusal did not say, until the
    /// latest reset of its full windows), and the next account with room is put in use.
    pub fn limit_reached(&self, account: &str, resets_at: Option<u64>, readings: &BTreeMap<String, Reading>, pause: Option<u8>, now: u64) -> io::Result<AfterLimit> {
        {
            let mut state = self.state.lock().unwrap();
            let until = resets_at.filter(|t| *t > now).or_else(|| readings.get(account).and_then(|r| r.windows.iter()
                .filter(|w| w.used_percent >= crate::quota::resets::WEEKLY_THRESHOLD)
                .filter_map(|w| w.resets_at).filter(|t| *t > now).max()))
                .unwrap_or(now + UNKNOWN_RESET_MS);
            let mut next = state.clone();
            next.limited_until.retain(|_, t| *t > now);
            next.limited_until.insert(account.into(), until);
            self.save(&next)?;
            *state = next;
            if state.in_use != account {
                return Ok(AfterLimit::Continue);
            }
        }
        Ok(if self.evaluate(readings, pause, now)? { AfterLimit::Continue } else { AfterLimit::NoRoom })
    }

    /// **A reset was used on `account`** (hunt part 1 v3, finding 49): the usage-limit refusal
    /// the backstop recorded has been answered, so it no longer holds the account; the fresh
    /// reading taken next decides. A reset that did not take leaves that reading at the
    /// weekly threshold (a reset is only attempted there, `resets.rs`), which still holds it.
    pub fn reset_used(&self, account: &str) -> io::Result<()> {
        let mut state = self.state.lock().unwrap();
        if !state.limited_until.contains_key(account) { return Ok(()); }
        let mut next = state.clone();
        next.limited_until.remove(account);
        self.save(&next)?;
        *state = next;
        Ok(())
    }

    /// **All accounts gone** (Frank's finding 2): held until the soonest reset across them.
    /// `None` with one account, so a user with one subscription sees no change.
    pub fn held_until(&self, readings: &BTreeMap<String, Reading>, pause: Option<u8>, now: u64) -> Option<u64> {
        self.held(readings, pause, now).map(|(_, _, until)| until)
    }

    /// The hold when every account is gone, with the account that comes back first and why it
    /// is gone: its label, the reason, and when (round 16: "names the soonest reset by account").
    pub fn held(&self, readings: &BTreeMap<String, Reading>, pause: Option<u8>, now: u64) -> Option<(String, Gone, u64)> {
        let state = self.state.lock().unwrap();
        if state.accounts.len() < 2 { return None; }
        let mut soonest: Option<(String, Gone, u64)> = None;
        let empty = Reading::default();
        for account in &state.accounts {
            let reading = readings.get(&account.id);
            match gone(reading.unwrap_or(&empty), state.at_threshold, pause,
                state.limited_until.get(&account.id).copied(), now) {
                Some((why, until)) if soonest.as_ref().is_none_or(|(_, _, s)| until < *s) => {
                    soonest = Some((account.label.clone(), why, until));
                }
                // Gone, but not the soonest back.
                Some(_) => {}
                // An account with a reading and room: work can run.
                None if reading.is_some() => return None,
                // No reading: unknown, never counted as room.
                None => {}
            }
        }
        soonest
    }

    /// The one-line switch notice, taken once by whoever shows it. `None` while a switch is
    /// only decided ([`Accounts::ran_on`]).
    pub fn take_notice(&self) -> Option<String> { self.notice.lock().unwrap().take() }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::quota::tests::Scratch;

    pub const NOW: u64 = 1_000_000_000_000;
    const HOUR: u64 = 3_600_000;
    pub fn reading(five: f64, five_reset: u64, weekly: f64, weekly_reset: u64) -> Reading {
        Reading { windows: vec![
            Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: five, resets_at: Some(NOW + five_reset), duration_ms: 5 * HOUR },
            Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: weekly, resets_at: Some(NOW + weekly_reset), duration_ms: 168 * HOUR },
        ], speeds: BTreeMap::new(), expected: false, rises: BTreeMap::new(), weekly_cutoff: None }
    }
    fn two_accounts() -> (Scratch, Accounts) {
        let dir = Scratch::new();
        let accounts = Accounts::open(dir.path()).unwrap();
        accounts.add("Work").unwrap();
        (dir, accounts)
    }
    fn readings(one: Reading, two: Reading) -> BTreeMap<String, Reading> {
        [("1".to_string(), one), ("2".to_string(), two)].into_iter().collect()
    }
    /// One whole reply of Rich's on `account`: it starts there and ends.
    fn replied(accounts: &Accounts, account: &str) {
        let turn = format!("reply-{}", uuid::Uuid::new_v4());
        accounts.ran_on(&turn, account);
        accounts.turn_ended(&turn);
    }

    /// **His answer 1.** Account 1's weekly window is at 99%. Of the two with room, "Personal"
    /// (id 3) has its week end in one day and "Work" (id 2) in six, so Personal is next even
    /// though Work comes first in the list. "Spare" (id 4) has its week end soonest of all,
    /// but it is at 99.5%, so it has no room.
    #[test]
    fn the_next_account_is_the_one_whose_weekly_window_resets_soonest_among_those_with_room() {
        let dir = Scratch::new();
        let accounts = Accounts::open(dir.path()).unwrap();
        for label in ["Work", "Personal", "Spare"] { accounts.add(label).unwrap(); }
        let readings: BTreeMap<String, Reading> = [
            ("1", reading(40., HOUR, 99., 2 * 24 * HOUR)),
            ("2", reading(0., HOUR, 10., 6 * 24 * HOUR)),
            ("3", reading(0., HOUR, 50., 24 * HOUR)),
            ("4", reading(0., HOUR, 99.5, 3 * HOUR)),
        ].into_iter().map(|(id, r)| (id.to_string(), r)).collect();
        assert!(accounts.evaluate(&readings, Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "3");
        // Decided is not switched: the notice waits until something runs under Personal.
        assert_eq!(accounts.take_notice(), None);
        replied(&accounts, "1");
        assert_eq!(accounts.take_notice(), None, "a turn on the account being left says nothing");
        replied(&accounts, "3");
        assert_eq!(accounts.take_notice().as_deref(), Some("I switched the team to your **Personal** account. Account 1 had used 99% of its weekly limit, and it is fresh again in 2 d.\n\nNothing stopped. The team carried on right where it was."));
        let last = accounts.last_switch().unwrap();
        assert_eq!((last.from.as_str(), last.to.as_str(), last.at, last.why.as_str(), last.used), ("1", "3", NOW, "weekly", Some(99.)));
        // Staying is fill-first: Personal has room, so nothing moves, even though Work is
        // earlier in the list.
        assert!(!accounts.evaluate(&readings, Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "3");
        // The choice survives a restart.
        assert_eq!(Accounts::open(dir.path()).unwrap().in_use().id, "3");
    }

    /// **"I switched the team" only once the team has switched** (nightly 65, 2026-10-18: the
    /// line was released at Rich's first turn on Home, 22:49:03Z, while Stu's lease, opened on
    /// Work a second before the decision, ran the whole job on Work and stayed there). A
    /// conversation turn on the new account is not enough while a back end is on the old one;
    /// the line comes when that back end is on the new account too, or has no lease at all.
    #[test]
    fn the_switch_is_announced_only_once_every_back_end_is_on_the_new_account() {
        let (_dir, accounts) = two_accounts();
        accounts.back_end_on("thread-a", Some("1"));
        accounts.back_end_on("thread-b", Some("1"));
        assert!(accounts.evaluate(&readings(reading(10., HOUR, 99., 48 * HOUR), reading(0., HOUR, 0., 72 * HOUR)), None, NOW).unwrap());
        assert!(accounts.switch_pending());
        replied(&accounts, "2");
        assert_eq!(accounts.take_notice(), None, "both back ends are still on Account 1");
        accounts.back_end_on("thread-a", Some("2"));
        assert_eq!(accounts.take_notice(), None, "one back end is still on Account 1");
        accounts.back_end_on("thread-b", None);
        assert!(accounts.take_notice().unwrap().starts_with("I switched the team to your **Work** account."));
        assert!(!accounts.switch_pending());

        // With every back end already moved, the conversation's first turn on it says it.
        let (_dir, accounts) = two_accounts();
        assert!(accounts.evaluate(&readings(reading(10., HOUR, 99., 48 * HOUR), reading(0., HOUR, 0., 72 * HOUR)), None, NOW).unwrap());
        accounts.back_end_on("thread-a", Some("2"));
        assert_eq!(accounts.take_notice(), None, "no conversation turn has run on Work yet");
        replied(&accounts, "2");
        assert!(accounts.take_notice().is_some());
    }

    /// **Feedback item 8 (the CEO 2026-10-06): he picks which account drains first**, his own
    /// test case: Account 1 in use at 30% of its week, Work at 97%. He picks Work. It stays in
    /// use at 97% and at 98.9% (fill-first; Account 1 having more room never pulls it back),
    /// the choice survives a relaunch, and at 99% the automatic switch moves to Account 1 with
    /// the usual one line. Picking makes no line of its own and drops the last automatic
    /// switch's card.
    #[test]
    fn he_picks_the_account_that_drains_first_and_the_99_percent_switch_still_happens_from_it() {
        let (dir, accounts) = two_accounts();
        let one = reading(10., HOUR, 30., 72 * HOUR);
        assert!(accounts.evaluate(&readings(reading(95., HOUR, 99., 48 * HOUR), reading(5., HOUR, 20., 72 * HOUR)), None, NOW).unwrap());
        assert!(accounts.last_switch().is_some());
        accounts.use_first("1").unwrap();
        assert_eq!(accounts.in_use().id, "1");
        assert_eq!(accounts.last_switch(), None, "the automatic switch's card no longer describes the account in use");
        replied(&accounts, "2");
        assert_eq!(accounts.take_notice(), None, "his own choice is not announced as a switch");

        accounts.use_first("2").unwrap();
        assert_eq!(accounts.in_use().id, "2");
        assert!(!accounts.evaluate(&readings(one.clone(), reading(0., HOUR, 97., 48 * HOUR)), Some(93), NOW).unwrap());
        assert!(!accounts.evaluate(&readings(one.clone(), reading(0., HOUR, 98.9, 48 * HOUR)), Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "2", "his pick drains first");
        let accounts = Accounts::open(dir.path()).unwrap();
        assert_eq!(accounts.in_use().id, "2", "the pick survives a relaunch");

        assert!(accounts.evaluate(&readings(one, reading(0., HOUR, 99., 48 * HOUR)), Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "1", "at 99% of its week the switch moves on");
        replied(&accounts, "1");
        assert_eq!(accounts.take_notice().as_deref(), Some("I switched the team to **Account 1**. Work had used 99% of its weekly limit, and it is fresh again in 2 d.\n\nNothing stopped. The team carried on right where it was."));
        assert!(accounts.use_first("9").is_err(), "an account that is not on this Mac cannot be picked");
        assert_eq!(accounts.in_use().id, "1");
    }

    /// **A model's own week decides like the account's week** (follow-ups of 2026-10-29:
    /// "`model:Fable` never decides `leaving`"; the 2026-10-04 plan counts a model window used up
    /// as the account's). Account 1's week reads 40% but its Fable week 99%, fresh again in 2
    /// days: it is gone until that reset, and the switch moves to Work, saying when Account 1 is
    /// fresh again. Below its point (98%), or with its reset already passed, nothing moves. On
    /// main only `seven_day` was read, so Account 1 stayed in use until Fable's usage limit
    /// refused it.
    #[test]
    fn a_models_own_week_at_its_point_leaves_the_account() {
        let fable = |used: f64, reset: u64| {
            let mut r = reading(10., HOUR, 40., 72 * HOUR);
            r.windows.push(Window { id: "model:Fable".into(), label: "Weekly · Fable".into(), used_percent: used,
                resets_at: Some(reset), duration_ms: 168 * HOUR });
            r
        };
        let fresh_in_two_days = NOW + 48 * HOUR;
        assert_eq!(gone(&fable(99., fresh_in_two_days), AtThreshold::Pause, Some(93), None, NOW),
            Some((Gone::Weekly(99.), fresh_in_two_days)));
        assert!(fable(99., fresh_in_two_days).leaving(NOW), "`leaving` and the switch read the same windows");
        assert_eq!(gone(&fable(98., fresh_in_two_days), AtThreshold::Pause, Some(93), None, NOW), None, "under its point");
        assert_eq!(gone(&fable(99., NOW - 1), AtThreshold::Pause, Some(93), None, NOW), None, "its reset has passed: a fresh week");
        assert!(!fable(99., NOW - 1).leaving(NOW));

        let (_dir, accounts) = two_accounts();
        assert!(accounts.evaluate(&readings(fable(99., fresh_in_two_days), reading(5., HOUR, 20., 72 * HOUR)), Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "2");
        replied(&accounts, "2");
        assert!(accounts.take_notice().is_some_and(|line| line.contains("fresh again in 2 d")),
            "fresh again at the Fable week's reset");
    }

    /// **Round 16: the one switch is the subject of the sentence.** Switch chosen, Account 1's
    /// five-hour window at 95%: with the automatic switch off nothing happens at the line (it
    /// is only drawn); on, the next account takes over. The weekly 99% switch happens either
    /// way, because that is what a second account is for.
    #[test]
    fn with_the_switch_off_nothing_happens_at_the_five_hour_line_but_the_weekly_switch_still_does() {
        let (_dir, accounts) = two_accounts();
        accounts.set_at_threshold(AtThreshold::Switch).unwrap();
        let past_line = readings(reading(95., HOUR, 30., 48 * HOUR), reading(5., HOUR, 5., 72 * HOUR));
        assert!(!accounts.evaluate(&past_line, None, NOW).unwrap(), "off: the five-hour line is only drawn");
        assert_eq!(accounts.in_use().id, "1");
        assert_eq!(accounts.held_until(&past_line, None, NOW), None, "off: Account 1 is not counted as gone");
        assert!(accounts.evaluate(&past_line, Some(93), NOW).unwrap(), "on: Switch moves at the line");
        assert_eq!(accounts.in_use().id, "2");
        let (_dir, accounts) = two_accounts();
        let weekly = readings(reading(10., HOUR, 99., 48 * HOUR), reading(5., HOUR, 5., 72 * HOUR));
        assert!(accounts.evaluate(&weekly, None, NOW).unwrap(), "off: the weekly 99% switch still happens");
        assert_eq!(accounts.in_use().id, "2");
    }

    /// **§108, normal speed:** the weekly switch point is 99%. At 98.9% nothing moves; at 99%
    /// the next account takes over. A slow, steady speed (the weekly window's own even pace,
    /// 100 points a week) does not move it earlier.
    #[test]
    fn at_normal_speed_the_weekly_switch_happens_at_99_percent() {
        let (_dir, accounts) = two_accounts();
        let mut slow = reading(10., HOUR, 98.9, 24 * HOUR);
        slow.speeds.insert("seven_day".into(), 100. / (168. * HOUR as f64));
        assert!(!slow.fast());
        assert!(!accounts.evaluate(&readings(slow.clone(), reading(0., HOUR, 5., 48 * HOUR)), Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "1", "98.9% at normal speed stays");
        slow.windows[1].used_percent = 99.;
        assert!(accounts.evaluate(&readings(slow, reading(0., HOUR, 5., 48 * HOUR)), Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "2", "99% at normal speed switches");
    }

    /// **§108 addendum (plan answer 8), a fast burn on the WEEKLY window**, at the speed the
    /// 2026-09-29 run measured on its five-hour window (4 points a minute; richos-hq
    /// `docs/research/2026-10-04-fifteen-fable-workers-quota-burn.md`), used here as the worst
    /// case: no weekly figure was measured for that run. That is fast (20 points per
    /// five-minute check), so the next check is one minute away. **The weekly point is the
    /// handoff point** (weekly-switch plan §1, his words 2026-10-07: "the percentage for the
    /// switch needs to be adjusted dynamically so that this doesn't happen", "this" being 100%
    /// before the switch): the next check and a 5-minute handoff would use 4 x 6 = 24 points,
    /// so it is 99 - (24 - 1) = 76, and 76 + 24 = 100. At 75.9% it stays; at 76% it switches,
    /// so the next check and the handoff end by 100%.
    #[test]
    fn a_fast_weekly_burn_switches_as_early_as_its_speed_requires() {
        let (_dir, accounts) = two_accounts();
        let per_ms = 4.0 / 60_000.;
        let mut fast = reading(10., HOUR, 75.9, 24 * HOUR);
        fast.speeds.insert("seven_day".into(), per_ms);
        assert!(fast.fast());
        assert_eq!(fast.interval(), crate::quota::FAST_REFRESH_INTERVAL_MS);
        assert!((fast.weekly_point(&fast.windows[1]) - 76.).abs() < 1e-9);
        assert!(!accounts.evaluate(&readings(fast.clone(), reading(0., HOUR, 5., 48 * HOUR)), Some(93), NOW).unwrap());
        fast.windows[1].used_percent = 76.;
        assert!(accounts.evaluate(&readings(fast, reading(0., HOUR, 5., 48 * HOUR)), Some(93), NOW).unwrap());
        assert_eq!(accounts.in_use().id, "2", "switched at 76%, 24 points before 100%");
    }

    /// **Round 18, why it switched: the line in the conversation**, in the mockup's words (the
    /// CEO 2026-10-06, feedback item 9). Which account the team is on now, which one was left
    /// and why, and when that one is fresh again, as a clock time once the webview has said its
    /// offset. A usage-limit refusal says the step runs again. No m-dash or n-dash.
    #[test]
    fn the_switch_line_says_which_account_why_and_when_the_one_left_is_fresh_again() {
        let (_dir, accounts) = two_accounts();
        accounts.rename("1", "Home").unwrap();
        accounts.set_utc_offset(0);
        accounts.set_at_threshold(AtThreshold::Switch).unwrap();
        // NOW is Sunday 2001-09-09 1:46:40 AM UTC; Home's five-hour window resets an hour on.
        assert!(accounts.evaluate(&readings(reading(95., HOUR, 30., 72 * HOUR), reading(5., HOUR, 5., 96 * HOUR)), Some(93), NOW).unwrap());
        replied(&accounts, "2");
        let line = accounts.take_notice().unwrap();
        assert_eq!(line, "I switched the team to your **Work** account. Home had used 95% of its 5-hour limit, and it is fresh again today at 2:47 AM.\n\nNothing stopped. The team carried on right where it was.");
        assert!(!line.contains('\u{2014}') && !line.contains('\u{2013}'));
        // Work's week at 99.4%, fresh again Wednesday: back to Home.
        assert!(accounts.evaluate(&readings(reading(5., 5 * HOUR, 30., 72 * HOUR), reading(5., HOUR, 99.4, 72 * HOUR)), Some(93), NOW).unwrap());
        replied(&accounts, "1");
        assert_eq!(accounts.take_notice().as_deref(), Some("I switched the team to your **Home** account. Work had used 99% of its weekly limit, and it is fresh again on Wednesday at 1:47 AM.\n\nNothing stopped. The team carried on right where it was."));
        // A usage-limit refusal inside a turn on Home: the step runs again on Work.
        let room = readings(reading(5., 5 * HOUR, 30., 72 * HOUR), reading(5., HOUR, 10., 72 * HOUR));
        assert_eq!(accounts.limit_reached("1", Some(NOW + HOUR), &room, Some(93), NOW).unwrap(), AfterLimit::Continue);
        replied(&accounts, "2");
        assert_eq!(accounts.take_notice().as_deref(), Some("I switched the team to your **Work** account. Home reached a Claude usage limit, so the step it turned away runs again on Work."));
    }

    /// **Round 18, why it switched: Recent changes**, the third place. Adding is said once the
    /// sign-in comes back as a different account (not when the folder is made, and once however
    /// often it signs in again); choosing, the automatic switch with its reason, and removing
    /// are each a row. An adding canceled during its sign-in leaves no row at all. The rows
    /// survive a relaunch.
    #[test]
    fn recent_changes_record_adding_choosing_switching_and_removing_and_a_canceled_adding_leaves_nothing() {
        let dir = Scratch::new();
        let accounts = Accounts::open(dir.path()).unwrap();
        let work = accounts.add("Work").unwrap();
        assert!(accounts.changes().is_empty(), "added is said when the sign-in comes back, not before");
        accounts.signed_in(&work.id, NOW).unwrap();
        accounts.signed_in(&work.id, NOW + 1).unwrap();
        accounts.use_first(&work.id).unwrap();
        assert!(accounts.evaluate(&readings(reading(10., HOUR, 30., 72 * HOUR), reading(0., HOUR, 99., 48 * HOUR)), Some(93), NOW + 2).unwrap());
        accounts.remove(&work.id).unwrap();
        let spare = accounts.add("Spare").unwrap();
        assert_eq!(spare.id, work.id, "the id is reused, and its old rows must not count as this adding");
        accounts.remove(&spare.id).unwrap();
        let rows = |a: &Accounts| a.changes().iter().map(|c| (c.kind.clone(), c.label.clone(), c.from.clone(), c.why.clone())).collect::<Vec<_>>();
        let expected = vec![
            ("added".to_string(), "Work".to_string(), None, None),
            ("chose".into(), "Work".into(), None, None),
            ("switched".into(), "Account 1".into(), Some("Work".to_string()), Some("weekly".to_string())),
            ("removed".into(), "Work".into(), None, None),
        ];
        assert_eq!(rows(&accounts), expected);
        assert_eq!(rows(&Accounts::open(dir.path()).unwrap()), expected, "Recent changes survive a relaunch");
    }

    /// Account 1's Claude Code folder (`home`) and the app's data folder, side by side, with the
    /// records shared as the desktop entrypoint shares them.
    fn shared(dir: &Scratch) -> (PathBuf, Accounts) {
        let home = dir.path().join("home");
        let accounts = Accounts::open(&dir.path().join("data")).unwrap();
        accounts.share_records(&home).unwrap();
        (home, accounts)
    }
    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// **(a) The CEO 2026-11-01: removing an account removed the chats that had run on it.** A
    /// chat run on Work is written by Claude Code under Work's folder (`CLAUDE_CONFIG_DIR`); that
    /// is Account 1's records folder now, so removing Work removes its sign-in and nothing else.
    #[test]
    fn removing_an_account_removes_only_its_sign_in_and_every_chats_record_stays() {
        let dir = Scratch::new();
        let (home, accounts) = shared(&dir);
        let work = accounts.add("Work").unwrap();
        let folder = work.folder.clone().unwrap();
        write(&folder.join(".claude.json"), "{\"oauthAccount\":{}}");
        let record = folder.join("projects/-coordination/s1.jsonl");
        write(&record, "{\"type\":\"user\",\"message\":\"hello from Work\"}\n");
        write(&folder.join("projects/-coordination/s1/subagents/agent-h.jsonl"), "helper\n");
        accounts.remove(&work.id).unwrap();
        assert!(!folder.exists(), "the sign-in folder is gone");
        assert_eq!(fs::read_to_string(home.join("projects/-coordination/s1.jsonl")).unwrap(),
            "{\"type\":\"user\",\"message\":\"hello from Work\"}\n", "the chat's record stays");
        assert_eq!(fs::read_to_string(home.join("projects/-coordination/s1/subagents/agent-h.jsonl")).unwrap(), "helper\n");
        assert_eq!(record_in(&record, &home.join(RECORDS)), Some(home.join("projects/-coordination/s1.jsonl")),
            "the path Claude Code named under Work finds the record");
    }

    /// **Removing never deletes a record, even with nowhere to move it**: before the shared place
    /// is named (or where a link is refused), Work's records stay in its folder when it is removed
    /// and everything else goes; the next launch moves them into the shared place.
    #[test]
    fn removing_an_account_with_nowhere_to_move_its_records_keeps_them_and_the_next_launch_moves_them() {
        let dir = Scratch::new();
        let accounts = Accounts::open(&dir.path().join("data")).unwrap();
        let work = accounts.add("Work").unwrap().folder.unwrap();
        write(&work.join(".claude.json"), "{}");
        write(&work.join("projects/-p/s.jsonl"), "kept\n");
        accounts.remove("2").unwrap();
        assert!(!work.join(".claude.json").exists(), "the sign-in goes");
        assert_eq!(fs::read_to_string(work.join("projects/-p/s.jsonl")).unwrap(), "kept\n", "the record stays");
        let home = dir.path().join("home");
        Accounts::open(&dir.path().join("data")).unwrap().share_records(&home).unwrap();
        assert_eq!(fs::read_to_string(home.join("projects/-p/s.jsonl")).unwrap(), "kept\n");
        assert!(!work.join("projects").exists());
    }

    /// **(b) A chat started on one account resumes on another**: both accounts' Claude Code
    /// write and read the same records folder, so a session Work wrote is the file Account 1
    /// resumes, and one Account 1 wrote is the file Work resumes.
    #[test]
    fn a_chat_started_on_one_account_is_the_same_record_on_every_other() {
        let dir = Scratch::new();
        let (home, accounts) = shared(&dir);
        let work = accounts.add("Work").unwrap().folder.unwrap();
        let spare = accounts.add("Spare").unwrap().folder.unwrap();
        write(&work.join("projects/-p/on-work.jsonl"), "work turn\n");
        write(&home.join("projects/-p/on-home.jsonl"), "home turn\n");
        for folder in [&home, &work, &spare] {
            assert_eq!(fs::read_to_string(folder.join("projects/-p/on-work.jsonl")).unwrap(), "work turn\n");
            assert_eq!(fs::read_to_string(folder.join("projects/-p/on-home.jsonl")).unwrap(), "home turn\n");
        }
        // Continued on Spare: the turn is in the one record every account reads.
        use std::io::Write;
        fs::OpenOptions::new().append(true).open(spare.join("projects/-p/on-work.jsonl")).unwrap().write_all(b"spare turn\n").unwrap();
        assert_eq!(fs::read_to_string(home.join("projects/-p/on-work.jsonl")).unwrap(), "work turn\nspare turn\n");
    }

    /// **(c) An install whose records are inside an added account's folder** (every version
    /// before this one) has them moved into the shared place once, losing nothing: a record only
    /// Work had is moved; one both had with the same bytes is kept once; one both had differently
    /// keeps the newer under its name and the other beside it; a removed account's left-behind
    /// records join too; Account 1's own records are untouched; and a second launch moves nothing.
    #[test]
    fn an_install_with_records_in_an_account_folder_moves_them_once_into_the_shared_place_without_loss() {
        let dir = Scratch::new();
        let home = dir.path().join("home");
        let accounts = Accounts::open(&dir.path().join("data")).unwrap();
        let work = accounts.add("Work").unwrap().folder.unwrap();
        let orphan = dir.path().join("data/claude-accounts/7");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        let aged = |path: &Path| fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
        write(&work.join("projects/-p/only-work.jsonl"), "only work\n");
        write(&work.join("projects/-p/only-work/subagents/agent-a.jsonl"), "helper\n");
        write(&work.join("projects/-p/memory/MEMORY.md"), "memory\n");
        write(&work.join("projects/-p/same.jsonl"), "same\n");
        write(&work.join("projects/-p/both.jsonl"), "home turn\nwork turn\n");
        write(&home.join("projects/-p/same.jsonl"), "same\n");
        write(&home.join("projects/-p/both.jsonl"), "home turn\n");
        aged(&home.join("projects/-p/both.jsonl"));
        write(&home.join("projects/-t/terminal.jsonl"), "terminal\n");
        write(&orphan.join("projects/-q/removed.jsonl"), "removed account\n");
        write(&work.join(".claude.json"), "{}");

        accounts.share_records(&home).unwrap();
        let shared = home.join("projects");
        assert_eq!(fs::read_link(work.join("projects")).unwrap(), shared, "Work's records folder is the shared one");
        let read = |rel: &str| fs::read_to_string(shared.join(rel)).unwrap();
        assert_eq!(read("-p/only-work.jsonl"), "only work\n");
        assert_eq!(read("-p/only-work/subagents/agent-a.jsonl"), "helper\n");
        assert_eq!(read("-p/memory/MEMORY.md"), "memory\n");
        assert_eq!(read("-p/same.jsonl"), "same\n");
        assert!(!shared.join("-p/same.jsonl.from-account-2").exists(), "the same record twice is kept once");
        assert_eq!(read("-p/both.jsonl"), "home turn\nwork turn\n", "the newer copy keeps the record's name");
        assert_eq!(read("-p/both.jsonl.from-account-2"), "home turn\n", "the other copy is kept beside it");
        assert_eq!(read("-t/terminal.jsonl"), "terminal\n", "Account 1's own records are untouched");
        assert_eq!(read("-q/removed.jsonl"), "removed account\n", "a removed account's records join");
        assert!(!orphan.join("projects").exists() && orphan.exists(), "only the emptied records folder goes");
        assert!(work.join(".claude.json").exists(), "the sign-in stays where it is");

        let listing = |root: &Path| { let mut all = Vec::new(); let mut stack = vec![root.to_path_buf()];
            while let Some(d) = stack.pop() { for e in fs::read_dir(&d).unwrap().flatten() {
                if e.file_type().unwrap().is_dir() { stack.push(e.path()) } else { all.push((e.path(), fs::read(e.path()).unwrap())) } } }
            all.sort(); all };
        let before = listing(&shared);
        let again = Accounts::open(&dir.path().join("data")).unwrap();
        again.share_records(&home).unwrap();
        assert_eq!(listing(&shared), before, "a second launch moves nothing");
    }
}
