//! The SPINE — the durable Rich that owns the conversation and the CEO's attention.
//!
//! Ties together: the ledger (durable conversation + action record), the thread data
//! model (topic views over the shared ledger), the current compute lease (a swappable
//! `Cognition`), the turn-boundary controller (queue-not-interrupt), the re-prime seam,
//! **app-owned turn-boundary rotation on a context watermark**, **mid-turn-crash
//! recovery/replay**, and **the proactive-attention seam**. The lease is disposable;
//! THIS is Rich.
//!
//! ## Concurrency note (why rotation can never race a CEO message)
//! Every method here takes `&mut self`. The Tauri shell holds the one `Spine` behind a
//! `Mutex`, so exactly one command (`send_message`, rotation, recovery — all of it) runs
//! at a time; a message that "arrives during rotation" simply blocks on that mutex until
//! rotation's synchronous call stack returns, then proceeds normally against the fresh
//! lease. This is the mechanism that satisfies continuity §3.3's "a message that arrives
//! during rotation is queued" — no separate queuing code needed for that case (the
//! `queue: VecDeque<Queued>` field below handles the DIFFERENT case: a message arriving
//! while a TURN, not a rotation, is in flight).

use crate::cognition::{Cognition, CognitionError, LeaseFactory, TurnItem};
use crate::entity::{EntityId, EntityRegistry, ThreadBinding};
use crate::journal::MachineryJournal;
use crate::ledger::{ActionStatus, ActionVisibility, AttentionTier, Ledger, LedgerError, Message, Source};
use crate::live::{
    proactive_message_events, EventFence, LiveEvent, LiveObserver, LiveTurn, ThreadStatus, TurnStatus,
};
use crate::machinery::{ContextUsage, MachineryObserver, MachineryRecord};
use crate::reprime::{
    EvidenceLookup, EvidenceRequest, EvidenceTier, LoroContextCompiler, LoroTier, RePrimePayload,
    SliceRequest, DEFAULT_LORO_BUDGET_CHARS, DEFAULT_TAIL_TURNS,
};
use crate::correction::{ProposalObserver, SharedCorrectionDesk};
use crate::loro::SharedSliceProvenance;
use crate::staging::{CorrectionObserver, SharedCandidateDesk};
use crate::steering::{ActiveTurn, IntakeRecord, StopClaim, TurnControl};
use crate::stream::{StreamEvent, TurnObserver};
use crate::thread::{summaries, ThreadSummary};
use crate::timeline::Timeline;
use crate::util::now_millis;
use crate::worker_events::{self, WorkerEventRow};
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SpineError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Cognition(#[from] CognitionError),
    /// No thread is active AND none can be chosen without guessing. UX §21 "Entity
    /// binding failure": *"Block send. State that Rich cannot safely determine which
    /// entity the work belongs to. Require an explicit entity choice. Never default to
    /// the last entity."* That is why this no longer silently creates a thread.
    #[error(
        "no active thread, and no entity was named — Rich will not guess which entity area this \
         belongs to. Choose an entity, or activate an existing thread."
    )]
    NoActiveThread,
    /// An entity id that is not in the registry (ECS §3.3: never default, never guess).
    #[error("unknown entity {0}: not in the entity registry")]
    UnknownEntity(String),
    #[error("no compute lease attached")]
    NoLease,
    #[error("no lease factory attached — cannot rotate or recover")]
    NoLeaseFactory,
    /// The durable intake log (UX §9.2/§9.3) could not be written. Surfaced rather than
    /// swallowed: an undrained record is the CEO's words still waiting to be delivered.
    #[error("steering intake: {0}")]
    Steering(String),
}

/// A prompt accepted while a turn was in flight, awaiting delivery at the next turn
/// boundary (queue-not-interrupt: the CEO is never blocked, workers are never killed).
///
/// Carries the BINDING it was accepted under, not just a thread id (ECS §3.4: the binding
/// is *"captured when the turn started"*). A queued prompt that the CEO typed in entity A
/// is therefore still delivered as entity A even if the active context has since moved to
/// entity B — the queue can never launder a turn across the boundary.
struct Queued {
    turn_id: String,
    binding: ThreadBinding,
    text: String,
    /// The intake-log id this came from, when it was a steering message (UX §9.2).
    ///
    /// It is the ORDERING KEY a stop is settled against: steering the CEO wrote BEFORE he
    /// pressed stop is stopped with it, steering he wrote AFTER is a new instruction and
    /// survives. `None` means it did not come through the intake log at all, which is
    /// treated as "before", because there is no evidence it came after.
    intake_id: Option<u64>,
}

/// Rough chars-per-token ESTIMATE — the well-known ~4-chars/token heuristic for English
/// text under the Claude tokenizer family.
///
/// **This is now the FALLBACK, not the watermark.** Continuity design §3.2 permits "agent
/// usage reporting OR AN ESTIMATE"; the adapter does report, so the estimate is used only
/// while the current lease has not yet said anything about itself. `context_source()`
/// answers which of the two is in force at any instant, and nothing surfaces the estimate
/// as a measurement.
///
/// **It is kept, rather than deleted, because a fresh lease genuinely has no measurement**
/// — see `ContextSource::Estimated`. It is also, measurably, a bad one, and the size of
/// the error is stated here rather than left as a feeling. Re-derived from the five probe
/// runs at `docs/verification/acp-emission-probe-2026-08-28/`, comparing what this
/// estimate would have counted (prompt chars + reply chars, ÷ 4) against the `used` the
/// adapter reported over the same turn:
///
/// | run | est. tokens | measured Δ`used` | undercount |
/// |-----|------------:|-----------------:|-----------:|
/// | 1   |         387 |            1_354 |      3.5×  |
/// | 2   |         649 |            2_138 |      3.3×  |
/// | 3   |         284 |           11_523 |     40.6×  |
/// | 4   |         716 |            2_431 |      3.4×  |
/// | 5   |          97 |              225 |      2.3×  |
///
/// (Byte lengths, because that is what `text.len()` and `assistant_text.len()` return and
/// therefore what this estimate is actually made of. The re-derivation lives in
/// `tests/watermark_cadence_tests.rs` and re-runs from the raw capture on every
/// `cargo test`, so these figures cannot quietly drift away from the data.)
///
/// Run 3 is the tool-heavy one. `deliver` adds `prompt.len() + reply.len()` and nothing
/// else, so every tool input and tool output — for an orchestrator Rich, the large
/// majority of context — is invisible to it. And the error is not a constant that could
/// have been calibrated out: 2.3× on the lightest run, 40.6× on the heaviest, same code,
/// same adapter, same day. It is unbounded in that direction, which is why it may never
/// again be the primary trigger.
pub(crate) const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

/// Fallback context-window budget, used ONLY while no `usage_update` has arrived for the
/// current lease. Overridable via `set_context_budget`.
///
/// **Measured and wrong by 5×, kept honest rather than raised.** All 50 `usage_update`
/// events across the five 2026-08-28 probe runs reported `"size": 1000000` — the
/// adapter's own statement of the window. This constant is not corrected to 1_000_000
/// because it is not a measurement of anything: it is the number to use when the lease
/// has told us nothing, and guessing HIGH there is the dangerous direction (it delays
/// rotation on the one path that has no real signal). Guessing low costs a rotation;
/// guessing high risks the hard wall.
pub(crate) const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 200_000;
/// continuity design §8 Q2's recommended starting point: "~70% as the starting point,
/// tuned in dogfood." Overridable via `set_context_budget`.
pub(crate) const DEFAULT_WATERMARK_RATIO: f64 = 0.70;
/// The fraction of the MEASURED window past which a lease is treated as in danger of
/// hitting the hard context wall **inside the turn that is currently running** — the one
/// place continuity §3.1 forbids rotating.
///
/// **Why a second, higher threshold exists at all.** The watermark rotates BETWEEN turns,
/// so the question it cannot answer is "can the turn now in flight finish?" Crossing the
/// watermark at 0.70 of a measured 1_000_000 leaves 300_000 tokens of headroom. The
/// largest single-turn consumption in the probe capture is run 3's 11_523 tokens
/// (`used` 30_468 → 41_991), so 300_000 ÷ 11_523 = **26.0 turns** of headroom: on measured
/// traffic a turn cannot cross from the watermark to the wall, and this threshold should
/// never fire. It exists for the traffic that was not measured.
///
/// 0.95 leaves 50_000 tokens = **4.3×** the largest measured turn. The point at which one
/// more turn the size of the biggest ever measured would not fit is 1 − 11_523/1_000_000 =
/// **0.9885**, so 0.95 fires with margin ahead of it.
///
/// **What crossing it does, and does not, do.** It does NOT rotate — rotation mid-turn is
/// structurally forbidden and a mid-turn rotation would be worse than a late one. It
/// records the crossing durably (an `Internal` action, invisible to the CEO) and forces
/// rotation at the very next boundary under the reason `context-critical`, ahead of any
/// ratio the operator configured. If the turn dies at the wall before reaching that
/// boundary, that is a positive termination signal from the adapter and `recover_and_replay`
/// (§5.3) is what catches it. **The client cannot prevent a mid-turn hard limit. It can
/// only make it rare, and say so when it happened.**
const CONTEXT_CRITICAL_RATIO: f64 = 0.95;

/// Where the rotation watermark's numerator and denominator came from, right now.
///
/// This type exists so a fallback can never masquerade as a measurement. The two states
/// are genuinely different — one is the adapter's own arithmetic, the other is a
/// chars÷4 heuristic measured to undercount by 2.3× to 40.9× (`CHARS_PER_TOKEN_ESTIMATE`)
/// — and every surface that reports context has to say which one it is holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSource {
    /// The current lease reported `usage_update` at least once. `used`/`size` are the
    /// adapter's, and the watermark is anchored to them.
    Measured,
    /// The current lease has not reported yet. The watermark is running on the chars÷4
    /// proxy over `DEFAULT_CONTEXT_WINDOW_TOKENS` — an ESTIMATE, and named as one
    /// everywhere it is surfaced. Every fresh lease starts here and leaves as soon as the
    /// first `usage_update` arrives, which the probe measured at 6.8 s / event n=8 into
    /// the first turn.
    Estimated,
}

impl ContextSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextSource::Measured => "measured",
            ContextSource::Estimated => "estimated",
        }
    }
    /// True only for `Measured`. Written as a method so a caller cannot accidentally
    /// treat "we have a number" as "we measured a number".
    pub fn is_measured(&self) -> bool {
        matches!(self, ContextSource::Measured)
    }
}

/// The `stop_reason` a CEO-stopped turn carries on the legacy `stream.rs` family, and in
/// the ledger. One string, in one place, so nothing downstream has to pattern-match on a
/// phrase somebody typed twice.
pub const STOPPED_BY_CEO: &str = "stopped_by_ceo";

/// Rotation reason: the lease moves to the account now in use, at a turn boundary (fill-first).
pub const ACCOUNT_SWITCH: &str = "account-switch";
/// Rotation reason: a turn was refused for a usage limit and is re-served on the next account.
pub const ACCOUNT_EXHAUSTED: &str = "account-exhausted";
/// Rotation reason: the lease moves to the model his setting now names (§119), at a turn
/// boundary — saved when Preferences closed, in use from the next reply.
pub const MODEL_CHANGED: &str = "model-changed";

/// The mouth every sentence typed or spoken at the Mac is recorded under, on an install that
/// keeps it (operator back-end spec r3 (s); `ledger::Event::PromptReceived::channel`). A
/// phone's is the intake record's own value, `"phone"`.
pub const DESK_CHANNEL: &str = "desk";

/// How far back the correction trigger looks for the wrong form it is being asked to fix.
///
/// Eight messages is four exchanges. It is a WINDOW rather than the whole thread for the
/// same reason `dictation.js`'s `MATCH_MIN_SIMILARITY` pairing has a ten-minute one: a
/// correction is made while the wrong word is still on screen, and matching against an
/// unbounded history turns an old coincidence into fresh "evidence". It is also the reason
/// the anchor is not a gate — see `spoken.rs` and the measurement: the window necessarily
/// misses a name he can still see but that scrolled past eight messages ago, and a gate
/// would have thrown that correction away rather than merely quoting nothing.
pub const ANCHOR_WINDOW_MESSAGES: usize = 8;

/// **THE DESK THAT IS PRIMED BEFORE THE THREAD IT WILL SERVE EXISTS** — the CEO's §55, and
/// the only shape that removes the seconds `docs/verification/first-words-2026-09-18-sendlock.md`
/// §2.2 measured and could not move.
///
/// # Why a spare has to exist at all, re-derived rather than assumed
///
/// A brand-new thread's record is created by `create_thread_in`, which `ui/main.js` calls
/// **inside the Send handler** (`ui/main.js:1688`, the `draftEntityId` branch, whose own
/// comment reads *"NOTHING was persisted when the CEO opened the new-thread screen"*), and
/// `openThread` -> `get_timeline` -> `ready_the_front_desk` runs microseconds later with
/// `send_message` right behind it. So on the path that matters the prime and the send are
/// back-to-back by construction: he pays the WHOLE priming turn, not a remainder of it, and
/// no hook in that chain can be moved early enough to change it. Starting the prime at
/// thread CREATION — the cheap null hypothesis §6 asked to be priced first — buys **nothing**,
/// because thread creation is already inside the send.
///
/// The window that IS free is the one before Send: the CEO is on the entity's new-thread
/// screen, typing. Nothing existed to prime during it. This is that thing.
///
/// # The invariant the thread scoping protects, and how it is kept
///
/// `EngineProfile::scope_to` pins `(entity_id, thread_id)` into the child's environment
/// (`RICHOS_APP_ENTITY` / `RICHOS_APP_THREAD`) and into its workspace partition
/// (`workspace_state`'s sha256 of that pair) **before the child is spawned**, and
/// `engine/mega-lander/app.py:62` refuses any dispatch whose binding disagrees with those two
/// variables. That is a real boundary, not bookkeeping: a spare spawned under thread A and
/// later adopted by thread B would be a child dispatching work under a name that is not its
/// own and writing into another conversation's workspace.
///
/// **So the scoping does not move.** The spare is spawned already bound to the thread it will
/// serve, under an id [`crate::ledger::Ledger::reserve_thread_id`] mints with no record behind
/// it. UX §3.3's *"no pre-created thread record until the CEO sends the first message"* is kept
/// to the letter: nothing is appended, nothing is listed, and an id he never spends costs
/// nothing because nothing was written.
///
/// # What it costs
///
/// One live `claude` child and one priming model turn per spare — measured, not estimated, in
/// `docs/verification/first-words-2026-09-18-primed.md` §2. It is the same priming turn his
/// first message pays for today, moved off his clock, plus — in the one case where he opens the
/// app, looks at a new-thread screen and never types — a turn that would not otherwise have
/// been spent. That is the identical bargain [`Spine::prime_front_desk`] already accepts and
/// names in its own documentation.
struct SpareFrontDesk {
    lease: Box<dyn Cognition>,
    /// The entity this spare was primed FOR. A spare primed for one entity is never adopted by
    /// another — the same entity-boundary rule `IntakeRecord::Steer::thread_id` states — and the
    /// re-prime payload is entity-scoped (`reprime.rs`'s action digest, `company_block`), so a
    /// cross-entity adoption would hand one company's Rich to another company's first sentence.
    entity: EntityId,
    /// The id this desk is scoped to: reserved and unspent. **No ledger record carries it.**
    reserved_thread: String,
    /// Whether its priming turn completed. A spare can be un-primed after the fact — the central
    /// folder moved, the memory wiring was torn down — and an un-primed spare is NOT discarded:
    /// it is adopted anyway and primes at his first send, which is exactly what every thread did
    /// before this existed. The degrade is the old path rather than a new failure.
    primed: bool,
    /// The company material it was primed with, moved into the chair with it. Without this the
    /// chair compares the adopted desk against whatever the PREVIOUS desk was primed with and
    /// re-primes it for nothing — which is the defect run H measured.
    onboarding_block: Option<String>,
    /// The watermark baseline its priming turn established, moved into the chair with it.
    context_chars: usize,
}

/// **What [`Spine::begin_a_spare`] decided, carried across the two steps that run with the
/// spine's mutex DOWN.** It owns everything those steps need and borrows nothing from the
/// `Spine`, which is the property that makes the unlocked road possible at all.
struct SparePlan {
    entity: EntityId,
    binding: ThreadBinding,
    reserved: String,
    /// A second handle to the same factory, or `None` when this factory cannot hand one out —
    /// see [`crate::cognition::LeaseFactory::duplicate`]. `None` puts the spawn back under the
    /// lock and changes nothing else.
    factory: Option<Box<dyn crate::cognition::LeaseFactory>>,
    control: crate::steering::TurnControl,
    /// What `front_desk_generation` read when this was decided. Step 5 compares.
    generation: u64,
    started: std::time::Instant,
}

/// What the detached child is told, and the durable claim that says it was told.
struct SparePriming {
    priming: String,
    onboarding_block: Option<String>,
    scope: Option<crate::onboarding_tools::OnboardingToolScope>,
    /// The ledger action, claimed in step 3 and settled in step 5.
    action: String,
}

/// **Step 4: the priming turn itself, against a lease nobody else can see.**
///
/// A free function and not a method, deliberately — it takes no `&Spine` at all, and that is
/// the proof that it can run with the mutex down. The machinery it collects is emitted by
/// step 5, which has the journal and the observer.
///
/// **The between-turn residue is drained and DISCARDED**, where the chair's is stamped against
/// its thread. A fresh child announces its commands on the between-turn lane at session start;
/// `pump_between_turn_stamped` would refuse the reserved binding and leave those records
/// sitting in the lease, where the CEO's FIRST REAL TURN would drain them and stamp them
/// against a turn he is watching. That is the rotation-tell shape §1.5 exists to exclude,
/// arriving by a new road. They belong to a desk he has never seen, so they are read off and
/// dropped here.
fn run_a_spare_priming_turn(
    lease: &mut dyn Cognition,
    plan: &SparePlan,
    priming: &SparePriming,
    machinery: &mut Vec<crate::machinery::MachineryRecord>,
) -> Result<(), SpineError> {
    if let Some(scope) = &priming.scope {
        lease.set_onboarding_scope(plan.binding.entity_id(), &scope.central_root, &scope.record_path)?;
    }
    let mut on_item = |item: TurnItem| {
        if let TurnItem::Machinery(record) = item {
            machinery.push(record);
        }
    };
    lease.reprime(&priming.priming, &mut on_item)?;
    // **AND THE CONTEXT BRIEF, HERE, because nothing later can supply it before he speaks.** An
    // adopted spare keeps `primed`, so `prime_lease_if_needed` returns before its own
    // `supply_context_ahead`, and his first turn paid the hidden round trip the chair no longer
    // pays (review rv-20261014T073429Z-4fd01c4c-58c4). Later is too late anyway: `ui/main.js`
    // creates the thread inside the Send handler. The reserved id IS the thread's id, so the
    // brief read now is the one his turn reads; the turn id is this priming's ledger action,
    // the only durable id a thread with no record has. A failure costs his turn the round trip.
    if let Err(e) = lease.supply_context_ahead(&plan.binding, &priming.action, &mut on_item) {
        eprintln!("[richos] the spare desk's context brief could not be supplied before he speaks ({e}); his turn supplies it");
    }
    let _ = lease.drain_between_turn();
    Ok(())
}

/// **Ready a spare WITHOUT holding the spine across the spawn and the priming turn** — the
/// CEO's §55, and the shape the shell takes.
///
/// The same five steps [`Spine::ready_a_spare_front_desk`] runs, in the same order, with the
/// mutex taken for steps 1, 3 and 5 and DOWN for 2 and 4. Nothing about what is produced
/// differs; what differs is that a window command arriving in the middle is served in
/// microseconds instead of waiting out a model turn.
///
/// **Step 2 falls back under the lock when the factory cannot be duplicated**
/// ([`crate::cognition::LeaseFactory::duplicate`] answering `None`) — today that is every test
/// double and nothing that ships. The fallback is the old behavior exactly, which is correct
/// and merely slower.
///
/// **A poisoned mutex is not a verdict about the spare.** It is reported as `NotReady` and
/// nothing is left in flight: the flag lives inside the spine, so a poisoned spine has already
/// lost the flag with everything else.
pub fn ready_a_spare_front_desk_without_the_spine(
    spine: &std::sync::Mutex<Spine>,
    entity: &EntityId,
) -> SpareReady {
    macro_rules! hold {
        () => {
            match spine.lock() {
                Ok(guard) => guard,
                Err(_) => return SpareReady::NotReady("the spine's mutex is poisoned".into()),
            }
        };
    }
    let plan = match hold!().begin_a_spare(entity) {
        Ok(plan) => plan,
        Err(verdict) => return verdict,
    };
    // ---- step 2, with the mutex down whenever the factory allows it ----------------------
    let spawned = match &plan.factory {
        Some(factory) => factory.spawn_scoped(&plan.binding, &plan.control),
        None => hold!().spawn_a_spare_here(&plan),
    };
    let mut lease = match spawned {
        Ok(lease) => lease,
        Err(e) => return hold!().abandon_a_spare(plan, None, e.to_string()),
    };
    let priming = match hold!().priming_for_a_spare(&plan, lease.as_ref()) {
        Ok(priming) => priming,
        Err(e) => return hold!().abandon_a_spare(plan, None, e.to_string()),
    };
    // ---- step 4, ALWAYS with the mutex down. This is the model turn. ---------------------
    let mut machinery = Vec::new();
    if let Err(e) = run_a_spare_priming_turn(lease.as_mut(), &plan, &priming, &mut machinery) {
        return hold!().abandon_a_spare(plan, Some(priming), e.to_string());
    }
    hold!().install_the_spare(plan, lease, priming, machinery)
}

/// **What happened when the app asked for a spare front desk** —
/// [`Spine::ready_a_spare_front_desk`]'s answer, and a deliberate mirror of [`FrontDeskReady`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpareReady {
    /// A spare was spawned and primed by this call. `millis` is what the CEO's first message
    /// into the next brand-new thread would otherwise have waited through.
    Ready { millis: u64 },
    /// A primed spare for this entity was already standing there: nothing was spent, nothing
    /// was sent.
    AlreadyReady,
    /// A turn is in flight. Priming the spare is a model turn on its OWN child, so it breaks no
    /// continuity-§3.1 invariant — but the caller holds the spine's mutex for the whole of it,
    /// and taking it here would put a second multi-second hold in front of everything the
    /// running turn needs. Refused for the reason [`FrontDeskReady::TurnInProgress`] is.
    TurnInProgress,
    /// It could not be done: no lease factory, an unregistered entity, or the priming turn
    /// itself failed. The next brand-new thread then primes the way it does today, which is the
    /// only cost, and nothing is surfaced to the CEO.
    NotReady(String),
}

/// **ONE THREAD'S FRONT DESK, PARKED WHILE ANOTHER THREAD IS SPEAKING** — the CEO's Two
/// Riches page: *"each conversation thread always holds one front desk Rich and one
/// back-end Rich"*, and *"each conversation thread in the app can go forever."*
///
/// Before this, a thread switch called `clear_lease` and the outgoing child was KILLED:
/// coming back meant a fresh provider session, a fresh `claude` process and a full re-prime
/// off the ledger tail. His sentence was true as reconstitution and false as residence, and
/// the difference is visible — the re-primed Rich has only what the payload carried.
///
/// A parked lease is a live child with its own provider session, its own priming and its
/// own context accounting. Nothing here is a copy of the spine's state: the fields are
/// MOVED out of the chair and moved back into it, so there is never a second opinion about
/// how much context a session has used.
struct Resident {
    lease: Box<dyn Cognition>,
    primed: bool,
    /// The company material THIS desk was primed with — see [`Spine::onboarding_primed_block`].
    /// Parked and restored with the lease, because it is a fact about the lease.
    onboarding_block: Option<String>,
    /// Its own consumption, so a thread coming back is not rotated on another thread's
    /// numbers. `install_lease` clears these for a FRESH lease and this restores them for
    /// a returning one — a resident that inherited the chair's usage would rotate itself on
    /// its first turn back.
    context_chars: usize,
    context_usage: Option<ContextUsage>,
    context_pressure: Option<(String, ContextUsage)>,
    /// When this thread last had the chair. The eviction order and nothing else.
    parked_at_ms: u64,
    /// The renewal THIS desk is owed, and since when it has waited for a command — part 1 hunt
    /// finding 42. `Spine::pending_rotation_reason` and `rotation_deferred_since` describe
    /// whichever lease sits in the chair, exactly like the usage numbers above, so they travel
    /// with the desk. Left behind, the reason was consumed by the NEXT thread's first request
    /// and renewed the wrong (just resumed) session, while the desk that reached its limit
    /// stayed parked with no renewal scheduled.
    pending_rotation_reason: Option<String>,
    rotation_deferred_since: Option<std::time::Instant>,
}

/// How many front desks stay resident at once, the one in the chair included.
///
/// **His page says "any number of conversation threads" and this is not a limit on that.**
/// A thread is a ledger partition and there can be thousands; this bounds how many live
/// `claude` children the app holds open at one time, which is a memory and process
/// question rather than a product one. Past the cap the LEAST RECENTLY SPOKEN parked desk
/// is retired, which puts exactly that thread back to the behavior every thread had before
/// this change: its next turn spawns a fresh lease and re-primes from the ledger. So the
/// degradation is the old path rather than a new failure, and it never touches the desk in
/// the chair or the one he is typing into.
///
/// Eight, because that is more conversations than the CEO has ever had open at once and
/// eight idle children is a bounded, honest footprint. It is a number to revisit with a
/// measurement, not a constant with an argument behind it.
pub const MAX_RESIDENT_FRONT_DESKS: usize = 8;

/// **What happened when the app asked for the front desk to be ready BEFORE he types** —
/// [`Spine::prime_front_desk`]'s answer.
///
/// Every variant is a fact about a LEASE, never a promise about a turn: nothing here changes what
/// his next message does, only when the waiting for it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontDeskReady {
    /// The lease was spawned and/or primed by this call. `millis` is what he would otherwise have
    /// waited through on his first message.
    Ready { millis: u64, spawned: bool },
    /// It was already primed for this thread: nothing was spent and nothing was sent.
    AlreadyReady,
    /// A turn is in flight, so nothing was done. Priming is NEVER run underneath a turn — the
    /// lease is the thing running it, and a second prompt into it is what the continuity design
    /// forbids structurally (§3.1).
    TurnInProgress,
    /// It could not be done: the thread does not exist, has no entity home, or the priming turn
    /// itself failed. His first message then primes the way it always did, which is the only cost.
    NotReady(String),
}

pub struct Spine {
    ledger: Ledger,
    reader: Option<crate::read_view::SpineReader>,
    lease: Option<Box<dyn Cognition>>,
    /// **The other front desks** — every thread that has spoken, keyed by thread id, minus
    /// whichever one currently holds [`Spine::lease`]. See [`Resident`].
    resident: std::collections::HashMap<String, Resident>,
    /// **The desk for the thread he has not started yet.** See [`SpareFrontDesk`].
    ///
    /// **ONE, and the bound comes from the product rather than from the price.** A spare covers
    /// "the next brand-new thread", and there is exactly one of those at any instant: the app
    /// has one composer and one new-thread screen, so a second spare could only ever be adopted
    /// by a second brand-new thread opened before the first had been spoken in, which no
    /// sequence of clicks can produce. `MAX_RESIDENT_FRONT_DESKS` bounds desks he HAS used and
    /// answers a different question.
    spare: Option<SpareFrontDesk>,
    /// **A spare's spawn and priming turn are in flight, and for part of that time this mutex
    /// is DOWN** — see [`ready_a_spare_front_desk_without_the_spine`]. The slot is reserved by
    /// step 1 and released by step 5, so a second ask arriving in the gap is answered
    /// `AlreadyReady` instead of spawning a second child for one slot.
    spare_in_flight: bool,
    /// **Bumped every time the app stops believing in what its desks were primed with.**
    ///
    /// [`Spine::unprime_every_front_desk`] can now run BETWEEN the steps of a spare's priming,
    /// because those steps no longer hold one lock end to end. Without a generation the finished
    /// spare would be installed `primed: true` carrying material the app had already discarded —
    /// silently, until it rotated on its own watermark, which is the exact defect that method's
    /// own documentation exists to prevent.
    front_desk_generation: u64,
    /// THE ACTIVE CONTEXT (ECS §3.3): person + entity + thread + binding revision. Not a
    /// bare thread id — holding the full binding is what lets every downstream call be
    /// scoped without re-deriving (or re-guessing) the entity.
    active: Option<ThreadBinding>,
    /// The entity areas this spine will accept. **EMPTY by default** — the registry
    /// belongs to the person running the app and is loaded from their own configuration
    /// file (`entity.rs` rule 4), so a spine that nobody has told about any company accepts
    /// none. Membership is checked on thread creation, so an unregistered entity can never
    /// become a thread's immutable home; with an empty registry that means every
    /// `create_thread` is refused, which is the correct and honest state of an install
    /// whose owner has not said which company they work for yet.
    ///
    /// The shell installs the real one with [`Spine::set_entity_registry`] as soon as it
    /// has read the file.
    registry: EntityRegistry,
    /// The turn-boundary controller state. Keyed on turn-in-progress (NOT workers-live):
    /// a turn can END while engine subagents keep running, so delivery/rotation proceeds
    /// the moment the turn ends. Rotation NEVER happens inside a turn.
    turn_in_progress: bool,
    queue: VecDeque<Queued>,
    /// **The conversations whose turns have ENDED since the shell last asked** — one binding
    /// per thread, in the order their first turn ended. See [`Spine::take_ended_turns`].
    ended_turns: Vec<ThreadBinding>,
    input_channels: std::collections::HashMap<String,String>,
    /// Set true once the current lease has been re-primed (continuity foundation).
    lease_primed: bool,
    lease_primed_thread: Option<String>,
    /// The live UI sink (streaming deltas + turn state). Optional so the spine runs
    /// headless (tests, the round-trip examples) with zero UI attached.
    observer: Option<Box<dyn TurnObserver>>,
    /// Spawns a fresh, un-primed lease at rotation/recovery time. `None` means the spine
    /// can prime/run ONE lease (whatever was `attach_lease`d) but can never rotate or
    /// recover from a crash — an honest degrade for tests/contexts with no respawn story.
    lease_factory: Option<Box<dyn LeaseFactory>>,
    /// The optional Tier-C seam (continuity §2.3/§4) — a different engineer's parallel
    /// work on `loro/`. Absent by default; the re-prime payload degrades gracefully.
    loro_compiler: Option<Box<dyn LoroContextCompiler>>,
    /// The optional Tier-E seam — the evidence lookup over the CEO's own files (the
    /// 2026-09-17 evidence-retrieval ruling §1/§4). Absent by default, exactly like the
    /// compiler above, and the payload degrades to `EvidenceTier::NotWired`.
    ///
    /// **It is gated on the compiler, not merely placed beside it.** The lookup runs only
    /// when the compiled slice's own `coverage` says memory did not answer, so an install
    /// with a lookup and no compiler never looks at anything — there is no coverage label,
    /// and absence of the signal is not the signal.
    evidence_lookup: Option<Box<dyn EvidenceLookup>>,
    /// Cumulative prompt+reply chars sent/received on the CURRENT lease since it was
    /// last (re)primed — the context-watermark measurement (see `CHARS_PER_TOKEN_ESTIMATE`).
    context_chars: usize,
    /// The MEASURED context usage the current lease last reported (`usage_update`'s
    /// `{used, size}`). `None` means this lease has not reported yet and the watermark is
    /// running on the estimate above — a genuinely different state, exposed as
    /// [`ContextSource`] rather than smoothed over.
    ///
    /// Cleared in `install_lease`, which is the only place `self.lease` is assigned: a
    /// successor inherits none of its predecessor's consumption, and carrying the old
    /// number forward would rotate every fresh lease instantly.
    context_usage: Option<ContextUsage>,
    /// Set when a `usage_update` arriving DURING a turn crosses `CONTEXT_CRITICAL_RATIO`.
    /// Carries the turn it crossed in, because that is the turn the durable record
    /// belongs to. Consumed at the next turn boundary; see `after_turn_boundary`.
    context_pressure: Option<(String, ContextUsage)>,
    context_window_tokens: usize,
    watermark_ratio: f64,
    /// An explicit rotation request (continuity §3.2 "Explicit (!rotate equivalent)").
    /// If set while a turn is in flight, honored at the NEXT turn boundary instead of
    /// firing mid-turn.
    pending_rotation_reason: Option<String>,
    /// Since when a due watermark renewal has waited for a command the lease started (the
    /// product reap gap design C6, the same rule as `work_host.rs`'s back end).
    rotation_deferred_since: Option<std::time::Instant>,
    /// **Keep the mouth his words came through on each prompt record** (operator back-end
    /// spec r3 (s)). Off unless the shell turns it on, which it does only on an install whose
    /// `operator.json` passed the gate at launch, so every product install writes exactly the
    /// ledger lines it wrote before. See `ledger::Event::PromptReceived::channel`.
    keep_intake_channel: bool,
    /// Proactive-message UI events deferred because a turn was in flight when raised.
    rotation_count: u64,
    last_rotation_reason: Option<String>,
    /// The machinery journal (techy-mode design §2.1). `None` means machinery is routed
    /// and emitted but NOT retained — an honest degrade for tests and headless runs, not
    /// a supported product state: §3.2's rule is that retention runs ALWAYS, because the
    /// CEO's requirement is to flip a thread he ALREADY HAD.
    machinery_journal: Option<MachineryJournal>,
    /// The live machinery sink. A SEPARATE observer from `TurnObserver` on purpose — two
    /// families means the default UI's subscription list is the proof that the calm view
    /// carries no machinery (§3.3).
    machinery_observer: Option<Box<dyn MachineryObserver>>,
    /// THE OUTPUT RECORD (Output side panel PRD §3-§4) and the evidence folder its witnesses
    /// (b) and (c) read, `<app-data>/engine-state/evidence`. `None` — every test and headless
    /// run — records nothing and behaves exactly as before; the shell sets both
    /// ([`Spine::set_output_store`]).
    output: Option<(crate::output::OutputStore, Option<PathBuf>)>,
    /// The THIRD live sink: the additive §13 event family (`live.rs`). Separate from
    /// `observer` on purpose — the four `stream.rs` events are unchanged and a UI that
    /// listens only to them is unaffected by anything on this one.
    live: Option<Box<dyn LiveObserver>>,
    /// THE FLYWHEEL'S AUTOMATIC TRIGGER (`spoken.rs` + `staging.rs`). Optional: a build
    /// with no desk attached behaves exactly as this file did before it existed. Held as a
    /// `SharedCandidateDesk` (an `Arc<Mutex<_>>`) rather than owned, so answering §7's
    /// question never waits on the turn lock — see `staging::SharedCandidateDesk`.
    candidates: Option<SharedCandidateDesk>,
    /// The dictation journal the heard-vs-sent trigger reads. `None` on every install
    /// without a journalling dictation app, and on every install where the CEO has not
    /// switched the trigger on — see [`Spine::set_heard_source`].
    heard: Option<Box<dyn crate::heard::HeardSource>>,
    /// THE LORO DESK, and the memory it resolves against (`belief.rs` + `loro.rs`).
    /// Optional as a PAIR: a build with a desk and no provenance can resolve nothing, and a
    /// build with provenance and no desk has nowhere to file. Both absent is the ordinary
    /// state of an install with no corpus.
    correction_desk: Option<SharedCorrectionDesk>,
    loro_provenance: Option<SharedSliceProvenance>,
    proposal_observer: Option<Box<dyn ProposalObserver>>,
    /// Where a staged correction is announced. A FOURTH family, separate from the other
    /// three for the reason `machinery_observer` is separate from `observer`.
    correction_observer: Option<Box<dyn CorrectionObserver>>,
    /// THE CEO'S TWO MID-TURN CONTROLS (UX §9.2/§9.3), and the only thing in this struct
    /// that is reachable while a turn is running.
    ///
    /// Everything else here is behind `&mut self`, which `deliver()` holds for the whole
    /// turn. `TurnControl` is an `Arc` the shell keeps its own handle on, so a stop can be
    /// recorded and delivered to the lease without ever waiting for this spine's lock —
    /// which is the difference between a stop control and a button that fires after the
    /// work it meant to interrupt has finished.
    ///
    /// Defaults to `TurnControl::detached()`: turn bookkeeping works, stop and steer are
    /// REFUSED, because there is nowhere durable to record the request. Every pre-existing
    /// test and headless run therefore behaves exactly as it did.
    control: TurnControl,
    /// THE CENTRAL FOLDER, or `None` when nobody has told this spine where it is.
    ///
    /// `None` is the shipped default and it is a real state rather than a placeholder: every
    /// headless example and every test in this crate runs without one and gets exactly the
    /// priming turn it got before this field existed. The Tauri shell sets it at boot
    /// ([`Spine::set_central_root`]).
    ///
    /// What it buys is `company.rs`: `<central>/companies/<entity>/company.md`, read at prime
    /// time and appended to the scoped assertion — `richos-central-folder-2026-09-06.md` §6,
    /// right-hand column, the half that "waits on the app".
    central_root: Option<std::path::PathBuf>,
    /// Where `onboarding.rs`'s declination record lives, or `None` when nobody has said.
    /// `None` behaves as "no declination on file", which leans toward asking — see
    /// `OnboardingRecord::load` on why the failure leans that way and only that way.
    onboarding_record: Option<std::path::PathBuf>,
    /// **The company material THE CHAIR'S DESK was last primed with.** External MCP
    /// saves/declines invalidate it, and `prime_lease_if_needed` re-primes when what the
    /// thread about to speak needs is not what the desk in the chair is holding.
    ///
    /// **It travels with the desk** — parked into [`Resident`], carried on
    /// [`SpareFrontDesk`], restored by `resume_front_desk`. It did not until 2026-09-19, and a
    /// spine-wide field describing one lease was wrong the moment a second lease could take the
    /// chair. A spare front desk made that cost the whole slice: the spare primed WITH the
    /// company block, the chair's field still read `None`, and the comparison below un-primed
    /// the adopted desk and spent the priming turn again on his clock — 2.123 s of run H, with
    /// the spare's own 4.709 s thrown away. It is also why a parked desk could be re-primed for
    /// nothing after a switch between two companies.
    onboarding_primed_block: Option<String>,
    /// Where [`Spine::timeline`] reads the engine's worker-lifecycle stream from, so a
    /// `Task` tool call can be joined to the worker it spawned (UX §7).
    ///
    /// Defaults to [`WorkerEventsSource::Disabled`], which is what shipped before this
    /// slice: `Timeline::project` supplied an EMPTY stream, so `project_with_workers`
    /// existed and was reachable only from tests. Nothing in the app ever set it, which
    /// meant `TimelineItem::WorkerActivity` could not occur on the wire at all.
    worker_events: WorkerEventsSource,
    /// **THE BOUNDED, VISIBLE RETRY ALLOWANCE for upstream model-API failures**
    /// (`open-items.md` row 3.30, measured 2026-09-03).
    ///
    /// It lives on the SPINE and not on a turn, and that is the whole of the fix. A budget
    /// scoped to one turn is exactly the unbounded case the row measured, one turn at a
    /// time: `deliver` already refused a second recovery WITHIN a turn (`allow_recovery:
    /// false`), and the four consecutive attempts that produced nothing on 2026-09-03 were
    /// four separate turns. Consecutive failures charge it; any completed turn restores it,
    /// which is a POSITIVE signal that the upstream works rather than the absence of a
    /// failure.
    upstream_budget: crate::upstream::RetryBudget,
    /// How an overload retry waits out the CEO's schedule (`upstream::OVERLOAD_RETRY_WAITS`).
    /// The real clock in the app; a test drives the 158 minutes with a fake one.
    retry_clock: std::sync::Arc<dyn crate::upstream::RetryClock>,
}

/// Where a timeline read finds the engine's worker-lifecycle rows.
///
/// A source rather than a `Vec` because the stream is APPEND-ONLY and long-lived: a read
/// must see the rows written since the last one, and the team directory a session writes
/// to can change without the app relaunching. Both non-disabled variants re-read on every
/// call for that reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEventsSource {
    /// No stream. Every `Task` call stays an ordinary activity row — the honest degrade
    /// when the engine's lifecycle hooks are not registered (they snapshot at session
    /// start, so a freshly installed emitter writes its first row only in the NEXT
    /// session).
    Disabled,
    /// A fixed file. Tests, and an explicit operator override.
    File(PathBuf),
    /// Resolve THIS SESSION's team directory on every read
    /// (`worker_status::resolve_team_dir`, honoring `RICHOS_TEAM_DIR`) and read its
    /// `worker-events.jsonl`.
    ///
    /// Re-resolved per read rather than bound at boot because the lease is swappable: a
    /// rotation mints a new session id, and the directory follows it. It is derived from
    /// that id and NEVER from a directory mtime — see `worker_status.rs`, "WHOSE workers
    /// these are". When no directory can be attributed, the stream is EMPTY, which costs
    /// exactly one thing (a `Task` call stays an ordinary activity row) and claims nothing.
    ///
    /// The home fallback (`~/.claude/worker-events.jsonl`) is deliberately never resolved —
    /// it accumulates across every session that ever missed a team dir and cannot be
    /// session-scoped (`worker_events.rs`, "Session scope").
    CurrentTeamDir,
}

impl WorkerEventsSource {
    /// Read the rows this source currently holds, for the session `session_id` names.
    ///
    /// A missing/unreadable file is an EMPTY stream, never an error: "no worker events" is
    /// a true and common state. So is `session_id: None` — no lease, therefore no
    /// directory this app is entitled to, therefore no rows.
    pub fn read(&self, session_id: Option<&str>) -> Vec<WorkerEventRow> {
        match self {
            WorkerEventsSource::Disabled => Vec::new(),
            WorkerEventsSource::File(p) => worker_events::read_stream(p),
            WorkerEventsSource::CurrentTeamDir => {
                match crate::worker_status::resolve_team_dir(session_id) {
                    Ok(dir) => worker_events::read_stream(&worker_events::worker_events_path(&dir)),
                    Err(_) => Vec::new(),
                }
            }
        }
    }
}

impl Spine {
    /// Share immutable window snapshots without sharing the turn's lock or writer.
    pub fn reader(&mut self) -> crate::read_view::SpineReader {
        if let Some(reader) = &self.reader { return reader.clone(); }
        let reader = crate::read_view::SpineReader::new(self);
        let updates = reader.clone();
        self.ledger.observe_reads(std::sync::Arc::new(move |event| updates.apply(event.clone())));
        self.reader = Some(reader.clone());
        reader
    }

    fn publish_read_metadata(&self) {
        if let Some(reader) = &self.reader { reader.refresh(self); }
    }

    pub(crate) fn read_metadata(&self) -> crate::read_view::ReadMetadata {
        crate::read_view::ReadMetadata {
            active: self.active.clone(), registry: self.registry.clone(),
            machinery_root: self.machinery_journal.as_ref().map(|j| j.root().to_path_buf()),
            workers: self.worker_events.clone(), control: self.control.clone(),
            central_root: self.central_root.clone(), onboarding_record: self.onboarding_record.clone(),
        }
    }

    pub fn new(ledger: Ledger) -> Self {
        Spine {
            ledger,
            reader: None,
            lease: None,
            resident: std::collections::HashMap::new(),
            spare: None,
            spare_in_flight: false,
            front_desk_generation: 0,
            active: None,
            registry: EntityRegistry::empty(),
            turn_in_progress: false,
            control: TurnControl::detached(),
            central_root: None,
            onboarding_record: None,
            onboarding_primed_block: None,
            queue: VecDeque::new(),
            ended_turns: Vec::new(),
            input_channels: std::collections::HashMap::new(),
            lease_primed: false,
            lease_primed_thread: None,
            observer: None,
            lease_factory: None,
            loro_compiler: None,
            evidence_lookup: None,
            context_chars: 0,
            context_usage: None,
            context_pressure: None,
            context_window_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
            watermark_ratio: DEFAULT_WATERMARK_RATIO,
            pending_rotation_reason: None,
            rotation_deferred_since: None,
            keep_intake_channel: false,
            rotation_count: 0,
            last_rotation_reason: None,
            machinery_journal: None,
            machinery_observer: None,
            output: None,
            live: None,
            candidates: None,
            heard: None,
            correction_observer: None,
            correction_desk: None,
            loro_provenance: None,
            proposal_observer: None,
            worker_events: WorkerEventsSource::Disabled,
            upstream_budget: crate::upstream::RetryBudget::new(),
            retry_clock: std::sync::Arc::new(crate::upstream::SystemRetryClock),
        }
    }

    /// Point the timeline read path at the engine's worker-lifecycle stream (UX §7).
    ///
    /// Without this a `Task` call projects as an ordinary activity row reading *"Worked"*
    /// — no name, no state, no chip — because there is no identity to join by. With it,
    /// a `Task` call that carries an extractable `agentId` AND has at least one row in the
    /// stream **for the same session** becomes a `TimelineItem::WorkerActivity`.
    ///
    /// ## The id spaces match — settled 2026-08-29, no longer a caveat
    /// The join's session clause compares the MACHINERY record's `session_id` — the lease's
    /// session id the adapter minted (`Cognition::session_id`) — against the worker row's
    /// `session_id`, which the engine hook read from the Claude Code harness. This used to
    /// say the two might be different id spaces and could not be measured in this checkout.
    /// They are the SAME: the probe's ACP session id
    /// `55c79b81-ace3-4b07-a5f3-406853ac1a36`
    /// (`docs/verification/acp-emission-probe-2026-08-28/run1.raw.jsonl`) has a Claude Code
    /// transcript at `~/.claude/projects/-Users-alex-ab-richos-engine/55c79b81-….jsonl`. So
    /// the join can fire in production, and `WorkerEventsSource::CurrentTeamDir` derives its
    /// directory from that same id.
    ///
    /// A refusal is still possible and still reported — it now means one thing,
    /// "genuinely another session's worker", surfaced as
    /// `RejectionReason::WorkerSessionMismatch` so it can be told apart from "the engine
    /// emitted nothing".
    ///
    /// Loosening the session clause to make the join fire is not an option: `agent_id` is
    /// not globally unique (the engine's own residue reuses one id across twelve rows) and
    /// the clause is what stops another session's worker name and authored summary
    /// rendering inside this entity's thread (timeline.rs `project_with_workers`).
    pub fn set_worker_events(&mut self, source: WorkerEventsSource) {
        self.worker_events = source;
        self.publish_read_metadata();
    }

    pub fn worker_events_source(&self) -> &WorkerEventsSource {
        &self.worker_events
    }

    /// Attach the live UI sink. The spine emits turn-start, per-delta chunk, and
    /// terminal (completed/error) events to it — keyed to thread + turn — while the
    /// ledger remains the source of truth. Headless callers simply never set one.
    pub fn set_observer(&mut self, observer: Box<dyn TurnObserver>) {
        self.observer = Some(observer);
    }

    /// Emit one live turn event to the UI sink, if attached. Infallible: a missing or
    /// non-listening UI never affects the turn (the ledger already holds the truth).
    fn emit(&self, event: StreamEvent) {
        if let Some(obs) = self.observer.as_deref() {
            obs.on_event(&event);
        }
    }

    /// Attach the ADDITIVE live-work sink (UX brief §13). Optional and independent: the
    /// four `stream.rs` events fire identically whether or not this is set, so the
    /// shipping UI is unaffected by its presence or absence.
    pub fn set_live_observer(&mut self, observer: Box<dyn LiveObserver>) {
        self.live = Some(observer);
    }

    /// THE ONE CHOKEPOINT for the additive family — every `LiveEvent` in this file goes
    /// through here or through [`Spine::forward_live`], which is this function's body.
    fn emit_live(&self, events: Vec<LiveEvent>) {
        Self::forward_live(self.live.as_deref(), events);
    }

    /// The gate, as an associated function so the streaming closure in `deliver` (which
    /// holds a `&mut Ledger` and therefore cannot call `&self` methods) enforces the SAME
    /// rule rather than a second copy of it.
    ///
    /// **Only `Visibility::Ceo` leaves this process on the additive family.** An internal
    /// item (re-prime, rotation, model reasoning, a Tier-3 silent proactive message) and a
    /// technical-only item are both refused here; technical detail already has its own
    /// family, `rich://machinery`, which the calm view does not subscribe to. The item was
    /// CONSTRUCTED before being refused, deliberately: a guard that is never asked cannot
    /// be tested, and `tests/live_event_tests.rs` removes this line to watch it leak.
    fn forward_live(observer: Option<&dyn LiveObserver>, events: Vec<LiveEvent>) {
        let Some(obs) = observer else { return };
        for e in events {
            if e.may_reach_webview() {
                obs.on_live_event(&e);
            }
        }
    }

    /// §13 `rich://thread-summary-updated` for one thread, computed exactly the way
    /// `thread::summaries` computes the sidebar today — same title, same scoped message
    /// count, same recency — so a live row and a re-listed row can never disagree.
    fn thread_summary_event(&self, binding: &ThreadBinding, turn_id: &str, status: ThreadStatus) -> Vec<LiveEvent> {
        let thread_id = binding.thread_id();
        let Some(thread) = self.ledger.threads().iter().find(|t| t.id == thread_id) else {
            return Vec::new();
        };
        let message_count = self.ledger.messages(thread_id).map(|m| m.len()).unwrap_or(0);
        let last_activity = self
            .ledger
            .turns()
            .iter()
            .filter(|tn| tn.thread_id == thread_id)
            .map(|tn| tn.created_at)
            .max()
            .unwrap_or(thread.created_at);
        vec![LiveEvent::ThreadSummaryUpdated {
            fence: EventFence::for_turn(binding, turn_id),
            title: thread.title.clone(),
            message_count,
            last_activity,
            status,
            at: now_millis(),
        }]
    }

    /// `rich://ceo-message` — his own sentence, read back OUT of the ledger for the reason
    /// [`Spine::turn_status_event`] is: the wire must carry what survived the write.
    ///
    /// **THE ID AND THE INSTANT ARE THE PROJECTION'S**, `{turn_id}:user` and `turn.created_at`,
    /// which is what makes a consumer that merges on `id` see one row rather than two — see
    /// [`LiveEvent::CeoMessage`]. They are derived from the same turn record `timeline.rs`
    /// derives them from, so a change in `timeline.rs` that moved either would be caught by
    /// `tests/live_event_tests.rs`'s wire-and-reload agreement rather than by a phone.
    ///
    /// Returns nothing at all for a turn with no CEO text — a proactive turn has none by
    /// construction, and the projection contributes no user item for one either.
    fn ceo_message_event(&self, binding: &ThreadBinding, turn_id: &str) -> Vec<LiveEvent> {
        let Some(turn) = self.ledger.turn(turn_id) else { return Vec::new() };
        // A pressed button's words are not his, so they are never sent as his row (finding 14).
        if turn.user_text.is_empty() || turn.source == Source::Proactive || turn.pressed {
            return Vec::new();
        }
        vec![LiveEvent::CeoMessage {
            fence: EventFence::for_turn(binding, turn_id),
            message_id: format!("{turn_id}:user"),
            text: turn.user_text.clone(),
            source: turn.source,
            created_at: turn.created_at,
            client_id: turn.client_id.clone(),
            voice_note: turn.voice_note,
            at: now_millis(),
        }]
    }

    /// §13 `rich://turn-status`, read back OUT of the ledger rather than assembled from
    /// the values in hand: `started_at` and the MEASURED `active_ms` are whatever survived
    /// the write, so the wire cannot report a span the ledger does not hold.
    fn turn_status_event(
        &self,
        binding: &ThreadBinding,
        turn_id: &str,
        status: TurnStatus,
        supersedes_turn_id: Option<String>,
    ) -> Vec<LiveEvent> {
        let turn = self.ledger.turn(turn_id);
        vec![LiveEvent::TurnStatus {
            fence: EventFence::for_turn(binding, turn_id),
            status,
            started_at: turn.and_then(|t| t.started_at),
            active_duration_ms: turn.and_then(|t| t.active_ms()),
            supersedes_turn_id,
            answered: false,
            at: now_millis(),
        }]
    }

    /// Attach a fresh compute lease. This is the swappable-lease seam: a later
    /// turn-boundary rotation re-attaches here and the spine re-primes it before the
    /// next turn. Marks the lease un-primed so continuity re-injection happens.
    /// Attach the machinery journal (§2.1). Retention is unconditional once attached —
    /// there is no flag gating it, deliberately (§3.2).
    pub fn set_machinery_journal(&mut self, journal: MachineryJournal) {
        self.machinery_journal = Some(journal);
        self.publish_read_metadata();
    }

    pub fn has_machinery_journal(&self) -> bool {
        self.machinery_journal.is_some()
    }

    /// Read-only access for a UI/command layer that wants to project a thread's machinery
    /// (`journal.project_thread`). Read-only because nothing outside the spine may write
    /// to this store.
    pub fn machinery_journal(&self) -> Option<&MachineryJournal> {
        self.machinery_journal.as_ref()
    }

    /// Attach the live `rich://machinery` sink. Optional: the spine runs headless with
    /// nothing listening, and per §2.2 a UI that isn't listening never stalls a turn.
    pub fn set_machinery_observer(&mut self, observer: Box<dyn MachineryObserver>) {
        self.machinery_observer = Some(observer);
    }

    /// Attach the output record (Output side panel PRD §3) and the evidence folder,
    /// `<app-data>/engine-state/evidence`, its hook witnesses write to.
    ///
    /// With it, witness (a) records each of Rich's own completed write-tool calls at the live
    /// merge, and at the end of every turn this lease's `writes.jsonl` is projected (witnesses
    /// (b) and (c): a front-desk worker's writes and the files Rich's commands made).
    pub fn set_output_store(&mut self, store: crate::output::OutputStore, evidence_root: Option<PathBuf>) {
        self.output = Some((store, evidence_root));
    }

    /// Witnesses (b) and (c) for the lease that just ran a turn of `thread_id`: its session's
    /// `writes.jsonl`, joined through the ledger's `TurnStarted` rows (one lease serves one
    /// thread). A failure is logged and never fails the turn (§4.7).
    fn project_output_evidence(&self, thread_id: &str) {
        let Some((store, Some(root))) = self.output.as_ref() else { return };
        let Some(session) = self.lease_session_id() else { return };
        let join = crate::output::SessionJoin::new(self.ledger.turns(), &[]);
        if let Err(e) = store.project_session(thread_id, root, session, &join) {
            eprintln!("[richos] output: this turn's hook witnesses could not be recorded: {e}");
        }
    }

    /// Attach the correction-staging desk. **This is what makes the flywheel's trigger
    /// automatic**: with a desk attached, every CEO utterance is examined as it is
    /// submitted, and a correction is written down without a command being typed. With no
    /// desk attached this file behaves exactly as it did before the desk existed.
    /// Attach the loro correction desk the belief trigger files into. The SAME `Arc` the
    /// Tauri commands hold, so a proposal filed inside a turn is answerable during it.
    pub fn set_correction_desk(&mut self, desk: SharedCorrectionDesk) {
        self.correction_desk = Some(desk);
    }

    /// Attach the provenance of what memory Rich was actually given. The SAME `Arc` the
    /// context compiler writes to (`CliContextCompiler::set_provenance_sink`) — without it
    /// the belief trigger can resolve nothing and stays silent, which is the honest
    /// degradation rather than a guessed reference.
    pub fn set_loro_provenance(&mut self, prov: SharedSliceProvenance) {
        self.loro_provenance = Some(prov);
    }

    pub fn set_proposal_observer(&mut self, observer: Box<dyn ProposalObserver>) {
        self.proposal_observer = Some(observer);
    }

    pub fn set_candidate_desk(&mut self, desk: SharedCandidateDesk) {
        self.candidates = Some(desk);
    }

    /// Attach the dictation journal — the "heard" side of the DIFF trigger.
    ///
    /// **Optional, and OFF unless something calls this.** Two independent reasons, and the
    /// second is the one that matters:
    ///
    /// 1. Most installs have no journalling dictation app at all, so there is nothing to
    ///    read. `heard.rs`'s trigger cannot fire without a journal, which is an honest
    ///    degradation rather than a failure.
    /// 2. **It is the only one of the three triggers that did not measure precision
    ///    1.000.** It measured 0.972 over 156 invented pairs, and the single false positive
    ///    is a pair that would corrupt an ordinary English word in every future decode
    ///    (`heard.rs`, "the one false positive"). It also fires on an edit the CEO never
    ///    volunteered, so a wrong question here is more expensive than in either trigger
    ///    before it. Until that defect is repaired in the SHARED expansion rule where it
    ///    lives, this stays behind a deliberate switch rather than on by default, and the
    ///    shell honours `RICHOS_HEARD_TRIGGER` rather than deciding for him.
    pub fn set_heard_source(&mut self, source: Box<dyn crate::heard::HeardSource>) {
        self.heard = Some(source);
    }

    /// Attach the sink that renders §7's ask. Optional and independent of the desk: the
    /// question is durable on disk BEFORE this is called, so a surface that is not
    /// listening loses the prompt, never the record.
    pub fn set_correction_observer(&mut self, observer: Box<dyn CorrectionObserver>) {
        self.correction_observer = Some(observer);
    }

    /// The shared handle, for the surface that answers §7's three outcomes. It is a handle
    /// rather than a borrow precisely so the answer path does not go through this struct.
    pub fn candidate_desk(&self) -> Option<&SharedCandidateDesk> {
        self.candidates.as_ref()
    }

    pub fn attach_lease(&mut self, lease: Box<dyn Cognition>) {
        self.install_lease(lease);
    }

    /// THE ONLY place `self.lease` is assigned — boot, clean rotation and crash recovery
    /// all come through here.
    ///
    /// It exists because two things outside the spine lock must never be able to describe a
    /// lease that is gone, and both used to. `set_cancel` was documented as being called
    /// "when a lease is attached, rotated or recovered" and was in fact called only on
    /// attach, so after a rotation the shell's stop button held the DEAD child's cancel
    /// handle. And `set_lease_session` (new) is what the worker path derives its team
    /// directory from, so a stale value there is a false-attribution channel of exactly the
    /// kind this commit removes. Centralizing the assignment is what makes both true by
    /// construction rather than by remembering.
    fn install_lease(&mut self, lease: Box<dyn Cognition>) {
        // Published BEFORE the lease can be handed a turn. A lease with no cancel story
        // publishes `None`, and a stop against it reports `reached_lease: false` rather
        // than claiming an interrupt that never happened.
        self.control.set_cancel(lease.cancel_handle());
        self.control.set_lease_session(Some(lease.session_id().to_string()));
        self.lease = Some(lease);
        self.lease_primed = false;
        self.lease_primed_thread = None;
        // A FRESH LEASE HAS NOT MEASURED ITSELF YET, and must not inherit a number about
        // a session that no longer exists. Clearing here — the one place `self.lease` is
        // assigned — is what makes that true by construction on all three paths (boot,
        // clean rotation, crash recovery) instead of by remembering it at each. Leaving it
        // set would put a successor over the watermark on its first turn and rotate it
        // immediately: rotation as a loop, which is worse than no rotation at all.
        self.context_usage = None;
        self.context_pressure = None;
    }

    /// The current lease's session id — the ONLY honest answer to "whose workers is this
    /// app looking at" (`worker_status.rs`). `None` when no lease is attached, and callers
    /// must propagate that rather than substituting anything for it.
    pub fn lease_session_id(&self) -> Option<&str> {
        self.lease.as_ref().map(|l| l.session_id())
    }

    /// **Take the chair's front desk and park it under its own thread** (see [`Resident`]).
    ///
    /// The counterpart of `clear_lease`, and the difference is the whole of what "resident"
    /// means: `clear_lease` drops the `Box<dyn Cognition>`, whose `Drop` kills the child;
    /// this MOVES it somewhere it stays alive. Everything the chair knew about that lease —
    /// whether it was primed, and every context number it has measured — goes with it, so
    /// coming back is a return rather than a reconstruction.
    ///
    /// **It parks nothing it cannot name.** A lease with no `lease_primed_thread` has never
    /// been primed for a thread, so there is no key to file it under and it is dropped
    /// exactly as before.
    fn park_current_front_desk(&mut self) {
        let Some(thread) = self.lease_primed_thread.clone() else {
            self.clear_lease();
            return;
        };
        let Some(lease) = self.lease.take() else {
            self.clear_lease();
            return;
        };
        self.resident.insert(thread, Resident {
            lease,
            primed: self.lease_primed,
            onboarding_block: self.onboarding_primed_block.take(),
            context_chars: self.context_chars,
            context_usage: self.context_usage.take(),
            context_pressure: self.context_pressure.take(),
            parked_at_ms: now_millis(),
            pending_rotation_reason: self.pending_rotation_reason.take(),
            rotation_deferred_since: self.rotation_deferred_since.take(),
        });
        // The chair is now empty, and the control must say so before anything else runs:
        // a stop pressed in this instant has to reach nothing rather than reach the desk
        // that just stood up. `install_lease` or `resume_front_desk` publishes the next one.
        self.control.set_cancel(None);
        self.control.set_lease_session(None);
        self.lease_primed = false;
        self.lease_primed_thread = None;
        self.context_chars = 0;
        self.evict_beyond_the_cap();
    }

    /// Put a parked front desk back in the chair, if this thread has one.
    ///
    /// Returns whether it found one. `false` means the thread has never spoken, or its desk
    /// was retired at the cap, and `prepare_request` then takes the path every thread took
    /// before residency: spawn a lease and prime it from the ledger.
    fn resume_front_desk(&mut self, thread_id: &str) -> bool {
        let Some(resident) = self.resident.remove(thread_id) else { return false };
        // Published BEFORE the lease can be handed a turn, for the reason `install_lease`
        // states: two things outside the spine lock must never describe a lease that is gone.
        self.control.set_cancel(resident.lease.cancel_handle());
        self.control.set_lease_session(Some(resident.lease.session_id().to_string()));
        self.lease = Some(resident.lease);
        // **Restored, not reset.** This is the one place a lease is installed WITHOUT
        // clearing its measurements, and that is the point: they are its own, and a desk
        // that came back with an empty watermark would be a session claiming it had used
        // nothing when it has been talking to him for an hour.
        self.lease_primed = resident.primed;
        // **RESTORED WITH THE DESK, for the reason the field's own doc gives.** A desk that came
        // back to a chair still describing the previous desk's company material is un-primed by
        // `prime_lease_if_needed`'s comparison and re-primed on his clock, for nothing.
        self.onboarding_primed_block = resident.onboarding_block;
        self.lease_primed_thread = Some(thread_id.to_string());
        self.context_chars = resident.context_chars;
        self.context_usage = resident.context_usage;
        self.context_pressure = resident.context_pressure;
        // Its own renewal, if it was owed one when it was parked (finding 42). The chair was
        // just emptied by `park_current_front_desk`, so nothing here overwrites a reason that
        // belongs to another desk.
        self.pending_rotation_reason = resident.pending_rotation_reason;
        self.rotation_deferred_since = resident.rotation_deferred_since;
        true
    }

    /// **Un-prime EVERY front desk, the parked ones included.**
    ///
    /// A defect residency would otherwise have introduced, and it is the reason this is a
    /// named method rather than one more `self.lease_primed = false`. Some of the things
    /// that invalidate priming are facts about the CHAIR's lease; several are facts about
    /// the whole app — the central folder moved, the memory wiring was torn down, the
    /// company material changed. Before residency, un-priming the chair was enough, because
    /// every other thread's next turn built a lease from scratch anyway. Now they do not:
    /// a parked desk comes back with `primed: true` and would serve the CEO from material
    /// the app has stopped believing in, silently, until it rotated on its own watermark.
    fn unprime_every_front_desk(&mut self) {
        // **AND THE ONE THAT IS NOT HERE YET.** A spare being primed right now holds no place
        // in `self.spare` — its steps run with this mutex down — so there is nothing here to
        // set `primed: false` on. The generation is how it is caught: step 5 compares what it
        // read at step 1 against this, and installs the finished child UN-PRIMED when they
        // differ. Without it, the one desk most likely to be holding discarded material is the
        // one desk this method cannot reach.
        self.front_desk_generation = self.front_desk_generation.wrapping_add(1);
        self.lease_primed = false;
        for resident in self.resident.values_mut() {
            resident.primed = false;
        }
        // **AND THE SPARE**, for the reason the doc above gives about parked desks and one
        // more: a spare is primed EARLIER than any of them — before its thread exists — so it
        // is the desk most likely to be holding material the app has since stopped believing
        // in. It is not discarded; an un-primed spare is adopted and primes at his first send,
        // which is the behavior every thread had before spares existed.
        if let Some(spare) = self.spare.as_mut() {
            spare.primed = false;
        }
    }

    /// Retire the least recently spoken PARKED desk while there are more than the cap.
    ///
    /// Never the one in the chair — it is not in this map — and never one mid-turn, which
    /// cannot happen: a turn runs on the chair's lease, and parking only ever happens at a
    /// turn boundary (`prepare_request`, reached with `turn_in_progress == false`).
    fn evict_beyond_the_cap(&mut self) {
        while self.resident.len() >= MAX_RESIDENT_FRONT_DESKS {
            let Some(oldest) = self.resident.iter()
                .min_by_key(|(_, resident)| resident.parked_at_ms)
                .map(|(thread, _)| thread.clone()) else { break };
            // Dropping the `Resident` drops its `Box<dyn Cognition>`, whose `Drop` kills the
            // child. That is deliberate and it is the old behavior for this one thread.
            self.resident.remove(&oldest);
        }
    }

    /// **PRIME A DESK FOR THE THREAD HE HAS NOT STARTED YET** — the CEO's §55, and the only
    /// shape that recovers the seconds `first-words-2026-09-18-sendlock.md` §2.2 measured.
    ///
    /// See [`SpareFrontDesk`] for why a spare has to exist, why its thread id is reserved
    /// rather than created, and what it costs. This is the operation: reserve an id, spawn a
    /// child SCOPED to it, and spend its priming turn now, so that
    /// [`Spine::create_thread`] can hand the CEO's first brand-new thread a desk that is
    /// already primed and [`Spine::prime_front_desk`] finds nothing left to do.
    ///
    /// **Idempotent per entity.** A primed spare for `entity` already standing there is
    /// [`SpareReady::AlreadyReady`] and costs nothing, so the shell may call this on every
    /// view change without a second thought. A spare for a DIFFERENT entity is retired here
    /// and replaced — see [`Spine::discard_spare`] for why that is the decision and not a
    /// per-entity pool.
    ///
    /// **It never creates a thread and never touches the chair.** The spine's lease,
    /// `lease_primed`, the resident map and the active context are all untouched: a spare is a
    /// child standing beside the app, not a front desk in the chair, until `create_thread`
    /// adopts it.
    ///
    /// # THIS ROAD HOLDS THE MUTEX FOR THE WHOLE OF IT, AND THE APP NO LONGER TAKES IT
    ///
    /// The caller holds the spine's mutex across the spawn AND the priming turn — seconds,
    /// both of them — which is what a test wants and what the CEO cannot afford. The shell
    /// takes [`ready_a_spare_front_desk_without_the_spine`] instead. The two are the SAME five
    /// steps in the same order; only which of them run under the lock differs, so there is one
    /// behavior and no second implementation to drift.
    pub fn ready_a_spare_front_desk(&mut self, entity: &EntityId) -> SpareReady {
        let plan = match self.begin_a_spare(entity) {
            Ok(plan) => plan,
            Err(verdict) => return verdict,
        };
        let mut lease = match self.spawn_a_spare_here(&plan) {
            Ok(lease) => lease,
            Err(e) => return self.abandon_a_spare(plan, None, e.to_string()),
        };
        let priming = match self.priming_for_a_spare(&plan, lease.as_ref()) {
            Ok(priming) => priming,
            Err(e) => return self.abandon_a_spare(plan, None, e.to_string()),
        };
        let mut machinery = Vec::new();
        if let Err(e) = run_a_spare_priming_turn(lease.as_mut(), &plan, &priming, &mut machinery) {
            return self.abandon_a_spare(plan, Some(priming), e.to_string());
        }
        self.install_the_spare(plan, lease, priming, machinery)
    }

    // -----------------------------------------------------------------------------------
    // THE FIVE STEPS, SPLIT SO THE TWO SLOW ONES CAN RUN WITH THE MUTEX DOWN
    // -----------------------------------------------------------------------------------
    //
    // Steps 1, 3 and 5 are ledger and configuration work and take microseconds. Steps 2 and 4
    // — the child's spawn and its priming turn — are SECONDS, and they touch nothing but the
    // detached lease. Until 2026-09-19 all five ran under one lock, and that is where the
    // CEO's whole wait was: `create_thread_in` starts this work microseconds before the window
    // issues eight more bridge calls, and every one of them queued behind the priming turn.
    // Measured on the real window that day — `a window command waited 18314 ms for the spine`
    // on a debug build, the turn's own header starting its clock 18.6 s after the keystroke;
    // on the shipped candidate the same shape is Ray's 5378 ms spare and ~6 s of pre-turn
    // overhead.

    /// **Step 1 (locked): decide, and reserve the slot.** Everything that reads or writes the
    /// spine happens here, so that steps 2 and 4 can run against nothing but the plan.
    fn begin_a_spare(&mut self, entity: &EntityId) -> Result<SparePlan, SpareReady> {
        if self.turn_in_progress {
            return Err(SpareReady::TurnInProgress);
        }
        if !self.registry.contains(entity) {
            return Err(SpareReady::NotReady(SpineError::UnknownEntity(entity.to_string()).to_string()));
        }
        if self.spare.as_ref().is_some_and(|spare| &spare.entity == entity && spare.primed) {
            return Err(SpareReady::AlreadyReady);
        }
        // **ONE ATTEMPT AT A TIME, and this is the flag that makes the unlocked road safe.**
        // With the mutex down between the steps, a second ask can arrive and find no spare
        // standing — because the first one has not installed it yet. Two spawns would then
        // race for one slot and the loser's child would be killed after it had been paid for.
        if self.spare_in_flight {
            return Err(SpareReady::AlreadyReady);
        }
        // Anything else standing here is for the wrong entity, or was un-primed by a change
        // the whole app made (`unprime_every_front_desk`). Either way it is retired BEFORE the
        // replacement is spawned, so two spare children never exist at once.
        self.discard_spare();
        if self.control.stop_claim().is_some() {
            return Err(SpareReady::NotReady("a stop is claimed".into()));
        }
        if self.lease_factory.is_none() {
            return Err(SpareReady::NotReady(SpineError::NoLeaseFactory.to_string()));
        }
        // **THE ID IS MINTED HERE AND WRITTEN NOWHERE** (UX §3.3). The binding built around it
        // is for SPAWN SCOPING only — `EngineProfile::scope_to` and nothing else — and it is
        // deliberately unable to pass `Ledger::verify_binding`: the thread does not exist, so
        // every scoped read or write through it fails closed, which is the property that makes
        // it safe to hold a binding for a conversation that has not started.
        let reserved = crate::ledger::Ledger::reserve_thread_id();
        let binding = crate::entity::ThreadBinding::new(
            crate::entity::PersonId::default_ceo(),
            entity.clone(),
            &reserved,
            self.ledger.take_revision(),
        );
        self.spare_in_flight = true;
        Ok(SparePlan {
            entity: entity.clone(),
            binding,
            reserved,
            // `None` is not a failure: it means this factory cannot hand out a second handle,
            // so step 2 runs under the caller's lock the way it always did.
            factory: self.lease_factory.as_ref().and_then(|f| f.duplicate()),
            control: self.control.clone(),
            generation: self.front_desk_generation,
            started: std::time::Instant::now(),
        })
    }

    /// **Step 2, the locked spelling.** Used by [`Spine::ready_a_spare_front_desk`], and by the
    /// unlocked road when the factory cannot be duplicated.
    fn spawn_a_spare_here(&self, plan: &SparePlan) -> Result<Box<dyn Cognition>, crate::cognition::CognitionError> {
        let factory = self.lease_factory.as_ref().ok_or(crate::cognition::CognitionError::Protocol(
            SpineError::NoLeaseFactory.to_string(),
        ))?;
        factory.spawn_scoped(&plan.binding, &plan.control)
    }

    /// **Step 3 (locked): what the detached child will be told.**
    ///
    /// [`RePrimePayload::assemble_for_a_reserved_thread`] is infallible because a reserved id
    /// provably has no turns — see its own documentation.
    ///
    /// **No `record_prompt_received`.** That call verifies the binding and would fail closed,
    /// correctly: there is no thread to file an Internal turn against. The durable record that
    /// this happened is the ACTION claimed here, which is thread-agnostic.
    fn priming_for_a_spare(
        &mut self,
        plan: &SparePlan,
        lease: &dyn Cognition,
    ) -> Result<SparePriming, SpineError> {
        let binding = &plan.binding;
        let onboarding_block = self.company_block(binding);
        let session = lease.session_id().to_string();
        let workers = lease.worker_status();
        let mut payload = RePrimePayload::assemble_for_a_reserved_thread(
            &self.ledger, binding, DEFAULT_TAIL_TURNS, Some(session.as_str()),
        );
        if let Some(workers) = workers {
            payload.worker_state_unknown = workers.unattributed;
            payload.worker_state = workers.priming_lines();
        }
        self.fill_loro_tier(&mut payload, binding);
        let mut priming = payload.to_priming_prompt();
        if let Some(block) = &onboarding_block { priming.push_str(block); }
        // Claim-then-execute, Internal visibility, exactly like the chair's `session_reprime`.
        // **"The spare WAS primed" is the fact the whole saving rests on**, so it is durable
        // even though the thread it names does not exist yet — and naming the reserved id is
        // what lets a later reader join this line to the thread it became.
        let action = self.ledger.record_action_with(
            None,
            "spare_front_desk_reprime",
            &format!(
                "entity={}; reserved_thread={}; session={session}; priming_chars={}",
                binding.entity_id(), binding.thread_id(), priming.len()
            ),
            ActionVisibility::Internal,
            ActionStatus::Claimed,
        )?;
        Ok(SparePriming { priming, onboarding_block, scope: self.onboarding_tool_scope(binding), action })
    }

    /// **Step 5 (locked): stand the primed desk up, or decide the world moved under it.**
    fn install_the_spare(
        &mut self,
        plan: SparePlan,
        lease: Box<dyn Cognition>,
        priming: SparePriming,
        machinery: Vec<crate::machinery::MachineryRecord>,
    ) -> SpareReady {
        self.spare_in_flight = false;
        // THE MACHINERY OF A TURN NOBODY WATCHED, emitted here rather than as it arrived,
        // because step 4 has no `self` to emit through. `internal: true` and `turn_id: None`,
        // like every priming turn's machinery; the thread id is the reserved one, which is the
        // conversation these records will belong to the moment the CEO sends into it.
        for record in machinery {
            let record = record.stamp(&plan.reserved, None, true);
            Self::retain_and_emit_machinery(self.machinery_journal.as_ref(), self.machinery_observer.as_deref(), record);
        }
        let _ = self.ledger.update_action(&priming.action, ActionStatus::Completed);
        // **DID THE WORLD MOVE WHILE THE MUTEX WAS DOWN?** The child is correctly scoped and
        // paid for either way, so it is kept — but a desk primed against material the app has
        // since stopped believing in must not be handed to him as primed. `primed: false` is
        // the same honest degrade `unprime_every_front_desk` already applies to a standing
        // spare: it is adopted and primes at his first send, which is what every thread did
        // before spares existed.
        let stale = plan.generation != self.front_desk_generation || !self.registry.contains(&plan.entity);
        self.spare = Some(SpareFrontDesk {
            lease,
            entity: plan.entity,
            reserved_thread: plan.reserved,
            primed: !stale,
            onboarding_block: priming.onboarding_block,
            context_chars: priming.priming.len(),
        });
        if stale {
            return SpareReady::NotReady(
                "the app un-primed every front desk while this one was being primed; it is kept, \
                 un-primed, and his first message primes it the way it did before".into(),
            );
        }
        SpareReady::Ready { millis: plan.started.elapsed().as_millis() as u64 }
    }

    /// The failure end of the same five steps. **A spare that could not be spawned or primed
    /// is not a disaster and is not hidden**: the slot is released, the claimed action is
    /// failed, and his first message primes the way it does today.
    fn abandon_a_spare(&mut self, _plan: SparePlan, priming: Option<SparePriming>, why: String) -> SpareReady {
        self.spare_in_flight = false;
        if let Some(priming) = priming {
            let _ = self.ledger.update_action(&priming.action, ActionStatus::Failed);
        }
        SpareReady::NotReady(why)
    }

    /// Retire the spare: its `Box<dyn Cognition>` is dropped, whose `Drop` kills the child.
    ///
    /// **This is the answer to "what happens to the spare on an entity switch", and it is
    /// discard-and-re-prime rather than one-spare-per-entity.** The cost decision follows the
    /// measurement in `docs/verification/first-words-2026-09-18-primed.md` §2: an idle primed
    /// child is not free, and a pool sized by the number of companies the CEO has registered
    /// would hold that many children open for a surface that can only ever consume one of them
    /// next. The thing thrown away is one priming turn, and it is thrown away only when he has
    /// moved to a different company — at which point the spare was primed with the wrong
    /// company's material and could not have been adopted anyway.
    fn discard_spare(&mut self) {
        self.spare = None;
    }

    /// Is a PRIMED spare standing ready for this entity? **Test scaffolding and the shell's
    /// own log line** — nothing in the product branches on it.
    pub fn spare_front_desk_is_ready_for(&self, entity: &EntityId) -> bool {
        self.spare.as_ref().is_some_and(|spare| &spare.entity == entity && spare.primed)
    }

    /// The thread id the spare is scoped to, reserved and unspent. **Test scaffolding** — it is
    /// how a test proves the id the child was spawned under is the id the thread ends up with.
    #[doc(hidden)]
    pub fn spare_front_desk_reserved_thread(&self) -> Option<&str> {
        self.spare.as_ref().map(|spare| spare.reserved_thread.as_str())
    }

    /// How many front desks this spine is holding open, the chair included. **A
    /// measurement, for the test that proves a thread's desk survived another thread's
    /// turn** — nothing in the product reads it.
    #[doc(hidden)]
    pub fn resident_front_desks(&self) -> usize {
        self.resident.len() + usize::from(self.lease.is_some())
    }

    /// Is this thread's front desk alive — in the chair or parked? **Test scaffolding**,
    /// and the honest form of the question: "resident" is about the child process, not
    /// about who is speaking.
    #[doc(hidden)]
    pub fn front_desk_is_resident(&self, thread_id: &str) -> bool {
        self.resident.contains_key(thread_id)
            || (self.lease.is_some() && self.lease_primed_thread.as_deref() == Some(thread_id))
    }

    /// Install the shared stop/steer control (UX §9.2/§9.3). The shell keeps a clone of
    /// the same handle beside the `Mutex<Spine>`; this is the only channel by which
    /// anything reaches a turn that is already running.
    pub fn set_turn_control(&mut self, control: TurnControl) {
        control.set_cancel(self.lease.as_ref().and_then(|l| l.cancel_handle()));
        control.set_lease_session(self.lease.as_ref().map(|l| l.session_id().to_string()));
        self.control = control;
        self.publish_read_metadata();
    }

    /// A clone of the shared control handle, for the shell's own commands.
    pub fn turn_control(&self) -> TurnControl {
        self.control.clone()
    }

    /// Replace the clock an overload retry waits on. Tests only; the app keeps the real one.
    pub fn set_retry_clock(&mut self, clock: std::sync::Arc<dyn crate::upstream::RetryClock>) {
        self.retry_clock = clock;
    }

    pub fn has_lease(&self) -> bool {
        self.lease.is_some()
    }

    /// Called with the shell's idle lock held and worker observations checked.
    pub fn retire_idle_lease(&mut self) -> Result<(), String> {
        if self.control.active_turn().is_some() { return Err("A turn is still running.".into()); }
        self.clear_lease(); Ok(())
    }

    pub fn clear_memory_wiring(&mut self) {
        self.loro_compiler = None;
        // THE LOOKUP GOES WITH THE COMPILER. It was resolved from the same corpus, and a
        // lookup left attached to a corpus the app has stopped reading would be pointed at
        // an evidence zone nothing else in the build still believes in.
        self.evidence_lookup = None;
        self.correction_desk = None;
        self.unprime_every_front_desk();
    }

    /// Retire a canceled or failed conversation child without another roundtrip.
    fn clear_lease(&mut self) {
        self.lease = None;
        self.control.set_cancel(None);
        self.control.set_lease_session(None);
        self.lease_primed = false;
        self.lease_primed_thread = None;
        self.context_usage = None;
        self.context_pressure = None;
    }

    /// Tell this spine where the central folder is, so the company layer reaches the model.
    ///
    /// It clears `lease_primed`. The priming turn is what carries the company layer; a lease already
    /// primed without it would keep serving the conversation with no company material until
    /// the next thread switch, and nothing would say so. Un-priming re-issues it at the next
    /// turn, which is the cheap and honest answer.
    ///
    /// This never creates the folder and never checks it here. `company.rs` reads it at prime
    /// time and reports what it found; a path stored at boot and validated at boot would be a
    /// path that is right once, and the four dangling `corpus.*` symlinks on this machine are
    /// what that costs (`richos-central-folder-2026-09-06.md` §1.3).
    pub fn set_central_root(&mut self, root: std::path::PathBuf) {
        self.central_root = Some(root);
        self.publish_read_metadata();
        self.unprime_every_front_desk();
    }

    /// What the company layer looks like for one thread, right now.
    ///
    /// `None` means no central folder has been set — which is NOT the same as a central folder
    /// with nothing in it ([`crate::company::CompanyLayer::NoHome`]), and the two must stay
    /// apart: one is "nobody told the app where to look", the other is "the app looked and the
    /// company is not there". Collapsing them would report an unconfigured install as an
    /// un-interviewed one.
    pub fn company_layer(&self, binding: &ThreadBinding) -> Option<crate::company::CompanyLayer> {
        let root = self.central_root.as_ref()?;
        Some(crate::company::CompanyLayer::read(root, binding.entity_id()))
    }

    /// Where the declination record lives — `onboarding.rs`'s single durable fact.
    ///
    /// Separate from [`Spine::set_central_root`] because the two answer different questions and
    /// live in different places: the company layer is the CEO's material in HIS folder, the
    /// declination is the app's own record in the app's own configuration directory. Folding
    /// them into one setter would put a RichOS bookkeeping file inside a folder the CEO owns.
    pub fn set_onboarding_record(&mut self, path: std::path::PathBuf) {
        self.onboarding_record = Some(path);
        self.publish_read_metadata();
        self.unprime_every_front_desk();
    }

    /// The declination record as it stands on disk right now.
    ///
    /// Re-read at every prime rather than cached, so a declination recorded during a
    /// conversation takes effect at the next prime instead of at the next launch.
    fn onboarding_record(&self, binding: &ThreadBinding) -> crate::onboarding::OnboardingRecord {
        match self.onboarding_record.as_ref() {
            Some(p) => crate::onboarding::OnboardingRecord::load(p).for_entity(binding.entity_id()),
            None => crate::onboarding::OnboardingRecord::default(),
        }
    }

    /// The company contribution to a priming turn: its name exactly as he typed it, then the
    /// company's material, the offer of an interview, or the honest note that says why there
    /// is neither (`onboarding.rs`).
    ///
    /// **The name, because the id is not the name** (the CEO, 2026-10-13: he typed a name with
    /// a word in capitals and Rich wrote it back re-capitalized). Every other line names the
    /// company by its id, which is lower case, so Rich rebuilt a name from it.
    fn company_block(&self, binding: &ThreadBinding) -> Option<String> {
        let layer = self.company_layer(binding);
        let onboarding =
            crate::onboarding::priming_block(binding.entity_id(), layer.as_ref(), &self.onboarding_record(binding));
        match self.registry.get(binding.entity_id()) {
            Some(entity) => Some(
                crate::onboarding::company_name_block(binding.entity_id(), &entity.display_name)
                    + &onboarding.unwrap_or_default(),
            ),
            None => onboarding,
        }
    }

    /// One line per company for the boot log — what this launch will actually do about
    /// onboarding, said out loud because the failure being fixed was silence.
    pub fn describe_onboarding(&self, binding: &ThreadBinding) -> String {
        let layer = self.company_layer(binding);
        crate::onboarding::describe(binding.entity_id(), layer.as_ref(), &self.onboarding_record(binding))
    }

    /// WHERE THIS INSTALL STANDS ON ONBOARDING, for the WINDOW rather than for the log.
    ///
    /// [`Spine::describe_onboarding`] answers the same question and answers it as a sentence
    /// for a terminal. This returns the state itself, because the surface that shows the CEO
    /// the offer has to branch on it, and a surface that parsed the log line would be a second
    /// opinion about a fact that already has one.
    ///
    /// It is DERIVED at every call, from the same two facts on disk the priming block is
    /// derived from ([`crate::onboarding::state`]), so the notice on screen and the block in
    /// the priming turn cannot disagree. Nothing is cached, and there is deliberately no
    /// "the notice was shown" anywhere in this crate — `onboarding.rs`'s module doc gives the
    /// reason and [`OnboardingRecord`] has a test whose only job is to keep it that way.
    ///
    /// [`OnboardingRecord`]: crate::onboarding::OnboardingRecord
    pub fn onboarding_state(&self, binding: &ThreadBinding) -> crate::onboarding::OnboardingState {
        let layer = self.company_layer(binding);
        crate::onboarding::state(layer.as_ref(), &self.onboarding_record(binding))
    }

    /// HE WAS ASKED AND SAID NOT NOW — the write half of the one fact the company file
    /// cannot hold.
    ///
    /// `record_declination` existed in `onboarding.rs` from the day that module landed and
    /// had NO CALLER anywhere in the product, which made [`OnboardingState::Declined`] and
    /// `DECLINED_BLOCK` unreachable outside their own tests: a CEO who said "not now" in
    /// conversation was re-offered the interview at the next prime, forever, which is exactly
    /// the nag the onboarding document's M5 named and the memory question had already paid
    /// for once. This is that caller.
    ///
    /// **`Err` when nobody has said where the record lives.** Not a silent success and not a
    /// default path: a declination that goes nowhere is worse than one that was refused, since
    /// the refusal can be reported to him and the silent drop cannot. The same "no silent
    /// default" the corpus provisioner enforces at the only door that can create a corpus.
    ///
    /// [`OnboardingState::Declined`]: crate::onboarding::OnboardingState::Declined
    pub fn record_onboarding_declination(&mut self, now_millis: u64) -> Result<(), crate::doctrine::DoctrineError> {
        let path = self.onboarding_record.as_ref().ok_or_else(|| crate::doctrine::DoctrineError::Unwritable {
            path: "<no onboarding record configured>".into(),
            why: "nobody told this spine where the declination record lives, so there is nowhere to write".into(),
        })?;
        let binding = self.active_binding().ok_or_else(|| crate::doctrine::DoctrineError::Unwritable {
            path: path.display().to_string(), why: "no company is selected; no answer was recorded".into(),
        })?;
        crate::onboarding::OnboardingRecord::record_declination_for_entity(path, binding.entity_id(), now_millis)?;
        self.unprime_every_front_desk();
        Ok(())
    }

    /// Run after restoring the active company (or its first explicit selection). Legacy
    /// records cannot identify a company, so this is a one-time, explicit compatibility step.
    pub fn migrate_legacy_onboarding_declination(&mut self) -> Result<bool, crate::doctrine::DoctrineError> {
        let (Some(path), Some(binding)) = (self.onboarding_record.as_ref(), self.active_binding()) else { return Ok(false); };
        let changed = crate::onboarding::OnboardingRecord::migrate_legacy(path, binding.entity_id())?;
        if changed { self.unprime_every_front_desk(); }
        Ok(changed)
    }

    /// Only the app's bound company and configured paths become tool authority.
    pub fn onboarding_tool_scope(&self, binding: &ThreadBinding) -> Option<crate::onboarding_tools::OnboardingToolScope> {
        Some(crate::onboarding_tools::OnboardingToolScope::new(
            binding.entity_id(), self.central_root.as_ref()?, self.onboarding_record.as_ref()?,
        ))
    }

    /// Attach the rotation/recovery seam. Without one, the spine can still run its
    /// (single) attached lease indefinitely, but a context watermark, an explicit
    /// rotation request, or a mid-turn crash all degrade to their pre-P1.4 behavior
    /// (watermark: never fires since rotation can't proceed; crash: surfaces the error
    /// honestly instead of silently retrying against nothing).
    pub fn set_lease_factory(&mut self, factory: Box<dyn LeaseFactory>) {
        self.lease_factory = Some(factory);
    }

    pub fn has_lease_factory(&self) -> bool {
        self.lease_factory.is_some()
    }

    /// Attach the optional Tier-C seam (continuity §2.3/§4). See `LoroContextCompiler`'s
    /// doc for the degrade-gracefully contract when this is never called, and
    /// `crate::loro::CliContextCompiler` for the shipped implementation.
    pub fn set_loro_context_compiler(&mut self, compiler: Box<dyn LoroContextCompiler>) {
        self.loro_compiler = Some(compiler);
    }

    pub fn has_loro_context_compiler(&self) -> bool {
        self.loro_compiler.is_some()
    }

    /// Attach the optional Tier-E seam — the evidence lookup over the CEO's own files
    /// (the 2026-09-17 evidence-retrieval ruling §1/§4). See `EvidenceLookup`'s doc for the
    /// degrade-gracefully contract when this is never called, and
    /// `crate::evidence::CliEvidenceLookup` for the shipped implementation.
    pub fn set_evidence_lookup(&mut self, lookup: Box<dyn EvidenceLookup>) {
        self.evidence_lookup = Some(lookup);
    }

    pub fn has_evidence_lookup(&self) -> bool {
        self.evidence_lookup.is_some()
    }

    /// Record ONE completed, CEO-facing action that the app took outside a turn.
    ///
    /// The shell needs this because `ledger()` is deliberately `&Ledger`: the ledger is
    /// append-only evidence and handing out `&mut` would be one more place a caller could
    /// write an event the spine knows nothing about. The concrete need is a confirmed loro
    /// correction — "Rich changed what the company believes" is exactly the class of fact
    /// the action-ledger digest exists to stop a successor denying from absent memory
    /// (continuity §2.1 #6, `reprime.rs`'s identity assertion: *"an entry PRESENT is proof
    /// the action happened"*).
    ///
    /// `turn_id: None` is first-class here, not a gap: a correction the CEO confirms in a
    /// side panel belongs to the THREAD, not to any turn, the same way re-prime machinery
    /// does (§1.4 G4).
    pub fn record_ceo_action(&mut self, kind: &str, detail: &str) -> Result<String, SpineError> {
        Ok(self.ledger.record_action_with(
            None,
            kind,
            detail,
            ActionVisibility::CeoFacing,
            ActionStatus::Completed,
        )?)
    }

    /// Fill Tier C of a payload whose Tiers A and B are already assembled.
    ///
    /// **The ordering is forced, not stylistic.** A slice is always topical
    /// (`CONTEXT-CONTRACT.md` §1) and the topic is the CEO's current intent, which lives in
    /// Tier A. So Tier C cannot be assembled inside `RePrimePayload::assemble` alongside
    /// the rest — it is compiled FROM the rest, here, once, on the one path that then
    /// renders the prompt.
    ///
    /// **No compiler attached leaves `LoroTier::NotWired` and returns.** That is the
    /// degrade-gracefully contract, and it is what every install with no corpus keeps.
    ///
    /// **It cannot fail the rotation.** `compile_slice` has no error arm by construction,
    /// so there is no `?` to add here, and a memory miss cannot take down a session
    /// rotation the CEO is not supposed to be able to see.
    fn fill_loro_tier(&self, payload: &mut RePrimePayload, binding: &ThreadBinding) {
        // **WHAT THE PREVIOUS SESSION WAS GIVEN IS NOT WHAT THIS ONE IS GIVEN** (part 1 hunt
        // finding 43). This runs once per priming of a fresh session, and the provenance sink
        // describes the CURRENT session's prompt (`SliceProvenance::record`'s own doc). The
        // compiler replaces the entry only when it ACCEPTS a slice, so a thin answer, a refusal,
        // a compiler failure or a thread with no topic would all leave the previous session's
        // records in place, resolvable by `stage_belief_correction` into a proposal citing memory
        // this session never saw. Forgotten first; an accepted slice records its own entry again
        // below. Only when a compiler is attached: it is the only writer of this sink, so a spine
        // without one has no previous session's slice to drop.
        let Some(compiler) = self.loro_compiler.as_ref() else { return };
        if let Some(sink) = self.loro_provenance.as_ref() {
            if let Ok(mut held) = sink.lock() {
                held.forget(binding.thread_id());
            }
        }
        // A COMPILER AND NO LOOKUP IS THE ORDINARY STATE, and it leaves `EvidenceTier::NotWired`
        // untouched below — never a claim that the CEO has no such files.
        // Acceptance is journaled before priming. Use the newest pending request only as
        // the read-only retrieval query; do not put it back in the model's context transcript
        // or current_intent, where it would be consumed before visible delivery.
        let pending_topic = self.ledger.thread_turns_scoped(binding).ok().and_then(|turns| {
            turns.into_iter().rev().find(|turn| {
                turn.source != Source::Internal && turn.superseded_by.is_none()
                    && matches!(turn.state, crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight)
                    && !turn.user_text.trim().is_empty()
            }).map(|turn| turn.user_text.clone())
        });
        let Some(topic) = pending_topic.or_else(|| payload.topic().map(str::to_string)) else {
            payload.loro = LoroTier::Unavailable(
                "no topic — this thread holds nothing the CEO has said, and there is no such thing                  as a topic-less slice"
                    .into(),
            );
            return;
        };
        let req = SliceRequest {
            thread_id: binding.thread_id(),
            entity_id: binding.entity_id().as_str(),
            topic: &topic,
            budget_chars: DEFAULT_LORO_BUDGET_CHARS,
        };
        // ONE COMPILE, TWO OUTPUTS: the injectable tier, and the compiler's own verdict on
        // whether it answered. The verdict is not injected — it is what decides whether to
        // look in the CEO's files, and it belongs to THIS compile and no other.
        let compiled = compiler.compile_tier(&req);
        payload.loro = compiled.tier;

        // TIER E — the evidence lookup. Gated on the compiler's own coverage label, and on
        // nothing else: not on the tier being thin, not on `items` being empty, not on how the
        // text reads. `coverage` is the signal the compiler emits for exactly this purpose
        // (the 2026-09-17 ruling §1, "When it runs: on demand, not on every re-prime").
        //
        // **The memory budget is not touched here and cannot be.** `req.budget_chars` was
        // spent above and is not re-read; the block goes into a different field, rendered
        // under a different heading, below the company-memory block. Asserted by a test that
        // measures the rendered memory section with and without a block present.
        let Some(lookup) = self.evidence_lookup.as_ref() else { return };
        let ev = EvidenceRequest {
            thread_id: binding.thread_id(),
            entity_id: binding.entity_id().as_str(),
            topic: &topic,
            coverage: compiled.coverage,
        };
        payload.evidence = if compiled.coverage.should_consult_evidence() {
            lookup.look_up(&ev)
        } else {
            // Not even asked. Spending a process to be told "covered" would be one child
            // process per turn for an answer this side already has.
            EvidenceTier::NotConsulted(match compiled.coverage.as_str() {
                Some(label) => format!("the compiled slice reported coverage {label:?}"),
                None => "the compiler did not report a coverage label".into(),
            })
        };
    }

    /// Override the context-window watermark budget (continuity §8 Q2). `window_tokens`
    /// is the model's approximate context window; `watermark_ratio` (0.0–1.0) is the
    /// fraction of it that triggers a scheduled rotation at the next turn boundary.
    pub fn set_context_budget(&mut self, window_tokens: usize, watermark_ratio: f64) {
        self.context_window_tokens = window_tokens.max(1);
        self.watermark_ratio = watermark_ratio.clamp(0.0, 1.0);
    }

    /// The current lease's ESTIMATED consumed-context in tokens (chars ÷ 4 — see
    /// `CHARS_PER_TOKEN_ESTIMATE`). An estimate, never presented as an exact measurement,
    /// and no longer the watermark's input once the lease has reported.
    pub fn context_estimate_tokens(&self) -> usize {
        self.context_chars / CHARS_PER_TOKEN_ESTIMATE
    }

    /// The MEASURED `{used, size}` the current lease last reported, or `None` if it has
    /// not reported yet. Read from the machinery STREAM as records go past — never from
    /// the journal, whose Tier-B payloads are evictable (`machinery.rs`).
    pub fn context_usage(&self) -> Option<ContextUsage> {
        self.context_usage
    }

    /// Which of the two the watermark is running on RIGHT NOW. The whole point of
    /// exposing this is that a caller must never be able to print a context number
    /// without also being able to say where it came from.
    pub fn context_source(&self) -> ContextSource {
        match self.context_usage {
            Some(_) => ContextSource::Measured,
            None => ContextSource::Estimated,
        }
    }

    /// Consumed context in tokens: the adapter's `used` when it has reported, otherwise
    /// the chars÷4 estimate. Pair it with `context_source()` before showing it to anyone.
    pub fn context_used_tokens(&self) -> usize {
        match self.context_usage {
            Some(u) => u.used as usize,
            None => self.context_estimate_tokens(),
        }
    }

    /// The context window in tokens: the adapter's `size` when it has reported, otherwise
    /// the configured fallback.
    ///
    /// **The wire outranks `set_context_budget`'s window, deliberately.** The adapter knows
    /// which model is behind the session; this process is guessing. Measured 50/50 on
    /// 2026-08-28: `size` = 1_000_000 against a configured default of 200_000. The
    /// configured ratio is NOT overridden — that is policy, and policy stays ours.
    pub fn context_window_tokens(&self) -> usize {
        match self.context_usage {
            Some(u) if u.size > 0 => u.size as usize,
            _ => self.context_window_tokens,
        }
    }

    /// The window this spine would fall back to if the lease never reported. Exposed
    /// separately so `set_context_budget`'s effect stays inspectable even once a
    /// measurement has superseded it.
    pub fn configured_context_window_tokens(&self) -> usize {
        self.context_window_tokens
    }

    /// The fraction of the context window consumed — the exact number `watermark_reached`
    /// compares against the ratio. Measured or estimated per `context_source()`.
    pub fn context_fraction(&self) -> f64 {
        match self.context_usage {
            Some(u) => u.fraction(),
            None => {
                let window = self.context_window_tokens.max(1) as f64;
                self.context_estimate_tokens() as f64 / window
            }
        }
    }

    /// Whether the current lease has crossed the watermark and is due for rotation at
    /// the next turn boundary (continuity §3.2, primary trigger).
    ///
    /// **Measured first, estimated only as a fallback** (Frank's F2, 2026-08-29). When the
    /// lease has reported `usage_update` this is `used / size >= ratio` — the adapter's own
    /// arithmetic. Until then it is the chars÷4 proxy over the configured fallback window,
    /// which is an estimate and is labelled one by `context_source()`.
    ///
    /// The integer-threshold form was kept for the fallback branch so the existing
    /// `set_context_budget(1000, 0.001)` tests keep meaning exactly what they meant.
    pub fn watermark_reached(&self) -> bool {
        match self.context_usage {
            Some(u) if u.size > 0 => u.fraction() >= self.watermark_ratio,
            // No measurement (fresh lease, or an adapter that reported a zero window).
            _ => {
                let threshold = (self.context_window_tokens as f64 * self.watermark_ratio) as usize;
                self.context_estimate_tokens() >= threshold
            }
        }
    }

    /// The measured usage that crossed `CONTEXT_CRITICAL_RATIO` mid-turn and has not yet
    /// been settled at a boundary, with the turn it crossed in. `None` in the normal case.
    pub fn context_pressure(&self) -> Option<(&str, ContextUsage)> {
        self.context_pressure.as_ref().map(|(t, u)| (t.as_str(), *u))
    }

    pub fn rotation_count(&self) -> u64 {
        self.rotation_count
    }

    pub fn last_rotation_reason(&self) -> Option<&str> {
        self.last_rotation_reason.as_deref()
    }

    /// Schedule a fresh lease for the next accepted request. Its preparation then
    /// owns the wait, Stop and any error, before the user prompt reaches a model.
    pub fn request_rotation(&mut self, reason: &str) -> Result<(), SpineError> {
        if self.lease_factory.is_none() { return Err(SpineError::NoLeaseFactory); }
        self.ensure_active_thread()?;
        self.pending_rotation_reason = Some(reason.to_string());
        Ok(())
    }

    /// Raise a proactive message (the attention seam's persistence + UI half — UX §5).
    /// **Judgment of WHEN to fire is explicitly NOT this method's job** — that is a
    /// later leg (an attention-seam trigger watching engine event logs / loro / timers,
    /// per architecture §4.2). This is the SEAM: given a tier + text a caller (a future
    /// trigger, or a test) has already decided on, persist it durably and — for Tier 1/2
    /// only, never Tier 3/Silent — surface it to the UI at once, also while a turn is in
    /// flight (round 16 draws the switch line beside the working row). Returns the new
    /// turn id.
    pub fn raise_proactive(
        &mut self,
        thread_id: Option<&str>,
        tier: AttentionTier,
        text: &str,
    ) -> Result<String, SpineError> {
        // A proactive message is a scoped WRITE like any other: the binding comes from the
        // ledger (never from the caller), so Rich cannot speak unprompted into an unbound
        // thread or into an entity the target thread does not belong to.
        let binding = match thread_id {
            Some(t) => self.ledger.thread_binding(t)?,
            None => self.ensure_active_thread()?,
        };
        let thread_id = binding.thread_id().to_string();
        // Durable regardless of tier or turn state — once Rich has "said" something
        // (even if Silent never renders it), it must survive a crash immediately after.
        let turn_id = self.ledger.record_proactive_message(&binding, tier, text)?;

        // ACTION LEDGER (continuity §5.4 / §6.1): raising a proactive message is the one
        // genuinely CEO-FACING thing this app does on its own initiative today — Rich
        // reached out unprompted. Recording it is what makes the re-prime's
        // "ground truth for what Rich has done" claim TRUE rather than aspirational:
        //   - a successor can never deny having flagged something (false attribution), and
        //   - a successor can never re-raise the same thing (double execution), which for
        //     Tier 3/Silent is otherwise UNKNOWABLE — a Silent message has no render path
        //     at all (`messages()` skips it), so outside the action ledger there is no
        //     surface on which a successor could ever learn it already happened.
        // Recorded `Completed`, not `Claimed`: the durable, fsync'd ProactiveMessage
        // event above IS the execution — there is no second phase that could fail.
        self.ledger.record_action_with(
            Some(&turn_id),
            "proactive_message",
            &format!("[{}] {}", tier.as_str(), text),
            ActionVisibility::CeoFacing,
            ActionStatus::Completed,
        )?;

        // **SAID AT ONCE, EVEN WHILE A TURN RUNS** (round 16, state `switched-line`: Rich's
        // "Switched to Work: …" line sits above "• 3 agents working on Work", the row that
        // exists only while a turn runs). This used to be deferred to the turn's end so it
        // would not "collide with the working row"; round 16 draws the two together, and the
        // switch notice is raised INSIDE the turn that runs on the new account
        // (`prepare_request`, after `ran_on`), so deferring it hid the line for exactly the
        // turn it describes. The message is its own turn, already durable above and already in
        // the published read view, so the window's reload draws it beside the running turn
        // without touching that turn's live state.
        if tier != AttentionTier::Silent {
            self.emit(StreamEvent::ProactiveMessage {
                thread_id,
                turn_id: turn_id.clone(),
                tier,
                at: now_millis(),
            });
            self.emit_proactive_live(&binding, &turn_id, tier, text);
        }
        Ok(turn_id)
    }

    /// **A conversation the app opens on its own, with Rich's first message already in it**
    /// (the CEO, 2026-10-13: once dictation is set up, a conversation titled "Dictation feature"
    /// is created under the company, holding Rich's message about it).
    ///
    /// The message is a proactive turn ([`Self::raise_proactive`]), so it is Rich's, durable,
    /// and in the verbatim tail the desk answering there is primed with. The thread is created
    /// through the ledger directly rather than [`Self::create_thread`], so a spare front desk
    /// standing ready for his next new conversation is not spent on this one, and the
    /// conversation he is in stays the active one.
    ///
    /// Answers `(thread id, message turn id)`; the turn id is what [`Self::replace_app_message`]
    /// replaces.
    pub fn open_conversation_with_message(
        &mut self,
        entity_id: &EntityId,
        title: &str,
        text: &str,
    ) -> Result<(String, String), SpineError> {
        if !self.registry.contains(entity_id) {
            return Err(SpineError::UnknownEntity(entity_id.to_string()));
        }
        let id = self.ledger.create_thread(title, entity_id)?;
        let turn = self.raise_proactive(Some(&id), AttentionTier::Digest, text)?;
        self.publish_read_metadata();
        Ok((id, turn))
    }

    /// **Replace a message the app wrote for Rich** (the CEO, 2026-10-13: the "Dictation feature"
    /// message must name the key he actually picked, also when he picks another one later).
    /// The new text is a new proactive turn in the same conversation and the old turn is marked
    /// superseded by it, the ledger's own mechanism (§5.3): the conversation and the re-prime
    /// tail and the action ledger digest all leave a superseded turn out, so he sees one message
    /// and Rich remembers one.
    ///
    /// **Every desk of that company is retired, not un-primed** (Ray,
    /// esc-20261014T041934Z-2acf6aeb). A desk primed with the old text still holds it however
    /// often it is primed again: priming adds a turn to its session and takes nothing out, so
    /// Rich answered "an earlier one said F2". A retired desk's thread starts a fresh session at
    /// its next turn, primed from the ledger, which holds only the new text. Company-wide,
    /// because the digest carried the old line into every conversation of the company. Never
    /// the desk running a turn (continuity §3.1): this takes `&mut self`, which a running turn
    /// holds, so that desk cannot be the one in the chair here; the check states it anyway.
    ///
    /// **`fact` is what changed, recorded dated and primed after everything older** (review
    /// rv-20261014T044933Z-51176a08-f956): history is not scrubbed, so Rich's own earlier reply
    /// "Tap F2 to start dictation" stays in the tail and a saved handoff summary naming F2 stays
    /// in the priming. One CEO-facing action on the new turn, "<date>: The dictation key is now
    /// F3. The old key F2 no longer works.", goes into the action ledger digest of every
    /// conversation of the company, which is rendered after the summary and the tail, so the
    /// newest statement wins over the older text.
    ///
    /// **`turn_id` is `None` for a record written before the message's turn was kept** (review
    /// rv-20261014T050558Z-e9854621-0b36): the message replaced is then the turn the
    /// conversation was opened with ([`Self::open_conversation_with_message`] writes it first),
    /// and the fact is recorded and the desks retired all the same.
    /// Answers the new turn id.
    pub fn replace_app_message(&mut self, thread_id: &str, turn_id: Option<&str>, text: &str, fact: &str) -> Result<String, SpineError> {
        let old_turn = match turn_id {
            Some(turn) => Some(turn.to_string()),
            None => self.ledger.thread_turns(thread_id)?.first().map(|turn| turn.id.clone()),
        };
        let new_turn = self.raise_proactive(Some(thread_id), AttentionTier::Digest, text)?;
        if let Some(old_turn) = old_turn {
            self.ledger.mark_turn_superseded(&old_turn, &new_turn)?;
        }
        let today = time::OffsetDateTime::now_utc().date();
        self.ledger.record_action_with(
            Some(&new_turn),
            "settings_change",
            &format!("{today}: {fact}"),
            ActionVisibility::CeoFacing,
            ActionStatus::Completed,
        )?;
        let entity = self.ledger.thread_binding(thread_id)?.entity_id().clone();
        let ledger = &self.ledger;
        let of_company = |thread: &str| ledger.thread_binding(thread).is_ok_and(|b| b.entity_id() == &entity);
        let chair = !self.turn_in_progress && self.lease_primed_thread.as_deref().is_some_and(of_company);
        self.resident.retain(|thread, _| !of_company(thread));
        if chair {
            self.clear_lease();
        }
        if self.spare.as_ref().is_some_and(|spare| spare.entity == entity) {
            self.discard_spare();
        }
        self.publish_read_metadata();
        Ok(new_turn)
    }

    // ---- threads -----------------------------------------------------------

    /// Replace the entity registry (tests, or a future CEO-configured registry).
    pub fn set_entity_registry(&mut self, registry: EntityRegistry) {
        self.registry = registry;
        self.publish_read_metadata();
    }

    pub fn entity_registry(&self) -> &EntityRegistry {
        &self.registry
    }

    /// Create a thread with its IMMUTABLE entity home. The entity must be registered —
    /// an unknown one is refused, never invented (ECS §3.3).
    ///
    /// **AND THIS IS WHERE A SPARE FRONT DESK IS ADOPTED** (see [`SpareFrontDesk`]). If a
    /// primed spare is standing ready for this entity, the thread takes ITS reserved id and the
    /// spare is filed as this thread's resident desk, already primed. The CEO's first message
    /// then finds `prepare_request` resuming a primed desk instead of spawning and priming one,
    /// which is the whole of the saving.
    ///
    /// **The record is identical either way.** Same event, same immutable entity home, same
    /// fresh binding revision, same auto-activation. Only the id was decided earlier, and only
    /// so that the child already running was scoped to THIS conversation rather than to a name
    /// it would have had to borrow.
    ///
    /// **A spare for another entity is left standing.** It is not this thread's and adopting it
    /// would be the cross-entity leak its own doc names; it is retired at the next
    /// [`Spine::ready_a_spare_front_desk`] for a different entity, not here, because "he
    /// started a thread in company A" is not evidence about company B.
    pub fn create_thread(&mut self, title: &str, entity_id: &EntityId) -> Result<String, SpineError> {
        if !self.registry.contains(entity_id) {
            return Err(SpineError::UnknownEntity(entity_id.to_string()));
        }
        let adopted = match &self.spare {
            Some(spare) if &spare.entity == entity_id => Some(spare.reserved_thread.clone()),
            _ => None,
        };
        let id = match adopted {
            // **THE ID IS SPENT BEFORE THE DESK IS FILED, AND THE ORDER MATTERS.** If the
            // append fails the spare is still standing, unspent and re-usable by the next
            // attempt; if it succeeded, the desk is filed against a thread that now exists.
            // Filing first would leave a resident desk keyed to a thread nobody can read.
            Some(reserved) => {
                let id = self.ledger.create_thread_with_reserved_id(&reserved, title, entity_id)?;
                let spare = self.spare.take().expect("checked directly above");
                self.resident.insert(id.clone(), Resident {
                    lease: spare.lease,
                    primed: spare.primed,
                    onboarding_block: spare.onboarding_block,
                    context_chars: spare.context_chars,
                    context_usage: None,
                    context_pressure: None,
                    parked_at_ms: now_millis(),
                    pending_rotation_reason: None,
                    rotation_deferred_since: None,
                });
                // Never the desk just filed — it is the most recently parked, and the cap
                // retires the LEAST recently spoken.
                self.evict_beyond_the_cap();
                id
            }
            None => self.ledger.create_thread(title, entity_id)?,
        };
        if self.active.is_none() {
            self.activate(&id)?;
        }
        Ok(id)
    }

    /// The ACTIVE-CONTEXT TRANSACTION (ECS §11.3: *"Active-context switching is a
    /// transaction, so entity and thread cannot disagree"*).
    ///
    /// Re-reads the entity from the durable record and issues a NEW binding revision, so
    /// every command captured under the previous context is now stale and will be refused
    /// (ECS §3.4). Entity and thread move together or not at all: an unbound thread cannot
    /// be activated, so there is no state in which a thread is active without an entity.
    fn activate(&mut self, thread_id: &str) -> Result<ThreadBinding, SpineError> {
        let binding = self.ledger.rebind_at_new_revision(thread_id)?;
        self.active = Some(binding.clone());
        self.publish_read_metadata();
        Ok(binding)
    }

    /// The active context's binding, or `NoActiveThread`. NEVER falls back to "the first
    /// thread" — that would be picking an entity for the CEO, which UX §21 forbids
    /// ("Never default to the last entity") and which is the exact failure mode §22 calls
    /// out as unfakeable.
    pub fn ensure_active_thread(&mut self) -> Result<ThreadBinding, SpineError> {
        self.active.clone().ok_or(SpineError::NoActiveThread)
    }

    /// Ensure a thread is active WITHIN A NAMED ENTITY. This is the honest replacement for
    /// the old zero-argument auto-create: the entity is supplied by the caller (in the
    /// dogfood shell, from deterministic repository-root resolution — ECS §3.3), never
    /// inferred here. If a thread is already active in that entity it is returned; if one
    /// exists in that entity it is activated (most recent first); otherwise a new
    /// "Onboarding" thread is created bound to it.
    ///
    /// A thread active in a DIFFERENT entity is not reused — that would be a silent
    /// cross-entity switch.
    pub fn ensure_active_thread_in(&mut self, entity_id: &EntityId) -> Result<ThreadBinding, SpineError> {
        if !self.registry.contains(entity_id) {
            return Err(SpineError::UnknownEntity(entity_id.to_string()));
        }
        if let Some(active) = &self.active {
            if active.entity_id() == entity_id {
                return Ok(active.clone());
            }
        }
        let existing = self
            .ledger
            .threads()
            .iter()
            .filter(|t| t.entity_id() == Some(entity_id))
            .map(|t| (t.id.clone(), t.created_at))
            .max_by_key(|(_, created)| *created)
            .map(|(id, _)| id);
        match existing {
            Some(id) => self.activate(&id),
            None => {
                // "Onboarding" (the CEO, 2026-10-13: the first conversation of the first
                // company "should be named "Onboarding""; it was "Running" per the UX direction
                // doc §2.1): the pinned default thread's real title, not a placeholder the UI
                // has to cosmetically relabel.
                let id = self.create_thread("Onboarding", entity_id)?;
                self.activate(&id)
            }
        }
    }

    /// Switch the active topic view. Continuity holds across the switch because every
    /// thread folds over the SAME shared ledger (and later, shared loro) — WITHIN its
    /// entity. Switching to an unbound legacy thread fails closed.
    pub fn switch_thread(&mut self, thread_id: &str) -> Result<(), SpineError> {
        self.activate(thread_id)?;
        Ok(())
    }

    pub fn active_thread(&self) -> Option<&str> {
        self.active.as_ref().map(|b| b.thread_id())
    }

    /// The active entity area — the scope every read and write is currently under.
    pub fn active_entity(&self) -> Option<&EntityId> {
        self.active.as_ref().map(|b| b.entity_id())
    }

    /// The full active-context binding (person + entity + thread + revision).
    pub fn active_binding(&self) -> Option<&ThreadBinding> {
        self.active.as_ref()
    }

    /// ECS §3.4's FENCING CHECK for an outbound command: *"The command is rejected as
    /// `stale_binding` if the active-context binding revision ... has advanced ... an old
    /// Rich instance is never allowed to send, dispatch or write into a newly switched
    /// entity/thread. This is a fencing token, not a UI hint."*
    ///
    /// A caller that captured a binding at turn start passes it back here before acting on
    /// the outside world. A binding for a different thread or entity is `ScopeMismatch`; a
    /// binding older than the current active context is `StaleBinding`.
    ///
    /// **Honest scope note.** Slice 1's spine is synchronous and single-threaded (the
    /// shell serializes every call behind one `Mutex<Spine>`), so there is no concurrent
    /// in-process holder that can actually go stale today. This is the seam the Tauri
    /// command layer and any future async writer must call, and it is exercised by test
    /// rather than merely declared — but it is defense in depth, not a bug being fixed.
    pub fn verify_active_binding(&self, binding: &ThreadBinding) -> Result<(), SpineError> {
        let active = self.active.as_ref().ok_or(SpineError::NoActiveThread)?;
        if binding.thread_id() != active.thread_id() || binding.entity_id() != active.entity_id() {
            return Err(LedgerError::ScopeMismatch {
                thread_id: active.thread_id().to_string(),
                home: format!("{}/{}", active.entity_id(), active.thread_id()),
                presented: format!("{}/{}", binding.entity_id(), binding.thread_id()),
            }
            .into());
        }
        if binding.binding_revision() < active.binding_revision() {
            return Err(LedgerError::StaleBinding {
                thread_id: active.thread_id().to_string(),
                presented: binding.binding_revision(),
                current: active.binding_revision(),
            }
            .into());
        }
        Ok(())
    }

    pub fn threads(&self) -> Vec<ThreadSummary> {
        summaries(&self.ledger)
    }

    /// Scoped read. An unbound legacy thread returns `UnboundThread` rather than an empty
    /// conversation — see `Ledger::messages`.
    pub fn messages(&self, thread_id: &str) -> Result<Vec<Message>, SpineError> {
        Ok(self.ledger.messages(thread_id)?)
    }

    /// One thread's TYPED TIMELINE (UX brief §12) — the ledger and the machinery journal
    /// folded into one scoped projection.
    ///
    /// The read path, and the only assembly point: without it a consumer would have to
    /// re-derive the binding and re-read the journal itself, and every consumer that did
    /// that would be one more place the scope guard could be forgotten. Fails closed on an
    /// unbound thread exactly like `messages()`.
    ///
    /// A spine with no journal attached still returns a timeline — one with the
    /// conversation and no activity rows. That is the honest degrade: machinery retention
    /// is a separate store that may legitimately be absent (`set_machinery_journal` is
    /// optional), and an empty activity lane is not a claim that no tools ran.
    ///
    /// The same posture applies to the worker stream: with
    /// [`WorkerEventsSource::Disabled`] (the default) a `Task` call projects as an
    /// ordinary activity row, which is what the app did until 2026-08-29. See
    /// [`Spine::set_worker_events`] for what wiring it can and cannot promise.
    pub fn timeline(&self, thread_id: &str) -> Result<Timeline, SpineError> {
        let binding = self.ledger.thread_binding(thread_id)?;
        let machinery =
            self.machinery_journal.as_ref().map(|j| j.read_thread(thread_id)).unwrap_or_default();
        // The worker stream is read HERE rather than by the caller, for the same reason the
        // binding and the journal are: every consumer that assembled these itself would be
        // one more place the scope guard could be forgotten.
        let workers = self.worker_events.read(self.lease_session_id());
        Ok(Timeline::project_with_workers(&self.ledger, &binding, &machinery, &workers)?)
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    // ---- the turn flow -----------------------------------------------------

    /// Accept a CEO prompt. CRASH-SAFETY: the prompt is journaled + fsync'd `received`
    /// BEFORE anything else. QUEUE-NOT-INTERRUPT: if a turn is in flight it is queued,
    /// never delivered as an interrupt. Returns the turn id (persisted regardless).
    /// SCOPE FIRST, then persist-before-send. The binding is captured before the durable
    /// write, not after, because ECS §3.4 says the store rejects an unscoped event —
    /// journaling first and scoping second would make the crash window contain exactly the
    /// record the model forbids. A turn whose entity cannot be resolved is therefore
    /// REFUSED, loudly:
    ///
    ///   - no active context           -> `SpineError::NoActiveThread`
    ///   - active thread is unbound    -> `LedgerError::UnboundThread`
    ///   - binding contradicts the log -> `LedgerError::ScopeMismatch`
    ///
    /// Nothing is lost by refusing. The caller still holds the CEO's text and the send is
    /// blocked with an explanation, which is precisely UX §21's "Entity binding failure"
    /// behavior: *"Block send. State that Rich cannot safely determine which entity the
    /// work belongs to. Require an explicit entity choice."* The alternative — persisting
    /// an unscoped turn and sorting it out later — is how a message ends up rendered in
    /// the wrong entity, which is a privacy incident rather than an inconvenience.
    /// **His words, on the thread he typed them into** — the CEO's *"the CEO could open and
    /// run multiple things in parallel."*
    ///
    /// `send_message` used to refuse outright when the named thread was not the single
    /// active one: *"The conversation changed before your message was sent. Open the
    /// original conversation to try again."* He typed a sentence and the app threw it back.
    /// Now the named thread is made active and the message is his message on that thread —
    /// **answered, never refused.** If a turn is already running, [`Spine::submit_prompt`]
    /// queues it with ITS OWN binding and the boundary delivers it there (the `Queued`
    /// record has carried its binding since before this existed, for exactly this reason).
    ///
    /// **Activating mid-turn is safe and was already possible**: he can switch threads while
    /// Rich is working, `deliver` holds the binding it was handed rather than re-reading the
    /// active one, and the deferred proactive emits carry their own. What is new is only
    /// that the front desk he switches away from stays alive (`park_current_front_desk`).
    ///
    /// **What this is not.** It is not two front desks speaking at once; they take turns,
    /// because both bind the CEO's single ECS cursor (`native.rs`'s `CONVERSATION_SEAT`).
    /// Their WORK runs in parallel and has since slice 1 — one back end per thread.
    pub fn submit_prompt_to(&mut self, thread_id: &str, text: &str, source: Source) -> Result<String, SpineError> {
        if self.active_thread() != Some(thread_id) {
            self.activate(thread_id)?;
        }
        self.submit_prompt(text, source)
    }

    /// **Keep the mouth each prompt came through on its ledger record** (operator back-end
    /// spec r3 (s); the operator-client record's §7 item 3). Turned on by the shell only on an
    /// operator install, before the first turn, so the host can tell a phone assignment from
    /// one given at the desk without the walk handing it in. Never turned on anywhere else.
    pub fn keep_intake_channel(&mut self, keep: bool) {
        self.keep_intake_channel = keep;
    }

    pub fn submit_prompt(&mut self, text: &str, source: Source) -> Result<String, SpineError> {
        self.submit_prompt_inner(text, source, None, false)
    }

    /// **Words the app sends Rich because he pressed a button**, into `thread_id` (product hunt
    /// 2026-10-15, finding 14: "Start the questions" posted "Let's do the twenty minutes of
    /// questions about my business." as his own message). Rich receives them as an ordinary
    /// turn; the record marks them `pressed`, so no surface shows them in his name, and they
    /// are not read as a correction he made.
    pub fn submit_pressed_prompt_to(&mut self, thread_id: &str, text: &str) -> Result<String, SpineError> {
        if self.active_thread() != Some(thread_id) {
            self.activate(thread_id)?;
        }
        self.submit_prompt_inner(text, Source::Text, None, true)
    }

    /// A SPOKEN prompt, carrying what the capture path measured about Rich's own audible
    /// window while the audio was being recorded.
    ///
    /// **It exists because `source: Source::Jam` was the whole of a spoken turn's provenance,
    /// and that is not enough to tell an echo-born turn from a genuine one after the fact**
    /// (the voice pipeline's own handoff, richos `c712ccd5`). Candidate .5 left a bubble in the
    /// CEO's thread that was Rich's own counting, and the ledger cannot say so today.
    ///
    /// Separate from [`Self::submit_prompt`] rather than an extra argument on it, so that a
    /// typed turn writes `None` by CONSTRUCTION. See `Event::PromptReceived::rich_audible`:
    /// `None` is "not recorded" and is never to be read as "no".
    pub fn submit_prompt_spoken(
        &mut self,
        text: &str,
        source: Source,
        rich_audible: bool,
    ) -> Result<String, SpineError> {
        self.submit_prompt_inner(text, source, Some(rich_audible), false)
    }

    fn submit_prompt_inner(
        &mut self,
        text: &str,
        source: Source,
        rich_audible: Option<bool>,
        pressed: bool,
    ) -> Result<String, SpineError> {
        let binding = self.ensure_active_thread()?;
        let turn_id = if pressed {
            self.accept_pressed_prompt(&binding, text)?
        } else {
            self.accept_prompt(&binding, text, source, rich_audible, None, DESK_CHANNEL)?
        };

        // (2) if a turn is already running, queue it (never interrupt / never kill workers).
        //     The BINDING rides along, so a context switch while it waits cannot re-scope it.
        if self.turn_in_progress {
            self.queue.push_back(Queued {
                turn_id: turn_id.clone(),
                binding,
                text: text.to_string(),
                intake_id: None,
            });
            return Ok(turn_id);
        }
        // (3) otherwise deliver now.
        self.upstream_budget.new_message();
        let delivered = self.deliver(&turn_id, &binding, text, true);
        let boundary = self.after_turn_boundary(&binding);
        let queued = self.drain_queue();
        delivered?;
        boundary?;
        queued?;
        Ok(turn_id)
    }

    /// **The one durable write of a sentence of his**, with the mouth it came through when
    /// this install keeps it (operator back-end spec r3 (s); [`Self::keep_intake_channel`]).
    /// With keeping off, this is exactly the write each road made before, byte for byte: the
    /// channel is never written on a product install.
    fn record_prompt(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        rich_audible: Option<bool>,
        intake_id: Option<u64>,
        channel: &str,
    ) -> Result<String, SpineError> {
        if self.keep_intake_channel {
            return Ok(self.ledger.record_prompt_received_via(binding, text, source, intake_id, rich_audible, channel)?);
        }
        Ok(match (rich_audible, intake_id) {
            (Some(audible), None) => {
                self.ledger.record_prompt_received_spoken(binding, text, source, audible)?
            }
            (None, Some(id)) => {
                self.ledger.record_prompt_received_from_intake(binding, text, source, id)?
            }
            (None, None) => self.ledger.record_prompt_received(binding, text, source)?,
            // A SPOKEN prompt off the intake log. Nothing constructs one today — the intake
            // log's three utterance records are all typed — and rather than invent a ledger
            // call for a combination no caller has, the durable write keeps the intake id,
            // because that is the half a crash can duplicate. Named here so the day it has a
            // caller it is a decision and not a discovery.
            (Some(audible), Some(id)) => {
                let turn = self.ledger.record_prompt_received_from_intake(binding, text, source, id)?;
                eprintln!(
                    "[richos] a spoken prompt arrived off the intake log (turn {turn}, audible \
                     {audible}); its audibility is not recorded — no caller builds this today"
                );
                turn
            }
        })
    }

    /// [`Self::record_prompt`] for a channel message that carries its sender's own id (a
    /// phone's `client_id`) or is the transcript of a voice note the sender recorded
    /// (`voice_note`). `Source::Text`, off the intake log, never desk-spoken here — the same
    /// three facts the `Channel` drain arm has always written — with the channel kept only
    /// where this install keeps it, exactly as `record_prompt` decides.
    fn record_prompt_from_client(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        intake_id: u64,
        channel: &str,
        client_id: Option<&str>,
        voice_note: bool,
    ) -> Result<String, SpineError> {
        let kept = self.keep_intake_channel.then_some(channel);
        Ok(self.ledger.record_prompt_received_from_client(binding, text, Source::Text, intake_id, kept, client_id, voice_note)?)
    }

    /// **ACCEPTING one CEO utterance: everything that happens between his sentence arriving
    /// and the decision to queue or deliver it.** Persist, announce, and look at it for the
    /// three correction families — in that order, and never in any other.
    ///
    /// # Why this is a function now, and what it stops from happening
    ///
    /// It was the body of [`Self::submit_prompt_inner`], whose own comment calls that
    /// function *"the ONE function every CEO utterance passes through"*. That was true, and
    /// then it stopped being true in one place: `drain_intake` files an intake-borne message
    /// straight into the ledger and pushes it on the queue, so it emits the `queued` pair and
    /// stages NOTHING. Today that only costs the phone. The moment a typed message can be
    /// deferred through the same log (`IntakeRecord::Desk`, the CEO's §55), a correction he
    /// typed during a front-desk prime would be dropped on the floor with no trace — the
    /// flywheel's trigger silently not firing because of WHICH ROAD his words took.
    ///
    /// So the claim is restored as a function rather than as a sentence: every path that
    /// accepts a CEO utterance calls this, and a path that does not, does not compile into
    /// the same shape.
    ///
    /// `intake_id` is `Some` when the utterance came off the intake log. It rides into the
    /// ledger so the de-duplication key survives a crash between the ledger write and
    /// `mark_drained` — that window is the whole reason `record_prompt_received_from_intake`
    /// exists, and it is at-least-once into a ledger that can spot the replay.
    fn accept_prompt(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        rich_audible: Option<bool>,
        intake_id: Option<u64>,
        channel: &str,
    ) -> Result<String, SpineError> {
        // (1) persist-before-send, under a verified scope — the message is durable before
        //     any risk, and it is never durable without an entity.
        let turn_id = self.record_prompt(binding, text, source, rich_audible, intake_id, channel)?;

        // ADDITIVE (§13): the turn is durably `received`, so its first authoritative state
        // transition is emittable — §11's `queued`, the state whose timeline treatment is
        // "CEO bubble plus scaffold". Emitted for BOTH branches below, so a prompt that is
        // delivered immediately still shows queued -> working rather than appearing
        // mid-flight. Nothing on `stream.rs` changes here.
        self.emit_live(self.turn_status_event(binding, &turn_id, TurnStatus::Queued, None));
        // **AND WHAT HE ACTUALLY SAID** — `esc-20260919T003541Z-6885f74b`. Every other event in
        // this family is about Rich; until today the CEO's own sentence reached a surface only
        // by being PROJECTED, which was enough while the one surface was a webview that had
        // already drawn it itself. The phone subscribes to this family and drew nothing, so a
        // message typed on the Mac did not appear there until something asked for the rows
        // again. Emitted HERE, beside the queued status, because this is the one function every
        // CEO utterance passes through and the message is durable one statement above.
        self.emit_live(self.ceo_message_event(binding, &turn_id));
        self.emit_live(self.thread_summary_event(binding, &turn_id, ThreadStatus::Queued));

        // (1b) THE FLYWHEEL'S AUTOMATIC TRIGGER. Here, and not in the shell, because this
        //      is the ONE function every CEO utterance passes through — voice mode calls
        //      it with `Source::Jam` (`src-tauri/src/main.rs`) and the composer calls it
        //      with `Source::Text`, so a correction is caught by SPEAKING it with no
        //      command typed. Placed before the queue/deliver branch so a correction
        //      spoken while Rich is working is recorded the moment he says it rather than
        //      whenever the turn ahead of it finishes.
        self.stage_spoken_correction(binding, &turn_id, text, source);
        // (1c) THE SAME SEAM, for the OTHER correction family. Both live here rather than
        //      one here and one in the shell, because this is the one function every CEO
        //      utterance passes through, and a correction family reached by a different
        //      route would be a correction family with different rules about when it runs.
        self.stage_belief_correction(binding, text, source);
        // (1d) AND THE THIRD, which is the only one that watches rather than listens: what
        //      he DICTATED against what he actually sent. Same seam, same reason.
        self.stage_heard_correction(binding, &turn_id, text, source);
        Ok(turn_id)
    }

    /// [`Self::accept_prompt`] for words the app sends because he pressed a button: the same
    /// durable write and the same queued status, recorded `pressed`, and none of the three
    /// correction seams, which read only what he said.
    fn accept_pressed_prompt(&mut self, binding: &ThreadBinding, text: &str) -> Result<String, SpineError> {
        let turn_id = self.ledger.record_prompt_received_pressed(binding, text, Source::Text)?;
        self.emit_live(self.turn_status_event(binding, &turn_id, TurnStatus::Queued, None));
        self.emit_live(self.thread_summary_event(binding, &turn_id, ThreadStatus::Queued));
        Ok(turn_id)
    }

    /// Examine one CEO utterance for a spoken correction and, if it is one, write it down.
    ///
    /// **Infallible from the turn's point of view, deliberately.** `journal.rs` establishes
    /// the rule this follows: a machinery write failure never fails a turn, because
    /// machinery is not truth. The same holds here for a stronger reason — a staged
    /// correction is a QUESTION, and losing the chance to ask one must never cost the CEO
    /// the sentence he actually said. Every error path below logs and returns.
    ///
    /// Runs on `Text` and `Jam` and on nothing else: `Internal` is a re-prime or a
    /// handoff-summary request that the CEO never wrote, and `Proactive` is Rich speaking.
    /// Neither is a person correcting anything, and detecting a "correction" in RichOS's
    /// own priming payload would be the system teaching itself its own mistakes.
    fn stage_spoken_correction(
        &mut self,
        binding: &ThreadBinding,
        turn_id: &str,
        text: &str,
        source: Source,
    ) {
        if !matches!(source, Source::Text | Source::Jam) {
            return;
        }
        if self.candidates.is_none() {
            return;
        }
        let thread_id = binding.thread_id().to_string();

        // The record the correction is spoken AGAINST — the anchor's evidence. THE
        // UTTERANCE ITSELF IS EXCLUDED: it was journalled a few lines above, and a
        // correction that anchors to its own sentence would report evidence it invented.
        let record: Vec<String> = match self.ledger.messages(&thread_id) {
            Ok(msgs) => {
                let mut kept: Vec<String> = msgs
                    .into_iter()
                    .filter(|m| m.turn_id != turn_id)
                    .map(|m| m.text)
                    .collect();
                if kept.len() > ANCHOR_WINDOW_MESSAGES {
                    kept.drain(..kept.len() - ANCHOR_WINDOW_MESSAGES);
                }
                kept
            }
            Err(e) => {
                eprintln!("[richos] correction trigger could not read the record: {e}");
                Vec::new()
            }
        };

        let detection = crate::spoken::detect(text, &record);
        if detection.asks.is_empty() {
            return;
        }
        let desk = self.candidates.as_ref().expect("checked above");
        // A poisoned desk lock loses the QUESTION, never the turn — same posture as the
        // disk-failure arm below, and stated rather than unwrapped.
        let Ok(mut desk) = desk.lock() else {
            eprintln!("[richos] correction desk lock is poisoned — the question is dropped, the turn is not");
            return;
        };
        match desk.stage(&detection, &thread_id, turn_id, text) {
            Ok(staged) => {
                // Released before the observer runs: a surface that blocks must not hold
                // the desk shut against the CEO's own answer.
                drop(desk);
                if let Some(obs) = self.correction_observer.as_deref() {
                    if !staged.is_empty() {
                        obs.on_correction_staged(&staged);
                    }
                }
            }
            // A disk failure here loses ONE question. It must not lose the turn.
            Err(e) => eprintln!("[richos] correction could not be staged: {e}"),
        }
    }

    /// Examine one SENT message against what was DICTATED, and if he silently fixed a
    /// mis-heard word, write the question down.
    ///
    /// **Infallible from the turn's point of view**, the same posture and for the same
    /// reason as the two triggers above: a staged correction is a QUESTION, and losing the
    /// chance to ask one must never cost the CEO the sentence he actually said.
    ///
    /// **`Source::Text` only** — one narrower than the other two, and deliberately. A
    /// `Jam` turn is live voice: `rich://voice-transcript` goes straight into the thread as
    /// a turn, so there is no composer, no edit, and nothing to diff. Running this on a Jam
    /// turn would diff a fresh utterance against an unrelated open-wispr dictation, which
    /// is precisely the wrong-match failure `heard::MATCH_MIN_SIMILARITY` exists to stop —
    /// and would be asking it to stop something we could simply not do.
    ///
    /// **Nothing is written by this** beyond the desk record. `CandidateDesk::confirm` — a
    /// human answer — remains the only path to a vocabulary write, and there is no argument
    /// this function could pass to skip it.
    fn stage_heard_correction(
        &mut self,
        binding: &ThreadBinding,
        turn_id: &str,
        text: &str,
        source: Source,
    ) {
        if !matches!(source, Source::Text) {
            return;
        }
        let (Some(src), Some(desk)) = (self.heard.as_ref(), self.candidates.as_ref()) else {
            return;
        };
        let now = crate::util::now_millis();
        // Only the window the pairing condition can claim anyway, so the journal read is
        // bounded by the same number the match is — one policy, not two.
        let journal = src.recent(now.saturating_sub(crate::heard::MATCH_WINDOW_MS));
        if journal.is_empty() {
            return;
        }
        let review = crate::heard::review(&journal, text, now);
        if review.detection.asks.is_empty() {
            return;
        }
        let thread_id = binding.thread_id().to_string();
        let Ok(mut desk) = desk.lock() else {
            eprintln!("[richos] correction desk lock is poisoned — the question is dropped, the turn is not");
            return;
        };
        match desk.stage(&review.detection, &thread_id, turn_id, text) {
            Ok(staged) => {
                // Released before the observer runs, for the reason the spoken path does
                // it: a surface that blocks must not hold the desk shut against the CEO's
                // own answer.
                drop(desk);
                if let Some(obs) = self.correction_observer.as_deref() {
                    if !staged.is_empty() {
                        obs.on_correction_staged(&staged);
                    }
                }
            }
            // A disk failure here loses ONE question. It must not lose the turn.
            Err(e) => eprintln!("[richos] a dictation correction could not be staged: {e}"),
        }
    }

    /// Examine one CEO utterance for a correction of a recorded BELIEF and, if it resolves
    /// to exactly one loro record, file a proposal on the correction desk.
    ///
    /// **Infallible from the turn's point of view**, the same posture and for a stronger
    /// version of the reason [`Self::stage_spoken_correction`] is: a proposal is a
    /// QUESTION, and losing the chance to ask one must never cost the CEO the sentence he
    /// actually said. Every error path below logs and returns.
    ///
    /// **Nothing is written by this.** `CorrectionDesk::propose` runs the loro writer with
    /// `--dry-run` and stores what WOULD be written; `confirm` — a human answer — remains
    /// the only path to a loro write, and there is no argument this function could pass to
    /// skip it (`correction.rs`, ceo-decisions.md §7).
    ///
    /// Runs on `Text` and `Jam` only, for exactly the reason the spoken trigger does:
    /// `Internal` is a re-prime or a handoff-summary request the CEO never wrote, and
    /// `Proactive` is Rich speaking. Detecting a "correction" inside RichOS's own priming
    /// payload would be the system correcting its own memory from a copy of it.
    fn stage_belief_correction(&mut self, binding: &ThreadBinding, text: &str, source: Source) {
        if !matches!(source, Source::Text | Source::Jam) {
            return;
        }
        let (Some(desk), Some(prov)) = (self.correction_desk.as_ref(), self.loro_provenance.as_ref())
        else {
            return;
        };
        let thread_id = binding.thread_id().to_string();
        // The records Rich was ACTUALLY given for this thread. Cloned out from under the
        // lock rather than held across detection: the compiler writes this same map on a
        // re-prime, and detection must not be able to stall a rotation.
        let records = match prov.lock() {
            Ok(p) => p.records_for(&thread_id).to_vec(),
            Err(_) => {
                eprintln!("[richos] the loro provenance lock is poisoned — the question is dropped, the turn is not");
                return;
            }
        };
        if records.is_empty() {
            return;
        }
        let detection = crate::belief::detect(text, &records);
        if detection.asks.is_empty() {
            return;
        }
        let entity = binding.entity_id().as_str().to_string();
        for ask in &detection.asks {
            let Ok(mut d) = desk.lock() else {
                eprintln!("[richos] the correction desk lock is poisoned — the proposal is dropped, the turn is not");
                return;
            };
            let filed = d.propose(&entity, &thread_id, ask.proposed_write(), ask.why());
            drop(d);
            match filed {
                Ok(p) => {
                    if let Some(obs) = self.proposal_observer.as_deref() {
                        obs.on_correction_proposed(&p);
                    }
                }
                // A suppressed record, a writer refusal at --dry-run, or a disk failure
                // loses ONE proposal. None of them may lose the turn.
                Err(e) => eprintln!("[richos] a loro correction could not be proposed: {e}"),
            }
        }
    }

    /// Deliver one already-journaled turn to the current lease. Each assistant delta is
    /// persisted to the ledger FIRST (crash-safe partial capture) and then emitted LIVE
    /// to the UI sink, so the CEO sees Rich's reply render token-by-token. Turn-state
    /// events bracket the turn: `turn-started` (the calm "Rich is working" affordance)
    /// and a terminal `turn-completed` / `turn-error`, all keyed to thread + turn.
    ///
    /// `allow_recovery`: when the lease dies mid-turn (a positive signal — `prompt()`
    /// returned `Err`, never inferred from silence), attempt ONE automatic mid-turn-crash
    /// recovery + replay (continuity §5.3) if a lease factory is attached. Pass `false`
    /// from the recovery path itself so a lease that dies immediately on every respawn
    /// surfaces honestly after one attempt rather than looping forever.
    fn deliver(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        text: &str,
        allow_recovery: bool,
    ) -> Result<(), SpineError> {
        // The scope is re-verified at delivery, not merely at acceptance: a queued turn
        // may have waited across an active-context switch, and this is the last point
        // before the text reaches a compute lease.
        self.ledger.verify_binding(binding)?;
        let thread_id = binding.thread_id();
        // §1.5 gap #1: everything the lease said while no turn was in flight — its
        // session-start traffic on the very first turn, and whatever arrived after the
        // previous turn's response — lands NOW, as honest thread-scoped machinery, BEFORE
        // the priming turn below can put its own residue in the lane. The ordering is the
        // rule: honest first, then internal. See `pump_between_turn_stamped`.
        self.pump_between_turn_stamped(binding, false);
        let session_id = self.lease_session_id().unwrap_or_default().to_string();
        self.ledger.mark_turn_started(turn_id, &session_id)?;
        self.turn_in_progress = true;
        // Publish the running turn to the shared control. A MIRROR of `turn_in_progress`,
        // written at the same instant and from the same durable fact — never inferred from
        // activity or from silence (continuity §5.2).
        self.control.begin_turn(ActiveTurn {
            turn_id: turn_id.to_string(),
            thread_id: thread_id.to_string(),
            entity_id: Some(binding.entity_id().clone()),
            started_at: self.ledger.turn(turn_id).and_then(|t| t.started_at),
        });

        // Turn start: the UI shows the calm "Rich is working" state now.
        self.emit(StreamEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            at: now_millis(),
        });
        // ADDITIVE (§13), after the SAME durable write and after the existing event, never
        // instead of it.
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Working, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Working));

        // Preparation belongs to the accepted request. Its text stays internal, but
        // its wait and cancellation must share the user's visible lifecycle.
        let preparation = if self.control.stop_claim_for(turn_id).is_some() {
            Ok(())
        } else {
            self.prepare_request(binding).and_then(|_| {
                if self.control.stop_claim_for(turn_id).is_some() { return Ok(()); }
                let journal = self.machinery_journal.as_ref();
                let observer = self.machinery_observer.as_deref();
                let mut on_item = |item: TurnItem| {
                    if let TurnItem::Machinery(record) = item {
                        Self::retain_and_emit_machinery(journal, observer, record.stamp(thread_id, Some(turn_id), true));
                    }
                };
                self.lease.as_mut().ok_or(SpineError::NoLease)?
                    .prepare_work_turn(binding, turn_id, self.ledger.turn(turn_id).ok_or_else(|| SpineError::NoLease)?.source, text, &mut on_item).map_err(SpineError::from)?;
                let channel = self.input_channels.get(turn_id).cloned()
                    .or_else(|| self.ledger.turn(turn_id).and_then(|turn| turn.channel.clone()));
                self.lease.as_mut().ok_or(SpineError::NoLease)?.set_input_channel(channel.as_deref()).map_err(SpineError::from)
            })
        };
        let stopped_before_prompt = self.control.stop_claim_for(turn_id).is_some();
        let preparation_stop = match &preparation {
            Err(SpineError::Cognition(CognitionError::PrimingStopped(reason))) => Some(reason.clone()),
            _ => None,
        };
        let preparation_failed = preparation.is_err();
        let preparation = preparation.map_err(|error| match error {
            SpineError::Cognition(error) => error,
            other => CognitionError::Protocol(other.to_string()),
        });

        // A turn the CEO never sees (re-prime injection, the rotation handoff summary)
        // produces machinery he must never see either — §1.5's `internal` rule, the same
        // distinction `ActionVisibility::Internal` already draws (`ledger.rs:119-133`),
        // and the standing order that Rich never reveals or references session rotation.
        let internal_turn = self.ledger.turn(turn_id).map(|t| t.source == Source::Internal).unwrap_or(false);

        // Disjoint field borrows: the closure appends each delta to the ledger AND emits
        // it live, while `lease` streams — `ledger`, `lease`, `observer`, `machinery_*`
        // are distinct fields of *self, so all can be borrowed at once.
        let ledger = &mut self.ledger;
        let observer = self.observer.as_deref();
        let journal = self.machinery_journal.as_ref();
        let machinery_observer = self.machinery_observer.as_deref();
        let live_observer = self.live.as_deref();
        // The output record, so witness (a) can record a file the moment the call that wrote
        // it completes (Output side panel PRD §4.1 (a)).
        let output_store = self.output.as_ref().map(|(store, _)| store);
        // The engine's worker-lifecycle stream, borrowed as a distinct field so the closure
        // below can re-read it DURING the turn — that is what makes a delegation reach the
        // screen while it is happening rather than at the next `get_timeline`. It is read
        // lazily (`LiveTurn::on_machinery` calls the thunk only once this turn has a
        // delegation to resolve), so a turn that never delegates does no extra file I/O.
        let worker_source = &self.worker_events;
        // THE MEASURED WATERMARK'S INPUT (Frank F2). Two more disjoint field borrows, so
        // the drain closure can consume `usage_update` AS IT GOES PAST — which is the
        // only place it can be consumed: `usage_update` lands in the EVICTABLE Tier B
        // (`journal.rs`), so reading it back from the journal is reading something that
        // may legitimately be gone. 50 of these arrived across five probe runs and every
        // one of them was retained and ignored; this is the line that stops ignoring them.
        let usage_slot = &mut self.context_usage;
        let pressure_slot = &mut self.context_pressure;
        // Captured up front: the borrow checker will not let `self` be touched inside the
        // stream closure below, and the session identity is what names the team directory.
        let worker_session = self.lease.as_ref().map(|l| l.session_id().to_string());
        let lease = self.lease.as_mut();
        let mut persist_err: Option<LedgerError> = None;
        // The additive family's per-turn bookkeeping (§13). Holds no counter of its own:
        // message ids come from the ledger's `text_runs` fold, activity identity and
        // position from `machinery.rs`'s merge rules.
        let mut live_turn = LiveTurn::new(EventFence::for_turn(binding, turn_id), internal_turn);

        let stop = if stopped_before_prompt {
            // An internal prime completing is not the user's request completing.
            // Stop at this boundary prevents delivery of the user prompt.
            Ok(preparation_stop.unwrap_or_else(|| crate::native::STOP_REASON_CANCELLED.to_string()))
        } else if let Err(error) = preparation {
            Err(error)
        } else {
            let mut on_item = |item: TurnItem| match item {
                // `seq` comes from the LEASE now (§1.4 G1): ONE counter per turn, shared
                // by text and machinery. The spine no longer counts text items itself —
                // that would be the second counter G1 exists to forbid.
                TurnItem::Text { seq, text: c } => {
                    // Ledger stays the source of truth: persist the delta BEFORE emitting
                    // — and persist it AT its shared-sequence position, so the
                    // interleaving of text and machinery survives the process (§1.4 G1).
                    let at = match ledger.append_assistant_delta(turn_id, c, seq) {
                        Ok(at) => at,
                        Err(e) => { persist_err = Some(e); return; }
                    };
                    // Then emit it live (clean output: assistant text only ever reaches here).
                    if let Some(obs) = observer {
                        obs.on_event(&StreamEvent::Chunk {
                            thread_id: thread_id.to_string(),
                            turn_id: turn_id.to_string(),
                            seq,
                            text_delta: c.to_string(),
                            at,
                        });
                    }
                    // ADDITIVE (§13): message-started / message-delta, keyed to the run the
                    // LEDGER just folded this delta into — so the boundary between "he said
                    // X" and "then said Z" is the ledger's, not a second opinion.
                    // Borrowed, never cloned: this runs once per delta, and copying the
                    // whole reply-so-far each time would be quadratic in the reply length.
                    let runs = ledger.turn(turn_id).map(|t| t.text_runs.as_slice()).unwrap_or(&[]);
                    Self::forward_live(live_observer, live_turn.on_text(runs, seq, c, now_millis()));
                }
                TurnItem::Machinery(record) => {
                    let record = record.stamp(thread_id, Some(turn_id), internal_turn);
                    // THE ONE machinery record that is read rather than merely retained.
                    // Everything else in this family is routed for later interpretation;
                    // this pair decides when Rich rotates, so it is consumed here, live.
                    if let Some(usage) = record.context_usage() {
                        *usage_slot = Some(usage);
                        // MID-TURN, AT THE WALL. Rotation is structurally forbidden here
                        // (continuity §3.1) and firing one would be worse than firing
                        // late, so this DETECTS and RECORDS; `after_turn_boundary` acts.
                        // First crossing wins — it is the honest "when", and a later,
                        // higher reading in the same turn changes nothing that can be done
                        // about it.
                        if usage.fraction() >= CONTEXT_CRITICAL_RATIO && pressure_slot.is_none() {
                            *pressure_slot = Some((turn_id.to_string(), usage));
                        }
                    }
                    // A tool call ENDS the open run of prose — the ledger will fold the next
                    // delta into a new run — so the message is closed here rather than at
                    // the turn's end, and §5.2's "commentary, then activity" is live-accurate.
                    let runs = ledger.turn(turn_id).map(|t| t.text_runs.as_slice()).unwrap_or(&[]);
                    Self::forward_live(live_observer, live_turn.close_open_message(runs, now_millis()));
                    Self::forward_live(live_observer, live_turn.on_machinery(&record, &|| worker_source.read(worker_session.as_deref())));
                    // Witness (a): a write-tool call of Rich's that just completed. Recorded
                    // here, live, so the count ticks while the turn is still running; a
                    // failure is logged and never fails the turn (§4.7).
                    let written = live_turn.take_output_writes();
                    if let (Some(store), false) = (output_store, written.is_empty()) {
                        if let Err(e) = store.append(thread_id, &written) {
                            eprintln!("[richos] output: a file Rich wrote could not be recorded: {e}");
                        }
                    }
                    Self::retain_and_emit_machinery(journal, machinery_observer, record);
                }
                // **HIS ANSWER IS COMPLETE, THE TURN RUNS ON** (product hunt 2026-10-15, finding
                // 21). The turn is still working, durably and for every other purpose (a message
                // he sends now steers it, exactly as before); this status only tells the screen
                // the answer is done, so it stops saying "Rich is working". Never for a turn he
                // does not see.
                TurnItem::AnswerComplete => {
                    if !internal_turn {
                        let turn = ledger.turn(turn_id);
                        Self::forward_live(live_observer, vec![LiveEvent::TurnStatus {
                            fence: EventFence::for_turn(binding, turn_id),
                            status: TurnStatus::Working,
                            started_at: turn.and_then(|t| t.started_at),
                            active_duration_ms: None,
                            supersedes_turn_id: None,
                            answered: true,
                            at: now_millis(),
                        }]);
                    }
                }
            };
            lease.expect("successful preparation has a lease").prompt(text, &mut on_item)
        };
        // `ledger` / `lease` / `observer` / `machinery_*` borrows end here.

        self.turn_in_progress = false;
        // Witnesses (b) and (c) of the Output side panel PRD, for the lease that just ran this
        // turn: a front-desk worker's writes (nested inside the lead's turn) and the files the
        // turn's commands made. Whatever the outcome — a stopped turn's files were still
        // written. Before any rotation, so the session is still the one that ran the turn.
        self.project_output_evidence(thread_id);
        // The turn is over, so anything still arriving has no turn to belong to (§1.5).
        // Pumped HONESTLY here, and pumped here rather than only at the next turn's start
        // for one ordering reason: a rotation may follow immediately, and the rotation path
        // drains the lane as INTERNAL. Without this line, post-turn traffic that is the
        // CEO's own turn's aftermath would be stamped internal and disappear.
        //
        // It usually lands nothing. The adapter writes those updates AFTER the response
        // that unblocked the drain loop, so the reader thread has often not read them yet —
        // which is why this is one of several pump points and not the only one.
        self.pump_between_turn_stamped(binding, false);
        // THE CEO'S STOP, read once, here. `stop_claim_for` returns a claim only if it
        // names THIS turn id — a stop is a statement about one turn and must never fall
        // through onto whatever runs next.
        let stop_claim = self.control.stop_claim_for(turn_id);
        // The mirror is cleared whatever happens below, including on the error paths: a
        // stale "still running" would make the next stop request name a dead turn.
        self.control.end_turn(turn_id);

        // Close whatever run of prose was still open, whatever the outcome: the partial
        // text is already durable, so finalizing it is a statement about the ledger rather
        // than about the turn's success.
        let final_runs = self.ledger.turn(turn_id).map(|t| t.text_runs.as_slice()).unwrap_or(&[]);
        let closing = live_turn.close_open_message(final_runs, now_millis());
        self.emit_live(closing);

        // ONE LAST WORKER RE-JOIN, before the terminal turn status goes out. A worker's
        // state changes through hook writes in another process and produces no agent traffic
        // at all, so the last machinery record is not necessarily the last thing that
        // happened to a delegation. Without this the live row and a snapshot taken a second
        // later could disagree at exactly the moment the transcript settles and collapses.
        // Emitted for every outcome — completed, stopped, interrupted — because a turn that
        // ended badly still delegated.
        // The session id is threaded through here for the same reason it is at the other
        // call site: whose workers these are is derived from the session identity, never
        // guessed from a directory mtime.
        let session_id = self.lease_session_id();
        let converged = live_turn.on_turn_end(&|| self.worker_events.read(session_id));
        self.emit_live(converged);

        // A ledger write failing mid-stream is terminal for the turn (durability first).
        if let Some(e) = persist_err {
            self.ledger.interrupt_turn(turn_id, &e.to_string())?;
            self.emit(StreamEvent::TurnError {
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                reason: e.to_string(),
                at: now_millis(),
            });
            // ADDITIVE (§13). No recovery is attempted on this path, so `failed` is final.
            self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Failed, None));
            self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Failed));
            return Err(e.into());
        }

        // Track consumed-context regardless of outcome — a partial reply before a crash
        // still consumed context (measured, not asserted: MEASURE = len(prompt sent) +
        // len(whatever the ledger actually holds for this turn's reply so far)).
        let reply_len = self.ledger.turn(turn_id).map(|t| t.assistant_text.len()).unwrap_or(0);
        self.context_chars += text.len() + reply_len;

        // THE CEO STOPPED IT (§9.3 steps 4 and 5) — IF the stop is what ended the turn.
        //
        // Two things must not happen when a stop genuinely lands: a `completed` row for
        // work the CEO ended, and a crash-replay of a turn he explicitly told to stop.
        // Everything already streamed is already durable (each delta was persisted before
        // it was emitted), so step 4 — "preserve partial commentary, activity and
        // assistant output" — needs no code: not deleting it is the whole implementation.
        //
        // A THIRD THING MUST NOT HAPPEN, AND USED TO. This branch was taken on the mere
        // EXISTENCE of a stop claim, before looking at what the lease reported — so a turn
        // that ran to completion while the stop was in flight was written `Stopped`, and
        // `app/ui/timeline.js` rendered `You stopped after {d}` (the one row that names the
        // CEO as the cause of anything) above a complete, successful answer. It failed in
        // the direction where he believes he prevented something he did not.
        //
        // The race is real and narrow: `NativeCancelHandle::cancel` clones the sink and
        // appends `ChunkMsg::Cancel` AFTER whatever is already queued, so when the
        // adapter's `Done` is already in the channel `rx.recv()` returns it first and
        // `prompt` returns `"end_turn"` (`native.rs`'s `prompt` loop). That handle's own doc guards
        // `Done` RACING the wake; this is `Done` ALREADY QUEUED before the wake exists.
        //
        // The distinguishing signal was passed into `finish_stopped_turn` and thrown away.
        // `STOP_REASON_CANCELLED` — the adapter's acknowledgement that it HONOURED the
        // cancel — had no production reader anywhere. It has one now, right here.
        if let Some(claim) = stop_claim {
            let lease_reported = stop.as_deref().ok();
            // Did the stop end this turn?
            //
            //   `cancelled`             — yes, the adapter said so.
            //   `cancel_unacknowledged` — yes as far as RichOS is concerned: the CEO's stop
            //                             stands and we stopped rendering, whatever the
            //                             adapter is still doing.
            //   `None` (the lease errored) — the turn did not reach a terminal of its own,
            //                             and a stopped turn must never be crash-replayed.
            //   anything else           — NO. The lease reported its OWN terminal
            //                             (`end_turn`, `max_tokens`, `refusal`, …). The
            //                             turn ended because it finished.
            let stop_ended_the_turn = stopped_before_prompt || preparation_failed || !matches!(
                lease_reported,
                Some(r) if r != crate::native::STOP_REASON_CANCELLED
                    && r != crate::native::STOP_REASON_CANCEL_UNACKNOWLEDGED
            );
            if stop_ended_the_turn {
                self.finish_stopped_turn(turn_id, binding, &claim, lease_reported)?;
            } else {
                self.finish_completed_turn_the_stop_missed(turn_id, binding, &claim, lease_reported.unwrap())?;
            }
            return Ok(());
        }

        // ==============================================================================
        // THE UPSTREAM MODEL API FAILED (`open-items.md` row 3.30, measured 2026-09-03)
        // ==============================================================================
        //
        // Checked BEFORE the ordinary Ok/Err split, because it can arrive on EITHER side
        // and neither side alone would catch it:
        //
        //   * `Err(CognitionError)` — the client gave up. Its `Display` carries the
        //     child's words.
        //   * `Ok(stop_reason)` — and this is the shape that would otherwise be missed
        //     entirely. `claude` reports an API failure as an ASSISTANT MESSAGE, so
        //     `native.rs` streams `API Error: 529 Overloaded …` as text, the ledger
        //     persists it as Rich's reply, and the turn ends with a perfectly ordinary
        //     terminal. Nothing in this file used to look at it, so a `529` presented as
        //     a completed turn whose answer was a vendor diagnostic in Rich's voice.
        //
        // NEITHER WIRE SHAPE IS CAPTURED, and that is stated rather than glossed: the
        // twelve runs in docs/verification/native-claude-stream-json-2026-08-31/raw/
        // contain no API error at all. So both are covered, which is breadth in place of
        // a capture and not the same thing as one.
        // ==============================================================================
        // FILL-FIRST'S BACKSTOP: THE TURN WAS REFUSED FOR A USAGE LIMIT ANYWAY
        // ==============================================================================
        //
        // The switch normally happens BEFORE a turn (`prepare_request`), from the freshest
        // reading and the measured speed, so the account is left before it is full. This is
        // only for one turn that by itself used up what remained. The account is marked gone,
        // the next account with room is put in use, and his prompt is re-served on a fresh
        // lease exactly as the mid-turn-crash path does: one clean exchange, the failed turn
        // superseded. With no account left, nothing changes here and the turn ends as it
        // would have with one account.
        if let Err(e) = &stop {
            if let Some(resets_at) = crate::claude_accounts::parse_usage_limit(&e.to_string()) {
                let account = self.lease.as_ref().and_then(|l| l.account()).map(str::to_string);
                if let (Some(quota), Some(account)) = (self.quota_service(), account) {
                    if quota.limit_reached(&account, resets_at) == crate::claude_accounts::AfterLimit::Continue {
                        self.ledger.interrupt_turn(turn_id, &e.to_string())?;
                        self.raise_quota_notices(Some(thread_id));
                        self.emit(StreamEvent::TurnError {
                            thread_id: thread_id.to_string(),
                            turn_id: turn_id.to_string(),
                            reason: "Continuing on the next Claude account.".to_string(),
                            at: now_millis(),
                        });
                        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Recovering, None));
                        return self.recover_and_replay(turn_id, binding, text, ACCOUNT_EXHAUSTED);
                    }
                }
            }
        }

        if let Some(failure) = self.detect_upstream_failure(turn_id, &stop) {
            return self.finish_upstream_failure(turn_id, binding, text, failure, allow_recovery);
        }

        match stop {
            Ok(stop_reason) => {
                // A COMPLETED TURN IS THE POSITIVE SIGNAL that the upstream works, and it
                // is the only thing that restores the retry allowance. Never the absence
                // of a failure, which is what silence looks like.
                self.upstream_budget.succeeded();
                self.ledger.complete_turn(turn_id, &stop_reason)?;
                if stop_reason == "question_asked" {
                    self.pending_rotation_reason=Some("question_asked".into());
                }
                self.emit(StreamEvent::TurnCompleted {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    stop_reason,
                    at: now_millis(),
                });
                // ADDITIVE (§13). `complete_turn` wrote `ended_at` first, so the
                // `activeDurationMs` this carries is the MEASURED span, not an estimate —
                // and it is still `null` for a turn whose start was never recorded.
                self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Completed, None));
                self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Idle));
                Ok(())
            }
            Err(e) => {
                // Deltas up to the failure are already persisted + emitted; mark interrupted.
                self.ledger.interrupt_turn(turn_id, &e.to_string())?;
                // WHY it ended, classified here and made durable beside it (D2). Before
                // 2026-09-17 the raw `CognitionError` Display went out on the wire as the
                // CEO-facing `reason` and the surface replaced it with two fixed sentences
                // that were wrong for this case — a permanent condition called a snag, a
                // promise about saved work that did not exist, and a retry that could not
                // succeed. The statement below is authored in `interruption.rs` from the
                // classification and from counts read off the ledger.
                let (cause, statement) = self.record_interruption(turn_id, &e.to_string());
                self.emit(StreamEvent::TurnError {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.to_string(),
                    reason: statement,
                    at: now_millis(),
                });
                // Mid-turn-crash recovery (continuity §5.3): a positive termination
                // signal (this `Err`) just fired — attempt ONE automatic respawn +
                // replay if a factory is attached. A genuinely dead recovery path (no
                // factory, or the fresh spawn ALSO fails) surfaces the error honestly.
                // **AND ONLY WHEN A REPLAY COULD ACTUALLY HELP** — the dev walk's N4
                // (`docs/verification/2026-09-17-main-aa0165cc-dev-walk-audit.md`). Its
                // evidence is the app's own ledger, from ONE press of Enter:
                //
                //     PromptReceived   turn_7ba34774…  "What is 17 times 3? …"   at …297651
                //     TurnInterrupted  turn_7ba34774…  "cognition io: …"         at …297664
                //     ActionRecorded   act_fa1397b0…   crash_recovery
                //     PromptReceived   turn_0cc5570d…  "What is 17 times 3? …"   at …297673
                //
                // **One keystroke, two prompts, 22 ms apart**, and the audit names the
                // principle exactly: *"An immediate automatic replay is the right instinct
                // for a crash and the wrong one for a refusal the app has already read."*
                //
                // On that walk both attempts were refused locally so nothing was charged.
                // On a build that reaches the model it is two requests for one question —
                // and against a permanent condition, an expired subscription say, the replay
                // can never succeed and he pays for it anyway.
                //
                // The discriminator is the classification written one line above, and that
                // is the point of having classified at all: `offers_retry` is already the
                // answer to "can asking again help?", and it is the same fact that decides
                // whether a retry CONTROL is drawn for him. The app cannot coherently tell
                // him a retry is pointless and then perform one itself, 22 ms later, without
                // asking. One fact, both decisions.
                //
                // `Transient` and `Unknown` still recover, which is the whole of what this
                // seam was built for (continuity §5.3) — a crashed child is exactly the case
                // a silent respawn should cover, and it still does.
                let will_recover = allow_recovery && cause.offers_retry() && self.lease_factory.is_some();
                // ADDITIVE (§13). THE TWO CASES ARE DIFFERENT STATEMENTS, and emitting the
                // wrong one is how the wire and a reload stop agreeing:
                //   - no recovery ahead  -> `failed`, and the turn stays visible as failed;
                //   - recovery ahead     -> `recovering`, because this turn is about to be
                //     SUPERSEDED and a reload will not render it at all (it becomes
                //     `Visibility::Internal`, timeline.rs). Claiming `failed` here would
                //     leave a failure on screen that vanishes on the next launch.
                // `recovering` is §11's own state and is sourced by a POSITIVE termination
                // signal — this `Err` — never by inferring death from silence (§5.2).
                if will_recover {
                    self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Recovering, None));
                } else {
                    self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Failed, None));
                    self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Failed));
                }
                if will_recover {
                    return self.recover_and_replay(turn_id, binding, text, "mid-turn-crash");
                }
                Err(e.into())
            }
        }
    }

    /// Retain ONE machinery record, then hand it to the live sink.
    ///
    /// **Retention first, rendering second, and retention is unconditional** (§3.2:
    /// *"Routing and retention run ALWAYS. The toggle controls rendering ONLY"*). Making
    /// retention conditional would destroy the feature, because the CEO's requirement is
    /// to flip a thread he ALREADY HAD.
    ///
    /// Drain the lease's between-turn lane into the journal (techy-mode §1.5, gap #1).
    ///
    /// Returns how many records landed. `0` is an ordinary answer and never an error: it
    /// means the adapter has said nothing since the last drain, which is the normal state
    /// of a quiet session.
    ///
    /// **`turn_id` is `None` and stays `None`.** These records attach to the THREAD. There
    /// is no turn they belong to, and stamping the previous turn (or the next one) would be
    /// a false attribution — the one thing a record of what happened must not do. §1.4 G4
    /// makes `turn_id: None` a first-class state rather than a gap.
    ///
    /// **Scope is verified BEFORE the drain, not after.** A drain is destructive: the lane
    /// hands its records over and forgets them. Verifying first means a thread whose binding
    /// no longer holds leaves the traffic PARKED for whoever can scope it, instead of
    /// draining it into a refusal.
    ///
    /// Retention needs no special case here — [`Self::retain_and_emit_machinery`] is the
    /// same append the in-turn path uses, so the raw window ([`crate::journal::RawRetention`])
    /// covers these records exactly as it covers every other one.
    pub fn pump_between_turn(&mut self) -> usize {
        let Some(binding) = self.active.clone() else { return 0 };
        self.pump_between_turn_stamped(&binding, false)
    }

    /// [`Self::pump_between_turn`], with the `internal` bit supplied by the caller that
    /// knows what the lease was just doing.
    ///
    /// **THIS IS WHERE THE STANDING ORDER IS HELD, and it is a structural rule rather than
    /// an argument about what the adapter is likely to say.** Machinery produced by a turn
    /// the CEO never sees is `internal: true` (§1.5) — but the lane is filled by the reader
    /// thread at moments no turn owns, so "which turn produced it" is not a question the
    /// record can answer for itself. The rule the callers follow instead:
    ///
    ///   * the lane is drained as HONEST traffic *before* an internal turn is started, and
    ///   * drained as INTERNAL immediately *after* one finishes.
    ///
    /// So anything the lease says in the aftermath of a re-prime, a handoff summary or a
    /// rotation is stamped `internal: true` and can never render, without anyone having to
    /// reason about whether a particular vendor kind would give a rotation away. Rotation
    /// stays invisible to the CEO, and that includes its residue.
    fn pump_between_turn_stamped(&mut self, binding: &ThreadBinding, internal: bool) -> usize {
        if self.ledger.verify_binding(binding).is_err() {
            return 0;
        }
        let records = match self.lease.as_mut() {
            Some(lease) => lease.drain_between_turn(),
            None => return 0,
        };
        if records.is_empty() {
            return 0;
        }
        let thread_id = binding.thread_id().to_string();
        let journal = self.machinery_journal.as_ref();
        let observer = self.machinery_observer.as_deref();
        let landed = records.len();
        for record in records {
            let record = record.stamp(&thread_id, None, internal);
            Self::retain_and_emit_machinery(journal, observer, record);
        }
        landed
    }

    /// **A machinery write failure NEVER fails a turn** (§2.2's corollary). `deliver`
    /// makes a LEDGER write failure terminal for the turn — correctly, because the ledger
    /// is truth. Machinery is not truth: a failed write is logged to stderr and the turn
    /// continues. Taking a `&` journal (not `&mut`) is what lets this sit inside the
    /// streaming closure alongside the `&mut Ledger` borrow.
    fn retain_and_emit_machinery(
        journal: Option<&MachineryJournal>,
        observer: Option<&dyn MachineryObserver>,
        record: MachineryRecord,
    ) {
        if let Some(j) = journal {
            if let Err(e) = j.append(&record) {
                eprintln!("[richos] machinery journal write failed (turn continues): {e}");
            }
        }
        if let Some(obs) = observer {
            obs.on_machinery(&record);
        }
    }

    /// Terminal handling for a turn the CEO stopped (UX §9.3 steps 4-6).
    ///
    /// Two things must not happen here, and both used to be the only options: a
    /// `completed` row for work the CEO ended, and a crash-replay of a turn he explicitly
    /// told to stop. `Ledger::stop_turn` quotes the REQUEST's timestamp rather than the
    /// clock, so `You stopped after {duration}` is anchored to the moment he pressed the
    /// button and not to the moment the lease got round to letting go.
    ///
    /// **This is now reached only when the stop actually ended the turn** — the lease
    /// reported `cancelled`, reported `cancel_unacknowledged`, or reported nothing because
    /// it errored. A lease that reported its own terminal goes to
    /// [`Spine::finish_completed_turn_the_stop_missed`] instead.
    ///
    /// **On the legacy `stream.rs` family this emits `TurnCompleted`, not `TurnError`.**
    /// That family has four events and none of them means "stopped"; of the two that could
    /// carry a terminal, `TurnError` says something went wrong, and nothing did. The
    /// authoritative statement is `TurnStatus::Stopped` on the additive §13 family, which
    /// is what the shipping UI reads. The legacy `stop_reason` carries `stopped_by_ceo` so
    /// even a consumer that only listens to the old family is not misled.
    fn finish_stopped_turn(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        claim: &StopClaim,
        lease_stop_reason: Option<&str>,
    ) -> Result<(), SpineError> {
        let thread_id = binding.thread_id().to_string();
        self.ledger.stop_turn(turn_id, claim.requested_at)?;
        self.emit(StreamEvent::TurnCompleted {
            thread_id,
            turn_id: turn_id.to_string(),
            stop_reason: STOPPED_BY_CEO.to_string(),
            at: now_millis(),
        });
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Stopped, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Idle));

        // A lease that never acknowledged `session/cancel` is no longer KNOWN to be idle —
        // it may still be working, and whatever it says next would land on the next turn.
        // Retire it directly. Asking that same child for a rotation handoff would wait
        // on a process already known to ignore cancellation. The next request reconnects.
        if lease_stop_reason == Some(crate::native::STOP_REASON_CANCEL_UNACKNOWLEDGED) {
            // Never ask an unresponsive child for a handoff before retiring it.
            self.clear_lease();
            self.context_chars = 0;
            self.pending_rotation_reason = None;
        }
        Ok(())
    }

    /// Terminal handling for a turn the CEO tried to stop and which finished anyway.
    ///
    /// The order below is the whole fix and it is not interchangeable:
    ///
    /// 1. `complete_turn` — because that is what happened. Written FIRST, while the turn is
    ///    still `InFlight`, so it actually takes.
    /// 2. `stop_turn` — because the CEO really did ask, and a request that did not land is
    ///    still a fact worth having on disk. `Ledger::apply` refuses to move a turn that has
    ///    already ended (*"A stop OVERRIDES nothing that already ended: a turn that completed
    ///    before the stop request reached the lease stays completed, because it did"*), so
    ///    this writes a durable `TurnStopped` event that the projection correctly ignores.
    ///
    /// That guard existed and was UNREACHABLE from the live path — the spine never called
    /// those two in that order, so the only thing proving it was a unit test calling the
    /// ledger directly. This is the call site that makes it real.
    ///
    /// The events emitted are the ordinary completion events, byte-for-byte, because the
    /// ordinary thing is what occurred: `stop_reason` is the lease's own terminal, verbatim.
    /// Emitting `Stopped` here is what put "You stopped after" over a finished answer.
    ///
    /// The stop is not discarded, only outranked: `settle_stop_claim` still runs at the
    /// boundary, so anything the CEO had QUEUED behind this turn is still stopped. That is
    /// the part of his instruction that can still be honoured, and withholding is the safe
    /// direction.
    fn finish_completed_turn_the_stop_missed(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        claim: &StopClaim,
        lease_stop_reason: &str,
    ) -> Result<(), SpineError> {
        let thread_id = binding.thread_id().to_string();
        self.ledger.complete_turn(turn_id, lease_stop_reason)?;
        self.ledger.stop_turn(turn_id, claim.requested_at)?; // recorded; refused by the guard
        self.emit(StreamEvent::TurnCompleted {
            thread_id,
            turn_id: turn_id.to_string(),
            stop_reason: lease_stop_reason.to_string(),
            at: now_millis(),
        });
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Completed, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Idle));
        Ok(())
    }

    /// What a stop means for work the CEO had already lined up behind it.
    ///
    /// A stop that immediately starts the next queued turn is not a stop — the CEO would
    /// press the button and watch Rich carry straight on. So anything ACCEPTED BUT NOT YET
    /// DELIVERED when the stop request landed is stopped too, and his words stay in the
    /// ledger and on screen rather than being deleted.
    ///
    /// The cut is by intake id, not by wall clock: steering written BEFORE the stop is
    /// cancelled with it, steering written AFTER it is a new instruction and survives.
    fn settle_stop_claim(&mut self, binding: &ThreadBinding) -> Result<(), SpineError> {
        let Some(claim) = self.control.stop_claim() else { return Ok(()) };
        let mut survivors: VecDeque<Queued> = VecDeque::new();
        while let Some(queued) = self.queue.pop_front() {
            let written_before_the_stop = queued.intake_id.map(|id| id < claim.intake_id).unwrap_or(true);
            if written_before_the_stop {
                self.ledger.stop_turn(&queued.turn_id, claim.requested_at)?;
                self.emit_live(self.turn_status_event(&queued.binding, &queued.turn_id, TurnStatus::Stopped, None));
            } else {
                survivors.push_back(queued);
            }
        }
        self.queue = survivors;
        if self.queue.is_empty() {
            self.emit_live(self.thread_summary_event(binding, &claim.turn_id, ThreadStatus::Idle));
        }
        self.control.clear_stop_claim();
        Ok(())
    }

    /// **THE BINDING AN EVENT MAY BE FENCED WITH, AND IT IS NOT ALWAYS THE LEDGER'S.**
    ///
    /// Two revisions exist for one thread and only one of them is on disk:
    ///
    ///   * `Ledger::thread_binding` returns the revision written when the thread was BOUND
    ///     and never rewritten — 1, for the life of a thread nobody re-binds.
    ///   * `Spine::activate` mints a FRESH, higher one from `Ledger::take_revision`
    ///     (`rebind_at_new_revision`) for the active context, and deliberately persists
    ///     nothing. That is the revision `main.js` hands `RichTimeline.bind` — 3 in
    ///     `phone_intake_tests::the_fence_on_his_phone_message_is_the_one_the_open_window_is_holding`.
    ///
    /// `timeline.js accepts()` applies §13's staleness fence — `payload.bindingRevision <
    /// model.bindingRevision` → reject — so an event fenced from the LEDGER for the thread
    /// the CEO currently has open is thrown away by the one surface it was written for.
    ///
    /// **That is Ray's R3 on candidate .16, and the measurement is the diagnosis.** A message
    /// typed on his phone took **7,288 ms** to reach the open thread and arrived in the SAME
    /// PAINT as the finished reply, while the sidebar's dot moved at **+471 ms** — because
    /// `rich://thread-summary-updated` is handled outside the timeline model and is not
    /// fenced at all. The events were on time; the fenced ones were discarded, and nothing
    /// but `turn-completed`'s reload ever drew them.
    /// (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.5-stale-engine-and-phone-audit.md`
    /// §"R3, phone → Mac".)
    ///
    /// A desk message never had the defect: `submit_prompt_inner` fences on
    /// `ensure_active_thread()`. So this is the intake road being given the same binding the
    /// desk road already uses, and nothing more — *"from here on the spine does not know or
    /// care which mouth the CEO used"* is restored to the events as well as to the ledger.
    ///
    /// For any OTHER thread the ledger's binding is the right and only answer: no window is
    /// fenced on it, and a snapshot draws it when he opens it.
    fn fence_binding(&self, thread_id: &str) -> Result<ThreadBinding, SpineError> {
        if let Some(active) = &self.active {
            if active.thread_id() == thread_id {
                return Ok(active.clone());
            }
        }
        Ok(self.ledger.thread_binding(thread_id)?)
    }

    /// Move durable intake records into the ledger, now that the boundary is clear.
    ///
    /// This is where §9.2's "persisted before delivery" pays out: the CEO's steering was
    /// already on disk when he pressed Enter, and it becomes a real, entity-scoped turn
    /// here — through `record_prompt_received`, under a binding read fresh from the
    /// ledger, exactly like any other prompt. The intake log is not a second source of
    /// truth; this is the only thing that ever happens to a record.
    ///
    /// **A record that cannot be turned into a turn is NOT marked drained and NOT
    /// discarded.** It stops the drain and comes back at the next boundary. That blocks
    /// everything behind it, which is the deliberate trade: the CEO's words surviving
    /// matters more than the liveness of the records queued after them, and a binding
    /// failure here is systemic rather than per-message.
    fn drain_intake(&mut self) -> Result<(), SpineError> {
        for record in self.control.pending_intake() {
            match record {
                IntakeRecord::Steer { id, thread_id, text, .. } => {
                    // ALREADY DRAINED, and this is not a theoretical branch. The order below
                    // is ledger-first, marker-second, on purpose: a crash between them must
                    // re-present the CEO's words rather than lose them. So the drain is
                    // at-least-once, and without this check a restart in that window files
                    // the same sentence as a second turn — the CEO's one message, asked
                    // twice.
                    if self.ledger.turn_for_intake(id).is_some() {
                        self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                        continue;
                    }
                    let binding = self.fence_binding(&thread_id)?;
                    let turn_id = self.record_prompt(&binding, &text, Source::Text, None, Some(id), DESK_CHANNEL)?;
                    self.emit_live(self.turn_status_event(&binding, &turn_id, TurnStatus::Queued, None));
                    // AND WHAT HE ACTUALLY SAID — the same line `accept_prompt` carries, for the
                    // same reason. See the `Channel` arm below, where its absence was a defect a
                    // person could see.
                    self.emit_live(self.ceo_message_event(&binding, &turn_id));
                    self.emit_live(self.thread_summary_event(&binding, &turn_id, ThreadStatus::Queued));
                    self.queue.push_back(Queued { turn_id, binding, text, intake_id: Some(id) });
                    self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                }
                IntakeRecord::Channel { id, thread_id, text, channel, client_id, voice_note, .. } => {
                    // IDENTICAL TO THE `Steer` ARM ABOVE, AND THAT IS THE POINT. From here
                    // on the spine does not know or care which mouth the CEO used
                    // (phone-client plan §4.2 iv) — the same de-duplication check, the same
                    // fresh binding, the same `record_prompt_received_from_intake`, the
                    // same `Source::Text`. What arrives on the phone is the CEO's own words
                    // on a thread he already has, so anything else here would be a
                    // distinction the back end has no business making.
                    //
                    // **AND THAT SENTENCE WAS NOT TRUE OF THE EVENTS UNTIL TODAY** — Ray's
                    // candidate .13 walk, defect R3. `accept_prompt` emits three events for a
                    // CEO utterance and this arm emitted two: the status and the sidebar row,
                    // but never `rich://ceo-message`, which is the only one carrying his WORDS.
                    // The Mac's window therefore had nothing to draw when he typed on his phone
                    // — it had always drawn his own sentence itself — and his message appeared
                    // there only when `turn-completed` reloaded the thread. Measured on his own
                    // Android: **5.68 s and 5.77 s**, his words and the finished answer landing
                    // in the same frame, against 0.096 s for the other direction. At 2.99 s the
                    // only thing that had changed on the Mac was the sidebar's working dot —
                    // which is this arm's OTHER event arriving on time, and is the proof that
                    // the path was live and this one line was missing from it.
                    //
                    // **AND EMITTING IT WAS ONLY HALF.** Ray re-measured on candidate .16 with
                    // the line above in the build: **7,288 ms**, worse than the 5.68 s it was
                    // meant to fix, the bubble and the whole reply still in one paint, and the
                    // sidebar's dot still moving at +471 ms. The event was on the wire and the
                    // WINDOW DISCARDED IT — it was fenced from `Ledger::thread_binding` and the
                    // open thread is fenced at the ACTIVATION revision, so `timeline.js`
                    // `accepts()` read it as stale. `fence_binding` below is that fix, and the
                    // dot is the tell in both halves: it is the one event in this arm that no
                    // fence is applied to.
                    if self.ledger.turn_for_intake(id).is_some() {
                        self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                        continue;
                    }
                    let binding = self.fence_binding(&thread_id)?;
                    // **The one line where the mouth survives** (operator back-end spec r3 (s)).
                    // Kept only when this install keeps it; see `record_prompt`.
                    //
                    // **And the phone's own id for the words, when it sent one** (adoption
                    // ledger §2.8 row C5). It goes onto the turn here, so the `ceo-message`
                    // emitted three lines down and every later projection of this turn name
                    // the message the phone is already showing: an echo that beats the phone's
                    // receipt is then recognized on arrival instead of drawn as a second line.
                    //
                    // **And whether the words are the transcript of a voice note** (the
                    // record's `voice_note`), for the same reason: that `ceo-message` is the row
                    // that can beat the receipt, and it must be a voice row from the start, not
                    // a text bubble the receipt later turns into the recording.
                    let turn_id = if client_id.is_some() || voice_note {
                        self.record_prompt_from_client(&binding, &text, id, &channel, client_id.as_deref(), voice_note)?
                    } else {
                        self.record_prompt(&binding, &text, Source::Text, None, Some(id), &channel)?
                    };
                    self.input_channels.insert(turn_id.clone(), channel);
                    self.emit_live(self.turn_status_event(&binding, &turn_id, TurnStatus::Queued, None));
                    self.emit_live(self.ceo_message_event(&binding, &turn_id));
                    self.emit_live(self.thread_summary_event(&binding, &turn_id, ThreadStatus::Queued));
                    self.queue.push_back(Queued { turn_id, binding, text, intake_id: Some(id) });
                    self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                }
                IntakeRecord::Desk { id, thread_id, text, .. } => {
                    // **HIS OWN COMPOSER, DEFERRED — so it lands exactly as if it had never
                    // been deferred** (CEO §55). The two arms above file the turn and stop;
                    // this one goes through `accept_prompt`, which is the whole of the
                    // acceptance `submit_prompt_inner` performs. A correction he typed while
                    // the front desk was priming is therefore staged, and the only difference
                    // between this road and the direct one is WHEN the spine could be reached.
                    if self.ledger.turn_for_intake(id).is_some() {
                        self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                        continue;
                    }
                    let binding = self.fence_binding(&thread_id)?;
                    let turn_id = self.accept_prompt(&binding, &text, Source::Text, None, Some(id), DESK_CHANNEL)?;
                    self.queue.push_back(Queued { turn_id, binding, text, intake_id: Some(id) });
                    self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                }
                IntakeRecord::Stop { id, turn_id, at } => {
                    // Reached only when a stop outlived the process (see
                    // `reconcile_intake`) or named a turn that had already ended. A turn
                    // still open is stopped; anything else is left exactly as it is,
                    // because `Ledger::stop_turn` refuses to rewrite a terminal state.
                    let open = self
                        .ledger
                        .turn(&turn_id)
                        .map(|t| {
                            matches!(
                                t.state,
                                crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight
                            )
                        })
                        .unwrap_or(false);
                    if open {
                        self.ledger.stop_turn(&turn_id, at)?;
                    }
                    self.control.mark_drained(id).map_err(|e| SpineError::Steering(e.to_string()))?;
                }
                IntakeRecord::Drained { .. } => {}
            }
        }
        Ok(())
    }

    /// **THE IDLE DRAIN — the entry point the phone is unusable without** (phone-client
    /// plan §4.2 ii and vii).
    ///
    /// # The gap this closes, measured rather than recalled
    ///
    /// Before this function, `drain_intake` was called from exactly two places:
    /// `after_turn_boundary` and `reconcile_intake` at boot. Both are true today — `:2658`
    /// and `:2614` in the version this was written against, and the plan cites the same two
    /// lines. **Neither of them happens while the app sits idle.** So a message written to
    /// the intake log by a channel that is not the desktop window would sit there until the
    /// CEO's next desktop turn or the next launch. On the desktop that was invisible,
    /// because the only writer was the steering composer, which by construction only writes
    /// while a turn is running and therefore always has a boundary coming. A phone does not:
    /// *"He is standing in the kitchen watching his phone"*, and silence there looks exactly
    /// like a broken bridge.
    ///
    /// # What it does, and what it deliberately does not
    ///
    /// Take the lock, drain, and start the turn if none is running — the plan's own three
    /// clauses, in that order. It is `reconcile_intake` minus the crash-replay half, which
    /// is why it is three lines rather than thirty: **no new machinery, one new caller for
    /// machinery that already exists.**
    ///
    /// **It never runs mid-turn**, and that is structural twice over. A caller cannot even
    /// reach it while a turn is running, because the shell holds the one `Spine` behind a
    /// mutex that `submit_prompt` keeps for the whole turn. The `turn_in_progress` check
    /// below is the second lock on the same door: if a future caller ever does hold the
    /// spine mid-turn, this returns without touching the queue rather than delivering a
    /// prompt into a running turn. Continuity §3.1 is queue-not-interrupt by construction
    /// and this does not become the exception.
    ///
    /// **It is called after a write, never on a timer.** Plan §6 forbids *"a polling loop
    /// that wakes the Mac"*, and a drain that runs on a schedule is exactly that. The
    /// bridge writes the record, `fsync`s it, answers the phone, and then calls this.
    ///
    /// Returns what `drain_queue` returns: the turn runs to completion inside this call,
    /// exactly as it does for a desktop prompt.
    pub fn queue_question_answer(&mut self,delivery:&crate::questions::Delivery)->Result<String,SpineError> {
        let binding=self.fence_binding(&delivery.thread_id)?;
        if binding.entity_id().as_str()!=delivery.entity_id {return Err(SpineError::Steering("Question scope mismatch".into()));}
        let turn_id=self.ledger.record_question_answer(&binding,delivery)?;
        if self.ledger.turn(&turn_id).is_some_and(|t|t.state==crate::ledger::TurnState::Received)
            && !self.queue.iter().any(|q|q.turn_id==turn_id) {
            if delivery.set_id.is_none(){self.emit_live(self.ceo_message_event(&binding,&turn_id));}
            self.queue.push_back(Queued{turn_id:turn_id.clone(),binding,text:delivery.text.clone(),intake_id:None});
        }
        Ok(turn_id)
    }

    pub fn poll_intake(&mut self) -> Result<(), SpineError> {
        if self.turn_in_progress {
            return Ok(());
        }
        self.drain_intake()?;
        self.drain_queue()
    }

    /// **Every conversation whose turn has ended since this was last asked, and the list
    /// emptied.** The shell's one question at every turn boundary: which conversations may
    /// now start the work Rich wrote down during them (`WorkHost::adopt_registered`,
    /// background-work spec §7.1: *"`prepare` runs on the work lease after the turn has
    /// ended"*).
    ///
    /// **Why the spine answers it and the caller does not guess.** The typed send used to
    /// adopt for the thread that was active afterwards, and the spoken turn and the phone's
    /// drain adopted for nothing (esc-20260926T083231Z-23865924): a task he gave aloud or
    /// from the phone sat `Registered` until he next typed. Worse, one call here can run
    /// turns on several conversations — `poll_intake` drains every record on the log, a
    /// turn's boundary drains what arrived during it, the boot reconciliation and the front
    /// desk's priming drain what was waiting — and only `after_turn_boundary` sees each of
    /// them end. So each ending is written down there, and every entrance asks here after
    /// its call and adopts for all of them, with the spine released first (§0 row 3).
    ///
    /// A turn that FAILED is on the list too: a registration written before the failure is
    /// still a receipt he was given, and `adopt_registered` is a directory read that adopts
    /// only `Registered` rows and each one once. Nothing here retries or restarts anything
    /// (spec §6.3).
    pub fn take_ended_turns(&mut self) -> Vec<ThreadBinding> {
        std::mem::take(&mut self.ended_turns)
    }

    /// Startup reconciliation: apply stop requests that outlived the process.
    ///
    /// The crash window this closes is one line wide. The stop request is fsync'd before
    /// the lease is touched (`steering.rs`), and the ledger's `TurnStopped` is written when
    /// the turn ends. Die in between and the turn is `in_flight` forever with a durable
    /// stop request sitting beside it — and §5.3's crash-replay would REPLAY a turn the
    /// CEO explicitly stopped. Running this at boot is what makes that impossible.
    ///
    /// Called by the shell after the ledger and the control are both open. Safe to call
    /// when there is nothing to do.
    pub fn reconcile_intake(&mut self) -> Result<(), SpineError> {
        let interrupted: Vec<String> = self.ledger.pending_turns().iter()
            .filter(|turn| turn.source != Source::Internal)
            .map(|turn| turn.id.clone()).collect();
        for record in self.control.pending_intake() {
            if let IntakeRecord::Stop { turn_id, at, .. } = record {
                self.ledger.stop_turn_after_restart(&turn_id, at)?;
            }
        }
        self.drain_intake()?;
        for id in interrupted {
            let Some(turn) = self.ledger.turn(&id) else { continue };
            if !matches!(turn.state, crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight) {
                continue;
            }
            let binding = self.fence_binding(&turn.thread_id)?;
            if turn.state == crate::ledger::TurnState::Received && (id.starts_with("question-answer:") || id.starts_with("question-followup:")) {
                if !self.queue.iter().any(|q|q.turn_id==id) {
                    self.queue.push_back(Queued{turn_id:id.clone(),binding,text:turn.user_text.clone(),intake_id:None});
                }
                continue;
            }
            self.ledger.interrupt_turn_after_restart(&id, "RichOS closed before this request finished. Your message is saved; send it again to retry.")?;
            self.emit_live(self.turn_status_event(&binding, &id, TurnStatus::Failed, None));
            self.emit_live(self.thread_summary_event(&binding, &id, ThreadStatus::Failed));
        }
        self.drain_queue()
    }

    /// Deliver queued prompts (FIFO) now that the turn boundary is clear.
    fn drain_queue(&mut self) -> Result<(), SpineError> {
        let mut failure = None;
        while !self.turn_in_progress {
            let Some(next) = self.queue.pop_front() else { break };
            self.upstream_budget.new_message();
            let delivered = self.deliver(&next.turn_id, &next.binding, &next.text, true);
            let boundary = self.after_turn_boundary(&next.binding);
            if let Err(error) = delivered.and(boundary) {
                if failure.is_none() { failure = Some(error); }
            }
        }
        match failure { Some(error) => Err(error), None => Ok(()) }
    }

    /// Everything that happens AT a turn boundary, after delivery and before the next
    /// prompt is considered (continuity §3.1: the turn-boundary controller's other job,
    /// alongside queue-not-interrupt). Runs whether the turn came from `submit_prompt`
    /// directly or from draining the queue — both call sites only reach here once
    /// `turn_in_progress` is false, so nothing below ever runs mid-turn.
    fn after_turn_boundary(&mut self, binding: &ThreadBinding) -> Result<(), SpineError> {
        // FIRST, before anything below can fail: this turn has ended, whatever it ended in,
        // so the work it wrote down may now start (`take_ended_turns`, background-work §7.1).
        if !self.ended_turns.iter().any(|known| known.thread_id() == binding.thread_id()) {
            self.ended_turns.push(binding.clone());
        }
        // Keep transient phone provenance through an in-process lease recovery, then
        // release it at the boundary. Product installs still write no channel to the ledger.
        self.input_channels.retain(|id, _| self.ledger.turn(id).is_some_and(|turn|
            matches!(turn.state, crate::ledger::TurnState::Received | crate::ledger::TurnState::InFlight)));
        // A switch notice or a high-speed alert that arrived while this turn held the spine
        // is said now, the first moment it can be.
        self.raise_quota_notices(Some(binding.thread_id()));
        // The CEO's two mid-turn controls settle HERE, at the boundary, and in this order.
        //
        // DRAIN FIRST, THEN STOP — and the order was chosen by running it the other way.
        // Settling the stop first drops the pre-stop steering records straight out of the
        // intake log, so words the CEO had already watched appear as a bubble never became
        // a ledger turn: they survived until the next reload and then vanished. Draining
        // first means everything he typed becomes a real, durable turn, and the stop then
        // marks the ones it covers as stopped. His words stay on screen, correctly marked,
        // and a reload agrees with what he saw.
        self.drain_intake()?;
        self.settle_stop_claim(binding)?;
        self.settle_context_pressure()?;
        // Never start hidden model roundtrips after telling the user the request
        // finished. The next request performs this work under Working and Stop.
        if self.pending_rotation_reason.is_none() && self.lease_factory.is_some() && self.watermark_reached() {
            self.pending_rotation_reason = Some("context-watermark".to_string());
        }
        Ok(())
    }

    /// Settle a mid-turn crossing of `CONTEXT_CRITICAL_RATIO`, at the first legal moment.
    ///
    /// **This is the honest answer to "what happens when a turn's own consumption crosses
    /// the limit while it is running", and the honest answer is: not much, on purpose.**
    /// The three things this process can actually do are done here, and the fourth thing
    /// is named rather than faked.
    ///
    /// 1. **It is written down, durably, against the turn it happened in.** `Internal`
    ///    visibility, so it never reaches the CEO's view and is excluded from the re-prime
    ///    digest (`reprime.rs`) — the standing order is that Rich never reveals rotation,
    ///    and this is rotation's cause.
    /// 2. **The next boundary schedules rotation, whatever the configured ratio says.** An operator
    ///    who set `watermark_ratio` to 0.99 has, at 0.95 measured, already been overtaken
    ///    by events; `context-critical` outranks the policy that let it get here.
    /// 3. **Nothing rotates now.** `after_turn_boundary` is only ever reached with
    ///    `turn_in_progress == false` — that is what makes this legal, and it is the same
    ///    invariant the whole controller is built on.
    /// 4. **The turn may still die at the wall before it reaches this point, and this
    ///    cannot prevent that.** A client cannot stop a session it does not own from
    ///    running out of context inside a request it has already sent. What happens then
    ///    is the existing crash path, unchanged and correct: the adapter returns a
    ///    positive termination signal, the turn is marked `interrupted` in the durable
    ///    ledger, and `recover_and_replay` (§5.3) replays it on a fresh lease with
    ///    `supersedesTurnId`. The CEO sees a calm reconnect, not a stack trace. **The
    ///    watermark's job is to make that path rare; this constant's job is to notice when
    ///    it nearly happened.**
    fn settle_context_pressure(&mut self) -> Result<(), SpineError> {
        let Some((turn_id, usage)) = self.context_pressure.take() else { return Ok(()) };
        let session = self.lease_session_id().unwrap_or("unknown").to_string();
        // NO FACTORY MEANS NO ROTATION, and the record says so rather than pretending.
        // This mirrors the watermark path's existing honest degrade (`set_lease_factory`'s
        // doc: "watermark: never fires since rotation can't proceed"). Setting
        // `pending_rotation_reason` here without a factory would make `after_turn_boundary`
        // return `NoLeaseFactory` out of a turn that SUCCEEDED — reporting a failure for
        // work that completed, which is the wrong direction to be wrong in.
        let can_rotate = self.lease_factory.is_some();
        // THE OUTCOME GOES FIRST, and that ordering is load-bearing rather than stylistic:
        // `Ledger::record_action_with` truncates `detail` at ACTION_DETAIL_MAX_CHARS (160),
        // and the first version of this line put the outcome LAST — where truncation ate
        // "rotation cannot proceed" and left a record that read as though rotation had
        // happened. A record whose most important clause is the one that gets cut is worse
        // than no record.
        let outcome = if can_rotate {
            "rotation deferred to this boundary, never fired mid-turn"
        } else {
            "NO lease factory: rotation cannot proceed, detected and recorded only"
        };
        self.ledger.record_action_with(
            Some(&turn_id),
            "context_pressure",
            &format!(
                "{outcome}; used={} size={} = {:.1}% (over the {:.0}% critical mark) mid-turn on session={session}",
                usage.used,
                usage.size,
                usage.fraction() * 100.0,
                CONTEXT_CRITICAL_RATIO * 100.0,
            ),
            ActionVisibility::Internal,
            ActionStatus::Completed,
        )?;
        // Does not clobber an explicit request already pending — a CEO-requested rotation
        // and this one do the same thing, and his reason is the more informative one.
        if can_rotate && self.pending_rotation_reason.is_none() {
            self.pending_rotation_reason = Some("context-critical".to_string());
        }
        Ok(())
    }

    /// The ADDITIVE half of a proactive message (§13).
    ///
    /// **This is the ONE place a real, non-`unknown` message phase exists.** A streamed
    /// reply has no phase signal anywhere on the wire (`live.rs`'s module doc, with the
    /// measurement behind it), but a proactive message knows what it is because the LEDGER
    /// recorded it as one — `Source::Proactive` plus a tier. So it is emitted as
    /// `phase: "proactive"`, and it is the proof that the phase field is a real field
    /// rather than a decorative one.
    ///
    /// A proactive turn is written ATOMICALLY, so there is no delivery span to measure:
    /// `started_at` is `None` and `activeDurationMs` is `null`. It emits `completed`
    /// (which it already is) and never `queued` or `working`, both of which would describe
    /// a delivery that never happened.
    fn emit_proactive_live(&self, binding: &ThreadBinding, turn_id: &str, tier: AttentionTier, text: &str) {
        let fence = EventFence::for_turn(binding, turn_id);
        self.emit_live(proactive_message_events(&fence, tier, text, now_millis()));
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Completed, None));
        // Said during a turn of this same thread, the thread is still working: the message
        // is complete, the turn beside it is not.
        let working = self.turn_in_progress
            && self.control.active_turn().is_some_and(|active| active.thread_id == binding.thread_id());
        let status = if working { ThreadStatus::Working } else { ThreadStatus::Idle };
        self.emit_live(self.thread_summary_event(binding, turn_id, status));
    }

    /// Mid-turn-crash recovery + replay (continuity §5.3). The dead lease is dropped;
    /// a fresh one is spawned and re-primed (`prime_lease_if_needed`, called from the
    /// nested `deliver`, naturally carries the just-interrupted turn forward — it is
    /// still `Interrupted` in the ledger at this point, so `reprime.rs`'s
    /// `pending_decisions` picks it up as "Unfinished: <text>" — plus the full action
    /// ledger, the anti-double-execution guard). The CEO's original prompt is then
    /// RE-SERVED as a brand-new turn; the failed turn is marked superseded (never edited
    /// in place — it stays in the ledger as the durable crash record) so the CEO sees
    /// ONE clean exchange, not a duplicate.
    /// **Was this turn's outcome an upstream model-API failure?** `None` for everything
    /// else, which is the answer this returns almost always.
    ///
    /// Three channels are read, in the order they are trustworthy:
    ///
    /// 1. the lease's own error `Display`, when it errored;
    /// 2. the assistant TEXT the ledger holds for this turn — the shape a `529` actually
    ///    takes on this wire, where the vendor reports it as a message rather than as a
    ///    transport failure;
    /// 3. nothing else. A `stop_reason` is a vendor terminal, not a diagnosis, and
    ///    inventing a fault from `error_during_execution` would classify every interrupted
    ///    turn as an outage.
    ///
    /// **The text channel is read line by line and only the LAST run is considered.** A
    /// turn whose real answer happened to quote an `API Error:` line — the CEO pasting a
    /// log and asking about it — must not be reported as an outage, and the failure always
    /// arrives as the final thing the lease said.
    fn detect_upstream_failure(
        &self,
        turn_id: &str,
        stop: &Result<String, CognitionError>,
    ) -> Option<crate::upstream::UpstreamFailure> {
        if let Err(e) = stop {
            if let Some(f) = crate::upstream::UpstreamFailure::classify_lines(&e.to_string()) {
                return Some(f);
            }
        }
        let turn = self.ledger.turn(turn_id)?;
        let last = turn.text_runs.last()?;
        crate::upstream::UpstreamFailure::classify_lines(&last.text)
    }

    /// **WHAT SURVIVED THIS TURN, read off the ledger rather than narrated.**
    ///
    /// Extracted from `finish_upstream_failure` on 2026-09-17, behavior unchanged, so the
    /// ordinary interruption path (D2) states what is on disk using the SAME counts the
    /// upstream path does. Two implementations of "what survived" is how the sentence and
    /// the screen come to disagree, and the half that drifted would be the half that tells
    /// a customer his work is safe.
    ///
    /// `lease_context_lost` is `true` unconditionally: whatever the session had worked out
    /// and had not yet said is gone with the lease, on every one of these endings.
    fn turn_loss(&self, turn_id: &str) -> crate::upstream::TurnLoss {
        let turn = self.ledger.turn(turn_id);
        crate::upstream::TurnLoss {
            prompt_is_durable: turn.map(|t| !t.user_text.is_empty()).unwrap_or(false),
            // The vendor's diagnostic is NOT part of what survived, so it is cut out of
            // the count by the SAME function the timeline projects with
            // (`upstream::split_at_vendor_diagnostic`). Telling him "169 characters of the
            // answer are saved" when all 169 are the error message would be a true number
            // about the wrong thing — and having two implementations of "which half is the
            // answer" would be worse: the sentence and the screen would eventually
            // disagree, and neither would be wrong on its own terms.
            partial_reply_chars: turn
                .map(|t| {
                    t.text_runs
                        .iter()
                        .map(|r| {
                            crate::upstream::split_at_vendor_diagnostic(&r.text).0.chars().count()
                        })
                        .sum()
                })
                .unwrap_or(0),
            actions_recorded: self
                .ledger
                .actions()
                .iter()
                .filter(|a| a.turn_id.as_deref() == Some(turn_id))
                .count(),
            lease_context_lost: true,
        }
    }

    /// **CLASSIFY AN ORDINARY INTERRUPTION AND MAKE THE ANSWER DURABLE — the nightly's D2.**
    ///
    /// Called immediately after `interrupt_turn`, never before it, per
    /// `Ledger::record_interruption_cause`'s stated contract.
    ///
    /// It is handed the SAME string that was written as the turn's `reason`, so the record
    /// and the reason cannot describe two different events. The returned statement is what
    /// the CEO is shown — assembled here from the authored sentences so the live event and a
    /// reload months later say the same thing.
    ///
    /// **A failure to write the record is NOT a failure of the turn.** The turn is already
    /// terminal and durable at this point; losing the explanation degrades the surface to
    /// its generic card, which is exactly the fallback a pre-2026-09-17 record gets. Raising
    /// here would convert a cosmetic loss into a lost error path.
    fn record_interruption(
        &mut self,
        turn_id: &str,
        reason: &str,
    ) -> (crate::interruption::InterruptionCause, String) {
        let cause = crate::interruption::classify(reason);
        let loss = self.turn_loss(turn_id);
        let record = crate::interruption::InterruptionRecord::new(cause, &loss);
        let statement = match &record.loss_message {
            Some(loss) => format!("{} {loss}", record.ceo_message),
            None => record.ceo_message.clone(),
        };
        // The operator's line keeps the machinery the CEO never sees, beside the tag, so a
        // misclassification is visible in a log without reading the ledger.
        eprintln!("[richos] turn interrupted [{}]: {reason}", cause.tag());
        if let Err(e) = self.ledger.record_interruption_cause(turn_id, &record) {
            eprintln!("[richos] the reason for that interruption could not be written down: {e}");
        }
        (cause, statement)
    }

    /// **Everything that happens when a turn dies to the upstream API, in the order the
    /// durability rules require.**
    ///
    /// 1. `interrupt_turn` — the turn ended, and that fact is written first. Its `reason`
    ///    stays the OPERATOR's summary (`upstream overloaded http=529 request_id=…`)
    ///    rather than the CEO's sentence, because `TimelineItem::SystemError` renders it
    ///    at Technical visibility and §21's rule for that record is unchanged.
    /// 2. `record_upstream_failure` — the classification and the three sentences the CEO
    ///    is about to be shown, made durable so a reload months later shows him what he
    ///    was told rather than what a later build would say.
    /// 3. the live events — the same statement, now.
    /// 4. the retry decision, LAST, because it is the only step that can start more work.
    ///
    /// **The loss statement is built from COUNTS read off the ledger at this instant**, so
    /// it cannot claim something survived that did not. `lease_context_lost` is `true`
    /// unconditionally and that is a fact about this failure class rather than a
    /// convenience: whatever the session had worked out and not yet said is gone with it.
    fn finish_upstream_failure(
        &mut self,
        turn_id: &str,
        binding: &ThreadBinding,
        original_text: &str,
        failure: crate::upstream::UpstreamFailure,
        allow_recovery: bool,
    ) -> Result<(), SpineError> {
        let thread_id = binding.thread_id().to_string();
        eprintln!("[richos] {}", failure.summary());

        let loss = self.turn_loss(turn_id);

        // CHARGED BEFORE ANYTHING IS EMITTED, so the sentence the CEO reads carries the
        // attempt that just failed rather than the one before it.
        let may_retry = self.upstream_budget.charge(failure.fault);
        let record = crate::upstream::UpstreamRecord::new(&failure, &loss, &self.upstream_budget);

        self.ledger.interrupt_turn(turn_id, &failure.summary())?;
        self.ledger.record_upstream_failure(turn_id, &record)?;

        // ONE statement, in the order a person needs it: what happened, what survived,
        // what was spent. `retry_message` already contains the fault's own sentence, so it
        // replaces the first clause rather than repeating it.
        let statement = match &record.retry_message {
            Some(spent) => format!("{spent} {}", record.loss_message),
            None => format!("{} {}", record.ceo_message, record.loss_message),
        };
        self.emit(StreamEvent::TurnError {
            thread_id: thread_id.clone(),
            turn_id: turn_id.to_string(),
            reason: statement,
            at: now_millis(),
        });

        // AN OVERLOAD WAITS OUT HIS SCHEDULE AND RETRIES ON THE SAME SESSION (CEO, 2026-10-07):
        // 1, 2, 5, 10, 20, 40 and 80 minutes before retries 1 to 7. Bounded by the budget alone,
        // so it does not depend on `allow_recovery`, and it needs no lease factory: the lease
        // that met the `529` is kept and asked again, so its context is not lost.
        if may_retry && failure.fault == crate::upstream::UpstreamFault::Overloaded {
            let wait = self.upstream_budget.wait_before_retry().unwrap_or_default();
            self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Recovering, None));
            return self.retry_after_overload(turn_id, binding, original_text, wait);
        }
        let will_recover = may_retry && allow_recovery && self.lease_factory.is_some();
        if will_recover {
            self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Recovering, None));
            return self.recover_and_replay(turn_id, binding, original_text, "mid-turn-crash");
        }
        self.emit_live(self.turn_status_event(binding, turn_id, TurnStatus::Failed, None));
        self.emit_live(self.thread_summary_event(binding, turn_id, ThreadStatus::Failed));
        Err(SpineError::Cognition(CognitionError::Protocol(failure.summary())))
    }

    /// **The overload retry: wait, then ask the SAME lease again** (CEO, 2026-10-07).
    ///
    /// The replay turn is journaled first and published as the running turn, so his Stop (and
    /// a quit, which stops the running turn) reaches it during the wait: the wait ends at once
    /// and `deliver` then finds the stop before the prompt and ends the turn as stopped. The
    /// failed turn is superseded exactly as `recover_and_replay` supersedes one; unlike it, the
    /// lease is NOT cleared, so the session keeps what it had worked out.
    fn retry_after_overload(
        &mut self,
        failed_turn_id: &str,
        binding: &ThreadBinding,
        original_text: &str,
        wait: std::time::Duration,
    ) -> Result<(), SpineError> {
        let original_channel = self.input_channels.get(failed_turn_id).cloned()
            .or_else(|| self.ledger.turn(failed_turn_id).and_then(|t| t.channel.clone()));
        // A pressed button's words stay pressed through the replay (finding 14).
        let pressed = self.ledger.turn(failed_turn_id).is_some_and(|t| t.pressed);
        let retry_turn_id = match original_channel.clone().filter(|_| self.keep_intake_channel) {
            _ if pressed => self.ledger.record_prompt_received_pressed(binding, original_text, Source::Text)?,
            Some(channel) => self.record_prompt(binding, original_text, Source::Text, None, None, &channel)?,
            None => self.ledger.record_prompt_received(binding, original_text, Source::Text)?,
        };
        if let Some(channel) = original_channel { self.input_channels.insert(retry_turn_id.clone(), channel); }
        self.ledger.mark_turn_superseded(failed_turn_id, &retry_turn_id)?;
        self.emit_live(self.turn_status_event(
            binding, &retry_turn_id, TurnStatus::Queued, Some(failed_turn_id.to_string()),
        ));
        self.control.begin_turn(ActiveTurn {
            turn_id: retry_turn_id.clone(),
            thread_id: binding.thread_id().to_string(),
            entity_id: Some(binding.entity_id().clone()),
            started_at: None,
        });
        eprintln!("[richos] upstream overloaded: retrying in {} s on the same session", wait.as_secs());
        let control = self.control.clone();
        let stop_for = retry_turn_id.clone();
        self.retry_clock.wait(wait, &|| control.stop_claim_for(&stop_for).is_some());
        self.deliver(&retry_turn_id, binding, original_text, true)
    }

    fn recover_and_replay(
        &mut self,
        failed_turn_id: &str,
        binding: &ThreadBinding,
        original_text: &str,
        reason: &str,
    ) -> Result<(), SpineError> {
        if self.lease_factory.is_none() { return Err(SpineError::NoLeaseFactory); }
        let recovery_action = self.ledger.record_action_with(
            None, "crash_recovery", &format!("replay interrupted turn {failed_turn_id}"),
            ActionVisibility::Internal, ActionStatus::Claimed,
        )?;
        let from_session = self.lease_session_id().unwrap_or("crashed").to_string();
        // A replay re-issues HIS words, so on an install that keeps the mouth it keeps the
        // original's; one that was never recorded stays unrecorded rather than becoming the
        // desk's (operator back-end spec r3 (s)).
        let original_channel = self.input_channels.get(failed_turn_id).cloned()
            .or_else(|| self.ledger.turn(failed_turn_id).and_then(|t| t.channel.clone()));
        // A pressed button's words stay pressed through the replay (finding 14).
        let pressed = self.ledger.turn(failed_turn_id).is_some_and(|t| t.pressed);
        let replay_turn_id = match original_channel.clone().filter(|_| self.keep_intake_channel) {
            _ if pressed => self.ledger.record_prompt_received_pressed(binding, original_text, Source::Text)?,
            Some(channel) => self.record_prompt(binding, original_text, Source::Text, None, None, &channel)?,
            None => self.ledger.record_prompt_received(binding, original_text, Source::Text)?,
        };
        if let Some(channel) = original_channel { self.input_channels.insert(replay_turn_id.clone(), channel); }
        self.ledger.mark_turn_superseded(failed_turn_id, &replay_turn_id)?;
        self.emit_live(self.turn_status_event(
            binding, &replay_turn_id, TurnStatus::Queued, Some(failed_turn_id.to_string()),
        ));
        // Retire the old child without asking it to finish another roundtrip. Delivery
        // covers spawning, priming and the replay with one cancellable request lifecycle.
        self.clear_lease();
        let outcome = self.deliver(&replay_turn_id, binding, original_text, false);
        if let Some(to_session) = self.lease_session_id().map(str::to_string) {
            self.ledger.record_rotation(&from_session, &to_session, reason)?;
            self.rotation_count += 1;
            self.last_rotation_reason = Some(reason.to_string());
        }
        self.ledger.update_action(
            &recovery_action,
            if outcome.is_ok() { ActionStatus::Completed } else { ActionStatus::Failed },
        )?;
        outcome
    }

    /// Rotate during the next request's preparation, before its user prompt is sent.
    /// The request owns cancellation while all handoff/priming text stays internal.
    fn rotate_lease(&mut self, binding: &ThreadBinding, reason: &str) -> Result<(), SpineError> {
        if self.lease_factory.is_none() {
            return Err(SpineError::NoLeaseFactory);
        }
        let thread_id = binding.thread_id();

        // CLAIM-THEN-EXECUTE (§6.4), Internal visibility. Claimed at the very TOP so the
        // claim covers the whole operation (handoff-summary ask -> spawn -> re-prime ->
        // swap); a crash anywhere inside leaves a durable claim. These internal
        // actions retain no CEO-turn association even though the accepted request
        // owns their progress and cancellation during preparation.
        let rotation_action = self.ledger.record_action_with(
            None,
            "session_rotation",
            &format!("reason={reason}; thread={thread_id}"),
            ActionVisibility::Internal,
            ActionStatus::Claimed,
        )?;

        // Step 1: ask the OUTGOING session for a self-authored handoff summary — one
        // cheap internal turn, never rendered (§2.4). Best-effort: a failure here is NOT
        // fatal to rotation (the deterministic structured digest in reprime.rs is the
        // crash-safe floor either way), so errors are swallowed, not propagated.
        // A pending rotation can be consumed after switching companies. Summarize
        // only the outgoing lease's last primed thread, never the new target's thread.
        let outgoing = self.lease_primed_thread.as_ref()
            .and_then(|id| self.ledger.thread_binding(id).ok());
        if let Some(outgoing) = outgoing {
            match self.request_handoff_summary(&outgoing) {
                Ok(summary) if !summary.trim().is_empty() => {
                    self.ledger.record_handoff_summary(outgoing.thread_id(), &summary)?;
                }
                Err(error) if self.control.stop_claim().is_some() => {
                    self.ledger.update_action(&rotation_action, ActionStatus::Failed)?;
                    return Err(error);
                }
                _ => {}
            }
        }
        if self.control.stop_claim().is_some() {
            self.ledger.update_action(&rotation_action, ActionStatus::Failed)?;
            return Err(CognitionError::PrimingStopped(crate::native::STOP_REASON_CANCELLED.into()).into());
        }

        // Step 4: spawn the fresh child BEFORE tearing down the old one, so a spawn
        // failure leaves the CEO on the still-working outgoing lease instead of
        // stranding the conversation lease-less.
        let spawned = self.lease_factory.as_ref().unwrap().spawn_scoped(binding, &self.control);
        let mut fresh = match spawned {
            Ok(f) => f,
            Err(e) => {
                // The CEO stays on the still-working outgoing lease (see the comment
                // above); the failed rotation is now durable rather than invisible.
                self.ledger.update_action(&rotation_action, ActionStatus::Failed)?;
                return Err(e.into());
            }
        };
        let from_session = self.lease.as_ref().map(|l| l.session_id().to_string()).unwrap_or_default();
        let to_session = fresh.session_id().to_string();

        // Step 3: assemble the re-prime payload (Tiers A/B from the ledger; Tier C from
        // the optional loro seam, degrading gracefully when absent).
        // The worker section is scoped to the session THIS payload is being built for —
        // the OUTGOING lease on a rotation, which is the session whose workers the
        // conversation so far actually belongs to. Never a directory picked by mtime.
        let mut payload =
            RePrimePayload::assemble_for_priming(&self.ledger, binding, DEFAULT_TAIL_TURNS, self.lease_session_id())?;
        if let Some(workers) = self.lease.as_ref().and_then(|lease| lease.worker_status()) {
            payload.worker_state_unknown = workers.unattributed;
            payload.worker_state = workers.priming_lines();
        }
        self.fill_loro_tier(&mut payload, binding);
        let mut priming = payload.to_priming_prompt();
        // Preserve the company material and interview offer in the conversation.
        let onboarding_block = self.company_block(binding);
        if let Some(block) = &onboarding_block { priming.push_str(block); }
        // Durable but NEVER rendered — same Internal-turn discipline as first-attach priming.
        let _ = self.ledger.record_prompt_received(binding, "[re-prime:rotation]", Source::Internal);

        // Step 5: inject the re-prime payload as an internal priming turn — itself a
        // recorded (Internal) action, because "the successor WAS primed" is the single
        // fact the whole anti-false-attribution guarantee rests on. `priming_chars` is
        // measured (`priming.len()`), never asserted.
        let reprime_action = self.ledger.record_action_with(
            None,
            "session_reprime",
            &format!(
                "thread={thread_id}; session={to_session}; priming_chars={}; ceo_facing_actions={}",
                priming.len(),
                payload.action_ledger_digest.len()
            ),
            ActionVisibility::Internal,
            ActionStatus::Claimed,
        )?;
        // The successor's priming machinery, like the first-attach path's: `internal:
        // true`, `turn_id: None`, retained for debugging and NEVER rendered (§1.5). A
        // rotation must stay invisible to the CEO, and that includes its machinery.
        let journal = self.machinery_journal.as_ref();
        let machinery_observer = self.machinery_observer.as_deref();
        let mut on_item = |item: TurnItem| {
            if let TurnItem::Machinery(record) = item {
                let record = record.stamp(thread_id, None, true);
                Self::retain_and_emit_machinery(journal, machinery_observer, record);
            }
        };
        let scoped = match self.onboarding_tool_scope(binding) {
            Some(scope) => fresh.set_onboarding_scope(binding.entity_id(), &scope.central_root, &scope.record_path),
            None => Ok(()),
        };
        // Publish the successor's Stop handle before any priming request.
        self.control.set_cancel(fresh.cancel_handle());
        let primed = if self.control.stop_claim().is_some() {
            Err(CognitionError::PrimingStopped(crate::native::STOP_REASON_CANCELLED.into()))
        } else {
            scoped.and_then(|_| fresh.reprime(&priming, &mut on_item))
        };
        if let Err(e) = primed {
            self.control.set_cancel(self.lease.as_ref().and_then(|lease| lease.cancel_handle()));
            self.ledger.update_action(&reprime_action, ActionStatus::Failed)?;
            self.ledger.update_action(&rotation_action, ActionStatus::Failed)?;
            return Err(e.into());
        }
        self.ledger.update_action(&reprime_action, ActionStatus::Completed)?;

        // Steps 6/7: swap. The OLD lease (replaced here) is dropped — its `Drop` impl
        // (see `native::NativeClient`) kills + waits on the child process, so exactly one live
        // session exists at any instant ("serialize" — §3.3 step 6). The CEO's next
        // prompt (queued or freshly typed) lands on the already-primed successor.
        // `install_lease` republishes the cancel seam AND the session id in one place:
        // between the swap and those two calls the control would still describe the dead
        // lease, and a stop in that window would be written to a child that no longer
        // exists while the worker path read the retired session's team directory.
        self.install_lease(fresh);
        if let Some(active) = self.control.active_turn() {
            self.ledger.mark_turn_started(&active.turn_id, &to_session)?;
        }
        // The SUCCESSOR's own session-start and post-priming traffic, drained off the newly
        // installed lease and stamped `internal: true`. A fresh `NativeClient` starts with an
        // empty `last_session_meta` slot, so it re-announces its commands — and that
        // re-announcement, appearing mid-conversation, is exactly the shape of a rotation
        // tell. It never renders.
        self.pump_between_turn_stamped(binding, true);
        self.lease_primed_thread = Some(binding.thread_id().to_string());
        self.onboarding_primed_block = onboarding_block;
        self.lease_primed = true; // already primed above — deliver() won't re-prime redundantly
        self.context_chars = priming.len(); // reset the watermark baseline to the new payload

        self.ledger.record_rotation(&from_session, &to_session, reason)?;
        self.ledger.update_action(&rotation_action, ActionStatus::Completed)?;
        self.rotation_count += 1;
        self.last_rotation_reason = Some(reason.to_string());
        Ok(())
    }

    /// Ask the CURRENT (outgoing) lease to summarize the conversation for its successor
    /// (continuity §2.4) — ONE cheap internal turn, journaled + streamed through the
    /// SAME durable machinery as a normal turn (so a crash mid-summary is still
    /// crash-safe) but as `Source::Internal`, so it has NO render path (`messages()`
    /// excludes it) exactly like re-prime injection.
    fn request_handoff_summary(&mut self, binding: &ThreadBinding) -> Result<String, SpineError> {
        let thread_id = binding.thread_id();
        const HANDOFF_PROMPT: &str = "[INTERNAL — do not mention this message] Before you're \
            rotated to a successor, summarize this conversation so far in a few sentences: \
            topics covered, decisions reached, commitments you made to the CEO. Reply with \
            ONLY the summary, nothing else.";
        let turn_id = self.ledger.record_prompt_received(binding, HANDOFF_PROMPT, Source::Internal)?;

        // Disjoint field borrows — same pattern as `deliver()`.
        let ledger = &mut self.ledger;
        let journal = self.machinery_journal.as_ref();
        let machinery_observer = self.machinery_observer.as_deref();
        let lease = self.lease.as_mut().ok_or(SpineError::NoLease)?;
        let result = {
            let mut on_item = |item: TurnItem| match item {
                TurnItem::Text { seq, text: c } => {
                    let _ = ledger.append_assistant_delta(&turn_id, c, seq);
                }
                // Rotation machinery: retained for debugging, `internal: true`, NEVER in a
                // thread render (§1.5). The CEO must never see that a rotation happened.
                TurnItem::Machinery(record) => {
                    let record = record.stamp(thread_id, Some(&turn_id), true);
                    Self::retain_and_emit_machinery(journal, machinery_observer, record);
                }
                TurnItem::AnswerComplete => {}
            };
            lease.prompt_context_only(HANDOFF_PROMPT, &mut on_item)
        };

        match result {
            Ok(stop_reason) if stop_reason == "end_turn" => {
                self.ledger.complete_turn(&turn_id, &stop_reason)?;
            }
            Ok(stop_reason) => {
                self.ledger.interrupt_turn(&turn_id, &stop_reason)?;
                return Err(CognitionError::PrimingStopped(stop_reason).into());
            }
            Err(e) => {
                self.ledger.interrupt_turn(&turn_id, &e.to_string())?;
                return Err(e.into());
            }
        }
        // The handoff turn's residue, stamped `internal: true` for the same reason the turn
        // itself is: the CEO must never see that a rotation happened, and that includes
        // whatever the outgoing lease says on its way out (§1.5).
        self.pump_between_turn_stamped(binding, true);
        Ok(self.ledger.turn(&turn_id).map(|t| t.assistant_text.clone()).unwrap_or_default())
    }

    /// Get the chair ready to take a turn for `binding`.
    ///
    /// **Returns whether a CHILD WAS STARTED**, which is not the same question as "was the chair
    /// empty" and stopped being answerable from outside on the day a spare could fill an empty
    /// chair without starting anything. Only this function knows: it is the one place a
    /// conversation lease is spawned outside rotation, and rotation reports through it too.
    /// [`Spine::prime_front_desk`] prints the answer into `app.log` through the shell, so the
    /// value has to come from the code that did the thing rather than from a proxy for it.
    /// The quota service, when the lease factory knows the accounts (fill-first).
    fn quota_service(&self) -> Option<std::sync::Arc<crate::quota::Service>> {
        self.lease_factory.as_ref().and_then(|factory| factory.quota())
    }

    /// **Is the lease in the chair on a model his setting no longer names?** (§119, and the
    /// CEO's addendum: *"Closing the preferences panel IS the confirmation that the settings
    /// chosen there now need to be saved and implemented."*) The choice is saved when
    /// Preferences closes; this is the implementing half, asked before every turn, so his next
    /// reply comes from the model he chose. Never inside a turn (continuity §3.1). A lease or a
    /// factory that cannot say never moves, and neither does a lease with a command it started
    /// still running or unreadable, by the same rule as [`Self::account_switch_due`].
    fn model_switch_due(&self) -> bool {
        use crate::lease_commands::CommandReading;
        let Some(wanted) = self.lease_factory.as_ref().and_then(|f| f.lease_models()) else { return false };
        let Some(lease) = self.lease.as_ref() else { return false };
        let Some(running_on) = lease.lease_model() else { return false };
        if running_on == wanted.rich {
            return false;
        }
        if matches!(lease.running_commands(), Some(CommandReading::Running(_) | CommandReading::Unreadable)) {
            eprintln!("[richos] front desk: its model changed in Preferences; the switch waits while a command it started may still be running");
            return false;
        }
        true
    }

    /// **Must the lease in the chair move to another account before this turn?** Hands the
    /// lease's own streamed reading to the quota service, which decides on the freshest
    /// readings and the measured speed (`claude_accounts.rs`), and answers whether the account
    /// now in use differs from the lease's. A lease that cannot say its account never moves.
    ///
    /// **Nor does one with a command it started still running, or unreadable** (hunt part 1
    /// v3, finding 50): retiring the lease would end that command, and a switch happens only
    /// at a boundary where nothing is running. The check is asked again at the next turn, as
    /// the work host's is (`work_host.rs`, `account_switch_due`); the limit backstop covers a
    /// turn that runs out meanwhile.
    fn account_switch_due(&mut self) -> bool {
        use crate::lease_commands::CommandReading;
        let Some(quota) = self.quota_service() else { return false };
        let Some(lease) = self.lease.as_ref() else { return false };
        let Some(account) = lease.account().map(str::to_string) else { return false };
        if quota.before_turn(&account, lease.streamed_usage()) == account {
            return false;
        }
        if matches!(lease.running_commands(), Some(CommandReading::Running(_) | CommandReading::Unreadable)) {
            eprintln!("[richos] front desk: its Claude account is to be left; the switch waits while a command it started may still be running");
            return false;
        }
        true
    }

    /// Report account changes and work that is actually held. Routine meter speed,
    /// polling intervals and moving thresholds stay in the account settings view.
    pub fn raise_quota_notices(&mut self, thread_id: Option<&str>) {
        let Some(quota) = self.quota_service() else { return };
        let thread = match thread_id {
            Some(thread) => thread.to_string(),
            None => match self.active.as_ref() {
                Some(binding) => binding.thread_id().to_string(),
                None => return,
            },
        };
        // The kind, when the line is about accounts (round 18): its turn is recorded, so the
        // conversation draws the buttons round 18 puts under it (See your accounts, Add a
        // second account, Not now).
        let mut pending = vec![
            (AttentionTier::Digest, Some("switched".to_string()), quota.accounts.take_notice()),
        ];
        pending.extend(quota.take_notes().into_iter().map(|note| (AttentionTier::Digest, None, Some(note))));
        pending.extend(quota.take_account_notes().into_iter().map(|(kind, note)| (AttentionTier::Digest, Some(kind), Some(note))));
        for (tier, kind, text) in pending {
            if let Some(text) = text {
                match self.raise_proactive(Some(&thread), tier, &text) {
                    Ok(turn) => if let Some(kind) = kind {
                        if let Err(error) = quota.accounts.noted(&kind, &turn) {
                            eprintln!("[richos] claude accounts: a line's buttons could not be recorded ({error})");
                        }
                    },
                    Err(error) => eprintln!("[richos] claude accounts: a notice could not be written ({error})"),
                }
            }
        }
    }

    fn prepare_request(&mut self, binding: &ThreadBinding) -> Result<bool, SpineError> {
        let mut spawned = false;
        // **THE DESK THAT IS ALREADY THIS THREAD'S TAKES THE CHAIR FIRST.**
        //
        // The isolation branch below has resumed parked desks since residency landed, but it
        // can only fire when a lease is IN the chair and primed for some OTHER thread — which
        // was every case that could produce a resident, because parking was the only way into
        // the map. `create_thread` now files an adopted spare there, and it does so in the two
        // states that branch never reaches: an empty chair (first thread after launch) and a
        // chair holding an un-primed lease. Without this, the spare would be left in the map
        // and a second child spawned and primed beside it — the exact cost this is here to
        // remove, paid twice.
        if self.lease_primed_thread.as_deref() != Some(binding.thread_id())
            && self.resident.contains_key(binding.thread_id())
        {
            self.park_current_front_desk();
            self.resume_front_desk(binding.thread_id());
        }
        if self.lease_primed_thread.as_deref().map(|thread| thread != binding.thread_id()).unwrap_or(false)
            && self.lease.as_ref().map(|lease| lease.requires_thread_isolation()).unwrap_or(false) {
            // Never carry one entity's provider context or ECS session into another thread.
            //
            // **THE ISOLATION IS UNCHANGED AND THE TEARDOWN IS GONE.** This used to be
            // `clear_lease()`, which killed the outgoing child; a thread he came back to got
            // a brand-new session re-primed from the ledger tail. The CEO's Two Riches page
            // says *"each conversation thread always holds one front desk Rich"* and
            // *"each conversation thread in the app can go forever"* — residence, not
            // reconstitution. So the outgoing desk is parked alive under its own thread and
            // this thread's own desk, if it has one, takes the chair.
            //
            // Nothing crosses: each desk keeps its own provider session, its own ECS
            // session id and its own priming, and the incoming one is the destination's or
            // it is fresh. What is NOT yet true is simultaneity — two desks cannot hold a
            // turn open at once, because both bind the CEO's single ECS cursor
            // (`native.rs`'s `CONVERSATION_SEAT`, with the measurement beside it). Turns
            // take turns; the work behind them has run in parallel since slice 1.
            self.park_current_front_desk();
            self.resume_front_desk(binding.thread_id());
        }
        // **FILL-FIRST, BEFORE EVERY TURN** (plan §15 answers 6-9, ruling §108). The freshest
        // reading — this lease's own streamed `rate_limit_event` when newer than the probe —
        // and the measured speed decide whether the account in use must be left; when this
        // lease is no longer on the account in use, it rotates HERE, at the boundary, before
        // the prompt is sent. Never inside a turn, and never at 100%.
        if self.pending_rotation_reason.is_none() && self.account_switch_due() {
            self.pending_rotation_reason = Some(ACCOUNT_SWITCH.to_string());
        }
        // §119: the model he chose in Preferences, from the first reply after it closed.
        if self.pending_rotation_reason.is_none() && self.model_switch_due() {
            self.pending_rotation_reason = Some(MODEL_CHANGED.to_string());
        }
        if let Some(reason) = self.pending_rotation_reason.take() {
            // **A WATERMARK RENEWAL WAITS FOR A COMMAND THE LEASE STARTED** (reap gap C6).
            // Retiring the lease ends its tool commands, and a renewal is invisible, so one due
            // only to the watermark is held while the lease's supervisor shows a command
            // running (or cannot say), up to `work_host::ROTATION_WAITS_FOR_COMMANDS`. An
            // explicit or context-critical rotation is not held.
            use crate::lease_commands::CommandReading;
            let commands = self.lease.as_ref().and_then(|lease| lease.running_commands());
            let holding = reason == "context-watermark"
                && matches!(commands, Some(CommandReading::Running(_) | CommandReading::Unreadable));
            let waited = holding.then(|| self.rotation_deferred_since.get_or_insert_with(std::time::Instant::now).elapsed());
            if waited.is_some_and(|waited| waited < crate::work_host::ROTATION_WAITS_FOR_COMMANDS) {
                eprintln!("[richos] front desk: renewal deferred while a command it started is running ({commands:?})");
                self.pending_rotation_reason = Some(reason);
            } else if self.lease.is_some() {
                self.rotation_deferred_since = None;
                self.rotate_lease(binding, &reason)?;
                spawned = true;
            }
        }
        if self.lease.is_none() {
            let factory = self.lease_factory.as_ref().ok_or(SpineError::NoLease)?;
            let fresh = factory.spawn_scoped(binding, &self.control)?;
            self.install_lease(fresh);
            spawned = true;
            if let Some(active) = self.control.active_turn() {
                let session = self.lease_session_id().unwrap().to_string();
                self.ledger.mark_turn_started(&active.turn_id, &session)?;
            }
        }
        // **THE SWITCH NOTICE ONLY ONCE THE SWITCH HAS HAPPENED.** The lease that takes this
        // turn is in the chair now; if it is on the account a switch went to, that switch is
        // real and its notice is said. A switch still waiting on a command this lease started
        // (`account_switch_due`) leaves the turn on the old account and says nothing.
        if let (Some(quota), Some(account)) = (self.quota_service(), self.lease.as_ref().and_then(|l| l.account())) {
            quota.accounts.ran_on(account);
        }
        self.raise_quota_notices(Some(binding.thread_id()));
        if self.control.stop_claim().is_some() { return Ok(spawned); }
        self.prime_lease_if_needed(binding).map(|()| spawned)
    }

    /// **PRIME THE FRONT DESK BEFORE HE TYPES — the CEO's §55, the half that is not about the
    /// reply at all.**
    ///
    /// # The measurement this exists for, and the premise it corrects
    ///
    /// `docs/verification/first-reply-2026-09-18.md` measured the lease's first visible turn at
    /// **13.464 s / 13.628 s / 15.513 s** against **7.128 s / 8.505 s / 8.920 s** warm, and
    /// recorded the ~8 s difference as *"the lease waking up while he waits"*. It is more specific
    /// than that, and the difference decides what can be done about it: **those seconds are a
    /// whole model turn.** [`Spine::prime_lease_if_needed`] runs `Cognition::reprime`, which is a
    /// real turn with the re-prime payload in it, and it is called from
    /// [`Spine::prepare_request`] — which runs INSIDE `submit_prompt`. So his first message pays
    /// for the continuity priming of the lease that is about to answer it, while he watches.
    ///
    /// **So priming cannot be made "a process start only", and that is a finding rather than a
    /// preference.** The child process is already started before his first message reaches the
    /// provider, in the app (`prepare_request` spawns from the lease factory) and in the probe
    /// (`attach_lease`) alike. The seconds are the priming TURN. This method moves that turn off
    /// his wait; it does not invent it.
    ///
    /// # What it spends
    ///
    /// **At most one model turn per lease, and it is the same turn his first message spends
    /// today.** Called at launch, or when a thread is opened, that turn is paid before he is
    /// waiting on it; his first message then finds the desk primed and goes straight to the
    /// provider. The ONE case where this spends something that would not have been spent is a
    /// launch where he never says anything at all — one priming turn on a desk that answered
    /// nothing. Named here rather than discovered on a bill.
    ///
    /// # What it will not do
    ///
    /// - **Never underneath a turn.** [`FrontDeskReady::TurnInProgress`], not a wait: the lease is
    ///   the thing running that turn.
    /// - **Never for a thread that does not exist, and never by creating one.** A caller that
    ///   wants a thread makes one; this makes a thread's desk ready.
    /// - **Nothing about rotation.** It goes through `prepare_request`, so a rotation that was
    ///   already pending is honored exactly as his next message would have honored it, and none is
    ///   ever started on the strength of a launch.
    ///
    /// The caller holds the spine's lock for the whole of it (~8 s, measured). That is deliberate,
    /// and it is never worse than today: a message he sends during priming waits for the priming
    /// his message used to perform itself, and then runs on a primed desk.
    pub fn prime_front_desk(&mut self, thread_id: &str) -> FrontDeskReady {
        if self.turn_in_progress {
            return FrontDeskReady::TurnInProgress;
        }
        let binding = match self.ledger.thread_binding(thread_id) {
            Ok(binding) => binding,
            Err(e) => return FrontDeskReady::NotReady(e.to_string()),
        };
        if self.lease.is_some()
            && self.lease_primed
            && self.lease_primed_thread.as_deref() == Some(thread_id)
        {
            return FrontDeskReady::AlreadyReady;
        }
        // **WHETHER A CHILD WAS STARTED IS `prepare_request`'s ANSWER, NOT A GUESS TAKEN HERE.**
        //
        // This used to read `let spawned = self.lease.is_none();` on this line — which answers
        // "was the chair empty", and that was the same question only while an empty chair could
        // be filled in exactly one way. It no longer can: `prepare_request` now puts an ADOPTED
        // desk in an empty chair (a spare that `create_thread` filed, or a parked resident), and
        // the old reading called that a spawn. The shell prints this flag verbatim into
        // `app.log` as *", including the lease's own start"* — so a wrong value here is a wrong
        // number in a log and then in a record two days later, which is §3 of
        // `first-words-2026-09-18-sendlock.md` happening to the other field of this same struct.
        let started = std::time::Instant::now();
        // ===================================================================================
        // FROM HERE THE SPINE IS SHUT FOR A MODEL TURN, AND HIS SEND MUST NOT WAIT ON IT
        // ===================================================================================
        //
        // The caller holds the spine's mutex for the whole of `prepare_request` below, which
        // is a real model turn (~2.9 s in the probe, 4412 ms in candidate .10's own `app.log`).
        // Until this marker existed, a Send issued inside that window blocked at
        // `state.spine.lock()` in `send_message` for the remainder of it, with the CEO looking
        // at "Sending your message / Waiting for Rich to accept it" the entire time and his
        // sentence living nowhere but the webview — a quit in that window lost it.
        //
        // The marker opens the intake log's road for THIS thread only, and the four lines
        // after the prime close it again and deliver whatever came down it, in order, before
        // this function returns and the mutex is released.
        //
        // **WHAT THIS DOES NOT DO, stated here because the shape invites the opposite
        // reading.** It does not make his first words arrive sooner. The lease is serial —
        // one turn at a time, continuity §3.1 — so his prompt queues behind the priming turn
        // whether it waits on a mutex or on this log; the contended cost is
        // `prime_remainder + turn` either way, and re-priming is not paid twice
        // (`prime_lease_if_needed` returns early on an already-primed thread). What it does
        // is make his sentence DURABLE at the instant he sends it and the Send itself
        // instant. The seconds themselves are the pre-prime's own turn, and the only thing
        // that removes them is a desk that was already primed when the thread opened.
        self.control.begin_front_desk_prime(thread_id);
        let prepared = self.prepare_request(&binding);
        // **THE CLOCK STOPS HERE, BEFORE THE DRAIN, AND THAT IS NOT A DETAIL.** `Ready.millis`
        // is documented as *"what he would otherwise have waited through on his first
        // message"*, and the shell prints it as *"the front desk is ready before he types:
        // {millis} ms"*. Taking it after the lines below would fold HIS OWN TURN into it — run
        // F of `examples/first_reply_timing_e2e.rs` printed `Ready { millis: 11355 }` for a
        // ~4 s prime that had a ~7 s turn drained behind it, and the probe's decomposition came
        // out NEGATIVE, which is how this was found. A number that ends up in `app.log` and
        // then in a record is exactly the kind that has to be right at the source.
        let primed_in = started.elapsed();
        // Shut the road BEFORE draining it, from under the same spine mutex. Every ordering
        // is covered by `TurnControl::defer_send`'s own guard: a send accepted before this
        // line is drained by the next one, and a send that arrives after it blocks on a spine
        // that is about to be free.
        self.control.end_front_desk_prime();
        if let Err(e) = self.poll_intake() {
            // His message is already durable and still pending; it is delivered at the next
            // boundary. Machinery channel only — the CEO is told by the turn itself.
            eprintln!("[richos] a message sent during the front desk's priming could not be delivered yet ({e})");
        }
        match prepared {
            Ok(spawned) => FrontDeskReady::Ready { millis: primed_in.as_millis() as u64, spawned },
            // **A FAILURE HERE IS NOT HIS PROBLEM AND MUST NOT BECOME ONE.** Priming only decides
            // WHEN the waiting happens; if it cannot be done now, his first message takes the path
            // it has always taken and primes on its own. So the reason is said once, on the
            // machinery channel, and nothing is surfaced to him and nothing is retried.
            Err(e) => {
                eprintln!(
                    "[richos] the front desk could not be made ready before he types ({e}); his \
                     first message will prime it the way it did before, which is the only cost"
                );
                FrontDeskReady::NotReady(e.to_string())
            }
        }
    }

    /// Assemble + inject the re-prime payload once per lease (before its first turn).
    fn prime_lease_if_needed(&mut self, binding: &ThreadBinding) -> Result<(), SpineError> {
        let onboarding_block = self.company_block(binding);
        if onboarding_block != self.onboarding_primed_block { self.lease_primed = false; }
        if (self.lease_primed && self.lease_primed_thread.as_deref() == Some(binding.thread_id())) || self.lease.is_none() {
            return Ok(());
        }
        let thread_id = binding.thread_id();
        // The worker section is scoped to the session THIS payload is being built for —
        // the OUTGOING lease on a rotation, which is the session whose workers the
        // conversation so far actually belongs to. Never a directory picked by mtime.
        let mut payload =
            RePrimePayload::assemble_for_priming(&self.ledger, binding, DEFAULT_TAIL_TURNS, self.lease_session_id())?;
        if let Some(workers) = self.lease.as_ref().and_then(|lease| lease.worker_status()) {
            payload.worker_state_unknown = workers.unattributed;
            payload.worker_state = workers.priming_lines();
        }
        self.fill_loro_tier(&mut payload, binding);
        let mut priming = payload.to_priming_prompt();
        // Preserve the company material and interview offer in the conversation.
        if let Some(block) = &onboarding_block { priming.push_str(block); }
        // Record the priming as an Internal turn so it is durable but NEVER rendered.
        let reprime_turn = self.ledger.record_prompt_received(binding, "[re-prime]", Source::Internal);
        // ... and as an Internal ACTION, claim-then-execute: this is the first-attach
        // priming path (boot, or a lease attached with no factory), the counterpart of
        // the rotation path's own `session_reprime` record.
        let session = self.lease.as_ref().map(|l| l.session_id().to_string()).unwrap_or_default();
        let reprime_action = self.ledger.record_action_with(
            None,
            "session_reprime",
            &format!(
                "thread={thread_id}; session={session}; priming_chars={}; ceo_facing_actions={}",
                priming.len(),
                payload.action_ledger_digest.len()
            ),
            ActionVisibility::Internal,
            ActionStatus::Claimed,
        )?;
        // §1.5: re-prime runs a real turn, so its machinery now flows. `internal: true`,
        // `turn_id: None` — attached to the THREAD, not to a turn, because there is no
        // CEO turn here to attach it to (§1.4 G4: `turn_id: None` is a first-class state,
        // not a bug). Retained for debugging; never rendered.
        let scoped = match self.onboarding_tool_scope(binding) {
            Some(scope) => match self.lease.as_mut() {
                Some(lease) => lease.set_onboarding_scope(binding.entity_id(), &scope.central_root, &scope.record_path),
                None => Ok(()),
            },
            None => Ok(()),
        };
        let journal = self.machinery_journal.as_ref();
        let machinery_observer = self.machinery_observer.as_deref();
        let primed = match self.lease.as_mut() {
            Some(lease) => {
                let mut on_item = |item: TurnItem| {
                    if let TurnItem::Machinery(record) = item {
                        let record = record.stamp(thread_id, None, true);
                        Self::retain_and_emit_machinery(journal, machinery_observer, record);
                    }
                    // Priming TEXT is discarded exactly as before — never rendered.
                };
                scoped.and_then(|_| lease.reprime(&priming, &mut on_item))
            }
            None => Ok(()),
        };
        if let Err(e) = primed {
            self.ledger.update_action(&reprime_action, ActionStatus::Failed)?;
            return Err(e.into());
        }
        self.ledger.update_action(&reprime_action, ActionStatus::Completed)?;
        // **AND THE CONTEXT BRIEF HIS FIRST TURN WOULD OTHERWISE SEND FIRST** (CEO 2026-10-14:
        // "From barge in to a 10-second delay now?"). `prepare_work_turn` gives a session its
        // continuity brief as a hidden model turn before his words unless the session already
        // holds it, and a freshly primed one never did — so his first turn after every launch
        // and thread open paid a whole round trip before his words left. Supplied here under
        // this priming turn's own ledger id, it is held when he speaks. An optimization of WHEN:
        // a failure is said on the machinery channel and his turn supplies it as before.
        if let (Ok(turn), Some(lease)) = (reprime_turn.as_ref(), self.lease.as_mut()) {
            let journal = self.machinery_journal.as_ref();
            let machinery_observer = self.machinery_observer.as_deref();
            let mut on_item = |item: TurnItem| {
                if let TurnItem::Machinery(record) = item {
                    Self::retain_and_emit_machinery(journal, machinery_observer, record.stamp(thread_id, None, true));
                }
            };
            if let Err(e) = lease.supply_context_ahead(binding, turn, &mut on_item) {
                eprintln!("[richos] the context brief could not be supplied before he speaks ({e}); his turn supplies it");
            }
        }
        // The priming turn's RESIDUE. Whatever the lease says once the priming response has
        // returned lands in the between-turn lane with no turn to own it, so it is drained
        // HERE and stamped `internal: true` — the aftermath of a turn the CEO never saw is
        // as unrenderable as the turn itself (§1.5, and the standing order that Rich never
        // reveals or references session rotation).
        self.pump_between_turn_stamped(binding, true);
        self.onboarding_primed_block = onboarding_block;
        self.lease_primed_thread = Some(binding.thread_id().to_string());
        self.lease_primed = true;
        self.context_chars = priming.len(); // baseline the watermark measurement
        Ok(())
    }

    pub fn queue_depth(&self) -> usize {
        self.queue.len()
    }

    pub fn is_turn_in_progress(&self) -> bool {
        self.turn_in_progress
    }

    /// TEST-ONLY seam. This spine is fully synchronous and single-threaded (the Tauri
    /// shell serializes every call behind one `Mutex<Spine>` — see the module doc), so
    /// genuine "a message arrives WHILE a turn is in flight" concurrency never occurs
    /// in-process during a test. This lets an integration test force that state
    /// deterministically to exercise the proactive seam's deferred-emit branch (§ raise_proactive)
    /// without spinning up real threads. NOT part of the spine's real API contract.
    #[doc(hidden)]
    pub fn debug_set_turn_in_progress(&mut self, in_progress: bool) {
        self.turn_in_progress = in_progress;
    }
}
