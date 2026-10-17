//! Durable receiving inbox for answers addressed to a product work assignment.
//! The existing work runner consumes these only at its assignment boundary.
use crate::questions::Delivery;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
#[derive(Serialize, Deserialize)]
struct Input {
    delivery: Delivery,
    /// Let go: taken by a back end, acknowledged by his team, or discarded with its ended job.
    handed: bool,
    /// **Which back-end session took it** (the work-path design, richos-hq
    /// `docs/plans/2026-09-27-work-path-answer-delivery-design.md` D1). Written on the child's
    /// first item of the turn that carried it, never before the send. A fresh session has never
    /// seen it, so it is told again there (D5). `None` on a file written before this field, or
    /// on an answer his team acknowledged: an unknown, dead session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    taken_in: Option<String>,
    /// **Let go with its assignment, which ended**, whether or not a back end ever took it. It is
    /// never told to anyone again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    discarded: bool,
}
fn directory(root: &Path) -> PathBuf {
    root.join("questions/work-inputs")
}
fn path(root: &Path, id: &str) -> PathBuf {
    directory(root).join(format!("{:x}.json", Sha256::digest(id.as_bytes())))
}
fn write(path: &Path, value: &Input) -> Result<(), String> {
    use std::io::Write;
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let temp = dir.join(format!("{}.new", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut f, value).map_err(|e| e.to_string())?;
        f.write_all(b"\n").map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
        std::fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        drop(std::fs::remove_file(temp));
    }
    result
}
fn read(path: &Path) -> Result<Input, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
/// Called under the question store lock. An uncertain retry keeps the same input.
pub fn enqueue(root: &Path, delivery: &Delivery) -> Result<bool, String> {
    let path = path(root, &delivery.id);
    if path.exists() {
        let held = read(&path)?;
        if held.delivery.text != delivery.text {
            return Err("Question input identity changed".into());
        }
        return Ok(!held.handed);
    }
    write(
        &path,
        &Input {
            delivery: delivery.clone(),
            handed: false,
            taken_in: None,
            discarded: false,
        },
    )?;
    Ok(true)
}
fn all(root: &Path) -> Result<Vec<Input>, String> {
    let mut values = Vec::new();
    let files = match std::fs::read_dir(directory(root)) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(values),
        Err(e) => return Err(e.to_string()),
    };
    for file in files {
        let path = file.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|s| s == "json") {
            values.push(read(&path)?);
        }
    }
    values.sort_by(|a, b| a.delivery.id.cmp(&b.delivery.id));
    Ok(values)
}
pub fn pending(root: &Path) -> Result<Vec<Delivery>, String> {
    Ok(all(root)?.into_iter().filter(|input| !input.handed).map(|input| input.delivery).collect())
}

/// **What one run of an assignment carries to its back end** (design D1, D5).
#[derive(Debug, Default)]
pub struct Carried {
    /// His answers no back end has taken yet. They stay pending until [`taken`].
    pub new: Vec<Delivery>,
    /// Answers a back end took in a session that is not `session`: that session is gone
    /// (relaunch, renewal, a retired lease), and the one running now has never seen them.
    pub earlier: Vec<Delivery>,
}
impl Carried {
    pub fn is_empty(&self) -> bool {
        self.new.is_empty() && self.earlier.is_empty()
    }
    pub fn ids(&self) -> Vec<String> {
        self.new.iter().chain(&self.earlier).map(|d| d.id.clone()).collect()
    }
}

/// **Read, and change nothing** (design D1). The answers for this asker that `session` has not
/// taken. Nothing is let go here: an answer is let go only when the back end's first item of
/// the turn carrying it arrives ([`taken`]), so a crash or a failed send before that leaves it
/// pending. `session` is the lease the prompt goes to.
pub fn peek(root: &Path, entity: &str, thread: &str, asker: &str, session: &str) -> Result<Carried, String> {
    let mut carried = Carried::default();
    for input in all(root)? {
        let d = &input.delivery;
        if d.entity_id != entity || d.thread_id != thread || d.asker != asker || input.discarded {
            continue;
        }
        if !input.handed {
            carried.new.push(input.delivery);
        } else if input.taken_in.as_deref() != Some(session) {
            carried.earlier.push(input.delivery);
        }
    }
    Ok(carried)
}

/// **The back end has it** (design D1): its first item of the turn that carried these answers
/// arrived. Synced, like every write here. An answer already let go with its ended job stays
/// discarded.
pub fn taken(root: &Path, ids: &[String], session: &str) -> Result<(), String> {
    let mut sets = Vec::new();
    for id in ids {
        let path = path(root, id);
        let mut input = read(&path)?;
        if input.discarded || (input.handed && input.taken_in.as_deref() == Some(session)) {
            continue;
        }
        input.handed = true;
        input.taken_in = Some(session.to_string());
        write(&path, &input)?;
        sets.extend(input.delivery.set_id.clone());
    }
    // His card may now say Rich has it (D7). Never called under the store lock or `inner`.
    crate::questions::Store::new(root).taker_has(&sets)
}

/// **Let go of this asker's answers because its assignment ended** — the cleanup, and his
/// Stop. It is not "taken", and nothing is told to a back end after it. An answer no back end
/// had taken (`handed == false`) marks its card: the job stopped before Rich got it.
pub fn discard(root: &Path, entity: &str, thread: &str, asker: &str) -> Result<(), String> {
    let mut untaken = Vec::new();
    for mut input in all(root)? {
        let d = &input.delivery;
        if d.entity_id != entity || d.thread_id != thread || (d.asker != asker && d.obligation_id.as_deref() != Some(asker)) || input.discarded {
            continue;
        }
        let path = path(root, &d.id);
        if !input.handed {
            untaken.extend(d.set_id.clone());
        }
        input.handed = true;
        input.discarded = true;
        write(&path, &input)?;
    }
    // Same lock order as `taken` -> `taker_has`: the store lock is taken here, after every
    // inbox write, and never under the store lock or a work back end's `inner`.
    crate::questions::Store::new(root).ended_before_taken(&untaken)
}

/// Is this answer sitting in the inbox, not yet taken? The question store asks, inside the
/// transaction that writes its receipt, so "Rich has your answer" waits for the taker (D7).
pub fn waiting(root: &Path, id: &str) -> bool {
    let path = path(root, id);
    path.exists() && read(&path).is_ok_and(|input| !input.handed)
}

/// Is one of his answers for this asker saved and not yet taken? Recovery reads this (D6).
pub fn untaken(root: &Path, entity: &str, thread: &str, asker: &str) -> Result<bool, String> {
    Ok(pending(root)?.iter().any(|d| d.entity_id == entity && d.thread_id == thread && (d.asker == asker || d.obligation_id.as_deref() == Some(asker))))
}

/// The complete MCP result is the receiving input for a words answer. Keep it
/// durably bound to that turn before marking the set handed off. Repeating the
/// MCP call returns the same payload instead of starting another conversation turn.
pub fn tool_result(root: &Path, turn: &str, delivery: &Delivery) -> Result<String, String> {
    let receipt = format!("tool-result:{turn}:{}", delivery.id);
    let path = root
        .join("questions/tool-results")
        .join(format!("{:x}.json", Sha256::digest(receipt.as_bytes())));
    if path.exists() {
        let held = read(&path)?;
        if held.delivery.text != delivery.text {
            return Err("Question result identity changed".into());
        }
    } else {
        write(
            &path,
            &Input {
                delivery: delivery.clone(),
                handed: true,
                taken_in: None,
                discarded: false,
            },
        )?;
    }
    Ok(receipt)
}
pub fn read_tool_result(root: &Path, turn: &str, set: &str) -> Result<Option<String>, String> {
    let receipt = format!("tool-result:{turn}:question-set:{set}");
    let path = root
        .join("questions/tool-results")
        .join(format!("{:x}.json", Sha256::digest(receipt.as_bytes())));
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read(&path)?.delivery.text))
}

/// Record the receiving host's acknowledgement for exactly this logical input.
pub fn acknowledge(root: &Path, id: &str) -> Result<(), String> {
    let path = path(root, id);
    let mut input = read(&path)?;
    if !input.handed { input.handed = true; write(&path, &input)?; }
    // His card may now say his team has it (D7). Never called under the store lock.
    crate::questions::Store::new(root).taker_has(&input.delivery.set_id.into_iter().collect::<Vec<_>>())
}

/// [`acknowledge`] for an answer that may never have entered this inbox (the operator host is
/// also called directly). `Ok(false)` when it is not here.
pub fn acknowledge_if_present(root: &Path, id: &str) -> Result<bool, String> {
    if !path(root, id).exists() {
        return Ok(false);
    }
    acknowledge(root, id).map(|_| true)
}
