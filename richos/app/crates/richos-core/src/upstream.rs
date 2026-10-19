//! **UPSTREAM MODEL-API FAILURE — the vocabulary, and the one place it is decided.**
//!
//! `open-items.md` row 3.30, and `richos-hq/wiki/richos-frontend.md`
//! §"Upstream model-API failure is a first-class failure mode, not an edge case
//! (observed 2026-09-03)". This is not a hypothetical failure class. On 2026-09-03 the
//! Anthropic API returned `529 Overloaded` and killed four running agents mid-task, plus a
//! fifth earlier on a session limit. Each died between its dispatch and its first tool
//! call, writing nothing. The verbatim bytes — including all four request ids — are
//! committed at `docs/verification/upstream-failure-2026-09-05/captured-529.txt`.
//!
//! ## WHAT THIS MODULE DECIDES, AND WHAT IT REFUSES TO DECIDE
//!
//! It answers one question — *"is this failure the upstream model API, and if so which
//! kind?"* — and it answers `None` rather than guessing. Everything downstream (the retry
//! budget, the loss statement, the voice cut-off line) reads its
//! answer; nothing downstream re-derives it.
//!
//! ## THE `429` / `529` DISTINCTION IS THE WHOLE POINT OF HAVING AN ENUM
//!
//! The row is explicit: *"one clears on a schedule the operator can be told and the other
//! does not"*. They arrive on the same wire, in the same sentence shape, from the same
//! vendor, and presenting them identically tells the CEO to wait for a thing that is not
//! coming — or to stop waiting for one that is. [`UpstreamFault::ceo_message`] is
//! therefore a `match` with no shared arm, and
//! [`UpstreamFault::clears_on_a_known_schedule`] is the machine-readable half of the same
//! fact.
//!
//! ## IT IS KEYED ON STRUCTURE, NEVER ON PROSE
//!
//! The vendor's message body is English that can be reworded in any release. Its
//! STRUCTURE is code, and it was read out of the shipped bundle rather than assumed:
//!
//! ```text
//!   API Error: <status> <message> (error type <kind>, HTTP <status>, request id <id>,
//!                                  model sent to the API: <model>)
//! ```
//!
//! with `kind` drawn from the vendor's own `new Set(["rate_limit","overloaded","server_error"])`.
//! So this module matches `HTTP <status>`, `error type <kind>` and the leading
//! `API Error: <status>`, and matches nothing else. `upstream_classification_tests.rs`
//! pins that by classifying a `529` whose entire message body has been replaced with
//! `qqqq`.
//!
//! ## WHAT IS NOT PROVEN, SAID HERE RATHER THAN DISCOVERED LATER
//!
//! **No capture exists of an API error crossing the `claude` stream-json wire.** The
//! twelve runs in `docs/verification/native-claude-stream-json-2026-08-31/raw/` contain
//! none: their only `is_error: true` frames are the three interrupt runs. So
//! [`UpstreamFailure::classify`] is fed from every channel such a failure could plausibly
//! arrive on — assistant text, the `result` frame's `result` string, the child's stderr
//! tail, and a [`crate::cognition::CognitionError`]'s `Display` — rather than from one
//! channel somebody picked. Breadth in place of a capture is not the same thing as a
//! capture, and this paragraph is the difference being stated instead of glossed.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// The vendor's own error-kind vocabulary, read verbatim out of the shipped Claude Code
/// bundle at `~/.local/share/claude/versions/2.1.261`:
///
/// ```js
/// var Z2o = new Set(["rate_limit", "overloaded", "server_error"]);
/// ```
///
/// Kept as a constant because it is the ONE token in the message that the vendor computes
/// rather than writes, and a grep for it must find both the classifier and this note.
pub const VENDOR_ERROR_KINDS: [&str; 3] = ["rate_limit", "overloaded", "server_error"];

/// What kind of upstream failure this is.
///
/// Four arms, and the fourth is the honest one: a status RichOS has no story for is
/// [`UpstreamFault::Unclassified`] and says so, rather than being folded into whichever
/// arm looked closest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamFault {
    /// `529` — the API is at capacity. **Clears when it clears.**
    Overloaded,
    /// `429` — quota or rate limit. **Clears on a schedule**, which is the entire reason
    /// this is not the same arm as [`UpstreamFault::Overloaded`].
    RateLimited,
    /// Any other `5xx`. Their side, not the CEO's, and no schedule either way.
    ServerError,
    /// An `API Error:` RichOS has no bucket for — a `400`, a `401`, a status invented
    /// after this was written. Named rather than guessed.
    Unclassified,
}

impl UpstreamFault {
    /// **THE CEO-FACING LINE, and there is deliberately no shared arm.**
    ///
    /// Named `ceo_message` for the second reason `controller.rs` gives in
    /// `richos-voice`: `app/ui/tests/lib/state-strings.js` scrapes the product's
    /// CEO-facing sentences out of source, and under `app/crates` the only shape it can
    /// see is a literal inside a function with this name. A sentence the state registry
    /// cannot see is a sentence nobody has said whether the CEO can act on.
    ///
    /// **None of them issues an instruction.** There is no control in RichOS that makes
    /// Anthropic less busy or resets a usage window, so a sentence telling him to press
    /// something would be a request wearing a status's clothes — the same discipline
    /// `VoiceNotice::ceo_message` holds to. They state what happened, whose side it is on,
    /// and — the load-bearing half — whether waiting is a plan.
    ///
    /// **The `429` line does not name a time, and that is a gap rather than a choice.**
    /// No captured sample carries a `Retry-After` or a reset timestamp, so RichOS knows
    /// the wait has a schedule and does not know what the schedule is. Saying the shape of
    /// the wait is true; naming an hour would not be.
    pub fn ceo_message(&self) -> &'static str {
        match self {
            UpstreamFault::Overloaded => {
                "Anthropic's servers are at capacity, so that request never reached \
                 Claude. This one ends when their capacity frees up and nothing on this \
                 machine brings it back sooner."
            }
            UpstreamFault::RateLimited => {
                "Your Claude usage limit is used up, so that request never reached Claude. \
                 Unlike a capacity problem, this one ends on a schedule: your plan's usage \
                 window has to roll over. RichOS was not told what time that is."
            }
            UpstreamFault::ServerError => {
                "Claude's API answered with a server error, so that request never reached \
                 the model. It is on Anthropic's side rather than yours and it carries no \
                 schedule."
            }
            UpstreamFault::Unclassified => {
                "Claude's API refused that request and RichOS has no name for the reason. \
                 Its own words are kept with this message rather than summarized."
            }
        }
    }

    /// **The machine-readable half of the `429`/`529` distinction.**
    ///
    /// `true` means "waiting is a plan and the wait has an end the operator could in
    /// principle be told". `false` means waiting may work and may not, and RichOS must not
    /// imply otherwise. Retry policy reads this: a fault whose window rolls over on a schedule
    /// gets no automatic retry at all (see [`RetryBudget`]).
    pub fn clears_on_a_known_schedule(&self) -> bool {
        matches!(self, UpstreamFault::RateLimited)
    }

    /// A short, stable token for logs, the action ledger and event payloads. Never shown
    /// to the CEO — [`UpstreamFault::ceo_message`] is what he reads.
    pub fn tag(&self) -> &'static str {
        match self {
            UpstreamFault::Overloaded => "overloaded",
            UpstreamFault::RateLimited => "rate_limit",
            UpstreamFault::ServerError => "server_error",
            UpstreamFault::Unclassified => "unclassified",
        }
    }
}

/// One classified upstream failure, with whatever the vendor said about it.
///
/// Every field beyond `fault` is `Option`, because the shortest real captured line —
/// `API Error: 529 Overloaded.` — carries none of them. A classifier that needs the
/// decorated form is a classifier that fails on the corpus it was built for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamFailure {
    pub fault: UpstreamFault,
    /// The HTTP status, when the message stated one.
    pub status: Option<u16>,
    /// The vendor's `request id req_...`, when present. This is the id that makes an
    /// incident findable in Anthropic's own logs, so it is carried rather than dropped.
    pub request_id: Option<String>,
    /// `model sent to the API: ...`, when present. The 2026-09-03 incident hit
    /// `claude-fable-5-1` AND `claude-opus-5`, which is how the record knows it was not
    /// model-specific.
    pub model: Option<String>,
    /// The line as it arrived, trimmed. Quoted back to the operator verbatim; never
    /// parsed for meaning beyond the structural tokens above.
    pub raw: String,
}

impl UpstreamFailure {
    /// **Classify one line of text. `None` means "this is not an upstream API failure",
    /// and it is the answer this function gives most often.**
    ///
    /// The gate is the literal `API Error:` prefix or an explicit `error type <kind>`
    /// token. A broken pipe, a missing binary, a ledger write failure and a turn the CEO
    /// stopped all return `None` here — they are real failures with their own vocabulary,
    /// and dressing one of them as an overload would be a lie in the direction of "wait,
    /// it will fix itself".
    pub fn classify(text: &str) -> Option<UpstreamFailure> {
        let raw = text.trim();
        if raw.is_empty() {
            return None;
        }
        let status = parse_status(raw);
        let kind = parse_error_kind(raw);
        let looks_like_api_error = raw.contains("API Error:");

        // A status token alone is not enough — `HTTP 429` could appear in a document the
        // CEO pasted. Either the vendor's prefix or the vendor's `error type` token has to
        // be there too.
        if !looks_like_api_error && kind.is_none() {
            return None;
        }

        let fault = match (status, kind.as_deref()) {
            // STATUS WINS over kind, and the captured corpus is why: every real `529`
            // line says `error type server_error`, so trusting the kind alone would
            // classify the whole 2026-09-03 incident as a generic server error and lose
            // the one distinction the row is about.
            (Some(529), _) => UpstreamFault::Overloaded,
            (Some(429), _) => UpstreamFault::RateLimited,
            (Some(s), _) if (500..600).contains(&s) => UpstreamFault::ServerError,
            (Some(_), _) => UpstreamFault::Unclassified,
            (None, Some("overloaded")) => UpstreamFault::Overloaded,
            (None, Some("rate_limit")) => UpstreamFault::RateLimited,
            (None, Some("server_error")) => UpstreamFault::ServerError,
            (None, _) => UpstreamFault::Unclassified,
        };

        Some(UpstreamFailure {
            fault,
            status,
            request_id: capture_after(raw, "request id "),
            model: capture_after(raw, "model sent to the API: "),
            raw: raw.to_string(),
        })
    }

    /// Classify the first line in a multi-line blob that classifies at all — the shape a
    /// child's stderr tail arrives in.
    ///
    /// **Whole-blob classification would be wrong here.** A `529` line and an
    /// `unknown option` line in the same 64-line tail are two different faults, and
    /// concatenating them produces a string that matches both.
    pub fn classify_lines(blob: &str) -> Option<UpstreamFailure> {
        blob.lines().find_map(UpstreamFailure::classify)
    }

    /// The whole CEO-facing statement: the fault's sentence, then the vendor's own words,
    /// attributed to the vendor rather than spoken in Rich's voice.
    ///
    /// **This is the second half of the defect the row names.** Without it, an
    /// `API Error: 529 …` string arriving as an assistant message is appended to the
    /// ledger as Rich's own reply — the CEO reads a vendor diagnostic in Rich's voice and
    /// has no way to tell it apart from an answer.
    pub fn ceo_message(&self) -> String {
        format!("{} Claude reported: {}", self.fault.ceo_message(), self.raw)
    }

    /// One line for stderr and for the action ledger. Operator-facing.
    pub fn summary(&self) -> String {
        let mut s = format!("upstream {}", self.fault.tag());
        if let Some(st) = self.status {
            s.push_str(&format!(" http={st}"));
        }
        if let Some(id) = &self.request_id {
            s.push_str(&format!(" request_id={id}"));
        }
        if let Some(m) = &self.model {
            s.push_str(&format!(" model={m}"));
        }
        s
    }
}

impl fmt::Display for UpstreamFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary())
    }
}

/// `HTTP 529` first (the vendor computes it), then the `API Error: 529` prefix, then Claude
/// Code's own end to repeated overloads, `API Error: Repeated 529 Overloaded errors.` (2.1.302).
fn parse_status(raw: &str) -> Option<u16> {
    if let Some(v) = three_digits_after(raw, "HTTP ") {
        return Some(v);
    }
    three_digits_after(raw, "API Error: ").or_else(|| three_digits_after(raw, "API Error: Repeated "))
}

fn three_digits_after(raw: &str, marker: &str) -> Option<u16> {
    let at = raw.find(marker)? + marker.len();
    let digits: String = raw[at..].chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() != 3 {
        return None;
    }
    digits.parse().ok()
}

/// `error type <kind>` — and only a kind the vendor actually emits.
///
/// Restricting to [`VENDOR_ERROR_KINDS`] is deliberate: an unknown kind must fall through
/// to [`UpstreamFault::Unclassified`] rather than be carried around as though RichOS knew
/// what it meant.
fn parse_error_kind(raw: &str) -> Option<String> {
    let at = raw.find("error type ")? + "error type ".len();
    let kind: String =
        raw[at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if VENDOR_ERROR_KINDS.contains(&kind.as_str()) {
        Some(kind)
    } else {
        None
    }
}

/// Everything after `marker` up to `,` or `)` — the shape of every field in the vendor's
/// `d.join(", ")` suffix.
fn capture_after(raw: &str, marker: &str) -> Option<String> {
    let at = raw.find(marker)? + marker.len();
    let rest = &raw[at..];
    let end = rest.find([',', ')']).unwrap_or(rest.len());
    let v = rest[..end].trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Split one run's text at the vendor's `API Error:` prefix. Returns
/// `(rich's prose, the vendor's diagnostic)`; the second is empty when there is none,
/// which is every ordinary turn.
///
/// **THE BOUNDARY IS THE PREFIX, NOT THE LINE, and that is a correction a real run
/// forced.** The first version split on newlines. It is wrong here, and the reason is
/// mechanical: assistant text arrives as DELTAS and the ledger concatenates them into one
/// run with no separator, so a turn that streamed *"Here is what I found so far. "* and
/// then failed holds `"Here is what I found so far. API Error: 529 Overloaded…"` as ONE
/// line. Splitting that on newlines finds none, classifies the whole thing, and demotes
/// the answer along with the error — throwing away what the CEO was owed in order to hide
/// what he was not. That is this row's own failure, committed in the other direction.
///
/// `API Error: ` is the vendor's `Bl` constant, read verbatim out of the shipped bundle at
/// `~/.local/share/claude/versions/2.1.261` (see
/// `docs/verification/upstream-failure-2026-09-05/README.md`), so this is the same
/// structural key `upstream.rs` classifies on and not a guess at its English.
///
/// **What it does NOT split, said rather than left to be found.** A fragment carrying only
/// an `error type <kind>` token and no `API Error:` prefix classifies but does not split —
/// there is no marker to cut at, and cutting at the token would leave the status and the
/// word "Error" rendering as Rich's own sentence. No such fragment appears in the captured
/// corpus; if one is ever seen, this is the function that changes.
pub fn split_at_vendor_diagnostic(text: &str) -> (&str, &str) {
    const MARKER: &str = "API Error:";
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(MARKER) {
        let at = from + rel;
        if UpstreamFailure::classify(&text[at..]).is_some() {
            return (&text[..at], &text[at..]);
        }
        from = at + MARKER.len();
    }
    (text, "")
}

// =========================================================================================
// BOUNDED, VISIBLE RETRY
// =========================================================================================

/// **The ceiling on consecutive automatic retries after an upstream failure.**
///
/// **Derived from the incident, not picked.** On 2026-09-03 four consecutive retries
/// consumed quota and produced nothing — request ids `req_011Cegb417YK6i1BEVDFmzU1`,
/// `req_011CegbZnNQkV7nSHESJQXcq`, `req_011CegbzZpYpdqtCRHTmEh9H`,
/// `req_011CegdeZDky4nwegbmqvQoV`. So the arithmetic the ceiling has to satisfy is:
///
/// ```text
///   attempts that produced nothing on 2026-09-03   4
///   attempts RichOS will make                      1 + 1 = 2      (the try, then ONE retry)
///   attempts saved on an outage of that shape      4 - 2 = 2
/// ```
///
/// **One immediate retry, for every fault EXCEPT an overload.** A `429` gets none (its
/// window rolls over in hours); a `529` follows [`OVERLOAD_RETRY_WAITS`] instead, by the
/// CEO's ruling of 2026-10-07, which replaced the old "a second retry is a coin flip" reasoning
/// for that one fault. A server error or an unclassified refusal keeps the one immediate retry.
pub const MAX_UPSTREAM_RETRIES: u32 = 1;

/// **THE OVERLOAD SCHEDULE — the CEO's words of 2026-10-07, verbatim:** *"first retry after 1
/// minute; second retry: after another 2-minute wait; third retry: after another 5-minute
/// wait; 4th retry: after another 10-minute wait; 5th retry: after another 20-minute wait;
/// 6th retry: after another 40-minute wait; 7th retry: after another 80-minute wait."*
///
/// Entry `k` is the wait before retry `k + 1`. Claude Code's own retry runs first, inside each
/// attempt, and cannot do this itself: in 2.1.292 each of its waits is capped at 32 s and its
/// retry count at 15, so it gives up after about 8 minutes. This schedule is the layer around
/// it. Total, measured off the array rather than asserted: 1 + 2 + 5 + 10 + 20 + 40 + 80 =
/// 158 minutes of waiting across 8 attempts.
pub const OVERLOAD_RETRY_WAITS: [Duration; 7] = [
    Duration::from_secs(60),
    Duration::from_secs(2 * 60),
    Duration::from_secs(5 * 60),
    Duration::from_secs(10 * 60),
    Duration::from_secs(20 * 60),
    Duration::from_secs(40 * 60),
    Duration::from_secs(80 * 60),
];

/// How many retries an overload gets: one per entry of [`OVERLOAD_RETRY_WAITS`].
pub const MAX_OVERLOAD_RETRIES: u32 = OVERLOAD_RETRY_WAITS.len() as u32;

/// **How a retry waits.** A seam so a test can drive the 158 minutes with a fake clock, and so
/// the wait ends the moment the CEO stops the turn or quits.
pub trait RetryClock: Send + Sync {
    /// Wait for `wait`, or until `stop_asked()` answers `true`. Returns `true` when the whole
    /// wait elapsed and `false` when it was cut short.
    fn wait(&self, wait: Duration, stop_asked: &dyn Fn() -> bool) -> bool;
}

/// The real clock: sleeps in slices of at most [`SystemRetryClock::POLL`], asking `stop_asked`
/// between them, so a stop is honored within that slice rather than after 80 minutes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRetryClock;

impl SystemRetryClock {
    pub const POLL: Duration = Duration::from_millis(200);
}

impl RetryClock for SystemRetryClock {
    fn wait(&self, wait: Duration, stop_asked: &dyn Fn() -> bool) -> bool {
        let end = std::time::Instant::now() + wait;
        loop {
            if stop_asked() {
                return false;
            }
            let now = std::time::Instant::now();
            if now >= end {
                return true;
            }
            std::thread::sleep((end - now).min(Self::POLL));
        }
    }
}

/// **A bounded, VISIBLE retry allowance — the answer to "silent retry against an
/// overloaded upstream converts an outage into a bill".**
///
/// Two properties, and neither is optional:
///
/// 1. **Bounded.** An overload gets [`MAX_OVERLOAD_RETRIES`] retries, waiting
///    [`OVERLOAD_RETRY_WAITS`] before each (the CEO's schedule, 2026-10-07); every other fault
///    keeps [`MAX_UPSTREAM_RETRIES`] immediate retry, and a `429` gets none.
/// 2. **Visible.** [`RetryBudget::ceo_message`] states what was spent, in attempts, or when the
///    next try is, in the CEO's own units. A ceiling nobody is told about is still a silent
///    retry — it just stops sooner.
///
/// **It counts CONSECUTIVE failures and resets on a success** ([`RetryBudget::succeeded`]).
/// A budget that never reset would burn itself out over a week of healthy use and then be
/// unavailable on the day it was needed; a budget scoped to a single turn is exactly the
/// unbounded case the row measured, one turn at a time. After a success the next overload
/// starts the schedule again at its first wait.
#[derive(Debug, Clone, Default)]
pub struct RetryBudget {
    /// Immediate retries spent since the last success, on faults other than an overload.
    spent: u32,
    /// Overload retries spent since the last success: the index of the next wait in
    /// [`OVERLOAD_RETRY_WAITS`].
    overload_spent: u32,
    /// Attempts made since the last success, INCLUDING the first, non-retry attempt.
    /// This is the number the CEO is told, because it is the number that was billed.
    attempts: u32,
    /// The fault the last failure was classified as, for the message.
    last: Option<UpstreamFault>,
    /// The wait before the retry the last [`RetryBudget::charge`] granted; `None` when it
    /// granted none.
    pending: Option<Duration>,
}

impl RetryBudget {
    pub fn new() -> RetryBudget {
        RetryBudget::default()
    }

    /// Record one failed attempt against `fault`. Returns whether a retry is still
    /// allowed AFTER charging it; [`RetryBudget::wait_before_retry`] then says how long to
    /// wait first.
    ///
    /// **A fault that clears on a schedule gets no retry at all.** A `429` window rolls
    /// over in hours; retrying it within the same second cannot succeed, so spending an
    /// attempt on it is pure cost. That is not a narrowing of the ceiling — it is the
    /// ceiling applied to a fault whose own schedule already answers the question.
    pub fn charge(&mut self, fault: UpstreamFault) -> bool {
        self.attempts += 1;
        self.last = Some(fault);
        self.pending = None;
        match fault {
            UpstreamFault::RateLimited => {
                // Consume the immediate allowance: there is nothing a retry could do.
                self.spent = MAX_UPSTREAM_RETRIES;
                false
            }
            UpstreamFault::Overloaded => {
                let Some(wait) = OVERLOAD_RETRY_WAITS.get(self.overload_spent as usize) else {
                    return false;
                };
                self.overload_spent += 1;
                self.pending = Some(*wait);
                true
            }
            UpstreamFault::ServerError | UpstreamFault::Unclassified => {
                if self.spent >= MAX_UPSTREAM_RETRIES {
                    return false;
                }
                self.spent += 1;
                self.pending = Some(Duration::ZERO);
                true
            }
        }
    }

    /// How long to wait before the retry the last [`RetryBudget::charge`] granted: the
    /// schedule's next entry for an overload, zero for the one immediate retry, `None` when no
    /// retry was granted.
    pub fn wait_before_retry(&self) -> Option<Duration> {
        self.pending
    }

    /// Whether a failure of `fault` would still be retried, without charging anything.
    pub fn may_retry(&self, fault: UpstreamFault) -> bool {
        match fault {
            UpstreamFault::RateLimited => false,
            UpstreamFault::Overloaded => self.overload_spent < MAX_OVERLOAD_RETRIES,
            UpstreamFault::ServerError | UpstreamFault::Unclassified => self.spent < MAX_UPSTREAM_RETRIES,
        }
    }

    /// A turn completed. The allowance is restored, because the upstream demonstrably
    /// works — a POSITIVE signal, never the absence of a failure.
    pub fn succeeded(&mut self) {
        *self = RetryBudget::default();
    }

    /// **A new message from him starts the overload schedule again at its first wait**
    /// (CEO, 2026-10-07: "go with recommended"). Only the overload count and the attempts
    /// the CEO is told about restart; the immediate-retry allowance for other faults is
    /// untouched.
    pub fn new_message(&mut self) {
        self.overload_spent = 0;
        self.attempts = 0;
    }

    /// Attempts made since the last success, including the first.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Automatic retries spent since the last success, of every kind.
    pub fn retries_spent(&self) -> u32 {
        self.spent + self.overload_spent
    }

    /// **What was spent trying, or when the next try is, in the CEO's units.** `None` before
    /// anything has failed — there is nothing to report on a healthy run and inventing a line
    /// for it would be noise.
    pub fn ceo_message(&self) -> Option<String> {
        let fault = self.last?;
        if self.attempts == 0 {
            return None;
        }
        if let (UpstreamFault::Overloaded, Some(wait)) = (fault, self.pending) {
            let minutes = wait.as_secs() / 60;
            let when = if minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") };
            return Some(format!(
                "{} RichOS will try again in {when}, in the same conversation, so nothing is \
                 lost. This is retry {} of {MAX_OVERLOAD_RETRIES}.",
                fault.ceo_message(),
                self.overload_spent
            ));
        }
        let spent = if self.attempts == 1 {
            "RichOS tried once and stopped there.".to_string()
        } else {
            format!("RichOS tried {} times and stopped there.", self.attempts)
        };
        Some(format!(
            "{} {} Each attempt costs against your Claude usage whether or not it \
             produces an answer, so it is not repeating this on its own.",
            fault.ceo_message(),
            spent
        ))
    }
}

// =========================================================================================
// WHAT SURVIVED, AND WHAT DID NOT
// =========================================================================================

/// **The statement made AT the moment a turn dies, about what is still on disk and what
/// is gone.**
///
/// The row: *"A running ACP task dies with its context. Whatever it had established is
/// gone unless the work was already durable."* RichOS's spine already makes the durable
/// half true — every assistant delta is persisted to the ledger BEFORE it is emitted, the
/// prompt is journaled before it is delivered, and actions are claimed before they are
/// executed. What was missing is anybody SAYING so, at the time, to the person who has to
/// decide whether to ask again.
///
/// This is deliberately built from counts the ledger already holds rather than from a
/// narrative, so it cannot claim something survived that did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnLoss {
    /// The CEO's own words for this turn are on disk and can be replayed.
    pub prompt_is_durable: bool,
    /// Characters of Rich's reply that were persisted before the failure.
    pub partial_reply_chars: usize,
    /// Actions recorded for this turn, in any state. These are the record of what was
    /// actually DONE, and they outlive the lease.
    pub actions_recorded: usize,
    /// Whether the compute lease's in-memory context is gone. It always is, on this
    /// failure class — the field exists so the statement is read off a fact rather than
    /// assumed by the sentence.
    pub lease_context_lost: bool,
}

impl TurnLoss {
    /// **The CEO-facing sentence, composed from the counts above.**
    ///
    /// Three claims, in the order that matters to someone deciding what to do next:
    /// what is safe, what is gone, and what that means for asking again. It never says
    /// "nothing was lost", because something always was — the lease's working context is
    /// not recoverable and pretending otherwise is the failure this exists to end.
    pub fn ceo_message(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.prompt_is_durable {
            parts.push("what you asked for is saved".to_string());
        }
        if self.partial_reply_chars > 0 {
            parts.push(format!(
                "the {} characters of the answer that had already arrived are saved",
                self.partial_reply_chars
            ));
        }
        if self.actions_recorded > 0 {
            parts.push(format!(
                "and the {} recorded action(s) for this turn are saved",
                self.actions_recorded
            ));
        }
        let kept = if parts.is_empty() {
            "Nothing had been written to disk yet for this turn.".to_string()
        } else {
            format!("On disk: {}.", parts.join(", "))
        };
        let lost = if self.lease_context_lost {
            " Not on disk: everything the session had worked out in its head and had not \
             yet said. Asking again starts that part over."
        } else {
            ""
        };
        format!("{kept}{lost}")
    }
}

// =========================================================================================
// THE DURABLE RECORD
// =========================================================================================

/// **What is written to the ledger when a turn dies to the upstream API — and it carries
/// the SENTENCES, not just the fault.**
///
/// Storing authored prose in an append-only record looks redundant until you ask what the
/// record is FOR. It is the evidence of what the CEO was told, at the moment he was told
/// it. A later build that reworded [`UpstreamFault::ceo_message`] would otherwise rewrite
/// history: reopening a six-month-old thread would show him a sentence nobody ever showed
/// him. So the sentence that was shown is the sentence that is kept.
///
/// It is also what makes the loss statement survive a reload, which is the difference
/// between *"visible at the moment it happens"* and *"discoverable afterward"* — the exact
/// distinction row 3.30 draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamRecord {
    /// [`UpstreamFault::tag`] — the machine-readable classification.
    pub fault: String,
    /// The HTTP status, when the vendor stated one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The vendor's `req_…`. **Operator-facing and deliberately not rendered**: it is the
    /// id that makes an incident findable in Anthropic's own logs and it means nothing to
    /// the CEO, so it is kept and never shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// The model the vendor said the request went to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// [`UpstreamFault::ceo_message`] as it stood when this happened.
    pub ceo_message: String,
    /// [`TurnLoss::ceo_message`] as it stood when this happened — what was on disk and
    /// what was not, from counts read off the ledger at that instant.
    pub loss_message: String,
    /// [`RetryBudget::ceo_message`] as it stood when this happened. `None` when nothing
    /// had been spent yet, which is a different statement from "nothing was spent".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_message: Option<String>,
}

impl UpstreamRecord {
    /// Assemble the record from the three things that are true at the failure boundary.
    pub fn new(failure: &UpstreamFailure, loss: &TurnLoss, budget: &RetryBudget) -> UpstreamRecord {
        UpstreamRecord {
            fault: failure.fault.tag().to_string(),
            status: failure.status,
            request_id: failure.request_id.clone(),
            model: failure.model.clone(),
            ceo_message: failure.fault.ceo_message().to_string(),
            loss_message: loss.ceo_message(),
            retry_message: budget.ceo_message(),
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: `429` and `529` never produce the same CEO-facing sentence, and the
    /// schedule fact that separates them is machine-readable.
    #[test]
    fn quota_exhaustion_and_overload_are_two_different_statements() {
        assert_ne!(
            UpstreamFault::RateLimited.ceo_message(),
            UpstreamFault::Overloaded.ceo_message()
        );
        assert!(UpstreamFault::RateLimited.clears_on_a_known_schedule());
        assert!(!UpstreamFault::Overloaded.clears_on_a_known_schedule());
        assert!(!UpstreamFault::ServerError.clears_on_a_known_schedule());
    }

    /// POSITIVE CONTROL for the classifier's gate: ordinary local failures are NOT
    /// upstream faults, and a guard that called them one would be useless.
    #[test]
    fn local_failures_are_not_classified_as_upstream_faults() {
        for text in [
            "cognition io: broken pipe",
            "claude channel closed (child exited?)",
            "error: unknown option '--permission-prompt-tool'",
            "the loro slice did not parse",
            "",
        ] {
            assert_eq!(UpstreamFailure::classify(text), None, "{text:?} must not classify");
        }
    }
}
