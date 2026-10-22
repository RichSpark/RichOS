//! The TYPED TIMELINE — §12 of the Codex-inspired conversation UX brief
//! (`docs/design/richos-codex-inspired-conversation-ux-2026-08-28.md`), slice 2 of §24.
//!
//! §12 opens with the instruction this module exists to obey: *"Do not model the
//! conversation as alternating `user` and `assistant` strings. Use typed items."*
//!
//! ## What this module is, and is not
//! It is a **projection**, not a new event source. Every item it emits is derived from
//! something already durable — a ledger turn, a persisted text run, a journaled machinery
//! record. It invents no state, opens no counter, and writes nothing. That is deliberate:
//! §23's Phase 1 exit gate is *"kill and restart during each turn state without losing or
//! inventing visible state"*, and a projection over durable inputs passes that gate by
//! construction rather than by care. Re-project the same inputs and you get byte-identical
//! items, ids included (see [`TimelineItem::id`] — every id is DERIVED from its source's
//! identity, never freshly generated).
//!
//! ## The three properties that are enforced here rather than documented
//!
//! **1. One shared per-turn sequence (techy-mode §1.4 G1).** A timeline item's
//! [`sequence`](TimelineBase::sequence) is never computed by this module. It is the value
//! the client's drain point already assigned (`native.rs`'s `prompt`), read back off the machinery
//! record or the persisted text run. There is no timeline counter. A stream item with no
//! recorded position reports `sequence: None` — *unknown*, not zero.
//!
//! **2. Entity scope on every record.** Slice 1 proved the leak class is live: the
//! re-prime digest was assembling every entity's actions into every session. A record type
//! that carries `thread_id` but not `entity_id` re-opens that by omission, so
//! [`TimelineBase`] carries the full ECS fence — `entity_id`, `thread_id`, `turn_id`,
//! `binding_revision` — on every single item, and [`Timeline::project`] refuses to emit an
//! item it cannot place inside the requested binding. `MachineryRecord` carries a
//! `thread_id` and NO entity, which is exactly the omission the guard below exists for.
//!
//! **3. Visibility is a gate, not a hint.** §5's grammar depends on raw tool syntax
//! staying hidden by default, and Phase 5's exit gate is that the CEO can review an
//! outcome without seeing implementation machinery. So [`Visibility`] is not a field a
//! renderer must remember to check:
//!
//!   - [`Timeline`] does **not** implement `Serialize`. You cannot hand the whole thing to
//!     a webview. (The compile_fail doctest on [`Timeline`] pins that.)
//!   - The only way to obtain items for rendering is [`Timeline::view`], which takes a
//!     [`ViewMode`] and returns a [`TimelineView`] that has already **dropped** every item
//!     the mode may not see and **removed** — not flagged, removed — the technical detail
//!     (exact commands, output previews, file paths) from the items it keeps.
//!   - The unfiltered list is reachable only through
//!     [`Timeline::audit_including_internal`], which is named so that nobody arrives there
//!     by accident.
//!
//! ## What is REAL today and what is modelled but unsourced
//! §22's rule is *"if the source signal does not exist, build the signal first or show
//! unknown"*, so the union below is complete per §12 while the constructors are not:
//!
//! | Variant | Source today |
//! |---|---|
//! | [`TimelineItem::UserMessage`] | REAL — the ledger turn's `user_text` |
//! | [`TimelineItem::RichMessage`] | REAL text, **phase `Unknown`** for every CEO turn (see below) |
//! | [`TimelineItem::WorkDuration`] | REAL — measured from the turn's start/end events |
//! | [`TimelineItem::Activity`] | REAL — journaled machinery records |
//! | [`TimelineItem::SystemError`] | REAL — an interrupted turn |
//! | [`TimelineItem::UpstreamOutage`] | REAL — a turn that died to the model API (row 3.30) |
//! | [`TimelineItem::Recovery`] | REAL — `TurnSuperseded` (mid-turn-crash replay) |
//! | [`TimelineItem::WorkerActivity`] | REAL — a `Task` call joined by `agent_id` to the lifecycle stream |
//! | [`TimelineItem::Worker`] | **NO SOURCE.** §12-verbatim `WorkerRun`; never constructed here. |
//! | [`TimelineItem::Plan`] | **NO SOURCE.** `plan` updates are Phase-2 machinery. |
//! | [`TimelineItem::Approval`] | **NO SOURCE.** Nothing asks the CEO to approve anything. |
//! | [`TimelineItem::Question`] | **NO SOURCE.** There is no `waiting_for_user` state. |
//! | [`TimelineItem::Artifact`] | **NO SOURCE.** Phase 5 owns artifacts and provenance. |
//!
//! ### The message phase, stated plainly
//! §22's required change to `stream.rs` is *"one undifferentiated assistant text stream"*
//! → *"message phase plus typed activity events"*. This module delivers the typed activity
//! events and the phase TYPE. It does not deliver a phase SIGNAL, because none exists:
//! every assistant token in this runtime arrives on one channel with nothing marking where
//! commentary ends and the answer begins. So every CEO turn's prose is
//! [`RichMessagePhase::Unknown`].
//!
//! Guessing would be cheap and wrong. The obvious heuristic — "the last run is the final
//! answer" — is false the moment Rich runs one more verification command after writing his
//! conclusion, and the CEO reads the final response as the deliverable, so mislabelling
//! commentary as final is a product defect rather than a cosmetic one. `Unknown` is the
//! honest answer until the emitter is taught to open a message with a phase
//! (§13 `rich://message-started`).
//!
//! What this module DOES contribute toward that split is structural: a turn's prose is no
//! longer one blob. It is one item per contiguous run of the shared sequence
//! (`ledger::TextRun`), so "he said X, then ran Y, then said Z" is three items with real
//! positions, and the phase field is the only thing still missing.
//!
//! ### Workers, stated plainly — the signal landed, and what it still cannot say
//! §22 names active worker count, worker waiting state and completion state as things that
//! must not be faked. When slice 2a was written there was no engine lifecycle signal at
//! all, so [`WorkerRun`] and [`TimelineItem::Worker`] had no constructor and a delegated
//! `Task` call became an ordinary [`TimelineItem::Activity`].
//!
//! The engine landed the signal at `d14bc54` (`engine/docs/worker-lifecycle-events.md`), so
//! a `Task` call **joined to a real `agent_id`** now becomes a
//! [`TimelineItem::WorkerActivity`]. Three things about that join are load-bearing:
//!
//! 1. **It joins by IDENTITY, never by name and never by timing proximity.** The witness is
//!    the same one `worker-created-handoff.sh` uses — the harness's async-launch
//!    acknowledgement *and* an extractable `agentId` — read from the tool call's own raw
//!    payload. A `Task` call with no extractable id, or an id with no row in the stream,
//!    stays an ordinary `Activity`, exactly as before.
//! 2. **The join is SESSION-SCOPED.** `agent_id` is the join key but it is not globally
//!    unique across sessions, so a row is admitted only when its `session_id` matches the
//!    record's. Without that clause another session's worker name and authored summary
//!    render inside this entity's thread on a row that looks perfectly scoped — the same
//!    shape of leak slice 2a found in the `toolCallId` merge. See
//!    `no_worker_row_from_another_session_attaches_to_this_sessions_task_call`.
//! 3. **Only the four observable states cross the boundary.** The stream witnesses
//!    `created`, `started`, `updated` and `run_ended`; `waiting`, `interrupted` and `failed`
//!    have no witness at all (see [`crate::worker_events`]). `run_ended` maps to
//!    [`RUN_ENDED_WORKER_STATE`], which is [`WorkerState::Unknown`] — **not**
//!    [`WorkerState::Completed`].
//!
//! [`TimelineItem::Worker`] and [`WorkerRun`] are untouched and still have no constructor:
//! they are §12-verbatim and carry fields (`result_ref`, `error_ref`, `completed_at`) that
//! nothing witnesses.

use crate::entity::{EntityId, ThreadBinding};
use crate::worker_events::{ObservedWorkerState, SessionScope, WorkerEventRow};
use crate::ledger::{AttentionTier, Ledger, LedgerError, Source, Turn, TurnState};
use crate::machinery::{MachineryKind, MachineryRecord, ToolStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// VISIBILITY — the gate
// ---------------------------------------------------------------------------

/// Who an item is for (§12 `visibility: "ceo" | "technical" | "internal"`).
///
/// Not a hint. See [`Timeline::view`] for the enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// The calm conversation. Semantic, never raw syntax (§5.3).
    Ceo,
    /// Visible only in technical mode: exact commands, output previews, file paths,
    /// vendor kinds, failure reasons (§5.3's "allowed only after expansion or in
    /// technical mode").
    Technical,
    /// Renders in NO mode, ever. Re-prime traffic, rotation and handoff turns, crash
    /// recovery, model reasoning text. Mirrors `MachineryRecord.internal` (§1.5) and
    /// `ledger::ActionVisibility::Internal`, and honours the standing order that Rich
    /// never reveals or references session rotation.
    Internal,
}

/// What a renderer may ask for. There is deliberately no `ViewMode::Internal`: internal
/// items have no render path at all, so no mode can request them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    Ceo,
    Technical,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Ceo => "ceo",
            Visibility::Technical => "technical",
            Visibility::Internal => "internal",
        }
    }

    /// Whether an item at this visibility renders in `mode`. `Internal` is false for
    /// every mode — that is the whole point of the variant.
    pub fn renders_in(&self, mode: ViewMode) -> bool {
        match (self, mode) {
            (Visibility::Ceo, _) => true,
            (Visibility::Technical, ViewMode::Technical) => true,
            (Visibility::Technical, ViewMode::Ceo) => false,
            (Visibility::Internal, _) => false,
        }
    }
}

// ---------------------------------------------------------------------------
// THE SHARED BASE
// ---------------------------------------------------------------------------

/// Where an item sits relative to the turn's shared stream counter.
///
/// The counter (§1.4 G1) covers the STREAM — everything the lease emitted while answering.
/// The CEO's prompt happens before the lease is prompted at all, and the duration row is a
/// property of the finished turn; neither has a position IN the counter, and giving them
/// one would mean inventing numbers. So ordering is `(turn, slot, sequence)`: a
/// discriminant, then the one real counter. `slot` counts nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineSlot {
    /// Belongs to the turn's start, outside the stream counter: the CEO's message, and a
    /// proactive message (written atomically, never streamed).
    Opening,
    /// Emitted by the lease during the turn, carrying a shared-counter position.
    Stream,
    /// A property of the finished turn: the duration row, a failure, a recovery.
    Terminal,
}

/// §12's minimum shared fields, plus `binding_revision`.
///
/// `binding_revision` is not in §12's list and is here on purpose: it completes the ECS
/// scope key (`person + entity + thread + revision + turn + audience`) that slice 1 made
/// real, and §13 requires every event on the wire to carry it so the renderer can reject
/// anything that does not match the immutable binding. `audience` is still absent — slice
/// 1 deferred it as decoration while exactly one audience exists, and that has not changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineBase {
    /// DERIVED from the source record's identity, never generated. Re-projecting the same
    /// durable inputs after a restart produces the same id — which is what makes §13's
    /// *"repeated event IDs are idempotent"* achievable and what stops a cold reopen from
    /// looking like a stream of brand-new items.
    pub id: String,
    pub entity_id: EntityId,
    pub thread_id: String,
    pub turn_id: String,
    pub binding_revision: u64,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
    /// The position in the ONE per-turn counter shared with assistant text and machinery.
    ///
    /// `None` means one of exactly two honest things, distinguished by `slot`:
    /// `Opening`/`Terminal` ⇒ this item is outside the stream counter by construction;
    /// `Stream` ⇒ the position existed but was never written down (a text run from before
    /// the ledger persisted `seq`). It never means zero.
    pub sequence: Option<u64>,
    pub slot: TimelineSlot,
    pub visibility: Visibility,
}

impl TimelineBase {
    /// The ordering key. `(slot, sequence)` within a turn; an unpositioned stream item
    /// sorts AFTER every positioned one, because claiming it came first would be a claim.
    fn order_key(&self) -> (TimelineSlot, u64, u64) {
        match self.sequence {
            Some(seq) => (self.slot, 0, seq),
            None => (self.slot, 1, 0),
        }
    }
}

// ---------------------------------------------------------------------------
// ITEM-SPECIFIC TYPES
// ---------------------------------------------------------------------------

/// §12's `RichMessagePhase`, plus the fifth state the runtime actually produces.
///
/// `Unknown` is not in §12. It is here because §22 forbids faking and requires the honest
/// alternative: today no signal distinguishes commentary from the final response, so every
/// CEO turn's prose is `Unknown`. A renderer that wants to show the final answer must wait
/// for a real phase rather than read a default. Serializing it as the string `"unknown"`
/// (rather than omitting the field) is deliberate — an absent field invites
/// `phase ?? "final"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RichMessagePhase {
    Commentary,
    Final,
    Proactive,
    Recovery,
    Unknown,
}

/// §12's `ActivityState`, plus `Unknown`, which the measured wire forces.
///
/// 34 of the 58 tool events measured on 2026-08-28
/// (`docs/verification/acp-emission-probe-2026-08-28.md`, and `machinery.rs`'s deviation
/// 3) carried NO `status` field at all, and `in_progress` never appeared once. A record
/// whose merged status is absent, or is a vendor status we do not recognize, has an
/// unknown state — it is not "completed", and `ToolStatus::is_terminal` already refuses to
/// call an unrecognized status terminal. §22 lists completion state under "must not be
/// faked".
///
/// `Stopped` has no source: nothing in this runtime can stop a tool call on the CEO's
/// behalf (§9.3's stop button does not exist yet), so it is modelled and never produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Queued,
    Running,
    Completed,
    Stopped,
    Failed,
    Unknown,
}

/// §12's `activityType` set, verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    Read,
    Search,
    Command,
    Patch,
    Integration,
    Browser,
    Image,
    Environment,
    Thread,
    Approval,
    Other,
}

/// The turn-level work state behind the §6.1 duration row.
///
/// Mapped 1:1 from `ledger::TurnState`, using the ledger's own vocabulary rather than
/// §6.1's labels.
///
/// **What changed on 2026-08-29, and why the note that used to sit here is gone.** It read:
/// *"`Interrupted` covers a crash, a rotation and a cancel alike, so rendering 'You stopped
/// after 18s' would attribute the stop to the CEO on no evidence. Distinguishing them needs
/// a CEO-stop signal that does not exist (§9.3)."* That signal now exists: a stop request is
/// written to `steering::IntakeLog` and fsync'd BEFORE the lease is touched, and
/// `Ledger::stop_turn` is reachable only from it. So [`WorkState::Stopped`] is sourced
/// evidence of a CEO decision, and [`WorkState::Interrupted`] keeps its old meaning
/// unchanged — a crash or a rotation, WHICH ONE IS STILL NOT RECORDED, and still not
/// attributable to anybody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    /// Journaled, not yet handed to a lease.
    Queued,
    /// Handed to a lease; no terminal event yet.
    Working,
    Completed,
    /// Ended before turn-end: crash or rotation. Which one is not recorded — and it is
    /// NOT the CEO, which is the one thing this state can now say for certain.
    Interrupted,
    /// Ended because the CEO stopped it (§9.3). The only state §6.1's `You stopped after
    /// {duration}` may be rendered from.
    Stopped,
}

/// §12's `WorkerRun`, verbatim in shape.
///
/// **MODELLED, NOT SOURCED.** No constructor exists and [`Timeline::project`] never emits
/// one. See the module doc: the engine emits no worker lifecycle signal, and §22 names
/// active worker count, worker waiting state and completion state as things that must not
/// be faked. The consumer half of this is written against the real schema once the
/// lifecycle signal lands, not against a guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRun {
    pub id: String,
    pub parent_turn_id: String,
    pub entity_id: EntityId,
    pub thread_id: String,
    pub display_name: String,
    pub role: String,
    pub delegated_task: String,
    pub state: WorkerState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_update: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_ref: Option<String>,
}

/// §12's worker lifecycle states, **plus `Unknown`, which the measured signal forces.**
///
/// §12 lists six states and assumes all six are knowable. The engine's measurement
/// (`engine/docs/worker-lifecycle-events.md`) says three of them are not: `Waiting`,
/// `Interrupted` and `Failed` have no witnessing signal anywhere in the hook set. Every run
/// that ends arrives as `run_ended` with the reason genuinely unobservable, and §12 has no
/// variant for that — so without `Unknown` this enum **cannot represent a real worker run**.
///
/// `Unknown` is the same admission [`RichMessagePhase::Unknown`] makes for the message
/// phase, for the same reason and with the same consequence: a renderer that wants to show
/// a completion or a failure must wait for a real signal instead of reading a default.
///
/// The three unwitnessed variants are retained because they are §12's vocabulary and a real
/// signal may yet arrive for them — but **nothing in this crate constructs them**, and
/// [`RUN_ENDED_WORKER_STATE`] exists precisely so no future author reaches for `Completed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    PendingInit,
    Running,
    /// NO SIGNAL. `TeammateIdle` cannot separate "paused for input" from "finished for
    /// good". Never constructed.
    Waiting,
    /// NO SIGNAL at worker grain. `TaskCompleted` is authoritative but task-grain; a
    /// `SubagentStop` is not a completion. Never constructed from the lifecycle stream.
    Completed,
    /// NO SIGNAL. A `shutdown_request` is an instruction issued before anything happens,
    /// not an observation. Never constructed.
    Interrupted,
    /// NO SIGNAL. No payload carries an outcome; `stop_hook_active`, a transcript path and
    /// a last assistant message are not verdicts. Never constructed.
    Failed,
    /// The run is over and **the reason is not observable**. The honest superset of
    /// `Completed`, `Interrupted` and `Failed`. Serialized explicitly as `"unknown"`.
    Unknown,
}

/// The state a `run_ended` observation maps to — a NAMED CONSTANT, so the mapping is one
/// declared thing rather than a match arm each author re-decides.
///
/// This follows slice 3's `STREAMED_MESSAGE_PHASE` precedent exactly, and for the same
/// reason: an unknown that is not named gets silently defaulted into a confident one. The
/// tempting default here is `Completed`, and it would be wrong roughly two thirds of the
/// time — `run_ended` is the superset of completed, interrupted and failed, and §7.4 renders
/// failure and recovery completely differently from success. A wrong collapse does not
/// mislabel a row; it draws the wrong thing.
pub const RUN_ENDED_WORKER_STATE: WorkerState = WorkerState::Unknown;

impl WorkerState {
    /// Map an observed lifecycle state onto §12's vocabulary. The ONLY mapping in this
    /// crate, and it never yields `Completed`, `Waiting`, `Interrupted` or `Failed`.
    ///
    /// `Updated` maps to `Running` on the same basis as `Started`: the run is open (no
    /// terminal event has been observed for it). Whether the host is still ALIVE is a
    /// separate question, answered by a real pid probe in `worker_status.rs` — a timeline
    /// item is a record of what was witnessed during a turn, not a live count.
    pub fn from_observed(observed: ObservedWorkerState) -> WorkerState {
        match observed {
            // The harness accepted the spawn and returned an id. Execution not confirmed —
            // which is exactly what §7.1 renders as "Starting".
            ObservedWorkerState::Created => WorkerState::PendingInit,
            ObservedWorkerState::Started | ObservedWorkerState::Updated => WorkerState::Running,
            ObservedWorkerState::RunEnded => RUN_ENDED_WORKER_STATE,
        }
    }
}

/// A delegated AI worker, as witnessed by the engine's lifecycle stream (§7).
///
/// **REAL and sourced**, unlike [`WorkerRun`] beside it. Built only when a `Task` tool call
/// carries an extractable `agentId` AND that id has at least one row in the worker-lifecycle
/// stream for the SAME session. Everything in here was observed; nothing is derived from
/// timing, from a name match, or from the text of a last assistant message.
///
/// The fields §12's `WorkerRun` has and this does not — `result_ref`, `error_ref`,
/// `completed_at` — are absent because nothing witnesses them. An outcome would have to be
/// scraped from a transcript, which is a guess wearing a field name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerActivityItem {
    /// THE JOIN KEY, and the only one. Never a display name, never proximity in time.
    pub agent_id: String,
    /// The spawn-time display name, from a `created` row. `None` when only `started` was
    /// observed — that payload has no name and nothing invents one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// The LAST state witnessed for this worker in this session. A record of an
    /// observation, never a verdict — in particular `RunEnded` is not a completion.
    pub observed_state: ObservedWorkerState,
    /// [`observed_state`](Self::observed_state) mapped onto §12's vocabulary via
    /// [`WorkerState::from_observed`]. `RunEnded` becomes [`RUN_ENDED_WORKER_STATE`].
    pub state: WorkerState,
    /// The latest `summary` a worker actually authored (§7.2 item 4). Never the message
    /// body — the emitter does not log content, so this cannot leak one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_update: Option<String>,
    /// RFC-3339 timestamps as the emitter wrote them, first and last row observed. Labels
    /// for display; no duration is computed from them, because §22 forbids faking elapsed
    /// active time and wall-clock spread is not active time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_observed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_observed_at: Option<String>,
    /// How many lifecycle rows were observed for this worker. Evidence volume, not progress.
    pub events_observed: usize,
}

/// One CEO-readable plan row (§8). Modelled, not sourced — `plan` session updates are
/// Phase-2 machinery (`machinery.rs`), retained verbatim under `MachineryKind::Unknown`
/// and deliberately not typed here: their entries live only in the evictable raw payload,
/// so a plan projected from them would silently empty out after the Tier-B window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub text: String,
    pub state: ActivityState,
}

/// The technical half of an activity row: exact command, output preview, touched paths.
///
/// §5.3: *"Do not render full shell commands by default … Exact commands are allowed only
/// after expansion or in technical mode."* This struct is what [`Timeline::view`] REMOVES
/// — not masks, removes — from a `ViewMode::Ceo` view. A CEO-mode item has `detail: None`
/// because the bytes were dropped before the item was handed over, not because a renderer
/// was asked not to look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDetail {
    /// The merged tool-call title — frequently the exact command (`cat VERSION`).
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub locations: Vec<String>,
    /// The vendor's own `sessionUpdate` kind for an untyped record, so §1.4 G5's "one dim
    /// line" stays truthful in technical mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_kind: Option<String>,
}

/// One record of what the lease said while NO turn was in flight (techy-mode §1.5, gap #1).
///
/// **A SEPARATE LANE, and not a [`TimelineItem`], for one structural reason.** Every
/// `TimelineItem` carries a [`TimelineBase`] with a `turn_id`, and is placed in the
/// timeline by that turn: the item list is turn-buckets end to end. A between-turn record
/// has no turn — §1.4 G4 makes `turn_id: None` a first-class state — so putting it in that
/// list would mean either giving `TimelineBase.turn_id` an absent case that 30-odd
/// constructors and every consumer would have to re-decide, or inventing a turn for it. The
/// second is a false attribution, which is the one thing a record of what happened must not
/// do; the first buys nothing, because the item would still have no bucket to sit in and
/// would have to be appended somewhere, which is a claim about position that nothing
/// witnessed.
///
/// So the honest shape is the one the data has: these are thread-scoped, they are ordered
/// among themselves, and they are not positioned in the conversation. The renderer shows
/// them as their own section for exactly that reason.
///
/// **It is gated by the SAME [`Visibility`] the items are** — [`Timeline::view`] filters
/// this lane through `renders_in` in the same pass — so there is one gate and one enum, not
/// a second mechanism beside them.
///
/// **What it deliberately does NOT carry:** `sessionId`. Every `MachineryRecord` has one
/// (rotation must stay reconstructible — `ledger.rs:170`), and no rendering type in this
/// file has ever exposed it. A lane whose records arrive at session boundaries is precisely
/// where a session identifier would become a rotation tell, so the omission is load-bearing
/// here rather than incidental.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BetweenTurnItem {
    /// The machinery id — already stable and already on disk, so a re-projection after a
    /// restart yields the same id. Same rule as [`TimelineBase::id`].
    pub id: String,
    pub entity_id: EntityId,
    pub thread_id: String,
    pub binding_revision: u64,
    /// The lane's own counter, carried for identity. **NOT a sort key**, and not §1.4 G1's
    /// shared per-turn counter: the lane's counter is per LEASE and restarts on a rotation.
    /// Order is journal append order — see [`crate::machinery::project_between_turns`].
    pub sequence: u64,
    /// Epoch millis. A LABEL, never the ordering key (§1.4 G3).
    pub at: u64,
    pub visibility: Visibility,
    /// The vendor's own `sessionUpdate` kind. These records are technical by construction —
    /// there is no CEO-safe semantic line for "the adapter restated its command list" —
    /// so the vendor kind IS the row, which is §1.4 G5's "one dim line" said plainly.
    pub vendor_kind: String,
    /// The journal key for a drill-down into the raw payload (§12's `detailRef`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_ref: Option<String>,
    /// Technical-mode only, exactly like an activity row's. Removed outright by
    /// [`TimelineItem::redacted`]'s counterpart in [`Timeline::view`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ActivityDetail>,
}

/// The technical half of the duration row: the raw vendor stop reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// THE UNION
// ---------------------------------------------------------------------------

/// §12's `TimelineItem` union.
///
/// Internally tagged on `kind`, camelCase payloads, base flattened — so one item
/// serializes to the flat shape §12 and §13 describe:
/// `{ "kind": "activity", "id": …, "entityId": …, "threadId": …, "turnId": …, … }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelineItem {
    /// The CEO's message (§5.1). REAL.
    #[serde(rename_all = "camelCase")]
    UserMessage {
        #[serde(flatten)]
        base: TimelineBase,
        text: String,
        /// How the input arrived — text or voice. Both land in one thread.
        source: Source,
        /// The phone's own id for this message when a phone sent it (`Turn::client_id`,
        /// adoption ledger §2.8 row C5). `clientId` on the wire; absent otherwise, so every
        /// other turn serializes exactly as it did before this field existed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// The words are the transcript of a voice message he recorded on a phone
        /// (`Turn::voice_note`). `voiceNote: true` on the wire; absent otherwise, so every other
        /// turn serializes exactly as it did before this field existed.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        voice_note: bool,
    },
    /// One contiguous run of Rich's prose (§5.2 / §5.4). REAL text; `phase` is `Unknown`
    /// for every CEO turn until a phase signal exists.
    #[serde(rename_all = "camelCase")]
    RichMessage {
        #[serde(flatten)]
        base: TimelineBase,
        phase: RichMessagePhase,
        text: String,
        /// Set only for a proactive message — the tier it was raised at (§5.1).
        #[serde(skip_serializing_if = "Option::is_none")]
        tier: Option<AttentionTier>,
    },
    /// The working-duration row (§6). REAL, measured, and `active_ms` is `None` whenever
    /// the span was not fully recorded.
    #[serde(rename_all = "camelCase")]
    WorkDuration {
        #[serde(flatten)]
        base: TimelineBase,
        state: WorkState,
        #[serde(skip_serializing_if = "Option::is_none")]
        started_at: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ended_at: Option<u64>,
        /// The measured active span. `None` = not recorded; NEVER `now() - started_at`
        /// (UX §6.3, `ledger::Turn::active_ms`).
        #[serde(skip_serializing_if = "Option::is_none")]
        active_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<WorkDetail>,
    },
    /// One semantic work-activity row (§5.3 / §12's `ActivityItem`). REAL — projected from
    /// journaled machinery.
    ///
    /// §12's union names this member `ActivityGroupItem` while its type block defines a
    /// single `ActivityItem` with `kind: "activity"`. The type block wins: grouping
    /// (*"Read 8 files"*) is a renderer rollup over these rows, and a group is not
    /// something the data layer can compute without deciding what a "meaningful action
    /// cluster" is — a rendering judgement.
    #[serde(rename_all = "camelCase")]
    Activity {
        #[serde(flatten)]
        base: TimelineBase,
        activity_type: ActivityType,
        state: ActivityState,
        /// A CEO-safe semantic line built from the type alone — no command text, no
        /// vendor strings, no output (§5.3's "keep the CEO default semantic").
        summary: String,
        /// The journal key for a drill-down (§12's `detailRef`). An opaque id, not content.
        #[serde(skip_serializing_if = "Option::is_none")]
        detail_ref: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        started_at: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_at: Option<u64>,
        /// THE SHORTEST WAIT OF THIS KIND EVER MEASURED, in milliseconds
        /// (`machinery::measured_min_ms`), or `None` - which is every kind but one.
        ///
        /// It exists for one reason, and the reason is the CEO's own design for a compaction
        /// bar (2026-09-06): *"It doesn't need to know. It just needs to move to almost full
        /// with the expected minimum time and then stay at 'almost full' until all finished
        /// etc."* A renderer paces something over this span and then HOLDS at almost-full
        /// until the real ending arrives on the wire.
        ///
        /// IT IS NOT AN ESTIMATE AND IT IS NEVER SHOWN. It says nothing about the wait now
        /// running except that no such wait has ever been shorter, and no number derived from
        /// it reaches the screen - not a percentage, not a countdown, not a remaining time.
        /// The renderer that consumes it (`app/ui/main.js`, THE PACED BAR) needs no idea what
        /// a compaction is, which is what keeps that file free of a vendor concept.
        #[serde(skip_serializing_if = "Option::is_none")]
        measured_min_ms: Option<u64>,
        /// Technical-mode only. Removed outright from a CEO view.
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<ActivityDetail>,
    },
    /// A delegated AI worker (§7). **MODELLED, NEVER PRODUCED** — `WorkerRun` is
    /// §12-verbatim and carries fields (`result_ref`, `error_ref`, `completed_at`) that no
    /// signal witnesses. The sourced worker row is [`TimelineItem::WorkerActivity`].
    #[serde(rename_all = "camelCase")]
    Worker {
        #[serde(flatten)]
        base: TimelineBase,
        run: WorkerRun,
    },
    /// A delegated AI worker as actually witnessed (§7). **REAL** — a `Task` tool call
    /// joined by `agent_id` to the engine's worker-lifecycle stream. See the module doc's
    /// "Workers, stated plainly" for the three load-bearing properties of that join.
    #[serde(rename_all = "camelCase")]
    WorkerActivity {
        #[serde(flatten)]
        base: TimelineBase,
        worker: WorkerActivityItem,
        /// The journal key for a drill-down into the underlying `Task` call (§12's
        /// `detailRef`). An opaque id, not content.
        #[serde(skip_serializing_if = "Option::is_none")]
        detail_ref: Option<String>,
        /// Technical-mode only: the vendor tool title and touched paths. Removed outright
        /// from a CEO view, exactly like an `Activity`'s.
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<ActivityDetail>,
    },
    /// The CEO-readable plan projection (§8). MODELLED, NEVER PRODUCED.
    #[serde(rename_all = "camelCase")]
    Plan {
        #[serde(flatten)]
        base: TimelineBase,
        entries: Vec<PlanEntry>,
    },
    /// An actionable approval card (§5.5). MODELLED, NEVER PRODUCED: nothing in this
    /// runtime asks the CEO to approve anything. The permission requests that DO happen
    /// are auto-approved by `native::decide_permission` and recorded as a fact
    /// (`machinery.rs::from_permission_request`), so they project as an
    /// `Activity { activity_type: Approval, state: Completed }` — a thing that happened,
    /// not a decision awaiting the CEO.
    #[serde(rename_all = "camelCase")]
    Approval {
        #[serde(flatten)]
        base: TimelineBase,
        request: String,
        resolved: bool,
    },
    /// Rich waiting on a CEO answer (§9.4). MODELLED, NEVER PRODUCED: §11's
    /// `waiting_for_user` state does not exist in this runtime, and inferring a question
    /// from prose that ends in a question mark would be exactly the kind of guess §22
    /// forbids.
    #[serde(rename_all = "camelCase")]
    Question {
        #[serde(flatten)]
        base: TimelineBase,
        prompt: String,
        answered: bool,
    },
    /// An output card (§5.4 / Phase 5). MODELLED, NEVER PRODUCED — Phase 5 owns artifact
    /// content, diff review and provenance, and none of it is this slice's.
    #[serde(rename_all = "camelCase")]
    Artifact {
        #[serde(flatten)]
        base: TimelineBase,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        artifact_ref: Option<String>,
    },
    /// A mid-turn crash that was replayed on a fresh lease (§5.5, continuity §5.3). REAL,
    /// from `TurnSuperseded` — at `Internal` visibility, see [`Timeline::project`].
    #[serde(rename_all = "camelCase")]
    Recovery {
        #[serde(flatten)]
        base: TimelineBase,
        /// The turn that died. It stays `interrupted` in the ledger forever.
        recovered_turn_id: String,
        /// The turn that completed the work instead.
        replacement_turn_id: String,
    },
    /// A turn that ended before turn-end (§5.5, §21 "Turn failure"). REAL. Technical
    /// visibility: the FACT of the failure reaches the CEO through the duration row's
    /// `Interrupted` state, while the raw reason (`cognition io: broken pipe`) is
    /// implementation machinery. Nothing here composes CEO-facing copy — the calm wording
    /// §21 asks for is the renderer's, and this record refuses to invent it.
    #[serde(rename_all = "camelCase")]
    SystemError {
        #[serde(flatten)]
        base: TimelineBase,
        detail: Option<String>,
    },
    /// **The turn died to the UPSTREAM MODEL API** (`open-items.md` row 3.30, measured
    /// 2026-09-03). REAL — projected from the ledger's `UpstreamFailure` event.
    ///
    /// **Why this is CEO-visible when [`TimelineItem::SystemError`] beside it is not.**
    /// That record is Technical because its `detail` is a raw vendor string
    /// (`cognition io: broken pipe`) and its doc is right that nothing there may compose
    /// CEO-facing copy. This record composes none either: every sentence on it was
    /// AUTHORED in `upstream.rs`'s `ceo_message` functions at the moment of the failure,
    /// written to the ledger there, and is relayed here verbatim — the same
    /// backend-authored relay `state-strings.js` documents as blind spot B2 and scrapes
    /// for the affordance registry. The renderer neither writes nor edits these strings.
    ///
    /// **And why it exists at all.** Without it the whole answer to *"a dying task must
    /// not lose silently"* would live in a live event that a reload throws away, which is
    /// the difference between visible AT the moment and discoverable afterward.
    #[serde(rename_all = "camelCase")]
    UpstreamOutage {
        #[serde(flatten)]
        base: TimelineBase,
        /// `UpstreamFault::tag` — `overloaded`, `rate_limit`, `server_error`,
        /// `unclassified`. A token for the renderer to key on, never shown as text.
        fault: String,
        /// Whether waiting is a plan with a known end. The `429`/`529` distinction, as a
        /// boolean, so a renderer cannot get it wrong by reading the prose.
        clears_on_a_known_schedule: bool,
        /// What happened, authored by `UpstreamFault::ceo_message`.
        ceo_message: String,
        /// What is on disk and what is not, authored by `TurnLoss::ceo_message` from
        /// counts read off the ledger at the instant of the failure.
        loss_message: String,
        /// What was spent trying, authored by `RetryBudget::ceo_message`. `None` when
        /// nothing had been spent — a different statement from "nothing was spent".
        #[serde(skip_serializing_if = "Option::is_none")]
        retry_message: Option<String>,
        /// The vendor's `req_…`. **Technical**: it is what makes the incident findable in
        /// Anthropic's logs and it means nothing to the CEO, so it is carried and
        /// [`TimelineItem::redacted`] removes it from a CEO view.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    /// **WHY an ordinary turn ended without finishing** — the nightly's D2, projected from
    /// the ledger's `TurnInterruptionCause` event.
    ///
    /// The local sibling of [`TimelineItem::UpstreamOutage`], and CEO-visible for the same
    /// reason: every sentence on it was AUTHORED in `interruption.rs` at the moment of the
    /// failure and written to the ledger there. This projection composes nothing, edits
    /// nothing and interpolates nothing.
    ///
    /// **`offers_retry` is the field that matters.** On 2026-09-17 the published nightly
    /// drew a retry control on a turn that had failed because nobody was signed in — a
    /// button no number of presses could make work. The renderer keys the control on this
    /// boolean, not on the prose, so the sentence and the affordance are driven by one fact
    /// and cannot disagree.
    #[serde(rename_all = "camelCase")]
    TurnInterruption {
        #[serde(flatten)]
        base: TimelineBase,
        /// `InterruptionCause::tag` — a token for the renderer to key on, never shown.
        cause: String,
        /// What happened, authored by `InterruptionCause::ceo_message`.
        ceo_message: String,
        /// What is on disk and what is not. **`None` when nothing of Rich's had been
        /// written**, which is a statement rather than an omission: the card then says
        /// nothing at all about saved work, instead of reassuring about nothing.
        #[serde(skip_serializing_if = "Option::is_none")]
        loss_message: Option<String>,
        /// Whether asking again is a plan. Drives whether a retry control is drawn at all.
        offers_retry: bool,
        /// **Whether the turn carried words of his** (the same test that draws his bubble).
        /// `false` for Rich's reply to Stu's report, an app message or a pressed button:
        /// every sentence on the card is about HIS message (nightly 67 p3: "Press Restore my
        /// message ... Your message is saved" for a message he never sent), so none is drawn.
        his_words: bool,
    },
}

impl TimelineItem {
    pub fn base(&self) -> &TimelineBase {
        match self {
            TimelineItem::UserMessage { base, .. }
            | TimelineItem::RichMessage { base, .. }
            | TimelineItem::WorkDuration { base, .. }
            | TimelineItem::Activity { base, .. }
            | TimelineItem::Worker { base, .. }
            | TimelineItem::WorkerActivity { base, .. }
            | TimelineItem::Plan { base, .. }
            | TimelineItem::Approval { base, .. }
            | TimelineItem::Question { base, .. }
            | TimelineItem::Artifact { base, .. }
            | TimelineItem::Recovery { base, .. }
            | TimelineItem::SystemError { base, .. }
            | TimelineItem::UpstreamOutage { base, .. }
            | TimelineItem::TurnInterruption { base, .. } => base,
        }
    }

    pub fn id(&self) -> &str {
        &self.base().id
    }

    pub fn entity_id(&self) -> &EntityId {
        &self.base().entity_id
    }

    pub fn turn_id(&self) -> &str {
        &self.base().turn_id
    }

    pub fn sequence(&self) -> Option<u64> {
        self.base().sequence
    }

    pub fn visibility(&self) -> Visibility {
        self.base().visibility
    }

    /// Drop every technical-only payload from this item.
    ///
    /// Applied by [`Timeline::view`] for `ViewMode::Ceo`. The bytes are GONE from the
    /// returned value — a CEO view cannot leak a command it does not contain.
    pub(crate) fn redacted(mut self) -> Self {
        match &mut self {
            TimelineItem::Activity { detail, .. } => *detail = None,
            TimelineItem::WorkerActivity { detail, .. } => *detail = None,
            TimelineItem::WorkDuration { detail, .. } => *detail = None,
            TimelineItem::SystemError { detail, .. } => *detail = None,
            // The three authored sentences STAY — they are the whole point of this record
            // and they were written for him. Only the vendor's request id goes.
            TimelineItem::UpstreamOutage { request_id, .. } => *request_id = None,
            _ => {}
        }
        self
    }
}

// ---------------------------------------------------------------------------
// REJECTIONS
// ---------------------------------------------------------------------------

/// Why a record was refused a place in the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    /// The record names a different thread. A LEAK-CLASS rejection.
    ForeignThread,
    /// The record's turn is not one this binding may see: quarantined by the ledger's
    /// cross-entity guard, or absent from the log entirely. A LEAK-CLASS rejection —
    /// this is the clause that catches machinery attached to a forged turn, because
    /// `MachineryRecord` carries a `thread_id` and no entity of its own.
    UnscopedTurn,
    /// Legitimately not turn-scoped (`turn_id: None`) **and internal**: re-prime, rotation
    /// and handoff traffic (§1.5 G4). Not a violation, and still excluded.
    ///
    /// **NARROWED 2026-08-30, and the narrowing is the point.** This used to refuse every
    /// `turn_id: None` record, which lumped two different things together: machinery from a
    /// turn the CEO must never see, and the BETWEEN-TURN traffic §1.5 says should attach to
    /// the thread. The second now has a home — [`Timeline::between_turns`] — and only the
    /// first is refused here.
    ///
    /// So this reason now means exactly one thing: *machinery the standing order forbids
    /// rendering*. It is refused at the GUARD, before an item exists, which is a layer
    /// beneath [`Visibility::Internal`] rather than a replacement for it — an internal
    /// record has no item to gate, and the gate would refuse it anyway if it did.
    NotTurnScoped,
    /// DIAGNOSTIC, not a violation and not an exclusion — the record still rendered, as an
    /// ordinary activity row.
    ///
    /// A `Task` call carried an extractable `agentId`, the worker stream HAS rows for that
    /// exact id, and every one of them was refused by the join's session clause. The row is
    /// correct: `agent_id` is not globally unique and admitting a foreign session's rows is
    /// the leak `no_worker_row_from_another_session_attaches_to_this_sessions_task_call`
    /// pins.
    ///
    /// **It means exactly one thing now: genuinely another session's worker.** This used to
    /// carry a second candidate cause — *"the ACP session id and the harness session id are
    /// DIFFERENT ID SPACES, in which case the join can never fire in production and §7's
    /// whole worker treatment is dead on the wire while every test stays green"* — recorded
    /// as an open question because no live adapter was available to settle it.
    ///
    /// **Closed on 2026-08-29, from artifacts already on this disk.** The ACP session id
    /// `55c79b81-ace3-4b07-a5f3-406853ac1a36`
    /// (`docs/verification/acp-emission-probe-2026-08-28/run1.raw.jsonl`) has a Claude Code
    /// transcript at
    /// `~/.claude/projects/-Users-alex-ab-richos-engine/55c79b81-ace3-4b07-a5f3-406853ac1a36.jsonl`.
    /// The adapter's session id IS the harness session id, so the join can fire, and
    /// `worker_status::resolve_team_dir` now derives the team directory from that same id.
    /// The hypothesis is deleted rather than left standing: a doubt that outlives its own
    /// resolution is read by the next engineer as a live risk.
    ///
    /// Reported rather than logged so a caller can see it without a log scrape. It is NOT
    /// leak-class: nothing crossed a boundary — something was correctly kept out.
    WorkerSessionMismatch,
}

/// One refused record, reported rather than silently dropped — the same posture as
/// `ledger::ScopeViolation`: refusing to render AND saying so beats quietly showing less.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineRejection {
    /// The machinery record's id.
    pub record_id: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub reason: RejectionReason,
}

impl TimelineRejection {
    /// Leak-class: a record that would have crossed a scope boundary had it been rendered.
    pub fn is_scope_violation(&self) -> bool {
        matches!(self.reason, RejectionReason::ForeignThread | RejectionReason::UnscopedTurn)
    }
}

// ---------------------------------------------------------------------------
// THE PROJECTION
// ---------------------------------------------------------------------------

/// One thread's typed timeline, scoped to one entity binding.
///
/// **Deliberately not `Serialize`.** The whole point of [`Visibility`] would be lost if
/// this could be handed to a webview directly; the only serializable form is a
/// [`TimelineView`], which has already applied the gate:
///
/// ```compile_fail
/// # use richos_core::timeline::Timeline;
/// fn leak(t: &Timeline) -> String {
///     // Timeline does not implement Serialize — there is no ungated path to a webview.
///     serde_json::to_string(t).unwrap()
/// }
/// ```
///
/// The positive control for that negative test — same imports, same call, on the gated
/// type, which DOES serialize. If the block above ever starts failing for a boring reason
/// (a bad path, a missing crate) rather than the intended one, this one fails with it:
///
/// ```
/// # use richos_core::timeline::{Timeline, TimelineView, ViewMode};
/// fn ship(t: &Timeline) -> String {
///     serde_json::to_string(&t.view(ViewMode::Ceo)).unwrap()
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    entity_id: EntityId,
    thread_id: String,
    items: Vec<TimelineItem>,
    /// §1.5's between-turn lane: thread-scoped machinery with no turn. Separate from
    /// `items` because a `TimelineItem` is placed by its turn and these have none — see
    /// [`BetweenTurnItem`] for why that is the honest shape rather than a missing feature.
    between_turns: Vec<BetweenTurnItem>,
    rejections: Vec<TimelineRejection>,
}

impl Timeline {
    /// Build one thread's timeline from durable inputs.
    ///
    /// `machinery` takes the RAW records for this thread — the journal's Tier A lines, or
    /// the records as they arrived live. Raw, not pre-merged: this function calls
    /// `machinery::project` itself for the `toolCallId` merge (§1.4 G2), and it needs the
    /// pre-merge records for one measured reason. `merge_into` replaces `payload` with the
    /// LAST update's raw JSON, and the last update in the measured traffic is
    /// a `tool_result` with an outcome and no tool name — it carries neither the
    /// `kind` nor `_meta.claudeCode.toolName`. Both of those arrive on the OPENING event.
    /// So the activity type is resolved across all raw records for a tool call, before the
    /// merge throws that payload away.
    ///
    /// ## The guard
    /// Turns come from `Ledger::thread_turns_scoped`, the single chokepoint slice 1 built:
    /// it verifies the binding, fails closed on an unbound thread, and excludes every
    /// quarantined (cross-entity) turn. Machinery does NOT go through that chokepoint,
    /// because machinery is not in the ledger and `MachineryRecord` carries a `thread_id`
    /// and no `entity_id` at all. So it gets its own two clauses:
    ///
    ///   1. the record's `thread_id` must equal this binding's thread; and
    ///   2. the record's `turn_id` must be a turn the scoped view ACCEPTED.
    ///
    /// The two are not equivalent, and the difference is worth stating because it decides
    /// what each is FOR. Delete clause 1 and a record stamped with another thread but a
    /// turn id belonging to THIS thread — a mis-stamp, a corrupt shard, a future writer
    /// bug — is placed into this thread and rendered: a real leak, demonstrated by
    /// `tests/timeline_tests.rs::no_machinery_from_one_entity_renders_in_another_entitys_thread`.
    /// Clause 2 is the reported form of a containment that is already structural: `rank`
    /// holds exactly the accepted turns and is also the placement key, so a refused turn
    /// has no bucket of its OWN to be placed in.
    ///
    /// CORRECTED 2026-08-29 by Rich, who disabled the clause and ran the test rather than
    /// reasoning about it: **deleting clause 2 DOES leak.** Placement-by-bucket is not the
    /// only path in. A refused record that shares a `toolCallId` with a legitimate call
    /// MERGES INTO that call's row (§1.4 G2, "four wire events, ONE row"), and the merged
    /// row is stamped with THIS binding's entity — so it renders as
    /// `entity_id: femcboost, turn_id: turn_ok` while carrying deeply's title. It looks
    /// perfectly scoped and is not. Stamping the entity from the binding is not a guard;
    /// it is what makes this leak invisible. Clause 2 is load-bearing containment, not a
    /// reporting nicety — do not remove it as redundant.
    pub fn project(
        ledger: &Ledger,
        binding: &ThreadBinding,
        machinery: &[MachineryRecord],
    ) -> Result<Timeline, LedgerError> {
        // No worker stream supplied: every `Task` call stays an ordinary `Activity`, which
        // is exactly slice 2a's behavior. The honest degrade when the engine's lifecycle
        // hooks are not registered (they are snapshotted at session start, so a freshly
        // installed emitter produces its first row only in the NEXT session).
        //
        // NOTE, 2026-08-29: until this slice this was ALSO what the shipping app did.
        // `Spine::timeline` called this function, so `project_with_workers` was reachable
        // only from tests and `TimelineItem::WorkerActivity` could not occur on the wire.
        // The app now sets `WorkerEventsSource` and calls the other one.
        Timeline::project_with_workers(ledger, binding, machinery, &[])
    }

    /// [`Timeline::project`] with the engine's worker-lifecycle rows supplied, so a `Task`
    /// tool call can be joined to the worker it actually spawned.
    ///
    /// ## The worker join, and its own guard
    /// The two machinery clauses above are about the RECORD. They say nothing about the
    /// worker rows, which arrive from a different store entirely (`worker-events.jsonl`,
    /// written by hooks) and carry no entity and no thread — only an `agent_id` and a
    /// `session_id`. So the join gets a third clause:
    ///
    ///   3. a worker row is admitted only when its `session_id` equals the SESSION OF THE
    ///      RECORD it would attach to.
    ///
    /// That clause is load-bearing, not hygiene. `agent_id` is the join key and it is not
    /// globally unique across sessions — the engine's own test residue at
    /// `~/.claude/worker-events.jsonl` shows a single id (`aTESTWORKER00001`) reused across
    /// twelve rows. Delete clause 3 and a row from ANOTHER session attaches to this
    /// session's `Task` call, and its `worker_name` and authored `summary` render stamped
    /// with THIS binding's entity, thread and turn. It looks perfectly scoped and is not —
    /// the same shape of leak the `toolCallId` merge produced in clause 2's write-up.
    /// Proven by `no_worker_row_from_another_session_attaches_to_this_sessions_task_call`.
    pub fn project_with_workers(
        ledger: &Ledger,
        binding: &ThreadBinding,
        machinery: &[MachineryRecord],
        worker_events: &[WorkerEventRow],
    ) -> Result<Timeline, LedgerError> {
        let turns = ledger.thread_turns_scoped(binding)?;
        let entity = binding.entity_id().clone();
        let thread_id = binding.thread_id().to_string();

        // Turn order = log order = chronological. Rank, so items sort per turn without
        // ever consulting a clock (§1.4 G3).
        //
        // `rank` is ALSO the acceptance set, deliberately: it holds exactly the turns the
        // scoped view returned, so a record whose turn is not in it has nowhere to be
        // placed. Containment for that class is by construction rather than by a check
        // that could be deleted — and the explicit clause below turns the silent
        // impossibility into a reported refusal.
        let mut rank: HashMap<&str, usize> = HashMap::new();
        for (i, t) in turns.iter().enumerate() {
            rank.insert(t.id.as_str(), i);
        }

        let mut buckets: Vec<Vec<TimelineItem>> = vec![Vec::new(); turns.len()];
        for (i, turn) in turns.iter().enumerate() {
            buckets[i] = turn_items(turn, &entity, binding.binding_revision());
        }

        // --- machinery: the guard, then the rows -----------------------------------
        let mut rejections = Vec::new();
        let mut kept: Vec<&MachineryRecord> = Vec::new();
        let mut unturned: Vec<&MachineryRecord> = Vec::new();
        for r in machinery {
            if r.thread_id != thread_id {
                rejections.push(TimelineRejection {
                    record_id: r.machinery_id.clone(),
                    thread_id: r.thread_id.clone(),
                    turn_id: r.turn_id.clone(),
                    reason: RejectionReason::ForeignThread,
                });
                continue;
            }
            match r.turn_id.as_deref() {
                // TURN-LESS, and the two cases are different things (§1.5).
                //
                // `internal: true` is re-prime, rotation or handoff machinery: refused
                // HERE, before an item exists, because the standing order says it never
                // renders and the cheapest way to guarantee that is to never build a row
                // for it. That is a layer BENEATH `Visibility::Internal`, not a
                // replacement — the gate would refuse it too if it got that far.
                //
                // Everything else turn-less is BETWEEN-TURN traffic, which §1.5 says
                // attaches to the thread rather than to a turn. It goes to its own lane.
                None if r.internal => rejections.push(TimelineRejection {
                    record_id: r.machinery_id.clone(),
                    thread_id: r.thread_id.clone(),
                    turn_id: None,
                    reason: RejectionReason::NotTurnScoped,
                }),
                None => unturned.push(r),
                Some(turn_id) if rank.contains_key(turn_id) => kept.push(r),
                Some(turn_id) => rejections.push(TimelineRejection {
                    record_id: r.machinery_id.clone(),
                    thread_id: r.thread_id.clone(),
                    turn_id: Some(turn_id.to_string()),
                    reason: RejectionReason::UnscopedTurn,
                }),
            }
        }

        // Activity type + the observed time window, resolved over the RAW records (see
        // the doc above), keyed by tool call.
        let types = resolve_activity_types(&kept);
        let tool_names = resolve_tool_names(&kept);
        let delegations = resolve_delegations(&kept);
        let last_seen = resolve_last_seen(&kept);
        // Resolved over the RAW records for the same measured reason the activity type is:
        // the async-launch acknowledgement arrives on the tool RESULT, and `merge_into`
        // replaces `payload` with the last update's raw JSON. Resolve before the merge, or
        // resolve nothing.
        let agent_ids = resolve_agent_ids(&kept);

        let owned: Vec<MachineryRecord> = kept.into_iter().cloned().collect();
        for row in crate::machinery::project(owned) {
            // Every row here already passed both clauses. If one somehow did not, it is
            // REPORTED rather than dropped on the floor — there is no silent path out of
            // this loop, because a silent drop is how a guard stops being testable.
            let placement = row.turn_id.as_deref().and_then(|t| rank.get(t).map(|&i| (i, t)));
            let Some((i, _)) = placement else {
                rejections.push(TimelineRejection {
                    record_id: row.machinery_id.clone(),
                    thread_id: row.thread_id.clone(),
                    turn_id: row.turn_id.clone(),
                    reason: RejectionReason::UnscopedTurn,
                });
                continue;
            };
            let turn = turns[i];
            // The binding's thread id, not the record's — clause 1 above proved they are
            // equal, and passing the VERIFIED one means the projected item is stamped from
            // the scope that was checked rather than from the record that was checked.
            let internal_turn = turn.source == Source::Internal || turn.superseded_by.is_some();
            // A `Task` call joined BY IDENTITY to the lifecycle stream becomes a worker
            // row. No id, or an id with no in-session rows, falls through to the ordinary
            // activity row — the pre-signal behavior, unchanged.
            let delegated = row.tool_call_id.as_ref().and_then(|id| agent_ids.get(id));
            let joined =
                delegated.and_then(|agent_id| worker_activity(agent_id, &row.session_id, worker_events));
            // The join found nothing, but the stream DOES know this agent id — so the only
            // thing that refused it was the session clause. Say so; see the reason's doc.
            if let (Some(agent_id), None) = (delegated, joined.as_ref()) {
                if worker_events.iter().any(|r| &r.agent_id == agent_id) {
                    rejections.push(TimelineRejection {
                        record_id: row.machinery_id.clone(),
                        thread_id: row.thread_id.clone(),
                        turn_id: row.turn_id.clone(),
                        reason: RejectionReason::WorkerSessionMismatch,
                    });
                }
            }
            match joined {
                Some(worker) => buckets[i].push(worker_activity_item(
                    &row,
                    &entity,
                    binding.thread_id(),
                    &turn.id,
                    binding.binding_revision(),
                    internal_turn,
                    worker,
                )),
                None => buckets[i].push(activity_item(
                    &row,
                    &entity,
                    binding.thread_id(),
                    &turn.id,
                    binding.binding_revision(),
                    internal_turn,
                    &types,
                    &tool_names,
                    &delegations,
                    &last_seen,
                )),
            }
        }

        let mut items = Vec::new();
        for mut bucket in buckets {
            bucket.sort_by(|a, b| a.base().order_key().cmp(&b.base().order_key()));
            items.append(&mut bucket);
        }

        // The between-turn lane. Merged by the same `toolCallId` rule (§1.4 G2) and
        // deliberately NOT sorted: journal append order is chronological, while this lane's
        // `seq` is per-lease and restarts on a rotation — see
        // `machinery::project_between_turns`.
        let between_turns: Vec<BetweenTurnItem> =
            crate::machinery::project_between_turns(unturned.into_iter().cloned().collect())
                .iter()
                .map(|row| between_turn_item(row, &entity, binding.thread_id(), binding.binding_revision()))
                .collect();

        Ok(Timeline { entity_id: entity, thread_id, items, between_turns, rejections })
    }

    pub fn entity_id(&self) -> &EntityId {
        &self.entity_id
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    /// THE GATE. The only way to obtain items for rendering.
    ///
    /// Drops every item the mode may not see (`Internal` in both modes; `Technical` in
    /// CEO mode) and REMOVES the technical detail from the items it keeps when the mode is
    /// `Ceo`. A renderer cannot show what it was never handed.
    pub fn view(&self, mode: ViewMode) -> TimelineView {
        let items = self
            .items
            .iter()
            .filter(|i| i.visibility().renders_in(mode))
            .cloned()
            .map(|i| if mode == ViewMode::Ceo { i.redacted() } else { i })
            .collect();
        // THE SAME GATE, in the same pass, over the between-turn lane. One `renders_in`,
        // one `Visibility` — a second mechanism here is exactly what §1.5 says not to
        // build. `Technical` on every one of these means the CEO view gets an empty lane
        // by construction rather than by a renderer remembering to skip it.
        let between_turns: Vec<BetweenTurnItem> = self
            .between_turns
            .iter()
            .filter(|b| b.visibility.renders_in(mode))
            .cloned()
            .map(|mut b| {
                if mode == ViewMode::Ceo {
                    b.detail = None;
                }
                b
            })
            .collect();
        TimelineView {
            entity_id: self.entity_id.clone(),
            thread_id: self.thread_id.clone(),
            mode,
            items,
            between_turns,
        }
    }

    /// The RAW, UNGATED list, internal items included — the audit view. Named at length on
    /// purpose: every rendering path goes through [`Timeline::view`] instead.
    pub fn audit_including_internal(&self) -> &[TimelineItem] {
        &self.items
    }

    /// The between-turn lane, UNGATED. Named at the same length and for the same reason as
    /// [`Self::audit_including_internal`]: every rendering path goes through
    /// [`Timeline::view`].
    pub fn between_turns_ungated(&self) -> &[BetweenTurnItem] {
        &self.between_turns
    }

    /// Every refused record, including the non-violation exclusions.
    pub fn rejections(&self) -> &[TimelineRejection] {
        &self.rejections
    }

    /// Only the leak-class refusals — a record that would have crossed a scope boundary.
    pub fn scope_violations(&self) -> Vec<&TimelineRejection> {
        self.rejections.iter().filter(|r| r.is_scope_violation()).collect()
    }
}

/// A gated, serializable slice of a timeline. Constructible only by [`Timeline::view`],
/// so the mode it was built for and the items it contains can never disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineView {
    pub entity_id: EntityId,
    pub thread_id: String,
    pub mode: ViewMode,
    pub items: Vec<TimelineItem>,
    /// §1.5's between-turn lane, gated. **Always empty in `ViewMode::Ceo`**, because every
    /// row in it is `Visibility::Technical` and `renders_in` says so — the calm view is
    /// handed nothing to skip.
    pub between_turns: Vec<BetweenTurnItem>,
}

impl TimelineView {
    pub fn items(&self) -> &[TimelineItem] {
        &self.items
    }

    pub fn between_turns(&self) -> &[BetweenTurnItem] {
        &self.between_turns
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The JSON a webview receives. camelCase, `kind`-tagged, base flattened.
    pub fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

// ---------------------------------------------------------------------------
// TURN -> ITEMS
// ---------------------------------------------------------------------------

/// Every non-machinery item one turn contributes.
fn turn_items(turn: &Turn, entity: &EntityId, revision: u64) -> Vec<TimelineItem> {
    // THE TWO WHOLE-TURN DEMOTIONS.
    //
    // 1. An `Internal` turn (re-prime injection, the rotation handoff summary) has no
    //    render path at all — `Ledger::messages` excludes it structurally and §1.5 says
    //    its machinery is `internal` too. Everything it contributes is `Internal`.
    // 2. A SUPERSEDED turn (mid-turn crash, replayed as a new turn) is durable evidence
    //    that must not be rendered: `Ledger::messages` skips it so the CEO sees ONE clean
    //    exchange rather than a duplicated prompt. Rendering its items would put the
    //    duplicate back.
    //
    // A Tier-3 (Silent) proactive message is demoted for the third documented reason
    // (§5.1: it never appears in the conversation) — handled at the message below.
    let internal_turn = turn.source == Source::Internal || turn.superseded_by.is_some();
    let vis = |v: Visibility| if internal_turn { Visibility::Internal } else { v };

    let base = |id: String, seq: Option<u64>, slot: TimelineSlot, at: u64, v: Visibility| TimelineBase {
        id,
        entity_id: entity.clone(),
        thread_id: turn.thread_id.clone(),
        turn_id: turn.id.clone(),
        binding_revision: revision,
        created_at: at,
        updated_at: None,
        sequence: seq,
        slot,
        visibility: v,
    };

    let mut out = Vec::new();

    // --- the CEO's message (§5.1) ---
    // A proactive turn has no CEO prompt at all (`user_text` is empty by construction),
    // so it contributes no user item rather than an empty bubble. Nor does a turn the app
    // started for a button he pressed: those words are not his (product hunt 2026-10-15,
    // finding 14, `Turn::pressed`).
    let his_words = !matches!(turn.source, Source::Proactive | Source::Agent) && !turn.pressed && !turn.user_text.is_empty();
    if his_words {
        out.push(TimelineItem::UserMessage {
            base: base(format!("{}:user", turn.id), None, TimelineSlot::Opening, turn.created_at, vis(Visibility::Ceo)),
            text: turn.user_text.clone(),
            source: turn.source,
            client_id: turn.client_id.clone(),
            voice_note: turn.voice_note,
        });
    }

    // --- Rich's prose (§5.2 / §5.4) ---
    //
    // ONE PIECE OF TEXT IS DEMOTED HERE, AND IT IS NOT RICH'S PROSE AT ALL. When the
    // upstream model API fails, `claude` reports it as an ASSISTANT MESSAGE — the
    // `API Error: 529 Overloaded …` line arrives on the same frame type an answer does, so
    // `native.rs` hands it over as text and the ledger persists it as Rich's reply.
    // Rendering it puts a vendor diagnostic on screen in Rich's voice, indistinguishable
    // from an answer, which is the sharp end of what row 3.30 calls failing silent: the CEO
    // is not told anything went wrong, he is shown a stack-shaped sentence and left to work
    // it out for himself.
    //
    // **IT IS SPLIT AT THE LINE, NOT DROPPED AT THE RUN, and a real test forced that.**
    // The first version demoted a whole `TextRun` whose text classified. Runs are contiguous
    // stretches of the shared per-turn sequence and they only END where a non-text item took
    // a position, so a turn that streamed real prose and then failed WITH NO TOOL CALL
    // BETWEEN THEM has both in one run — and demoting it threw away the answer that did
    // arrive in order to hide the error. That is the failure this row exists to stop,
    // committed in the other direction.
    //
    // So the run is split at the first line that classifies: everything before it is Rich's
    // and renders; that line and everything after it is `Internal`. Nothing is edited or
    // deleted — the ledger keeps the run byte for byte, and both halves are still projected,
    // one of them into a visibility that renders in no mode.
    // =====================================================================================
    // SAID ONCE. A run that repeats what this turn has already said is retained, not shown.
    // =====================================================================================
    //
    // **Ray's candidate-.12 row 10 / defect C**, and the evidence is the walk's own ledger
    // (`conversation-ledger.jsonl` of that run, lines 764-779), one turn:
    //
    // ```text
    //   TurnStarted    session 06bfcb45-…
    //   TurnStarted    session 25a805fc-…      <- a DIFFERENT session, the SAME turn id
    //   PromptReceived source=internal "[re-prime]"
    //   AssistantDelta seq  7.. 12  "Hi. I've got you. What can I do for you?"
    //   AssistantDelta seq 24.. 26  "Hi. I've got you. What can I do for you?"
    //   TurnCompleted  end_turn
    // ```
    //
    // The turn was handed to one lease, the lease was replaced and re-primed under the same
    // turn id, and the successor answered the same prompt again. The seq gap 13..23 is the
    // second session's own non-text items, so `push_text_run` correctly made TWO runs — and
    // two runs are two `RichMessage`s. The Mac drew the greeting twice; the phone, whose
    // rows are built differently, drew it once. *"The Mac and the phone disagree about the
    // same thread."*
    //
    // **The rule is the one this product already applies twice in that same log** — the
    // register's receipt *"had already been said, so the model's own words after it were
    // withheld"*, and the checkpoint's closing line likewise. Say it once. Here it is one
    // clause: inside ONE turn, a run whose kept text is exactly a run already kept is
    // `Internal` — retained, projected, and rendered in no mode, exactly as the vendor
    // diagnostic below it is. Nothing is edited and nothing is deleted; the ledger keeps both
    // runs byte for byte, which is what makes it evidence.
    //
    // **It is deliberately EXACT and deliberately per-turn.** Two different paragraphs are
    // two things said; the same paragraph twice in one answer is one thing said twice. Across
    // turns it does not apply at all — he may ask the same question again and is entitled to
    // the same answer again.
    let mut already_said: Vec<String> = Vec::new();
    for (idx, run) in turn.text_runs.iter().enumerate() {
        let (kept, diagnostic) = crate::upstream::split_at_vendor_diagnostic(&run.text);
        let repeated = {
            let trimmed = kept.trim();
            !trimmed.is_empty() && already_said.iter().any(|said| said == trimmed)
        };
        if !kept.trim().is_empty() {
            already_said.push(kept.trim().to_string());
        }
        let (phase, slot, tier, visibility) = match turn.source {
            // REAL SIGNAL: a proactive message knows what it is. Tier 3 (Silent) never
            // appears in the conversation (§5.1) — internal, not merely quiet.
            Source::Proactive => (
                RichMessagePhase::Proactive,
                TimelineSlot::Opening,
                turn.tier,
                if turn.tier == Some(AttentionTier::Silent) { Visibility::Internal } else { Visibility::Ceo },
            ),
            // NO SIGNAL. Not "final" — see the module doc.
            _ => (RichMessagePhase::Unknown, TimelineSlot::Stream, None, Visibility::Ceo),
        };
        // Rich's half. Emitted under the run's OWN id, so an unaffected turn's items are
        // identical to what they were before any of this existed.
        if !kept.trim().is_empty() {
            out.push(TimelineItem::RichMessage {
                base: base(
                    format!("{}:text:{idx}", turn.id),
                    run.start_seq,
                    slot,
                    run.at,
                    // Said already in this turn -> `Internal`: kept, and shown in no mode.
                    vis(if repeated { Visibility::Internal } else { visibility }),
                ),
                phase,
                text: kept.to_string(),
                tier,
            });
        }
        // The vendor's half. `Internal` renders in NO mode (not even Technical), which is
        // the same treatment a re-prime's text gets and for the same reason: it is real,
        // it is retained, and it is not something Rich said.
        if !diagnostic.is_empty() {
            out.push(TimelineItem::RichMessage {
                base: base(
                    format!("{}:text:{idx}:upstream", turn.id),
                    run.start_seq,
                    slot,
                    run.at,
                    Visibility::Internal,
                ),
                phase,
                text: diagnostic.to_string(),
                tier,
            });
        }
    }

    // --- the working-duration row (§6) ---
    let state = match turn.state {
        TurnState::Received => WorkState::Queued,
        TurnState::InFlight => WorkState::Working,
        TurnState::Completed => WorkState::Completed,
        TurnState::Interrupted => WorkState::Interrupted,
        TurnState::Stopped => WorkState::Stopped,
    };
    out.push(TimelineItem::WorkDuration {
        base: base(
            format!("{}:duration", turn.id),
            None,
            TimelineSlot::Terminal,
            turn.created_at,
            vis(Visibility::Ceo),
        ),
        state,
        started_at: turn.started_at,
        ended_at: turn.ended_at,
        active_ms: turn.active_ms(),
        detail: Some(WorkDetail { stop_reason: turn.stop_reason.clone() }),
    });

    // --- failure (§5.5 / §21) ---
    if turn.state == TurnState::Interrupted {
        out.push(TimelineItem::SystemError {
            base: base(
                format!("{}:error", turn.id),
                None,
                TimelineSlot::Terminal,
                turn.ended_at.unwrap_or(turn.created_at),
                vis(Visibility::Technical),
            ),
            detail: turn.stop_reason.clone(),
        });
    }

    // --- the upstream outage (row 3.30) ---
    //
    // AFTER the SystemError row, because the technical fact came first and this is the
    // explanation of it; and at Ceo visibility, because unlike that row every sentence
    // here was authored for him and written to the ledger at the time. Its timestamp is
    // the turn's own end, so it sorts with the failure rather than at the tail.
    if let Some(up) = &turn.upstream_failure {
        out.push(TimelineItem::UpstreamOutage {
            base: base(
                format!("{}:upstream", turn.id),
                None,
                TimelineSlot::Terminal,
                turn.ended_at.unwrap_or(turn.created_at),
                vis(Visibility::Ceo),
            ),
            fault: up.fault.clone(),
            // Derived from the STORED tag rather than re-classified from prose. A record
            // written by an older build carries a tag this one still understands; a tag it
            // does not understand is reported as having no known schedule, which is the
            // safe direction — it never tells him a wait has an end that nobody measured.
            clears_on_a_known_schedule: up.fault
                == crate::upstream::UpstreamFault::RateLimited.tag(),
            ceo_message: up.ceo_message.clone(),
            loss_message: up.loss_message.clone(),
            retry_message: up.retry_message.clone(),
            request_id: up.request_id.clone(),
        });
    }

    // --- why the turn was interrupted (D2) ---
    //
    // Beside the upstream row and never instead of it: a turn that died upstream has BOTH
    // records and the two say different things (which vendor fault, and what class of
    // ending). `finish_upstream_failure` does not classify locally, so in practice only one
    // is present — but the projection does not depend on that and never has to.
    //
    // Same slot, same timestamp and same Ceo visibility as the upstream row, for the same
    // reasons: it sorts with the failure rather than at the tail, and every sentence on it
    // was authored for him.
    if let Some(cause) = &turn.interruption {
        out.push(TimelineItem::TurnInterruption {
            base: base(
                format!("{}:interruption", turn.id),
                None,
                TimelineSlot::Terminal,
                turn.ended_at.unwrap_or(turn.created_at),
                vis(Visibility::Ceo),
            ),
            cause: cause.cause.clone(),
            ceo_message: cause.ceo_message.clone(),
            loss_message: cause.loss_message.clone(),
            // Read off the STORED value rather than re-derived from the tag, so a record
            // written by an older or newer build renders the control it was written with.
            offers_retry: cause.offers_retry,
            his_words,
        });
    }

    // --- recovery (continuity §5.3) ---
    // ALWAYS Internal, whatever the turn's own visibility: crash recovery and session
    // rotation are recorded as `ActionVisibility::Internal` by the spine, and the standing
    // order is that Rich never reveals or references either. A CEO-facing "we reconnected"
    // row (§21) would need product wording and a decision this slice does not own; the
    // record exists so that decision has something real to render when it is made.
    if let Some(replacement) = turn.superseded_by.as_deref() {
        out.push(TimelineItem::Recovery {
            base: base(
                format!("{}:recovery", turn.id),
                None,
                TimelineSlot::Terminal,
                turn.ended_at.unwrap_or(turn.created_at),
                Visibility::Internal,
            ),
            recovered_turn_id: turn.id.clone(),
            replacement_turn_id: replacement.to_string(),
        });
    }

    out
}

// ---------------------------------------------------------------------------
// MACHINERY -> ACTIVITY
// ---------------------------------------------------------------------------

/// THE ONE VISIBILITY RULE FOR A MACHINERY RECORD, in one place.
///
/// Extracted 2026-08-30 when the between-turn lane (§1.5, gap #1) became a second caller.
/// It was previously inline in `activity_item`, which was fine while there was one caller
/// and is exactly how two rules get born the moment there are two. The standing order that
/// re-prime and rotation machinery never renders is held by the FIRST clause below, and
/// mirroring it meant calling it rather than restating it.
///
/// `internal_turn` folds the caller's whole-turn demotions (an `Internal`-source turn, a
/// superseded replay) into one flag. The between-turn lane passes `false`: it has no turn,
/// so it has no whole-turn demotion — its internal traffic is refused earlier, at the guard
/// in `project_with_workers` (see [`RejectionReason::NotTurnScoped`]).
pub(crate) fn machinery_visibility(row: &MachineryRecord, internal_turn: bool) -> Visibility {
    // A thought is model reasoning text, which §5.3 lists under "do not render". It is
    // kept as an item — dropping it would punch a hole in the shared sequence — at
    // `Internal`, so no mode can surface it.
    // An `internal` record (re-prime, rotation, handoff) is internal for the same reason
    // it is in machinery.rs: the CEO must never see that a rotation happened.
    if row.internal || row.kind == MachineryKind::Thought || internal_turn {
        Visibility::Internal
    } else if row.kind == MachineryKind::Compaction {
        // A COMPACTION IS A CEO ROW, and it is the one place this file lets a `system` frame
        // through to the calm surface.
        //
        // The rule everywhere else here is that vendor plumbing is technical and the CEO is
        // told about work, not about the machine. A compaction is plumbing — and it is also
        // 38.4 to 62.0 seconds of HIS time, measured over 16 boundaries
        // (`machinery.rs`'s COMPACTION block, `docs/verification/compaction-notice-2026-09-06/`),
        // during which the calm surface received nothing at all and the app looked, in the
        // first outside user's words, "like a crashed application". Silence about a
        // fifty-second wait is not calm; it is the defect.
        //
        // It is not the rotation exception in disguise. Rotation is INVISIBLE because it costs
        // the CEO nothing and Rich must never say it happened; this costs him a minute and
        // says only what the wire said, in his words rather than the model's.
        Visibility::Ceo
    } else if row.kind == MachineryKind::PermissionRequested {
        // A PERMISSION REQUEST IS MACHINERY, NOT A CEO ROW.
        //
        // Corrected 2026-08-29 on Frank's §1.1. This branch did not exist: the row fell
        // through to `Visibility::Ceo` and rendered as *"Requested approval"*, rolled up by
        // `app/ui/timeline.js` as *"Requested approval 7 times"* — 7 being the measured
        // count of `session/request_permission` calls across five short probe runs, so this
        // was frequent, not an edge case.
        //
        // Three things are wrong with that on the calm surface, and the third is the one
        // that matters. It is duplicate: the tool call this request belongs to already
        // renders its own semantic row ("Ran a command"), so nothing is lost here. It
        // implies a decision-maker, when `native::decide_permission` auto-approves every request and
        // nobody was asked. And with `state: completed` beside it, a reasonable CEO reads
        // it as GRANTED — which manufactures the demand for an approval queue that does not
        // exist. R2 business-action governance is deferred to V2 by CEO decision for v1 and
        // all 1.x; a summary string is not the way to un-defer it.
        //
        // Every structural refusal in this family held — `rich://approval-requested` and
        // `-resolved` are deliberately not emitted, and `TimelineItem::Approval` is modelled
        // with no constructor (below). The noun walked past them. This is the same
        // correction, made the same way, as `MachineryKind::Unknown` immediately below:
        // route it, retain it, render it in technical mode, keep it off the calm view.
        Visibility::Technical
    } else if row.kind == MachineryKind::Unknown {
        // AN UNTYPED VENDOR KIND IS A TECHNICAL ROW, NOT A CEO ROW.
        //
        // Corrected 2026-08-29, slice 3, by running a real ACP turn rather than reasoning
        // about one. `usage_update`, `available_commands_update` and `session_info_update`
        // all land here (machinery.rs:231), and this branch previously returned `Ceo` —
        // which the comment on `activity_type_of` already contradicted by calling them
        // "one dim TECHNICAL row". The measured cost of the contradiction, from
        // `examples/live_events_roundtrip.rs` against a live session on 2026-08-29: ONE
        // real command produced SIX CEO-facing rows reading *"Worked"* with
        // `state: unknown` (shared-sequence positions 0, 1, 5, 8, 12, 13) against ONE
        // *"Ran a command"* — a 6:1 noise ratio, and the probe measured 50 usage_updates
        // across five runs, so a longer turn is worse.
        //
        // "Worked" is not a thing Rich did; it is an accounting update with no semantic
        // line available, and §5.3's CEO default is a SEMANTIC row. Nothing is lost: the
        // record is still routed, still retained, still on `rich://machinery`, and still
        // rendered in technical mode with its vendor kind in `detail.vendor_kind`.
        //
        // A `ToolCall` whose payload merely failed to classify is not decided here: its
        // record has no type yet. `activity_item` demotes that row too, once the type is
        // known to be `Other` (product hunt 2026-10-15, finding 13).
        Visibility::Technical
    } else {
        // Everything else is a CEO-visible SEMANTIC row whose technical half (the exact
        // command, the output preview, the paths) is carried in `detail` and removed
        // outright from a CEO view.
        Visibility::Ceo
    }
}

/// One between-turn row from one merged turn-less machinery record (§1.5, gap #1).
///
/// The visibility comes from [`machinery_visibility`] — the SAME function the activity rows
/// use — passing `internal_turn: false` because there is no turn to demote. In practice
/// every record reaching here is already non-internal: `project_with_workers` refuses
/// internal turn-less records at the guard. Calling the shared rule anyway is what stops
/// this becoming a second opinion about what may be seen.
pub(crate) fn between_turn_item(
    row: &MachineryRecord,
    entity: &EntityId,
    thread_id: &str,
    revision: u64,
) -> BetweenTurnItem {
    BetweenTurnItem {
        id: row.machinery_id.clone(),
        entity_id: entity.clone(),
        // The BINDING's thread, not the record's — the guard proved they are equal, and
        // stamping from the scope that was checked is the same discipline `activity_item`
        // follows.
        thread_id: thread_id.to_string(),
        binding_revision: revision,
        sequence: row.seq,
        at: row.at,
        visibility: machinery_visibility(row, false),
        vendor_kind: row.title.clone(),
        detail_ref: Some(row.machinery_id.clone()),
        detail: Some(ActivityDetail {
            title: row.title.clone(),
            summary: row.summary.clone(),
            locations: row.locations.clone(),
            vendor_kind: Some(row.title.clone()),
        }),
    }
}

/// One activity row from one merged machinery record.
///
/// Takes the SCOPE explicitly rather than a `&Turn`, so the LIVE path (`live.rs`, which
/// has a fence and a record but no folded `Turn` yet) and the RELOAD path (`project`,
/// which has both) run the same function over the same rules. That is what makes the
/// wire and a reload agree by construction instead of by two implementations agreeing to
/// behave — see `tests/live_event_tests.rs::the_wire_and_the_reload_agree_on_every_field`.
///
/// `internal_turn` folds the caller's two whole-turn demotions (an `Internal`-source turn,
/// a superseded replay) into one flag; the record's own `internal` bit and its kind are
/// still consulted below.
#[allow(clippy::too_many_arguments)]
pub(crate) fn activity_item(
    row: &MachineryRecord,
    entity: &EntityId,
    thread_id: &str,
    turn_id: &str,
    revision: u64,
    internal_turn: bool,
    types: &HashMap<String, ActivityType>,
    tool_names: &HashMap<String, String>,
    delegations: &HashMap<String, String>,
    last_seen: &HashMap<String, u64>,
) -> TimelineItem {
    let activity_type = activity_type_of(row, types);
    let state = activity_state_of(row);
    // **A ROW THAT ONLY SAYS "Worked" IS NOT SHOWN IN HIS VIEW** (product hunt 2026-10-15,
    // finding 13). A tool call that classifies to nothing gets the summary "Worked", and on
    // nightly 57 that word appeared as its own row under "Worked for 15s", as "Worked queued"
    // while Rich was still working, and as the band's "Rich is working / Worked". It names no
    // action and is in the past tense for work not yet done. The record is kept and stays in
    // technical mode, exactly as the untyped vendor kinds above are.
    //
    // A DELEGATION IS NOT NAMELESS. A `Task`/`Agent` call classifies to `Other` too (see
    // [`classify`]), but it is Rich handing work to a teammate, and he sees it while it runs:
    // before its worker row exists (the lifecycle row can land after the tool result), and for
    // a synchronous delegation that never becomes a worker at all.
    let delegation = row
        .tool_call_id
        .as_ref()
        .and_then(|id| tool_names.get(id))
        .is_some_and(|n| n == "Task" || n == "Agent");
    let nameless = activity_type == ActivityType::Other && row.kind == MachineryKind::ToolCall && !delegation;
    // And so its row says what Rich handed over and to whom, from the delegation call's own
    // arguments ([`delegation_summary`]), never the bare "Worked" finding 13 is about. Before
    // those arguments arrive (the streamed open carries `input: {}`) it still names the act.
    let summary = if delegation && activity_type == ActivityType::Other {
        row.tool_call_id
            .as_ref()
            .and_then(|id| delegations.get(id))
            .cloned()
            .unwrap_or_else(|| delegation_line(None, None))
    } else {
        semantic_summary(activity_type, row)
    };
    let visibility = match machinery_visibility(row, internal_turn) {
        Visibility::Ceo if nameless => Visibility::Technical,
        v => v,
    };

    let key = crate::machinery::merge_key(row);
    let updated = last_seen.get(&key).copied().filter(|&t| t > row.at);
    let vendor_kind = (row.kind == MachineryKind::Unknown).then(|| row.title.clone());

    TimelineItem::Activity {
        base: TimelineBase {
            // The machinery id — already stable and already on disk, so a re-projection
            // after restart yields the same item id.
            id: row.machinery_id.clone(),
            entity_id: entity.clone(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            binding_revision: revision,
            created_at: row.at,
            updated_at: updated,
            // NOT a new number: the position the client's drain point assigned (§1.4 G1),
            // carried through the journal and read back here.
            sequence: Some(row.seq),
            slot: TimelineSlot::Stream,
            visibility,
        },
        activity_type,
        state,
        summary,
        detail_ref: Some(row.machinery_id.clone()),
        started_at: Some(row.at),
        // Only when the row actually reached a terminal state. An unknown state has no
        // completion time, because it has no completion.
        completed_at: matches!(state, ActivityState::Completed | ActivityState::Failed)
            .then(|| last_seen.get(&key).copied().unwrap_or(row.at)),
        // From the KIND alone, so the live path and a reload cannot disagree about the pace
        // of a bar any more than they can disagree about its sentence.
        measured_min_ms: crate::machinery::measured_min_ms(row.kind),
        detail: Some(ActivityDetail {
            title: row.title.clone(),
            summary: row.summary.clone(),
            locations: row.locations.clone(),
            vendor_kind,
        }),
    }
}

/// §12's `ActivityState`, mapped from the merged tool status.
fn activity_state_of(row: &MachineryRecord) -> ActivityState {
    match row.kind {
        // A permission request is a thing that already happened by the time it is
        // recorded — the client auto-approved it and the record states that as a fact.
        MachineryKind::PermissionRequested => ActivityState::Completed,
        // THE PAUSE IS WHAT THIS ROW'S STATE IS ABOUT, so both endings are `Completed`.
        //
        // The vendor's own outcome is kept on the record (`ToolStatus::Failed` for
        // `compact_result: "failed"`) and is what technical mode reads. It is not what the
        // CEO's state says, because `ActivityState::Failed` renders as a "△ failed" mark and
        // would tell him something of HIS went wrong. Nothing did: measured twice in
        // `cellT1`, a failed compaction attempt cost 1 ms and the turn answered normally
        // straight after. The two endings differ in WORDS instead — see `semantic_summary`.
        MachineryKind::Compaction => match row.status.as_ref() {
            Some(ToolStatus::InProgress) => ActivityState::Running,
            // An ending, of either kind. `None` cannot occur: every compaction record is
            // built with a status by `compaction_phase`.
            _ => ActivityState::Completed,
        },
        _ => match row.status.as_ref() {
            Some(ToolStatus::Pending) => ActivityState::Queued,
            Some(ToolStatus::InProgress) => ActivityState::Running,
            Some(ToolStatus::Completed) => ActivityState::Completed,
            Some(ToolStatus::Failed) => ActivityState::Failed,
            // An unrecognized vendor status is not a claim that the work finished
            // (`ToolStatus::is_terminal` says the same thing one level down)...
            Some(ToolStatus::Other(_)) => ActivityState::Unknown,
            // ...and 34 of 58 measured tool events carried no status at all.
            None => ActivityState::Unknown,
        },
    }
}

fn activity_type_of(row: &MachineryRecord, types: &HashMap<String, ActivityType>) -> ActivityType {
    match row.kind {
        MachineryKind::PermissionRequested => ActivityType::Approval,
        MachineryKind::ClientFsCall => {
            if row.title.starts_with("fs/write") {
                ActivityType::Patch
            } else {
                ActivityType::Read
            }
        }
        // Model reasoning. Kept, never rendered.
        MachineryKind::Thought => ActivityType::Other,
        // §12's `activityType` set is quoted verbatim in this file and has no member for
        // this, so the honest answer is `Other` rather than a member bent to fit —
        // `semantic_summary` gives the row its words from the KIND, not from the type.
        MachineryKind::Compaction => ActivityType::Other,
        // Every untyped vendor kind — including the Phase-2 `plan` and `usage_update` —
        // is one dim technical row, not a guess at what it meant (§1.4 G5).
        MachineryKind::Unknown => ActivityType::Other,
        MachineryKind::ToolCall => row
            .tool_call_id
            .as_ref()
            .and_then(|id| types.get(id))
            .copied()
            // No payload to classify from — most often because the Tier-B raw window has
            // been evicted (journal §2.4), which is an honest degrade: the row still
            // renders with its title, status and paths, just without a refined type.
            .unwrap_or(ActivityType::Other),
    }
}

/// A CEO-safe semantic line (§5.3's vocabulary), built from the TYPE alone.
///
/// Never the command, never the output, never a vendor string — those are technical-mode
/// content and live in [`ActivityDetail`]. Counting and pluralization (*"Read 8 files"*)
/// is the renderer's rollup over these rows; a single row says what a single row did.
fn semantic_summary(activity_type: ActivityType, row: &MachineryRecord) -> String {
    // ---- the compaction, in the CEO's words rather than the model's --------------------
    //
    // He is a non-technical CEO (`ceo-decisions.md` §13) and this is voice-first (§25), so
    // the sentence has to survive being SPOKEN: *"Rich is working, forty-four seconds. Making
    // room to keep going."* "Auto-compaction", "context window" and "summarizing the
    // transcript" are the model's vocabulary, not his, and none of them tell him the one
    // thing he needs, which is that the wait has a reason and an end.
    //
    // Rejected, and why: "Tidying up" (true of nothing in particular, and it sounds
    // optional); "Reorganizing" (a verb with no object — reorganizing WHAT?); "Summarizing
    // the conversation so far" (accurate, and it invites "did he lose it?", which is a
    // question about memory this sentence cannot answer in five words); "Freeing up memory"
    // (reads as the machine being short of RAM, which is false); "Almost done" (an estimate,
    // and the measured spread is 38 to 62 seconds — §22 forbids the guess).
    //
    // NO NUMBER OF ANY KIND LIVES HERE. The band already shows the turn's own elapsed clock
    // beside this line, and that number is measured. A percentage, an estimate or a countdown
    // would be invented — nothing on the wire says how far through a compaction is, and the
    // one frame that carries a duration (`compact_boundary`) arrives when it is already over.
    //
    // The tense is the row's state, so the line is true whenever it is read: it is happening,
    // or it happened.
    if row.kind == MachineryKind::Compaction {
        return match row.status.as_ref() {
            Some(ToolStatus::InProgress) => "Making room to keep going".to_string(),
            Some(ToolStatus::Failed) => "Kept going without making room".to_string(),
            // `compact_result: "success"`, and anything else the wire ever ends with.
            _ => "Made room to keep going".to_string(),
        };
    }
    let files = row.locations.len();
    match activity_type {
        ActivityType::Read if files > 1 => format!("Read {files} files"),
        ActivityType::Read => "Read a file".to_string(),
        ActivityType::Patch if files > 1 => format!("Edited {files} files"),
        ActivityType::Patch => "Edited a file".to_string(),
        ActivityType::Command => "Ran a command".to_string(),
        ActivityType::Search => "Searched".to_string(),
        ActivityType::Browser => "Used the web".to_string(),
        ActivityType::Image => "Viewed an image".to_string(),
        ActivityType::Environment => "Set up the environment".to_string(),
        ActivityType::Integration => "Used an integration".to_string(),
        ActivityType::Thread => "Updated a thread".to_string(),
        // WHAT ACTUALLY HAPPENED: a tool asked, the client answered by itself, nobody
        // decided. Every word is checked against that.
        //
        // "Answered" states what RichOS did and is not an authorization verb — unlike
        // "Approved"/"Allowed"/"Granted", each of which names a governance act and implies
        // an actor entitled to perform it. "a permission prompt" is the protocol's own
        // noun (`session/request_permission`), correct in a technical row and free of the
        // suggestion that a person was consulted. "automatically" is the load-bearing word:
        // it names the absence of a decision-maker, which is the fact the old string hid.
        //
        // Nothing here claims success, failure or breakage — the underlying tool call
        // reports its own outcome in its own row, and this one must not pre-empt it.
        // Rejected: "Requested approval" (the request was answered, not left open, and
        // "approval" names a decision nobody took); "Approved a tool automatically"
        // (Frank's option 2 — factual, but "Approved" still asserts an authorization);
        // "Auto-approved" (same objection, compressed); "Skipped a permission check"
        // (false — the check ran and was answered).
        //
        // This is a TECHNICAL row now (see `activity_row`), so it is written for someone
        // reading machinery, where the protocol noun is an asset rather than jargon.
        ActivityType::Approval => "Answered a permission prompt automatically".to_string(),
        ActivityType::Other => "Worked".to_string(),
    }
}

/// Resolve one activity type per tool call, across the RAW (pre-merge) records.
///
/// The classification lives on the OPENING frames — the tool's own `name`, which the native
/// wire supplies directly (`content_block_start.content_block.name`, the `assistant` frame's
/// `tool_use.name`, `tool_progress.tool_name`) with no `_meta` indirection. `merge_into`
/// keeps the LAST update's payload, and the last frame of a tool call is its `tool_result`,
/// which carries no name at all. First conclusive answer wins; a record with an evicted
/// payload contributes nothing.
fn resolve_activity_types(records: &[&MachineryRecord]) -> HashMap<String, ActivityType> {
    let mut out: HashMap<String, ActivityType> = HashMap::new();
    for r in records {
        let (Some(id), Some(payload)) = (r.tool_call_id.as_ref(), r.payload.as_ref()) else {
            continue;
        };
        if out.contains_key(id) {
            continue;
        }
        if let Some(t) = classify(payload) {
            out.insert(id.clone(), t);
        }
    }
    out
}

/// The latest `at` observed per tool call (or per record), across the raw events.
fn resolve_last_seen(records: &[&MachineryRecord]) -> HashMap<String, u64> {
    let mut out: HashMap<String, u64> = HashMap::new();
    for r in records {
        // `machinery::merge_key`, the SAME derivation the merge itself uses. Keying on
        // `tool_call_id` alone was correct while every mergeable row was a tool call and
        // became wrong the moment a compaction's key was the lease's: two compactions in one
        // thread share that key, so turn 3's row would have reported turn 4's `completedAt`.
        let key = crate::machinery::merge_key(r);
        let e = out.entry(key).or_insert(r.at);
        if r.at > *e {
            *e = r.at;
        }
    }
    out
}

/// Build the worker item for one `agent_id`, from the rows the stream actually holds for it
/// **in the given session**.
///
/// Returns `None` when the stream has no in-session row for this id — which is the honest
/// answer, not an empty worker: an id extracted from a tool result proves the harness
/// acknowledged a spawn, but this module reports only what the LIFECYCLE stream witnessed,
/// and deriving a second, parallel "created" here would be a competing source of truth.
/// **`pub(crate)` since 2026-08-29 so `live.rs` can make the SAME join during the turn**
/// rather than a second one that agrees by inspection. A live worker row and a reloaded
/// worker row are then the same row because they came out of the same function, not
/// because two implementations were compared.
pub(crate) fn worker_activity(
    agent_id: &str,
    session_id: &str,
    rows: &[WorkerEventRow],
) -> Option<WorkerActivityItem> {
    // CLAUSE 3 — see `project_with_workers`. Identity is the join key; the session is the
    // scope. Both are required, and neither is a name or a timestamp.
    let scope = SessionScope::Exact(session_id.to_string());
    let mine: Vec<&WorkerEventRow> =
        rows.iter().filter(|r| r.agent_id == agent_id && r.in_scope(&scope)).collect();
    let last = mine.last()?;

    let worker_name = mine.iter().rev().find(|r| !r.worker_name.is_empty()).map(|r| r.worker_name.clone());
    let agent_type = mine.iter().rev().find(|r| !r.agent_type.is_empty()).map(|r| r.agent_type.clone());
    let latest_update = mine
        .iter()
        .rev()
        .find(|r| r.lifecycle_state == ObservedWorkerState::Updated && !r.summary.is_empty())
        .map(|r| r.summary.clone());

    let observed_state = last.lifecycle_state;
    Some(WorkerActivityItem {
        agent_id: agent_id.to_string(),
        worker_name,
        agent_type,
        observed_state,
        // NEVER a bare match on Completed. The mapping is one named function and one named
        // constant; see `WorkerState::from_observed` / `RUN_ENDED_WORKER_STATE`.
        state: WorkerState::from_observed(observed_state),
        latest_update,
        first_observed_at: mine.first().map(|r| r.timestamp.clone()).filter(|t| !t.is_empty()),
        last_observed_at: Some(last.timestamp.clone()).filter(|t| !t.is_empty()),
        events_observed: mine.len(),
    })
}

/// The worker ROW, from the merged machinery record and the joined worker.
///
/// Takes `turn_id` + `internal_turn` rather than a `&Turn` (changed 2026-08-29) because
/// `live.rs` builds this row DURING the turn, where no `Turn` value is in hand — the spine
/// holds the ledger mutably for the whole stream. The two inputs it actually used are
/// exactly those two, so passing them directly costs nothing and lets the live path and the
/// reload path share one constructor instead of two that must be kept in step.
pub(crate) fn worker_activity_item(
    row: &MachineryRecord,
    entity: &EntityId,
    thread_id: &str,
    turn_id: &str,
    revision: u64,
    internal_turn: bool,
    worker: WorkerActivityItem,
) -> TimelineItem {
    // Same visibility rule as an activity row: internal traffic stays internal, and a
    // delegated worker inside a re-prime or rotation turn must never surface.
    let visibility = if row.internal || internal_turn { Visibility::Internal } else { Visibility::Ceo };
    TimelineItem::WorkerActivity {
        base: TimelineBase {
            id: row.machinery_id.clone(),
            entity_id: entity.clone(),
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            binding_revision: revision,
            created_at: row.at,
            updated_at: None,
            sequence: Some(row.seq),
            slot: TimelineSlot::Stream,
            visibility,
        },
        worker,
        detail_ref: Some(row.machinery_id.clone()),
        detail: Some(ActivityDetail {
            title: row.title.clone(),
            summary: row.summary.clone(),
            locations: row.locations.clone(),
            vendor_kind: None,
        }),
    }
}

/// The tool NAME per tool call, across the raw records — the memo that lets a name on the
/// opening frame reach a result frame that carries none. See [`extract_agent_id`].
fn resolve_tool_names(records: &[&MachineryRecord]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for r in records {
        let (Some(id), Some(payload)) = (r.tool_call_id.as_ref(), r.payload.as_ref()) else {
            continue;
        };
        if out.contains_key(id) {
            continue;
        }
        if let Some(name) = tool_name_of(payload) {
            out.insert(id.clone(), name.to_string());
        }
    }
    out
}

/// What each delegation call says it hands over and to whom, across the raw records.
///
/// Over the RAW records for the same reason as [`resolve_tool_names`]: the arguments ride on
/// the `assistant` frame's `tool_use` block, and `merge_into` keeps the last payload, which
/// is the `tool_result`'s. First conclusive answer wins. `live.rs` keeps the same memo.
fn resolve_delegations(records: &[&MachineryRecord]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for r in records {
        let (Some(id), Some(payload)) = (r.tool_call_id.as_ref(), r.payload.as_ref()) else {
            continue;
        };
        if out.contains_key(id) {
            continue;
        }
        if let Some(line) = delegation_summary(payload) {
            out.insert(id.clone(), line);
        }
    }
    out
}

/// The CEO line for a delegation, from the `Task`/`Agent` call's OWN arguments: what Rich
/// handed over (`description`, the call's short statement of the work) and to whom (`name`,
/// the teammate, shown as the worker row shows it; else `subagent_type`, the worker row's
/// fallback too). `None` when this payload is not a delegation or carries neither, as the
/// streamed open does (`input: {}`); the caller then falls back to [`delegation_line`].
/// Never the `prompt`: that is the whole brief, technical-mode content.
pub(crate) fn delegation_summary(payload: &Value) -> Option<String> {
    let name = tool_name_of(payload)?;
    if name != "Task" && name != "Agent" {
        return None;
    }
    let input = payload
        .get("input")
        .or_else(|| payload.get("event").and_then(|e| e.get("content_block")).and_then(|b| b.get("input")))?;
    let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let what = text("description").and_then(crate::machinery::summarize);
    let who = text("name").map(teammate_name).or_else(|| text("subagent_type").map(str::to_string));
    if what.is_none() && who.is_none() {
        return None;
    }
    Some(delegation_line(what.as_deref(), who.as_deref()))
}

/// "Handed to Mark: Fix the login bug". Past tense is true in every state the row can be in:
/// the handing over has happened the moment the call exists, whether the work is queued,
/// running or done.
fn delegation_line(what: Option<&str>, who: Option<&str>) -> String {
    let who = who.unwrap_or("a teammate");
    match what {
        Some(what) => format!("Handed to {who}: {what}"),
        None => format!("Handed work to {who}"),
    }
}

/// A teammate's display name, the SAME rule as `ui/timeline.js` `teammateName`, so this row
/// and the worker row that replaces it name one person one way: a `<role>-<model>-<id>`
/// spawn name (`/^([a-z][a-z0-9]{1,15})-[a-z]+-[a-z0-9]{1,12}$/`) shows as its capitalized
/// role ("mark-sonnet-f1" is "Mark"); anything else shows as it is.
fn teammate_name(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('-').collect();
    let lower_alnum = |s: &str| s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let fits = parts.len() == 3
        && (2..=16).contains(&parts[0].len())
        && parts[0].starts_with(|c: char| c.is_ascii_lowercase())
        && lower_alnum(parts[0])
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_lowercase())
        && (1..=12).contains(&parts[2].len())
        && lower_alnum(parts[2]);
    if !fits {
        return raw.to_string();
    }
    let mut chars = parts[0].chars();
    chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// The latest `agent_id` extractable per tool call, across the raw records.
///
/// TWO passes, because on this wire the identity witness spans two frames: the tool's name
/// arrives when the call opens and the async-launch acknowledgement when it closes.
fn resolve_agent_ids(records: &[&MachineryRecord]) -> HashMap<String, String> {
    let names = resolve_tool_names(records);
    let mut out: HashMap<String, String> = HashMap::new();
    for r in records {
        let (Some(id), Some(payload)) = (r.tool_call_id.as_ref(), r.payload.as_ref()) else {
            continue;
        };
        if out.contains_key(id) {
            continue;
        }
        if let Some(agent_id) = extract_agent_id(payload, names.get(id).map(String::as_str)) {
            out.insert(id.clone(), agent_id);
        }
    }
    out
}

/// Extract the spawned worker's `agentId` from a delegated-work tool call's raw payload.
///
/// The SAME two-part witness `worker-created-handoff.sh` requires, deliberately, so the two
/// sources cannot disagree about what counts as a spawn:
///
///   1. the harness's async-launch acknowledgement — this was a BACKGROUNDED spawn, not a
///      synchronous subagent run whose result had already returned; and
///   2. an extractable `agentId` — the join key every lifecycle event carries.
///
/// Missing either half yields `None`, and `None` means the call stays an ordinary activity.
/// Hand-rolled rather than a regex because this crate is deliberately dependency-light.
///
/// **`tool_name` is a separate argument now, and the wire is why.** On ACP both halves rode
/// on one `tool_call_update`: the name in `_meta` and the acknowledgement in `rawOutput`. On
/// the native wire the name is on the tool's OPENING frames and the acknowledgement is in the
/// `tool_result`, which carries no name — so a single payload can never hold both, and a
/// function that demanded both from one payload would answer `None` forever and silently
/// stop reporting workers. The caller resolves the name per `tool_call_id` first
/// ([`tool_name_of`]) and passes it in; `None` for the name is a hard refusal, exactly as an
/// unrecognized name is.
pub(crate) fn extract_agent_id(payload: &Value, tool_name: Option<&str>) -> Option<String> {
    // Only the vendor's delegated-work tools can spawn a worker. Anything else carrying
    // this text would be quoting it. The name may come from this payload (the open frame)
    // or from the caller's per-tool-call memo (the result frame).
    let name = tool_name.or_else(|| tool_name_of(payload))?;
    if name != "Task" && name != "Agent" {
        return None;
    }
    // Flatten: the harness has used more than one shape for a tool result, and a scan over
    // the flattened form survives all of them.
    let text = payload.to_string();
    if !text.contains("Async agent launched successfully") {
        return None;
    }
    let start = text.find("agentId:")? + "agentId:".len();
    let id: String = text[start..]
        .chars()
        .skip_while(|c| c.is_whitespace() || *c == '"' || *c == '\\')
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

/// The vendor's real tool name, wherever this wire puts it.
///
/// **Three places, because one tool call is three frames, and this is the ONE function that
/// knows them all.** ACP buried the real name in `_meta.claudeCode.toolName` and offered a
/// coarse `kind` (`execute` / `edit` / `read` / `other`) beside it; the native wire hands the
/// name over directly and has no coarse class at all, so the `kind` fallback is gone rather
/// than emulated.
///
/// - `event.content_block.name` — the streamed OPEN (`run9-rust-driven.jsonl:5`).
/// - `name` — the `assistant` frame's `tool_use` block, which this file's normalizer stores
///   as the record payload (`run9-rust-driven.jsonl:15`).
/// - `tool_name` — `tool_progress` heartbeats and `can_use_tool` permission requests.
///
/// A `tool_result` payload has NO name and answers `None`, which is why the classification
/// has to run over the RAW records before the merge.
pub(crate) fn tool_name_of(payload: &Value) -> Option<&str> {
    payload
        .get("event")
        .and_then(|e| e.get("content_block"))
        .and_then(|b| b.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("name").and_then(|v| v.as_str()))
        .or_else(|| payload.get("tool_name").and_then(|v| v.as_str()))
        .or_else(|| payload.get("request").and_then(|r| r.get("tool_name")).and_then(|v| v.as_str()))
}

/// Classify one raw tool payload by the tool's own name. Anything unrecognized returns
/// `None` rather than a guess, and `None` becomes `Other`.
pub(crate) fn classify(payload: &Value) -> Option<ActivityType> {
    match tool_name_of(payload)? {
        "Bash" | "BashOutput" | "KillShell" | "KillBash" => Some(ActivityType::Command),
        "Read" | "NotebookRead" => Some(ActivityType::Read),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => Some(ActivityType::Patch),
        "Glob" | "Grep" | "ToolSearch" => Some(ActivityType::Search),
        "WebSearch" => Some(ActivityType::Search),
        "WebFetch" => Some(ActivityType::Browser),
        // `Task` is the vendor's delegated-work tool and the ONE place a worker could be
        // inferred from. It is not: an activity row is a thing that happened, a worker run
        // is a lifecycle claim, and §22 forbids inventing the second.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entity() -> EntityId {
        EntityId::parse("femcboost").unwrap()
    }

    /// One record from one native frame, for the tests that need exactly one.
    fn record(seq: u64, frame: Value) -> MachineryRecord {
        let mut v = MachineryRecord::from_native_event(&frame, "sess", seq);
        assert_eq!(v.len(), 1, "expected exactly one record from {frame}");
        v.remove(0).stamp("thr", Some("turn_1"), false)
    }

    /// A tool row OPENED on the stream: `content_block_start` with a `tool_use` block. This
    /// is where the tool's real name lives on this wire.
    fn tool_open(seq: u64, id: &str, name: &str) -> MachineryRecord {
        record(
            seq,
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"tool_use","id":id,"name":name,"input":{}}}}),
        )
    }

    /// A tool row CLOSED by its `tool_result`. Carries no tool name — deliberately, because
    /// that is what the wire does.
    fn tool_result(seq: u64, id: &str, content: &str, is_error: bool) -> MachineryRecord {
        record(
            seq,
            json!({"type":"user","message":{"role":"user","content":[
                    {"tool_use_id":id,"type":"tool_result","is_error":is_error,"content":content}]}}),
        )
    }

    fn turn() -> Turn {
        // A completed CEO turn, streamed in two runs around one tool call at seq 1.
        Turn {
            id: "turn_1".into(),
            thread_id: "thr".into(),
            entity_id: Some(entity()),
            binding_revision: 3,
            quarantined: false,
            user_text: "how is the release".into(),
            source: Source::Text,
            state: TurnState::Completed,
            session_id: Some("sess".into()),
            started_sessions: vec!["sess".into()],
            assistant_text: "he said Xthen said Z".into(),
            text_runs: vec![
                crate::ledger::TextRun { start_seq: Some(0), end_seq: Some(0), text: "he said X".into(), at: 10 },
                crate::ledger::TextRun { start_seq: Some(2), end_seq: Some(2), text: "then said Z".into(), at: 30 },
            ],
            stop_reason: Some("end_turn".into()),
            created_at: 5,
            started_at: Some(8),
            ended_at: Some(19_360 + 8),
            tier: None,
            superseded_by: None,
            stop_requested_at: None,
            intake_id: None,
            upstream_failure: None,
            interruption: None,
            rich_audible: None,
            channel: None,
            client_id: None,
            voice_note: false,
            pressed: false,
        }
    }

    #[test]
    fn internal_visibility_renders_in_no_mode() {
        for mode in [ViewMode::Ceo, ViewMode::Technical] {
            assert!(!Visibility::Internal.renders_in(mode), "internal must never render, not even in {mode:?}");
        }
        assert!(Visibility::Ceo.renders_in(ViewMode::Ceo));
        assert!(Visibility::Ceo.renders_in(ViewMode::Technical));
        assert!(!Visibility::Technical.renders_in(ViewMode::Ceo));
        assert!(Visibility::Technical.renders_in(ViewMode::Technical));
    }

    #[test]
    fn a_turns_items_carry_the_full_entity_fence() {
        let t = turn();
        for item in turn_items(&t, &entity(), 3) {
            let b = item.base();
            assert_eq!(b.entity_id, entity(), "every record carries its entity — no leak by omission");
            assert_eq!(b.thread_id, "thr");
            assert_eq!(b.turn_id, "turn_1");
            assert_eq!(b.binding_revision, 3);
        }
    }

    #[test]
    fn an_interruption_says_whether_the_turn_carried_words_of_his() {
        // Nightly 67 p3: Rich's turn for Stu's status message was cut off, and the Mac drew
        // "Press Restore my message ... Your message is saved" for a message he never sent.
        let his_words = |source: Source, pressed: bool| {
            let mut t = turn();
            t.source = source;
            t.pressed = pressed;
            t.state = TurnState::Interrupted;
            t.interruption = Some(crate::interruption::InterruptionRecord {
                cause: "unknown".into(),
                ceo_message: "unused".into(),
                loss_message: None,
                offers_retry: true,
            });
            turn_items(&t, &entity(), 3).into_iter().find_map(|i| match i {
                TimelineItem::TurnInterruption { his_words, .. } => Some(his_words),
                _ => None,
            })
        };
        assert_eq!(his_words(Source::Text, false), Some(true), "his typed message");
        assert_eq!(his_words(Source::Agent, false), Some(false), "Stu's report to Rich");
        assert_eq!(his_words(Source::Text, true), Some(false), "a button he pressed");
    }

    #[test]
    fn every_ceo_turns_prose_is_phase_unknown_never_final() {
        let items = turn_items(&turn(), &entity(), 3);
        let phases: Vec<RichMessagePhase> = items
            .iter()
            .filter_map(|i| match i {
                TimelineItem::RichMessage { phase, .. } => Some(*phase),
                _ => None,
            })
            .collect();
        assert_eq!(phases, vec![RichMessagePhase::Unknown, RichMessagePhase::Unknown]);
        assert!(
            !phases.contains(&RichMessagePhase::Final),
            "the 'last run is the final answer' heuristic is false the moment Rich verifies \
             something after writing his conclusion"
        );
    }

    #[test]
    fn prose_is_split_at_the_gap_the_shared_counter_records() {
        let items = turn_items(&turn(), &entity(), 3);
        let runs: Vec<(Option<u64>, String)> = items
            .iter()
            .filter_map(|i| match i {
                TimelineItem::RichMessage { base, text, .. } => Some((base.sequence, text.clone())),
                _ => None,
            })
            .collect();
        // seq 1 is missing on purpose: a tool call occupied it. Two items, real positions.
        assert_eq!(runs, vec![(Some(0), "he said X".to_string()), (Some(2), "then said Z".to_string())]);
    }

    #[test]
    fn the_duration_row_reports_a_measured_span() {
        let items = turn_items(&turn(), &entity(), 3);
        let d = items
            .iter()
            .find_map(|i| match i {
                TimelineItem::WorkDuration { state, active_ms, .. } => Some((*state, *active_ms)),
                _ => None,
            })
            .expect("a turn always has a duration row");
        // 19_368 - 8 = 19_360ms = 19.36s, which §6.2 renders as `19s`.
        assert_eq!(d, (WorkState::Completed, Some(19_360)));
    }

    #[test]
    fn an_unfinished_turn_reports_no_duration_at_all() {
        let mut t = turn();
        t.state = TurnState::InFlight;
        t.ended_at = None;
        let items = turn_items(&t, &entity(), 3);
        let d = items.iter().find_map(|i| match i {
            TimelineItem::WorkDuration { state, active_ms, started_at, .. } => Some((*state, *active_ms, *started_at)),
            _ => None,
        });
        assert_eq!(
            d,
            Some((WorkState::Working, None, Some(8))),
            "started_at is real so a live view can tick from it; active_ms stays unknown so a \
             cold reopen cannot turn an overnight wait into a twelve-hour turn (§6.3)"
        );
    }

    #[test]
    fn a_silent_proactive_message_is_internal_not_merely_quiet() {
        let mut t = turn();
        t.source = Source::Proactive;
        t.user_text = String::new();
        t.tier = Some(AttentionTier::Silent);
        let items = turn_items(&t, &entity(), 3);
        let msg = items.iter().find(|i| matches!(i, TimelineItem::RichMessage { .. })).unwrap();
        assert_eq!(msg.visibility(), Visibility::Internal, "§5.1: Tier 3 never appears in the conversation");
        assert!(
            !items.iter().any(|i| matches!(i, TimelineItem::UserMessage { .. })),
            "a proactive turn has no CEO prompt, so it renders no empty bubble"
        );

        t.tier = Some(AttentionTier::Digest);
        let items = turn_items(&t, &entity(), 3);
        let msg = items.iter().find(|i| matches!(i, TimelineItem::RichMessage { .. })).unwrap();
        assert_eq!(msg.visibility(), Visibility::Ceo);
        match msg {
            TimelineItem::RichMessage { phase, tier, .. } => {
                assert_eq!(*phase, RichMessagePhase::Proactive, "this phase IS sourced — the ledger records it");
                assert_eq!(*tier, Some(AttentionTier::Digest));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn an_internal_turn_contributes_nothing_visible() {
        let mut t = turn();
        t.source = Source::Internal;
        t.user_text = "[re-prime]".into();
        for item in turn_items(&t, &entity(), 3) {
            assert_eq!(item.visibility(), Visibility::Internal, "re-prime traffic has no render path: {item:?}");
        }
    }

    #[test]
    fn a_superseded_turn_is_evidence_not_a_second_rendering() {
        let mut t = turn();
        t.state = TurnState::Interrupted;
        t.superseded_by = Some("turn_2".into());
        let items = turn_items(&t, &entity(), 3);
        for item in &items {
            assert_eq!(
                item.visibility(),
                Visibility::Internal,
                "a replayed turn must not put the CEO's prompt on screen twice: {item:?}"
            );
        }
        let rec = items.iter().find(|i| matches!(i, TimelineItem::Recovery { .. })).expect("recovery row");
        match rec {
            TimelineItem::Recovery { recovered_turn_id, replacement_turn_id, .. } => {
                assert_eq!(recovered_turn_id, "turn_1");
                assert_eq!(replacement_turn_id, "turn_2");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_failed_turn_shows_the_ceo_a_state_and_keeps_the_reason_technical() {
        let mut t = turn();
        t.state = TurnState::Interrupted;
        t.stop_reason = Some("interrupted: cognition io: broken pipe".into());
        let items = turn_items(&t, &entity(), 3);
        let err = items.iter().find(|i| matches!(i, TimelineItem::SystemError { .. })).unwrap();
        assert_eq!(err.visibility(), Visibility::Technical, "a stack-trace-ish reason is not CEO copy");
        let dur = items.iter().find(|i| matches!(i, TimelineItem::WorkDuration { .. })).unwrap();
        assert_eq!(dur.visibility(), Visibility::Ceo, "the FACT of the failure still reaches the CEO");
    }

    #[test]
    fn activity_state_never_invents_completion() {
        // A record with no status at all — on this wire, an `assistant` frame's complete
        // arguments, which carry no status because status is a POSITION here, not a string.
        let mut r = record(
            0,
            json!({"type":"assistant","message":{"role":"assistant","content":[
                    {"type":"tool_use","id":"t","name":"Bash","input":{"command":"x"}}]}}),
        );
        assert_eq!(r.status, None);
        assert_eq!(activity_state_of(&r), ActivityState::Unknown);
        // An unrecognized vendor status is not completion either.
        r.status = Some(ToolStatus::Other("quiesced".into()));
        assert_eq!(activity_state_of(&r), ActivityState::Unknown);
        // The four positions this wire produces, each through the frame that produces it.
        assert_eq!(activity_state_of(&tool_open(0, "t", "Bash")), ActivityState::Queued);
        let progress = record(
            0,
            json!({"type":"tool_progress","tool_use_id":"t-heartbeat-0","tool_name":"Bash",
                   "parent_tool_use_id":"t","elapsed_time_seconds":30,"heartbeat":true}),
        );
        assert_eq!(
            activity_state_of(&progress),
            ActivityState::Running,
            "`running` is REACHABLE on this wire for the first time — the ACP path never once \
             emitted in_progress (machinery.rs's own measurement)"
        );
        assert_eq!(activity_state_of(&tool_result(0, "t", "ok", false)), ActivityState::Completed);
        assert_eq!(activity_state_of(&tool_result(0, "t", "boom", true)), ActivityState::Failed);
    }

    #[test]
    fn the_activity_type_comes_from_the_opening_events_payload() {
        // The exact opening shape measured on the native wire (`run9-rust-driven.jsonl:5`).
        let open = tool_open(0, "toolu_A", "Bash");
        // ...and the closing `tool_result`, which carries NO tool name — the reason the type
        // must be resolved BEFORE the merge.
        let close = tool_result(3, "toolu_A", "1.0.0", false);
        assert_eq!(classify(close.payload.as_ref().unwrap()), None, "the closing payload cannot classify anything");

        let raw = vec![&open, &close];
        let types = resolve_activity_types(&raw);
        assert_eq!(types.get("toolu_A"), Some(&ActivityType::Command));

        let merged = crate::machinery::project(vec![open.clone(), close.clone()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(activity_type_of(&merged[0], &types), ActivityType::Command);
        // Without the pre-merge pass there is nothing left to classify from.
        assert_eq!(activity_type_of(&merged[0], &HashMap::new()), ActivityType::Other);
    }

    #[test]
    fn a_delegated_task_tool_call_with_no_joinable_identity_is_still_only_an_activity() {
        // CHANGED DELIBERATELY from slice 2a's
        // `a_delegated_task_tool_call_is_an_activity_and_never_a_worker`. That test pinned
        // an absolute — a Task call is NEVER a worker — which was correct only because no
        // lifecycle signal existed. The signal landed at d14bc54, so the invariant is now
        // sharper and conditional: a Task call becomes a worker row when, and only when, it
        // can be joined BY IDENTITY to that stream. This half pins the unjoinable case,
        // which is byte-for-byte the old behavior.
        let r = tool_open(0, "toolu_T", "Task");
        let types = resolve_activity_types(&[&r]);
        assert_eq!(activity_type_of(&r, &types), ActivityType::Other, "no worker type, no worker state, no guess");
        assert!(types.get("toolu_T").is_none(), "Task is deliberately unclassified");
        // No async-launch acknowledgement on this payload, so no identity, so no join.
        assert_eq!(resolve_agent_ids(&[&r]).get("toolu_T"), None);
        assert_eq!(extract_agent_id(r.payload.as_ref().unwrap(), Some("Task")), None);
    }

    fn wrow(state: &str, agent: &str, session: &str, extra: &str) -> WorkerEventRow {
        serde_json::from_str(&format!(
            r#"{{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"{state}","agent_id":"{agent}","agent_type":"sage","session_id":"{session}"{extra}}}"#
        ))
        .unwrap()
    }

    /// The tool result the harness returns for a BACKGROUNDED spawn — both halves of the
    /// witness the creation hook requires.
    fn task_ack(agent: &str) -> Value {
        json!({"block":{"tool_use_id":"toolu_T","type":"tool_result",
                        "content":format!("Async agent launched successfully. agentId: {agent}")},
               "tool_use_result":Value::Null})
    }

    #[test]
    fn the_identity_witness_requires_both_the_async_ack_and_an_agent_id() {
        // Exactly the creation hook's gate, so the two sources cannot disagree about what
        // counts as a spawn.
        assert_eq!(extract_agent_id(&task_ack("agt_sage_r3"), Some("Task")), Some("agt_sage_r3".to_string()));

        // A SYNCHRONOUS subagent run returns its finished result — no ack. Its PostToolUse
        // fires when the work is already over, so calling it a creation would announce a
        // live worker at the moment it stopped existing.
        let sync = json!({"block":{"tool_use_id":"toolu_T","type":"tool_result",
                                   "content":"Here is the finished result. agentId: agt_x"}});
        assert_eq!(extract_agent_id(&sync, Some("Task")), None, "no async ack, no creation");

        // An ack with no extractable id cannot be joined to anything.
        let no_id = json!({"block":{"tool_use_id":"toolu_T","type":"tool_result",
                                    "content":"Async agent launched successfully."}});
        assert_eq!(extract_agent_id(&no_id, Some("Task")), None);

        // And a different tool quoting the same string is not a spawn.
        assert_eq!(
            extract_agent_id(&task_ack("agt_x"), Some("Bash")),
            None,
            "only the delegated-work tools spawn workers"
        );

        // NEW, and it is the failure this wire made possible: the acknowledgement frame
        // carries NO tool name, so with no name from the caller's memo there is no witness.
        // A version that read the name only from the payload would answer None forever and
        // silently stop reporting every worker.
        assert_eq!(tool_name_of(&task_ack("agt_x")), None, "a tool_result names no tool");
        assert_eq!(extract_agent_id(&task_ack("agt_x"), None), None);
        // And the memo the real callers build supplies exactly that missing half.
        let open = tool_open(0, "toolu_T", "Task");
        let mut ack = record(1, json!({"type":"user","message":{"role":"user","content":[
            {"tool_use_id":"toolu_T","type":"tool_result",
             "content":"Async agent launched successfully. agentId: agt_sage_r3"}]}}));
        ack.turn_id = Some("turn_1".into());
        assert_eq!(
            resolve_agent_ids(&[&open, &ack]).get("toolu_T"),
            Some(&"agt_sage_r3".to_string()),
            "the two-frame witness resolves when the raw records are read together"
        );
    }

    #[test]
    fn a_worker_item_joins_by_identity_and_carries_only_witnessed_facts() {
        let rows = vec![
            wrow("created", "agt_sage_r3", "sess", r#","worker_name":"sage-opus-r3""#),
            wrow("started", "agt_sage_r3", "sess", ""),
            wrow("updated", "agt_sage_r3", "sess", r#","summary":"architecture package committed""#),
        ];
        let w = worker_activity("agt_sage_r3", "sess", &rows).unwrap();
        assert_eq!(w.agent_id, "agt_sage_r3");
        assert_eq!(w.worker_name.as_deref(), Some("sage-opus-r3"));
        assert_eq!(w.observed_state, ObservedWorkerState::Updated);
        assert_eq!(w.state, WorkerState::Running, "open run: started, no terminal event");
        assert_eq!(w.latest_update.as_deref(), Some("architecture package committed"));
        assert_eq!(w.events_observed, 3);
    }

    #[test]
    fn a_task_call_whose_agent_id_has_no_stream_row_stays_unjoined() {
        // An id extracted from a tool result is not a second source of truth. If the
        // lifecycle stream never witnessed this worker, this module reports nothing.
        assert!(worker_activity("agt_missing", "sess", &[]).is_none());
        let rows = vec![wrow("started", "agt_other", "sess", "")];
        assert!(worker_activity("agt_missing", "sess", &rows).is_none());
    }

    #[test]
    fn run_ended_maps_to_unknown_and_never_to_completed() {
        // THE COLLAPSE THIS SLICE EXISTS TO PREVENT, pinned at the type level.
        assert_eq!(RUN_ENDED_WORKER_STATE, WorkerState::Unknown);
        assert_ne!(RUN_ENDED_WORKER_STATE, WorkerState::Completed);
        assert_eq!(serde_json::to_string(&RUN_ENDED_WORKER_STATE).unwrap(), "\"unknown\"");
        assert_eq!(WorkerState::from_observed(ObservedWorkerState::RunEnded), RUN_ENDED_WORKER_STATE);
        assert_eq!(WorkerState::from_observed(ObservedWorkerState::Created), WorkerState::PendingInit);
        assert_eq!(WorkerState::from_observed(ObservedWorkerState::Started), WorkerState::Running);
        assert_eq!(WorkerState::from_observed(ObservedWorkerState::Updated), WorkerState::Running);

        // The three unwitnessed states are unreachable through the only mapping there is.
        for observed in [
            ObservedWorkerState::Created,
            ObservedWorkerState::Started,
            ObservedWorkerState::Updated,
            ObservedWorkerState::RunEnded,
        ] {
            let mapped = WorkerState::from_observed(observed);
            assert!(
                !matches!(mapped, WorkerState::Waiting | WorkerState::Interrupted | WorkerState::Failed | WorkerState::Completed),
                "{observed:?} must not map to an unwitnessed state, got {mapped:?}"
            );
        }

        // End to end through the join, including the serialized form a renderer sees.
        let rows = vec![
            wrow("started", "a1", "sess", r#","worker_name":"clark-sonnet-1""#),
            wrow("run_ended", "a1", "sess", ""),
        ];
        let w = worker_activity("a1", "sess", &rows).unwrap();
        assert_eq!(w.observed_state, ObservedWorkerState::RunEnded);
        assert_eq!(w.state, WorkerState::Unknown);
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains(r#""observedState":"run_ended""#), "{json}");
        assert!(json.contains(r#""state":"unknown""#), "{json}");
        for forbidden in ["completed", "failed", "interrupted", "waiting", "success"] {
            assert!(!json.contains(forbidden), "a run_ended worker must not serialize as {forbidden}: {json}");
        }
    }

    #[test]
    fn a_worker_item_never_carries_a_duration_or_an_outcome() {
        // §22 forbids faking elapsed active time; wall-clock spread between two log lines
        // is not active time. And no outcome field exists to be filled in with a guess.
        let rows = vec![wrow("started", "a1", "sess", ""), wrow("run_ended", "a1", "sess", "")];
        let json = serde_json::to_string(&worker_activity("a1", "sess", &rows).unwrap()).unwrap();
        for absent in ["durationMs", "activeMs", "elapsed", "resultRef", "errorRef", "completedAt"] {
            assert!(!json.contains(absent), "{absent} has no witness and must not appear: {json}");
        }
    }

    #[test]
    fn a_semantic_summary_never_carries_command_text() {
        let r = record(
            0,
            json!({"type":"assistant","message":{"role":"assistant","content":[
                    {"type":"tool_use","id":"t","name":"Bash",
                     "input":{"command":"rm -rf /tmp/secret-dir","file_path":"/tmp/secret-dir"}}]}}),
        );
        assert_eq!(r.title, "rm -rf /tmp/secret-dir", "the raw syntax IS on the record");
        let s = semantic_summary(ActivityType::Command, &r);
        assert_eq!(s, "Ran a command");
        assert!(!s.contains("rm -rf"), "§5.3: the CEO default is semantic, never raw syntax");
        assert_eq!(semantic_summary(ActivityType::Read, &r), "Read a file");
    }

    #[test]
    fn an_item_round_trips_through_json_in_the_shape_12_and_13_describe() {
        let items = turn_items(&turn(), &entity(), 3);
        let msg = items.iter().find(|i| matches!(i, TimelineItem::RichMessage { .. })).unwrap();
        let v = serde_json::to_value(msg).unwrap();
        assert_eq!(v["kind"], json!("rich_message"));
        assert_eq!(v["entityId"], json!("femcboost"));
        assert_eq!(v["threadId"], json!("thr"));
        assert_eq!(v["turnId"], json!("turn_1"));
        assert_eq!(v["bindingRevision"], json!(3));
        assert_eq!(v["sequence"], json!(0));
        assert_eq!(v["visibility"], json!("ceo"));
        assert_eq!(v["phase"], json!("unknown"), "serialized explicitly — an absent field invites `?? \"final\"`");
        let back: TimelineItem = serde_json::from_value(v).unwrap();
        assert_eq!(&back, msg);
    }

    #[test]
    fn an_unknown_sequence_is_null_and_never_zero() {
        let mut t = turn();
        t.text_runs = vec![crate::ledger::TextRun { start_seq: None, end_seq: None, text: "legacy".into(), at: 10 }];
        let items = turn_items(&t, &entity(), 3);
        let msg = items.iter().find(|i| matches!(i, TimelineItem::RichMessage { .. })).unwrap();
        let v = serde_json::to_value(msg).unwrap();
        assert_eq!(v["sequence"], json!(null));
        assert_ne!(v["sequence"], json!(0));
    }

    // =======================================================================================
    // THE COMPACTION ROW — what the CEO is told about a 38-62 second pause, and in what words.
    // Frames verbatim from `docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl`.
    // =======================================================================================

    fn compaction_row(phase: &str) -> MachineryRecord {
        let frame = match phase {
            "started" => json!({"type":"system","subtype":"status","status":"compacting",
                                "session_id":"0fee2d41","uuid":"6bb40a60"}),
            "success" => json!({"type":"system","subtype":"status","status":Value::Null,
                                "compact_result":"success","session_id":"0fee2d41","uuid":"9e1cfd6b"}),
            _ => json!({"type":"system","subtype":"status","status":Value::Null,
                        "compact_result":"failed","compact_error":"too_few_groups",
                        "session_id":"0fee2d41","uuid":"c27400bb"}),
        };
        record(0, frame)
    }

    fn compaction_item(phase: &str, internal_turn: bool) -> TimelineItem {
        let row = compaction_row(phase);
        activity_item(&row, &entity(), "thr", "turn_1", 3, internal_turn, &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new())
    }

    #[test]
    fn the_compaction_pause_reaches_the_ceo_in_his_words_and_says_no_number() {
        for (phase, expected, state) in [
            ("started", "Making room to keep going", ActivityState::Running),
            ("success", "Made room to keep going", ActivityState::Completed),
            // THE FAILED ATTEMPT ENDS THE PAUSE TOO, and the CEO's state says exactly that.
            // A `failed` state would draw the "△ failed" mark for something that cost him
            // nothing — measured twice in `cellT1`, an abandoned attempt took 1 ms and the
            // turn answered normally straight after. The vendor's outcome survives on the
            // record (`ToolStatus::Failed`) and in the words below.
            ("failed", "Kept going without making room", ActivityState::Completed),
        ] {
            let TimelineItem::Activity { base, summary, state: got, .. } = compaction_item(phase, false) else {
                panic!("a compaction projects as an activity row");
            };
            assert_eq!(summary, expected, "{phase}");
            assert_eq!(got, state, "{phase}");
            assert_eq!(base.visibility, Visibility::Ceo, "{phase}: silence is the defect, not the calm");
            // NO PERCENTAGE, NO ESTIMATE, NO COUNTDOWN. The measured spread is 38.1 to 62.0
            // seconds over 16 boundaries and nothing on the wire says how far through it is.
            assert!(
                !summary.chars().any(|c| c.is_ascii_digit()) && !summary.contains('%'),
                "{phase}: a number here would be invented — got {summary:?}"
            );
        }
    }

    /// THE FRAMES OF THE 2026-10-21 SCREENSHOTS, by name, never in the user's own view.
    ///
    /// A new user's failing first turn showed `control_request` and `system:init` as rows and
    /// `system:hook_started` / `system:hook_response` under "Between turns". Those are drawn
    /// only by the Technical view; this pins that the projection's own rule keeps every record
    /// those exact frames produce out of the user's view (`ViewMode::Ceo`), in a turn and
    /// between turns, while the Technical view still has them (the positive control). The
    /// failure class: docs/development/failure-classes/machinery-in-the-user-view.md.
    #[test]
    fn the_incident_frames_never_reach_the_users_own_view() {
        use crate::machinery::MachineryRecord;
        let frames = [
            json!({"type":"control_request","request_id":"r1","request":{"subtype":"hook_callback",
                   "callback_id":"richos-input-context","input":{"hook_event_name":"UserPromptSubmit","prompt":"what do I do now?"}}}),
            json!({"type":"system","subtype":"init","model":"claude-opus-5","session_id":"s"}),
            json!({"type":"system","subtype":"hook_started","hook_name":"SessionStart"}),
            json!({"type":"system","subtype":"hook_response","hook_name":"SessionStart"}),
        ];
        let mut seen = 0;
        for frame in &frames {
            let in_turn = MachineryRecord::from_native_event(frame, "s", 1);
            let between = MachineryRecord::from_native_between_turn(frame, "s", 1);
            for row in in_turn.iter().chain(between.iter()) {
                seen += 1;
                let v = machinery_visibility(row, false);
                assert!(!v.renders_in(ViewMode::Ceo), "{} reaches the user's own view as {v:?}", row.title);
                assert!(v.renders_in(ViewMode::Technical) || v == Visibility::Internal, "{}: {v:?}", row.title);
            }
        }
        assert!(seen >= frames.len(), "only {seen} records from {} frames: the premise moved", frames.len());
    }

    #[test]
    fn a_tool_call_that_classifies_to_nothing_is_not_a_worked_row_in_his_view() {
        // Product hunt 2026-10-15, finding 13: "Worked" under "Worked for 15s", "Worked
        // queued" while Rich was still working, and "Rich is working / Worked" in the band.
        // An unclassified tool call (a vendor tool with no semantic type) is technical.
        let row = tool_open(0, "toolu_X", "SomeVendorTool");
        let TimelineItem::Activity { base, summary, .. } =
            activity_item(&row, &entity(), "thr", "turn_1", 3, false, &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new())
        else {
            panic!("a tool call projects as an activity row");
        };
        assert_eq!(summary, "Worked", "the record keeps what it is");
        assert_eq!(base.visibility, Visibility::Technical, "a row that only says Worked is not in his view");

        // A classified call is untouched: it says what Rich did.
        let mut types = HashMap::new();
        types.insert("toolu_X".to_string(), ActivityType::Command);
        let TimelineItem::Activity { base, summary, .. } =
            activity_item(&row, &entity(), "thr", "turn_1", 3, false, &types, &HashMap::new(), &HashMap::new(), &HashMap::new())
        else {
            panic!("a tool call projects as an activity row");
        };
        assert_eq!(summary, "Ran a command");
        assert_eq!(base.visibility, Visibility::Ceo);

        // A delegation classifies to nothing too, and stays in his view: Rich handing work to
        // a teammate, before its worker row exists or when it never becomes one.
        for name in ["Task", "Agent"] {
            let delegation = tool_open(0, "toolu_D", name);
            let names = HashMap::from([("toolu_D".to_string(), name.to_string())]);
            let TimelineItem::Activity { base, summary, .. } =
                activity_item(&delegation, &entity(), "thr", "turn_1", 3, false, &HashMap::new(), &names, &HashMap::new(), &HashMap::new())
            else {
                panic!("a delegation without its worker projects as an activity row");
            };
            assert_eq!(base.visibility, Visibility::Ceo, "a {name} delegation is in his view");
            // Before its arguments arrive it still names the act, never a bare "Worked".
            assert_eq!(summary, "Handed work to a teammate");
        }
    }

    #[test]
    fn a_delegation_line_names_the_work_and_the_teammate_from_the_call_itself() {
        let call = |input: Value| json!({"type":"tool_use","id":"toolu_D","name":"Agent","input":input});
        assert_eq!(
            delegation_summary(&call(json!({"description":"Fix the login bug","name":"mark-sonnet-f1"}))).as_deref(),
            Some("Handed to Mark: Fix the login bug")
        );
        assert_eq!(
            delegation_summary(&call(json!({"name":"mark-sonnet-f1","prompt":"the whole brief"}))).as_deref(),
            Some("Handed work to Mark"),
            "never the prompt"
        );
        assert_eq!(delegation_summary(&call(json!({}))), None, "the streamed open says nothing yet");
        let bash = json!({"type":"tool_use","id":"toolu_B","name":"Bash","input":{"description":"List files"}});
        assert_eq!(delegation_summary(&bash), None, "only a delegation is one");
        // The same rule as ui/timeline.js `teammateName`.
        assert_eq!(teammate_name("sage-opus-r3"), "Sage");
        assert_eq!(teammate_name("Clark"), "Clark");
        assert_eq!(teammate_name("a-opus-r3"), "a-opus-r3", "a one-letter role is not a spawn name");
    }

    #[test]
    fn a_compaction_inside_an_internal_turn_is_still_invisible() {
        // A re-prime or rotation turn compacting is machinery about machinery. The CEO is
        // never told that a rotation happened (continuity §2/§5), and the whole-turn demotion
        // has to hold over this row exactly as it holds over every other.
        let TimelineItem::Activity { base, .. } = compaction_item("started", true) else {
            panic!("a compaction projects as an activity row");
        };
        assert_eq!(base.visibility, Visibility::Internal);

        // Same for a record the spine marked internal on a CEO turn.
        let mut row = compaction_row("started");
        row.internal = true;
        assert_eq!(machinery_visibility(&row, false), Visibility::Internal);
    }

    #[test]
    fn a_compaction_row_carries_its_raw_frame_for_technical_mode_and_loses_it_for_the_ceo() {
        let item = compaction_item("failed", false);
        let TimelineItem::Activity { detail, .. } = &item else { panic!("activity") };
        let detail = detail.as_ref().expect("the technical half is retained");
        assert_eq!(detail.title, "system:status", "technical mode still sees the vendor's own name");
        // The CEO's copy has the bytes REMOVED, not flagged — the same rule every activity
        // row follows.
        let TimelineItem::Activity { detail, summary, .. } = item.redacted() else { panic!("activity") };
        assert!(detail.is_none());
        assert_eq!(summary, "Kept going without making room", "the words survive redaction; the frame does not");
    }


    #[test]
    fn the_compaction_payload_the_webview_receives_is_printed_here_in_full() {
        // The JS suite `app/ui/tests/compaction-notice.js` drives the renderer with this
        // payload. Printing it from the crate is what stops the two copies drifting: run
        // `cargo test -p richos-core --lib the_compaction_payload -- --nocapture` and the
        // suite's fixture is the output.
        for phase in ["started", "success", "failed"] {
            let item = compaction_item(phase, false).redacted();
            let json = serde_json::to_value(&item).unwrap();
            println!("{phase}: {json}");
            assert_eq!(json["kind"], "activity");
            assert_eq!(json["visibility"], "ceo");
            assert_eq!(json["activityType"], "other");
            assert_eq!(json["state"], if phase == "started" { "running" } else { "completed" });
            assert_eq!(
                json["summary"],
                match phase {
                    "started" => "Making room to keep going",
                    "success" => "Made room to keep going",
                    _ => "Kept going without making room",
                }
            );
            assert!(
                json.get("detail").map_or(true, |d| d.is_null()),
                "the CEO's copy carries no raw frame"
            );
        }
    }


    #[test]
    fn two_compactions_in_one_thread_do_not_share_a_completion_time() {
        // THE BUG THIS EXISTS FOR, found by reading rather than by a failure: `resolve_last_seen`
        // keyed on `tool_call_id` alone, which is correct while every mergeable row is a tool
        // call (a vendor id is unique across the session) and WRONG the moment a compaction's
        // key is the lease's. Two compactions in one thread share that key, so the first row
        // would have reported the second one's instant as its own `completedAt` — 89 seconds
        // late in `cellT1`'s real spacing.
        let mut first = compaction_row("success");
        first.turn_id = Some("t3".into());
        first.at = 48_805;
        let mut second = compaction_row("success");
        second.turn_id = Some("t4".into());
        second.at = 90_231;

        let last_seen = resolve_last_seen(&[&first, &second]);
        // The map itself, first: two spans are two entries. With one entry both rows read the
        // later instant, and the assertion below would pass for the wrong reason — the lookup
        // would simply miss and fall back to the row's own `at`.
        assert_eq!(last_seen.len(), 2, "two compactions are two keys: {last_seen:?}");
        assert_eq!(last_seen.get(&crate::machinery::merge_key(&first)), Some(&48_805));
        assert_eq!(last_seen.get(&crate::machinery::merge_key(&second)), Some(&90_231));

        let item = activity_item(&first, &entity(), "thr", "t3", 3, false, &HashMap::new(), &HashMap::new(), &HashMap::new(), &last_seen);
        let TimelineItem::Activity { completed_at, base, .. } = item else { panic!("activity") };
        assert_eq!(completed_at, Some(48_805), "turn 3's pause ended when turn 3's pause ended");
        assert_eq!(base.updated_at, None, "and nothing later touched that row");
    }

    // THE PACE A BAR IS DRAWN AT - the CEO's design, 2026-09-06: *"It doesn't need to know.
    // It just needs to move to almost full with the expected minimum time and then stay at
    // 'almost full' until all finished etc."*
    //
    // The wire carries the SPAN and never a position, a percentage or a remaining time. The
    // renderer owns where a bar is; the crate owns the only measured number it is paced on.

    #[test]
    fn a_compaction_carries_the_measured_floor_and_every_other_row_carries_none() {
        for phase in ["started", "success", "failed"] {
            let TimelineItem::Activity { measured_min_ms, .. } = compaction_item(phase, false) else {
                panic!("a compaction projects as an activity row");
            };
            assert_eq!(
                measured_min_ms,
                Some(crate::machinery::COMPACTION_MEASURED_MIN_MS),
                "a compaction is paced at its measured floor in every phase, {phase} included - \
                 an ending that dropped the field would make a bar vanish at the instant it \
                 should complete"
            );
        }

        // Every other row. A tool call is the one that matters: it is the other long wait on
        // this surface, its committed population runs from milliseconds to minutes, and a bar
        // over it would be invention rather than measurement.
        let mut tool = compaction_row("started");
        tool.kind = MachineryKind::ToolCall;
        tool.tool_call_id = Some("toolu_A".into());
        let TimelineItem::Activity { measured_min_ms, .. } =
            activity_item(&tool, &entity(), "thr", "turn_1", 3, false, &HashMap::new(), &HashMap::new(), &HashMap::new(), &HashMap::new())
        else {
            panic!("activity");
        };
        assert_eq!(measured_min_ms, None, "nothing but a compaction has a measured floor");
    }

    #[test]
    fn the_measured_floor_is_the_shortest_boundary_ever_recorded_and_is_never_spoken() {
        // The constant's own derivation lives beside it in `machinery.rs` and is re-run
        // against the committed frames by `app/ui/tests/compaction-progress.js`. What is
        // asserted HERE is the pair of properties the crate is responsible for.
        //
        // 1. It is the minimum of the measured population and not its mean (49865) or its
        //    maximum (62029). Paced on the mean, half of all measured compactions would see
        //    the bar jump forward at the end from wherever it had got to; paced on the
        //    minimum it climbs for the first 38.1 s of every one of them and holds after.
        assert_eq!(crate::machinery::COMPACTION_MEASURED_MIN_MS, 38_138);

        // 2. It never becomes a sentence. The summary is the CEO's whole reading of a
        //    compaction, and it says no number of any kind - not this one, not a percentage,
        //    not a countdown.
        for phase in ["started", "success", "failed"] {
            let TimelineItem::Activity { summary, .. } = compaction_item(phase, false) else {
                panic!("activity");
            };
            assert!(
                !summary.chars().any(|c| c.is_ascii_digit()),
                "the compaction sentence says a number: {summary:?}"
            );
        }
    }

}
