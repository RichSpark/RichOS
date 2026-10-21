//! **WHY A TURN ENDED — one class per test, and a positive control for the transient arm.**
//!
//! `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D2. The
//! published nightly held `cognition protocol: "Not logged in · Please run /login"` in its
//! own ledger and told the CEO it had hit a snag, that his work was saved, and that pressing
//! a button would pick it back up. All three were false.
//!
//! Every reason string below is the REAL one: either a vendor constant read verbatim out of
//! the installed Claude Code bundle (`~/.local/share/claude/versions/2.1.274`, 2026-09-17),
//! or a sentence RichOS itself writes, wrapped in the `CognitionError` `Display` that
//! `spine.rs` passes to the ledger.
//!
//! **There is no American-English test here and that is deliberate**: `guard-dialect.sh`
//! refuses the WRITE of a non-American word anywhere in this repository, which is a stronger
//! guarantee than a test that has to be remembered, and a test enumerating the words it
//! looks for would itself be refused by that guard.

use richos_core::interruption::{classify, InterruptionCause, InterruptionRecord};
use richos_core::upstream::TurnLoss;

/// The exact string the nightly recorded, reproduced from the audit.
const THE_NIGHTLY_REASON: &str = "cognition protocol: \"Not logged in · Please run /login\"";

fn nothing_written() -> TurnLoss {
    TurnLoss {
        prompt_is_durable: true,
        partial_reply_chars: 0,
        actions_recorded: 0,
        lease_context_lost: true,
    }
}

// ===========================================================================================
// CLASSIFICATION — one arm at a time
// ===========================================================================================

/// **THE NIGHTLY'S D2, AS A VALUE.** The reason the app was holding, classified.
#[test]
fn the_reason_the_nightly_recorded_is_classified_as_a_missing_sign_in() {
    assert_eq!(classify(THE_NIGHTLY_REASON), InterruptionCause::NotSignedIn);
}

/// The four sibling vendor constants are a DIFFERENT problem with a different actor: a
/// credential is configured and was turned down. Telling the CEO to reconnect his account
/// would send him to a screen that cannot help.
#[test]
fn a_rejected_credential_is_not_the_same_class_as_a_missing_sign_in() {
    for vendor in [
        "Invalid API key · Fix external API key",
        "Invalid auth token · Fix external auth token",
        "Invalid ANTHROPIC_CUSTOM_HEADERS · Fix the environment variable",
        "Invalid request header from the environment · Fix the environment variable",
    ] {
        let reason = format!("cognition protocol: \"{vendor}\"");
        assert_eq!(classify(&reason), InterruptionCause::CredentialRejected, "{vendor}");
    }
}

/// The vendor ships one string carrying BOTH families' wording. It is a rejected credential,
/// and the ordering in `classify` is what makes it resolve that way rather than sending him
/// to the account screen.
#[test]
fn the_vendors_combined_401_string_resolves_to_the_credential_and_not_the_sign_in() {
    let reason = "cognition protocol: \"API Error: 401 Invalid API key · Please run /login\"";
    assert_eq!(classify(reason), InterruptionCause::CredentialRejected);
}

/// **THE POSITIVE CONTROL FOR THE TRANSIENT ARM.** Without this the suite could pass with a
/// `classify` that answered `Unknown` to everything it did not specifically name, and the
/// one arm where retrying IS the right advice would never be exercised.
#[test]
fn an_io_failure_is_transient_and_says_asking_again_is_worth_a_try() {
    let cause = classify("cognition io: broken pipe");
    assert_eq!(cause, InterruptionCause::Transient);
    assert!(cause.offers_retry(), "the one arm where a retry control belongs");
    assert!(cause.ceo_message().contains("send it again"), "{}", cause.ceo_message());
}

/// **THE 2026-09-17 CANDIDATE-WALK DEFECT, AS A VALUE.** `provider-supervisor.py`'s own
/// ENOENT text, wrapped exactly the way `native.rs::NativeError::Startup` and
/// `CognitionError::Io` actually wrap it before it reaches the ledger — carrying
/// `OUR_IO_PREFIX` too, which is what made this misclassify as `Transient` before this fix
/// (measured on the candidate walk: `[Errno 2] No such file or directory`, an ENOENT that
/// `native.rs`'s own `BinaryMissing` mapping never sees, because it fires inside a forked
/// child after `Command::spawn` already succeeded spawning the supervisor).
#[test]
fn a_missing_provider_binary_is_never_classified_as_transient() {
    let reason = "cognition io: claude failed to start (the child exited before answering \
                  the handshake); the child said: Provider could not start: [Errno 2] No \
                  such file or directory";
    let cause = classify(reason);
    assert_eq!(cause, InterruptionCause::ProviderMissing, "{reason}");
    assert!(!cause.offers_retry(), "a missing binary cannot be fixed by asking again");
    assert!(
        !cause.ceo_message().to_lowercase().contains("usually clears on its own"),
        "{}",
        cause.ceo_message()
    );
}

/// A stop is not a failure and must never be dressed as one.
#[test]
fn a_stop_the_ceo_asked_for_is_classified_as_his_own_stop() {
    let cause = classify("cognition protocol: Connection stopped at your request.");
    assert_eq!(cause, InterruptionCause::StoppedByCeo);
    assert!(!cause.offers_retry(), "nothing failed, so there is nothing to retry");
}

/// **A REWORDED VENDOR STRING FALLS TO UNKNOWN, AND THAT IS THE SAFE DIRECTION.** It never
/// becomes whichever arm looked closest — it says plainly that RichOS does not know, and
/// offers no route that might be wrong.
#[test]
fn a_reason_nobody_anticipated_is_named_unknown_rather_than_guessed() {
    let cause = classify("cognition protocol: \"qqqq\"");
    assert_eq!(cause, InterruptionCause::Unknown);
    assert!(
        !cause.ceo_message().contains("Settings"),
        "an unknown cause must not send him to a screen that might not help"
    );
    assert!(cause.ceo_message().contains("can't tell you why"), "{}", cause.ceo_message());
}

// ===========================================================================================
// WHAT HE IS TOLD — the three untruths, each made unreachable
// ===========================================================================================

/// **UNTRUTH 1: a permanent condition described as a passing hiccup.** The sign-in sentence
/// names the account connection, which is what the app already knows is wrong.
///
/// **UNTRUTH 3: a retry that cannot succeed.** No retry control on this class, ever.
///
/// And the route it offers is the one that exists, Settings, under Claude accounts (round 18),
/// which the audit's third finding said the app never mentioned despite having it.
#[test]
fn the_sign_in_case_names_the_account_offers_the_route_that_exists_and_no_retry() {
    let cause = InterruptionCause::NotSignedIn;
    let message = cause.ceo_message();

    assert!(message.contains("Claude account"), "{message}");
    assert!(!cause.offers_retry(), "a retry here cannot succeed however often it is pressed");
    assert!(
        message.contains("Settings") && message.contains("Claude accounts"),
        "the route the app actually has is not offered: {message}"
    );

    // It must not reach for the vocabulary the nightly used.
    assert!(!message.contains("snag"), "{message}");
    assert!(!message.to_lowercase().contains("pick it back up"), "{message}");
}

/// **UNTRUTH 2: a promise about saved work that does not exist.**
///
/// Nothing of Rich's was written, so the only true thing to say is about the CEO's own
/// words. The sentence the audit caught must be unreachable.
#[test]
fn nothing_written_means_no_claim_about_a_saved_answer_is_made() {
    let record = InterruptionRecord::new(InterruptionCause::NotSignedIn, &nothing_written());

    assert_eq!(
        record.loss_message.as_deref(),
        Some("Your message is saved. Rich hadn't written any of his answer yet."),
        "the only true thing to say here is about HIS words"
    );
    let whole = format!("{} {:?}", record.ceo_message, record.loss_message);
    assert!(
        !whole.contains("Everything I'd already written above is saved"),
        "the sentence the audit caught is reachable again: {whole}"
    );
    assert!(!whole.contains("already written above"), "{whole}");
    assert!(!record.offers_retry);
}

/// With nothing at all on disk — not even his message — the card says nothing about loss.
#[test]
fn an_empty_turn_makes_no_statement_about_what_survived() {
    let loss = TurnLoss {
        prompt_is_durable: false,
        partial_reply_chars: 0,
        actions_recorded: 0,
        lease_context_lost: true,
    };
    let record = InterruptionRecord::new(InterruptionCause::Unknown, &loss);
    assert_eq!(record.loss_message, None, "{record:?}");
}

/// When Rich HAD written something, the full statement applies — built from the same counts
/// the upstream path uses, so the two surfaces cannot come to disagree about what survived.
#[test]
fn a_partial_answer_is_reported_with_the_counts_that_are_actually_on_disk() {
    let loss = TurnLoss {
        prompt_is_durable: true,
        partial_reply_chars: 169,
        actions_recorded: 2,
        lease_context_lost: true,
    };
    let record = InterruptionRecord::new(InterruptionCause::Transient, &loss);
    let message = record.loss_message.expect("something was written, so say so");
    assert!(message.contains("169"), "{message}");
    assert!(message.contains("2 recorded action"), "{message}");
    assert!(message.contains("Not on disk"), "{message}");
    assert!(record.offers_retry);
}

// ===========================================================================================
// THE RECORD
// ===========================================================================================

/// The stored record round-trips, and an unrecognized tag reads back as `Unknown` rather
/// than as whichever arm happens to be first — the forward-compatible direction for a record
/// written by a newer RichOS.
#[test]
fn the_record_round_trips_and_an_unknown_tag_degrades_safely() {
    let record = InterruptionRecord::new(InterruptionCause::NotSignedIn, &nothing_written());
    let json = serde_json::to_string(&record).unwrap();
    let back: InterruptionRecord = serde_json::from_str(&json).unwrap();
    assert_eq!(record, back);
    assert_eq!(InterruptionCause::from_tag(&back.cause), InterruptionCause::NotSignedIn);

    assert_eq!(InterruptionCause::from_tag("something-from-2027"), InterruptionCause::Unknown);
}

/// Every arm has its own sentence. A shared arm would be a place where two different
/// situations get one explanation, which is the defect this module exists to end.
#[test]
fn no_two_causes_share_a_sentence() {
    let all = [
        InterruptionCause::NotSignedIn,
        InterruptionCause::CredentialRejected,
        InterruptionCause::ProviderMissing,
        InterruptionCause::StoppedByCeo,
        InterruptionCause::Transient,
        InterruptionCause::Repeated,
        InterruptionCause::Unknown,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a.ceo_message(), b.ceo_message(), "{a:?} and {b:?}");
            assert_ne!(a.tag(), b.tag(), "{a:?} and {b:?}");
        }
    }
}

/// **SPOKEN-SAFE.** RichOS is voice-first, so every one of these sentences is read aloud as
/// often as it is read. None may carry a path, an error code, a bracketed aside or
/// punctuation doing structural work — a sentence that needs a screen to parse is a sentence
/// that fails half the time it is delivered.
#[test]
fn every_sentence_survives_being_spoken() {
    for cause in [
        InterruptionCause::NotSignedIn,
        InterruptionCause::CredentialRejected,
        InterruptionCause::ProviderMissing,
        InterruptionCause::StoppedByCeo,
        InterruptionCause::Transient,
        InterruptionCause::Repeated,
        InterruptionCause::Unknown,
    ] {
        // THE ARROW IS ON THE FORBIDDEN LIST BECAUSE THE ROUTE IS IN THE SENTENCE NOW.
        // `Settings → Account connection` was the first form of the sign-in line, and an
        // arrow is read aloud as nothing at all in a product that is answered by voice.
        for m in [cause.ceo_message()] {
            for forbidden in ['/', '\\', '{', '}', '(', ')', '[', ']', '_', '*', '`', '<', '>', '→'] {
                assert!(!m.contains(forbidden), "{cause:?} carries {forbidden:?}: {m}");
            }
            assert!(!m.contains("cognition"), "machinery leaked into CEO copy: {m}");
            assert!(!m.contains("HTTP"), "{m}");
            assert!(m.ends_with('.'), "{cause:?} is not a finished sentence: {m}");
        }
    }
}

/// **NO SENTENCE PROMISES THAT A FAILURE CLEARS BY ITSELF** (2026-10-21). "This usually fixes
/// itself" was the lost-connection line, and a new user was shown it on every message of a Mac
/// where it never did. A lost connection is told once as what happened, with the offer to send
/// again; the same failure again is `Repeated`, which invites no further try.
#[test]
fn no_cause_promises_that_it_clears_by_itself() {
    for cause in [
        InterruptionCause::NotSignedIn,
        InterruptionCause::CredentialRejected,
        InterruptionCause::ProviderMissing,
        InterruptionCause::StoppedByCeo,
        InterruptionCause::Transient,
        InterruptionCause::Repeated,
        InterruptionCause::Unknown,
    ] {
        let m = cause.ceo_message().to_lowercase();
        for promise in ["fixes itself", "usually clears", "clears on its own", "should work now"] {
            assert!(!m.contains(promise), "{cause:?} promises it clears: {m}");
        }
    }
    let repeated = InterruptionCause::Repeated;
    assert!(!repeated.offers_retry(), "a failure that repeats is not offered as a retry");
    assert!(repeated.ceo_message().contains("Claude accounts"), "it names where to look");
    assert_eq!(InterruptionCause::from_tag(repeated.tag()), repeated);
}

/// **A CHILD THAT ENDED MID-TURN, WITH CLAUDE CODE SAYING NOBODY IS SIGNED IN**, is told as
/// the sign-in it is (`native.rs`, `NativeError::Exited`, which carries
/// `CHILD_EXIT_SIGNED_OUT`), never as a lost connection. The exact reason `spine.rs` records.
#[test]
fn a_child_exit_with_no_sign_in_is_not_signed_in() {
    let reason = format!(
        "cognition io: claude stopped before answering; it wrote nothing on stderr; {}",
        richos_core::native::CHILD_EXIT_SIGNED_OUT
    );
    assert_eq!(classify(&reason), InterruptionCause::NotSignedIn);
    // The same end without that verdict stays the lost connection it is.
    assert_eq!(
        classify("cognition io: claude stopped before answering; it wrote nothing on stderr"),
        InterruptionCause::Transient
    );
    // And the child's own words decide when they name the cause.
    assert_eq!(
        classify("cognition io: claude stopped before answering; the child said: Invalid API key · Fix external API key"),
        InterruptionCause::CredentialRejected
    );
}

/// **AN OAUTH SIGN-IN ANTHROPIC NO LONGER ACCEPTS** is a sign-in to renew, measured on Claude
/// Code 2.1.306 (2026-10-21, `CLAUDE_CODE_OAUTH_TOKEN` set to a token Anthropic does not know):
/// its `result` was exactly the string below. It fell to `Unknown` and its retry control.
#[test]
fn an_oauth_token_anthropic_refuses_is_a_sign_in_to_renew() {
    let reason = "cognition protocol: \"Failed to authenticate. API Error: 401 OAuth access token is invalid.\"";
    assert_eq!(classify(reason), InterruptionCause::NotSignedIn);
}
