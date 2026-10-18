//! THE BEFORE/AFTER WITNESS for any change to how a ledger is read.
//!
//! Produces a canonical, deterministic, **content-free** digest of the projection
//! `Ledger::open` builds from a ledger file. Run it against the same file with two
//! different builds of this crate and diff: identical output means the two builds read
//! that customer's history into exactly the same conversation.
//!
//! ## Why a digest and not a dump
//!
//! A ledger holds the CEO's own words. This has to be safe to run against a real ledger,
//! safe to paste into a handoff, and safe to commit as a fixture in a PUBLIC repository —
//! so **no field that can hold prose is ever emitted**. Every such field becomes
//! `<sha256>:<byte length>`. A hash answers the only question being asked ("did this byte
//! change?") and carries none of the risk.
//!
//! Emitted verbatim: ids, enum states, counters, timestamps. The STRUCTURE of the
//! conversation, never its content.
//!
//! ## Why it copies the file first
//!
//! `Ledger::open` opens its path `create(true).append(true)`. It appends nothing on its
//! own, but a WRITE handle on a customer's only copy of their history is not a thing to
//! open casually. So this copies the bytes to a scratch path and opens the COPY; the
//! original is only ever read.
//!
//! Shared by `tests/ledger_forward_compat_tests.rs` and
//! `examples/ledger_projection_digest.rs` — ONE implementation, so a golden captured by
//! the example is the same artifact the test compares against.
//!
//! Self-contained on purpose (no `use super::…`): the example includes it with
//! `#[path = …] mod`, where there is no parent module to reach.

#![allow(dead_code)]

use richos_core::ledger::Ledger;
use sha2::{Digest, Sha256};
use std::path::Path;

/// The digest of one ledger file, as a list of lines. `Err` carries a reason that is safe
/// to print (an io/parse message about the FILE, never about its contents).
pub fn digest_ledger(src: &Path, copy_to: &Path) -> Result<Vec<String>, String> {
    let raw = std::fs::read(src).map_err(|e| format!("read: {e}"))?;
    if let Some(dir) = copy_to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("scratch dir: {e}"))?;
    }
    std::fs::write(copy_to, &raw).map_err(|e| format!("copy: {e}"))?;

    let mut out = Vec::new();
    out.push(format!("file.bytes = {}", raw.len()));
    out.push(format!("file.lines = {}", raw.iter().filter(|b| **b == b'\n').count()));
    out.push(format!("file.sha256 = {}", hex(&raw)));

    let ledger = Ledger::open(copy_to).map_err(|e| format!("open: {e}"))?;

    out.push(format!("threads.count = {}", ledger.threads().len()));
    for t in ledger.threads() {
        out.push(format!(
            "thread {} | title={} | created_at={} | bound={} | entity={} | revision={}",
            t.id,
            digest(&t.title),
            t.created_at,
            t.is_bound(),
            t.entity_id().map(|e| e.to_string()).unwrap_or_else(|| "-".into()),
            match ledger.thread_binding(&t.id) {
                Ok(b) => b.binding_revision().to_string(),
                Err(_) => "-".into(),
            }
        ));
    }

    out.push(format!("turns.count = {}", ledger.turns().len()));
    for t in ledger.turns() {
        out.push(format!(
            "turn {} | thread={} | entity={} | revision={} | quarantined={} | source={:?} | \
             state={:?} | session={} | user={} | assistant={} | runs={} | stop_reason={} | \
             created_at={} | started_at={} | ended_at={} | tier={:?} | superseded_by={} | \
             stop_requested_at={} | intake_id={} | active_ms={}",
            t.id,
            t.thread_id,
            t.entity_id.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "-".into()),
            t.binding_revision,
            t.quarantined,
            t.source,
            t.state,
            t.session_id.clone().unwrap_or_else(|| "-".into()),
            digest(&t.user_text),
            digest(&t.assistant_text),
            t.text_runs.len(),
            t.stop_reason.as_deref().map(digest).unwrap_or_else(|| "-".into()),
            t.created_at,
            opt(t.started_at),
            opt(t.ended_at),
            t.tier,
            t.superseded_by.clone().unwrap_or_else(|| "-".into()),
            opt(t.stop_requested_at),
            opt(t.intake_id),
            opt(t.active_ms()),
        ));
        // THE UPSTREAM FAILURE, ON ITS OWN LINE AND ONLY WHEN PRESENT.
        //
        // Appended as a separate line rather than as another field on the turn line, and
        // that is the whole reason the existing goldens still match byte for byte: a new
        // field would have rewritten every turn line in both fixtures, and a golden that
        // changes everywhere proves nothing about the one thing that changed.
        //
        // The three sentences are HASHED like every other prose field — they are what the
        // CEO was shown, so they are content. The classification and the request id are
        // emitted verbatim: they are a tag and a vendor id, neither of them his words.
        if let Some(u) = &t.upstream_failure {
            out.push(format!(
                "  upstream {} | fault={} | status={} | request_id={} | model={} | ceo={} | loss={} | retry={}",
                t.id,
                u.fault,
                opt(u.status),
                u.request_id.clone().unwrap_or_else(|| "-".into()),
                u.model.clone().unwrap_or_else(|| "-".into()),
                digest(&u.ceo_message),
                digest(&u.loss_message),
                u.retry_message.as_deref().map(digest).unwrap_or_else(|| "-".into()),
            ));
        }
        // WHY THE TURN WAS INTERRUPTED, on its own line and only when present — same shape
        // as the upstream line above and appended for the same reason: a new field on the
        // turn line would have rewritten every turn line in both fixtures, and a golden
        // that changes everywhere proves nothing about the one thing that changed.
        //
        // The sentences are HASHED (they are what the CEO was shown, so they are content);
        // the cause tag and the retry boolean are emitted verbatim, because a change to
        // either changes what he can DO and must be legible in a diff rather than hidden
        // behind a hash.
        if let Some(c) = &t.interruption {
            out.push(format!(
                "  interruption {} | cause={} | ceo={} | loss={} | offers_retry={}",
                t.id,
                c.cause,
                digest(&c.ceo_message),
                c.loss_message.as_deref().map(digest).unwrap_or_else(|| "-".into()),
                c.offers_retry,
            ));
        }
        for (i, r) in t.text_runs.iter().enumerate() {
            out.push(format!(
                "  run {}.{} | start_seq={} | end_seq={} | at={} | text={}",
                t.id,
                i,
                opt(r.start_seq),
                opt(r.end_seq),
                r.at,
                digest(&r.text)
            ));
        }
    }

    out.push(format!("actions.count = {}", ledger.actions().len()));
    for a in ledger.actions() {
        out.push(format!(
            "action {} | turn={} | kind={} | detail={} | status={:?} | visibility={:?} | at={}",
            a.id,
            a.turn_id.clone().unwrap_or_else(|| "-".into()),
            digest(&a.kind),
            digest(&a.detail),
            a.status,
            a.visibility,
            a.at
        ));
    }

    // Messages are the RENDERED view — what the CEO actually sees. A projection that
    // matched on turns and differed here would still be a regression he could see.
    let mut thread_ids: Vec<&str> = ledger.threads().iter().map(|t| t.id.as_str()).collect();
    thread_ids.sort_unstable();
    for id in thread_ids {
        match ledger.messages(id) {
            Ok(msgs) => {
                out.push(format!("messages {} | count={}", id, msgs.len()));
                for (i, m) in msgs.iter().enumerate() {
                    out.push(format!(
                        "  message {}.{} | role={} | turn={} | at={} | text={}",
                        id,
                        i,
                        m.role,
                        m.turn_id,
                        m.at,
                        digest(&m.text)
                    ));
                }
            }
            // A refusal message is written by this crate and interpolates ids, never prose.
            // Hashed anyway, on the rule that this tool emits no string it did not choose.
            Err(e) => out.push(format!("messages {id} | refused={}", digest(&e.to_string()))),
        }
    }

    out.push(format!("scope_violations.count = {}", ledger.scope_violations().len()));
    for v in ledger.scope_violations() {
        out.push(format!(
            "scope_violation {} | thread={} | thread_entity={} | turn_entity={}",
            v.turn_id,
            v.thread_id,
            v.thread_entity.clone().unwrap_or_else(|| "-".into()),
            v.turn_entity.clone().unwrap_or_else(|| "-".into()),
        ));
    }

    out.push(format!("pending_turns.count = {}", ledger.pending_turns().len()));
    out.push(format!("open_actions.count = {}", ledger.open_actions().len()));
    out.push(format!("ceo_facing_actions.count = {}", ledger.ceo_facing_actions().len()));
    out.push(format!("internal_actions.count = {}", ledger.internal_actions().len()));
    out.push(format!("unbound_threads.count = {}", ledger.unbound_threads().len()));

    Ok(out)
}

/// The file's own name plus its parent directory — enough to tell five ledgers apart in a
/// diff, without printing an absolute path out of somebody's home directory.
pub fn label(path: &Path) -> String {
    let file = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let dir = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    format!("{dir}/{file}")
}

pub fn digest(s: &str) -> String {
    format!("{}:{}", hex(s.as_bytes()), s.len())
}

pub fn hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
}
