//! The ADDITIVE live-work event family — §13 of the Codex-inspired conversation UX brief
//! (`docs/design/richos-codex-inspired-conversation-ux-2026-08-28.md`), slice 3 of §24.
//!
//! §13's instruction is *"Keep the existing events for backward compatibility. Add typed
//! events."* So this is a SECOND family beside `stream.rs`, exactly as `machinery.rs` is a
//! second family beside it. Nothing in `stream.rs` changes: the four events
//! `app/STREAMING.md` documents keep their names, their payloads, their ordering and their
//! `seq`, and the shipping UI keeps working untouched. A consumer of the new family gets
//! strictly more; a consumer of the old family loses nothing.
//!
//! ## What is emitted, and what is deferred
//!
//! §13 lists eleven events. Seven have a real source today and are emitted. Four do not and
//! are NOT emitted — §22's rule is *"if the source signal does not exist, build the signal
//! first or show unknown"*, and an event with no source cannot even show unknown, because
//! its very arrival would be the claim.
//!
//! | §13 event | Here |
//! |---|---|
//! | `rich://turn-status` | **LIVE** — the ledger's own turn-state transitions |
//! | `rich://message-started` | **LIVE** — opens one contiguous run of Rich's prose |
//! | `rich://message-delta` | **LIVE** — one persisted assistant delta |
//! | `rich://message-completed` | **LIVE** — closes that run, carrying its full text |
//! | `rich://activity-upserted` | **LIVE** — one merged machinery row, CEO-safe |
//! | `rich://thread-summary-updated` | **LIVE** — title, count, recency, operational status |
//! | `rich://worker-upserted` | **LIVE** since 2026-08-29 — see below |
//! | `rich://plan-updated` | **DEFERRED** — plan entries live only in the evictable raw payload |
//! | `rich://approval-requested` | **DEFERRED** — nothing asks the CEO to approve anything |
//! | `rich://approval-resolved` | **DEFERRED** — same |
//! | `rich://artifact-upserted` | **DEFERRED** — Phase 5 owns artifacts and provenance |
//!
//! The deferred four have no constant in this module ON PURPOSE. A named constant is an
//! invitation; there is nothing here to emit them with.
//!
//! ## `rich://worker-upserted` — deferred in slice 3, emitted from 2026-08-29
//!
//! It was deferred for a reason that no longer holds: when this family was written there
//! was no worker lifecycle signal anywhere. There is now — the engine's four emitters write
//! `worker-events.jsonl` (`engine/docs/worker-lifecycle-events.md`), `worker_events.rs`
//! parses it, and `timeline.rs` already joined a `Task` tool call to it BY IDENTITY for the
//! reload path. Until this commit that join happened only on `get_timeline`, so a
//! delegation reached the screen after a snapshot read and showed as a nameless *"Worked"*
//! row during the turn — exactly when the CEO wants to know Rich has delegated. Measured by
//! the §26 fixture: 0 chips live, 3 after the snapshot.
//!
//! What is emitted is the SAME row the reload projects, because it is built by the SAME two
//! functions: [`crate::timeline::worker_activity`] (the join, with its session clause) and
//! [`crate::timeline::worker_activity_item`] (the row). Not a second implementation that
//! agrees by inspection.
//!
//! THREE THINGS THIS EVENT DELIBERATELY DOES NOT CARRY:
//!
//! 1. **No `completed` / `failed` / `reason` field.** A run that ends arrives as
//!    `run_ended` with the reason genuinely unobservable, which
//!    [`crate::timeline::RUN_ENDED_WORKER_STATE`] maps to [`WorkerState::Unknown`]. The
//!    live path says the same word the reload path says — *Ended, outcome not recorded* —
//!    because a row that changes meaning when the turn completes is worse than a row that
//!    arrived late.
//! 2. **No `waiting`, `interrupted` or `failed` state.** Three of §7.1's seven states have
//!    no witness at all (`worker_events.rs`), and nothing here invents one.
//! 3. **No poll and no timer.** See [`LiveTurn::on_machinery`] for what that costs.
//!
//! AND ONE THING THIS EVENT INHERITS RATHER THAN FIXES. The join's session clause compares
//! the session id the lease is running under against the `session_id` the engine hook read from
//! the Claude Code harness, and **whether those are the same id space has never been
//! measured** — the adapter is not installed in this checkout
//! ([`crate::spine::Spine::set_worker_events`] states it). If they differ, every row is
//! refused, every `Task` call stays an ordinary activity row, and the behavior is exactly
//! what it was before this event existed — safe, but silent.
//!
//! The reload path REPORTS that case as
//! [`crate::timeline::RejectionReason::WorkerSessionMismatch`]; this family has no rejection
//! channel, so the live path cannot. That is a real asymmetry and it is stated here rather
//! than discovered later. The diagnostic is not lost — the shell reads `Spine::timeline` on
//! every thread open — but nobody should read a live turn with no chips as proof that Rich
//! did not delegate.
//!
//! ## THE MESSAGE PHASE, STATED LOUDLY
//!
//! **Neither wire distinguishes commentary from the final response, so every
//! streamed message is emitted with `phase: "unknown"`.** This is measured, not assumed:
//! `docs/verification/acp-emission-probe-2026-08-28.md` §2 records the complete union of
//! inbound traffic across five runs — 52 `agent_message_chunk`s and **no** message-open,
//! message-close, or message-role update of any kind. `native.rs`'s `dispatch` confirms the client
//! side: every `agent_message_chunk` becomes `ChunkMsg::Text(text)` and nothing else on
//! the wire carries a phase.
//!
//! `rich://message-started` was supposed to be where the phase gets determined, and it is
//! also the point that proves it cannot be. It fires the instant the first delta of a run
//! is persisted — before the turn is over, before it is known whether Rich will run one
//! more command after this paragraph. At that instant no information about "final" exists
//! anywhere in the process. The tempting fallback, *"the last run of a completed turn is
//! the final answer"*, is (a) unavailable live, and (b) false even after the fact: Rich
//! routinely verifies something after writing his conclusion, which makes the last run a
//! two-word confirmation and the deliverable the run before it. **The CEO reads the final
//! response as the deliverable, so mislabelling commentary as final is a product defect,
//! not a cosmetic one.** [`STREAMED_MESSAGE_PHASE`] is therefore a named constant with a
//! test on it, so that "final" can never quietly become the default.
//!
//! ONE phase is real and is emitted as itself: [`RichMessagePhase::Proactive`]. The ledger
//! records `Source::Proactive` and its tier when Rich speaks unprompted, so a proactive
//! message knows what it is. That is the whole sourced set.
//!
//! **This is a gap in §13's premise, not in its mechanism.** Closing it needs a signal
//! that does not exist in the adapter today.
//!
//! ## The three properties enforced here rather than documented
//!
//! **1. The full ECS fence rides on every payload.** [`EventFence`] carries `entityId`,
//! `threadId`, `turnId` and `bindingRevision`, and its only constructor takes a
//! [`ThreadBinding`] — whose own constructor is crate-private to `Ledger` (`entity.rs`).
//! A fence cannot be assembled from a loose thread id, so an event cannot be emitted into
//! a scope nobody verified. §13: *"The renderer rejects events that do not match the
//! immutable binding."*
//!
//! **2. The visibility gate survives to the wire, and it is a gate.** Every event answers
//! [`LiveEvent::visibility`], and [`LiveEvent::may_reach_webview`] is false for anything
//! that is not [`Visibility::Ceo`]. THIS FAMILY IS THE CALM FAMILY: an internal item
//! (re-prime, rotation, a silent proactive message, model reasoning) never reaches it, and
//! neither does technical detail — which already has its own family, `rich://machinery`,
//! that the calm view does not subscribe to. So a renderer of THIS family cannot leak a
//! command it was never handed, in the same structural way `STREAMING.md`'s clean-output
//! guarantee works. `visibility` is still carried on the wire, because the renderer needs
//! the field and an absent field invites a default.
//!
//! **3. A live id equals the id a reload projects.** `rich://message-*` uses
//! `{turn_id}:text:{run_index}` and `rich://activity-upserted` uses the merged record's
//! `machinery_id` — the SAME ids [`crate::timeline::Timeline::project`] derives from the
//! same durable records. So §13's *"repeated event IDs are idempotent"* holds across a
//! restart: a cold reopen re-states the items the CEO already saw, it does not duplicate
//! them. `tests/live_event_tests.rs` asserts the emitted activity payload is byte-equal to
//! the projected one.
//!
//! ## Ordering, and why every emission is downstream of a durable write
//!
//! §13's ordering rules require that terminal turn state and text deltas are persisted
//! BEFORE emission. Every constructor here is called from `spine.rs` after the
//! corresponding ledger write has returned, and several read their content back OUT of the
//! ledger rather than from the value in hand (`text_runs`, `active_ms`) — so what goes on
//! the wire is what survived, not what was intended.

use crate::entity::{EntityId, ThreadBinding};
use crate::ledger::{AttentionTier, Source};
use crate::machinery::MachineryRecord;
use crate::timeline::{self, ActivityType, RichMessagePhase, TimelineItem, Visibility, WorkerActivityItem};
use crate::worker_events::WorkerEventRow;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Tauri event names. Constants so the Rust emitter and `app/STREAMING.md` cannot drift.
pub const EVENT_TURN_STATUS: &str = "rich://turn-status";
pub const EVENT_MESSAGE_STARTED: &str = "rich://message-started";
pub const EVENT_MESSAGE_DELTA: &str = "rich://message-delta";
pub const EVENT_MESSAGE_COMPLETED: &str = "rich://message-completed";
pub const EVENT_ACTIVITY_UPSERTED: &str = "rich://activity-upserted";
pub const EVENT_WORKER_UPSERTED: &str = "rich://worker-upserted";
pub const EVENT_THREAD_SUMMARY_UPDATED: &str = "rich://thread-summary-updated";
/// **The CEO's own message, live** — added 2026-09-19 for `esc-20260919T003541Z-6885f74b`.
/// See [`LiveEvent::CeoMessage`] for why this family had no event for the one thing the CEO
/// himself puts on the timeline.
pub const EVENT_CEO_MESSAGE: &str = "rich://ceo-message";

/// The phase of every STREAMED Rich message. See the module doc: the stream carries no
/// signal that separates commentary (§5.2) from the final response (§5.4), so this is
/// `Unknown` and a renderer must wait for a real phase rather than read a default.
///
/// A named constant rather than a literal, so that changing it is a deliberate act with a
/// test in the way (`streamed_prose_is_never_labelled_final`).
pub const STREAMED_MESSAGE_PHASE: RichMessagePhase = RichMessagePhase::Unknown;

// ---------------------------------------------------------------------------
// THE FENCE
// ---------------------------------------------------------------------------

/// The ECS scope every §13 event must carry: `entityId`, `threadId`, `turnId`,
/// `bindingRevision`.
///
/// Fields are private and the only constructor takes a verified [`ThreadBinding`]. That is
/// the point: `ThreadBinding::new` is crate-private to the ledger, so a caller cannot
/// conjure a scope, and an emitter cannot stamp an event with an entity nobody checked.
/// Slice 1 proved the leak class is live (the re-prime digest was assembling every
/// entity's actions into every session) and slice 2a proved a merged row can look
/// perfectly scoped while carrying another entity's content — stamping from the binding is
/// what makes such a leak invisible, so the binding has to be the real one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFence {
    entity_id: EntityId,
    thread_id: String,
    turn_id: String,
    binding_revision: u64,
}

impl EventFence {
    /// The ONE constructor. `pub(crate)` so only the spine can mint a fence, from a
    /// binding the ledger issued.
    pub(crate) fn for_turn(binding: &ThreadBinding, turn_id: &str) -> Self {
        EventFence {
            entity_id: binding.entity_id().clone(),
            thread_id: binding.thread_id().to_string(),
            turn_id: turn_id.to_string(),
            binding_revision: binding.binding_revision(),
        }
    }

    pub fn entity_id(&self) -> &EntityId {
        &self.entity_id
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn binding_revision(&self) -> u64 {
        self.binding_revision
    }

    fn write_into(&self, map: &mut Map<String, Value>) {
        map.insert("entityId".into(), json!(self.entity_id.as_str()));
        map.insert("threadId".into(), json!(self.thread_id));
        map.insert("turnId".into(), json!(self.turn_id));
        map.insert("bindingRevision".into(), json!(self.binding_revision));
    }
}

// ---------------------------------------------------------------------------
// TURN STATUS (§11)
// ---------------------------------------------------------------------------

/// The subset of §11's turn-state model that this runtime can actually observe.
///
/// §11 names ten states. Five have no source and are absent from this enum rather than
/// modelled-and-never-produced, because a turn status is a claim the renderer acts on
/// immediately:
///
/// | §11 state | Why it is not here |
/// |---|---|
/// | `draft` | A draft is UI state before a durable turn exists. Nothing to emit. |
/// | `streaming_final` | Requires knowing a message is FINAL — the signal that does not exist. |
/// | `waiting_for_user` | §9.4 does not exist; nothing can put a turn in it. |
/// | `stopping` / `stopped` | §9.3's stop control does not exist; nothing can request one. |
///
/// [`TurnStatus::Recovering`] IS here and IS sourced: it is emitted only when a POSITIVE
/// termination signal fired mid-turn (`prompt()` returned `Err` — never inferred from
/// silence, continuity §5.2) and an automatic replay is about to be attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStatus {
    /// Journaled `received`; not yet handed to a lease.
    Queued,
    /// Handed to a lease (`mark_turn_started`).
    Working,
    /// A mid-turn crash whose automatic replay is starting. The turn will be superseded.
    Recovering,
    Completed,
    /// Terminal failure with no automatic replay attempt left.
    Failed,
    /// Terminal, and ATTRIBUTED: the CEO stopped it (§9.3). Emitted only after
    /// `Ledger::stop_turn`, which is itself reachable only from a durable stop request.
    /// There is no path from a crash to this status.
    Stopped,
}

impl TurnStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnStatus::Queued => "queued",
            TurnStatus::Working => "working",
            TurnStatus::Recovering => "recovering",
            TurnStatus::Completed => "completed",
            TurnStatus::Failed => "failed",
            TurnStatus::Stopped => "stopped",
        }
    }
}

/// §3.2's sidebar status, restricted to what is observable.
///
/// `waiting_for_ceo`, `completed_while_away` and `archived` are omitted: the first has no
/// state to be in, the second needs a per-thread SEEN marker that does not exist, and the
/// third needs an archive that does not exist. §22 lists completion state under "must not
/// be faked", and "the CEO has not read this yet" is a completion claim about the CEO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    Idle,
    Queued,
    Working,
    Failed,
}

impl ThreadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThreadStatus::Idle => "idle",
            ThreadStatus::Queued => "queued",
            ThreadStatus::Working => "working",
            ThreadStatus::Failed => "failed",
        }
    }
}

// ---------------------------------------------------------------------------
// THE EVENTS
// ---------------------------------------------------------------------------

/// One event of the additive family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveEvent {
    /// §13 `rich://turn-status` — an authoritative turn-state transition.
    TurnStatus {
        fence: EventFence,
        status: TurnStatus,
        /// When the turn was handed to a lease, so a LIVE view can tick from it.
        started_at: Option<u64>,
        /// The MEASURED active span (`Turn::active_ms`). Explicitly `null` while
        /// unmeasured — never `now() - started_at`, which is how an overnight restart
        /// turns a five-minute task into a twelve-hour one (§6.3).
        active_duration_ms: Option<u64>,
        /// Present ONLY on the replacement turn of a mid-turn-crash replay: the turn this
        /// one continues. Beyond §13 (which has no event for an identity migration) and
        /// carried because without it a renderer draws the CEO's one prompt twice. It is
        /// a MERGE instruction, not an announcement — Rich never says a rotation happened.
        supersedes_turn_id: Option<String>,
        /// **His answer is complete while the turn is still `working`** (product hunt
        /// 2026-10-15, finding 21): the lease withholds everything after its accepted
        /// checkpoint, so nothing more will reach him as words in this turn. Carried only when
        /// true, on a second `working` status. The turn is otherwise live in every respect (a
        /// message he sends steers it, Stop still stops it); the screen stops saying
        /// "Rich is working" and stops its timer.
        answered: bool,
        at: u64,
    },
    /// §13 `rich://message-started` — opens one contiguous run of Rich's prose.
    MessageStarted {
        fence: EventFence,
        message_id: String,
        phase: RichMessagePhase,
        /// The run's first position in the ONE shared per-turn counter (§1.4 G1), or
        /// `null` when the position was never recorded. Never zero-by-default.
        seq: Option<u64>,
        visibility: Visibility,
        at: u64,
    },
    /// §13 `rich://message-delta` — one persisted assistant delta.
    ///
    /// Carries its OWN `visibility` rather than inheriting `Ceo` from the fact that a
    /// message was opened. The first draft of this type did inherit it, and
    /// `an_internal_turns_messages_are_built_internal_and_stop_at_the_gate` caught the
    /// consequence immediately: the re-prime turn's `message-started` was correctly refused
    /// while its deltas — the actual priming TEXT — sailed through. A delta is a piece of
    /// content, so it answers the visibility question itself.
    MessageDelta {
        fence: EventFence,
        message_id: String,
        seq: u64,
        text_delta: String,
        visibility: Visibility,
        at: u64,
    },
    /// §13 `rich://message-completed` — closes one run, carrying its FULL text as read
    /// back from the ledger, so a consumer that missed every delta is still correct.
    MessageCompleted {
        fence: EventFence,
        message_id: String,
        phase: RichMessagePhase,
        text: String,
        visibility: Visibility,
        at: u64,
    },
    /// §13 `rich://activity-upserted` — one merged machinery row as a CEO-safe semantic
    /// activity item. The payload IS the [`TimelineItem`] a reload projects, redacted.
    ActivityUpserted { fence: EventFence, item: TimelineItem, at: u64 },
    /// §13 `rich://worker-upserted` — one delegated AI worker (§7), as witnessed.
    ///
    /// The payload is the [`TimelineItem::WorkerActivity`] a reload projects, redacted:
    /// `entityId` + `threadId` are the equality keys, `bindingRevision` is a staleness
    /// fence only, and the event id is the [`crate::timeline::TimelineBase`] id — the
    /// machinery id, which is stable across re-projection, so repeated ids are idempotent.
    /// [`WorkerActivityItem`] rides inside it verbatim.
    WorkerUpserted { fence: EventFence, item: TimelineItem, at: u64 },
    /// **`rich://ceo-message` — what the CEO himself just said, at the instant it is durable.**
    ///
    /// # Why this family had no event for it until 2026-09-19
    ///
    /// Every other event here is about RICH: his prose, his machinery, his workers, the state
    /// of the turn answering him. The CEO's own message reached a surface only by being
    /// PROJECTED — `TimelineItem::UserMessage`, read on a reload — because the one surface that
    /// existed was a webview that had drawn his sentence itself the moment he pressed Send.
    ///
    /// A second surface makes that assumption false. `echo-opus-phonepage1` raised it from the
    /// phone page (`esc-20260919T003541Z-6885f74b`): the phone subscribes to this family and
    /// therefore learned nothing, live, about a message typed on the Mac — it had to ask the
    /// Mac for the rows again after a reply settled, and a turn that never produced a reply
    /// left his own sentence off the phone until the next reload.
    ///
    /// # The identity is the PROJECTION'S, deliberately
    ///
    /// `message_id` is `{turn_id}:user` and `created_at` is the turn's own `created_at` — the
    /// exact values `timeline.rs` derives for [`TimelineItem::UserMessage`]. So a consumer that
    /// merges on `id` sees the live row and the projected row as ONE row, and a phone that
    /// backfills after receiving this does not grow a duplicate. An id minted here would have
    /// been a second identity for one message, which is this event's own defect wearing a
    /// different hat.
    ///
    /// `Visibility::Ceo` is unconditional and is not a decision made here: the projection gates
    /// this item at `Visibility::Ceo` too, and an INTERNAL turn (a re-prime, a rotation) never
    /// reaches the emit site at all — `spine.rs` skips the whole family for one.
    CeoMessage {
        fence: EventFence,
        /// `{turn_id}:user` — the timeline item's own id, never a fresh one.
        message_id: String,
        text: String,
        /// How the input arrived. `Text` and `Jam` are the two a CEO turn can carry; the phone
        /// draws its microphone marker from this, exactly as `rows_from_payload` does.
        source: Source,
        /// The TURN's `created_at`, so the live row and the projected row sort identically.
        created_at: u64,
        /// The phone's own id for these words when a phone sent them (`Turn::client_id`), so
        /// the phone recognizes its own message in this row even when the row beats the
        /// phone's receipt. `clientId` on the wire, the projection's spelling; absent for
        /// anything else.
        client_id: Option<String>,
        /// The words are the transcript of a voice message he recorded on a phone
        /// (`Turn::voice_note`), so this row, which can beat the phone's receipt, is already a
        /// voice row. `voiceNote: true` on the wire, the projection's spelling; absent otherwise.
        voice_note: bool,
        at: u64,
    },
    /// §13 `rich://thread-summary-updated` — sidebar title, recency and operational status.
    ThreadSummaryUpdated {
        fence: EventFence,
        title: String,
        message_count: usize,
        last_activity: u64,
        status: ThreadStatus,
        at: u64,
    },
}

impl LiveEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            LiveEvent::TurnStatus { .. } => EVENT_TURN_STATUS,
            LiveEvent::MessageStarted { .. } => EVENT_MESSAGE_STARTED,
            LiveEvent::MessageDelta { .. } => EVENT_MESSAGE_DELTA,
            LiveEvent::MessageCompleted { .. } => EVENT_MESSAGE_COMPLETED,
            LiveEvent::ActivityUpserted { .. } => EVENT_ACTIVITY_UPSERTED,
            LiveEvent::WorkerUpserted { .. } => EVENT_WORKER_UPSERTED,
            LiveEvent::CeoMessage { .. } => EVENT_CEO_MESSAGE,
            LiveEvent::ThreadSummaryUpdated { .. } => EVENT_THREAD_SUMMARY_UPDATED,
        }
    }

    pub fn fence(&self) -> &EventFence {
        match self {
            LiveEvent::TurnStatus { fence, .. }
            | LiveEvent::MessageStarted { fence, .. }
            | LiveEvent::MessageDelta { fence, .. }
            | LiveEvent::MessageCompleted { fence, .. }
            | LiveEvent::ActivityUpserted { fence, .. }
            | LiveEvent::WorkerUpserted { fence, .. }
            | LiveEvent::CeoMessage { fence, .. }
            | LiveEvent::ThreadSummaryUpdated { fence, .. } => fence,
        }
    }

    /// Who this event is for. An activity item answers with its OWN projected visibility
    /// (the same value `Timeline::view` gates on), so a thought, an internal record or a
    /// record from an internal turn reports `Internal` here without any second rule.
    pub fn visibility(&self) -> Visibility {
        match self {
            // A turn's existence, its state and its sidebar row are CEO-facing facts.
            // An INTERNAL turn never reaches these constructors at all: `spine.rs` skips
            // the whole family for a turn the CEO must not see.
            // And his OWN message, for the same reason and with no second rule: the projection
            // gates `TimelineItem::UserMessage` at `Visibility::Ceo`, and an internal turn never
            // reaches the emit site.
            LiveEvent::TurnStatus { .. }
            | LiveEvent::CeoMessage { .. }
            | LiveEvent::ThreadSummaryUpdated { .. } => Visibility::Ceo,
            LiveEvent::MessageStarted { visibility, .. } | LiveEvent::MessageCompleted { visibility, .. } => {
                *visibility
            }
            LiveEvent::MessageDelta { visibility, .. } => *visibility,
            // A worker row answers with its OWN projected visibility for the same reason an
            // activity row does: a delegation inside a re-prime or rotation turn is
            // `Internal` and must stop at the gate below, not be filtered by a caller.
            LiveEvent::ActivityUpserted { item, .. } | LiveEvent::WorkerUpserted { item, .. } => item.visibility(),
        }
    }

    /// THE GATE. This family is the CALM family: only `Visibility::Ceo` reaches a webview.
    ///
    /// Technical detail is not "hidden" here, it is absent — it travels on
    /// `rich://machinery`, a family the calm view does not subscribe to, so a renderer of
    /// THIS family cannot show a raw command because it was never handed one. Internal
    /// items (re-prime, rotation, model reasoning, a Tier-3 silent proactive message) have
    /// no render path in any mode and stop here.
    ///
    /// Enforced at the single emit chokepoint in `spine.rs`, not by the caller.
    pub fn may_reach_webview(&self) -> bool {
        self.visibility() == Visibility::Ceo
    }

    /// The JSON payload delivered to the webview. camelCase; every payload carries the
    /// full fence and a `visibility`.
    pub fn payload(&self) -> Value {
        let mut map = Map::new();
        self.fence().write_into(&mut map);
        map.insert("visibility".into(), json!(self.visibility().as_str()));
        match self {
            LiveEvent::TurnStatus { status, started_at, active_duration_ms, supersedes_turn_id, answered, at, .. } => {
                map.insert("status".into(), json!(status.as_str()));
                map.insert("startedAt".into(), json!(started_at));
                // Explicit null, never omitted: an absent field invites `?? 0`, and 0 is a
                // measurement claim.
                map.insert("activeDurationMs".into(), json!(active_duration_ms));
                if let Some(s) = supersedes_turn_id {
                    map.insert("supersedesTurnId".into(), json!(s));
                }
                if *answered {
                    map.insert("answered".into(), json!(true));
                }
                map.insert("at".into(), json!(at));
            }
            LiveEvent::MessageStarted { message_id, phase, seq, at, .. } => {
                map.insert("messageId".into(), json!(message_id));
                // Serialized explicitly, including `"unknown"` — see the module doc.
                map.insert("phase".into(), serde_json::to_value(phase).unwrap_or(Value::Null));
                map.insert("seq".into(), json!(seq));
                map.insert("at".into(), json!(at));
            }
            LiveEvent::MessageDelta { message_id, seq, text_delta, at, .. } => {
                map.insert("messageId".into(), json!(message_id));
                map.insert("seq".into(), json!(seq));
                map.insert("textDelta".into(), json!(text_delta));
                map.insert("at".into(), json!(at));
            }
            LiveEvent::MessageCompleted { message_id, phase, text, at, .. } => {
                map.insert("messageId".into(), json!(message_id));
                map.insert("phase".into(), serde_json::to_value(phase).unwrap_or(Value::Null));
                map.insert("text".into(), json!(text));
                map.insert("at".into(), json!(at));
            }
            LiveEvent::ActivityUpserted { item, at, .. } | LiveEvent::WorkerUpserted { item, at, .. } => {
                // The item's own flattened base already carries `kind`, `id`, the whole
                // fence and `visibility` — so the payload is literally the timeline record
                // a reload projects. The fence written above is overwritten with the
                // identical values; `the_wire_and_the_reload_agree` is the test that they do.
                if let Ok(Value::Object(obj)) = serde_json::to_value(item) {
                    for (k, v) in obj {
                        map.insert(k, v);
                    }
                }
                map.insert("at".into(), json!(at));
            }
            LiveEvent::CeoMessage { message_id, text, source, created_at, client_id, voice_note, at, .. } => {
                map.insert("messageId".into(), json!(message_id));
                map.insert("text".into(), json!(text));
                // The SAME spelling the projection serializes (`snake_case` on `Source`), so a
                // consumer reads one vocabulary off the wire and off a reload.
                map.insert("source".into(), serde_json::to_value(source).unwrap_or(Value::Null));
                map.insert("createdAt".into(), json!(created_at));
                // Present only when there is one, as the projection serializes it.
                if let Some(client_id) = client_id {
                    map.insert("clientId".into(), json!(client_id));
                }
                // Present only when true, as the projection serializes it.
                if *voice_note {
                    map.insert("voiceNote".into(), json!(true));
                }
                map.insert("at".into(), json!(at));
            }
            LiveEvent::ThreadSummaryUpdated { title, message_count, last_activity, status, at, .. } => {
                map.insert("title".into(), json!(title));
                map.insert("messageCount".into(), json!(message_count));
                map.insert("lastActivity".into(), json!(last_activity));
                map.insert("status".into(), json!(status.as_str()));
                map.insert("at".into(), json!(at));
            }
        }
        Value::Object(map)
    }
}

/// A sink for the additive family. A THIRD observer beside `TurnObserver` and
/// `MachineryObserver`, deliberately: three families means a UI's subscription list is the
/// proof of what it can render, rather than a promise about what it chooses to.
pub trait LiveObserver: Send {
    /// Forward one event. MUST be non-blocking and infallible from the spine's view — a UI
    /// that isn't listening never stalls or fails a turn (§13: *"missed UI events never
    /// block the spine"*).
    fn on_live_event(&self, event: &LiveEvent);
}

// ---------------------------------------------------------------------------
// PER-TURN LIVE STATE
// ---------------------------------------------------------------------------

/// The per-turn bookkeeping the additive family needs, and NOTHING else.
///
/// It holds no counters of its own. Message ids come from the ledger's own `text_runs`
/// fold; activity identity, position and merge come from `machinery.rs`'s rules applied
/// incrementally. Everything it produces is reproducible by re-projecting the durable
/// records, which is what makes the wire and a reload agree.
pub(crate) struct LiveTurn {
    fence: EventFence,
    /// True for a turn the CEO must never see (re-prime injection, a rotation handoff
    /// summary). Every item it would produce is `Internal`.
    internal_turn: bool,
    /// The index into `Turn::text_runs` of the message currently open, if any.
    open_run: Option<usize>,
    /// One merged row per merge key — `machinery.rs`'s §1.4 G2 rule applied incrementally,
    /// so the row on the wire is the row a reload projects.
    merged: HashMap<String, MachineryRecord>,
    /// The activity type resolved from the OPENING frames' payload, per tool call. The
    /// closing `tool_result` carries no tool name at all, so the type must be captured
    /// before the merge overwrites the payload (timeline.rs says why).
    types: HashMap<String, ActivityType>,
    /// The tool NAME resolved from the OPENING frames, per tool call.
    ///
    /// Held separately from `types` because it is a different question with a different
    /// answer set: `Task` classifies to no `ActivityType` at all (deliberately — §22), yet
    /// it is exactly the name the worker-identity witness needs. On the native wire the name
    /// and the async-launch acknowledgement arrive on DIFFERENT frames, so without this memo
    /// `extract_agent_id` would answer `None` forever and workers would silently stop being
    /// reported. `timeline::resolve_tool_names` is the same memo, batched.
    tool_names: HashMap<String, String>,
    /// The latest `at` observed per merge key.
    last_seen: HashMap<String, u64>,
    /// Merge keys in FIRST-SIGHT order. A `HashMap` iteration order is not an order, and
    /// this list is walked on every observation to refresh the other delegations — so the
    /// events a turn emits are the same events in the same sequence on every run.
    order: Vec<String>,
    /// The `agentId` extracted per tool call, resolved from the RAW payload before the
    /// merge overwrites it — the identical reason `timeline::resolve_agent_ids` resolves
    /// over the raw records: the async-launch acknowledgement arrives on the tool RESULT,
    /// and `merge_into` replaces `payload` with the last update's JSON.
    agent_ids: HashMap<String, String>,
    /// The last worker row EMITTED per merge key, so a re-observation that witnessed
    /// nothing new emits nothing. §13's contract is *"emit on any observed state change"*,
    /// not "emit on every tick".
    emitted_workers: HashMap<String, WorkerActivityItem>,
    /// WITNESS (a) of the Output side panel PRD (§4.1 (a), richos-hq
    /// `docs/prds/2026-10-05-output-side-panel.md`): the tool calls already recorded as
    /// written files, so a call is recorded once however many frames follow its completion.
    output_witnessed: std::collections::HashSet<String>,
    /// Rows witnessed since the spine last took them ([`LiveTurn::take_output_writes`]).
    output_writes: Vec<crate::output::WriteRow>,
}

impl LiveTurn {
    pub(crate) fn new(fence: EventFence, internal_turn: bool) -> Self {
        LiveTurn {
            fence,
            internal_turn,
            open_run: None,
            merged: HashMap::new(),
            types: HashMap::new(),
            tool_names: HashMap::new(),
            last_seen: HashMap::new(),
            order: Vec::new(),
            agent_ids: HashMap::new(),
            emitted_workers: HashMap::new(),
            output_witnessed: std::collections::HashSet::new(),
            output_writes: Vec::new(),
        }
    }

    /// **Witness (a), at the live merge**: the moment a merged call becomes a COMPLETED
    /// write-tool call with locations and no parent, it is a file Rich wrote.
    ///
    /// Evaluated on the merged row, because on the native wire the name, the arguments and
    /// the completion are three different frames (Frank's minor 1). A call with a
    /// `parent_tool_use_id` is a front-desk WORKER's — witness (b) records it under the
    /// worker's name — and an internal turn's call is not the thread's work.
    fn witness_output(&mut self, key: &str) {
        let Some(row) = self.merged.get(key) else { return };
        let Some(id) = row.tool_call_id.clone() else { return };
        let is_write = self.tool_names.get(&id).is_some_and(|n| crate::output::WRITE_TOOLS.contains(&n.as_str()));
        if !is_write
            || row.internal
            || row.kind != crate::machinery::MachineryKind::ToolCall
            || row.status != Some(crate::machinery::ToolStatus::Completed)
            || row.locations.is_empty()
            || row.parent_tool_use_id.is_some()
        {
            return;
        }
        if self.output_witnessed.insert(id) {
            self.output_writes.extend(crate::output::tool_call_rows(row));
        }
    }

    /// The written-file rows witnessed since the last call, for the spine to append to the
    /// thread's output record.
    pub(crate) fn take_output_writes(&mut self) -> Vec<crate::output::WriteRow> {
        std::mem::take(&mut self.output_writes)
    }

    fn message_id(&self, run_index: usize) -> String {
        // The SAME derivation `timeline::turn_items` uses, so a live id and a projected id
        // are the same id.
        format!("{}:text:{run_index}", self.fence.turn_id())
    }

    fn visibility(&self) -> Visibility {
        if self.internal_turn {
            Visibility::Internal
        } else {
            Visibility::Ceo
        }
    }

    /// One assistant delta, ALREADY persisted. `runs` is the turn's `text_runs` read back
    /// out of the ledger after the append — so the run boundary is the ledger's own fold
    /// (`push_text_run`), never a second opinion about where prose was interrupted.
    pub(crate) fn on_text(
        &mut self,
        runs: &[crate::ledger::TextRun],
        seq: u64,
        text_delta: &str,
        at: u64,
    ) -> Vec<LiveEvent> {
        let mut out = Vec::new();
        let Some(index) = runs.len().checked_sub(1) else {
            return out;
        };
        if self.open_run != Some(index) {
            // A previous run just closed (a tool call took the positions in between).
            if let Some(prev) = self.open_run {
                out.extend(self.close_run(runs, prev, at));
            }
            self.open_run = Some(index);
            out.push(LiveEvent::MessageStarted {
                fence: self.fence.clone(),
                message_id: self.message_id(index),
                phase: STREAMED_MESSAGE_PHASE,
                seq: runs[index].start_seq,
                visibility: self.visibility(),
                at,
            });
        }
        out.push(LiveEvent::MessageDelta {
            fence: self.fence.clone(),
            message_id: self.message_id(index),
            seq,
            text_delta: text_delta.to_string(),
            visibility: self.visibility(),
            at,
        });
        out
    }

    fn close_run(&self, runs: &[crate::ledger::TextRun], index: usize, at: u64) -> Option<LiveEvent> {
        let run = runs.get(index)?;
        Some(LiveEvent::MessageCompleted {
            fence: self.fence.clone(),
            message_id: self.message_id(index),
            phase: STREAMED_MESSAGE_PHASE,
            text: run.text.clone(),
            visibility: self.visibility(),
            at,
        })
    }

    /// Close whatever message is open — at a tool call (the run is genuinely over) or at
    /// the turn's terminal event.
    pub(crate) fn close_open_message(&mut self, runs: &[crate::ledger::TextRun], at: u64) -> Vec<LiveEvent> {
        match self.open_run.take() {
            Some(index) => self.close_run(runs, index, at).into_iter().collect(),
            None => Vec::new(),
        }
    }

    /// One machinery record, ALREADY routed (and, where a journal is attached, retained).
    ///
    /// Produces the row this observation changed, as ONE of two events:
    ///
    ///   * `rich://worker-upserted` when the tool call is a DELEGATION whose `agentId`
    ///     joins to the engine's lifecycle stream in the same session, or
    ///   * `rich://activity-upserted` otherwise — the merged row so far, projected
    ///     through the SAME `timeline::activity_item` a reload uses and then redacted.
    ///
    /// Either way the payload is the item a reload projects, because it is built by the
    /// same functions. A record that projects to anything but `Visibility::Ceo` (a thought,
    /// an `internal: true` record, anything on an internal turn) yields an event whose
    /// visibility stops it at the emit chokepoint — it is CONSTRUCTED, so the gate has
    /// something to refuse, and the negative control has something to observe.
    ///
    /// It also refreshes EVERY OTHER delegation this turn has made, and that is not
    /// incidental — it is this path's whole answer to two real races:
    ///
    ///   1. **The lifecycle row can arrive after the tool result.** The `agentId` is only
    ///      extractable from the async-launch acknowledgement on the tool RESULT, and
    ///      `worker-created-handoff.sh` (`PostToolUse[Agent]`) writes its row at about the
    ///      same instant. Neither order is guaranteed, so a delegation whose row was not
    ///      on disk yet is picked up at the next observation instead of never.
    ///   2. **A worker changes state without producing any agent traffic at all.** `started`,
    ///      `updated` and `run_ended` are hook writes in another process; nothing about
    ///      them reaches this stream.
    ///
    /// **THE HONEST LIMIT, STATED RATHER THAN DISCOVERED LATER.** This path is driven by
    /// machinery arrival, so between two tool events a worker's state change is not
    /// observed — the refresh happens at the next record and, failing that, at
    /// [`LiveTurn::on_turn_end`]. There is no poll and no timer, deliberately: the spine
    /// holds `&mut self` for the whole length of a turn and `LiveObserver` is `Send` and
    /// not `Sync`, so a background ticker would need a second lock and a second emit path.
    /// The consequence is bounded and one-directional — a chip can be up to one tool call
    /// STALE; it is never wrong about a worker that was never witnessed.
    ///
    /// `worker_rows` is a thunk, not a slice, so a turn that never delegates never reads
    /// the lifecycle file at all (it is re-read per observation to see hook writes).
    pub(crate) fn on_machinery(
        &mut self,
        record: &MachineryRecord,
        worker_rows: &dyn Fn() -> Vec<WorkerEventRow>,
    ) -> Vec<LiveEvent> {
        // `machinery::merge_key`, the SAME derivation the reload path folds on, so a live row
        // and a reloaded row can never be keyed two different ways. A `LiveTurn` is per turn,
        // so the turn the key carries for a compaction is always this one — which is exactly
        // why calling the shared function costs nothing and stops a second rule being born.
        let key = crate::machinery::merge_key(record);

        let seen = self.last_seen.entry(key.clone()).or_insert(record.at);
        if record.at > *seen {
            *seen = record.at;
        }
        if let (Some(id), Some(payload)) = (record.tool_call_id.as_ref(), record.payload.as_ref()) {
            if !self.types.contains_key(id) {
                if let Some(t) = timeline::classify(payload) {
                    self.types.insert(id.clone(), t);
                }
            }
            if !self.tool_names.contains_key(id) {
                if let Some(name) = timeline::tool_name_of(payload) {
                    self.tool_names.insert(id.clone(), name.to_string());
                }
            }
            // Resolved from the RAW payload, before the merge below overwrites it. The name
            // comes from the memo above, because the frame carrying the acknowledgement
            // carries no name.
            if !self.agent_ids.contains_key(id) {
                let name = self.tool_names.get(id).map(String::as_str);
                if let Some(agent_id) = timeline::extract_agent_id(payload, name) {
                    self.agent_ids.insert(id.clone(), agent_id);
                }
            }
        }

        // A COMPACTION MERGES TOO, and it has to: its start, its 30-second heartbeats and its
        // ending are four or five separate `system/status` frames with no correlation id
        // between them (`machinery::compaction_key`). Unmerged they would be four rows saying
        // the same thing. The key is the LEASE's, which is turn-scoped here because a
        // `LiveTurn` is built per turn — `machinery::project` re-scopes it explicitly for the
        // reload path, where one map spans every turn of the thread.
        let mergeable = record.tool_call_id.is_some()
            && matches!(
                record.kind,
                crate::machinery::MachineryKind::ToolCall | crate::machinery::MachineryKind::Compaction
            );
        match self.merged.get_mut(&key) {
            Some(base) if mergeable => crate::machinery::merge_into(base, record.clone()),
            // Not mergeable, or first sight: this record IS the row. (An unmergeable
            // record keys on its own unique `machinery_id`, so it can never overwrite
            // another row.)
            _ => {
                self.merged.insert(key.clone(), record.clone());
                self.order.push(key.clone());
            }
        }
        self.witness_output(&key);

        let rows = if self.agent_ids.is_empty() { Vec::new() } else { worker_rows() };

        let mut out = Vec::new();
        match self.worker_upsert(&key, &rows) {
            Some(event) => out.push(event),
            // Not a delegation, or a delegation the lifecycle stream cannot yet vouch for
            // — the ordinary activity row, exactly as before. A key that has ALREADY been
            // emitted as a worker never falls back here: a row must not change kind
            // backwards because one read of the stream came up short.
            None if !self.emitted_workers.contains_key(&key)
                && !self.awaiting_delegation_identity(&key) =>
            {
                if let Some(event) = self.activity_upsert(&key) {
                    out.push(event);
                }
            }
            None => {}
        }
        out.extend(self.refresh_workers(Some(&key), &rows));
        out
    }

    /// Whether this row is a delegated-work tool call whose identity is not knowable YET.
    ///
    /// **This exists because the native wire moved the identity witness one frame later, and
    /// without it the CEO briefly sees a delegation drawn as a generic activity.** On ACP the
    /// tool name and the harness's async-launch acknowledgement rode on ONE
    /// `tool_call_update`, so the first record for a delegated call was already a worker. On
    /// this wire the name is on the OPENING frame and the acknowledgement on the closing
    /// `tool_result`, so between them there is a record that classifies to
    /// `ActivityType::Other` and renders, CEO-visible, as the word *"Worked"* —
    /// `timeline.rs`'s own comment calls that *"not a thing Rich did"*.
    ///
    /// **Bounded, and it never drops anything.** The hold ends the moment EITHER the identity
    /// resolves (the row becomes a worker row) OR the row reaches a terminal status (it was
    /// a synchronous Task after all, and becomes an ordinary activity row). Only the LIVE
    /// family is deferred: the record is on `rich://machinery` throughout, it is journaled,
    /// and `Timeline::project_with_workers` resolves the same two-pass witness on reload.
    ///
    /// **The residual, stated rather than hidden:** a SYNCHRONOUS `Task` that runs for a long
    /// time shows no live activity row until it returns, because until it returns RichOS
    /// genuinely does not know whether it delegated. Announcing "Worked" in the meantime
    /// would be a guess, and §22 forbids the guess more strongly than it minds the delay.
    fn awaiting_delegation_identity(&self, key: &str) -> bool {
        let Some(row) = self.merged.get(key) else {
            return false;
        };
        let Some(id) = row.tool_call_id.as_ref() else {
            return false;
        };
        if self.agent_ids.contains_key(id) {
            return false;
        }
        if row.status.as_ref().map(|s| s.is_terminal()).unwrap_or(false) {
            return false;
        }
        matches!(self.tool_names.get(id).map(String::as_str), Some("Task") | Some("Agent"))
    }

    /// The ordinary activity row for one merge key.
    fn activity_upsert(&self, key: &str) -> Option<LiveEvent> {
        let row = self.merged.get(key)?;
        let item = timeline::activity_item(
            row,
            self.fence.entity_id(),
            self.fence.thread_id(),
            self.fence.turn_id(),
            self.fence.binding_revision(),
            self.internal_turn,
            &self.types,
            &self.last_seen,
        );
        let at = row.at;
        // Redacted for the same reason `Timeline::view(Ceo)` redacts: the bytes are gone
        // from the value, not merely flagged on it.
        Some(LiveEvent::ActivityUpserted { fence: self.fence.clone(), item: item.redacted(), at })
    }

    /// The worker row for one merge key, IF this tool call delegated and the lifecycle
    /// stream has an in-session row for the id it spawned — and if anything about it
    /// changed since the last emission.
    ///
    /// Both halves of the join are `timeline.rs`'s own: `worker_activity` carries CLAUSE 3
    /// (a row is admitted only when its `session_id` matches the record's, because
    /// `agent_id` is not globally unique across sessions) and `worker_activity_item`
    /// builds the row. Nothing is re-derived here.
    fn worker_upsert(&mut self, key: &str, rows: &[WorkerEventRow]) -> Option<LiveEvent> {
        let record = self.merged.get(key)?;
        let tool_call_id = record.tool_call_id.as_ref()?;
        let agent_id = self.agent_ids.get(tool_call_id)?;
        let worker = timeline::worker_activity(agent_id, &record.session_id, rows)?;
        if self.emitted_workers.get(key) == Some(&worker) {
            return None; // witnessed nothing new
        }
        let item = timeline::worker_activity_item(
            record,
            self.fence.entity_id(),
            self.fence.thread_id(),
            self.fence.turn_id(),
            self.fence.binding_revision(),
            self.internal_turn,
            worker.clone(),
        );
        let at = record.at;
        self.emitted_workers.insert(key.to_string(), worker);
        Some(LiveEvent::WorkerUpserted { fence: self.fence.clone(), item: item.redacted(), at })
    }

    /// Re-join every delegation this turn has made, in first-sight order, and emit the ones
    /// that changed. `skip` is the key already handled by the caller.
    fn refresh_workers(&mut self, skip: Option<&str>, rows: &[WorkerEventRow]) -> Vec<LiveEvent> {
        if self.agent_ids.is_empty() {
            return Vec::new();
        }
        let keys: Vec<String> = self.order.iter().filter(|k| Some(k.as_str()) != skip).cloned().collect();
        keys.iter().filter_map(|k| self.worker_upsert(k, rows)).collect()
    }

    /// The turn is over: one last re-join, so the LAST thing the CEO saw live is the thing
    /// an immediate reload projects.
    ///
    /// Without this the two paths could legitimately disagree at exactly the moment the
    /// disagreement is most visible — the turn settles, the transcript collapses, and a
    /// snapshot read a second later redraws a chip the live path last described one tool
    /// call ago.
    pub(crate) fn on_turn_end(&mut self, worker_rows: &dyn Fn() -> Vec<WorkerEventRow>) -> Vec<LiveEvent> {
        if self.agent_ids.is_empty() {
            return Vec::new();
        }
        let rows = worker_rows();
        self.refresh_workers(None, &rows)
    }
}

// ---------------------------------------------------------------------------
// PROACTIVE MESSAGES — the one sourced phase
// ---------------------------------------------------------------------------

/// The message events for a proactive message (§5.1), which is written atomically rather
/// than streamed: one `message-started` and one `message-completed`, no deltas, with the
/// phase the ledger genuinely recorded.
///
/// Tier 3 / `Silent` produces `Visibility::Internal` — it never appears in the
/// conversation at all — and is therefore constructed and then refused by the gate, the
/// same posture as an internal activity row.
pub(crate) fn proactive_message_events(
    fence: &EventFence,
    tier: AttentionTier,
    text: &str,
    at: u64,
) -> Vec<LiveEvent> {
    let visibility = if tier == AttentionTier::Silent { Visibility::Internal } else { Visibility::Ceo };
    // A proactive turn's prose is one atomically-written run, so its projected id is
    // `{turn}:text:0` (ledger.rs writes exactly one `TextRun`).
    let message_id = format!("{}:text:0", fence.turn_id());
    vec![
        LiveEvent::MessageStarted {
            fence: fence.clone(),
            message_id: message_id.clone(),
            phase: RichMessagePhase::Proactive,
            // Written atomically, outside the stream counter — unknown, never zero.
            seq: None,
            visibility,
            at,
        },
        LiveEvent::MessageCompleted {
            fence: fence.clone(),
            message_id,
            phase: RichMessagePhase::Proactive,
            text: text.to_string(),
            visibility,
            at,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::TextRun;

    /// A fence for unit tests. The real one can only come from a `ThreadBinding`; this
    /// mirrors exactly what `EventFence::for_turn` produces, so payload shape can be
    /// asserted without standing up a ledger.
    fn fence() -> EventFence {
        EventFence {
            entity_id: EntityId::parse("femcboost").unwrap(),
            thread_id: "thr".into(),
            turn_id: "turn_1".into(),
            binding_revision: 3,
        }
    }

    #[test]
    fn streamed_prose_is_never_labelled_final() {
        assert_eq!(
            STREAMED_MESSAGE_PHASE,
            RichMessagePhase::Unknown,
            "the stream carries no commentary-vs-final signal (probe 2026-08-28 §2: 52 \
             agent_message_chunks, zero phase markers), and `message-started` fires before \
             the turn is over, so 'final' cannot be known — let alone defaulted to"
        );
        assert_ne!(STREAMED_MESSAGE_PHASE, RichMessagePhase::Final);
        assert_ne!(STREAMED_MESSAGE_PHASE, RichMessagePhase::Commentary);
    }

    #[test]
    fn the_unknown_phase_is_on_the_wire_explicitly_not_omitted() {
        let ev = LiveEvent::MessageStarted {
            fence: fence(),
            message_id: "turn_1:text:0".into(),
            phase: STREAMED_MESSAGE_PHASE,
            seq: Some(0),
            visibility: Visibility::Ceo,
            at: 10,
        };
        let p = ev.payload();
        assert_eq!(p["phase"], json!("unknown"), "an absent field invites `phase ?? 'final'`");
        assert!(p.get("phase").is_some());
    }

    #[test]
    fn every_payload_carries_the_whole_fence() {
        let events = vec![
            LiveEvent::TurnStatus {
                fence: fence(),
                status: TurnStatus::Working,
                started_at: Some(8),
                active_duration_ms: None,
                supersedes_turn_id: None,
                answered: false,
                at: 8,
            },
            LiveEvent::MessageStarted {
                fence: fence(),
                message_id: "m".into(),
                phase: STREAMED_MESSAGE_PHASE,
                seq: Some(0),
                visibility: Visibility::Ceo,
                at: 10,
            },
            LiveEvent::MessageDelta {
                fence: fence(),
                message_id: "m".into(),
                seq: 0,
                text_delta: "hi".into(),
                visibility: Visibility::Ceo,
                at: 10,
            },
            LiveEvent::MessageCompleted {
                fence: fence(),
                message_id: "m".into(),
                phase: STREAMED_MESSAGE_PHASE,
                text: "hi".into(),
                visibility: Visibility::Ceo,
                at: 11,
            },
            LiveEvent::ThreadSummaryUpdated {
                fence: fence(),
                title: "General".into(),
                message_count: 2,
                last_activity: 5,
                status: ThreadStatus::Idle,
                at: 12,
            },
        ];
        for ev in events {
            let p = ev.payload();
            assert_eq!(p["entityId"], json!("femcboost"), "{}", ev.event_name());
            assert_eq!(p["threadId"], json!("thr"), "{}", ev.event_name());
            assert_eq!(p["turnId"], json!("turn_1"), "{}", ev.event_name());
            assert_eq!(p["bindingRevision"], json!(3), "{}", ev.event_name());
            assert_eq!(p["visibility"], json!("ceo"), "{}", ev.event_name());
            assert!(p.get("at").is_some(), "{}", ev.event_name());
        }
    }

    #[test]
    fn an_unmeasured_duration_is_null_and_never_zero() {
        let p = LiveEvent::TurnStatus {
            fence: fence(),
            status: TurnStatus::Working,
            started_at: Some(8),
            active_duration_ms: None,
            supersedes_turn_id: None,
            answered: false,
            at: 8,
        }
        .payload();
        assert_eq!(p["activeDurationMs"], json!(null));
        assert_ne!(p["activeDurationMs"], json!(0), "§6.3: 0 is a measurement, not a placeholder");
        assert!(p.get("supersedesTurnId").is_none(), "only a replay carries it");
    }

    #[test]
    fn the_gate_refuses_everything_that_is_not_ceo_facing() {
        for v in [Visibility::Internal, Visibility::Technical] {
            let ev = LiveEvent::MessageStarted {
                fence: fence(),
                message_id: "m".into(),
                phase: RichMessagePhase::Proactive,
                seq: None,
                visibility: v,
                at: 1,
            };
            assert!(!ev.may_reach_webview(), "{v:?} must not reach the webview on the calm family");
        }
        let ok = LiveEvent::MessageStarted {
            fence: fence(),
            message_id: "m".into(),
            phase: RichMessagePhase::Proactive,
            seq: None,
            visibility: Visibility::Ceo,
            at: 1,
        };
        assert!(ok.may_reach_webview());
    }

    #[test]
    fn a_silent_proactive_message_is_constructed_and_then_refused() {
        let f = fence();
        let silent = proactive_message_events(&f, AttentionTier::Silent, "quietly noted", 7);
        assert_eq!(silent.len(), 2, "constructed, so the gate has something to refuse");
        for ev in &silent {
            assert!(!ev.may_reach_webview(), "§5.1: Tier 3 never appears in the conversation");
        }
        let digest = proactive_message_events(&f, AttentionTier::Digest, "the release slipped", 7);
        for ev in &digest {
            assert!(ev.may_reach_webview());
        }
        let p = digest[0].payload();
        assert_eq!(p["phase"], json!("proactive"), "THIS phase is sourced — the ledger records it");
        assert_eq!(p["messageId"], json!("turn_1:text:0"), "the id a reload projects");
        assert_eq!(p["seq"], json!(null), "written atomically, outside the stream counter");
        assert_eq!(digest[1].payload()["text"], json!("the release slipped"));
    }

    #[test]
    fn a_run_opens_once_and_closes_once_at_the_ledgers_own_boundary() {
        let mut live = LiveTurn::new(fence(), false);
        // The ledger's fold after one delta at seq 0.
        let runs = vec![TextRun { start_seq: Some(0), end_seq: Some(0), text: "he said X".into(), at: 10 }];
        let out = live.on_text(&runs, 0, "he said X", 10);
        assert_eq!(out.len(), 2, "started + delta");
        assert!(matches!(out[0], LiveEvent::MessageStarted { .. }));
        assert!(matches!(out[1], LiveEvent::MessageDelta { .. }));

        // A second delta EXTENDING the same run opens nothing new.
        let runs = vec![TextRun { start_seq: Some(0), end_seq: Some(1), text: "he said X more".into(), at: 10 }];
        let out = live.on_text(&runs, 1, " more", 11);
        assert_eq!(out.len(), 1, "delta only — the run is already open");

        // A tool call took seq 2; the ledger folds seq 3 into a NEW run.
        let runs = vec![
            TextRun { start_seq: Some(0), end_seq: Some(1), text: "he said X more".into(), at: 10 },
            TextRun { start_seq: Some(3), end_seq: Some(3), text: "then said Z".into(), at: 30 },
        ];
        let out = live.on_text(&runs, 3, "then said Z", 30);
        assert_eq!(out.len(), 3, "completed(prev) + started(new) + delta");
        match &out[0] {
            LiveEvent::MessageCompleted { message_id, text, .. } => {
                assert_eq!(message_id, "turn_1:text:0");
                assert_eq!(text, "he said X more", "the completed run carries its FULL text");
            }
            other => panic!("expected the previous run to close first, got {other:?}"),
        }
        match &out[1] {
            LiveEvent::MessageStarted { message_id, seq, .. } => {
                assert_eq!(message_id, "turn_1:text:1");
                assert_eq!(*seq, Some(3), "the run's real position, gap and all");
            }
            other => panic!("expected a new run to open, got {other:?}"),
        }
        // The terminal close is idempotent: once closed, nothing more.
        let closed = live.close_open_message(&runs, 40);
        assert_eq!(closed.len(), 1);
        assert!(live.close_open_message(&runs, 41).is_empty(), "a closed message closes once");
    }

    #[test]
    fn an_internal_turns_messages_are_built_internal_and_stop_at_the_gate() {
        // THE DELTA IS THE ONE THAT MATTERS. An earlier draft gave `MessageDelta` a fixed
        // `Ceo` visibility on the reasoning that it belongs to a message that was already
        // gated — and this assertion failed on the delta while passing on the start event,
        // i.e. the priming TEXT would have reached the webview with only its opener
        // refused. Content answers the visibility question itself.
        let mut live = LiveTurn::new(fence(), true);
        let runs = vec![TextRun { start_seq: Some(0), end_seq: Some(0), text: "[re-prime]".into(), at: 1 }];
        let events = live.on_text(&runs, 0, "[re-prime]", 1);
        assert!(events.iter().any(|e| matches!(e, LiveEvent::MessageDelta { .. })), "a delta was produced");
        for ev in &events {
            assert!(!ev.may_reach_webview(), "re-prime traffic has no render path: {ev:?}");
        }
        for ev in live.close_open_message(&runs, 2) {
            assert!(!ev.may_reach_webview(), "nor does its completion: {ev:?}");
        }
    }

    // =======================================================================================
    // THE COMPACTION, THROUGH THE LIVE PATH — one pause, one row, however many frames.
    // Frames verbatim from `docs/verification/compaction-notice-2026-09-06/raw/cellT1.jsonl`.
    // =======================================================================================

    fn compaction_frame(phase: &str) -> serde_json::Value {
        match phase {
            "start" => json!({"type":"system","subtype":"status","status":"compacting",
                              "session_id":"0fee2d41","uuid":"6bb40a60"}),
            "beat" => json!({"type":"system","subtype":"status","status":"compacting",
                             "session_id":"0fee2d41","uuid":"818ad359"}),
            _ => json!({"type":"system","subtype":"status","status":serde_json::Value::Null,
                        "compact_result":"success","session_id":"0fee2d41","uuid":"9e1cfd6b"}),
        }
    }

    #[test]
    fn a_compaction_reaches_the_webview_as_one_upserted_row() {
        let mut turn = LiveTurn::new(fence(), false);
        let none = || Vec::new();
        let mut seen: Vec<(String, String, String)> = Vec::new();

        // The real sequence of `cellT1` turn 3: announcement, one 30-second heartbeat, ending.
        for (i, phase) in ["start", "beat", "end"].iter().enumerate() {
            let rec = crate::machinery::MachineryRecord::from_native_event(
                &compaction_frame(phase),
                "0fee2d41",
                i as u64,
            )
            .remove(0)
            .stamp("thr", Some("turn_1"), false);
            for ev in turn.on_machinery(&rec, &none) {
                assert!(ev.may_reach_webview(), "a compaction is a CEO row; the gate must pass it");
                assert_eq!(ev.event_name(), EVENT_ACTIVITY_UPSERTED);
                let p = ev.payload();
                seen.push((
                    p["id"].as_str().unwrap().to_string(),
                    p["state"].as_str().unwrap().to_string(),
                    p["summary"].as_str().unwrap().to_string(),
                ));
            }
        }

        assert_eq!(seen.len(), 3, "three frames, three upserts");
        assert_eq!(seen[0].0, seen[1].0, "the heartbeat must UPDATE the row, not open a second one");
        assert_eq!(seen[0].0, seen[2].0, "and so must the ending");
        assert_eq!(seen[0].1, "running");
        assert_eq!(seen[1].1, "running");
        assert_eq!(seen[2].1, "completed", "the row closes, so a finished turn cannot leave one running");
        assert_eq!(seen[0].2, "Making room to keep going");
        assert_eq!(seen[2].2, "Made room to keep going");
    }

    #[test]
    fn a_compaction_on_an_internal_turn_never_reaches_the_webview() {
        // A re-prime or rotation turn compacting is machinery about machinery, and the CEO is
        // never told a rotation happened. The event is CONSTRUCTED so the gate has something
        // to refuse — the same discipline every other internal row follows.
        let mut turn = LiveTurn::new(fence(), true);
        let none = || Vec::new();
        let rec = crate::machinery::MachineryRecord::from_native_event(&compaction_frame("start"), "s", 0)
            .remove(0)
            .stamp("thr", Some("turn_1"), false);
        let events = turn.on_machinery(&rec, &none);
        assert_eq!(events.len(), 1, "the event exists, so the gate is what stops it");
        assert!(!events[0].may_reach_webview(), "an internal turn's compaction must stop at the gate");
    }

    /// The three wire frames of one `Write`, as the native driver hands them over; `parent`
    /// makes them a front-desk WORKER's, nested under the lead's `Agent` call.
    fn write_frames(id: &str, path: &str, parent: Option<&str>) -> Vec<crate::machinery::MachineryRecord> {
        let parent = json!(parent);
        [
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"tool_use","id":id,"name":"Write","input":{}}},"parent_tool_use_id":parent}),
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Write",
                   "input":{"file_path":path,"content":"x"}}]},"parent_tool_use_id":parent}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]},
                   "tool_use_result":{"type":"create","filePath":path},"parent_tool_use_id":parent}),
        ]
        .iter()
        .enumerate()
        .flat_map(|(i, f)| crate::machinery::MachineryRecord::from_native_event(f, "sess", i as u64))
        .map(|r| r.stamp("thr", Some("turn_1"), false))
        .collect()
    }

    /// Witness (a) at the live merge (Output side panel PRD §4.1 (a), S2): a lead `Write` as
    /// three frames is ONE row the moment it completes; a nested worker `Write` as three frames
    /// is none — that file is the worker's, and witness (b) names it.
    #[test]
    fn output_witness_a_takes_a_lead_write_once_and_never_a_nested_worker_write() {
        let mut turn = LiveTurn::new(fence(), false);
        let none = || Vec::new();
        let mut taken = Vec::new();
        for r in write_frames("toolu_L", "/Users/ceo/acme/brief.md", None) {
            turn.on_machinery(&r, &none);
            taken.extend(turn.take_output_writes());
        }
        assert_eq!(taken.len(), 1, "{taken:?}");
        assert_eq!(taken[0].key, "mach:toolu_L");
        assert_eq!(taken[0].actor, crate::output::Actor::Rich);
        assert_eq!((taken[0].thread_id.as_str(), taken[0].turn_id.as_deref()), ("thr", Some("turn_1")));
        // A late duplicate of the close records nothing more.
        for r in write_frames("toolu_L", "/Users/ceo/acme/brief.md", None).into_iter().skip(2) {
            turn.on_machinery(&r, &none);
        }
        assert!(turn.take_output_writes().is_empty());

        for r in write_frames("toolu_W", "/Users/ceo/acme/worker.md", Some("toolu_AGENT")) {
            turn.on_machinery(&r, &none);
        }
        assert!(turn.take_output_writes().is_empty(), "a nested worker's write is never Rich's");
    }

}
