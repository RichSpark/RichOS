//! Read-only desktop receipt summaries, scoped by the ledger's thread binding.
//! These describe persisted evidence, not worker liveness or whole-task completion.
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct WorkSummary {
    pub items: Vec<WorkItem>,
    pub omitted: usize,
}
#[derive(Debug, Serialize)]
pub struct WorkItem {
    pub title: String,
    pub repository: String,
    pub role: String,
    pub state: String,
    pub detail: String,
}

/// The existing engine receipt lock, shared with dispatch. Stop persists revocation
/// before waiting for it, snapshots native call identities while holding it and
/// releases it before waiting for provider control replies.
pub fn lock_receipts(state: &Path, entity: &str, thread: &str) -> Result<std::fs::File, String> {
    let identity = serde_json::to_vec(&[entity, thread]).map_err(|e| e.to_string())?;
    let parent = state.join("work-receipts");
    let root = parent.join(format!("{:x}", Sha256::digest(identity)));
    if parent.is_symlink() || root.is_symlink() { return Err("Work records have been redirected.".into()); }
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let lock = options.open(root.join(".lock")).map_err(|e| e.to_string())?;
    #[cfg(unix)] {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    Ok(lock)
}

pub fn read(state: &Path, entity: &str, thread: &str) -> Result<WorkSummary, String> {
    let identity = serde_json::to_vec(&[entity, thread]).map_err(|e| e.to_string())?;
    let partition = format!("{:x}", Sha256::digest(identity));
    let parent = state.join("work-receipts");
    let workspaces = state.join("workspaces").join(&partition);
    let root = parent.join(partition);
    if parent.is_symlink() || root.is_symlink() { return Err("Work records have been redirected.".into()); }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(WorkSummary::default()),
        Err(_) => return Err("Saved work could not be read.".into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|_| "Saved work could not be listed.")?.path();
        if path.extension().is_some_and(|e| e == "json") { paths.push(path); }
        if paths.len() > 10000 { return Err("There are too many saved work records for this view. Ask Rich to inspect a specific assignment.".into()); }
    }
    // The hundred shown are the hundred most recently written, not the first hundred
    // filenames: a receipt id says nothing about which work is current. The bounds on records
    // and bytes stay (they limit resource use); only the choice of WHICH records changed.
    // Receipts are rewritten as their state advances, so the modification time is the last
    // activity. Ties fall back to the path so the order is deterministic.
    let mut dated = Vec::with_capacity(paths.len());
    for path in paths {
        // A record that cannot even be dated sorts as the oldest; inside the hundred it is
        // still shown, as unreadable.
        let modified = std::fs::symlink_metadata(&path).ok().and_then(|meta| meta.modified().ok());
        dated.push((modified.unwrap_or(std::time::UNIX_EPOCH), path));
    }
    dated.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let paths: Vec<_> = dated.into_iter().map(|(_, p)| p).collect();
    let mut summary = WorkSummary { omitted: paths.len().saturating_sub(100), ..Default::default() };
    let mut bytes_read = 0;
    // Every shown record is read first, so each row can be told what the LATER rows of the
    // same assignment did to it (`Later`). A record that cannot be read contributes nothing.
    let rows: Vec<Result<Option<Value>, String>> = paths.iter().take(100)
        .map(|path| read_row(path, entity, thread, &mut bytes_read)).collect();
    let later = Later::of(rows.iter().filter_map(|row| row.as_ref().ok()?.as_ref()));
    // **One record the reader cannot read never hides the others** (Ray, test VM,
    // 2026-10-13). Each failure below is about ONE file, so it becomes one row saying that
    // file could not be read, and nothing from that file is shown: not its title, not its
    // repository, and never a guess at its state. Only a failure about the directory itself
    // (above) still refuses the whole view.
    for row in rows {
        let item = row.and_then(|row| match row {
            Some(row) => describe(&row, &later, &workspaces, &mut bytes_read).map(Some),
            None => Ok(None),
        }).unwrap_or_else(|_reason| Some(WorkItem {
            title: "Work whose details couldn't be read".into(),
            repository: "unknown repository".into(),
            role: "unknown role".into(),
            state: "unreadable".into(),
            // Plain words for a business owner (hunt 57, finding 11): the reader's own reason
            // is about files and records, which he cannot act on.
            detail: "Its details are damaged or missing, so none of them are shown. Everything else here is as saved.".into(),
        }));
        summary.items.extend(item);
    }
    Ok(summary)
}

/// The same terminal evidence as the engine's `retired_execution`: active cleanup
/// intent is insufficient, and a native result or verified integration is preserved.
/// Read only this conversation's canonical record, never rewrite attempt history.
fn retired_execution(workspaces: &Path, row: &Value, bytes_read: &mut u64) -> Result<bool, String> {
    let Some(key) = row["workspace_ref"].as_str() else { return Ok(false) };
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) || matches!(key, "." | "..") {
        return Err("A saved work reference is invalid.".into());
    }
    if [workspaces.parent().unwrap(), workspaces, &workspaces.join("agents"), &workspaces.join("done")].iter().any(|p| p.is_symlink()) {
        return Err("Work records have been redirected.".into());
    }
    if workspaces.join("agents").join(format!("{key}.json")).try_exists().map_err(|e| e.to_string())? { return Ok(false) }
    let path = workspaces.join("done").join(format!("{key}.json"));
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    if !meta.is_file() || meta.len() > 1024 * 1024 || *bytes_read + meta.len() > 16 * 1024 * 1024 {
        return Err("A saved work record is too large or redirected.".into());
    }
    *bytes_read += meta.len();
    let canonical: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if canonical["key"] != key { return Err("A saved work reference has a different identity.".into()) }
    Ok(canonical["end"].is_null() && canonical["disposition"]["kind"] == "landed")
}

/// One receipt, read into one row of the saved-work view, or the reason it could not be.
///
/// **`None` for a job that never started** (Ray's check of nightly 57, 2026-10-15: an
/// "Ended without starting. Its preparation was refused or closed before any worker was
/// dispatched" card under Under the hood). No worker ran and nothing was done, so there is
/// nothing of it to show. Which records those are is the engine's own `never_dispatched`
/// (`engine/mega-lander/app.py`): `blocked`, with no `tool_use_id` and no `agent_id`.
fn read_row(path: &Path, entity: &str, thread: &str, bytes_read: &mut u64) -> Result<Option<Value>, String> {
    let size = std::fs::symlink_metadata(path).map_err(|_| "A saved work record could not be read.")?;
    if !size.is_file() || size.len() > 1024 * 1024 || *bytes_read + size.len() > 16 * 1024 * 1024 {
        return Err("A saved work record is too large or redirected.".into());
    }
    *bytes_read += size.len();
    let bytes = std::fs::read(path).map_err(|_| "A saved work record could not be read.")?;
    let row: Value = serde_json::from_slice(&bytes).map_err(|_| "A saved work record is damaged.")?;
    if row["schema"] != 1 || row["binding"]["entity_id"] != entity || row["binding"]["thread_id"] != thread {
        return Err("A saved work record has an unsupported schema or different scope.".into());
    }
    let dispatched = |key: &str| row[key].as_str().is_some_and(|s| !s.is_empty());
    if row["status"] == "blocked" && !dispatched("tool_use_id") && !dispatched("agent_id") {
        return Ok(None);
    }
    Ok(Some(row))
}

/// **What later rows of the same assignment did to an earlier one** (Ray, nightly 65 run U
/// from 62, 2026-10-18). The month-end guide was written, fixed after a review, corrected
/// again and pushed; Under the hood still said of the first two steps "It still needs a review
/// before it's added to your project." and of the superseded review "A reviewer asked for
/// changes." A row says what is true NOW, and that depends on the rows after it.
///
/// Every link here is one the engine wrote itself, never a guess from titles or times:
/// - `continuation.worker_id` on a worker names the worker it continues. The engine starts
///   that worker at the earlier one's saved commit and lands only a commit the integration
///   branch then contains (`engine/mega-lander/app.py`, `prepare` and `integrate`), so every
///   worker up a landed worker's continuation chain is in the project. This is the same walk
///   the engine makes to clean those workspaces up after a land (`ancestors` in `integrate`),
///   with the same bounds: one assignment, one repository, at most 50 links, no cycle.
/// - `review_target.worker_id` on a reviewer names the worker whose commit it reviewed.
///
/// Only the rows this view shows are read, so a row is never described by a record that is
/// not on screen beside it.
#[derive(Default)]
struct Later {
    /// Workers with a verified integration of their own.
    landed: std::collections::HashSet<String>,
    /// Workers whose commit went into the project with a later, continuing worker's land.
    landed_with_later: std::collections::HashSet<String>,
    /// Workers that a later worker continues.
    continued: std::collections::HashSet<String>,
    /// Every worker shown, by the job and repository it worked on (`request.obligation_id`,
    /// `request.repo`): the other attempts at the same work.
    attempts: std::collections::HashMap<(String, String), Vec<String>>,
}

impl Later {
    /// **Second review of `0e974e68b`, P3 (follow-up 7 after nightly 65)**: "This attempt ended
    /// without a result" was shown the same way whether or not another attempt replaced it or
    /// the work was done. The engine records no "replaces" link between a retired attempt and a
    /// fresh one (nightly 65's retry started with no `continue_of`), so what a row can say is
    /// what the other attempts at the same job and repository did: one of them landed, or one
    /// is shown here, or there is none.
    fn other_attempts(&self, row: &Value) -> (bool, bool) {
        let key = (row["request"]["obligation_id"].as_str().unwrap_or("").to_string(),
                   row["request"]["repo"].as_str().unwrap_or("").to_string());
        let me = row["id"].as_str();
        let others: Vec<&String> = self.attempts.get(&key).into_iter().flatten().filter(|id| Some(id.as_str()) != me).collect();
        (others.iter().any(|id| self.landed.contains(*id) || self.landed_with_later.contains(*id)), !others.is_empty())
    }
}

impl Later {
    fn of<'a>(rows: impl Iterator<Item = &'a Value>) -> Later {
        let rows: Vec<&Value> = rows.collect();
        let by_id = |id: &str| rows.iter().copied().find(|row| row["id"].as_str() == Some(id));
        let worker = |row: &Value| row["request"]["role"].as_str() == Some("worker");
        let mut later = Later::default();
        for row in rows.iter().copied().filter(|row| worker(row)) {
            let Some(id) = row["id"].as_str() else { continue };
            if let (Some(job), Some(repo)) = (row["request"]["obligation_id"].as_str(), row["request"]["repo"].as_str()) {
                later.attempts.entry((job.to_string(), repo.to_string())).or_default().push(id.to_string());
            }
            if let Some(previous) = row["continuation"]["worker_id"].as_str() {
                later.continued.insert(previous.to_string());
            }
            if row["integration"]["verified"] != true {
                continue;
            }
            later.landed.insert(id.to_string());
            let mut seen = std::collections::HashSet::from([id.to_string()]);
            let mut step = row;
            while let Some(previous) = step["continuation"]["worker_id"].as_str() {
                let Some(earlier) = by_id(previous) else { break };
                let same = ["obligation_id", "repo", "role"].iter().all(|key| earlier["request"][key] == row["request"][key]);
                if !same || seen.contains(previous) || seen.len() >= 50 {
                    break;
                }
                seen.insert(previous.to_string());
                later.landed_with_later.insert(previous.to_string());
                step = earlier;
            }
        }
        later
    }
}

fn describe(row: &Value, later: &Later, workspaces: &Path, bytes_read: &mut u64) -> Result<WorkItem, String> {
    let field = |key: &str| -> Result<String, String> {
        row["request"][key].as_str().filter(|s| !s.is_empty() && s.len() <= 4096)
            .map(str::to_owned).ok_or_else(|| "A saved work record is incomplete.".into())
    };
    // A consult (plan §3, slice 3) has no repository and nothing to integrate: its record
    // is the teammate's answer, handed back to the back end.
    let consult = row["request"]["role"].as_str() == Some("consult");
    let id = row["id"].as_str();
    let verdict = row["review_observation"]["report"]["verdict"].as_str();
    //
    // **EVERY SENTENCE BELOW IS FOR A BUSINESS OWNER** (hunt 57, finding 11, 2026-10-15): the
    // Under the hood panel said "consult · no repository", "It changed no repository.",
    // "Workspace prepared. No worker dispatch has been observed." and "A current running state
    // is not established by this saved receipt." to a user whose jobs never touched a
    // repository. Each one now says what he needs to know, in his words, and still claims no
    // more than the record shows: a start is "started", never "running"; a land is "added to
    // your project" only with verified integration. Nothing names a repository, a worker, a
    // receipt, a workspace or a dispatch (`every_saved_work_sentence_is_in_plain_words`).
    let (state, detail) = if consult && row["status"] == "run-ended" {
        if row["consult_answer"]["answered"] == true {
            ("answered", "Done. The teammate's answer went back to Rich.")
        } else {
            ("run-ended", "The teammate stopped without an answer. Ask Rich about it.")
        }
    } else if row["integration"]["verified"] == true {
        if row["integration"]["cleanup_pending"].as_array().is_some_and(|a| a.is_empty()) {
            ("integrated", "This part was checked and added to your project.")
        } else {
            ("cleanup-pending", "This part was checked and added to your project. Some tidying up is still left.")
        }
    } else if !consult && id.is_some_and(|id| later.landed_with_later.contains(id)) {
        // Its commit is in the project, carried there by the later step that continued it.
        ("integrated-later", "Done. This part was added to your project together with a later step.")
    } else if !consult && id.is_some_and(|id| later.continued.contains(id)) {
        // The engine continues only settled work (`run-ended` or `interrupted`), so this step
        // has ended and a later one starts from where it stopped.
        ("continued", "This step ended. A later step carries on from where it stopped.")
    } else if retired_execution(workspaces, row, bytes_read)? {
        match later.other_attempts(row) {
            // "Other work on the same job", never "another attempt": the other workers of one job
            // may be independent parts of it, and no receipt says one replaced this one (review
            // rv-20261018T200142Z-c700e59d-1f6e).
            (true, _) => ("retired-other-added", "This attempt ended without a result. Other work on the same job was added to your project."),
            (false, true) => ("retired-other-shown", "This attempt ended without a result. Other work on the same job is shown separately."),
            (false, false) => ("retired", "This attempt ended without a result."),
        }
    } else if row["status"] == "run-ended" && row["review_observation"]["valid"] == true
        && matches!(verdict, Some("passed" | "changes-requested"))
    {
        // A review is about one worker's commit. What happened to that commit since is what
        // the row has to say, or a review a later one replaced reads as the current state.
        let target = row["review_target"]["worker_id"].as_str();
        let reviewed = |set: &std::collections::HashSet<String>| target.is_some_and(|t| set.contains(t));
        let passed = verdict == Some("passed");
        match (passed, reviewed(&later.landed), reviewed(&later.landed_with_later), reviewed(&later.continued)) {
            (true, true, _, _) => ("review-passed-added", "A reviewer approved it and it was added to your project."),
            (false, true, _, _) => ("review-changes-added", "A reviewer asked for changes. It has since been added to your project."),
            (true, _, true, _) => ("review-passed-later-added", "A reviewer approved this version. A later version was added to your project."),
            (false, _, true, _) => ("review-changes-later-added", "A reviewer asked for changes. A later version was added to your project."),
            (true, _, _, true) => ("review-passed-continued", "A reviewer approved this version. A later version is shown separately."),
            (false, _, _, true) => ("review-changes-continued", "A reviewer asked for changes. A later version is shown separately."),
            (true, ..) => ("review-passed", "A reviewer approved it. Adding it to your project is a separate step."),
            (false, ..) => ("changes-requested", "A reviewer asked for changes. Nothing has been added to your project yet."),
        }
    } else {
        // Every status `engine/mega-lander/app.py` writes into a receipt: `preparing`,
        // `prepared`, `dispatching`, `running`, `blocked`, `unknown`, `interrupted`,
        // `run-ended` and `integrated`.
        match row["status"].as_str() {
            Some("preparing" | "unknown") => ("unresolved", "It isn't clear how this went. Ask Rich to check it before trying again."),
            Some("prepared") => ("prepared", "Ready to start. No teammate has picked it up yet."),
            Some("dispatching") => ("dispatching", "Handed to a teammate, who hasn't confirmed starting yet."),
            // "Started", never "running": the record shows a start, not that it is still going.
            Some("running") => ("observed-start", "A teammate started on this."),
            // Both places the engine writes `blocked` come before any dispatch: a preparation
            // that was refused (Pierce's refusal included), and `settle_undispatched` closing
            // one that never reached a worker. So it ended, and it never started.
            Some("blocked") => ("ended-without-starting", "Ended without starting. It was stopped before a teammate began, so nothing was done."),
            Some("interrupted") => ("interrupted", "Interrupted. The work so far is kept. Ask Rich to carry on with it."),
            Some("run-ended") => ("run-ended", "The teammate finished. It still needs a review before it's added to your project."),
            // The engine writes `integrated` only together with `integration.verified`, which
            // is read above. Without that evidence the land is not claimed.
            Some("integrated") => ("unresolved", "It's marked as added to your project, but that couldn't be confirmed. Ask Rich to check."),
            _ => return Err("A saved work record has an unsupported state.".into()),
        }
    };
    let repository = if consult { "no repository".to_string() } else { field("repo")? };
    Ok(WorkItem { title: field("title")?, repository, role: field("role")?, state: state.into(), detail: detail.into() })
}

// =======================================================================================
// THE LAND RECORD, READ OFF THE RECEIPTS — CEO ruling §52, 2026-09-18
// =======================================================================================
//
// *"There's nothing that ever not lands on its own here in the terminal … So, yes, always
// land on its own."* A job that lands on its own has to be able to tell him WHAT it landed,
// and this is where that comes from.
//
// **The engine writes the land into the work receipt, and the app reads it there rather
// than anywhere else.** `integrate` records `integration = {reviewer_id, commit, branch,
// before, verified}` on the worker's receipt under this repository's land lock, at the
// moment the ref actually moved, and saves it into
// `<state>/work-receipts/sha256([entity,thread])/<id>.json`
// (`richos/engine/mega-lander/app.py:809-836`, `:81`). That file is already partitioned by
// company and thread, so a land can never be read across a company boundary — which is
// exactly why it, and not the engine's own `.lands` history beside the repository lock, is
// the source here. That history is keyed by REPOSITORY and holds every conversation's
// lands; reading it would mean the app deciding which rows were its own.
//
// **Nothing in here infers a land.** `verified` is written by the engine after a
// fast-forward it performed; absence of `verified` is absence of a land, never a failure;
// and the reviewer's verdict is read off the REVIEWER's own receipt rather than taken from
// the fact that a land happened, because "the engine would not have landed it without a
// passing review" is a claim about the engine and this is a report about evidence.

/// One land, as the receipts recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Land {
    /// The repository the engine merged into, exactly as the receipt spells it (a path).
    pub repository: String,
    /// The integration branch the ref moved on.
    pub branch: String,
    /// **An independent reviewer's own receipt says `passed` on this exact worker.** Read
    /// from the reviewer named by `integration.reviewer_id`, never assumed from the land.
    pub reviewed: bool,
}

impl Land {
    /// The repository in his terms: its own folder name, not the path to it. He asked for
    /// work in a repository he knows by name, and a sentence read aloud that spells out an
    /// absolute path is a sentence he cannot use.
    pub fn repository_name(&self) -> &str {
        self.repository.trim_end_matches('/').rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(&self.repository)
    }
}

/// **What the receipts say happened to ONE assignment's work.** Five counts, each of which
/// turns into one clause of what he is told, and none of which is a guess.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkTrail {
    /// Every land recorded for this assignment, in receipt-id order.
    pub lands: Vec<Land>,
    /// Reviewer receipts on this assignment whose observed verdict asked for changes. A
    /// reason a land did not happen, and a real one: the reviewer refused it.
    pub changes_requested: usize,
    /// Reviewer receipts whose observed verdict PASSED. Counted from the reviewer's own
    /// receipt, exactly as `changes_requested` is, and never from a land having happened.
    ///
    /// **It is here because "the review passed and the work has not landed" is a state the
    /// host has to be able to name.** Without it the only readable facts were "a land
    /// happened" and "a reviewer refused", so the one case in between — the green run of
    /// 2026-09-18, where a passing review sat on the worker's branch and nothing landed —
    /// was indistinguishable from a worker that simply had not been reviewed yet, and the
    /// back end was told to prepare a reviewer it had already had.
    pub reviews_passed: usize,
    /// Worker receipts whose run has ended and which carry no verified integration. The
    /// honest count of work that was done and not landed.
    pub not_landed: usize,
    /// Worker receipts at all. **Zero is a fact and not a gap**: no worker ever ran for this
    /// assignment, which is a different failure from one whose work was refused.
    pub workers: usize,
    /// A land the engine performed whose workspace cleanup it could not finish. The commit
    /// is integrated; a worktree or a branch was left behind. Reported, never softened, and
    /// never allowed to read as a failed land.
    pub cleanup_pending: bool,
    /// Consult receipts (plan §3, slice 3): a teammate asked on a job that changes no
    /// repository. **Never a worker**: it has nothing to land, so it is not counted in
    /// `workers` or `not_landed`, and an assignment answered with its help still closes on the
    /// back end's own report.
    pub consults: usize,
    /// **The helper whose run ended most recently was a consult**, by the end time its own
    /// receipt carries (`end_observation.at`). What the back end is told next depends on it: a
    /// consult's answer is to be read, not reviewed.
    pub last_ended_was_consult: bool,
    /// Every receipt of this assignment that names the platform's agent id of its run:
    /// `(agent id, receipt id, role)`. The weekly-switch handoff names a handed-off helper's
    /// receipt from it (`work_host`'s handoff continuation).
    pub helpers: Vec<(String, String, String)>,
    /// Unresolved native launches, paired with their owning session. No inferred agent names.
    pub stop_targets: Vec<(String, String)>,
}

/// Read one assignment's trail. `obligation` is the assignment's obligation id, which the
/// engine stamps on every receipt it writes for it (`request.obligation_id`).
///
/// **An unreadable record is an error and never an empty trail.** A caller that treated
/// "I could not read it" as "nothing happened" would tell him nothing landed on the day the
/// state directory was unreadable, which is the one answer this module exists to refuse. The
/// one exception is a partition that does not exist: no receipts were ever written for this
/// thread, which is the honest empty.
pub fn trail(state: &Path, entity: &str, thread: &str, obligation: &str) -> Result<WorkTrail, String> {
    let identity = serde_json::to_vec(&[entity, thread]).map_err(|e| e.to_string())?;
    let parent = state.join("work-receipts");
    let root = parent.join(format!("{:x}", Sha256::digest(identity)));
    if parent.is_symlink() || root.is_symlink() {
        return Err("Work records have been redirected.".into());
    }
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(WorkTrail::default()),
        Err(_) => return Err("Saved work could not be read.".into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|_| "Saved work could not be listed.")?.path();
        if path.extension().is_some_and(|e| e == "json") {
            paths.push(path);
        }
        if paths.len() > 10000 {
            return Err("There are too many saved work records for this assignment.".into());
        }
    }
    paths.sort();
    // Two passes, because a worker's land names its reviewer by id and the reviewer's
    // receipt may sort either side of it. Rows are held once, bounded the same way `read`
    // bounds itself.
    //
    // The 16 MiB budget counts only the records HELD for this assignment. Every file is still
    // read one at a time and dropped unless it belongs to the target, so memory stays bounded
    // while other assignments' intact receipts, however many, no longer use this one's
    // budget. A record that cannot be read, is oversized, or is damaged still refuses the
    // answer: it may be this assignment's, and guessing "nothing landed" is the one answer
    // this module refuses.
    let mut rows: Vec<Value> = Vec::new();
    let mut bytes_held = 0u64;
    for path in &paths {
        let meta = std::fs::symlink_metadata(path).map_err(|_| "A saved work record could not be read.")?;
        if !meta.is_file() || meta.len() > 1024 * 1024 {
            return Err("A saved work record is too large or redirected.".into());
        }
        let bytes = std::fs::read(path).map_err(|_| "A saved work record could not be read.")?;
        let row: Value = serde_json::from_slice(&bytes).map_err(|_| "A saved work record is damaged.")?;
        if row["schema"] != 1 || row["binding"]["entity_id"] != entity || row["binding"]["thread_id"] != thread {
            return Err("A saved work record has an unsupported schema or different scope.".into());
        }
        if row["request"]["obligation_id"].as_str() != Some(obligation) {
            continue;
        }
        bytes_held += meta.len();
        if bytes_held > 16 * 1024 * 1024 {
            return Err("A saved work record is too large or redirected.".into());
        }
        rows.push(row);
        if rows.len() > 200 {
            return Err("There are too many saved work records for this assignment.".into());
        }
    }
    // The reviewers first: receipt id to the verdict its own observation carries.
    let mut verdicts: Vec<(String, String)> = Vec::new();
    for row in &rows {
        if row["request"]["role"].as_str() != Some("reviewer") || row["review_observation"]["valid"] != true {
            continue;
        }
        let (Some(id), Some(verdict)) =
            (row["id"].as_str(), row["review_observation"]["report"]["verdict"].as_str())
        else {
            continue;
        };
        verdicts.push((id.to_string(), verdict.to_string()));
    }
    let mut trail = WorkTrail {
        changes_requested: verdicts.iter().filter(|(_, v)| v == "changes-requested").count(),
        reviews_passed: verdicts.iter().filter(|(_, v)| v == "passed").count(),
        helpers: rows.iter().filter_map(|row| Some((row["agent_id"].as_str()?.to_string(),
            row["id"].as_str()?.to_string(), row["request"]["role"].as_str()?.to_string()))).collect(),
        stop_targets: rows.iter().filter(|row| matches!(row["status"].as_str(), Some("dispatching" | "running" | "unknown")))
            .filter_map(|row| Some((row["binding"]["session_id"].as_str()?.to_string(), row["tool_use_id"].as_str()?.to_string())))
            .collect(),
        ..Default::default()
    };
    let mut last_end = f64::NEG_INFINITY;
    for row in &rows {
        if row["status"].as_str() == Some("run-ended") {
            if let Some(at) = row["end_observation"]["at"].as_f64().filter(|at| *at > last_end) {
                last_end = at;
                trail.last_ended_was_consult = row["request"]["role"].as_str() == Some("consult");
            }
        }
    }
    for row in &rows {
        if row["request"]["role"].as_str() == Some("reviewer") {
            continue;
        }
        if row["request"]["role"].as_str() == Some("consult") {
            trail.consults += 1;
            continue;
        }
        trail.workers += 1;
        let integration = &row["integration"];
        if integration["verified"] == true {
            let reviewer = integration["reviewer_id"].as_str().unwrap_or("");
            trail.lands.push(Land {
                repository: row["request"]["repo"].as_str().unwrap_or("").to_string(),
                branch: integration["branch"].as_str().unwrap_or("").to_string(),
                reviewed: verdicts.iter().any(|(id, v)| id == reviewer && v == "passed"),
            });
            if integration["cleanup_pending"].as_array().is_some_and(|a| !a.is_empty()) {
                trail.cleanup_pending = true;
            }
        } else if row["status"].as_str() == Some("run-ended") {
            trail.not_landed += 1;
        }
    }
    Ok(trail)
}

/// **What one conversation's receipts say each assignment has launched**, read in one pass so
/// the update gate's five-second poll reads the partition once, not once per assignment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Launches {
    /// A helper was handed to the provider: the receipt carries its tool call or agent id.
    pub dispatched: bool,
    /// The provider sessions of receipts that may still have a run in flight (`dispatching`,
    /// `running`, `unknown`: the same statuses as [`WorkTrail::stop_targets`]). A receipt with
    /// no session is recorded as `""`, which no worker reading can clear.
    pub unresolved: std::collections::BTreeSet<String>,
}

/// [`Launches`] by obligation id. **An unreadable record is an error, never an empty map**, for
/// the reason [`trail`] gives; a partition never written is the honest empty.
pub fn launches(state: &Path, entity: &str, thread: &str) -> Result<std::collections::HashMap<String, Launches>, String> {
    let identity = serde_json::to_vec(&[entity, thread]).map_err(|e| e.to_string())?;
    let parent = state.join("work-receipts");
    let root = parent.join(format!("{:x}", Sha256::digest(identity)));
    if parent.is_symlink() || root.is_symlink() {
        return Err("Work records have been redirected.".into());
    }
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(_) => return Err("Saved work could not be read.".into()),
    };
    let mut out: std::collections::HashMap<String, Launches> = Default::default();
    for (n, entry) in entries.enumerate() {
        if n > 10000 {
            return Err("There are too many saved work records.".into());
        }
        let path = entry.map_err(|_| "Saved work could not be listed.")?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let meta = std::fs::symlink_metadata(&path).map_err(|_| "A saved work record could not be read.")?;
        if !meta.is_file() || meta.len() > 1024 * 1024 {
            return Err("A saved work record is too large or redirected.".into());
        }
        let bytes = std::fs::read(&path).map_err(|_| "A saved work record could not be read.")?;
        let row: Value = serde_json::from_slice(&bytes).map_err(|_| "A saved work record is damaged.")?;
        if row["schema"] != 1 || row["binding"]["entity_id"] != entity || row["binding"]["thread_id"] != thread {
            return Err("A saved work record has an unsupported schema or different scope.".into());
        }
        // A receipt naming no obligation belongs to no assignment, as in [`trail`].
        let Some(obligation) = row["request"]["obligation_id"].as_str() else { continue };
        let launched = out.entry(obligation.to_string()).or_default();
        let named = |key: &str| row[key].as_str().is_some_and(|s| !s.is_empty());
        launched.dispatched |= named("tool_use_id") || named("agent_id");
        if matches!(row["status"].as_str(), Some("dispatching" | "running" | "unknown")) {
            launched.unresolved.insert(row["binding"]["session_id"].as_str().unwrap_or("").to_string());
        }
    }
    Ok(out)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn scoped_saved_evidence_never_infers_liveness_or_completion() {
        let root = std::env::temp_dir().join(format!("work-summary-{}", uuid::Uuid::new_v4()));
        let path = root.join("work-receipts").join(format!("{:x}", Sha256::digest(b"[\"depot\",\"thread\"]")));
        std::fs::create_dir_all(&path).unwrap();
        let mut row = serde_json::json!({"schema":1,"binding":{"entity_id":"depot","thread_id":"thread"},"request":{"title":"Fictional work","repo":"/fictional/project","role":"worker"},"status":"running"});
        let save = |row: &Value| std::fs::write(path.join("receipt.json"), serde_json::to_vec(row).unwrap()).unwrap();
        save(&row);
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state,"observed-start");
        assert!(read(&root,"other","thread").unwrap().items.is_empty());
        row["status"] = "run-ended".into(); save(&row);
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state,"run-ended");
        row["integration"] = serde_json::json!({"verified":true,"cleanup_pending":[]}); save(&row);
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state,"integrated");
        // A record bound to another scope, or damaged, is one unreadable row; nothing from
        // the file itself (its title, its repository) is shown.
        row["binding"]["entity_id"] = "other".into(); save(&row);
        let foreign = read(&root,"depot","thread").unwrap();
        assert_eq!((foreign.items.len(), foreign.items[0].state.as_str()), (1, "unreadable"));
        assert!(foreign.items[0].title != "Fictional work" && foreign.items[0].repository != "/fictional/project");
        std::fs::write(path.join("receipt.json"),"partial").unwrap();
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state, "unreadable");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test] fn retired_attempts_never_ask_to_resume_finished_work() {
        let root = temp();
        let mut old = receipt("old", "job", "worker");
        old["status"] = "interrupted".into();
        old["workspace_ref"] = "session--old".into();
        old["agent_id"] = "agent-old".into();
        let receipts = write(&root, &[old.clone()]);
        let workspace = root.join("workspaces").join(receipts.file_name().unwrap());
        std::fs::create_dir_all(workspace.join("done")).unwrap();
        let done = workspace.join("done/session--old.json");
        let canonical = serde_json::json!({"key":"session--old", "agent_id":"agent-old", "end":null,
            "disposition":{"kind":"landed"}});
        std::fs::write(&done, serde_json::to_vec(&canonical).unwrap()).unwrap();
        let summary = read(&root, "depot", "thread").unwrap();
        assert_eq!(summary.items[0].state, "retired");
        assert!(!summary.items[0].detail.contains("carry on"));
        // Active cleanup intent is not retirement. Nor is a native completed result.
        std::fs::create_dir_all(workspace.join("agents")).unwrap();
        std::fs::write(workspace.join("agents/session--old.json"), "{}").unwrap();
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state, "interrupted");
        std::fs::remove_file(workspace.join("agents/session--old.json")).unwrap();
        let mut ended = canonical.clone(); ended["end"] = serde_json::json!({"signal":"SubagentStop"});
        std::fs::write(&done, serde_json::to_vec(&ended).unwrap()).unwrap();
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state, "interrupted");
        std::fs::write(&done, "damaged").unwrap();
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state, "unreadable");
        std::fs::remove_file(&done).unwrap();
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state, "interrupted");
        std::fs::write(&done, serde_json::to_vec(&canonical).unwrap()).unwrap();
        old["integration"] = serde_json::json!({"verified":true,"cleanup_pending":[]});
        write(&root, &[old]);
        assert_eq!(read(&root,"depot","thread").unwrap().items[0].state, "integrated");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **Follow-up 7 after nightly 65** (second review of `0e974e68b`, P3): the "This attempt
    /// ended without a result" row read the same whether or not another attempt replaced it or
    /// the work was done. In Ray's run U the first guide attempt was retired, a fresh retry
    /// (no `continue_of`) was written, corrected and landed, and the retired row still pointed
    /// at "any later attempts". It now says what the other attempts at the same job did; an
    /// attempt at ANOTHER job, landed or not, says nothing about this one.
    #[test] fn a_retired_attempt_says_what_the_other_attempts_at_its_job_did() {
        let root = temp();
        let mut old = receipt("old", "job", "worker");
        old["status"] = "interrupted".into();
        old["workspace_ref"] = "session--old".into();
        old["agent_id"] = "agent-old".into();
        let receipts = write(&root, &[old]);
        let workspace = root.join("workspaces").join(receipts.file_name().unwrap());
        std::fs::create_dir_all(workspace.join("done")).unwrap();
        std::fs::write(workspace.join("done/session--old.json"), serde_json::to_vec(&serde_json::json!(
            {"key":"session--old","agent_id":"agent-old","end":null,"disposition":{"kind":"landed"}})).unwrap()).unwrap();
        let old_row = |root: &std::path::Path| read(root, "depot", "thread").unwrap().items.into_iter()
            .find(|i| i.state.starts_with("retired")).map(|i| (i.state, i.detail)).expect("the retired attempt is not shown");
        assert_eq!(old_row(&root), ("retired".into(), "This attempt ended without a result.".into()));
        // Another job's land is not this job's.
        let mut elsewhere = receipt("elsewhere", "another-job", "worker");
        elsewhere["integration"] = serde_json::json!({"verified":true,"cleanup_pending":[]});
        write(&root, &[elsewhere]);
        assert_eq!(old_row(&root).0, "retired", "another job's land was read as this one's");
        // A fresh attempt at the same job, still finishing.
        let mut retry = receipt("retry", "job", "worker");
        write(&root, &[retry.clone()]);
        assert_eq!(old_row(&root).0, "retired-other-shown");
        // That attempt landed: the retired row says the work was done.
        retry["status"] = "integrated".into();
        retry["integration"] = serde_json::json!({"verified":true,"cleanup_pending":[]});
        write(&root, &[retry]);
        let (state, detail) = old_row(&root);
        assert_eq!(state, "retired-other-added");
        assert!(!detail.contains("later attempts are shown"), "{detail}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **Review `rv-20261018T200142Z-c700e59d-1f6e`, P3**: one assignment may have several
    /// independent workers (`engine/mega-lander/app.py`, `prepare`: "An assignment may have
    /// several independent workers"), and nothing in the receipts says one replaced another. So
    /// when "Write guide" is retired and the independent "Update invoice" lands, the guide's row
    /// must not say another attempt did its work: it says what is known, at the job's level.
    #[test] fn an_independent_workers_land_is_not_called_this_attempts_replacement() {
        let root = temp();
        let mut guide = receipt("guide", "job", "worker");
        guide["request"]["title"] = "Write guide".into();
        guide["status"] = "interrupted".into();
        guide["workspace_ref"] = "session--guide".into();
        let mut invoice = receipt("invoice", "job", "worker");
        invoice["request"]["title"] = "Update invoice".into();
        let receipts = write(&root, &[guide, invoice.clone()]);
        let workspace = root.join("workspaces").join(receipts.file_name().unwrap());
        std::fs::create_dir_all(workspace.join("done")).unwrap();
        std::fs::write(workspace.join("done/session--guide.json"), serde_json::to_vec(&serde_json::json!(
            {"key":"session--guide","end":null,"disposition":{"kind":"landed"}})).unwrap()).unwrap();
        let guide_row = |root: &std::path::Path| read(root, "depot", "thread").unwrap().items.into_iter()
            .find(|i| i.title == "Write guide").map(|i| (i.state, i.detail)).expect("the guide is not shown");
        assert_eq!(guide_row(&root), ("retired-other-shown".into(),
            "This attempt ended without a result. Other work on the same job is shown separately.".into()));
        invoice["status"] = "integrated".into();
        invoice["integration"] = serde_json::json!({"verified":true,"cleanup_pending":[]});
        write(&root, &[invoice]);
        assert_eq!(guide_row(&root), ("retired-other-added".into(),
            "This attempt ended without a result. Other work on the same job was added to your project.".into()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test] fn a_verified_land_does_not_speculate_about_the_whole_job() {
        let root = temp();
        let mut worker = receipt("replacement", "job", "worker");
        worker["integration"] = serde_json::json!({"verified":true,"cleanup_pending":[]});
        write(&root, &[worker]);
        let summary = read(&root,"depot","thread").unwrap();
        assert_eq!(summary.items[0].state, "integrated");
        assert_eq!(summary.items[0].detail, "This part was checked and added to your project.");
        std::fs::remove_dir_all(root).unwrap();
    }

    // ---- CEO ruling §52: the land record is read, never inferred ------------------------

    fn receipt(id: &str, obligation: &str, role: &str) -> Value {
        serde_json::json!({"schema":1,"id":id,
            "binding":{"entity_id":"depot","thread_id":"thread"},
            "request":{"title":"Fictional work","repo":"/fictional/project","role":role,"obligation_id":obligation},
            "status":"run-ended"})
    }
    fn write(root: &std::path::Path, rows: &[Value]) -> std::path::PathBuf {
        let path = root.join("work-receipts").join(format!("{:x}", Sha256::digest(b"[\"depot\",\"thread\"]")));
        std::fs::create_dir_all(&path).unwrap();
        for row in rows {
            let name = format!("{}.json", row["id"].as_str().unwrap());
            std::fs::write(path.join(name), serde_json::to_vec(row).unwrap()).unwrap();
        }
        path
    }
    fn temp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("work-trail-{}", uuid::Uuid::new_v4()))
    }

    /// **A consult is read as an answer and never as a worker** (plan §3, slice 3). Its receipt
    /// carries no repository, so the saved-work view used to refuse the whole thread as
    /// "incomplete"; and counted as a worker it would make an assignment answered with a
    /// teammate's help look like code work that never landed. Which helper ended last is read
    /// off the end times the receipts carry, in both orders.
    #[test] fn a_consult_is_an_answer_with_no_repository_and_never_a_worker() {
        let root = temp();
        let mut consult = receipt("ccc", "obligation-7", "consult");
        consult["request"]["repo"] = Value::Null;
        consult["consult_answer"] = serde_json::json!({"message":"The older fixture is the CSV one.","answered":true});
        consult["end_observation"] = serde_json::json!({"at": 200.5, "signal": "SubagentStop"});
        let mut worker = receipt("www", "obligation-7", "worker");
        worker["end_observation"] = serde_json::json!({"at": 100.0, "signal": "SubagentStop"});
        write(&root, &[consult.clone(), worker.clone()]);
        let items = read(&root, "depot", "thread").unwrap().items;
        let item = items.iter().find(|i| i.role == "consult").expect("the consult is not shown");
        assert_eq!((item.state.as_str(), item.repository.as_str()), ("answered", "no repository"));
        let both = trail(&root, "depot", "thread", "obligation-7").unwrap();
        assert_eq!((both.workers, both.consults, both.not_landed), (1, 1, 1));
        assert!(both.last_ended_was_consult);
        // The worker ending after the consult is the newest end.
        worker["end_observation"]["at"] = serde_json::json!(300.0);
        write(&root, &[worker]);
        assert!(!trail(&root, "depot", "thread", "obligation-7").unwrap().last_ended_was_consult);
        std::fs::remove_dir_all(&root).unwrap();
        // A consult alone is no worker at all.
        let alone = temp();
        write(&alone, &[consult]);
        let only = trail(&alone, "depot", "thread", "obligation-7").unwrap();
        assert_eq!((only.workers, only.consults, only.last_ended_was_consult), (0, 1, true));
        std::fs::remove_dir_all(alone).unwrap();
    }

    /// **The land, the branch and the verdict, all read off evidence** — and the verdict is
    /// read off the REVIEWER's receipt, not deduced from the land having happened.
    ///
    /// The second assignment in the same partition is the positive control for the scoping:
    /// without it, "this trail names one land" would pass equally over a reader that ignored
    /// `obligation_id` entirely.
    #[test] fn a_land_is_read_with_its_branch_and_its_reviewers_own_verdict() {
        let root = temp();
        let mut worker = receipt("aaa", "obligation-7", "worker");
        worker["integration"] = serde_json::json!({"verified":true,"reviewer_id":"bbb",
            "branch":"cc/echo-1","commit":"deadbeef","cleanup_pending":[]});
        let mut reviewer = receipt("bbb", "obligation-7", "reviewer");
        reviewer["review_observation"] = serde_json::json!({"valid":true,"report":{"verdict":"passed"}});
        // ANOTHER assignment's land, in the same partition. It must not appear.
        let mut other = receipt("ccc", "obligation-9", "worker");
        other["integration"] = serde_json::json!({"verified":true,"reviewer_id":"zzz",
            "branch":"cc/somebody-else","commit":"cafe","cleanup_pending":[]});
        write(&root, &[worker, reviewer, other]);

        let trail = trail(&root, "depot", "thread", "obligation-7").unwrap();
        assert_eq!(trail.lands.len(), 1, "the trail crossed into another assignment's land");
        assert_eq!(trail.lands[0].branch, "cc/echo-1");
        assert_eq!(trail.lands[0].repository, "/fictional/project");
        assert_eq!(trail.lands[0].repository_name(), "project", "he would have been read an absolute path");
        assert!(trail.lands[0].reviewed, "the reviewer's own passing verdict was not found");
        assert_eq!(trail.not_landed, 0);
        assert_eq!(trail.changes_requested, 0);
        assert_eq!(trail.workers, 1);
        assert!(!trail.cleanup_pending);
        // The other assignment reads its own land and nobody else's.
        assert_eq!(super::trail(&root, "depot", "thread", "obligation-9").unwrap().lands[0].branch, "cc/somebody-else");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **`reviewed` is a reading and not a courtesy.** The same land with its reviewer's
    /// receipt saying `changes-requested` reports `reviewed: false` — so a sentence built on
    /// this field can never tell him a review passed because a merge happened.
    #[test] fn a_land_whose_reviewer_refused_it_is_not_reported_as_reviewed() {
        let root = temp();
        let mut worker = receipt("aaa", "obligation-7", "worker");
        worker["integration"] = serde_json::json!({"verified":true,"reviewer_id":"bbb",
            "branch":"cc/echo-1","commit":"deadbeef","cleanup_pending":[]});
        let mut reviewer = receipt("bbb", "obligation-7", "reviewer");
        reviewer["review_observation"] = serde_json::json!({"valid":true,"report":{"verdict":"changes-requested"}});
        write(&root, &[worker, reviewer]);
        let trail = trail(&root, "depot", "thread", "obligation-7").unwrap();
        assert_eq!(trail.lands.len(), 1);
        assert!(!trail.lands[0].reviewed, "a land was called reviewed on the strength of being a land");
        assert_eq!(trail.changes_requested, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **The three shapes of "it did not land", kept apart**, because they are three
    /// different things to tell him: the reviewer refused it, the work ran and never landed,
    /// and no worker ever ran at all.
    #[test] fn the_reasons_a_land_did_not_happen_are_counted_separately() {
        let root = temp();
        let worker = receipt("aaa", "obligation-7", "worker");
        let mut reviewer = receipt("bbb", "obligation-7", "reviewer");
        reviewer["review_observation"] = serde_json::json!({"valid":true,"report":{"verdict":"changes-requested"}});
        write(&root, &[worker, reviewer]);
        let refused = trail(&root, "depot", "thread", "obligation-7").unwrap();
        assert_eq!((refused.lands.len(), refused.not_landed, refused.changes_requested, refused.workers), (0, 1, 1, 1));
        // Nothing for this assignment at all: not a land, not a refusal, no worker.
        let nothing = trail(&root, "depot", "thread", "obligation-never").unwrap();
        assert_eq!(nothing, WorkTrail::default(), "an assignment with no receipts read as something");
        // And a partition that was never written is the honest empty rather than an error.
        assert_eq!(trail(&temp(), "depot", "thread", "obligation-7").unwrap(), WorkTrail::default());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **A land whose cleanup did not finish is still a land**, and the leftover is reported
    /// beside it rather than instead of it. Getting this backwards would tell him nothing
    /// landed while his branch had already moved.
    #[test] fn a_land_with_unfinished_cleanup_is_a_land_and_says_so() {
        let root = temp();
        let mut worker = receipt("aaa", "obligation-7", "worker");
        worker["integration"] = serde_json::json!({"verified":true,"reviewer_id":"bbb",
            "branch":"cc/echo-1","commit":"deadbeef","cleanup_pending":["one worktree remains"]});
        let mut reviewer = receipt("bbb", "obligation-7", "reviewer");
        reviewer["review_observation"] = serde_json::json!({"valid":true,"report":{"verdict":"passed"}});
        write(&root, &[worker, reviewer]);
        let trail = trail(&root, "depot", "thread", "obligation-7").unwrap();
        assert_eq!(trail.lands.len(), 1);
        assert!(trail.cleanup_pending);
        assert_eq!(trail.not_landed, 0, "a landed commit was counted as not landed");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Other assignments' intact receipts, however many, must not spend this assignment's
    /// size budget: 20 unrelated receipts of about 900 KB each exceed 16 MiB in total while
    /// this assignment's one small receipt is still readable.
    #[test] fn unrelated_receipts_do_not_use_up_one_assignments_size_budget() {
        let root = temp();
        let mut worker = receipt("aaa", "obligation-7", "worker");
        worker["integration"] = serde_json::json!({"verified":true,"reviewer_id":"bbb",
            "branch":"cc/echo-1","commit":"deadbeef","cleanup_pending":[]});
        let path = write(&root, &[worker]);
        for n in 0..20 {
            let mut other = receipt(&format!("big{n:02}"), "obligation-9", "worker");
            other["padding"] = "x".repeat(900 * 1024).into();
            std::fs::write(path.join(format!("big{n:02}.json")), serde_json::to_vec(&other).unwrap()).unwrap();
        }
        let trail = trail(&root, "depot", "thread", "obligation-7").unwrap();
        assert_eq!(trail.lands.len(), 1, "unrelated receipts made the assignment unreadable");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **Hunt 57, finding 11 (2026-10-15): the Under the hood panel speaks to a business
    /// owner.** Every sentence the saved-work view can put on screen, for every status the
    /// engine writes, a consult with and without an answer, a land with and without its tidy-up,
    /// both review verdicts, a record that cannot be read, and the assignment's "started" line,
    /// says nothing about repositories, workers, receipts, workspaces, dispatch, integration or
    /// the back end. Nightly 57 showed "It changed no repository.", "Workspace prepared. No worker
    /// dispatch has been observed." and "The back end has started on it." on a hiring job.
    #[test] fn every_saved_work_sentence_is_in_plain_words() {
        let root = temp();
        let mut rows = Vec::new();
        for (n, status) in ["preparing", "prepared", "dispatching", "running", "blocked", "unknown",
                            "interrupted", "run-ended", "integrated"].into_iter().enumerate() {
            let mut row = receipt(&format!("w{n}"), "o", "worker");
            row["status"] = status.into();
            // A `blocked` record that never dispatched is not shown at all
            // (`a_preparation_that_never_started_is_not_shown_and_never_hides_running_work`),
            // so this one carries a dispatch for its sentence to be produced and checked.
            if status == "blocked" {
                row["agent_id"] = "a".into();
            }
            rows.push(row);
        }
        for (n, verdict) in ["changes-requested", "passed"].into_iter().enumerate() {
            let mut row = receipt(&format!("r{n}"), "o", "worker");
            row["review_observation"] = serde_json::json!({"valid":true,"report":{"verdict":verdict}});
            rows.push(row);
        }
        for (n, cleanup) in [serde_json::json!([]), serde_json::json!(["one left"])].into_iter().enumerate() {
            let mut row = receipt(&format!("l{n}"), "o", "worker");
            row["integration"] = serde_json::json!({"verified":true,"cleanup_pending":cleanup});
            rows.push(row);
        }
        // Steps and reviews that later steps replaced, once landed and once still going.
        for landed in [true, false] {
            rows.extend(guide_job(if landed { "g" } else { "h" }, landed));
        }
        for (n, answered) in [true, false].into_iter().enumerate() {
            let mut row = receipt(&format!("c{n}"), "o", "consult");
            row["request"]["repo"] = Value::Null;
            row["consult_answer"] = serde_json::json!({"answered":answered});
            rows.push(row);
        }
        let path = write(&root, &rows);
        std::fs::write(path.join("damaged.json"), "partial").unwrap();
        let summary = read(&root, "depot", "thread").unwrap();
        assert_eq!(summary.items.len(), rows.len() + 1, "a sentence was not produced for every case");
        let mut said: Vec<String> = summary.items.iter().flat_map(|i| [i.title.clone(), i.detail.clone()]).collect();
        said.push(crate::work_host::STARTED_DETAIL.to_string());
        for line in &said {
            let lower = line.to_lowercase();
            for word in ["repositor", "worker", "receipt", "workspace", "dispatch", "integrat", "back end", "record"] {
                assert!(!lower.contains(word), "{line:?} names {word:?} to a business owner");
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The shape of the month-end guide job in Ray's nightly 65 run U from 62
    /// (`evidence/nightly-65-walk/u62/keepU.tgz`, receipts 75276bf9, 50a61580, f11a61bc,
    /// 5f43a5e4, 7e0885b6, f0b33645): written, approved with notes, fixed, sent back by the
    /// re-review, corrected, approved and landed. Each later worker continues the one before it
    /// and each reviewer names the worker whose commit it reviewed, as the engine writes them.
    /// With `landed` false the last step has finished but nothing has landed yet.
    fn guide_job(prefix: &str, landed: bool) -> Vec<Value> {
        let id = |n: &str| format!("{prefix}-{n}");
        let worker = |n: &str, continues: Option<&str>| {
            let mut row = receipt(&id(n), "guide", "worker");
            row["request"]["title"] = format!("step {n}").into();
            if let Some(previous) = continues {
                row["continuation"] = serde_json::json!({"worker_id": id(previous), "commit": "c0ffee"});
            }
            row
        };
        let reviewer = |n: &str, of: &str, verdict: &str| {
            let mut row = receipt(&id(n), "guide", "reviewer");
            row["request"]["title"] = format!("review {n}").into();
            row["review_target"] = serde_json::json!({"worker_id": id(of), "commit": "c0ffee"});
            row["review_observation"] = serde_json::json!({"valid": true, "report": {"verdict": verdict}});
            row
        };
        let mut last = worker("w3", Some("w2"));
        if landed {
            last["status"] = "integrated".into();
            last["integration"] = serde_json::json!({"verified": true, "reviewer_id": id("r3"), "cleanup_pending": []});
        }
        vec![worker("w1", None), reviewer("r1", "w1", "passed"), worker("w2", Some("w1")),
             reviewer("r2", "w2", "changes-requested"), last, reviewer("r3", "w3", "passed")]
    }

    /// **Item 3 of the follow-ups after nightly 65** (Ray, run U from 62): after the guide was
    /// landed and pushed, its first two steps still said "It still needs a review before it's
    /// added to your project." and the re-review that the correction replaced still said "A
    /// reviewer asked for changes." A row says what is true now.
    #[test] fn a_landed_jobs_earlier_steps_and_replaced_reviews_say_what_is_true_now() {
        let root = temp();
        write(&root, &guide_job("g", true));
        let items = read(&root, "depot", "thread").unwrap().items;
        let says = |title: &str| items.iter().find(|i| i.title == title).map(|i| (i.state.clone(), i.detail.clone())).unwrap();
        for step in ["step w1", "step w2"] {
            assert_eq!(says(step).0, "integrated-later", "{step}: {:?}", says(step).1);
            assert!(!says(step).1.contains("still needs a review"), "{step} still asks for a review after the land");
        }
        assert_eq!(says("step w3").0, "integrated");
        assert_eq!(says("review r1").0, "review-passed-later-added");
        assert_eq!(says("review r2").0, "review-changes-later-added");
        assert!(!says("review r2").1.contains("Nothing has been added"), "the replaced review reads as the current state");
        assert_eq!(says("review r3"), ("review-passed-added".into(), "A reviewer approved it and it was added to your project.".into()));
        std::fs::remove_dir_all(&root).unwrap();

        // Before the land: the earlier steps were carried on and their reviews were replaced,
        // and nothing claims a land that has not happened.
        let going = temp();
        write(&going, &guide_job("h", false));
        let items = read(&going, "depot", "thread").unwrap().items;
        let state = |title: &str| items.iter().find(|i| i.title == title).map(|i| i.state.clone()).unwrap();
        assert_eq!((state("step w1"), state("step w2"), state("step w3")), ("continued".into(), "continued".into(), "run-ended".into()));
        assert_eq!((state("review r1"), state("review r2"), state("review r3")),
            ("review-passed-continued".into(), "review-changes-continued".into(), "review-passed".into()));
        assert!(items.iter().all(|i| !i.state.starts_with("integrated") && !i.state.ends_with("-added")),
            "a land was claimed before one happened: {items:?}");
        std::fs::remove_dir_all(&going).unwrap();
    }

    /// The saved-work view shows the most recently written receipts, not the first hundred
    /// filenames: a newest receipt whose id sorts last must still be shown.
    #[test] fn the_saved_work_view_shows_the_newest_records_not_the_first_filenames() {
        let root = temp();
        let rows: Vec<Value> = (0..120).map(|n| {
            let mut r = receipt(&format!("a{n:03}"), "o", "worker");
            r["request"]["title"] = format!("old {n}").into();
            r
        }).collect();
        let path = write(&root, &rows);
        let base = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for (n, row) in rows.iter().enumerate() {
            let f = std::fs::File::options().write(true).open(path.join(format!("{}.json", row["id"].as_str().unwrap()))).unwrap();
            f.set_modified(base + std::time::Duration::from_secs(n as u64)).unwrap();
        }
        let mut newest = receipt("zzz-newest", "o", "worker");
        newest["request"]["title"] = "the newest work".into();
        write(&root, &[newest]);
        let f = std::fs::File::options().write(true).open(path.join("zzz-newest.json")).unwrap();
        f.set_modified(base + std::time::Duration::from_secs(5000)).unwrap();
        let summary = read(&root, "depot", "thread").unwrap();
        assert_eq!(summary.items.len(), 100);
        assert_eq!(summary.omitted, 21);
        assert!(summary.items.iter().any(|i| i.title == "the newest work"), "the newest record was hidden by its filename");
        assert!(!summary.items.iter().any(|i| i.title == "old 0"), "the oldest record outranked a newer one");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **A preparation the engine settled as `blocked` never hides running work** (Ray, test
    /// VM, 2026-10-13, esc-20261013T203037Z-9b49483d). Pierce refused a brief,
    /// `settle_undispatched` wrote `status: "blocked"` (`engine/mega-lander/app.py`
    /// `record.update(status="blocked", ...)`), and the one status this reader did not know
    /// turned the whole Under-the-hood list into "unsupported state" while the replacement
    /// consult was still running. Every status the engine writes is read, and a record the
    /// reader still cannot read is shown as one unreadable row next to the others.
    ///
    /// **And a job that never started is not a card at all** (Ray's check of nightly 57,
    /// 2026-10-15, `team/under-the-hood.png`: "Ended without starting. Its preparation was
    /// refused or closed before any worker was dispatched" under Under the hood, in two runs of
    /// four). No worker ran and nothing was done, so there is nothing of it to show him; the
    /// engine's own `never_dispatched` says which records those are.
    #[test] fn a_preparation_that_never_started_is_not_shown_and_never_hides_running_work() {
        let root = temp();
        let mut blocked = receipt("aaa", "obligation-7", "consult");
        blocked["request"]["repo"] = Value::Null;
        blocked["request"]["title"] = "Refused before it started".into();
        blocked["status"] = "blocked".into();
        blocked["pierce_refused"] = true.into();
        blocked["settled"] = serde_json::json!({"at":"2026-10-13T20:00:00Z","withdrawn":[],
            "reason":"never dispatched: no worker was registered or started for this receipt"});
        let mut running = receipt("bbb", "obligation-7", "consult");
        running["request"]["repo"] = Value::Null;
        running["request"]["title"] = "The replacement, still running".into();
        running["status"] = "running".into();
        let path = write(&root, &[blocked, running]);
        let summary = read(&root, "depot", "thread").expect("one blocked record hid the whole work list");
        let state = |summary: &WorkSummary, title: &str| summary.items.iter().find(|i| i.title == title).map(|i| i.state.clone());
        assert_eq!(state(&summary, "Refused before it started"), None, "a job that never started is shown as a card");
        assert_eq!(summary.items.len(), 1);
        assert_eq!(state(&summary, "The replacement, still running").as_deref(), Some("observed-start"));
        // Every status the engine writes into a receipt is a state this reader knows; a
        // `blocked` one that never dispatched is the one it leaves out.
        for status in ["preparing", "prepared", "dispatching", "running", "blocked", "unknown",
                       "interrupted", "run-ended", "integrated"] {
            let mut row = receipt("ccc", "obligation-7", "worker");
            row["status"] = status.into();
            write(&root, &[row]);
            let summary = read(&root, "depot", "thread").unwrap();
            assert_eq!(summary.items.len(), if status == "blocked" { 1 } else { 2 }, "status {status}");
            assert!(summary.items.iter().all(|i| i.state != "unreadable"), "status {status} was not read");
        }
        // A record it still cannot read is one row of its own; the others stay readable.
        std::fs::write(path.join("ccc.json"), "partial").unwrap();
        let summary = read(&root, "depot", "thread").expect("one damaged record hid the others");
        assert_eq!(summary.items.len(), 2);
        assert_eq!(summary.items.iter().filter(|i| i.state == "unreadable").count(), 1);
        assert_eq!(state(&summary, "The replacement, still running").as_deref(), Some("observed-start"));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **An unreadable record is an error, never an empty trail.** The positive control is
    /// the assertion above it: the same call over the same directory succeeds before the
    /// damage, so the error is a fact about the damaged row.
    #[test] fn a_damaged_record_refuses_to_answer_rather_than_reporting_nothing_landed() {
        let root = temp();
        let mut worker = receipt("aaa", "obligation-7", "worker");
        worker["integration"] = serde_json::json!({"verified":true,"reviewer_id":"bbb",
            "branch":"cc/echo-1","commit":"deadbeef","cleanup_pending":[]});
        let path = write(&root, &[worker]);
        assert_eq!(trail(&root, "depot", "thread", "obligation-7").unwrap().lands.len(), 1);
        std::fs::write(path.join("aaa.json"), "partial").unwrap();
        assert!(trail(&root, "depot", "thread", "obligation-7").is_err(), "a damaged record read as nothing landed");
        std::fs::remove_dir_all(root).unwrap();
    }
}
