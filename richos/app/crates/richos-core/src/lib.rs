//! richos-core — the RichOS runtime SPINE.
//!
//! The load-bearing leg of the RichOS front-end (the front-end architecture,
//! the front-end architecture plan, 2026-08-24, Phase 1). UI-agnostic and
//! native-dependency-free so it builds + tests fast and the Tauri shell (src-tauri/)
//! is a thin consumer.
//!
//! Pieces:
//!   - `entity`    — the ENTITY scope/privacy boundary: entity ids, the registry, the
//!                   immutable thread binding and the fail-closed root resolver (ECS §3.2–3.4).
//!   - `ledger`    — durable, append-only conversation + action LEDGER (crash-safe).
//!   - `thread`    — topic threads as VIEWS over the one shared ledger.
//!   - `cognition` — the swappable compute-lease seam (+ a test mock).
//!   - `native`    — the real compute-lease client: RichOS drives the NATIVE `claude`
//!                   binary over its stream-json stdio. No ACP adapter, no Node, no npm
//!                   anywhere under `app/` (CEO ruling, `wiki/ceo-decisions.md` §16).
//!   - `reprime`   — the session-continuity re-prime payload (foundation).
//!   - `loro`      — the Tier-C READ seam, implemented: company memory compiled into a
//!                   re-prime, with the cross-entity lane re-assertion on the finished
//!                   slice — and the PROVENANCE of what was injected, retained so a later
//!                   correction can name the record it is a correction of.
//!   - `correction` — the loro WRITE loop: propose, ASK the CEO, then write. Never the
//!                   other order — ceo-decisions.md §7, enforced by the state machine.
//!   - `belief`    — the loro desk's PROPOSER: what makes an utterance a correction of a
//!                   recorded BELIEF, and — the load-bearing half — which record it is a
//!                   correction of. Resolves or stays silent; never guesses a ref.
//!   - `spoken`    — the flywheel's AUTOMATIC TRIGGER: what makes an utterance a
//!                   correction, decided from a repair frame + the shipped §7 term gate.
//!                   Detects; never writes.
//!   - `staging`   — where a detected spoken correction LANDS: durable candidates and
//!                   §7's three outcomes. The only path to a vocabulary write is a human
//!                   answer; there is no threshold that reaches one.
//!   - `skip`     — ONE vocabulary for a record on disk this build could not read,
//!                   shared by the ledger and the intake log so they cannot drift apart
//!                   about what is damage and what was written by a newer RichOS.
//!   - `steering`  — the CEO's two mid-turn controls (UX §9.2/§9.3): the durable intake
//!                   log and the cancel seam, both reachable WITHOUT the spine lock.
//!   - `stream`    — the live, UI-facing turn events (streaming deltas + turn state).
//!   - `live`      — the ADDITIVE live-work event family (UX brief §13): typed turn status,
//!                   message phase, and semantic activity, beside `stream` — never replacing it.
//!   - `machinery` — the SECOND event family: every non-text agent frame, routed not dropped.
//!   - `journal`   — the per-thread, day-sharded machinery journal (separate store; not the ledger).
//!   - `timeline`  — the TYPED TIMELINE records (UX brief §12): the projection a renderer
//!                   reads, with entity scope on every item and visibility as a gate.
//!   - `upstream`  — the UPSTREAM MODEL-API failure vocabulary: `529` overload vs `429`
//!                   quota, classified from the vendor's own structural tokens, with the
//!                   bounded+visible retry budget and the what-survived statement.
//!                   Row 3.30, measured 2026-09-03.
//!   - `spine`     — ties it together: queue-not-interrupt, turn-boundary, re-prime seam,
//!                   turn-boundary rotation, mid-turn-crash recovery, the proactive seam.
//!   - `config`    — durable CEO-facing preferences: company name, the assertiveness dial.
//!   - `worker_events` — the CONSUMER of the engine's worker-lifecycle stream: the four
//!                     states it can witness, and the three it refuses to invent.
//!   - `worker_status` — the optional AI-worker drill-down, read from the engine's event logs.
//!   - `feedback`  — the in-app feedback channel's LOCAL half: the rating prompt, its
//!                   on-disk persistence, and the VERSIONED CLOSED VOCABULARY a report is
//!                   assembled from — a payload with no free-text field anywhere in it, so
//!                   the user's specifics are unrepresentable rather than filtered out.
//!                   Nothing in it sends anything, and its tests assert that.

/// American spelling, fixed mechanically in every reply and document the app gives him (CEO
/// §93, 2026-09-27): a compiled-in table, context rules, and a streaming hold-back.
pub mod american_spelling;
/// Where a conversation's attached files live, named once for the desk that writes them and
/// the session that reads them (CEO §86, 2026-09-24).
pub mod attachments;
pub mod belief;
/// Bust a bug (CEO §115, round 21): what is private and its stand-ins, Rich's write-up, the
/// issue word for word, and the reports on this Mac that wait to be sent. Opens no connection.
pub mod bug_report;
pub mod claude_accounts;
pub mod cognition;
pub mod company;
/// A worker's turn that a lost internet connection ended waits for the connection and carries
/// on (CEO, 2026-10-21).
pub mod connection;
pub mod correction;
pub mod config;
pub mod doctrine;
pub mod entity;
pub mod ecs;
pub mod evidence;
pub mod feedback;
pub mod home_field;
pub mod journal;
pub mod heard;
/// Why a turn ended without finishing — the sibling of [`upstream`] for the local half.
pub mod interruption;
pub mod launch;
pub mod ledger;
pub mod loro;
pub mod live;
pub mod machinery;
/// yt-dlp on the user's Mac, installed into the app's tools folder and kept at the newest
/// `nightly` release (media-tools plan slice 2).
pub mod media_tools;
pub mod machine_settings;
pub mod native;
/// The output record: every file a thread produced, written by witnesses (Output side panel
/// PRD §3-§4, richos-hq `docs/prds/2026-10-05-output-side-panel.md`).
pub mod output;
pub mod quota;
/// A recorded real Claude session played back as the provider child, for tests of the real
/// client and work host (T3 idea 2; richos-hq plan 2026-10-09 §4 row 6).
pub mod replay;
pub mod onboarding;
pub mod onboarding_tools;
pub mod provision;
pub mod questions;
pub mod question_work;
pub mod question_engine;
pub mod question_tools;
/// A file Rich or Stu puts in front of the user: the `attach_file` tool and its record (§138,
/// build plan item 12).
pub mod rich_files;
pub mod recovery;
pub mod reprime;
pub mod runtime;
/// **WAITING FOR THE SCREEN** — the CEO's ruling §56 (2026-09-18): *"I like that "Watch for
/// the Mac's screen to unlock" feature that you had running here. Should be added to the
/// RichOS app for automatic use when Rich needs it."* The decision half lives here and the
/// reading is in the shell (`src-tauri/src/screen.rs`), exactly as [`work_gate`] splits the
/// update gate — and its polarity is the INVERSE of that gate's: a reading this build could
/// not establish never blocks, because §56's wait has no timeout and an unbounded wait on
/// nothing is a worse failure than proceeding.
pub mod screen;
pub mod setup;
pub mod skills;
pub mod skip;
/// API-equivalent token spend (§118): every request priced at Claude's published API rates.
pub mod spend;
pub mod spine;
pub mod spoken;
pub mod staging;
pub mod steering;
pub mod stream;
pub mod thread;
pub mod timeline;
pub mod upstream;
pub mod util;
pub mod work_gate;
pub mod worker_events;
pub mod worker_status;

pub use belief::{BeliefAsk, BeliefDetection, BeliefRejection, ValueClass};
pub use heard::{DictationEntry, DictationJournal, HeardMatch, HeardReview, HeardSource};
pub use cognition::{Cognition, CognitionError, LeaseFactory};
pub use config::{Assertiveness, ConfigStore, TechyMode, TechySource};
pub use feedback::{
    render_disclosure, ApprovedReport, ContributingCondition, DiagnosisTerm, Disclosure,
    FailureClass, FeedbackEntry, FeedbackPayload, FeedbackStore, Occurrences, PromptOutcome,
    Rating, ReportDecision, TaxonomyError, TaxonomyVersion, DISCLOSURE_HEADING, PROMPT_OPTIONS,
    PROMPT_QUESTION, REPORT_OFFER, TAXONOMY_VERSION,
};
pub use entity::{
    app_config_dir, display_name_from_id, entity_registry_path, Entity, EntityError, EntityId, EntityRegistry,
    EntityResolveError, EntityStatus, PersonId, RegistryLoad, RegistrySource, ThreadBinding,
    ThreadEntity, ENTITY_REGISTRY_FILENAME, ENTITY_REGISTRY_VERSION, EXAMPLE_ENTITY_REGISTRY_JSON,
};
pub use journal::{MachineryJournal, ThreadMachinery};
pub use ledger::{AttentionTier, Ledger, Message, Source, TextRun, TurnState};
pub use live::{
    EventFence, LiveEvent, LiveObserver, ThreadStatus, TurnStatus, EVENT_ACTIVITY_UPSERTED,
    EVENT_MESSAGE_COMPLETED, EVENT_MESSAGE_DELTA, EVENT_MESSAGE_STARTED, EVENT_THREAD_SUMMARY_UPDATED,
    EVENT_TURN_STATUS, STREAMED_MESSAGE_PHASE,
};
pub use machinery::{MachineryKind, MachineryObserver, MachineryRecord, ToolStatus, EVENT_MACHINERY};
pub use correction::{
    CliLoroWriter, CorrectionDesk, CorrectionError, LoroWriteBackend, Proposal, ProposalObserver,
    ProposalState, ProposedWrite, SharedCorrectionDesk, WriteOutput, EVENT_LORO_PROPOSED,
};
pub use loro::{
    CliContextCompiler, CorpusLanes, InjectedSlice, LaneMap, LoroError, LoroRoot, LoroTools,
    SharedSliceProvenance, Slice, SliceProvenance, SliceRecord,
};
pub use reprime::{LoroContextCompiler, LoroTier, SliceRequest};
pub use spine::{Spine, SpineError, WorkerEventsSource};
pub use steering::{
    ActiveTurn, IntakeHealth, IntakeLog, IntakeRecord, SteeringError, StopClaim, StopOutcome, TurnCancel,
    TurnControl, ConversationControl, KNOWN_INTAKE_TAGS,
};
pub use stream::{StreamEvent, TurnObserver};
pub use screen::{
    FakeScreen, Screen, ScreenReading, ScreenSource, ScreenWatch, UnknownScreen, WaitOutcome,
    SCREEN_POLL,
};
pub use upstream::{
    RetryBudget, RetryClock, SystemRetryClock, TurnLoss, UpstreamFailure, UpstreamFault,
    MAX_OVERLOAD_RETRIES, MAX_UPSTREAM_RETRIES, OVERLOAD_RETRY_WAITS,
};
pub use timeline::{
    ActivityState, ActivityType, RichMessagePhase, Timeline, TimelineBase, TimelineItem, TimelineView, ViewMode,
    Visibility, WorkerActivityItem, WorkerRun, WorkerState, RUN_ENDED_WORKER_STATE,
};
pub use worker_events::{HostLiveness, ObservedWorkerState, OpenRun, SessionScope, WorkerEventRow};
pub use worker_status::WorkerStatusView;

pub mod owned_process;
pub mod lease_commands;
pub mod provider_auth;
pub mod engine_profile;
pub mod user_settings;
pub mod your_instructions;
pub mod instructions_update;
pub mod login_environment;
pub mod user_git;

pub mod repositories;
pub mod work_status;

/// The BACKGROUND-WORK leg (the background-work spec, richos-hq
/// `docs/plans/background-work-spec-2026-09-17.md` revision 5, `9255e71a`):
///   - `assignment` — the durable register of what he asked for. The turn ends at the
///                    receipt this writes (§1), and the notice it holds is what he is told
///                    when he is next listening (§3.4).
///   - `work_host`  — the second compute lease and the actor that owns it (§2.1). It is not
///                    the spine, it never takes the spine's lock, and it is never attached
///                    to the conversation's `TurnControl` (§4.2).
///   - `assignment_tools` — the app-owned MCP endpoint the CONVERSATION calls to write an
///                    assignment down and end its turn with the receipt (§1.1, §7.1).
///   - `status_tools` — the app-owned MCP endpoint the FRONT DESK looks with, and the
///                    other half of the CEO's Two Riches note 3: taking the orchestration
///                    tools away from the conversation leaves it nothing to answer "what is
///                    running" with unless it has a read of its own (Sage's check of that
///                    page, finding 6). Read-only, this conversation only, no call into the
///                    back end.
pub mod app_workers;
pub mod assignment;
pub mod assignment_tools;
pub mod status_tools;
pub mod session_tools;
pub mod work_host;
/// `agent_tasks` — the task book: which of the provider's own background tasks is which and how
/// each last stood, read off the stream, and the `stop_task` frame that stops one of them. The
/// regular app's Stop of one worker and its worker status run on it.
pub mod agent_tasks;
/// `hook_alarms` — the engine's alarms (a hook's `systemMessage`) read off a lease's stream,
/// verbatim, and delivered once across every lease (ruling §138: every user's app runs the
/// engine's full checks, and their alarms reach the user as work notices).
pub mod hook_alarms;
/// `review_watch` — the engine's review-watch run as the HOST's own child (automatic second
/// review, slice 4): mid-job reviews of the app's workers.
pub mod review_watch;
/// `codex_reviews` — the Settings switch "Let Codex review your team's work" (round 20.2, ruling
/// §114): the user's choice, saved where the app's review-watch reads it, and what the Mac reports
/// about Codex (installed, signed in), from Codex's own `login status`.
pub mod codex_reviews;

pub mod read_view;

/// **NOTHING TELLS ANYONE ABOUT DICTATION UNTIL DICTATION IS THERE** (dictation plan revision 2,
/// richos-hq `docs/plans/2026-10-08-dictation-anywhere.md`, minor 2 and section 9: "Nothing
/// user-visible reaches a nightly before all five slices are in"). The ways in, which are the
/// Settings row, Rich's offer and the setup sheet's new lines (round 19, states 1 to 4), are built
/// slice by slice and land on main as each passes; this one constant kept all of them off screen
/// until slice 5's walk passed and set it `true` (2026-10-08). A nightly published from main in
/// between showed exactly what it showed before.
pub const DICTATION_READY: bool = true;

/// The walks' preview of the ways in before [`DICTATION_READY`] is `true`: a test VM walk launches
/// the candidate with this set to `1` (`TESTVM_APP_ENV`), because a slice's screens must be walked
/// on the candidate's own bundle before the constant flips. No person sets it, nothing in the app
/// writes it, and it can only turn the ways in on, never off.
pub const DICTATION_PREVIEW_ENV: &str = "RICHOS_DICTATION_PREVIEW";

/// Whether the dictation ways in are shown in this process: [`DICTATION_READY`], or a walk's
/// [`DICTATION_PREVIEW_ENV`].
pub fn dictation_ready() -> bool {
    dictation_ready_with(std::env::var(DICTATION_PREVIEW_ENV).ok().as_deref())
}

/// [`dictation_ready`] for a stated preview value, so the rule is a unit test.
pub fn dictation_ready_with(preview: Option<&str>) -> bool {
    DICTATION_READY || preview == Some("1")
}

#[cfg(test)]
mod dictation_ready_tests {
    use super::*;

    /// INVARIANT: with no preview the constant alone decides; a walk's preview of exactly `1`
    /// turns the ways in on, and no value can turn them off.
    #[test]
    fn the_ways_in_follow_the_constant_unless_a_walk_previews_them() {
        assert_eq!(dictation_ready_with(None), DICTATION_READY);
        assert_eq!(dictation_ready_with(Some("0")), DICTATION_READY);
        assert_eq!(dictation_ready_with(Some("yes")), DICTATION_READY);
        assert!(dictation_ready_with(Some("1")));
    }
}

pub mod routing;
mod request_classifier;
mod thread_title;
