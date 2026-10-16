//! Session-continuity FOUNDATION — the re-prime payload (P1.4, foundation only).
//!
//! The durable Rich is the APP; a Claude session is a swappable compute lease
//! (continuity design §1.1). When a lease is (re)spawned, the app injects a re-prime
//! payload as an INTERNAL, NON-RENDERED priming turn so the successor resumes as the
//! SAME Rich, mid-conversation — no re-ask, no amnesia, no false attribution.
//!
//! This module builds the payload and renders it to the priming-turn text. Full
//! turn-boundary rotation on a context watermark + self-authored handoff summaries
//! are a LATER leg; the seam is here (Spine::reprime_current_session) so they drop in.

use crate::entity::ThreadBinding;
use crate::ledger::{ActionStatus, Ledger, LedgerError, Source, TurnState};
use crate::worker_status::Unattributed;
use serde::Serialize;

/// What Tier C asks the compiler for. A slice is ALWAYS topical — `CONTEXT-CONTRACT.md`
/// §1: *"there is no compile all of loro"* — so a thread id alone was never a legal
/// request, and the seam used to pass one. §3 says it plainly: pass the CEO's **current
/// intent**, one or two sentences of natural language; a long tail dilutes `coverage`
/// because every incidental word enlarges the denominator, so a squarely-relevant slice
/// gets labelled `adjacent` purely for having been asked verbosely.
///
/// `entity_id` rides along because a slice is also SCOPED (ECS §3.5). What the compiler
/// side does with it is its own business — narrow a lane, or refuse — but it must be told,
/// and it cannot recover the entity from the thread id.
#[derive(Debug, Clone, Copy)]
pub struct SliceRequest<'a> {
    /// The conversation being re-primed. Provenance and logging, not ranking.
    pub thread_id: &'a str,
    /// The entity area this thread is bound to, immutably (ECS §3.4).
    pub entity_id: &'a str,
    /// The CEO's current intent, in his words. Never empty — see [`RePrimePayload::topic`].
    pub topic: &'a str,
    /// Hard cap on the returned text, in CHARACTERS (`CONTEXT-CONTRACT.md` §3: characters,
    /// not tokens, because characters are model-agnostic and exactly checkable).
    pub budget_chars: usize,
}

/// Tier C's FOUR states, and three of them are not "a slice".
///
/// This is the rule the ACTION LEDGER and LIVE WORKER STATE sections of this payload
/// already follow, applied to company memory: **an absence must never read to a successor
/// as a denial.** "loro has nothing on this" (a real, checked answer), "loro could not be
/// consulted" (an unknown) and "this install has no corpus configured" (a different
/// unknown) are three different facts, and collapsing them into one silent `None` is how a
/// fresh Rich ends up asserting the company has no position on something it decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum LoroTier {
    /// No compiler is attached — the default, and what an install with no corpus
    /// configured keeps. Continuity holds; grounding is thinner (continuity §2.3/§4, §8 Q4).
    NotWired,
    /// Compiled, and loro had something to say. The string is `slice.text` VERBATIM —
    /// `CONTEXT-CONTRACT.md` §3: it is self-contained and carries its own
    /// `COMPANY MEMORY (loro) — bearing on: …` heading, so it is injected as its own block
    /// with no prefix. Adding one doubles the heading.
    Slice(String),
    /// Compiled, exit 0, `thin: true` — loro genuinely holds nothing bearing on the topic.
    /// The string is loro's OWN one-line honesty text (§5), which already ends *"Do not
    /// assume company facts — ask the CEO or check a live system."* That sentence is the
    /// answer; fabricating a slice in its place is the failure this variant exists to make
    /// impossible.
    NothingRecorded(String),
    /// A compiler is attached and could not produce a trustworthy answer: the process
    /// failed to spawn, exited non-zero, returned an unsupported `schemaVersion`, or the
    /// slice was REFUSED by the entity/lane re-assertion. Carries the reason, because a
    /// successor told nothing at all would infer "nothing recorded".
    Unavailable(String),
}

impl LoroTier {
    /// The injectable text, when there is one. `None` for both unknown states.
    pub fn text(&self) -> Option<&str> {
        match self {
            LoroTier::Slice(t) | LoroTier::NothingRecorded(t) => Some(t),
            _ => None,
        }
    }

    /// True only for a checked, positive answer from loro.
    pub fn is_slice(&self) -> bool {
        matches!(self, LoroTier::Slice(_))
    }
}

/// THE COMPILER'S OWN VERDICT ON WHETHER IT ANSWERED THE QUESTION — the slice's `coverage`.
///
/// `loro/lib/compile.js:144` labels a compiled slice `direct` or `adjacent` against
/// `TUNING.DIRECT_QUERY_COVERAGE`, and `:287` overrides that to `none` when the slice is
/// thin. Those three strings are the whole vocabulary, and they are the gate the evidence
/// lookup hangs on (the 2026-09-17 evidence-retrieval ruling, §1 "How it ranks": *"The
/// natural trigger is the slice reporting `coverage: 'none'` or `'adjacent'`"*).
///
/// # Why a fourth variant exists, and why it is the default
///
/// [`Self::Unknown`] is what a compiler that does not report coverage produces — a third-party
/// implementation of [`LoroContextCompiler`], or any build predating the field. It is NOT a
/// consult signal. `evidence-lookup.js:136-138` states the rule the app is holding to here:
/// *"A missing or unrecognized coverage label is NOT a reason to look: absence of the signal
/// is not the signal."* Collapsing `Unknown` into `NoneRecorded` would make every compiler
/// that says nothing look like a compiler that said "I have nothing", which is the same
/// absence-read-as-denial failure [`LoroTier`]'s four states exist to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SliceCoverage {
    /// A record squarely covers the topic. Memory answered; do not look in the files.
    Direct,
    /// Nothing squarely covers it; the items are the nearest material loro holds.
    Adjacent,
    /// `thin: true` — loro holds nothing bearing on the topic at all.
    NoneRecorded,
    /// The compiler did not say. Never a consult signal.
    Unknown,
}

impl SliceCoverage {
    /// Parse the compiler's own string. Anything else — including an empty string — is
    /// [`Self::Unknown`], never a guess at the nearest label.
    pub fn parse(label: &str) -> Self {
        match label.trim() {
            "direct" => SliceCoverage::Direct,
            "adjacent" => SliceCoverage::Adjacent,
            "none" => SliceCoverage::NoneRecorded,
            _ => SliceCoverage::Unknown,
        }
    }

    /// The string the compiler emitted, for passing back to the lookup unchanged. `Unknown`
    /// has no string — there is nothing to pass on, and inventing one would manufacture a
    /// signal the compiler never sent.
    pub fn as_str(self) -> Option<&'static str> {
        match self {
            SliceCoverage::Direct => Some("direct"),
            SliceCoverage::Adjacent => Some("adjacent"),
            SliceCoverage::NoneRecorded => Some("none"),
            SliceCoverage::Unknown => None,
        }
    }

    /// Should the evidence lookup run? `none` and `adjacent` ONLY — the ruling's §1, and the
    /// same set `evidence-lookup.js:127` holds. The JS is asked again on the other side of
    /// the process boundary; this is the gate that decides whether to spend a process at all,
    /// and the two agreeing is asserted by a test rather than assumed.
    pub fn should_consult_evidence(self) -> bool {
        matches!(self, SliceCoverage::NoneRecorded | SliceCoverage::Adjacent)
    }
}

/// One compile: the injectable tier PLUS the verdict that decides what happens next.
///
/// [`LoroTier`] deliberately carries text and nothing else — it is what gets injected. The
/// coverage label is not injectable and is not about the text; it is the compiler's statement
/// about whether it answered, and the caller needs it to decide whether to look in the CEO's
/// files. Keeping them in one value means a caller cannot act on a coverage label belonging to
/// a different compile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompiledTier {
    pub tier: LoroTier,
    pub coverage: SliceCoverage,
}

/// The Tier-C SEAM (continuity §2.3/§4, §8 Q4): the loro Context Compiler is loro-owned
/// and lives outside this repo (`richos-hq/loro/`, behind a versioned
/// `CONTEXT-CONTRACT.md`); richos-core stays out of it. This trait is the CONTRACT that
/// lets it plug in with zero coupling — implement it, call
/// `Spine::set_loro_context_compiler`, done. The shipped implementation is
/// [`crate::loro::CliContextCompiler`], which shells out to the contract's own entry point.
///
/// **It cannot fail the turn.** The return type has no error arm on purpose:
/// `CONTEXT-CONTRACT.md` §3 requires that a non-zero exit degrades to no slice and *never*
/// fails the turn, and a `Result` invites a caller to `?` it into the rotation path — where
/// a memory miss would take down a session rotation the CEO is not supposed to be able to
/// see. Every failure is an [`LoroTier::Unavailable`] carrying its reason instead.
pub trait LoroContextCompiler: Send {
    /// Compile the small, topical slice for `req` (§2.1 #8: strategy, constraints, prior
    /// decisions, CEO preferences bearing on the ACTIVE thread — not all of loro).
    fn compile_slice(&self, req: &SliceRequest<'_>) -> LoroTier;

    /// The same compile, plus the compiler's own `coverage` verdict on it.
    ///
    /// **Defaulted to [`SliceCoverage::Unknown`] on purpose, and that default is the safe
    /// one.** An implementation that does not override this has not said whether it answered
    /// the question, and `Unknown` never opens the evidence lookup. A default of
    /// `NoneRecorded` would send every such compiler's silence into the CEO's own files.
    ///
    /// The shipped implementation ([`crate::loro::CliContextCompiler`]) overrides it and
    /// reports the label the compiler actually emitted; `compile_slice` there is this method
    /// with the verdict dropped, so the two can never describe different compiles.
    fn compile_tier(&self, req: &SliceRequest<'_>) -> CompiledTier {
        CompiledTier { tier: self.compile_slice(req), coverage: SliceCoverage::Unknown }
    }
}

// ---------------------------------------------------------------------------
// TIER E — THE EVIDENCE LOOKUP. Files, not memory, and never confused with it.
// ---------------------------------------------------------------------------

/// What the evidence lookup is asked. It is a LOOKUP, not a compile: there is no budget,
/// because the block takes no characters from the memory lanes' budget (the 2026-09-17
/// evidence-retrieval ruling §1/§4, and `evidence-lookup.js` `budget.takenFromMemoryBudget`,
/// which is `false` and is *"there to be asserted"*).
#[derive(Debug, Clone, Copy)]
pub struct EvidenceRequest<'a> {
    /// The conversation. Provenance and logging, not ranking.
    pub thread_id: &'a str,
    /// The entity area this thread is bound to (ECS §3.4). The lookup serves the `rich`
    /// audience only in v1, so this narrows nothing today; it is passed because a lookup
    /// that could not name whose turn it served would be the wrong shape to widen later.
    pub entity_id: &'a str,
    /// What the CEO asked, in his words — the same topic the slice was compiled for.
    pub topic: &'a str,
    /// The compiled slice's own verdict. The lookup runs on `none` and `adjacent` ONLY.
    pub coverage: SliceCoverage,
}

/// Tier E's FIVE states, and three of them are not a block.
///
/// Exactly [`LoroTier`]'s discipline, for exactly its reason: an absence must never read to a
/// successor as a denial. "your files have nothing on this" (a real, checked answer), "the
/// lookup could not run" (an unknown), "memory answered so nobody looked" (a different
/// unknown) and "this install has no lookup configured" (a third) are four different facts.
///
/// **And one more rule this type carries that [`LoroTier`] does not:** whatever is in
/// [`Self::Block`] is UNTRUSTED TEXT out of a file anyone in the world can put in front of the
/// CEO. The renderer says so in the payload, every time, above the block — the same structural
/// DATA-not-instructions boundary `lib/workspace/immune.js` names as defense #1 of its POISONED
/// arm (*"all ingested content is DATA, never instructions"*), applied at the one place the
/// text finally reaches a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum EvidenceTier {
    /// No lookup is attached — the default, and what an install with no Workspace source keeps.
    NotWired,
    /// The slice covered the question, so nothing was looked at. Carries the coverage label.
    NotConsulted(String),
    /// The lookup ran and found material. `text` is the module's rendered block VERBATIM —
    /// heading, standing warning, boundary markers, deep links — and the app adds nothing to
    /// it. `spoken` is the same finding as a sentence that can be said out loud: title, source
    /// and date, and an OFFER, never the excerpt and never a URL.
    Block { text: String, spoken: String },
    /// The lookup ran and the CEO's files hold nothing matching. A checked answer.
    NothingFound(String),
    /// A lookup is attached and could not produce a trustworthy answer: the process failed to
    /// spawn, exited non-zero, or returned an unsupported schema. Carries the reason, because
    /// a successor told nothing at all would infer "there are no such files".
    Unavailable(String),
}

impl EvidenceTier {
    /// The block text, when there is one.
    pub fn text(&self) -> Option<&str> {
        match self {
            EvidenceTier::Block { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The SPOKEN form, when there is one — what Rich says in a voice turn.
    pub fn spoken(&self) -> Option<&str> {
        match self {
            EvidenceTier::Block { spoken, .. } => Some(spoken),
            _ => None,
        }
    }

    /// True only when there is a labeled block to render.
    pub fn is_block(&self) -> bool {
        matches!(self, EvidenceTier::Block { .. })
    }

    /// Characters this tier contributes to the priming prompt. **Never subtracted from
    /// [`DEFAULT_LORO_BUDGET_CHARS`]** — the memory lanes' budget belongs to memory, and the
    /// ruling says so in as many words. Exposed so a test can assert the separation rather
    /// than trust this sentence.
    pub fn chars(&self) -> usize {
        self.text().map(str::len).unwrap_or(0)
    }
}

/// THE TIER-E SEAM. The evidence lookup lives in `tools/richos-service` and reaches the CEO's
/// own files; richos-core stays out of it, exactly as it stays out of loro. The shipped
/// implementation is [`crate::evidence::CliEvidenceLookup`], which runs
/// `bin/richos-evidence.mjs`.
///
/// **It cannot fail the turn**, and the return type has no error arm for the same reason
/// [`LoroContextCompiler`]'s has none: a `Result` invites a caller to `?` it into the rotation
/// path, where a missing document would take down a session rotation the CEO is not supposed
/// to be able to see. Every failure is an [`EvidenceTier::Unavailable`] carrying its reason.
pub trait EvidenceLookup: Send {
    /// Look in the CEO's files for `req`. **The implementation must honor
    /// [`SliceCoverage::should_consult_evidence`] and read nothing when it is false** — the
    /// caller checks too, and both checking is the point: one of them is a cheap guard against
    /// spending a process, the other is the wall.
    fn look_up(&self, req: &EvidenceRequest<'_>) -> EvidenceTier;
}

/// How many recent turns carry VERBATIM (Tier A #4). Small by design — the payload
/// is billed on every rotation under BYO-Anthropic, so it is budgeted, not dumped.
pub const DEFAULT_TAIL_TURNS: usize = 8;

/// Tier C's character budget. `CONTEXT-CONTRACT.md` §3 asks for *"the number the payload
/// budgeter can afford after Tiers A and B"* and calls 800–1600 the sane band; **there is
/// no payload budgeter**, so this is a constant sitting at the contract's own default, and
/// it is named as a constant rather than dressed up as a measurement.
///
/// What it costs, MEASURED rather than asserted (2026-08-29,
/// `examples/loro_reprime_demo.rs` against the 515-record dogfood corpus, one CEO turn, no
/// action ledger): the compiled slice was **911 chars** of a **3,241-char** payload —
/// 28.1%. Had it filled its cap the payload would be 3,530 chars and Tier C 34.0% of it.
/// So Tier C is the single largest lever in the payload, and the number to turn first if a
/// re-prime ever has to get cheaper. At the contract's ~4 chars/token guide, 1200 chars is
/// ≈300 tokens.
pub const DEFAULT_LORO_BUDGET_CHARS: usize = 1200;

#[derive(Debug, Clone, Serialize)]
pub struct TurnView {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionView {
    pub kind: String,
    pub detail: String,
    pub status: String,
}

/// The distilled working set injected into a fresh session (continuity design §2.1).
#[derive(Debug, Clone, Serialize)]
pub struct RePrimePayload {
    // Tier A — always, verbatim, small
    pub identity_assertion: String,
    pub pending_decisions: Vec<String>,
    pub current_intent: Option<String>,
    pub recent_tail: Vec<TurnView>,
    // Tier B — summarized / structured
    pub rolling_summary: Option<String>,
    pub action_ledger_digest: Vec<ActionView>,
    pub worker_state: Vec<String>,
    /// Set when NO team directory could be attributed to this session, naming the reason
    /// (`worker_status.rs`). `worker_state` is then empty for a reason that is NOT "there
    /// are no workers", and `to_priming_prompt` says exactly that instead of falling
    /// silent — the same rule the ACTION LEDGER section already follows.
    pub worker_state_unknown: Option<Unattributed>,
    // Tier C — compiled, minimal. FOUR states, never one silent Option (see `LoroTier`).
    pub loro: LoroTier,
    /// Tier E — THE EVIDENCE LOOKUP: the CEO's own files, labeled as files, consulted only
    /// when Tier C says memory did not answer. It is not Tier D and it is not a lane of Tier
    /// C; it is a separate block under a separate heading, and the separation IS the product
    /// (the 2026-09-17 ruling §1: *"the two headings carry different epistemic weight and that
    /// difference is the entire product here"*). It takes no characters from Tier C's budget.
    pub evidence: EvidenceTier,
}

impl RePrimePayload {
    /// Assemble the payload for a conversation thread from the shared ledger.
    ///
    /// Tier C is [`LoroTier::NotWired`] here and is filled in by the caller
    /// (`Spine::compile_loro_tier`), because compiling it needs a TOPIC and the topic is
    /// derived from the payload this function returns — Tier A #3's current intent, or
    /// failing that the last thing the CEO actually said. Assembling Tiers A/B first and
    /// compiling Tier C from them is the only ordering that can produce a topical request
    /// (`CONTEXT-CONTRACT.md` §1: a slice is always topical).
    ///
    /// `session_id` is the compute lease this payload is being built for. It is what the
    /// worker section is ATTRIBUTED by: the team directory is derived from it
    /// (`worker_status::resolve_team_dir`), never picked off the filesystem. `None` — no
    /// lease — yields an explicitly-unknown worker section rather than an empty one, and
    /// performs no filesystem read at all.
    ///
    /// SCOPED (ECS §3.3–3.5): the payload is assembled from a [`ThreadBinding`], not a
    /// bare thread id, and every ingredient is read through the entity guard. A re-prime
    /// is the highest-leverage cross-entity leak in the app — whatever lands here is
    /// asserted to a fresh session as authoritative — so an unbound thread cannot be
    /// primed at all, and no other entity's turns or actions can reach the prompt.
    pub fn assemble(
        ledger: &Ledger,
        binding: &ThreadBinding,
        tail_turns: usize,
        session_id: Option<&str>,
    ) -> Result<Self, LedgerError> {
        Self::assemble_filtered(ledger, binding, tail_turns, session_id, false)
    }

    /// Context injection must not receive a request awaiting visible delivery. The ledger
    /// records acceptance before priming for crash recovery, not as model authorization.
    pub fn assemble_for_priming(
        ledger: &Ledger, binding: &ThreadBinding, tail_turns: usize,
        session_id: Option<&str>,
    ) -> Result<Self, LedgerError> {
        Self::assemble_filtered(ledger, binding, tail_turns, session_id, true)
    }

    fn assemble_filtered(
        ledger: &Ledger, binding: &ThreadBinding, tail_turns: usize,
        session_id: Option<&str>, exclude_pending: bool,
    ) -> Result<Self, LedgerError> {
        // Superseded turns (§5.3 mid-turn-crash replay) are excluded from the working
        // set entirely — a successfully-replayed turn is no longer "unfinished," and its
        // dead predecessor shouldn't pollute the verbatim tail either.
        let scoped = ledger.thread_turns_scoped(binding)?;
        let turns: Vec<_> = scoped
            .into_iter()
            .filter(|t| t.source != Source::Internal && t.superseded_by.is_none())
            .filter(|t| !exclude_pending || !matches!(t.state, TurnState::Received | TurnState::InFlight))
            .collect();
        Ok(Self::from_turns(ledger, binding, turns, tail_turns, session_id))
    }

    /// **The payload for a thread whose RECORD DOES NOT EXIST YET** — a spare front desk's
    /// priming, for an id [`Ledger::reserve_thread_id`] has handed out and nothing has spent.
    ///
    /// **It is infallible, and that is the finding rather than a convenience.** Every other
    /// assembler starts at `ledger.thread_turns_scoped(binding)`, which fails closed on an
    /// unknown thread — correctly, because a binding for a thread that is not in the ledger
    /// must never be served. A reserved id has no record, so it has no turns: not "the turns
    /// were skipped", but *there are none, provably, because nothing has ever been written
    /// under this id*. So the tail, the current intent, the pending decisions and the rolling
    /// summary are empty BY CONSTRUCTION here, exactly as they would be a microsecond after
    /// the record is created.
    ///
    /// What is NOT empty, and is the whole reason this is worth a model turn: the identity
    /// assertion (which names this conversation and its entity scope), the ENTITY-scoped
    /// action-ledger digest, and — added by the caller, as everywhere else — Tier C, Tier E
    /// and the company block. Those are what the CEO's first sentence would otherwise wait
    /// for.
    pub fn assemble_for_a_reserved_thread(
        ledger: &Ledger, binding: &ThreadBinding, tail_turns: usize, session_id: Option<&str>,
    ) -> Self {
        Self::from_turns(ledger, binding, Vec::new(), tail_turns, session_id)
    }

    fn from_turns(
        ledger: &Ledger, binding: &ThreadBinding, turns: Vec<&crate::ledger::Turn>,
        tail_turns: usize, session_id: Option<&str>,
    ) -> Self {
        let thread_id = binding.thread_id();

        // Tier A #4 — last N verbatim (user + assistant).
        let mut recent_tail = Vec::new();
        for t in turns.iter().rev().take(tail_turns).rev() {
            // A Proactive turn is Rich speaking UNPROMPTED — there was no CEO prompt, and
            // `user_text` is empty by construction (see `Event::ProactiveMessage` in
            // ledger.rs). Emitting the user line anyway put a PHANTOM BLANK CEO UTTERANCE
            // in front of it ("  user: " with nothing after the colon), which reads to a
            // successor as "the CEO said something and Rich answered" — mis-attributing
            // Rich's own initiative to the CEO. That is the same false-attribution class
            // §6 exists to structurally exclude, so the line is omitted and the reply is
            // labelled for what it was.
            if t.source != Source::Proactive {
                let role = if t.source == Source::Agent {
                    format!("agent ({})", t.channel.as_deref().unwrap_or("unknown"))
                } else {
                    "user".into()
                };
                recent_tail.push(TurnView { role, text: t.user_text.clone() });
            }
            if !t.assistant_text.is_empty() {
                let role = if t.source == Source::Proactive { "assistant (unprompted)" } else { "assistant" };
                recent_tail.push(TurnView { role: role.into(), text: t.assistant_text.clone() });
            }
        }

        // Tier A #3 — current intent: the most recent not-yet-completed ask.
        let current_intent = turns
            .iter()
            .rev()
            .find(|t| t.source != Source::Agent && matches!(t.state, TurnState::Received | TurnState::InFlight))
            .map(|t| t.user_text.clone());

        // Tier A #2 — pending decisions: interrupted turns are open loops a successor
        // must not silently drop (a fuller heuristic — "Rich asked, CEO hasn't answered"
        // — lands with the rolling-summary work).
        let pending_decisions: Vec<String> = turns
            .iter()
            .filter(|t| t.state == TurnState::Interrupted)
            .map(|t| if t.source==Source::Agent {format!("Unfinished agent input ({}): {}",t.channel.as_deref().unwrap_or("unknown"),t.user_text)} else {format!("Unfinished: {}", t.user_text)})
            .collect();

        // Tier B #5 — rolling summary of everything BEFORE the verbatim tail.
        // Continuity §2.4 recommends BOTH: an always-on deterministic structured digest
        // as the crash-safe floor, UPGRADED to a self-authored handoff summary on clean
        // rotation (highest fidelity — the session that lived the conversation distills
        // it). Prefer the handoff summary when one exists for this thread; otherwise fall
        // back to the structured digest so a thread that has never rotated (or whose
        // outgoing lease couldn't be asked, e.g. it already crashed) still carries SOME
        // rolling context.
        let older = turns.len().saturating_sub(tail_turns);
        let rolling_summary = match ledger.handoff_summary(thread_id) {
            Some(summary) => Some(summary.to_string()),
            None if older > 0 => Some(format!(
                "{older} earlier turn(s) in this thread precede the verbatim tail below; \
                 topics and decisions from them are recorded in the ledger and can be pulled on demand."
            )),
            None => None,
        };

        // Tier B #6 — action ledger digest (anti-false-attribution authority).
        // CEO-FACING actions only: internal machinery actions (lease rotation, re-prime
        // injection, crash recovery) are durably recorded in the same ledger but are
        // NEVER injected here — the identity assertion forbids the successor from ever
        // revealing or referencing session rotation (§6.2), so handing it a line reading
        // "[done] session_rotation" under a header that calls the section authoritative
        // ground truth for "what Rich has done" would manufacture that leak.
        //
        // ENTITY-SCOPED (ECS §3.5, UX §22 "cross-entity context must not be faked). The
        // digest used to be the WHOLE ledger's CEO-facing actions, which meant every
        // rotation injected every entity's actions into every entity's session under a
        // header calling them authoritative. Scoping is by entity, not by thread, so
        // nothing that was visible within an entity disappears — only the cross-entity
        // leak closes.
        //
        // A message the app REPLACED is left out (`Spine::replace_app_message`: the "Dictation
        // feature" message after he picks another key). Its line here put the old key in every
        // priming of the company beside the new one, and Rich answered "an earlier one said F2"
        // (Ray, esc-20261014T041934Z-2acf6aeb). Only a superseded proactive MESSAGE goes: an
        // action of a turn superseded by a crash replay (§5.3) stays, because what that dead turn
        // did is exactly what a successor must not do twice.
        let action_ledger_digest: Vec<ActionView> = ledger
            .ceo_facing_actions_for_entity(binding.entity_id())
            .iter()
            .filter(|a| {
                !(a.kind == "proactive_message"
                    && a.turn_id.as_deref().and_then(|t| ledger.turn(t)).is_some_and(|t| t.superseded_by.is_some()))
            })
            .map(|a| ActionView {
                kind: a.kind.clone(),
                detail: a.detail.clone(),
                status: match a.status {
                    ActionStatus::Claimed => "in-progress",
                    ActionStatus::Completed => "done",
                    ActionStatus::Failed => "failed",
                }
                .into(),
            })
            .collect();

        // Tier B #7 — live worker state, from the engine's durable event logs
        // (worker_status.rs), for THIS SESSION and no other.
        //
        // THIS IS THE HIGHEST-STAKES LINE IN THE MODULE and it was wrong until 2026-08-29.
        // `current_status()` took no argument and resolved its directory by picking the
        // most-recently-modified `~/.claude/teams/session-*` — so on any machine running a
        // second Claude Code session (the stated ICP runs one; the founder's dogfood
        // machine always does) this section handed a fresh Rich ANOTHER SESSION'S WORKERS,
        // under a header calling it live worker state, in the very payload whose Tier A #1
        // identity assertion exists to kill the false-attribution class. Four `session-*`
        // directories were present on the development machine when this was found, and the
        // mtime-newest belonged to the session writing the fix.
        //
        // It is now derived from the lease's own session id, and when that yields nothing
        // the payload SAYS SO (see `to_priming_prompt`) rather than falling silent.
        let workers = crate::worker_status::current_status(session_id);
        let worker_state_unknown = workers.unattributed;
        let worker_state: Vec<String> = workers.priming_lines();

        RePrimePayload {
            identity_assertion: Self::identity_assertion_scoped(binding),
            pending_decisions,
            current_intent,
            recent_tail,
            rolling_summary,
            action_ledger_digest,
            worker_state,
            worker_state_unknown,
            loro: LoroTier::NotWired,
            evidence: EvidenceTier::NotWired,
        }
    }

    /// Tier A #1 — the assertion that kills the false-attribution class at the root
    /// (continuity design §2.1 / §6), reduced to the part only a priming TURN can carry.
    ///
    /// **IT USED TO CARRY WHO RICH IS, AND IT NO LONGER DOES.** This function opened with
    /// *"You are Rich, the CEO's AI Chief of Staff"* and went on to forbid revealing session
    /// rotation and to explain that the ledger's coverage is partial. All three of those are
    /// true for every turn of every conversation on every install, so all three moved to the
    /// standing instruction (`doctrine.rs`), which is delivered as a SYSTEM PROMPT rather than
    /// as a turn — fixed at spawn, not subject to compaction, and not re-spent on every
    /// rotation. The inner-doctrine design §5.1 states the acceptance condition in one line:
    /// *"after the change, `identity_assertion` is SHORTER, not longer"*. Measured for the
    /// conversation id `thr_demo`: **980 characters → 197**, a saving re-spent on every
    /// rotation of every thread for the life of the install.
    ///
    /// **MOVED, not copied.** Two copies of one rule is not a risk of divergence, it is a
    /// schedule for it — and worse here than in the `spine.rs` case, because one copy would be
    /// in Rust and one in Markdown, so no grep could diff them.
    ///
    /// What is left is exactly what a system prompt CANNOT say, because it is different every
    /// time: which conversation this is, and that the ledger being referred to is the one
    /// printed below this sentence. The doctrine says how to read a partial record; only the
    /// turn can say *here it is*.
    pub fn identity_assertion(conv_id: &str) -> String {
        serde_json::json!({"thread_id":conv_id}).to_string()
    }

    /// The identity assertion PLUS the scope the successor is resuming inside (ECS §3.5:
    /// *"The default read set is the CEO layer plus the active entity"* and *"A thread
    /// cannot read another entity because a name appears related"*).
    ///
    /// A successor that is not told its entity has an advisory boundary, not an enforced
    /// one: everything else here is filtered for it, but nothing stops it from REASONING
    /// across entities out of its own training or from a name it recognizes. So the scope
    /// is stated as a fact and the cross-entity assumption is named and forbidden. This is
    /// the prompt half; the data half is the filtering above, and neither is sufficient
    /// alone.
    pub fn identity_assertion_scoped(binding: &ThreadBinding) -> String {
        serde_json::json!({"thread_id":binding.thread_id(),"entity_id":binding.entity_id()}).to_string()
    }

    /// The TOPIC to compile Tier C for, per `CONTEXT-CONTRACT.md` §3.
    ///
    /// §3 asks for the CEO's *current intent* — "one or two sentences of natural language,
    /// not a keyword and not the whole conversation tail", because a long tail dilutes
    /// `coverage`: every incidental word enlarges the IDF denominator, so a squarely
    /// relevant slice gets labelled `adjacent` for having been asked verbosely. It gives
    /// the fallback too: *"If you have only a long tail, send its last user turn."*
    ///
    /// So, in order: Tier A #3's `current_intent` (the most recent not-yet-completed ask —
    /// which is what "current intent" means here and in the continuity design §2.1 #3),
    /// then the last thing the CEO actually said in the verbatim tail. `None` when the
    /// thread holds neither, and `None` means DO NOT COMPILE: there is no legal
    /// topic-less request, and inventing one out of the thread id would ask loro a
    /// question nobody asked.
    ///
    /// Rich's own replies are never a topic. Compiling company memory for what the
    /// ASSISTANT last said would let a single hallucinated noun steer the next re-prime's
    /// grounding — memory retrieved for a question the CEO never asked, injected under a
    /// header calling it company memory.
    pub fn topic(&self) -> Option<&str> {
        if let Some(intent) = self.current_intent.as_deref() {
            let t = intent.trim();
            if !t.is_empty() {
                return Some(t);
            }
        }
        self.recent_tail
            .iter()
            .rev()
            .find(|tv| tv.role == "user" && !tv.text.trim().is_empty())
            .map(|tv| tv.text.trim())
    }

    /// Render the payload to the internal priming-turn text sent on `session/new`
    /// (continuity design §2.5). Its output is NEVER rendered to the CEO.
    pub fn to_priming_prompt(&self) -> String {self.to_priming_prompt_with_company(None)}
    pub fn to_priming_prompt_with_company(&self,company:Option<&str>)->String {
        #[derive(Serialize)]
        struct Restored<'a> {
            event:&'static str,context:&'a RePrimePayload,
            #[serde(skip_serializing_if="Option::is_none")]
            company_material:Option<serde_json::Value>,
        }
        let company_material=company.map(|text|serde_json::from_str(text).unwrap_or_else(|_|serde_json::Value::String(text.into())));
        serde_json::to_string(&Restored {event:"context_restored",context:self,company_material}).expect("serializable context")
    }
}

pub fn context_only_priming(text: &str) -> String {
    serde_json::json!({"event":"context_restored","context":text}).to_string()
}
