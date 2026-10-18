//! **`429` AND `529` ARE PRESENTED DIFFERENTLY — pinned to captured bytes.**
//!
//! Row 3.30's fifth answer: *"Quota exhaustion (`429`) and overload (`529`) look identical
//! to a user and are not. One clears on a schedule the operator can be told; the other
//! clears when it clears."*
//!
//! **The corpus is on disk, not in this file.** `include_str!` reaches into
//! `docs/verification/upstream-failure-2026-09-05/`, so deleting the evidence breaks the
//! build rather than quietly leaving a suite that tests a string somebody typed. Four of
//! the five `529` lines carry the exact request ids the wiki row names.
//!
//! **What this suite does NOT cover, stated rather than left to be found.** No `429` was
//! ever captured on this machine; `constructed-429.txt` is built from the vendor's own
//! message template (read out of the shipped bundle — see that folder's README) and its
//! PROSE is unverified. Every assertion here is therefore about the structural tokens the
//! vendor computes, never about its English. The test named
//! `the_classifier_does_not_read_the_vendors_english` is the proof of that claim rather
//! than a promise about it.

use richos_core::upstream::{RetryBudget, UpstreamFailure, UpstreamFault};

/// The verbatim bytes, from this machine's own transcripts on 2026-09-05.
const CAPTURED_529: &str =
    include_str!("../../../../../docs/verification/upstream-failure-2026-09-05/captured-529.txt");

/// CONSTRUCTED from the vendor's template. Structure verified, prose not.
const CONSTRUCTED_429: &str =
    include_str!("../../../../../docs/verification/upstream-failure-2026-09-05/constructed-429.txt");

/// The four request ids the wiki row names. Typed here ONCE, and the point of typing them
/// is that the fixture must contain them: if somebody swaps the evidence file for a
/// convenient synthetic one, this goes red.
const ROW_REQUEST_IDS: [&str; 4] = [
    "req_011Cegb417YK6i1BEVDFmzU1",
    "req_011CegbZnNQkV7nSHESJQXcq",
    "req_011CegbzZpYpdqtCRHTmEh9H",
    "req_011CegdeZDky4nwegbmqvQoV",
];

/// INVARIANT: every captured `529` line classifies as an overload, and the incident's own
/// request ids and models come back out of it.
///
/// The models matter as much as the ids: two `claude-fable-5-1` and one `claude-opus-5`
/// in this corpus is how the record knows the failure was not model-specific.
#[test]
fn every_captured_529_line_classifies_as_an_overload_and_keeps_its_request_id() {
    let lines: Vec<&str> = CAPTURED_529.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 5, "the captured corpus is five lines; the fixture changed");

    let mut ids = Vec::new();
    let mut models = Vec::new();
    for line in &lines {
        let f = UpstreamFailure::classify(line)
            .unwrap_or_else(|| panic!("a captured 529 line failed to classify: {line}"));
        assert_eq!(f.fault, UpstreamFault::Overloaded, "line: {line}");
        assert_eq!(f.status, Some(529));
        assert!(!f.fault.clears_on_a_known_schedule(), "an overload has no schedule");
        if let Some(id) = f.request_id {
            ids.push(id);
        }
        if let Some(m) = f.model {
            models.push(m);
        }
    }

    for want in ROW_REQUEST_IDS {
        assert!(ids.iter().any(|id| id == want), "request id {want} is not in the fixture");
    }
    assert_eq!(ids.len(), 4, "four of five captured lines carry a request id");
    assert_eq!(
        models.iter().filter(|m| *m == "claude-fable-5-1").count(),
        3,
        "the incident hit claude-fable-5-1 three times"
    );
    assert_eq!(
        models.iter().filter(|m| *m == "claude-opus-5").count(),
        1,
        "and claude-opus-5 once — which is how we know it was not model-specific"
    );
}

/// INVARIANT: the shortest real line — no suffix, no request id, no model — still
/// classifies. A classifier that needs the decorated form fails on a fifth of its own
/// corpus.
#[test]
fn the_undecorated_line_still_classifies() {
    let f = UpstreamFailure::classify("API Error: 529 Overloaded.").expect("must classify");
    assert_eq!(f.fault, UpstreamFault::Overloaded);
    assert_eq!(f.status, Some(529));
    assert_eq!(f.request_id, None);
    assert_eq!(f.model, None);
}

/// INVARIANT: a `429` is a DIFFERENT fault with a DIFFERENT sentence and a schedule.
///
/// This is the row's fifth answer in one assertion. Before this existed the two arrived
/// as the same `TurnError` string and the CEO had no way to tell "wait, it will come
/// back" from "wait until your window rolls over".
#[test]
fn a_429_is_a_different_fault_with_a_known_schedule() {
    let lines: Vec<&str> = CONSTRUCTED_429.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2, "the constructed corpus is two lines; the fixture changed");

    for line in &lines {
        let f = UpstreamFailure::classify(line)
            .unwrap_or_else(|| panic!("a 429 line failed to classify: {line}"));
        assert_eq!(f.fault, UpstreamFault::RateLimited, "line: {line}");
        assert_eq!(f.status, Some(429));
        assert!(f.fault.clears_on_a_known_schedule(), "quota exhaustion has a schedule");
    }

    let overload = UpstreamFailure::classify("API Error: 529 Overloaded.").unwrap();
    let quota = UpstreamFailure::classify(lines[0]).unwrap();
    assert_ne!(overload.fault.tag(),quota.fault.tag());
    assert!(!overload.fault.clears_on_a_known_schedule());
}

/// INVARIANT: classification reads the vendor's STRUCTURE, never its English.
///
/// The message body is replaced wholesale with `qqqq`. If a future release rewords
/// "Overloaded" to anything at all, the classifier still gets it right — and if somebody
/// later replaces this with a prose match, this test is what goes red.
#[test]
fn the_classifier_does_not_read_the_vendors_english() {
    let reworded = "API Error: 529 qqqq (error type server_error, HTTP 529, request id \
                    req_qqqq, model sent to the API: claude-opus-5)";
    let f = UpstreamFailure::classify(reworded).expect("structure alone must classify it");
    assert_eq!(f.fault, UpstreamFault::Overloaded);
    assert_eq!(f.request_id.as_deref(), Some("req_qqqq"));

    // And the same in the other direction: the WORD "overloaded" with no structural token
    // anywhere is not an API failure. Otherwise a CEO who types "the server is overloaded"
    // gets an outage notice.
    assert_eq!(UpstreamFailure::classify("the server is overloaded today"), None);
}

/// POSITIVE CONTROL. A guard that refuses everything is not a guard, so this proves the
/// classifier admits the cases it must and rejects only what it must.
#[test]
fn the_classifier_admits_real_faults_and_rejects_ordinary_local_failures() {
    // Admits.
    for (text, want) in [
        ("API Error: 529 Overloaded.", UpstreamFault::Overloaded),
        ("API Error: 429 Rate limit exceeded.", UpstreamFault::RateLimited),
        ("API Error: 503 Service unavailable.", UpstreamFault::ServerError),
        ("API Error: 401 Invalid API key · Please run /login", UpstreamFault::Unclassified),
    ] {
        let f = UpstreamFailure::classify(text).unwrap_or_else(|| panic!("must classify: {text}"));
        assert_eq!(f.fault, want, "{text}");
    }
    // Rejects — every one of these is a REAL failure with its own vocabulary, and calling
    // any of them an outage would tell the CEO to wait for something that will never come.
    for text in [
        "cognition io: broken pipe",
        "claude protocol error: no result frame",
        "the claude binary was not found at /usr/local/bin/claude",
        "claude failed to start (exit 1); the child said: error: unknown option",
        "ledger write failed: No space left on device",
    ] {
        assert_eq!(UpstreamFailure::classify(text), None, "must not classify: {text}");
    }
}

/// INVARIANT: a stderr TAIL is classified line by line, so one `529` in a 64-line buffer
/// is found and a neighboring unrelated line cannot change its verdict.
#[test]
fn a_stderr_tail_is_classified_line_by_line() {
    let tail = "node:internal/process warning\n\
                some unrelated diagnostic\n\
                API Error: 529 Overloaded. (error type server_error, HTTP 529)\n\
                error: unknown option '--nope'\n";
    let f = UpstreamFailure::classify_lines(tail).expect("the 529 line must be found");
    assert_eq!(f.fault, UpstreamFault::Overloaded);
    // NEGATIVE SIDE: a tail with no API error at all classifies as nothing.
    assert_eq!(
        UpstreamFailure::classify_lines("warning: something\nerror: unknown option '--nope'\n"),
        None
    );
}


// =========================================================================================
// Retry
// =========================================================================================

/// INVARIANT: retry is BOUNDED. An overload follows the CEO's schedule (2026-10-07) — seven
/// retries after 1, 2, 5, 10, 20, 40 and 80 minutes — and then stops, whatever the caller asks
/// for. Any other fault keeps its one immediate retry.
#[test]
fn retry_is_bounded_and_the_ceiling_holds_against_a_caller_that_keeps_asking() {
    let mut b = RetryBudget::new();
    let mut waits = Vec::new();
    for _ in 0..7 {
        assert!(b.may_retry(UpstreamFault::Overloaded));
        assert!(b.charge(UpstreamFault::Overloaded), "each of the first seven buys a retry");
        waits.push(b.wait_before_retry().unwrap().as_secs() / 60);
    }
    assert_eq!(waits, [1, 2, 5, 10, 20, 40, 80], "his schedule, in minutes");
    assert!(!b.may_retry(UpstreamFault::Overloaded), "and there is no eighth");
    for _ in 0..10 {
        assert!(!b.charge(UpstreamFault::Overloaded), "the ceiling does not move");
        assert_eq!(b.wait_before_retry(), None);
    }
    assert_eq!(b.retries_spent(), 7);
    assert_eq!(b.attempts(), 17, "every attempt is still counted, so the CEO can be told");

    let mut b = RetryBudget::new();
    assert!(b.charge(UpstreamFault::ServerError), "a server error keeps its one retry");
    assert_eq!(b.wait_before_retry(), Some(std::time::Duration::ZERO), "immediate, as before");
    assert!(!b.charge(UpstreamFault::ServerError), "and only one");
}

/// INVARIANT: a `429` buys NO retry at all — its window rolls over in hours, so an
/// immediate retry is pure cost.
#[test]
fn quota_exhaustion_buys_no_retry_because_its_schedule_answers_the_question() {
    let mut b = RetryBudget::new();
    assert!(!b.charge(UpstreamFault::RateLimited));
    assert!(!b.may_retry(UpstreamFault::RateLimited));
    assert_eq!(b.wait_before_retry(), None);
    assert_eq!(b.attempts(), 1);
}

/// POSITIVE CONTROL: the budget is restored by a SUCCESS, so a healthy week does not
/// leave the app with no allowance on the day it matters.
#[test]
fn a_successful_turn_restores_the_allowance() {
    let mut b = RetryBudget::new();
    b.charge(UpstreamFault::ServerError);
    assert!(!b.may_retry(UpstreamFault::ServerError));
    b.succeeded();
    assert!(b.may_retry(UpstreamFault::ServerError), "a completed turn is a positive signal that the upstream works");
    assert_eq!(b.attempts(), 0);
}


// =========================================================================================
// What survived
// =========================================================================================
