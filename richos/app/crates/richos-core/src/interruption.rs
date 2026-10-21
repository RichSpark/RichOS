//! **WHY A TURN ENDED WITHOUT FINISHING — the vocabulary, and the one place it is decided.**
//!
//! `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D2. The
//! published nightly wrote this to its own ledger:
//!
//! ```text
//! {"event":"TurnInterrupted","turn_id":"turn_afe3ad4a…",
//!  "reason":"cognition protocol: \"Not logged in · Please run /login\"", …}
//! ```
//!
//! and showed the CEO this:
//!
//! > **Stopped after 1s**
//! > I hit a snag mid-thought and had to stop — say the word and I'll pick it back up.
//! > Everything I'd already written above is saved.
//! > \[Pick it back up\]
//!
//! **The app had the exact, actionable cause and showed him something else.** Three separate
//! untruths in four lines: a permanent condition described as a passing hiccup; a promise
//! about saved work when nothing had been written; and a retry control for a state no number
//! of retries can clear. The audit's own note on why this outlives its harness is the reason
//! this module exists: *"not logged in is not an exotic state — it is where every expiring
//! subscription session lands."*
//!
//! # WHAT THIS MODULE DECIDES, AND WHAT IT REFUSES TO DECIDE
//!
//! It answers one question — *"which KIND of ending was this?"* — and it answers
//! [`InterruptionCause::Unknown`] rather than guessing. It is the sibling of
//! [`crate::upstream`], which answers the same shape of question about the model API, and it
//! is deliberately NOT an arm of that enum: a `claude` that refuses locally because nobody is
//! signed in never reached Anthropic, so filing it under "upstream" would be the exact
//! dishonesty that module's header warns against.
//!
//! # IT IS KEYED ON THE VENDOR'S OWN CONSTANTS, READ OUT OF THE SHIPPED BUNDLE
//!
//! There is no status code or error tag on this path — `native.rs` builds
//! `CognitionError::Protocol` from the `result` frame's `result` string, so the only thing
//! RichOS holds is the sentence Claude Code wrote. That sentence is not guessed at here. It
//! was read verbatim out of the installed bundle
//! (`~/.local/share/claude/versions/2.1.274`, 2026-09-17), where the whole family sits
//! together as adjacent constants:
//!
//! ```js
//! gVe="Not logged in \xB7 Please run /login",
//! HEe="Invalid API key \xB7 Fix external API key",
//! Qwn="Invalid auth token \xB7 Fix external auth token",
//! Zwn="Invalid ANTHROPIC_CUSTOM_HEADERS \xB7 Fix the environment variable",
//! Jwn="Invalid request header from the environment \xB7 Fix the environment variable"
//! ```
//!
//! **Those five are two different problems with two different actors**, which is why they are
//! two arms below. The first is the CEO's own account. The other four are a credential
//! somebody put in this machine's environment, and the person who can fix one of those is
//! whoever set RichOS up — telling the CEO to reconnect his account would send him to a
//! screen that cannot help.
//!
//! **A reworded vendor string falls to [`InterruptionCause::Unknown`], and that is the safe
//! direction**: Unknown says plainly that RichOS does not know why, which is true, and offers
//! no route that might be wrong. It never silently becomes the arm that looked closest.
//!
//! # NOTHING HERE EVER CLAIMS SOMETHING SURVIVED THAT DID NOT
//!
//! [`InterruptionRecord::new`] builds its loss sentence from [`crate::upstream::TurnLoss`] —
//! counts read off the ledger at the instant of the failure — and when nothing of Rich's had
//! been written it says nothing about saved work at all. The sentence the audit caught,
//! *"Everything I'd already written above is saved"*, is unreachable from here by
//! construction.

use serde::{Deserialize, Serialize};

use crate::upstream::TurnLoss;

/// The CEO's own sign-in, missing or expired. The first was read verbatim from the shipped
/// bundle (see the module header). The second is the `result` Claude Code 2.1.306 writes for
/// an OAuth sign-in whose token is no longer accepted, measured 2026-10-21 with
/// `CLAUDE_CODE_OAUTH_TOKEN` set to a token Anthropic does not know: *"Failed to authenticate.
/// API Error: 401 OAuth access token is invalid."* Until then it fell to
/// [`InterruptionCause::Unknown`] and its retry control, for a state only a new sign-in clears.
const VENDOR_NOT_SIGNED_IN: [&str; 2] = ["Not logged in", "OAuth access token is invalid"];

/// RichOS's own words when the child ended mid-turn and Claude Code's `auth status` then said
/// nobody is signed in (`native.rs`, [`crate::native::CHILD_EXIT_SIGNED_OUT`]). Ours, so it is
/// matched as our own structure. Checked ahead of the IO prefix the same message carries.
const OUR_CHILD_EXIT_SIGNED_OUT: &str = crate::native::CHILD_EXIT_SIGNED_OUT;

/// A credential that is PRESENT and rejected — an API key, an auth token, or a header set in
/// this machine's environment. Four vendor constants, all of them somebody else's to fix.
const VENDOR_CREDENTIAL_REJECTED: [&str; 4] = [
    "Invalid API key",
    "Invalid auth token",
    "Invalid ANTHROPIC_CUSTOM_HEADERS",
    "Invalid request header from the environment",
];

/// RichOS's own sentence when the CEO's stop reached the child (`native.rs`, the `Cancel`
/// arm). Ours, not the vendor's, so matching it is matching our own structure.
const OUR_STOPPED_AT_YOUR_REQUEST: &str = "stopped at your request";

/// `provider-supervisor.py`'s own words when `execvp` cannot start the binary it was handed —
/// measured verbatim on the 2026-09-17 candidate walk: `Provider could not start: [Errno 2]
/// No such file or directory`. It fires from inside a FORKED CHILD, after `Command::spawn`
/// has already succeeded spawning the supervisor itself, so `native.rs`'s own
/// `NativeError::BinaryMissing` mapping — which depends on `Command::spawn` seeing `ENOENT`
/// — never fires for it; the failure only surfaces here, two layers downstream, as this
/// process's own stderr. First-party and ours to keep matching (not a vendor string that can
/// be renamed underneath this file), so it is checked ahead of the generic IO prefix below.
const OUR_PROVIDER_COULD_NOT_START: &str = "Provider could not start";

/// `CognitionError::Io`'s `Display` prefix. A broken pipe, a closed child, a read that
/// failed — the class where asking again is a real plan.
const OUR_IO_PREFIX: &str = "cognition io:";

/// Which kind of ending this was.
///
/// Six arms, and the last is the honest one. Compare [`crate::upstream::UpstreamFault`],
/// which is shaped the same way and for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptionCause {
    /// Nobody is signed in to the Anthropic account RichOS thinks with, or the session
    /// expired. **Permanent until somebody signs in**; retrying cannot clear it.
    NotSignedIn,
    /// A credential IS configured on this machine and was rejected. Also permanent until
    /// changed, and the person who can change it is whoever set RichOS up — not the CEO.
    CredentialRejected,
    /// The `claude` binary itself could not be started — missing, or not runnable — on this
    /// Mac. Permanent until whoever set RichOS up installs it or points RichOS at it; the
    /// CEO cannot fix it from the chat surface. The 2026-09-17 candidate-walk defect: this
    /// used to fall through to [`InterruptionCause::Transient`], promising a retry that could
    /// never succeed.
    ProviderMissing,
    /// The CEO stopped it. Not a failure at all, and must never be dressed as one.
    StoppedByCeo,
    /// A pipe, a child that went away, a read that failed. **The one arm where asking
    /// again is a plan**, which is exactly why it is not the default.
    Transient,
    /// **The same lost connection again, right after the last one, with nothing of Rich's
    /// written in between.** A failure that repeats is not passing, so this arm never invites
    /// another try: it says what to check and where (2026-10-21: a new user's every message
    /// ended "This usually fixes itself. Press Restore my message, then send it again." in a
    /// loop, `docs/development/failure-classes/permanent-failure-told-as-passing.md`).
    /// Decided by `spine.rs::record_interruption` from the ledger, never by [`classify`],
    /// which sees one reason string and cannot know what came before it.
    Repeated,
    /// Something RichOS has no story for. Named rather than guessed.
    Unknown,
}

impl InterruptionCause {
    /// The machine-readable tag stored in the ledger and read back by the projection. A tag
    /// this build does not recognize reads back as [`InterruptionCause::Unknown`], which is
    /// the safe direction for a record written by a newer RichOS.
    pub fn tag(self) -> &'static str {
        match self {
            InterruptionCause::NotSignedIn => "not-signed-in",
            InterruptionCause::CredentialRejected => "credential-rejected",
            InterruptionCause::ProviderMissing => "provider-missing",
            InterruptionCause::StoppedByCeo => "stopped-by-ceo",
            InterruptionCause::Transient => "transient",
            InterruptionCause::Repeated => "repeated",
            InterruptionCause::Unknown => "unknown",
        }
    }

    pub fn from_tag(tag: &str) -> InterruptionCause {
        match tag {
            "not-signed-in" => InterruptionCause::NotSignedIn,
            "credential-rejected" => InterruptionCause::CredentialRejected,
            "provider-missing" => InterruptionCause::ProviderMissing,
            "stopped-by-ceo" => InterruptionCause::StoppedByCeo,
            "transient" => InterruptionCause::Transient,
            "repeated" => InterruptionCause::Repeated,
            _ => InterruptionCause::Unknown,
        }
    }

    /// **THE CEO-FACING LINE, and there is deliberately no shared arm.**
    ///
    /// Named `ceo_message` for the reason [`crate::upstream::UpstreamFault::ceo_message`]
    /// gives: `app/ui/tests/lib/state-strings.js` scrapes the product's CEO-facing sentences
    /// out of source, and under `app/crates` the only shape it can see is a literal inside a
    /// function with this name.
    ///
    /// **Every one of them reads the same spoken as written.** RichOS is voice-first, so a
    /// sentence that needs a screen to make sense is a sentence that fails half the time it
    /// is delivered. No paths, no error codes, no punctuation doing structural work.
    ///
    /// **None of them promises a retry.** Whether asking again can help is
    /// [`InterruptionCause::offers_retry`], and the sentence and the control are driven by
    /// the same fact so they cannot disagree — which is precisely how the nightly ended up
    /// inviting the CEO to press a button that could not work.
    pub fn ceo_message(self) -> &'static str {
        match self {
            // THE ROUTE IS IN THE SENTENCE, not beside it. It was a second string returned
            // by a `route()` method until `affordances.js` refused the row: under
            // `app/crates` the state registry can only see literals inside a function
            // called `ceo_message`, so a route authored anywhere else would have reached his
            // screen while being invisible to the one inventory that asks whether he can act
            // on what he is told. That is the hole this module's own header objects to, and
            // the fix is to have one authored sentence rather than a way around the scrape.
            //
            // It also reads better aloud. `Settings → Account connection` was the first
            // form, and an arrow is read aloud as nothing at all.
            // AND IT DOES NOT PROMISE TO RESUME. This arm ended "…and I'll pick this
            // straight back up" for one commit, which is the nightly's D6 in a new place:
            // connecting the account clears the CAUSE, and it does not restart a turn that
            // is already terminal. He sends it again, and his message is saved so he can.
            // A sentence that says otherwise is a promise the app cannot keep, which is the
            // whole of what D6 was about.
            // THE ROW IS "Claude accounts" since round 18 (the CEO 2026-10-06, feedback item 9),
            // which took the place of the old account row in Settings and holds the first
            // account's sign-in. The sentence names the row that exists.
            InterruptionCause::NotSignedIn => {
                "RichOS couldn't send this to Rich because no Claude account is signed in on this Mac or the sign-in has expired. Open Settings, choose Claude accounts and sign in, then send your message again."
            }
            InterruptionCause::CredentialRejected => {
                "RichOS couldn't send this to Rich because the Claude key or sign-in token set up on this Mac was refused. Sending again won't help until that key or token is replaced."
            }
            InterruptionCause::ProviderMissing => {
                "RichOS couldn't send this to Rich because Claude Code, which Rich runs on, could not be found on this Mac. Sending again won't help until Claude Code is installed again."
            }
            InterruptionCause::StoppedByCeo => {
                "Stopped, as you asked."
            }
            // NO PROMISE THAT IT CLEARS. "This usually fixes itself" stood here until
            // 2026-10-21 and was said on every message of a Mac where it never did. Once is a
            // fact and an offer; the second time in a row is `Repeated`, below.
            InterruptionCause::Transient => {
                "RichOS lost its connection to Claude Code while Rich was answering, so his answer is incomplete. Press Restore my message, then send it again."
            }
            InterruptionCause::Repeated => {
                "Rich could not answer again because Claude Code stopped before he replied, so sending again is unlikely to help. Open Settings and choose Claude accounts to check that your account is signed in on this Mac. If it is, press Bust a bug in Settings so we can see what went wrong."
            }
            InterruptionCause::Unknown => {
                "Rich's answer stopped early and RichOS can't tell you why. Press Restore my message, then send it again."
            }
        }
    }

    /// **Is asking again a plan, or a false promise?**
    ///
    /// The whole of D2's harm sits in this method. `false` means the surface must not draw a
    /// retry control: a button that cannot work is worse than no button, because pressing it
    /// and watching nothing happen is how somebody stops believing what the app tells them.
    pub fn offers_retry(self) -> bool {
        match self {
            InterruptionCause::NotSignedIn
            | InterruptionCause::CredentialRejected
            | InterruptionCause::ProviderMissing
            | InterruptionCause::Repeated => false,
            // Nothing failed. There is nothing to retry, and his words go back in the box
            // by the ordinary route rather than through a failure card's control.
            InterruptionCause::StoppedByCeo => false,
            InterruptionCause::Transient | InterruptionCause::Unknown => true,
        }
    }

}

/// Decide the cause from the reason string the ledger stores.
///
/// **The input is deliberately the SAME string `Ledger::interrupt_turn` is given**, so the
/// record and the reason can never describe two different events. The caller classifies at
/// the failure boundary and writes both.
///
/// Order matters in one place only: the vendor ships
/// `"API Error: 401 Invalid API key · Please run /login"`, which contains BOTH families'
/// wording. It is a rejected credential, not a missing sign-in, so the sign-in test is the
/// narrower string and the credential test runs where it can still win.
pub fn classify(reason: &str) -> InterruptionCause {
    if reason.contains(OUR_STOPPED_AT_YOUR_REQUEST) {
        return InterruptionCause::StoppedByCeo;
    }
    if VENDOR_CREDENTIAL_REJECTED.iter().any(|needle| reason.contains(needle)) {
        return InterruptionCause::CredentialRejected;
    }
    if VENDOR_NOT_SIGNED_IN.iter().any(|needle| reason.contains(needle)) || reason.contains(OUR_CHILD_EXIT_SIGNED_OUT) {
        return InterruptionCause::NotSignedIn;
    }
    // Checked BEFORE the generic IO prefix: `provider-supervisor.py`'s own failure text
    // arrives wrapped as `NativeError::Startup` -> `CognitionError::Io`, whose `Display`
    // also carries `OUR_IO_PREFIX` — so a permanent, un-retryable cause would otherwise be
    // caught by the generic Transient arm below, exactly as it was on 2026-09-17.
    if reason.contains(OUR_PROVIDER_COULD_NOT_START) {
        return InterruptionCause::ProviderMissing;
    }
    if reason.contains(OUR_IO_PREFIX) {
        return InterruptionCause::Transient;
    }
    InterruptionCause::Unknown
}

/// **What is written to the ledger when a turn ends without finishing — and it carries the
/// SENTENCES, not just the cause.**
///
/// Same reasoning as [`crate::upstream::UpstreamRecord`], which this mirrors field for
/// field where the fields mean the same thing: the record is the evidence of what the CEO
/// was told, at the moment he was told it. A later build that rewords
/// [`InterruptionCause::ceo_message`] must not be able to rewrite what a six-month-old
/// thread shows him.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptionRecord {
    /// [`InterruptionCause::tag`].
    pub cause: String,
    /// [`InterruptionCause::ceo_message`] as it stood when this happened.
    pub ceo_message: String,
    /// What was on disk and what was not — **absent entirely when nothing of Rich's had
    /// been written**, which is the sentence the nightly got wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_message: Option<String>,
    /// [`InterruptionCause::offers_retry`], stored rather than re-derived so the control the
    /// CEO sees on a reload is the control he saw at the time.
    pub offers_retry: bool,
}

impl InterruptionRecord {
    /// Assemble the record from the two things that are true at the failure boundary.
    ///
    /// # THE LOSS SENTENCE, AND WHEN THERE ISN'T ONE
    ///
    /// * **Rich had written something** — a partial reply, or a recorded action — so the
    ///   full statement from [`TurnLoss::ceo_message`] applies: what is on disk, what is
    ///   not, and what that means for asking again.
    /// * **Rich had written nothing, but the CEO's message is durable** — one short true
    ///   sentence about HIS words, which is the thing he actually wants to know. It says
    ///   nothing about an answer, because there was no answer.
    /// * **Nothing at all is on disk** — no sentence. Silence is the honest output; a
    ///   reassurance about nothing is what produced D2.
    pub fn new(cause: InterruptionCause, loss: &TurnLoss) -> InterruptionRecord {
        let rich_wrote_something = loss.partial_reply_chars > 0 || loss.actions_recorded > 0;
        let loss_message = if rich_wrote_something {
            Some(loss.ceo_message())
        } else if loss.prompt_is_durable {
            Some("Your message is saved. Rich hadn't written any of his answer yet.".to_string())
        } else {
            None
        };
        InterruptionRecord {
            cause: cause.tag().to_string(),
            ceo_message: cause.ceo_message().to_string(),
            loss_message,
            offers_retry: cause.offers_retry(),
        }
    }
}
