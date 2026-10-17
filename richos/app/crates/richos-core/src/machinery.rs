//! The MACHINERY seam — every non-text frame the agent emits, routed instead of dropped.
//!
//! Built to `docs/plans/richos-techy-mode-2026-08-26.md` (Sage, 2026-08-26), Phase 1
//! (§5). **Re-based 2026-08-31 from the ACP adapter's `session/update` vocabulary onto the
//! NATIVE `claude` binary's stream-json stdio** (`wiki/ceo-decisions.md` §16 — *"the adapter
//! goes"*), against the frames actually captured in
//! `docs/verification/native-claude-stream-json-2026-08-31/raw/` and
//! `docs/verification/native-claude-tool-status-2026-08-31/raw/`. Where the design's
//! field-level assumptions and the wire disagree, the wire wins and the deviation is named
//! here.
//!
//! ## Why a second event family, and not new `StreamEvent` variants
//! `stream.rs:8-10` states the clean-output invariant as a property of the type: *"the
//! ONLY text that ever reaches a chunk event is assistant-message text … Tool calls,
//! worker chatter, hooks, and machinery have no path here at all."* Design §1.1/§1.3: if
//! machinery became a `StreamEvent`, that invariant would degrade from **structural** to
//! *"the renderer remembers to filter"*. So machinery travels on its own record, its own
//! observer trait, and its own Tauri event name — `rich://machinery` — and the default
//! UI's subscription list is the proof.
//!
//! Rich's formatted conversation lease takes its visible words only from the
//! native `StructuredOutput.reply` field. Its ordinary assistant text uses this
//! existing machinery lane as technical commentary. Worker and legacy leases
//! retain their original text path.
//!
//! ## ONE event name, deliberately (design §1.3)
//! `stream.rs` and `voice/event.rs` use one Tauri event per semantic event because those
//! sets are OURS and closed. The agent's frame set is the VENDOR's and open. One name plus
//! a `kind` field means a new vendor frame needs no new event, no new subscription, and no
//! contract revision.
//!
//! ## ONE frame can be SEVERAL records, and that is new
//! [`MachineryRecord::from_native_event`] returns a `Vec`, where the ACP normalizer it
//! replaces returned an `Option`. This is forced by the wire, not chosen: an `assistant`
//! frame carries `message.content` as an ARRAY, and one frame can hold a `thinking` block
//! AND a `tool_use` block. Collapsing them into one record would invent a fact. The
//! caller advances the shared per-turn `seq` (§1.4 G1) by exactly the number of records it
//! delivered, so the counter still numbers *delivered items*, exactly as before.
//!
//! ## What is retained, and the SIX drops, every one of them declared
//! Typed here: tool calls (opened from `tool_use`, closed from `tool_result`, kept alive by
//! `tool_progress`), `thinking`, the client-directed `can_use_tool` permission request, and
//! `Unknown`.
//!
//! Everything not typed — `system/init`, `system/status`, `system/thinking_tokens`,
//! `rate_limit_event`, `control_response`, `stream_event/message_delta` — falls to
//! `MachineryKind::Unknown`, which retains the vendor frame type as the title and the
//! verbatim payload. §1.4 G5 forbids re-introducing a silent drop.
//!
//! **The complete drop set on this wire. Nothing else returns no record:**
//!
//! 1. `stream_event → content_block_delta → text_delta` — THE clean-output text path, the
//!    native equivalent of ACP's `agent_message_chunk`. It reaches the CEO as
//!    `StreamEvent::Chunk` and a machinery copy would be a second source of truth for the
//!    reply.
//! 2. `assistant` frames' `text` blocks — the same fact in whole-message form, already
//!    delivered by (1).
//! 3. `user` frames' `text` blocks — **the ONE deliberate drop §1.2 named**, ported intact.
//!    `ledger.rs:386-409` already holds the CEO's words verbatim and fsync'd, and the stop
//!    control already records a stop, so neither an echoed prompt nor the CLI's injected
//!    `[Request interrupted by user]` marker may become a second source of truth. (Measured:
//!    `raw/run9-rust-driven.jsonl` carries exactly two `user` frames — one `tool_result`,
//!    one that interrupt marker — and **never our own prompt**, so on this wire the rule
//!    guards the marker rather than the echo.)
//! 4. The streaming ENVELOPE — `message_start`, `message_stop`, `content_block_stop`, and
//!    `content_block_start` for `text`/`thinking` blocks. They carry no fact of their own;
//!    they bracket (1).
//! 5. `input_json_delta` / `thinking_delta` / `signature_delta` — FRAGMENTS whose complete
//!    value arrives verbatim on the following `assistant` frame. Twenty-five
//!    `content_block_delta`s in one measured turn (`run9`); retaining them would be N
//!    partial copies of one fact, which is §1.2's own argument one level down.
//! 6. `result` — the turn TERMINAL. The driver returns its stop reason from `prompt`,
//!    exactly as the ACP path returned the `session/prompt` response and recorded no
//!    machinery for it.
//!
//! ## The watermark measurement is DERIVED here, not given
//! ACP emitted `usage_update {used, size}` outright. The native wire gives the numerator
//! and the denominator in different frames and at different times, so
//! [`MachineryRecord::from_context_usage`] builds an explicitly DERIVED record and
//! [`MachineryRecord::context_usage`] reads it back. `used` is
//! `stream_event/message_delta`'s `usage.input_tokens + cache_read_input_tokens +
//! cache_creation_input_tokens` (mid-turn, observed monotonic 29,342 → 30,113 in `run9`);
//! `size` is `result.modelUsage[<session model>].contextWindow` = 1,000,000 for
//! `claude-sonnet-5`, which arrives only when a turn ENDS. **Caveat C3 of the spike is real
//! and is honoured rather than papered over:** the first turn of a fresh lease has a
//! numerator and no denominator, so no record is emitted at all and the spine stays on its
//! chars÷4 estimate until a turn completes.
//!
//! **How wide that gap is in the shipping path, measured 2026-08-31 rather than reasoned
//! about** (`examples/watermark_roundtrip.rs`, live): the CEO never meets it, because the
//! lease's first turn is the RE-PRIME turn, which ends and supplies the denominator before
//! his first prompt is sent — the run reports `source=measured` after his FIRST turn. The
//! gap is real for a lease handed a CEO turn with no priming turn first, and nothing else.
//! Worth knowing what it costs while it lasts: on the same turn the chars÷4 estimate read
//! **632** tokens against a measured **19,477** — **30.8x under**, because the estimate
//! counts the CEO's words and the reply and cannot see the system prompt, the persona, the
//! hooks or the cached context the model is actually holding.
//!
//! ## Deviations forced by measured wire data
//! 1. **`tool_progress` keys on `parent_tool_use_id`, never `tool_use_id`.** Its own
//!    `tool_use_id` is a synthetic `<real-id>-heartbeat-<n>` that matches no row anywhere
//!    (`native-claude-tool-status-2026-08-31/findings.md`). Keying on the obvious field
//!    silently updates nothing.
//! 2. **A `tool_use` content-block START carries `input: {}`.** The complete arguments
//!    arrive on the following whole-message `assistant` frame, so §1.4 G2's merge is
//!    mandatory, not an optimization — the same conclusion the ACP path reached for a
//!    different reason.
//! 3. **There is no `status` string on this wire.** Status is a POSITION: `tool_use` opens
//!    `Pending`, `tool_progress` means `InProgress`, `tool_result` closes `Completed` or
//!    `Failed` from its boolean `is_error`. `ToolStatus::Other` is retained for a wire that
//!    starts carrying one.
//! 4. **`locations` does not exist.** Paths are extracted from the tool's own `input`
//!    (`file_path`, `path`, `notebook_path`) and from `tool_use_result.filePath`.
//! 5. **The SessionMeta suppression key ignores `uuid`.** Four `system/init` frames in
//!    `run9` differ ONLY by a per-frame `uuid`, so a verbatim-equality slot would suppress
//!    nothing and the "last value wins" rule would be dead code. See [`meta_identity`].
//!
//! ## What this file must NEVER do — a licence condition, not a preference
//! RichOS may never collect, store, or intermediate Claude credentials or session tokens
//! (`docs/research/claude-code-redistribution-2026-08-31.md`; `ceo-decisions.md` §16). The
//! `control_response` to `initialize` carries an `account` object with the customer's email
//! and subscription type. **It is not normalized here and the driver never retains it** —
//! see `native.rs`'s handshake, which reads that response for liveness only and keeps none
//! of it.

use crate::util::{new_id, now_millis};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The ONE Tauri event name for every machinery record (§1.3).
pub const EVENT_MACHINERY: &str = "rich://machinery";

/// Per-record cap on the raw `payload` (design §2.4). Measured in BYTES of the serialized
/// JSON, truncated at a char boundary — `&s[..n]` panics mid-codepoint on the first
/// non-ASCII byte, the same trap `ledger.rs:628-638` already warns about.
pub const PAYLOAD_MAX_BYTES: usize = 32 * 1024;

/// Cap on the bounded display `summary` (design §2.4): first non-empty line, 84 chars,
/// `"N lines"` fallback.
pub const SUMMARY_MAX_CHARS: usize = 84;

/// The native frame titles §1.2's SessionMeta rule applies to, **last value wins**.
///
/// Named here rather than inline at the one call site because the *reason* they are a set
/// is a property of the vendor's frame family, not of the caller: they are small,
/// near-static, and repeated once per turn. Measured on the native wire
/// (`docs/verification/native-claude-stream-json-2026-08-31/raw/run9-rust-driven.jsonl`):
/// **four `system/init` frames across four turns, and four `system/status` frames**, one of
/// each per turn — the same shape the ACP adapter's `available_commands_update` /
/// `session_info_update` pair had, in the vendor's new vocabulary. `system/init` is the
/// large one: it carries `slash_commands` (49), `tools`, `agents`, `models`, `mcp_servers`,
/// `permissionMode` and `capabilities`, i.e. everything BOTH retired ACP kinds carried and
/// more.
///
/// These are the strings [`MachineryRecord::from_native_event`] uses as the record `title`,
/// not raw `type` values, because `system` alone does not identify a family — the subtype
/// does.
pub const SESSION_META_KINDS: [&str; 3] = ["system:init", "system:status", "rate_limit_event"];

/// Whether a normalized record title belongs to the last-value-wins SessionMeta family.
pub fn is_session_meta(title: &str) -> bool {
    SESSION_META_KINDS.contains(&title)
}

/// The title one raw frame normalizes to, WITHOUT normalizing it.
///
/// The between-turn lane has to classify a frame before it can decide whether to queue it,
/// and building a whole `MachineryRecord` (with its uuid and its clock read) only to throw it
/// away would be waste on the reader thread. This is the one place both answers come from, so
/// the lane and the record can never disagree about what a frame is called.
///
/// `"user:text"` is the answer for §1.2's ONE deliberate drop, which is why a frame that
/// produces no record still has a title.
pub fn frame_title(frame: &Value) -> String {
    let ty = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "system" => format!("system:{}", frame.get("subtype").and_then(|v| v.as_str()).unwrap_or("")),
        "user" => {
            let only_text = content_blocks(frame)
                .iter()
                .all(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"));
            if only_text && !content_blocks(frame).is_empty() {
                "user:text".to_string()
            } else {
                "user".to_string()
            }
        }
        other => other.to_string(),
    }
}

/// The comparison key for §1.2's last-value-wins slot: the frame with its per-frame
/// identifiers removed.
///
/// **Measured, and it is the whole reason this function exists.** The four `system/init`
/// frames in `run9-rust-driven.jsonl` are byte-identical except for `uuid` — sha256 of the
/// canonicalized frames: `acbe3949a0de`, `43f5fdcdeeae`, `2b06161ab350`, `6baf67daacfa`,
/// four distinct digests for four frames that say the same thing. A slot comparing frames
/// verbatim, as the ACP path could, would suppress **zero** of them and the "retaining every
/// repeat is waste" rule would be dead code that looked alive.
///
/// `uuid` is the frame's own identity and `session_id` is the lease's; neither is part of
/// the statement the frame makes. Everything else is compared.
pub fn meta_identity(frame: &Value) -> Value {
    let Some(obj) = frame.as_object() else {
        return frame.clone();
    };
    let mut stripped = obj.clone();
    stripped.remove("uuid");
    stripped.remove("session_id");
    Value::Object(stripped)
}

/// The title carried by the marker record that reports a between-turn buffer overflow.
///
/// A vendor kind never has this spelling, so it cannot collide with a real update, and it
/// is a CONSTANT because the JS suite and the Rust test both assert on it and two spellings
/// is how a marker stops being findable.
pub const BETWEEN_TURN_OVERFLOW: &str = "between_turn_overflow";

/// Our normalized kind. Deliberately NOT one-to-one with the vendor's `sessionUpdate` —
/// see the module doc for what Phase 1 types and what falls to `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachineryKind {
    ToolCall,
    Thought,
    PermissionRequested,
    ClientFsCall,
    /// The child pausing the turn to summarize its own conversation and drop the detail —
    /// see [`compaction_phase`] for the frames, and the arithmetic that identified them.
    Compaction,
    Unknown,
}

impl MachineryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MachineryKind::ToolCall => "tool_call",
            MachineryKind::Thought => "thought",
            MachineryKind::PermissionRequested => "permission_requested",
            MachineryKind::ClientFsCall => "client_fs_call",
            MachineryKind::Compaction => "compaction",
            MachineryKind::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// COMPACTION — the 38-62 SECOND SILENCE, AND THE FRAME THAT ANNOUNCES IT
//
// The first outside user of RichOS, 2026-09-06: a long wait *"looks like a crashed
// application"*. One cause of a long wait is the child compacting its own context mid-turn:
// measured at 38.4-62.0 s, mean 51.1 s, over 14 boundaries by `echo-opus-q14`
// (`docs/verification/inner-doctrine-opens-2026-09-06/` Q1.3, re-derived here from their
// committed frames: n=14, min 38397 ms, max 62029 ms, mean 51149.4 ms) and over 2 more of
// mine below. For that whole time the calm surface received NOTHING, because every `system`
// frame fell to `MachineryKind::Unknown`, which `timeline.rs` routes to `Visibility::Technical`.
//
// WHICH FRAME, AND WHY IT IS NOT THE ONE THE BRIEF NAMED
// =====================================================
// `system/compact_boundary` cannot drive a live label. It arrives at the END of the pause and
// reports it in the past tense — its `duration_ms` is the span that already elapsed. Timed
// against a real `claude` 2.1.263 stream
// (`docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl.timed.jsonl`,
// `python3 gaps.py`):
//
//     turn 3 prompt sent            t =  5177 ms
//     system/status "compacting"    t =  5190 ms   <- the pause STARTS here
//     system/status "compacting"    t = 35190 ms   <- +30000 ms exactly, a heartbeat
//     status compact_result:success t = 48805 ms   <- the pause ENDS here
//     system/compact_boundary       t = 48805 ms   duration_ms = 43611
//
//     48805 - 5190 = 43615 ms observed against 43611 ms reported: 4 ms apart.
//     Second boundary, same cell: 90231 - 52086 = 38145 ms against 38138 ms: 7 ms apart.
//
// So `system/status` is the announcement, it lands 13 ms after the prompt is accepted, and it
// REPEATS every 30.000 s while the compaction runs. `compact_boundary` is left as an
// `Unknown` technical record on purpose: it says nothing about the pause that the pair below
// does not, it arrives too late to say it, and reading it would have required an edit to
// `native.rs`, which belongs to another agent this round.
// ---------------------------------------------------------------------------

/// Where a compaction is, as the wire states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionPhase {
    /// `status: "compacting"` — emitted at the start and repeated every 30.000 s after.
    Started,
    /// `compact_result: "success"` — the context was summarized and the pause is over.
    MadeRoom,
    /// `compact_result: "failed"` — the attempt was abandoned and the pause is over. Measured
    /// twice in `cellT1` with `compact_error: "too_few_groups"`, 1 ms after the start, which
    /// is what a 2% threshold override does to a conversation with almost no history to
    /// summarize. **The turn then proceeded normally and answered**, both times.
    KeptGoing,
}

impl CompactionPhase {
    /// The record's own status. The vendor's outcome is kept HERE, truthfully, even though
    /// `timeline.rs` shows the CEO one `completed` state for both endings — the pause is what
    /// ended, and a red "failed" mark for a compaction that cost him nothing would be a false
    /// alarm. The distinction survives in words, in the summary, and in the raw payload.
    pub fn status(self) -> ToolStatus {
        match self {
            CompactionPhase::Started => ToolStatus::InProgress,
            CompactionPhase::MadeRoom => ToolStatus::Completed,
            CompactionPhase::KeptGoing => ToolStatus::Failed,
        }
    }
}

/// Which compaction phase a frame states, or `None` for every other frame on the wire.
///
/// Only two shapes match, both `system/status`, both verbatim from `cellT1.jsonl`:
///
/// ```json
/// {"type":"system","subtype":"status","status":"compacting","session_id":"…","uuid":"…"}
/// {"type":"system","subtype":"status","status":null,"compact_result":"failed",
///  "compact_error":"too_few_groups","session_id":"…","uuid":"…"}
/// ```
///
/// Deliberately NOT matched: `status: "requesting"` (measured 3 ms before `compacting` on
/// every turn, including turns that never compact, so it announces nothing), and
/// `system/compact_boundary` (see the block above). An unrecognized `compact_result` string
/// is read as an ENDING rather than ignored — the pause is over either way, and the only
/// thing at stake is which of two sentences is said.
pub fn compaction_phase(frame: &Value) -> Option<CompactionPhase> {
    if frame.get("type").and_then(|v| v.as_str()) != Some("system") {
        return None;
    }
    if frame.get("subtype").and_then(|v| v.as_str()) != Some("status") {
        return None;
    }
    if let Some(result) = frame.get("compact_result").and_then(|v| v.as_str()) {
        return Some(if result == "success" { CompactionPhase::MadeRoom } else { CompactionPhase::KeptGoing });
    }
    if frame.get("status").and_then(|v| v.as_str()) == Some("compacting") {
        return Some(CompactionPhase::Started);
    }
    None
}

/// The merge key that folds one compaction's start, its 30-second heartbeats and its ending
/// into ONE row.
///
/// **The wire carries no correlation id.** The start, the heartbeat and the ending share only
/// `session_id`; their `uuid`s all differ and nothing points from one to another. So the key
/// is the lease's, and the TURN does the rest of the scoping: `LiveTurn` is built per turn
/// (`live.rs`), and [`project`] keys a compaction row on `(turn, key)` for the reload path.
///
/// **The limit that leaves, named rather than discovered later:** two compactions inside ONE
/// turn would fold into one row whose state is the later one's. Never observed — 16 of 16
/// measured boundaries (q14's 14 and this record's 2) are one per turn, because the trigger is
/// the incoming prompt.
pub fn compaction_key(session_id: &str) -> String {
    format!("compaction:{session_id}")
}

// ---------------------------------------------------------------------------
// HOW LONG A COMPACTION HAS EVER TAKEN - the one number a paced bar is allowed
//
// CEO, 2026-09-06, asked whether the compaction row could carry a bar like the splash
// screen's, and was told plainly that the duration is unknown:
//
//   *"It doesn't need to know. It just needs to move to almost full with the expected
//    minimum time and then stay at 'almost full' until all finished etc."*
//
// That is the whole design, and it is why a bar can be honest here: it fills over the
// SHORTEST compaction ever measured, and then it HOLDS. It never claims a completion it does
// not have. Reaching almost-full and stopping is itself a true statement - *this one is
// taking longer than the fastest case* - where a bar that stalls mid-way looks broken and a
// bar that completes early lies.
//
// THE NUMBER, RE-DERIVED HERE RATHER THAN QUOTED. Every `system/compact_boundary` frame in
// every committed cell, read by `compact_metadata.duration_ms`:
//
//   $ python3 -c 'import json,glob,statistics; ds=[json.loads(l)["compact_metadata"]["duration_ms"]
//       for p in sorted(glob.glob("docs/verification/inner-doctrine-opens-2026-09-06/raw/*.jsonl"))
//             +["docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl"]
//       for l in open(p) if l.strip() and json.loads(l).get("subtype")=="compact_boundary"];
//       print(len(ds), min(ds), max(ds), statistics.mean(ds))'
//   16 38138 62029 49865.0
//
//   sorted: 38138 38397 39085 42950 43611 44813 46975 49790
//           50158 50411 50654 58298 59901 60747 61883 62029
//
// **One correction to the record this constant is built on, small and mine.**
// `compaction-notice-2026-09-06/README.md` gives the population as 16 boundaries,
// 38138-62029 ms - min and max stand exactly - and also carries "mean ~51,149 ms". That mean
// is the n=14 one (51149.4); adding this record's own 43611 and 38138 brings the n=16 mean to
// 49865.0. Nothing downstream reads the mean. It is corrected here so the next author does
// not inherit a number that quietly changed population.
//
// WHY THE MINIMUM AND NOT THE MEAN. A bar paced on the mean would already be holding on 8 of
// these 16 and still climbing on the other 8, so half of all compactions would see it jump
// forward at the end from wherever it happened to have got to. Paced on the MINIMUM it is
// climbing for the first 38.1 s of every compaction ever measured and holding for the rest of
// every one of them: one rule, the same every time, and the hold means exactly what it says.
//
// THIS NUMBER NEVER REACHES THE SCREEN AS A NUMBER. It is a rate, not a claim. Nothing in the
// UI renders "38 seconds", a percentage, a countdown or an estimate - the sentence beside the
// bar says no number at all and two suites enforce it (`app/ui/tests/compaction-notice.js`
// and `app/ui/tests/compaction-progress.js`).
// ---------------------------------------------------------------------------

/// The shortest compaction ever measured, in milliseconds. The derivation is directly above,
/// and `app/ui/tests/compaction-progress.js` re-runs that command against the committed
/// frames on every run, so this constant cannot drift from the record it claims to come from.
pub const COMPACTION_MEASURED_MIN_MS: u64 = 38_138;

/// The shortest duration ever MEASURED for a wait of this kind, or `None` when there is no
/// measurement - which is every other kind, and the honest default.
///
/// It is emitted on the timeline item (`timeline.rs`, `measuredMinMs`) so a renderer can PACE
/// something over it. It is deliberately not an estimate, a prediction or a remaining time:
/// it is the floor of an observed population, and it says nothing whatever about the wait
/// currently running except that no such wait has ever been shorter.
pub fn measured_min_ms(kind: MachineryKind) -> Option<u64> {
    match kind {
        MachineryKind::Compaction => Some(COMPACTION_MEASURED_MIN_MS),
        // A tool call has no floor worth pacing on: the committed runs in
        // `docs/verification/native-claude-tool-status-2026-08-31/` hold calls from a few
        // milliseconds to minutes, and a bar over a population that wide would be invention.
        _ => None,
    }
}

/// THE ONE MERGE KEY DERIVATION. Every map that folds records into rows uses this and nothing
/// else — [`project`], `live.rs`'s `on_machinery`, and `timeline.rs`'s `resolve_last_seen` and
/// `activity_item`.
///
/// It exists because the compaction key is the LEASE's and a lease outlives a turn, so the key
/// has to carry the turn — and a second copy of that rule is how one map disagrees with
/// another. It nearly did: `resolve_last_seen` keyed on `tool_call_id` alone, which would have
/// given turn 3's compaction row turn 4's `completedAt`.
///
/// A tool call keys on the vendor's own id, which is unique across the session, so the turn
/// changes nothing for it and is not added.
pub fn merge_key(row: &MachineryRecord) -> String {
    match (row.kind, row.tool_call_id.as_ref()) {
        (MachineryKind::Compaction, Some(id)) => format!("{}::{}", row.turn_id.as_deref().unwrap_or("-"), id),
        (_, Some(id)) => id.clone(),
        (_, None) => row.machinery_id.clone(),
    }
}

/// A tool call's lifecycle status. The four the ACP schema declares, plus `Other` so an
/// unrecognized wire value is RETAINED rather than silently dropped (§1.4 G5's rule
/// applied one level down, to a field instead of a kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Other(String),
}

impl ToolStatus {
    pub fn from_wire(s: &str) -> ToolStatus {
        match s {
            "pending" => ToolStatus::Pending,
            "in_progress" => ToolStatus::InProgress,
            "completed" => ToolStatus::Completed,
            "failed" => ToolStatus::Failed,
            other => ToolStatus::Other(other.to_string()),
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            ToolStatus::Pending => "pending",
            ToolStatus::InProgress => "in_progress",
            ToolStatus::Completed => "completed",
            ToolStatus::Failed => "failed",
            ToolStatus::Other(s) => s,
        }
    }
    /// Terminal for the CEO's status dot. `Other` is deliberately NOT terminal — an
    /// unknown status is not a claim that the work finished.
    pub fn is_terminal(&self) -> bool {
        matches!(self, ToolStatus::Completed | ToolStatus::Failed)
    }
}

impl Serialize for ToolStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for ToolStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(ToolStatus::from_wire(&String::deserialize(d)?))
    }
}

/// The normalized machinery record (design §1.3), as written to the journal and delivered
/// on `rich://machinery`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineryRecord {
    /// Ours, because the vendor gives us none for thoughts (`util.rs:16`).
    pub machinery_id: String,
    pub thread_id: String,
    /// `None` for re-prime and between-turn traffic — a first-class state, not a bug (§1.5, G4).
    pub turn_id: Option<String>,
    /// Which compute lease produced it. Rotation is invisible to the CEO but must stay
    /// reconstructible (`ledger.rs:170`).
    pub session_id: String,
    /// THE ordering guarantee (§1.4 G1) — ONE counter per turn, shared with assistant text.
    pub seq: u64,
    /// Epoch millis. A LABEL, never the ordering key (`util.rs:5-8`, §1.4 G3).
    pub at: u64,
    pub kind: MachineryKind,
    /// The merge key for `tool_call` / `tool_call_update` (§1.4 G2).
    pub tool_call_id: Option<String>,
    pub status: Option<ToolStatus>,
    /// Empty string means "the wire carried no title on THIS event" — which is what makes
    /// the merge in `project()` correct rather than destructive.
    pub title: String,
    pub summary: Option<String>,
    /// File paths touched. Extracted from the wire's `[{path, line?}]` objects.
    pub locations: Vec<String>,
    /// **The frame's `parent_tool_use_id`: set when this record came from a WORKER's frame
    /// nested under the lead's `Agent` call**, `None` for the lead's own (Output side panel PRD
    /// §4.1 (a), richos-hq `docs/prds/2026-10-05-output-side-panel.md`).
    ///
    /// It has to be a field because the retained `tool_use` and `tool_result` records keep the
    /// BLOCK, not the frame, as their payload, and the merge keeps the last payload — so after
    /// the merge nothing else says whose call it was. Witness (a) skips any merged call that
    /// carries one: a nested worker's `Write` is not Rich's, and witness (b) records it under
    /// the worker's name.
    ///
    /// `None` on every row written before the field existed (`serde(default)`), and omitted
    /// when `None`, so a lead's record serializes byte-for-byte as it did before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    /// `true` ⇒ never rendered in a thread view (§1.5): re-prime traffic, handoff
    /// summaries, crash-recovery machinery. Mirrors `ActionVisibility::Internal`
    /// (`ledger.rs:119-133`) and honours the standing order that Rich never reveals
    /// session rotation.
    pub internal: bool,
    /// The raw `update` JSON, subject to the raw window (§2.4). `None` on a record read
    /// back from the journal whose Tier-B sidecar has been evicted.
    #[serde(default)]
    pub payload: Option<Value>,
    pub truncated: bool,
}

impl MachineryRecord {
    /// Normalize ONE line of the native binary's stream-json stdout into zero, one or
    /// several records.
    ///
    /// Returns a `Vec` because one frame can be several facts — see the module doc. Records
    /// come back numbered `seq, seq+1, …`; the caller advances its shared per-turn counter
    /// (§1.4 G1) by `result.len()`, so the counter still numbers delivered items and a
    /// dropped frame still consumes no position.
    ///
    /// An empty `Vec` is one of the six DECLARED drops in the module doc and nothing else.
    ///
    /// `thread_id` / `turn_id` / `internal` are stamped by the caller that knows them
    /// (the spine); the driver knows only the session and the seq.
    pub fn from_native_event(frame: &Value, session_id: &str, seq: u64) -> Vec<MachineryRecord> {
        let ty = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");
        // Whose frame this is, stamped on EVERY record it yields (the `content_block_start`,
        // `tool_use`, `tool_result` and `text` routes alike). Not on `tool_progress`: there the
        // field names the tool call the heartbeat BELONGS to (DEVIATION 1 below), which is the
        // lead's own call as often as not, so reading it as "nested" would be wrong.
        let parent_tool_use_id = if ty == "tool_progress" {
            None
        } else {
            frame.get("parent_tool_use_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string)
        };
        let mut out: Vec<MachineryRecord> = Vec::new();
        let mut push = |kind, tool_call_id, status, title: String, summary, locations, body: &Value| {
            let (payload, truncated) = cap_payload(body);
            out.push(MachineryRecord {
                machinery_id: new_id("mach"),
                thread_id: String::new(),
                turn_id: None,
                session_id: session_id.to_string(),
                seq: seq + out.len() as u64,
                at: now_millis(),
                kind,
                tool_call_id,
                status,
                title,
                summary,
                locations,
                parent_tool_use_id: parent_tool_use_id.clone(),
                internal: false,
                payload,
                truncated,
            });
        };

        match ty {
            // ---- the streamed half of a message -------------------------------------
            "stream_event" => {
                let ev = frame.get("event").unwrap_or(&Value::Null);
                let ev_ty = ev.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match ev_ty {
                    // DROP 4 — the envelope. It brackets the text path and states nothing.
                    "message_start" | "message_stop" | "content_block_stop" => {}
                    "content_block_start" => {
                        let block = ev.get("content_block").unwrap_or(&Value::Null);
                        match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                            // The tool row OPENS here, streamed, before the arguments
                            // exist — earlier than the ACP path ever opened one.
                            // `input: {}` is measured (run9:5); deviation 2. The summary is
                            // still read from it rather than hard-coded to `None`: the
                            // measured open carries nothing, so this yields nothing today,
                            // and a build that starts populating it costs no code change.
                            "tool_use" => {
                                let input = block.get("input").unwrap_or(&Value::Null);
                                push(
                                    MachineryKind::ToolCall,
                                    block.get("id").and_then(|v| v.as_str()).map(str::to_string),
                                    Some(ToolStatus::Pending),
                                    block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                    tool_input_summary(input),
                                    input_locations(input),
                                    frame,
                                )
                            }
                            // DROP 4 — `text` and `thinking` block starts are envelope.
                            "text" | "thinking" => {}
                            other => push(
                                MachineryKind::Unknown,
                                None,
                                None,
                                format!("content_block_start:{other}"),
                                None,
                                Vec::new(),
                                frame,
                            ),
                        }
                    }
                    "content_block_delta" => {
                        match ev.get("delta").and_then(|d| d.get("type")).and_then(|v| v.as_str()).unwrap_or("") {
                            // DROP 1 — THE clean-output text path.
                            "text_delta" => {}
                            // DROP 5 — fragments; the whole value arrives on `assistant`.
                            "input_json_delta" | "thinking_delta" | "signature_delta" => {}
                            other => push(
                                MachineryKind::Unknown,
                                None,
                                None,
                                format!("content_block_delta:{other}"),
                                None,
                                Vec::new(),
                                frame,
                            ),
                        }
                    }
                    // Retained: this is the frame the watermark numerator is read from.
                    other => push(
                        MachineryKind::Unknown,
                        None,
                        None,
                        format!("stream_event:{other}"),
                        None,
                        Vec::new(),
                        frame,
                    ),
                }
            }

            // ---- the whole-message half ---------------------------------------------
            "assistant" => {
                for block in content_blocks(frame) {
                    match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                        // The arguments, complete. Merges into the row opened by the
                        // `content_block_start` above on the same `id` (§1.4 G2).
                        //
                        // **THE TITLE IS THE HUMAN LINE HERE, NOT THE TOOL NAME, AND THE
                        // MERGE IS WHY.** `merge_into` keeps the LAST payload, and the last
                        // frame of a tool call is its `tool_result` — which carries the
                        // outcome and nothing about what was asked. So if the command lived
                        // only in this frame's payload it would be gone from the projected
                        // row, and the technical view would show `Bash` / `Host prod` with
                        // no command between them. The title is where it survives, exactly
                        // as ACP's `tool_call_update.title` carried it.
                        //
                        // Falls back to the tool NAME when the input has no summarizable
                        // field, so an unrecognized tool still names itself rather than
                        // going blank. Classification is unaffected: `timeline::classify`
                        // reads `name` off the payload, never the title.
                        "tool_use" => {
                            let input = block.get("input").unwrap_or(&Value::Null);
                            let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            push(
                                MachineryKind::ToolCall,
                                block.get("id").and_then(|v| v.as_str()).map(str::to_string),
                                None,
                                tool_input_summary(input).unwrap_or_else(|| name.to_string()),
                                // NOT a summary. The summary slot belongs to the OUTCOME,
                                // which arrives on the `tool_result`; filling it here with
                                // the arguments would put the request where the result goes
                                // for every tool that never returns one.
                                None,
                                input_locations(input),
                                block,
                            );
                        }
                        // OBSERVED AS PRESENCE ONLY: 7 thinking blocks across the native
                        // captures, every one with an empty `thinking` text and a signature.
                        // The route exists so that the day real text arrives there is no
                        // hole — the same reason the ACP path carried it unobserved.
                        "thinking" => push(
                            MachineryKind::Thought,
                            None,
                            None,
                            "thinking".to_string(),
                            summarize(block.get("thinking").and_then(|v| v.as_str()).unwrap_or("")),
                            Vec::new(),
                            block,
                        ),
                        // Only the lead's text is clean conversation output. A native
                        // worker's whole message remains attributed operational evidence.
                        "text" => {
                            if frame.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null()) {
                                let detail=serde_json::json!({"parent_tool_use_id":frame["parent_tool_use_id"],"block":block});
                                push(MachineryKind::Unknown,None,None,"Worker report".into(),
                                     summarize(block.get("text").and_then(Value::as_str).unwrap_or("")),Vec::new(),&detail);
                            }
                        }
                        other => push(
                            MachineryKind::Unknown,
                            None,
                            None,
                            format!("assistant:{other}"),
                            None,
                            Vec::new(),
                            block,
                        ),
                    }
                }
            }

            "user" => {
                for block in content_blocks(frame) {
                    match block.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                        // The tool row CLOSES here. Status is a POSITION on this wire, not
                        // a string (deviation 3): `is_error` decides.
                        "tool_result" => {
                            let failed = block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                            let detail = serde_json::json!({
                                "block": block,
                                "tool_use_result": frame.get("tool_use_result"),
                            });
                            let mut locations = Vec::new();
                            if let Some(p) =
                                frame.get("tool_use_result").and_then(|r| r.get("filePath")).and_then(|v| v.as_str())
                            {
                                locations.push(p.to_string());
                            }
                            push(
                                MachineryKind::ToolCall,
                                block.get("tool_use_id").and_then(|v| v.as_str()).map(str::to_string),
                                Some(if failed { ToolStatus::Failed } else { ToolStatus::Completed }),
                                // EMPTY means "absent", which is what makes the merge
                                // non-destructive — a `tool_result` knows no tool name and
                                // must never overwrite the one the open event set.
                                String::new(),
                                summarize(&block_text(block.get("content"))),
                                locations,
                                &detail,
                            );
                        }
                        // DROP 3 — the ONE deliberate drop (§1.2), ported intact.
                        "text" => {}
                        other => push(
                            MachineryKind::Unknown,
                            None,
                            None,
                            format!("user:{other}"),
                            None,
                            Vec::new(),
                            block,
                        ),
                    }
                }
            }

            // ---- the liveness heartbeat, and its trap -------------------------------
            //
            // DEVIATION 1, and it would have cost a day: `tool_use_id` here is a synthetic
            // `<real-id>-heartbeat-<n>` that matches no row anywhere. The row this belongs
            // to is `parent_tool_use_id`. Keying on the obvious field updates nothing,
            // silently. Measured cadence 30.002 s
            // (`native-claude-tool-status-2026-08-31/findings.md`).
            "tool_progress" => {
                let elapsed = frame.get("elapsed_time_seconds").and_then(|v| v.as_u64());
                push(
                    MachineryKind::ToolCall,
                    frame.get("parent_tool_use_id").and_then(|v| v.as_str()).map(str::to_string),
                    Some(ToolStatus::InProgress),
                    String::new(),
                    elapsed.map(|s| format!("running {s}s")),
                    Vec::new(),
                    frame,
                );
            }

            // ---- DROP 6 — the turn terminal, returned as the stop reason -------------
            "result" => {}

            // ---- everything else is RETAINED verbatim (§1.4 G5) ----------------------
            "system" => {
                let sub = frame.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
                // THE ONE TYPED `system` FRAME: the compaction the CEO is sitting through.
                // Everything else on this subtype family stays `Unknown` exactly as before —
                // see [`compaction_phase`] for what is matched and what is deliberately not.
                match compaction_phase(frame) {
                    Some(phase) => push(
                        MachineryKind::Compaction,
                        Some(compaction_key(session_id)),
                        Some(phase.status()),
                        // The title stays the frame's own, because [`frame_title`] is
                        // documented as the ONE place the lane and the record agree about
                        // what a frame is called, and `is_session_meta` reads that string.
                        format!("system:{sub}"),
                        None,
                        Vec::new(),
                        frame,
                    ),
                    None => {
                        push(MachineryKind::Unknown, None, None, format!("system:{sub}"), None, Vec::new(), frame)
                    }
                }
            }
            other => push(MachineryKind::Unknown, None, None, other.to_string(), None, Vec::new(), frame),
        }

        out
    }

    /// The DERIVED context measurement the rotation watermark runs on.
    ///
    /// **Derived, and it says so in the payload**, because the native wire does not hand it
    /// over the way ACP's `usage_update` did: `used` comes from a mid-turn
    /// `stream_event/message_delta` and `size` from a previous turn's
    /// `result.modelUsage[<session model>].contextWindow`. `native.rs` is the only caller,
    /// it holds both halves, and it emits nothing at all until it holds BOTH — spike caveat
    /// C3, honoured rather than papered over.
    ///
    /// `usage` is the verbatim vendor object the numerator was summed from, so the record
    /// still contains the bytes that produced the number.
    pub fn from_context_usage(
        used: u64,
        size: u64,
        usage: &Value,
        session_id: &str,
        seq: u64,
    ) -> MachineryRecord {
        let detail = serde_json::json!({
            "kind": CONTEXT_USAGE,
            "derived": true,
            "used": used,
            "size": size,
            "usedFrom": "stream_event.message_delta.usage \
                         (input_tokens + cache_read_input_tokens + cache_creation_input_tokens)",
            "sizeFrom": "result.modelUsage[<session model>].contextWindow",
            "usage": usage,
        });
        let (payload, truncated) = cap_payload(&detail);
        MachineryRecord {
            machinery_id: new_id("mach"),
            thread_id: String::new(),
            turn_id: None,
            session_id: session_id.to_string(),
            seq,
            at: now_millis(),
            kind: MachineryKind::Unknown,
            tool_call_id: None,
            status: None,
            title: CONTEXT_USAGE.to_string(),
            summary: Some(format!("{used} / {size} tokens")),
            locations: Vec::new(),
            parent_tool_use_id: None,
            internal: false,
            payload,
            truncated,
        }
    }

    /// The `{used, size}` pair off a derived context-usage record, `None` for every other.
    ///
    /// **Read this from the STREAM, never from the journal.** The record lands in the
    /// evictable Tier B (`journal.rs`), so a record read back after eviction has
    /// `payload: None` and this returns `None` - correctly, because at that point the
    /// measurement genuinely is gone. The spine consumes it in `deliver`'s drain closure,
    /// as the record goes past.
    ///
    /// Returns `None` on a `truncated` record too: a truncated payload is a JSON *string*,
    /// not an object, and half a number is not a measurement. In practice this never
    /// fires - a derived payload is ~400 bytes against a 32 KB cap - but the check is here
    /// so the type can never be built out of a fragment.
    pub fn context_usage(&self) -> Option<ContextUsage> {
        if self.truncated {
            return None;
        }
        let p = self.payload.as_ref()?;
        if p.get("kind").and_then(|v| v.as_str()) != Some(CONTEXT_USAGE) {
            return None;
        }
        // BOTH fields required. A numerator says nothing about the denominator, and
        // guessing a denominator is the exact failure this replaces.
        Some(ContextUsage { used: p.get("used")?.as_u64()?, size: p.get("size")?.as_u64()? })
    }

    /// Normalize a client-directed `control_request{can_use_tool}` (§1.2). Recording the
    /// decision is a FACT, not a policy: `native::decide_permission` still auto-approves
    /// exactly as `acp.rs:444-479` did before it was deleted, and this design says nothing
    /// else about gap #1.
    ///
    /// Measured shape (`run9-rust-driven.jsonl:17`, and `run8`): `request = { subtype,
    /// tool_name, display_name, input, description, permission_suggestions, decision_reason,
    /// decision_reason_type, tool_use_id }`. `tool_use_id` is what links the record back to
    /// the tool row it is about.
    ///
    /// **Richer than the ACP shape it replaces**, and the extra field is retained rather
    /// than discarded: `decision_reason` says WHY approval was needed, in words — *"Path is
    /// outside allowed working directories"*. ACP carried no equivalent on any field.
    pub fn from_permission_request(
        request: &Value,
        chosen: &str,
        session_id: &str,
        seq: u64,
    ) -> MachineryRecord {
        // The resolved human line first (it is specific), then the display name, then the
        // raw tool name. Same precedence rule the ACP path used, over the fields that exist.
        let title = ["description", "display_name", "tool_name"]
            .iter()
            .find_map(|k| request.get(*k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
            .unwrap_or("permission request")
            .to_string();
        let detail = serde_json::json!({
            "request": request,
            "chosen": chosen,
            "decisionReason": request.get("decision_reason"),
            "decisionReasonType": request.get("decision_reason_type"),
            // The native callback records the decision. It cannot infer whether
            // the scoped desk or a validated host tool contract supplied it.
        });
        let (payload, truncated) = cap_payload(&detail);
        MachineryRecord {
            machinery_id: new_id("mach"),
            thread_id: String::new(),
            turn_id: None,
            session_id: session_id.to_string(),
            seq,
            at: now_millis(),
            kind: MachineryKind::PermissionRequested,
            tool_call_id: request.get("tool_use_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
            status: None,
            title,
            summary: Some(format!("permission decision: {chosen}")),
            locations: input_locations(request.get("input").unwrap_or(&Value::Null)),
            parent_tool_use_id: None,
            internal: false,
            payload,
            truncated,
        }
    }

    /// Normalize a client-directed file-IO request.
    ///
    /// **Named honestly, and it now has LESS traffic than it did, not more.** On the ACP
    /// path this route existed and `claude-agent-acp` 0.70.0 emitted zero of them across
    /// five probe runs. On the native path the question is closed the other way: the CLI
    /// does its own file IO and was **OBSERVED NOT TO ASK** — it wrote `rust-probe-out.txt`
    /// and edited `edit-target.txt` itself, asking only for *permission*
    /// (`native-claude-stream-json-2026-08-31/findings.md` §4, `run9`/`run11`).
    ///
    /// So this constructor has **no producer in `native.rs`**, deliberately, and that is
    /// stated rather than left for a reader to discover. It is kept because
    /// `MachineryKind::ClientFsCall` is part of the record contract the UI renders and the
    /// journal has already written, and deleting a kind the stored records use would make
    /// old journals unreadable.
    pub fn from_client_fs_call(method: &str, params: &Value, session_id: &str, seq: u64) -> MachineryRecord {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let (payload, truncated) = cap_payload(&serde_json::json!({ "method": method, "params": params }));
        MachineryRecord {
            machinery_id: new_id("mach"),
            thread_id: String::new(),
            turn_id: None,
            session_id: session_id.to_string(),
            seq,
            at: now_millis(),
            kind: MachineryKind::ClientFsCall,
            tool_call_id: None,
            status: None,
            title: format!("{method} {path}"),
            summary: None,
            locations: if path.is_empty() { Vec::new() } else { vec![path.to_string()] },
            parent_tool_use_id: None,
            internal: false,
            payload,
            truncated,
        }
    }

    /// Normalize ONE frame that arrived while NO turn was in flight (§1.5's first gap).
    ///
    /// Between-turn traffic is not a different KIND of traffic — it is the same vendor
    /// frames arriving at a moment when `current_prompt` is `None`, which before §1.5 meant
    /// they hit no sink at all. So this delegates to [`MachineryRecord::from_native_event`]
    /// and differs from it in exactly two places, both forced by the absence of a turn:
    ///
    ///   1. **Assistant TEXT is RETAINED here**, where in-turn it is dropped. In a turn it
    ///      is the clean-output text path and a machinery copy would be a second source of
    ///      truth for the reply. Between turns there is no turn to attach text to and no
    ///      clean-output path to carry it, so the choice is retain-as-machinery or drop, and
    ///      §1.4 G5 forbids the drop. It still never reaches `StreamEvent::Chunk`: this
    ///      returns `MachineryRecord`s, which travel on the other family entirely.
    ///
    ///      On this wire that means an `assistant` frame's `text` blocks and a
    ///      `text_delta`. The ACP path had one carrier for both (`agent_message_chunk`);
    ///      the native path streams and then repeats, so both are retained and each says
    ///      which it was.
    ///   2. **A `user` frame's `text` block is still dropped** — the ONE deliberate drop
    ///      (§1.2), and it does not become less deliberate for arriving between turns.
    ///
    /// `turn_id` stays `None` and the caller stamps the thread: §1.4 G4, `turn_id: None` is
    /// a first-class state.
    pub fn from_native_between_turn(frame: &Value, session_id: &str, seq: u64) -> Vec<MachineryRecord> {
        let ty = frame.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let orphan = |title: &str, text: &str, body: &Value, seq: u64| {
            let (payload, truncated) = cap_payload(body);
            MachineryRecord {
                machinery_id: new_id("mach"),
                thread_id: String::new(),
                turn_id: None,
                session_id: session_id.to_string(),
                seq,
                at: now_millis(),
                // Deliberately NOT a typed kind. This is the vendor's text arriving where
                // nothing can attribute it, which is precisely what `Unknown` is for —
                // retained verbatim, one dim line, no claim about what it was.
                kind: MachineryKind::Unknown,
                tool_call_id: None,
                status: None,
                title: title.to_string(),
                summary: summarize(text),
                locations: Vec::new(),
                parent_tool_use_id: None,
                internal: false,
                payload,
                truncated,
            }
        };

        if ty == "assistant" {
            // Retain the text blocks that `from_native_event` drops, in place, and let
            // every other block kind go through the ordinary normalizer. Doing it in ONE
            // pass keeps arrival order, which is the lane's only ordering authority.
            let mut out = Vec::new();
            for block in content_blocks(frame) {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    out.push(orphan("assistant:text", text, block, seq + out.len() as u64));
                } else {
                    // The frame's parent travels with the block, so a background worker's call
                    // arriving between turns is still recognizably not the lead's.
                    let one = serde_json::json!({ "type": "assistant", "message": { "content": [block] },
                                                  "parent_tool_use_id": frame.get("parent_tool_use_id") });
                    for r in MachineryRecord::from_native_event(&one, session_id, seq + out.len() as u64) {
                        out.push(r);
                    }
                }
            }
            return out;
        }

        if ty == "stream_event" {
            let ev = frame.get("event").unwrap_or(&Value::Null);
            let is_text_delta = ev.get("type").and_then(|v| v.as_str()) == Some("content_block_delta")
                && ev.get("delta").and_then(|d| d.get("type")).and_then(|v| v.as_str()) == Some("text_delta");
            if is_text_delta {
                let text = ev.get("delta").and_then(|d| d.get("text")).and_then(|v| v.as_str()).unwrap_or("");
                return vec![orphan("text_delta", text, frame, seq)];
            }
        }

        MachineryRecord::from_native_event(frame, session_id, seq)
    }

    /// The marker record for updates the between-turn buffer could not hold.
    ///
    /// A bounded buffer that silently forgets its overflow is the silent drop §1.4 G5
    /// exists to forbid, one level down. So the overflow is itself a record: it says how
    /// many updates were refused and it renders as an ordinary technical row, which means
    /// the hole is visible in the same place the traffic would have been.
    pub fn between_turn_overflow(dropped: u64, session_id: &str, seq: u64) -> MachineryRecord {
        let (payload, truncated) = cap_payload(&serde_json::json!({ "dropped": dropped }));
        MachineryRecord {
            machinery_id: new_id("mach"),
            thread_id: String::new(),
            turn_id: None,
            session_id: session_id.to_string(),
            seq,
            at: now_millis(),
            kind: MachineryKind::Unknown,
            tool_call_id: None,
            status: None,
            title: BETWEEN_TURN_OVERFLOW.to_string(),
            summary: Some(format!(
                "{dropped} update(s) arrived between turns and were not kept"
            )),
            locations: Vec::new(),
            parent_tool_use_id: None,
            internal: false,
            payload,
            truncated,
        }
    }

    /// Stamp the fields only the spine knows (§1.5's `internal` rule included).
    pub fn stamp(mut self, thread_id: &str, turn_id: Option<&str>, internal: bool) -> MachineryRecord {
        self.thread_id = thread_id.to_string();
        self.turn_id = turn_id.map(|s| s.to_string());
        self.internal = internal;
        self
    }

    /// The JSON payload delivered on `rich://machinery`, camelCase for the JS side.
    pub fn event_payload(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// A sink for machinery records. Deliberately a SEPARATE trait from `TurnObserver`
/// (`stream.rs:88`): two families means the default UI's subscription list is the proof
/// that the calm view carries no machinery (§3.3's provable-not-promised test (a)).
pub trait MachineryObserver: Send {
    /// MUST be non-blocking and infallible from the spine's view. A UI that isn't
    /// listening never stalls or fails a turn — and per §2.2, machinery is not truth.
    fn on_machinery(&self, record: &MachineryRecord);
}

// ---- normalization helpers -------------------------------------------------

/// The content blocks of an `assistant` / `user` frame, or an empty slice.
///
/// Both frames wrap them the same way — `message.content: [...]` — measured in `run9`
/// lines 15, 18, 26 and 51.
fn content_blocks(frame: &Value) -> &[Value] {
    frame
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

/// A `tool_result`'s `content`, which the wire carries EITHER as a plain string OR as an
/// array of `{type:"text", text}` blocks. Both were observed; a consumer that handled only
/// one would show a blank line for half the tools.
fn block_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Best available bounded preview of what a tool was ASKED to do.
///
/// The native path's asymmetry against ACP, stated plainly: ACP's `rawOutput` gave the
/// RESULT on the same event as the call, so one record could preview the outcome. Here the
/// call and the result are two frames on two different `id` fields, so the open record
/// previews the ARGUMENTS and the closing `tool_result` record supplies the outcome. §1.4
/// G2's merge is what puts them on one row.
fn tool_input_summary(input: &Value) -> Option<String> {
    for key in ["command", "description", "file_path", "path", "pattern", "query", "prompt", "url"] {
        if let Some(s) = input.get(key).and_then(|v| v.as_str()) {
            if let Some(sum) = summarize(s) {
                return Some(sum);
            }
        }
    }
    if input.is_null() || input.as_object().map(|o| o.is_empty()).unwrap_or(false) {
        return None;
    }
    summarize(&input.to_string())
}

/// §2.4's summary rule, lifted as an IDEA from t3code (`session-logic.ts:1487-1518`) and
/// written from scratch in Rust — no vendored lines, so the open-source license gate
/// (`wiki/open-source-strategy.md`) stays unencumbered (§4.2).
///
/// First non-empty line, capped at 84 chars; `"N lines"` when there is no non-empty line
/// to show. Char-boundary safe.
pub fn summarize(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    match text.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(line) => {
            if line.chars().count() <= SUMMARY_MAX_CHARS {
                Some(line.to_string())
            } else {
                let mut out: String = line.chars().take(SUMMARY_MAX_CHARS).collect();
                out.push('\u{2026}');
                Some(out)
            }
        }
        None => Some(format!("{} lines", text.lines().count())),
    }
}

/// DEVIATION 4 — **`locations` does not exist on this wire.**
///
/// ACP shipped a `locations: [{path, line?}]` array and `machinery.rs` extracted `.path`
/// from it. The native `can_use_tool` for an `Edit` carries the edit INTENT
/// (`{file_path, old_string, new_string, replace_all}`) and **no `locations` at all**
/// (`native-claude-stream-json-2026-08-31/findings.md` §6, `raw/run11`). So the paths are
/// recovered from the tool's own input by the field names the observed tools actually use.
///
/// Named as a LOSS rather than an equivalence: this recovers a path, not the vendor's own
/// statement of which files a call touched, and a tool whose path lives under a key not
/// listed here contributes nothing rather than a guess.
fn input_locations(input: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["file_path", "path", "notebook_path"] {
        if let Some(s) = input.get(key).and_then(|v| v.as_str()) {
            if !s.is_empty() && !out.iter().any(|p| p == s) {
                out.push(s.to_string());
            }
        }
    }
    out
}

/// The title carried by the DERIVED context measurement, and the key
/// [`MachineryRecord::context_usage`] matches on.
///
/// A CONSTANT because the record is built in one place and read in another, and two
/// spellings is how a measurement stops being findable — the same argument
/// [`BETWEEN_TURN_OVERFLOW`] carries.
pub const CONTEXT_USAGE: &str = "context_usage";

/// How much of the model's context window this session has consumed - `{used, size}`.
///
/// **DERIVED on this wire, and the record says so.** ACP handed the pair over on a
/// `usage_update`; the native binary reports the numerator mid-turn on
/// `stream_event/message_delta` (observed monotonic within one turn: 29,342 -> 29,528 ->
/// 29,599 -> 30,113) and the denominator only when a turn ENDS, on
/// `result.modelUsage["claude-sonnet-5"].contextWindow` = **1_000_000**
/// (`native-claude-stream-json-2026-08-31/findings.md` §4 and caveat C3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextUsage {
    /// Tokens consumed so far on this session, as the agent counts them.
    pub used: u64,
    /// The session's context window, as the agent reports it.
    pub size: u64,
}

impl ContextUsage {
    /// `used / size`, or 0.0 when the agent reported a zero window (which would make the
    /// ratio meaningless - a zero denominator is refused rather than divided by).
    pub fn fraction(&self) -> f64 {
        if self.size == 0 {
            0.0
        } else {
            self.used as f64 / self.size as f64
        }
    }
}

/// Apply §2.4's 32 KB per-record payload cap. An over-cap payload is replaced by a
/// char-boundary-safe truncation of its serialized form, as a JSON string, with
/// `truncated: true` — visibly a different shape, so nothing can mistake it for the
/// complete object.
fn cap_payload(v: &Value) -> (Option<Value>, bool) {
    let s = v.to_string();
    if s.len() <= PAYLOAD_MAX_BYTES {
        return (Some(v.clone()), false);
    }
    let mut end = PAYLOAD_MAX_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (Some(Value::String(s[..end].to_string())), true)
}

// ---- the projection (§1.4 G2 + G6) -----------------------------------------

/// Fold an append-only journal into the rows a renderer would show.
///
/// §1.4 G6: the journal is append-only and each record immutable — a `tool_call_update`
/// APPENDS a line; the merge happens HERE, in the projection, never in the file.
///
/// §1.4 G2, and the reason it is load-bearing rather than decorative: 34 of the 58 tool
/// events measured on 2026-08-28 carried **no `status` field at all**, and the OPEN event
/// carries a placeholder title and an empty `rawInput`. So the rule is last-write-wins per
/// field **present** in the update, absent fields untouched. A whole-record overwrite
/// would blank the CEO's status dot on most events and leave him a column of "Terminal".
///
/// A `tool_call_update` for an id we never saw OPENS a record — adapters do reorder.
///
/// Identity and position come from the FIRST record for an id: `machinery_id`, `seq` and
/// `at` are where the tool call *started*, which is where §3.4 renders it in the
/// interleaved stream.
///
/// Ordering is `(turn, seq)` per §1.4 G3 — never the clock. Turns are ordered by first
/// appearance in the journal, which is append order, which is chronological.
pub fn project(records: Vec<MachineryRecord>) -> Vec<MachineryRecord> {
    let mut turn_order: Vec<Option<String>> = Vec::new();
    let mut out: Vec<MachineryRecord> = Vec::new();
    // index into `out` for each merged tool call, keyed by tool_call_id
    let mut by_tool: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for r in records {
        if !turn_order.contains(&r.turn_id) {
            turn_order.push(r.turn_id.clone());
        }
        let mergeable =
            matches!(r.kind, MachineryKind::ToolCall | MachineryKind::Compaction) && r.tool_call_id.is_some();
        if !mergeable {
            out.push(r);
            continue;
        }
        // [`merge_key`], never a second copy of it. A compaction's key carries its TURN,
        // because the key itself is the lease's and a lease outlives a turn: without that, the
        // two compactions measured in `cellT1` (one in turn 3, one in turn 4) would fold into
        // ONE row — turn 4's state on turn 3's row, and one of two real events lost.
        let key = merge_key(&r);
        match by_tool.get(&key) {
            Some(&idx) => merge_into(&mut out[idx], r),
            None => {
                by_tool.insert(key, out.len());
                out.push(r);
            }
        }
    }

    let turn_rank = |t: &Option<String>| turn_order.iter().position(|x| x == t).unwrap_or(usize::MAX);
    out.sort_by(|a, b| turn_rank(&a.turn_id).cmp(&turn_rank(&b.turn_id)).then(a.seq.cmp(&b.seq)));
    out
}

/// Fold the records that carry NO turn — §1.5's between-turn lane.
///
/// The SAME merge rule as [`project`] (§1.4 G2, via [`merge_into`]), and a DIFFERENT order.
/// The difference is measured, not stylistic:
///
/// [`project`] sorts by `(turn, seq)` because `seq` is the one per-TURN counter that text
/// and machinery share (§1.4 G1). A between-turn record has no turn, and its counter is
/// per LEASE — a rotation installs a fresh `NativeClient` whose between-turn counter restarts
/// at 0 — so sorting this lane by `seq` would place a successor's first records before its
/// predecessor's last ones. Journal append order is chronological and is already the
/// authority `project` leans on for turn order (§1.4 G6), so it is the order kept here,
/// and `seq` is carried for identity rather than used for sorting.
///
/// The caller has already excluded `internal: true` — see [`crate::journal::MachineryJournal`].
pub fn project_between_turns(records: Vec<MachineryRecord>) -> Vec<MachineryRecord> {
    let mut out: Vec<MachineryRecord> = Vec::new();
    let mut by_tool: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in records {
        let mergeable = r.kind == MachineryKind::ToolCall && r.tool_call_id.is_some();
        if !mergeable {
            out.push(r);
            continue;
        }
        let key = r.tool_call_id.clone().unwrap();
        match by_tool.get(&key) {
            Some(&idx) => merge_into(&mut out[idx], r),
            None => {
                by_tool.insert(key, out.len());
                out.push(r);
            }
        }
    }
    out
}

/// Last-write-wins per field PRESENT. Absent fields are untouched (§1.4 G2).
///
/// `pub(crate)` so the LIVE upsert path (`live.rs`) merges by the SAME rule the
/// batch projection uses, incrementally, rather than re-implementing it.
pub(crate) fn merge_into(base: &mut MachineryRecord, incoming: MachineryRecord) {
    if !incoming.title.is_empty() {
        base.title = incoming.title;
    }
    if incoming.status.is_some() {
        base.status = incoming.status;
    }
    if incoming.summary.is_some() {
        base.summary = incoming.summary;
    }
    if !incoming.locations.is_empty() {
        base.locations = incoming.locations;
    }
    if incoming.payload.is_some() {
        base.payload = incoming.payload;
        base.truncated = incoming.truncated;
    }
    // Kept when ANY frame of the call said it was nested: a worker's call stays a worker's
    // whichever of its frames arrives last (Output side panel PRD §4.1 (a)).
    if incoming.parent_tool_use_id.is_some() {
        base.parent_tool_use_id = incoming.parent_tool_use_id;
    }
    // machinery_id / seq / at / turn_id / thread_id / session_id / internal keep the
    // OPENING record's values — see the `project` doc.
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The tool row as it OPENS, verbatim from
    /// `docs/verification/native-claude-stream-json-2026-08-31/raw/run9-rust-driven.jsonl:5`.
    fn open_write() -> Value {
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":"toolu_A","name":"Write","input":{},
                                "caller":{"type":"direct"}}},
               "session_id":"sess","parent_tool_use_id":null,"uuid":"u1"})
    }

    /// The same tool with its arguments COMPLETE — `run9-rust-driven.jsonl:15`.
    fn assistant_tool_use() -> Value {
        json!({"type":"assistant","message":{"role":"assistant","content":[
                 {"type":"tool_use","id":"toolu_A","name":"Write",
                  "input":{"file_path":"/private/tmp/claude-501/rust-probe-out.txt","content":"RUSTOK"},
                  "caller":{"type":"direct"}}]},
               "session_id":"sess","uuid":"u2"})
    }

    /// The tool row as it CLOSES — `run9-rust-driven.jsonl:18`.
    fn tool_result_ok() -> Value {
        json!({"type":"user","message":{"role":"user","content":[
                 {"tool_use_id":"toolu_A","type":"tool_result",
                  "content":"File created successfully at: /private/tmp/claude-501/rust-probe-out.txt"}]},
               "session_id":"sess","uuid":"u3",
               "tool_use_result":{"type":"create","filePath":"/private/tmp/claude-501/rust-probe-out.txt",
                                  "content":"RUSTOK"}})
    }

    fn one(frame: &Value, seq: u64) -> MachineryRecord {
        let mut v = MachineryRecord::from_native_event(frame, "s", seq);
        assert_eq!(v.len(), 1, "expected exactly one record from {frame}");
        v.remove(0)
    }

    // ---- the six declared drops ------------------------------------------------------

    #[test]
    fn the_clean_output_text_path_is_not_machinery() {
        // DROP 1 and DROP 2 — the ported half of
        // `agent_message_chunk_and_user_message_chunk_are_not_machinery`. Streamed text and
        // its whole-message repeat both reach the CEO as `StreamEvent::Chunk`; a machinery
        // copy would be a second source of truth for the reply.
        let delta = json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,
                           "delta":{"type":"text_delta","text":"T"}}});
        let whole = json!({"type":"assistant","message":{"role":"assistant",
                           "content":[{"type":"text","text":"TURN1-OK"}]}});
        assert!(MachineryRecord::from_native_event(&delta, "s", 0).is_empty());
        assert!(MachineryRecord::from_native_event(&whole, "s", 0).is_empty());
    }

    #[test]
    fn a_user_text_block_is_still_the_one_deliberate_drop() {
        // DROP 3 — the other half of the same original test, and §1.2's ONE deliberate drop
        // carried across the wire change. On the native wire it guards the injected
        // interrupt marker rather than an echoed prompt (measured: `run9` never echoes our
        // prompt), and the reason is identical — the ledger and the stop control already
        // hold those facts, fsync'd, exactly once.
        let marker = json!({"type":"user","message":{"role":"user",
                            "content":[{"type":"text","text":"[Request interrupted by user]"}]}});
        assert!(MachineryRecord::from_native_event(&marker, "s", 0).is_empty());
    }

    #[test]
    fn the_streaming_envelope_and_the_fragments_are_dropped_and_the_list_is_closed() {
        // DROPS 4, 5 and 6, pinned as a SET rather than one at a time: this test is the
        // enumeration the module doc claims, so a seventh drop added later fails here.
        for f in [
            json!({"type":"stream_event","event":{"type":"message_start","message":{}}}),
            json!({"type":"stream_event","event":{"type":"message_stop"}}),
            json!({"type":"stream_event","event":{"type":"content_block_stop","index":0}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"text","text":""}}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"thinking","thinking":"","signature":""}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta",
                   "delta":{"type":"input_json_delta","partial_json":"{\"fi"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta",
                   "delta":{"type":"thinking_delta","thinking":"","estimated_tokens":50}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta",
                   "delta":{"type":"signature_delta","signature":"EvgCC"}}}),
            json!({"type":"result","subtype":"success","stop_reason":"end_turn"}),
        ] {
            assert!(
                MachineryRecord::from_native_event(&f, "s", 0).is_empty(),
                "declared drop must produce no record: {f}"
            );
        }
    }

    #[test]
    fn untyped_vendor_frames_are_retained_as_unknown_not_dropped() {
        // The ported `phase_two_kinds_are_retained_as_unknown_not_dropped`, against the
        // frames this wire actually carries. A silent drop is what §1.4 G5 forbids, and the
        // vendor frame type must survive as the dim line's text.
        let cases = [
            (json!({"type":"system","subtype":"init","model":"claude-sonnet-5"}), "system:init"),
            (json!({"type":"system","subtype":"status","status":"requesting"}), "system:status"),
            (json!({"type":"system","subtype":"thinking_tokens","estimated_tokens":50}), "system:thinking_tokens"),
            (json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}), "rate_limit_event"),
            (json!({"type":"control_response","response":{"subtype":"success"}}), "control_response"),
            (json!({"type":"stream_event","event":{"type":"message_delta","usage":{"input_tokens":2}}}),
             "stream_event:message_delta"),
        ];
        for (f, title) in cases {
            let r = one(&f, 3);
            assert_eq!(r.kind, MachineryKind::Unknown);
            assert_eq!(r.title, title, "the vendor frame must survive as the dim line's text");
            assert_eq!(r.payload.as_ref().unwrap()["type"], f["type"]);
            assert_eq!(r.seq, 3);
        }
    }

    // ---- the derived context measurement ---------------------------------------------

    fn usage_obj() -> Value {
        // Verbatim from `run9-rust-driven.jsonl:19`'s `message_delta`.
        json!({"input_tokens":2,"cache_creation_input_tokens":3603,"cache_read_input_tokens":25737,
               "output_tokens":95})
    }

    #[test]
    fn a_derived_context_usage_record_yields_the_measured_used_and_size_pair() {
        // 2 + 3603 + 25737 = 29_342 — the first of the four monotonic numerators
        // `findings.md` §4 reports, re-derived here rather than trusted.
        let r = MachineryRecord::from_context_usage(29_342, 1_000_000, &usage_obj(), "sess", 8);
        let usage = r.context_usage().expect("a derived record carries a measurement");
        assert_eq!(usage.used, 29_342);
        assert_eq!(usage.size, 1_000_000);
        // 29342 / 1000000 = 0.029342 — re-derived here rather than trusted.
        assert!((usage.fraction() - 0.029_342).abs() < 1e-9, "got {}", usage.fraction());
        assert_eq!(r.title, CONTEXT_USAGE);
        assert_eq!(r.payload.as_ref().unwrap()["derived"], json!(true), "the record states that it is derived");
        assert_eq!(
            r.payload.as_ref().unwrap()["usage"]["cache_read_input_tokens"],
            json!(25737),
            "the bytes the number was summed from are retained with it"
        );
    }

    #[test]
    fn an_extra_vendor_field_on_the_usage_object_still_yields_the_measurement() {
        // The ported `the_rate_limit_meta_variant_still_yields_the_measurement`: the wire is
        // the vendor's and open, so an added field must not cost us the numbers.
        let mut usage = usage_obj();
        usage["service_tier"] = json!("standard");
        usage["iterations"] = json!([{"input_tokens": 2}]);
        let r = MachineryRecord::from_context_usage(30_113, 1_000_000, &usage, "sess", 30);
        assert_eq!(r.context_usage().map(|u| (u.used, u.size)), Some((30_113, 1_000_000)));
    }

    #[test]
    fn only_a_context_usage_record_is_a_measurement() {
        // Every other machinery record must return None - a tool call is not a token count,
        // and a watermark that read one would be worse than the estimate it replaced.
        for f in [
            open_write(),
            json!({"type":"system","subtype":"init","used":99,"size":100}),
            json!({"type":"stream_event","event":{"type":"message_delta",
                   "usage":{"input_tokens":99,"cache_read_input_tokens":1}}}),
        ] {
            let r = one(&f, 0);
            assert_eq!(r.context_usage(), None, "non-usage frame must carry no measurement: {f}");
        }
        // A `system` frame carrying used/size is not hypothetical caution: the wire is the
        // vendor's and open, so the KIND, not the field names, is what makes a number a
        // measurement.
    }

    #[test]
    fn half_a_measurement_is_no_measurement() {
        for broken in [
            json!({"kind":CONTEXT_USAGE,"used":30477}),
            json!({"kind":CONTEXT_USAGE,"size":1000000}),
            json!({"kind":CONTEXT_USAGE}),
            json!({"kind":CONTEXT_USAGE,"used":"30477","size":"1000000"}),
        ] {
            let mut r = MachineryRecord::from_context_usage(1, 2, &Value::Null, "s", 0);
            r.payload = Some(broken.clone());
            assert_eq!(r.context_usage(), None, "must refuse a partial/untyped pair: {broken}");
        }
    }

    #[test]
    fn a_zero_window_is_refused_rather_than_divided_by() {
        let r = MachineryRecord::from_context_usage(500, 0, &Value::Null, "s", 0);
        let usage = r.context_usage().unwrap();
        assert_eq!(usage.fraction(), 0.0, "a zero denominator must not become inf/NaN");
    }

    #[test]
    fn an_evicted_payload_reports_no_measurement_rather_than_a_stale_one() {
        // Tier B is evictable (journal.rs). A record read back after eviction has
        // payload: None, and the honest answer there is "gone", not a remembered number.
        let mut r = MachineryRecord::from_context_usage(29_342, 1_000_000, &usage_obj(), "s", 0);
        r.payload = None;
        assert_eq!(r.context_usage(), None);
    }

    #[test]
    fn a_truncated_payload_reports_no_measurement() {
        let mut r = MachineryRecord::from_context_usage(29_342, 1_000_000, &usage_obj(), "s", 0);
        r.truncated = true;
        assert_eq!(r.context_usage(), None, "half a number is not a measurement");
    }

    // ---- tool rows: open, merge, close -----------------------------------------------

    #[test]
    fn an_open_tool_call_carries_the_tool_name_and_no_arguments() {
        // The measured reality that makes the merge mandatory (deviation 2): the row opens
        // on the STREAM, before `input` exists.
        let r = one(&open_write(), 7);
        assert_eq!(r.kind, MachineryKind::ToolCall);
        assert_eq!(r.title, "Write", "the real tool name, direct — no `_meta` indirection");
        assert_eq!(r.tool_call_id.as_deref(), Some("toolu_A"));
        assert_eq!(r.status, Some(ToolStatus::Pending));
        assert_eq!(r.summary, None, "`input: {{}}` on the open frame — nothing to preview yet");
        assert_eq!(r.seq, 7);
        assert_eq!(r.session_id, "s");
    }

    #[test]
    fn merge_keeps_the_arguments_and_the_outcome_and_never_blanks_a_status() {
        // The exact four-frame sequence measured for `toolu_015USp34…` in run9: open on the
        // stream, arguments on the whole message, a heartbeat, then the result.
        let frames = vec![
            open_write(),
            assistant_tool_use(),
            json!({"type":"tool_progress","tool_use_id":"toolu_A-heartbeat-0","tool_name":"Write",
                   "parent_tool_use_id":"toolu_A","elapsed_time_seconds":30,"heartbeat":true}),
            tool_result_ok(),
        ];
        let mut recs = Vec::new();
        for (i, f) in frames.iter().enumerate() {
            for r in MachineryRecord::from_native_event(f, "s", i as u64) {
                recs.push(r.stamp("thr", Some("t1"), false));
            }
        }
        let opening_id = recs[0].machinery_id.clone();
        let rows = project(recs);
        assert_eq!(rows.len(), 1, "four wire frames, ONE row — never a second row (G2)");
        let row = &rows[0];
        assert_eq!(
            row.title, "/private/tmp/claude-501/rust-probe-out.txt",
            "the human line from the ARGUMENTS frame wins over the open frame's tool name — \
             and survives the merge, which keeps the last payload and would otherwise lose it"
        );
        assert_eq!(row.status, Some(ToolStatus::Completed));
        assert_eq!(
            row.summary.as_deref(),
            Some("File created successfully at: /private/tmp/claude-501/rust-probe-out.txt"),
            "the outcome, which arrives on the CLOSING frame, wins over the arguments"
        );
        assert_eq!(row.locations, vec!["/private/tmp/claude-501/rust-probe-out.txt".to_string()]);
        assert_eq!(row.seq, 0, "position is where the call STARTED");
        assert_eq!(row.machinery_id, opening_id, "identity is the opening record's");
    }

    #[test]
    fn a_status_less_frame_does_not_erase_a_status_already_seen() {
        // The failure a naive whole-record overwrite would produce, pinned as a test. The
        // `assistant` frame carries the complete arguments and NO status.
        let mut recs = Vec::new();
        for (i, f) in [open_write(), assistant_tool_use()].iter().enumerate() {
            recs.extend(MachineryRecord::from_native_event(f, "s", i as u64));
        }
        let rows = project(recs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, Some(ToolStatus::Pending));
        assert_eq!(rows[0].title, "/private/tmp/claude-501/rust-probe-out.txt");
        assert_eq!(rows[0].summary, None, "the summary slot belongs to the outcome, which has not arrived");
    }

    #[test]
    fn a_tool_progress_heartbeat_keys_on_parent_tool_use_id_and_never_on_its_own() {
        // DEVIATION 1, and the trap `native-claude-tool-status-2026-08-31/findings.md` says
        // would have cost a day: the frame's own `tool_use_id` is a synthetic
        // `<real-id>-heartbeat-<n>` that matches NO row anywhere. A consumer keying on it
        // silently updates nothing. Verbatim frame from `raw/run13-longtool-bash-ticking.jsonl:22`.
        let hb = json!({"type":"tool_progress",
                        "tool_use_id":"toolu_01KJdbWL6beA2nhNoi3QdpkW-heartbeat-0",
                        "tool_name":"Bash",
                        "parent_tool_use_id":"toolu_01KJdbWL6beA2nhNoi3QdpkW",
                        "elapsed_time_seconds":30,"heartbeat":true,
                        "session_id":"83eb151a","uuid":"3f6352f0"});
        let r = one(&hb, 0);
        assert_eq!(
            r.tool_call_id.as_deref(),
            Some("toolu_01KJdbWL6beA2nhNoi3QdpkW"),
            "the row this belongs to is parent_tool_use_id, NEVER tool_use_id"
        );
        assert_ne!(r.tool_call_id.as_deref(), Some("toolu_01KJdbWL6beA2nhNoi3QdpkW-heartbeat-0"));
        assert_eq!(r.status, Some(ToolStatus::InProgress));
        assert!(!r.status.as_ref().unwrap().is_terminal());
        assert_eq!(r.summary.as_deref(), Some("running 30s"));
        assert_eq!(r.title, "", "empty means absent, which is what makes the merge non-destructive");
    }

    #[test]
    fn a_tool_result_for_an_unseen_id_opens_a_record() {
        // "adapters do reorder" (§1.4 G2), and a lease that was rotated mid-tool would show
        // exactly this.
        let orphan = json!({"type":"user","message":{"role":"user","content":[
                             {"tool_use_id":"orphan","type":"tool_result","content":"x"}]}});
        let rows = project(MachineryRecord::from_native_event(&orphan, "s", 0));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, Some(ToolStatus::Completed));
    }

    #[test]
    fn failed_is_carried_through_and_is_terminal() {
        // Status is a POSITION on this wire, not a string (deviation 3): `is_error` decides.
        let f = json!({"type":"user","message":{"role":"user","content":[
                        {"tool_use_id":"t","type":"tool_result","is_error":true,
                         "content":"Exit code 1\ncat: /Users/example/nope: No such file or directory"}]}});
        let r = one(&f, 0);
        assert_eq!(r.status, Some(ToolStatus::Failed));
        assert!(r.status.as_ref().unwrap().is_terminal());
        assert_eq!(r.summary.as_deref(), Some("Exit code 1"));
    }

    #[test]
    fn a_tool_result_content_array_is_read_as_well_as_a_bare_string() {
        // Both shapes are on the wire. A consumer that handled one would show a blank line
        // for half the tools.
        let f = json!({"type":"user","message":{"role":"user","content":[
                        {"tool_use_id":"t","type":"tool_result",
                         "content":[{"type":"text","text":"1.0.0"}]}]}});
        assert_eq!(one(&f, 0).summary.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn an_unrecognised_wire_status_is_retained_not_dropped() {
        // §1.4 G5's rule one level down, applied to a field. There is no producer for this
        // on the native wire today — status is a position, not a string — so the test drives
        // the type directly rather than inventing a frame the binary does not emit.
        let s = ToolStatus::from_wire("quiesced");
        assert_eq!(s, ToolStatus::Other("quiesced".into()));
        assert!(!s.is_terminal(), "an unknown status is not a claim of completion");
        assert_eq!(s.as_str(), "quiesced", "the wire value is retained verbatim");
    }

    #[test]
    fn locations_are_recovered_from_the_tools_own_input() {
        // DEVIATION 4: there is no `locations` array on this wire, so paths come from the
        // tool's input. Named as a loss, not an equivalence.
        let f = json!({"type":"assistant","message":{"role":"assistant","content":[
                        {"type":"tool_use","id":"t","name":"Edit",
                         "input":{"file_path":"/tmp/a.txt","old_string":"a","new_string":"b"}}]}});
        assert_eq!(one(&f, 0).locations, vec!["/tmp/a.txt".to_string()]);
        // A tool whose path lives under a key we do not know contributes NOTHING rather
        // than a guess.
        let unknown = json!({"type":"assistant","message":{"role":"assistant","content":[
                              {"type":"tool_use","id":"t","name":"Weird",
                               "input":{"target_document":"/tmp/b.txt"}}]}});
        assert!(one(&unknown, 0).locations.is_empty());
    }

    #[test]
    fn thinking_blocks_route_to_thought() {
        // Seven were observed across the native captures, EVERY ONE with empty text and a
        // signature only. The route exists so that the day readable thinking arrives there
        // is no hole — the same reason the ACP path carried it unobserved.
        let f = json!({"type":"assistant","message":{"role":"assistant","content":[
                        {"type":"thinking","thinking":"Let me check the frame math.\nsecond line",
                         "signature":"EvgCC"}]}});
        let r = one(&f, 2);
        assert_eq!(r.kind, MachineryKind::Thought);
        assert_eq!(r.title, "thinking");
        assert_eq!(r.summary.as_deref(), Some("Let me check the frame math."));
        // And the observed shape: presence only.
        let empty = json!({"type":"assistant","message":{"role":"assistant","content":[
                            {"type":"thinking","thinking":"","signature":"EvgCC"}]}});
        assert_eq!(one(&empty, 0).summary, None, "a signature is not readable thinking");
    }

    #[test]
    fn one_frame_can_be_several_records_and_they_number_consecutively() {
        // The structural change the wire forces: `message.content` is an ARRAY. Collapsing a
        // thinking block and a tool call into one record would invent a fact, and the
        // shared per-turn counter (§1.4 G1) must still number DELIVERED items.
        let f = json!({"type":"assistant","message":{"role":"assistant","content":[
                        {"type":"thinking","thinking":"weighing it","signature":"s"},
                        {"type":"text","text":"dropped — clean output"},
                        {"type":"tool_use","id":"t","name":"Bash","input":{"command":"ls"}}]}});
        let recs = MachineryRecord::from_native_event(&f, "s", 5);
        assert_eq!(recs.len(), 2, "three blocks, one of them a declared drop");
        assert_eq!(recs.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![5, 6]);
        assert_eq!(recs[0].kind, MachineryKind::Thought);
        assert_eq!(recs[1].kind, MachineryKind::ToolCall);
        assert_eq!(recs[1].title, "ls");
    }

    #[test]
    fn permission_requests_record_the_decision_without_inventing_approval_origin() {
        // Verbatim `request` from `run9-rust-driven.jsonl:17`.
        let request = json!({
            "subtype":"can_use_tool","tool_name":"Write","display_name":"Write",
            "input":{"file_path":"/private/tmp/claude-501/rust-probe-out.txt","content":"RUSTOK"},
            "description":"/private/tmp/claude-501/rust-probe-out.txt",
            "permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],
            "decision_reason":"Path is outside allowed working directories",
            "decision_reason_type":"workingDir",
            "tool_use_id":"toolu_015USp34XWJGrGpgNY6TLvbV"});
        let r = MachineryRecord::from_permission_request(&request, "allow", "sess", 4);
        assert_eq!(r.kind, MachineryKind::PermissionRequested);
        assert_eq!(r.title, "/private/tmp/claude-501/rust-probe-out.txt");
        assert_eq!(
            r.tool_call_id.as_deref(),
            Some("toolu_015USp34XWJGrGpgNY6TLvbV"),
            "links back to its tool call"
        );
        assert!(r.payload.as_ref().unwrap().get("auto").is_none());
        let denied = MachineryRecord::from_permission_request(&request, "deny", "sess", 5);
        assert_eq!(denied.summary.as_deref(), Some("permission decision: deny"));
        assert_eq!(r.payload.as_ref().unwrap()["chosen"], json!("allow"));
        // Richer than the shape it replaces, and the extra fact is RETAINED, not discarded.
        assert_eq!(
            r.payload.as_ref().unwrap()["decisionReason"],
            json!("Path is outside allowed working directories")
        );
        assert_eq!(r.locations, vec!["/private/tmp/claude-501/rust-probe-out.txt".to_string()]);
    }

    #[test]
    fn client_fs_calls_route_with_their_path() {
        // No producer on the native wire — the CLI does its own file IO and was observed not
        // to ask. The kind survives because stored journals already contain it.
        let r = MachineryRecord::from_client_fs_call("fs/read_text_file", &json!({"path":"/tmp/x"}), "s", 1);
        assert_eq!(r.kind, MachineryKind::ClientFsCall);
        assert_eq!(r.locations, vec!["/tmp/x".to_string()]);
    }

    // ---- the SessionMeta identity key -------------------------------------------------

    #[test]
    fn the_meta_identity_key_ignores_the_per_frame_uuid() {
        // MEASURED, and the whole reason `meta_identity` exists: the four `system/init`
        // frames in run9 differ ONLY by `uuid`, so a verbatim-equality slot would suppress
        // zero of them and "last value wins" would be dead code that looked alive.
        let a = json!({"type":"system","subtype":"init","model":"claude-sonnet-5",
                       "session_id":"sess","uuid":"acbe3949a0de"});
        let b = json!({"type":"system","subtype":"init","model":"claude-sonnet-5",
                       "session_id":"sess","uuid":"43f5fdcdeeae"});
        assert_ne!(a, b, "the frames really are different bytes");
        assert_eq!(meta_identity(&a), meta_identity(&b), "and they say the same thing");
        // A frame that says something DIFFERENT is still different.
        let c = json!({"type":"system","subtype":"init","model":"claude-opus-5",
                       "session_id":"sess","uuid":"acbe3949a0de"});
        assert_ne!(meta_identity(&a), meta_identity(&c));
    }

    #[test]
    fn the_session_meta_family_is_the_repeating_per_turn_traffic() {
        assert!(is_session_meta("system:init"));
        assert!(is_session_meta("system:status"));
        assert!(is_session_meta("rate_limit_event"));
        assert!(!is_session_meta("system:thinking_tokens"), "an estimate is a new statement each time");
        assert!(!is_session_meta("tool_call"));
    }

    // ---- unchanged by the wire change --------------------------------------------------

    #[test]
    fn summary_takes_the_first_non_empty_line_capped_at_84_chars() {
        assert_eq!(summarize("\n\n  hello  \nworld"), Some("hello".to_string()));
        let long = "x".repeat(200);
        let s = summarize(&long).unwrap();
        assert_eq!(s.chars().count(), SUMMARY_MAX_CHARS + 1, "84 chars plus the ellipsis");
        assert!(s.ends_with('\u{2026}'));
        assert_eq!(summarize("\n\n\n"), Some("3 lines".to_string()));
        assert_eq!(summarize(""), None);
    }

    #[test]
    fn summary_truncation_is_char_boundary_safe() {
        // `&s[..84]` would panic mid-codepoint here — the trap ledger.rs:628-638 warns about.
        let s = summarize(&"é".repeat(200)).unwrap();
        assert_eq!(s.chars().count(), SUMMARY_MAX_CHARS + 1);
    }

    #[test]
    fn an_over_cap_payload_is_truncated_at_a_char_boundary_and_flagged() {
        let big = json!({"type":"user","message":{"role":"user","content":[
                          {"tool_use_id":"t","type":"tool_result","content":"é".repeat(40_000)}]}});
        let r = one(&big, 0);
        assert!(r.truncated);
        let held = r.payload.unwrap();
        let s = held.as_str().expect("an over-cap payload becomes a string, visibly not the object");
        assert!(s.len() <= PAYLOAD_MAX_BYTES);
        // The record itself still renders: status and summary survive the cap.
        assert!(r.summary.is_some());
        assert_eq!(r.status, Some(ToolStatus::Completed));
    }

    #[test]
    fn a_record_round_trips_through_json_in_camel_case() {
        let r = one(&open_write(), 1).stamp("thr_1", Some("turn_1"), true);
        let v = r.event_payload();
        assert_eq!(v["machineryId"], json!(r.machinery_id));
        assert_eq!(v["threadId"], json!("thr_1"));
        assert_eq!(v["turnId"], json!("turn_1"));
        assert_eq!(v["toolCallId"], json!("toolu_A"));
        assert_eq!(v["kind"], json!("tool_call"));
        assert_eq!(v["status"], json!("pending"));
        assert_eq!(v["internal"], json!(true));
        let back: MachineryRecord = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn ordering_is_by_turn_then_seq_never_by_the_clock() {
        // Same `at` on every record — millisecond timestamps collide inside a streaming
        // turn (§1.4 G3, util.rs:5-8). Only (turn, seq) may decide order.
        let mk = |turn: Option<&str>, seq: u64| {
            let f = json!({"type":"assistant","message":{"role":"assistant","content":[
                            {"type":"thinking","thinking":"t","signature":"s"}]}});
            let mut r = one(&f, seq).stamp("thr", turn, false);
            r.at = 1_700_000_000_000;
            r
        };
        let rows = project(vec![mk(Some("t1"), 2), mk(Some("t2"), 0), mk(Some("t1"), 0), mk(None, 9)]);
        let got: Vec<_> = rows.iter().map(|r| (r.turn_id.clone(), r.seq)).collect();
        assert_eq!(
            got,
            vec![
                (Some("t1".to_string()), 0),
                (Some("t1".to_string()), 2),
                (Some("t2".to_string()), 0),
                (None, 9),
            ]
        );
    }

    // ---- the between-turn variant ------------------------------------------------------

    #[test]
    fn between_turn_text_is_retained_as_machinery_and_never_as_a_chunk() {
        // In a turn this is the clean-output path and has no machinery record. Between turns
        // there is no turn to attach it to, so §1.4 G5 says retain rather than drop — and it
        // travels on the machinery family, never `StreamEvent::Chunk`.
        let whole = json!({"type":"assistant","message":{"role":"assistant",
                           "content":[{"type":"text","text":"orphan"}]}});
        let delta = json!({"type":"stream_event","event":{"type":"content_block_delta",
                           "delta":{"type":"text_delta","text":"orphan"}}});
        for (f, title) in [(whole, "assistant:text"), (delta, "text_delta")] {
            let recs = MachineryRecord::from_native_between_turn(&f, "s", 0);
            assert_eq!(recs.len(), 1);
            assert_eq!(recs[0].kind, MachineryKind::Unknown);
            assert_eq!(recs[0].title, title);
            assert_eq!(recs[0].summary.as_deref(), Some("orphan"));
        }
    }

    #[test]
    fn a_user_text_block_is_dropped_between_turns_too() {
        let marker = json!({"type":"user","message":{"role":"user",
                            "content":[{"type":"text","text":"[Request interrupted by user]"}]}});
        assert!(MachineryRecord::from_native_between_turn(&marker, "s", 0).is_empty());
    }

    #[test]
    fn a_mixed_assistant_frame_keeps_arrival_order_between_turns() {
        // The one path where retained text and ordinary machinery interleave. Order is the
        // lane's only ordering authority, so it must survive the split.
        let f = json!({"type":"assistant","message":{"role":"assistant","content":[
                        {"type":"text","text":"said this"},
                        {"type":"tool_use","id":"t","name":"Bash","input":{"command":"ls"}}]}});
        let recs = MachineryRecord::from_native_between_turn(&f, "s", 0);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].title, "assistant:text");
        assert_eq!(recs[1].kind, MachineryKind::ToolCall);
        assert_eq!(recs.iter().map(|r| r.seq).collect::<Vec<_>>(), vec![0, 1]);
    }

    // =======================================================================================
    // THE COMPACTION PAUSE — every frame below is VERBATIM from a real `claude` 2.1.263
    // stream, `docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl`, with only
    // the uuids shortened. Nothing here is a frame shape reasoned about.
    // =======================================================================================

    /// `cellT1.jsonl` line 32 — the announcement, 13 ms after the prompt was accepted.
    fn compact_start() -> Value {
        json!({"type":"system","subtype":"status","status":"compacting",
               "session_id":"0fee2d41","uuid":"6bb40a60"})
    }

    /// `cellT1.jsonl` line 33 — the SAME frame again 30.000 s later, differing only by uuid.
    fn compact_heartbeat() -> Value {
        json!({"type":"system","subtype":"status","status":"compacting",
               "session_id":"0fee2d41","uuid":"818ad359"})
    }

    /// `cellT1.jsonl` line 34 — the ending, at the same millisecond the silence stops.
    fn compact_success() -> Value {
        json!({"type":"system","subtype":"status","status":Value::Null,"compact_result":"success",
               "session_id":"0fee2d41","uuid":"9e1cfd6b"})
    }

    /// `cellT1.jsonl` line 4 — the ending of an attempt that was abandoned 1 ms after it
    /// started, which is what a 2% threshold override does to a short conversation.
    fn compact_failed() -> Value {
        json!({"type":"system","subtype":"status","status":Value::Null,"compact_result":"failed",
               "compact_error":"too_few_groups","session_id":"0fee2d41","uuid":"c27400bb"})
    }

    #[test]
    fn the_frame_that_announces_a_compaction_is_the_status_frame_not_the_boundary() {
        assert_eq!(compaction_phase(&compact_start()), Some(CompactionPhase::Started));
        assert_eq!(compaction_phase(&compact_heartbeat()), Some(CompactionPhase::Started));
        assert_eq!(compaction_phase(&compact_success()), Some(CompactionPhase::MadeRoom));
        assert_eq!(compaction_phase(&compact_failed()), Some(CompactionPhase::KeptGoing));

        // `compact_boundary` REPORTS the pause once it is over — measured 43611 ms after the
        // `compacting` frame in the same run — so it can never drive a live label and is
        // deliberately left as an untyped technical record. Verbatim, `cellT1.jsonl` line 35.
        let boundary = json!({"type":"system","subtype":"compact_boundary",
                              "compact_metadata":{"trigger":"auto","pre_tokens":71895,
                                                  "post_tokens":14640,"duration_ms":43611},
                              "session_id":"0fee2d41","uuid":"99e6797c"});
        assert_eq!(compaction_phase(&boundary), None);
        let recs = MachineryRecord::from_native_event(&boundary, "sess", 0);
        assert_eq!(recs[0].kind, MachineryKind::Unknown, "the boundary stays technical");
        assert_eq!(recs[0].title, "system:compact_boundary");

        // `requesting` arrives 3 ms before `compacting` on EVERY turn, including the two in
        // `cellT1` that never compacted. It announces nothing and must not be read as a pause.
        let requesting = json!({"type":"system","subtype":"status","status":"requesting",
                                "session_id":"0fee2d41","uuid":"176086ad"});
        assert_eq!(compaction_phase(&requesting), None);
        assert_eq!(MachineryRecord::from_native_event(&requesting, "sess", 0)[0].kind, MachineryKind::Unknown);
    }

    #[test]
    fn a_compaction_record_keeps_the_frames_own_title_so_the_lane_and_the_record_agree() {
        let rec = &MachineryRecord::from_native_event(&compact_start(), "sess", 7)[0];
        assert_eq!(rec.kind, MachineryKind::Compaction);
        assert_eq!(rec.status, Some(ToolStatus::InProgress));
        assert_eq!(rec.seq, 7, "typing a frame must not change what position it takes");
        // `frame_title` is the ONE place the between-turn lane and the record agree about what
        // a frame is called, and `is_session_meta` reads that string. Typing the frame must
        // not fork that answer.
        assert_eq!(rec.title, frame_title(&compact_start()));
        assert!(is_session_meta(&rec.title));
        // The vendor's own bytes survive, so technical mode still has the raw frame.
        assert_eq!(rec.payload.as_ref().unwrap()["status"], "compacting");
    }

    #[test]
    fn the_vendors_failure_is_kept_on_the_record_even_though_the_ceo_is_shown_a_finished_pause() {
        let rec = &MachineryRecord::from_native_event(&compact_failed(), "sess", 0)[0];
        assert_eq!(rec.status, Some(ToolStatus::Failed), "the record tells the truth about the attempt");
        assert_eq!(rec.payload.as_ref().unwrap()["compact_error"], "too_few_groups");
    }

    #[test]
    fn one_compaction_is_one_row_however_many_heartbeats_it_takes() {
        // The exact frame sequence of `cellT1` turn 3: announcement, one 30-second heartbeat,
        // ending. Three frames, one row, and the row's identity is the announcement's.
        let rows = project(vec![
            MachineryRecord::from_native_event(&compact_start(), "sess", 0).remove(0).stamp("thr", Some("t3"), false),
            MachineryRecord::from_native_event(&compact_heartbeat(), "sess", 1)
                .remove(0)
                .stamp("thr", Some("t3"), false),
            MachineryRecord::from_native_event(&compact_success(), "sess", 2).remove(0).stamp("thr", Some("t3"), false),
        ]);
        assert_eq!(rows.len(), 1, "three frames about one pause are one row, not three");
        assert_eq!(rows[0].seq, 0, "the row sits where the pause STARTED");
        assert_eq!(rows[0].status, Some(ToolStatus::Completed));
    }

    #[test]
    fn two_compactions_in_two_turns_stay_two_rows() {
        // MEASURED: `cellT1` compacted once in turn 3 and once in turn 4. The wire gives the
        // two spans no correlation id and no distinguishing field — only the turn separates
        // them — so a key that ignored the turn would report one event where there were two.
        let rows = project(vec![
            MachineryRecord::from_native_event(&compact_start(), "sess", 0).remove(0).stamp("thr", Some("t3"), false),
            MachineryRecord::from_native_event(&compact_success(), "sess", 1).remove(0).stamp("thr", Some("t3"), false),
            MachineryRecord::from_native_event(&compact_start(), "sess", 0).remove(0).stamp("thr", Some("t4"), false),
            MachineryRecord::from_native_event(&compact_success(), "sess", 1).remove(0).stamp("thr", Some("t4"), false),
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].turn_id.as_deref(), Some("t3"));
        assert_eq!(rows[1].turn_id.as_deref(), Some("t4"));
    }

    #[test]
    fn a_compaction_never_merges_with_a_tool_call() {
        // Different kinds, different keys — a `compaction:` key cannot collide with a vendor
        // tool id, and the merge gate checks the kind as well as the key.
        let rows = project(vec![
            tool_open_rec("toolu_A", "Bash"),
            MachineryRecord::from_native_event(&compact_start(), "sess", 1).remove(0).stamp("thr", Some("t3"), false),
        ]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, MachineryKind::ToolCall);
        assert_eq!(rows[1].kind, MachineryKind::Compaction);
        assert!(compaction_key("sess").starts_with("compaction:"));
    }

    fn tool_open_rec(id: &str, name: &str) -> MachineryRecord {
        MachineryRecord::from_native_event(
            &json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                    "content_block":{"type":"tool_use","id":id,"name":name,"input":{}}}}),
            "sess",
            0,
        )
        .remove(0)
        .stamp("thr", Some("t3"), false)
    }

    // ---- whose call it was: parent_tool_use_id (Output side panel PRD §4.1 (a), S2) ------

    /// `frame` as a WORKER's: the same three frames nested under the lead's `Agent` call,
    /// `parent_tool_use_id` on every one of them, the `user` frame included — the shape the
    /// captured wire shows (`docs/verification/claude-md-sentinel-2026-09-06/README.md`, a
    /// `user` frame carrying a `tool_result` and `"parent_tool_use_id":"toolu_…"`).
    fn nested(mut frame: Value) -> Value {
        frame["parent_tool_use_id"] = json!("toolu_AGENT");
        frame
    }

    fn merged(frames: &[Value]) -> MachineryRecord {
        let mut records = frames.iter().enumerate().map(|(i, f)| one(f, i as u64));
        let mut base = records.next().unwrap();
        for r in records {
            merge_into(&mut base, r);
        }
        base
    }

    #[test]
    fn machinery_a_lead_write_as_three_frames_merges_with_no_parent() {
        let call = merged(&[open_write(), assistant_tool_use(), tool_result_ok()]);
        assert_eq!(call.status, Some(ToolStatus::Completed));
        assert_eq!(call.locations, vec!["/private/tmp/claude-501/rust-probe-out.txt".to_string()]);
        assert_eq!(call.parent_tool_use_id, None);
        // And the lead's record serializes exactly as it did before the field existed.
        assert!(!serde_json::to_string(&call).unwrap().contains("parentToolUseId"));
    }

    #[test]
    fn machinery_a_nested_worker_write_as_three_frames_keeps_its_parent_through_the_merge() {
        let frames = [nested(open_write()), nested(assistant_tool_use()), nested(tool_result_ok())];
        // Stamped from the frame on every route: the open, the arguments and the close.
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(one(f, i as u64).parent_tool_use_id.as_deref(), Some("toolu_AGENT"), "{f}");
        }
        let call = merged(&frames);
        assert_eq!(call.status, Some(ToolStatus::Completed));
        assert_eq!(call.parent_tool_use_id.as_deref(), Some("toolu_AGENT"));
        // Kept even when a later frame (here the close, without the field) says nothing.
        let mut close = tool_result_ok();
        close.as_object_mut().unwrap().remove("parent_tool_use_id");
        let call = merged(&[nested(open_write()), nested(assistant_tool_use()), close]);
        assert_eq!(call.parent_tool_use_id.as_deref(), Some("toolu_AGENT"));
        // A heartbeat's parent field names the call it belongs to, not a worker: never stamped.
        let beat = json!({"type":"tool_progress","tool_use_id":"toolu_A-heartbeat-1",
                          "parent_tool_use_id":"toolu_A","elapsed_time_seconds":30});
        assert_eq!(one(&beat, 0).parent_tool_use_id, None);
    }

    #[test]
    fn machinery_a_journal_row_written_before_the_field_reads_back_as_none() {
        let mut row = serde_json::to_value(one(&open_write(), 0)).unwrap();
        row.as_object_mut().unwrap().remove("parentToolUseId");
        let back: MachineryRecord = serde_json::from_value(row).unwrap();
        assert_eq!(back.parent_tool_use_id, None);
        let nested_row = serde_json::to_value(one(&nested(open_write()), 0)).unwrap();
        assert_eq!(nested_row["parentToolUseId"], json!("toolu_AGENT"));
    }
}
