//! The body of `get_machinery` — techy mode's read path (techy-mode design §3.4).
//!
//! In its own file for the same reason `timeline_view.rs` is: an example can include THIS
//! source by path and print the exact JSON the webview receives, so a payload proof cannot
//! drift from the command by re-implementing it.
//!
//! ## Why this is a SECOND command and not a mode argument on `get_timeline`
//! `timeline_view.rs` carries the CEO gate as a property nobody can route around: it calls
//! `view(ViewMode::Ceo)` and `Timeline` does not implement `Serialize`, so that function
//! never holds a raw command, a file path or an internal item. Adding a `mode` parameter
//! to it would trade that structural fact for a branch — and the calm path would then be
//! one wrong argument away from serving technical bytes. So the calm command is untouched,
//! byte for byte, and technical mode arrives through its own door.
//!
//! `ViewMode::Technical` still cannot reach an `Internal` item: `Visibility::renders_in`
//! is false for `Internal` in EVERY mode, and there is deliberately no `ViewMode::Internal`
//! to ask for (timeline.rs). Re-prime traffic, rotation machinery and model reasoning have
//! no render path here either.
//!
//! ## What it does NOT carry, and why
//! No raw payloads. §2.4 caps one payload at 32 KB and a working thread holds hundreds of
//! records, so shipping every raw blob on every thread open would put megabytes through
//! the IPC boundary to render a column of one-line rows. The raw pane is a second command
//! (`get_machinery_raw`), fetched when the CEO actually expands a row.

use richos_core::journal::ThreadMachinery;
use richos_core::read_view::SpineView;
use richos_core::timeline::ViewMode;
use serde_json::json;

/// The CEO-facing sentence for a thread that ran before the routing commit.
///
/// **"No machinery was recorded for this conversation" and "this conversation had no
/// machinery" are different sentences, and only the first one is true.** Retroactivity
/// began at the routing commit (richos `48561e4`); nothing earlier is recoverable, ever
/// (§5). This says what was recorded, not what happened.
pub const NOTHING_RECORDED: &str =
    "No machinery was recorded for this conversation. Retention started on 2026-08-28 and \
     anything Rich did before that was never written down, so this is a gap in the record, \
     not a quiet conversation.";

/// The CEO-facing sentence when no machinery has ever been retained on this install.
pub const NOT_RETAINED: &str =
    "Nothing has been recorded on this machine yet. The technical view reads a store that \
     hasn't been written to. It fills up as Rich works.";

/// The CEO-facing sentence when the store is there and could not be read. **Never the
/// empty-state sentence:** "I could not read it" and "there is nothing in it" are
/// different statements, and serving the second over the first is how a renderer lies.
/// The operator-facing reason travels alongside, in its own field, and names who owns it.
pub const UNREADABLE: &str =
    "I can't read the technical record for this conversation. It's on this machine and I \
     haven't lost it. Something is refusing to open it and whoever set RichOS up needs to \
     look.";

/// The CEO-facing sentence for a thread whose BETWEEN-TURN lane is empty (techy-mode §1.5).
///
/// Shaped exactly like [`NOTHING_RECORDED`] and for the same reason: the lane is empty in
/// two different situations and only one of them means the session was quiet. Between-turn
/// routing landed on 2026-08-30; every conversation older than that has an empty lane
/// because nothing was ever written, not because nothing ever arrived. The sentence says
/// which, rather than letting an empty box imply the feature is broken.
pub const BETWEEN_TURNS_QUIET: &str =
    "Nothing was recorded between turns in this conversation. Rich started keeping this on \
     2026-08-30, so in an older conversation that is a gap in the record, not proof the \
     session was quiet.";

/// One thread's machinery, as the technical view of its timeline, plus WHY there is
/// nothing when there is nothing.
///
/// The rows are the same items a reload projects, at the same ids, in the same
/// `(turn, slot, sequence)` order — §3.4's *"inline, in `seq` order, interleaved between
/// the message bubbles of the same turn"* is therefore the projection's ordering, not a
/// second ordering invented in the renderer.
///
/// Fails closed on an unbound thread, exactly like `get_messages` and `get_timeline`.
pub fn machinery_payload(spine: &dyn SpineView, thread_id: &str) -> Result<serde_json::Value, String> {
    // The state FIRST, and from the checked read — `Spine::timeline` uses the unchecked
    // one, which cannot tell an unreadable directory from an empty thread. If the store
    // refuses, say so and do not hand back a prose-only timeline that reads as "he did
    // nothing".
    let state = match spine.machinery_journal() {
        Some(journal) => journal.project_thread_checked(thread_id),
        // The spine runs headless with no journal attached; so does a shell that failed to
        // create the directory. A fact about the install, not about the thread.
        None => ThreadMachinery::NotRetained,
    };

    let (sentence, reason) = match &state {
        ThreadMachinery::Recorded(_) => (None, None),
        ThreadMachinery::NothingRecorded => (Some(NOTHING_RECORDED), None),
        ThreadMachinery::NotRetained => (Some(NOT_RETAINED), None),
        ThreadMachinery::Unreadable(why) => (Some(UNREADABLE), Some(why.clone())),
    };

    // The timeline is projected even in the empty states: the CEO's conversation is still
    // there and still has to render. What changes is the sentence under it.
    let timeline = spine.timeline(thread_id).map_err(|e| e.to_string())?;
    let view = timeline.view(ViewMode::Technical);

    // §1.5's between-turn lane travels inside `view.payload()` already — it is part of the
    // gated `TimelineView`. What does NOT travel with it is why the lane is empty when it
    // is empty, which is this field.
    //
    // **It is `None` when the store was UNREADABLE**, and that is the whole care taken here.
    // In that state the lane is empty because nothing could be read, so "nothing was
    // recorded between turns" would be a claim the store never supported — the same
    // substitution `UNREADABLE` exists to prevent, one level down. The state sentence above
    // already speaks for the whole view.
    let between_turns_sentence = match &state {
        ThreadMachinery::Unreadable(_) => None,
        _ if view.between_turns().is_empty() => Some(BETWEEN_TURNS_QUIET),
        _ => None,
    };

    Ok(json!({
        "threadId": thread_id,
        "state": state.as_str(),
        "rowCount": state.records().len(),
        "sentence": sentence,
        "reason": reason,
        "betweenTurnsSentence": between_turns_sentence,
        "timeline": view.payload(),
    }))
}
