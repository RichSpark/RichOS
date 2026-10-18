//! Types shared by explicit company-memory and evidence lookups.

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
    /// The user's explicit lookup topic.
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

/// Default budget for explicit company-memory lookups.
pub const DEFAULT_LORO_BUDGET_CHARS: usize = 1200;
