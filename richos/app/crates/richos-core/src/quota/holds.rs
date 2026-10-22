//! Evidence of a wait, not an inference from the quota reading. A held file is
//! locked by its waiter for the entire pause. Process death releases the lock,
//! so abandoned records cannot turn into permanently "paused" agents in the UI.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hold {
    pub kind: String,
    pub id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub session_id: String,
    pub name: String,
    pub task: Option<String>,
    pub since_at: u64,
    pub released_at: Option<u64>,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub held: Vec<Hold>,
    pub released: Vec<Hold>,
    pub resumes_at: Option<u64>,
    /// The account in use, by its label, while there are two or more (round 16's working row,
    /// "3 agents working on Work"). Set by the desktop shell from the quota view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}

fn open(path: &Path, create: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create_new(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Invalid pause record"));
    }
    Ok(file)
}

fn lock(file: &File, exclusive: bool) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe {
            libc::flock(
                file.as_raw_fd(),
                (if exclusive {
                    libc::LOCK_EX
                } else {
                    libc::LOCK_SH
                // lock-busy: busy is the answer: a liveness probe of the pause record's holder, returned as Ok(false).
                }) | libc::LOCK_NB,
            )
        } == 0
        {
            return Ok(true);
        }
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::WouldBlock {
            return Ok(false);
        }
        Err(e)
    }
    #[cfg(not(unix))]
    {
        let _best_effort = (file, exclusive);
        Err(io::Error::other(
            "Pause observation is unavailable on this platform",
        ))
    }
}

fn decode(file: &File) -> io::Result<Hold> {
    let mut bytes = Vec::new();
    file.take(16385).read_to_end(&mut bytes)?;
    if bytes.len() > 16384 {
        return Err(io::Error::other("Oversized pause record"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn paths(state: &Path) -> io::Result<Vec<PathBuf>> {
    let root = state.join("quota-waits");
    if root.is_symlink() {
        return Err(io::Error::other("Pause records have been redirected"));
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut paths = vec![];
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|v| v == "json") {
            paths.push(path);
        }
        if paths.len() > 4096 {
            return Err(io::Error::other("Too many pause records to inspect"));
        }
    }
    Ok(paths)
}

pub struct Guard {
    file: File,
    path: PathBuf,
    record: Hold,
}
impl Guard {
    pub fn begin(state: &Path, record: Hold) -> io::Result<Self> {
        // Mutation belongs to the writer. Read-only status calls never clean up.
        for path in paths(state)? {
            if let Ok(file) = open(&path, false) {
                if lock(&file, false).unwrap_or(false)
                    && decode(&file).is_ok_and(|r| {
                        r.released_at
                            .is_none_or(|t| crate::util::now_millis().saturating_sub(t) > 60_000)
                    })
                {
                    let _best_effort = fs::remove_file(path);
                }
            }
        }
        let root = state.join("quota-waits");
        fs::create_dir_all(&root)?;
        let path = root.join(format!("{}.json", uuid::Uuid::new_v4()));
        let staging = path.with_extension("pending");
        let mut file = open(&staging, true)?;
        let result = (|| {
            if !lock(&file, true)? {
                return Err(io::Error::other("Could not observe this pause"));
            }
            file.write_all(&serde_json::to_vec(&record)?)?;
            fs::rename(&staging, &path)
        })();
        if let Err(e) = result {
            let _best_effort = fs::remove_file(staging);
            return Err(e);
        }
        Ok(Self { file, path, record })
    }
    /// Only a successful admission produces release evidence. Stop, timeout and
    /// process death must never be shown as a successful continuation.
    pub fn release(mut self) {
        self.record.released_at = Some(crate::util::now_millis());
        let _best_effort = super::atomic_write(&self.path.with_extension("released.json"), &self.record);
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _best_effort = &self.file;
        let _best_effort = fs::remove_file(&self.path);
    }
}

fn key(row: &Hold) -> (&str, &str, &str, &str, &str) {
    (
        &row.entity_id,
        &row.thread_id,
        &row.session_id,
        &row.kind,
        &row.id,
    )
}

pub fn read(state: &Path, scope: Option<(&str, &str)>) -> io::Result<Activity> {
    let mut view = Activity::default();
    let now = crate::util::now_millis();
    for path in paths(state)? {
        let file = match open(&path, false) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue, // waiter just left
            Err(e) => return Err(e),
        };
        let inactive = lock(&file, false)?;
        let row = decode(&file)?;
        if scope.is_some_and(|(entity, thread)| row.entity_id != entity || row.thread_id != thread)
        {
            continue;
        }
        if !inactive && row.released_at.is_none() {
            view.held.push(row);
        } else if row
            .released_at
            .is_some_and(|t| t <= now && now - t < 30_000)
        {
            view.released.push(row);
        }
    }
    // Several pending steps can belong to the same agent. Count the agent once,
    // retain its earliest wait and suppress release while another step waits.
    view.held.sort_by_key(|row| row.since_at);
    let mut unique = Vec::new();
    for row in view.held {
        if !unique.iter().any(|r| key(r) == key(&row)) {
            unique.push(row);
        }
    }
    view.held = unique;
    view.released
        .sort_by_key(|row| std::cmp::Reverse(row.released_at));
    let mut unique = Vec::new();
    for row in view.released {
        if !view
            .held
            .iter()
            .chain(unique.iter())
            .any(|r| key(r) == key(&row))
        {
            unique.push(row);
        }
    }
    view.released = unique;
    Ok(view)
}

pub(super) fn from_hook(state: &Path, scope: &Path, payload: &Value) -> io::Result<Hold> {
    let binding: Value = super::gate::read_json(scope)?;
    let get = |k: &str| binding["binding"][k].as_str().unwrap_or("").to_owned();
    let agent = payload["agent_id"].as_str().filter(|s| !s.is_empty());
    let mut record = Hold {
        kind: if agent.is_some() { "agent" } else { "dispatch" }.into(),
        id: agent.unwrap_or("dispatch").into(),
        entity_id: get("entity_id"),
        thread_id: get("thread_id"),
        session_id: get("session_id"),
        name: agent.unwrap_or("New agent").chars().take(120).collect(),
        task: None,
        since_at: crate::util::now_millis(),
        released_at: None,
    };
    if agent.is_none() {
        return Ok(record);
    }
    // Enrich only from this exact thread and session's saved worker receipt.
    let partition = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&[&record.entity_id, &record.thread_id])?)
    );
    let parent = state.join("work-receipts");
    let root = parent.join(partition);
    if parent.is_symlink() || root.is_symlink() {
        return Ok(record);
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.take(1000).flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|v| v != "json") {
                continue;
            }
            let Ok(file) = open(&path, false) else {
                continue;
            };
            if file.metadata()?.len() > 262144 {
                continue;
            }
            let Ok(row) = serde_json::from_reader::<_, Value>(file.take(262145)) else {
                continue;
            };
            if row["schema"] != 1
                || row["agent_id"].as_str() != agent
                || row["binding"]["entity_id"] != record.entity_id
                || row["binding"]["thread_id"] != record.thread_id
                || row["binding"]["session_id"] != record.session_id
            {
                continue;
            }
            if let Some(name) = row["name"].as_str().filter(|v| !v.is_empty()) {
                record.name = name.chars().take(120).collect();
            }
            record.task = row["request"]["title"]
                .as_str()
                .map(|v| v.chars().take(240).collect());
            break;
        }
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::tests::Scratch;
    fn row() -> Hold {
        Hold {
            kind: "agent".into(),
            id: "a".into(),
            entity_id: "e".into(),
            thread_id: "t".into(),
            session_id: "s".into(),
            name: "Fable reviewer".into(),
            task: Some("Review settings".into()),
            since_at: crate::util::now_millis(),
            released_at: None,
        }
    }
    #[test]
    fn counts_only_locked_waiters_and_deduplicates_steps() {
        let root = Scratch::new();
        let a = Guard::begin(root.path(), row()).unwrap();
        let b = Guard::begin(root.path(), row()).unwrap();
        assert_eq!(read(root.path(), Some(("e", "t"))).unwrap().held.len(), 1);
        assert!(read(root.path(), Some(("e", "other")))
            .unwrap()
            .held
            .is_empty());
        a.release();
        let view = read(root.path(), None).unwrap();
        assert_eq!(view.held.len(), 1);
        assert!(view.released.is_empty());
        b.release();
        let view = read(root.path(), None).unwrap();
        assert!(view.held.is_empty());
        assert_eq!(view.released.len(), 1);
    }
    #[test]
    fn stop_or_abandoned_unlocked_record_does_not_claim_resumption() {
        let root = Scratch::new();
        let a = Guard::begin(root.path(), row()).unwrap();
        let path = a.path.clone();
        drop(a);
        super::super::atomic_write(&path, &row()).unwrap();
        let view = read(root.path(), None).unwrap();
        assert!(view.held.is_empty() && view.released.is_empty());
    }

    #[test]
    fn killed_waiter_disappears_without_claiming_release() {
        const ENV: &str = "RICHOS_QUOTA_TEST_WAITER";
        if let Some(root) = std::env::var_os(ENV) {
            let _guard = Guard::begin(Path::new(&root), row()).unwrap();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let root = Scratch::new();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "quota::holds::tests::killed_waiter_disappears_without_claiming_release",
            ])
            .env(ENV, root.path())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        // A hang guard on a fresh copy of this whole test binary starting and taking its
        // lock, which on a busy or swapping Mac has no fixed cost; it was 5 s (audit R9's
        // class). The loop leaves the moment the lock is seen.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut observed = false;
        while std::time::Instant::now() < deadline {
            if read(root.path(), None).unwrap().held.len() == 1 {
                observed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(observed, "child published a real locked pause");
        let view = read(root.path(), None).unwrap();
        assert!(view.held.is_empty() && view.released.is_empty());
    }

    #[test]
    fn hook_names_come_only_from_the_bound_worker_receipt() {
        let root = Scratch::new();
        let scope = root.path().join("scope.json");
        let binding = serde_json::json!({"entity_id":"e", "thread_id":"t", "session_id":"s"});
        super::super::atomic_write(&scope, &serde_json::json!({"binding":binding})).unwrap();
        let partition = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&["e", "t"]).unwrap())
        );
        let receipt = root
            .path()
            .join("work-receipts")
            .join(partition)
            .join("worker.json");
        fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        let mut data = serde_json::json!({"schema":1, "binding":binding, "agent_id":"a", "name":"Fable reviewer", "request":{"title":"Review settings"}});
        super::super::atomic_write(&receipt, &data).unwrap();
        let payload = serde_json::json!({"agent_id":"a"});
        let result = from_hook(root.path(), &scope, &payload).unwrap();
        assert_eq!(result.name, "Fable reviewer");
        assert_eq!(result.task.as_deref(), Some("Review settings"));
        data["binding"]["session_id"] = "other-session".into();
        super::super::atomic_write(&receipt, &data).unwrap();
        let result = from_hook(root.path(), &scope, &payload).unwrap();
        assert_eq!(result.name, "a");
        assert!(result.task.is_none());
    }
}
