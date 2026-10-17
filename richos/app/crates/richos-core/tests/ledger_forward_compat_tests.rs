//! **NOTHING ALREADY READABLE MAY BECOME LESS READABLE.**
//!
//! This suite exists to hold one line, and it holds it with a recording rather than an
//! argument: `tests/fixtures/ledgers/*.golden` is a content-free digest of the projection
//! that `Ledger::open` built from those exact bytes at `ccaaf00` — the reader that shipped
//! in v1.0.0, v1.0.1 and v1.0.2. Every one of those three builds is still published and
//! still downloadable. If a change to how a ledger is read alters one character of a
//! golden, it has changed what a customer's history says, and this suite fails.
//!
//! The fixtures are synthetic (see `fixtures/ledgers/README.md`) but their SHAPES are not
//! invented: each record shape was read off the five real `conversation-ledger.jsonl`
//! files on the author's machine and reproduced field-for-field with the content replaced.
//! A public repository is no place for a real ledger, even a hashed one.

#[path = "support/ledger_digest.rs"]
mod ledger_digest;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ledgers")
}

/// A private directory per call, and the key is a COUNTER rather than a clock.
///
/// **A clock read is not a unique key.** `SystemTime::now()` on this machine ticks at
/// exactly 1000 ns — measured, min tick and median tick both 1000 ns over 200,000 reads —
/// so `as_nanos()` carries microsecond resolution and nothing finer. In a 12-thread probe,
/// **206,474 of 240,000 samples (86.0 %) took a value another thread had already taken.**
///
/// This binary runs its 20 tests in parallel in ONE process, so `process::id()` is a
/// constant here, and 18 of the 23 call sites below pass the same `name` (`v1-current`).
/// That left a bare microsecond tick as the only thing separating two tests' `edited.jsonl`
/// — and it did not separate them. Seen live: `a_skipped_record_is_reported_in_words_the_ceo_can_read`
/// failed with `skipped` 0 against an expected 1, having opened a file another test wrote,
/// while the same binary run alone passed 20/20 ten times over.
///
/// The false RED is the cheap half. A test reading a state some other test happened to
/// write can just as easily report a property it never established — a false GREEN, in the
/// harness whose whole job is proving that a customer's history still reads the same.
fn scratch(name: &str) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "richos-fwdcompat-{}-{}-{name}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Digest one fixture and compare it, line by line, against the golden beside it.
fn assert_matches_golden(stem: &str) {
    let src = fixtures().join(format!("{stem}.jsonl"));
    let golden_path = fixtures().join(format!("{stem}.golden"));
    let dir = scratch(stem);

    let mut actual = vec![format!("=== ledger {} ===", ledger_digest::label(&src))];
    actual.extend(
        ledger_digest::digest_ledger(&src, &dir.join("copy.jsonl"))
            .unwrap_or_else(|e| panic!("{stem} did not replay at all: {e}")),
    );

    let golden = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|e| panic!("golden {}: {e}", golden_path.display()));
    let expected: Vec<&str> = golden.lines().collect();

    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            a,
            e,
            "\n{stem}: line {} of the projection changed.\n  the shipped reader produced: {e}\n  \
             this build produced:         {a}\nA customer on v1.0.0-v1.0.2 reads these bytes \
             with the recorded behavior. Changing it changes their history.",
            i + 1
        );
    }
    assert_eq!(
        actual.len(),
        expected.len(),
        "{stem}: the projection has {} lines and the shipped reader produced {}",
        actual.len(),
        expected.len()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_record_shape_the_shipped_builds_wrote_still_replays_identically() {
    assert_matches_golden("v1-current");
}

#[test]
fn every_legacy_record_shape_still_replays_identically() {
    assert_matches_golden("v1-legacy");
}

/// The digest is only evidence if it is deterministic. Two runs over the same bytes must
/// agree, or a passing golden comparison proves nothing about the run after it.
#[test]
fn the_digest_is_deterministic_across_runs() {
    let src = fixtures().join("v1-legacy.jsonl");
    let a = ledger_digest::digest_ledger(&src, &scratch("det-a").join("copy.jsonl")).unwrap();
    let b = ledger_digest::digest_ledger(&src, &scratch("det-b").join("copy.jsonl")).unwrap();
    assert_eq!(a, b, "the same bytes digested twice must give the same answer");
}

/// The tool is run against real ledgers on a real machine. It must never open the
/// original for writing, and it must leave the original byte-for-byte and
/// modification-time untouched.
#[test]
fn digesting_a_ledger_does_not_touch_the_original() {
    let dir = scratch("readonly");
    let original = dir.join("customer.jsonl");
    let bytes = std::fs::read(fixtures().join("v1-current.jsonl")).unwrap();
    std::fs::write(&original, &bytes).unwrap();
    let before_len = std::fs::metadata(&original).unwrap().len();
    let before_mtime = std::fs::metadata(&original).unwrap().modified().unwrap();

    ledger_digest::digest_ledger(&original, &dir.join("copy.jsonl")).unwrap();

    let after = std::fs::read(&original).unwrap();
    assert_eq!(after, bytes, "the original ledger's bytes changed");
    assert_eq!(std::fs::metadata(&original).unwrap().len(), before_len);
    assert_eq!(
        std::fs::metadata(&original).unwrap().modified().unwrap(),
        before_mtime,
        "the original ledger was written to"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// =============================================================================================
// PART 2 — THE TOLERANT READER
//
// Two halves, and both are load-bearing:
//
//   * a ledger holding a record this build cannot read must load EVERYTHING ELSE and SAY
//     that it skipped something; and
//   * an unmodified ledger must skip NOTHING. A suite that passed because the reader now
//     skips everything would be exactly the failure this work exists to fix, wearing a
//     green tick. `nothing_at_all_is_skipped_on_an_unmodified_ledger` is what stops that,
//     and every test below asserts `records_applied` as well as `skipped`.
// =============================================================================================

use richos_core::entity::EntityId;
use richos_core::ledger::{Event, Ledger, SkipKind, KNOWN_EVENT_TAGS};
use richos_core::ledger::Source;

/// Copy a fixture into scratch, edit its lines, and open the copy.
fn open_edited(stem: &str, edit: impl FnOnce(Vec<String>) -> Vec<String>) -> (Ledger, PathBuf) {
    let raw = std::fs::read_to_string(fixtures().join(format!("{stem}.jsonl"))).unwrap();
    let edited = edit(raw.lines().map(str::to_string).collect());
    let path = scratch(stem).join("edited.jsonl");
    std::fs::write(&path, format!("{}\n", edited.join("\n"))).unwrap();
    (Ledger::open(&path).expect("a ledger must still open"), path)
}

/// The digest with the three `file.*` lines dropped — those describe the BYTES, which change
/// the moment a test edits the file. Everything after them describes the CONVERSATION, which
/// is the thing that must not change.
fn projection_lines(path: &Path, stem: &str) -> Vec<String> {
    ledger_digest::digest_ledger(path, &scratch(stem).join("copy.jsonl"))
        .unwrap()
        .into_iter()
        .filter(|l| !l.starts_with("file."))
        .collect()
}

fn golden_projection_lines(stem: &str) -> Vec<String> {
    std::fs::read_to_string(fixtures().join(format!("{stem}.golden")))
        .unwrap()
        .lines()
        .skip(1) // the "=== ledger … ===" header
        .filter(|l| !l.starts_with("file."))
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The tag table is the only thing that can tell the future from damage. It must not drift.
// ---------------------------------------------------------------------------------------------

/// EXHAUSTIVE on purpose. Adding a variant to `Event` without adding its name to
/// `KNOWN_EVENT_TAGS` fails to COMPILE here — which is the point, because a missing entry
/// would make this build report a genuinely DAMAGED record of that type as a benign one
/// from the future.
fn tag_of(e: &Event) -> &'static str {
    match e {
        Event::ThreadCreated { .. } => "ThreadCreated",
        Event::ThreadEntityBound { .. } => "ThreadEntityBound",
        Event::PromptReceived { .. } => "PromptReceived",
        Event::TurnStarted { .. } => "TurnStarted",
        Event::AssistantDelta { .. } => "AssistantDelta",
        Event::TurnCompleted { .. } => "TurnCompleted",
        Event::TurnInterrupted { .. } => "TurnInterrupted",
        Event::UpstreamFailure { .. } => "UpstreamFailure",
        Event::TurnInterruptionCause { .. } => "TurnInterruptionCause",
        Event::TurnStopped { .. } => "TurnStopped",
        Event::ActionRecorded { .. } => "ActionRecorded",
        Event::ActionUpdated { .. } => "ActionUpdated",
        Event::SessionRotated { .. } => "SessionRotated",
        Event::ProactiveMessage { .. } => "ProactiveMessage",
        Event::HandoffSummaryUpdated { .. } => "HandoffSummaryUpdated",
        Event::TurnSuperseded { .. } => "TurnSuperseded",
        Event::TurnReplayed { .. } => "TurnReplayed",
    }
}

/// Historical fixtures plus the atomic-replay fixture carry every variant. Historical
/// fixture bytes and their shipped-reader goldens remain unchanged.
#[test]
fn the_known_tag_table_matches_the_event_type_exactly() {
    let mut seen: Vec<String> = Vec::new();
    for stem in ["v1-current", "v1-legacy", "v1-replay"] {
        let raw = std::fs::read_to_string(fixtures().join(format!("{stem}.jsonl"))).unwrap();
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let event: Event = serde_json::from_str(line).expect("fixture record parses");
            let tag = tag_of(&event);
            // The tag the exhaustive match names is the tag serde actually writes.
            let re = serde_json::to_value(&event).unwrap();
            assert_eq!(re.get("event").unwrap().as_str().unwrap(), tag);
            assert!(KNOWN_EVENT_TAGS.contains(&tag), "{tag} is missing from KNOWN_EVENT_TAGS");
            if !seen.iter().any(|s| s == tag) {
                seen.push(tag.to_string());
            }
        }
    }
    seen.sort();
    let mut table: Vec<String> = KNOWN_EVENT_TAGS.iter().map(|s| s.to_string()).collect();
    table.sort();
    assert_eq!(
        seen, table,
        "the fixtures must carry one of every Event variant, and KNOWN_EVENT_TAGS must hold \
         exactly those names — no more, no fewer"
    );
}

// ---------------------------------------------------------------------------------------------
// THE ANTI-VACUOUS TEST. Everything below is worthless without it.
// ---------------------------------------------------------------------------------------------

#[test]
fn nothing_at_all_is_skipped_on_an_unmodified_ledger() {
    for (stem, records) in [("v1-current", 20usize), ("v1-legacy", 26usize)] {
        let (ledger, _) = open_edited(stem, |l| l);
        let h = ledger.history_health();
        assert!(h.is_clean(), "{stem}: {:?}", ledger.skipped_records());
        assert_eq!(h.records_read, records, "{stem}: record count");
        assert_eq!(
            h.records_applied, records,
            "{stem}: every record must be APPLIED, not skipped"
        );
        assert_eq!(h.skipped, 0);
        assert_eq!(h.headline, "", "a clean history says nothing at all");
        assert_eq!(h.detail, "");
    }
}

// ---------------------------------------------------------------------------------------------
// A RECORD FROM THE FUTURE
// ---------------------------------------------------------------------------------------------

/// The whole reason this exists. A newer RichOS writes a record type an older build cannot
/// name; the older build loads the rest of the conversation anyway.
#[test]
fn a_record_from_the_future_does_not_take_the_rest_of_the_history_with_it() {
    // Inserted in the MIDDLE, between a turn's two deltas — not appended at the end,
    // because "everything BEFORE the bad line loaded" is not the claim being made.
    let future = r#"{"event":"ManagedSourceAssigned","turn_id":"turn_00000000000000000000000000000003","source":"managed","assigned_by":"rich","at":1787975161005,"binding_revision":2}"#;
    let (ledger, path) = open_edited("v1-current", |mut lines| {
        lines.insert(11, future.to_string());
        lines
    });

    let h = ledger.history_health();
    assert_eq!(h.skipped, 1);
    assert_eq!(h.from_future, 1);
    assert_eq!(h.damaged, 0);
    assert_eq!(h.ambiguous, 0);
    assert_eq!(h.records_read, 21);
    assert_eq!(h.records_applied, 20, "every record this build knows still loaded");

    let r = &ledger.skipped_records()[0];
    assert_eq!(r.line, 12, "1-based line number, so an operator can go and look");
    assert_eq!(r.kind, SkipKind::FromFuture);
    assert_eq!(r.tag.as_deref(), Some("ManagedSourceAssigned"));

    // AND the conversation is the one the shipped reader produced, line for line.
    assert_eq!(
        projection_lines(&path, "future-mid"),
        golden_projection_lines("v1-current"),
        "a record from the future changed the projection of every OTHER record"
    );
}

/// Skipping must never be silent, and it must be visible where it counts: a structured
/// record a caller can act on, and a sentence the window can render.
#[test]
fn a_skipped_record_is_reported_in_words_the_ceo_can_read() {
    let (ledger, _) = open_edited("v1-current", |mut lines| {
        lines.push(r#"{"event":"Nudged","at":1788223893900}"#.to_string());
        lines
    });
    let h = ledger.history_health();
    assert_eq!(h.skipped, 1);
    assert_eq!(
        h.headline, "Part of this conversation was written by a newer version of RichOS.",
        "the calm headline: a record from the future is expected, not an incident"
    );
    assert!(h.detail.contains("1 record was written by a newer version of RichOS"), "{}", h.detail);
    assert!(h.detail.contains("Updating will bring it back."), "{}", h.detail);
    assert!(h.detail.contains("Everything else loaded: 20 of 21 records."), "{}", h.detail);
    assert!(h.detail.contains("Nothing was deleted"), "{}", h.detail);
    // No stack trace, no parser jargon, no file path in what the CEO reads.
    for word in ["serde", "Err(", "panic", "unwrap", ".jsonl", "JSON"] {
        assert!(!h.detail.contains(word), "the CEO's sentence contains {word:?}: {}", h.detail);
        assert!(!h.headline.contains(word), "the headline contains {word:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// DAMAGE IS NOT THE FUTURE
// ---------------------------------------------------------------------------------------------

/// The exact shape of a crash during an append: the last record is half written.
#[test]
fn a_torn_final_record_is_reported_as_damage_and_costs_only_itself() {
    let (ledger, path) = open_edited("v1-current", |mut lines| {
        let last = lines.pop().unwrap();
        lines.push(last[..last.len() / 2].to_string());
        lines
    });
    let h = ledger.history_health();
    assert_eq!(h.damaged, 1);
    assert_eq!(h.from_future, 0, "a torn write is NOT a record from the future");
    assert_eq!(h.records_read, 20);
    assert_eq!(h.records_applied, 19);
    assert_eq!(ledger.skipped_records()[0].kind, SkipKind::Damaged);
    assert!(ledger.skipped_records()[0].detail.contains("not valid JSON"));
    assert_eq!(ledger.skipped_records()[0].tag, None);
    assert_eq!(
        h.headline, "One record of this conversation could not be read.",
        "damage gets a different sentence from a record written by a newer version"
    );

    // Every turn is still there. Only the torn record's own effect is missing.
    let after = projection_lines(&path, "torn");
    let before = golden_projection_lines("v1-current");
    assert_eq!(
        after.iter().filter(|l| l.starts_with("turn ")).count(),
        before.iter().filter(|l| l.starts_with("turn ")).count(),
        "a turn disappeared over one torn record"
    );
}

/// The discriminator that stops corruption being waved through as "the future": a tag that
/// is not a plain identifier cannot have come from any build of RichOS.
#[test]
fn a_tag_that_no_build_could_have_written_is_damage_not_the_future() {
    let long = "x".repeat(65);
    let bad_tags = ["", "two words", "Turn-Started", "\u{1F600}", "9Leading", long.as_str()];
    for bad_tag in bad_tags {
        let line = format!(r#"{{"event":{},"at":1}}"#, serde_json::to_string(bad_tag).unwrap());
        let (ledger, _) = open_edited("v1-current", |mut lines| {
            lines.push(line.clone());
            lines
        });
        let r = &ledger.skipped_records()[0];
        assert_eq!(r.kind, SkipKind::Damaged, "tag {bad_tag:?} was let through as the future");
        assert_eq!(r.tag, None, "a tag that is not an identifier is never kept");
        assert_eq!(ledger.history_health().records_applied, 20);
    }
}

#[test]
fn json_that_is_not_a_record_at_all_is_damage() {
    for junk in ["[1,2,3]", "\"a bare string\"", "17", "null", "{", "not json at all"] {
        let (ledger, _) = open_edited("v1-current", |mut lines| {
            lines.push(junk.to_string());
            lines
        });
        assert_eq!(ledger.skipped_records()[0].kind, SkipKind::Damaged, "{junk}");
        assert_eq!(ledger.history_health().records_applied, 20, "{junk}");
    }
}

/// `lines()` yields `Err` for a line that is not valid UTF-8, and the old `?` turned one bad
/// byte into a total failure. It is a skipped record now — and, critically, the reader keeps
/// going PAST it.
#[test]
fn a_line_that_is_not_valid_utf8_is_skipped_rather_than_ending_the_replay() {
    let path = scratch("utf8").join("edited.jsonl");
    let mut bytes = std::fs::read(fixtures().join("v1-current.jsonl")).unwrap();
    bytes.extend_from_slice(b"{\"event\":\"TurnStarted\",\"turn_id\":\"\xff\xfe\"}\n");
    bytes.extend_from_slice(
        br#"{"event":"AssistantDelta","turn_id":"turn_00000000000000000000000000000004","text":"after the bad bytes","at":1788223894000,"seq":1}"#,
    );
    bytes.push(b'\n');
    std::fs::write(&path, &bytes).unwrap();

    let ledger = Ledger::open(&path).expect("a bad byte must not stop the file opening");
    let h = ledger.history_health();
    assert_eq!(h.damaged, 1);
    assert_eq!(ledger.skipped_records()[0].kind, SkipKind::Damaged);
    assert!(ledger.skipped_records()[0].detail.contains("not valid UTF-8"));
    assert_eq!(h.records_applied, 21, "the record AFTER the bad bytes still loaded");
}

// ---------------------------------------------------------------------------------------------
// THE CASE THE FORMAT CANNOT DECIDE — and it says so rather than picking
// ---------------------------------------------------------------------------------------------

#[test]
fn a_known_record_with_an_unknown_shape_is_reported_as_undetermined() {
    // A newer RichOS that made `session_id` optional would write this. So would a
    // `TurnStarted` whose bytes were mangled in place. Nothing in the file tells them apart.
    let (ledger, _) = open_edited("v1-current", |mut lines| {
        lines.push(
            r#"{"event":"TurnStarted","turn_id":"turn_00000000000000000000000000000004","at":1788223894000}"#
                .to_string(),
        );
        lines
    });
    let h = ledger.history_health();
    assert_eq!(h.ambiguous, 1);
    assert_eq!(h.from_future, 0);
    assert_eq!(h.damaged, 0);
    let r = &ledger.skipped_records()[0];
    assert_eq!(r.kind, SkipKind::Ambiguous);
    assert_eq!(r.tag.as_deref(), Some("TurnStarted"));
    assert!(
        r.detail.contains("cannot tell") && r.detail.contains("no writer version"),
        "the ambiguity must be stated, not resolved by guessing: {}",
        r.detail
    );
    assert!(
        h.detail.contains("cannot tell which"),
        "and the CEO is told the same thing in his own words: {}",
        h.detail
    );
}

// ---------------------------------------------------------------------------------------------
// NOTHING THE CEO SAID EVER REACHES A LOG OR A REPORT
// ---------------------------------------------------------------------------------------------

/// serde's own messages quote the offending value — `invalid type: string "…"`. That value
/// is the CEO's words. `classify_skip` composes its sentences instead of reading them, and
/// this is what holds it.
#[test]
fn a_skipped_records_report_never_contains_one_word_of_its_content() {
    const SECRET: &str = "ACQUISITION-PRICE-IS-FORTY-MILLION";
    let cases = [
        // from the future, content in a field
        format!(r#"{{"event":"DealNoted","note":"{SECRET}","at":1}}"#),
        // known tag, wrong shape, content where a number belongs
        format!(r#"{{"event":"TurnCompleted","turn_id":"t","stop_reason":"x","at":"{SECRET}"}}"#),
        // damaged, content inside
        format!(r#"{{"event":"TurnStarted","turn_id":"{SECRET}""#),
        // damaged, content AS the tag
        format!(r#"{{"event":"{SECRET} and more","at":1}}"#),
    ];
    for case in cases {
        let (ledger, _) = open_edited("v1-current", |mut lines| {
            lines.push(case.clone());
            lines
        });
        let r = &ledger.skipped_records()[0];
        let printed = format!("{:?} {:?} {}", r.kind, r.tag, r.detail);
        assert!(!printed.contains(SECRET), "the skip report leaked content: {printed}");
        assert!(!printed.contains("FORTY"), "the skip report leaked content: {printed}");
        let h = ledger.history_health();
        assert!(!h.detail.contains(SECRET) && !h.headline.contains(SECRET));
    }
}

// ---------------------------------------------------------------------------------------------
// ORDERING AND PROJECTION ACROSS A SKIP
// ---------------------------------------------------------------------------------------------

/// What the fold assumes about sequence, stated as a test rather than as a claim: `apply`
/// reaches every existing turn by id and no-ops when it is absent, and text runs are cut by
/// GAPS in the shared per-turn counter. So a skipped record between two deltas must leave
/// the runs exactly as the counter describes them — a skip must not merge two runs, and it
/// must not invent a third.
#[test]
fn a_skip_between_two_deltas_leaves_the_text_runs_exactly_as_the_counter_describes_them() {
    let (ledger, path) = open_edited("v1-current", |mut lines| {
        // Between seq 1 and seq 3 of turn …003, where a run boundary already is …
        lines.insert(12, r#"{"event":"ToolInvoked","turn_id":"turn_00000000000000000000000000000003","at":1787975161400}"#.to_string());
        // … and inside a contiguous run, between seq 0 and seq 1.
        lines.insert(11, r#"{"event":"ToolInvoked","turn_id":"turn_00000000000000000000000000000003","at":1787975161005}"#.to_string());
        lines
    });
    assert_eq!(ledger.history_health().from_future, 2);
    assert_eq!(
        projection_lines(&path, "runs"),
        golden_projection_lines("v1-current"),
        "a skipped record changed how the reply was split into runs"
    );
}

/// A record from a newer RichOS carries a real fencing token (ECS §3.4). The counter has to
/// stay ahead of every revision DURABLY recorded, including one this build could not fold,
/// or a restart re-issues a revision the file already used.
#[test]
fn a_fencing_revision_on_a_future_record_still_advances_the_counter() {
    let (mut ledger, _) = open_edited("v1-current", |mut lines| {
        lines.push(r#"{"event":"ContextActivated","thread_id":"thr_00000000000000000000000000000002","binding_revision":900,"at":1}"#.to_string());
        lines
    });
    assert_eq!(ledger.history_health().from_future, 1);
    let id = ledger.create_thread("next", &EntityId::parse("richos").unwrap()).unwrap();
    assert_eq!(
        ledger.thread_binding(&id).unwrap().binding_revision(),
        901,
        "the next revision must clear the highest one on disk, including one on a record this \
         build could not read"
    );
}

/// A damaged line's numbers are not facts, so nothing is salvaged from one — including a
/// number that would otherwise shove the fencing counter to `u64::MAX`.
#[test]
fn nothing_is_salvaged_from_a_damaged_record() {
    let (mut ledger, _) = open_edited("v1-current", |mut lines| {
        lines.push(format!(r#"{{"event":"two words","binding_revision":{},"at":1}}"#, u64::MAX));
        lines
    });
    assert_eq!(ledger.history_health().damaged, 1);
    let id = ledger.create_thread("next", &EntityId::parse("richos").unwrap()).unwrap();
    assert_eq!(ledger.thread_binding(&id).unwrap().binding_revision(), 3);
}

// ---------------------------------------------------------------------------------------------
// THE WRITE SIDE — what a future version may safely write, and what it may NOT
// ---------------------------------------------------------------------------------------------

/// **Adding a FIELD to an existing record is safe.** `Event` sets no `deny_unknown_fields`,
/// so an older reader ignores it entirely and the record still folds with its old meaning.
/// This is what makes the per-record schema version named in `Ledger::history_health`'s doc
/// a purely additive change, readable by v1.0.0-v1.0.2 as well as by this build.
#[test]
fn a_new_field_on_an_existing_record_replays_unchanged_on_this_build() {
    let (ledger, path) = open_edited("v1-current", |lines| {
        lines
            .into_iter()
            .map(|l| {
                if l.contains("\"PromptReceived\"") || l.contains("\"AssistantDelta\"") {
                    format!("{},\"schema\":7,\"written_by\":\"1.1.0\"}}", l.trim_end_matches('}'))
                } else {
                    l
                }
            })
            .collect()
    });
    assert!(ledger.history_health().is_clean(), "{:?}", ledger.skipped_records());
    assert_eq!(
        projection_lines(&path, "newfield"),
        golden_projection_lines("v1-current"),
        "an added field changed the projection"
    );
}

/// **A record can never span two lines.** `serde_json` escapes every newline inside a
/// string, so a reply full of them is still exactly one line of JSONL. That is what makes
/// line-at-a-time skipping safe at all: a skipped record can never swallow the record after
/// it.
#[test]
fn a_record_containing_newlines_is_still_exactly_one_line() {
    let e = Event::AssistantDelta {
        turn_id: "turn_00000000000000000000000000000004".into(),
        text: "line one\nline two\r\nline three\u{2028}and a line separator".into(),
        at: 1788223894000,
        seq: Some(1),
    };
    let encoded = serde_json::to_string(&e).unwrap();
    assert!(!encoded.contains('\n'), "a record with a raw newline in it would split in two");
    assert!(!encoded.contains('\r'));
    let (ledger, _) = open_edited("v1-current", |mut lines| {
        lines.push(encoded.clone());
        lines
    });
    assert!(ledger.history_health().is_clean(), "{:?}", ledger.skipped_records());
}

/// **THE GAP, PINNED AS A MEASUREMENT RATHER THAN CLAIMED IN PROSE.**
///
/// A new variant of a NESTED enum is not a new record type and does not degrade like one.
/// `Source::Managed` makes the whole `PromptReceived` unreadable — so the turn does not
/// exist, and the CEO loses an exchange rather than an unknown record. The reader survives
/// it; the conversation does not come back intact.
///
/// The minimal fix is on the WRITE side and it is one attribute: a `#[serde(other)]`
/// fallback variant on each nested enum that may grow (`Source`, `ActionStatus`,
/// `AttentionTier`, `ActionVisibility`), so an unknown value degrades to "unknown" instead
/// of destroying its record. That changes a shipped record type and what renders from it,
/// so it is not made here — it is MEASURED here.
#[test]
fn a_new_value_in_a_nested_enum_costs_the_whole_record_and_the_turn_with_it() {
    let (ledger, _) = open_edited("v1-current", |lines| {
        lines
            .into_iter()
            .map(|l| l.replace(r#""source":"text""#, r#""source":"managed""#))
            .collect()
    });
    let h = ledger.history_health();
    assert_eq!(h.ambiguous, 2, "both `text` prompts became unreadable records");
    assert_eq!(h.from_future, 0, "and NOT as benign records from the future");
    assert_eq!(
        ledger.turns().len(),
        2,
        "two of the four turns are gone entirely — a new nested-enum value costs the TURN, \
         not just the record"
    );
    assert!(
        ledger.turn("turn_00000000000000000000000000000003").is_none(),
        "the turn whose PromptReceived was unreadable does not exist"
    );
}

// ---------------------------------------------------------------------------------------------
// POINT 18 — `written_by`: THE CASE THE FORMAT COULD NOT DECIDE, DECIDED
//
// Spec: `richos-hq docs/plans/nightly-channel-spec-2026-09-17.md` point 18, which quotes
// `Ledger::history_health`'s own request for this field and the reason it was declined
// ("starting to write a new field is a change to what goes on customers' disks"). The CEO's
// 2026-09-17 ruling authorizes that change; these tests are what it bought.
//
// Every negative below carries its positive control ON THE SAME BYTES: the identical record
// with the stamp removed, or carrying a different one, so the verdict is proven to turn on
// the stamp and on nothing else.
// ---------------------------------------------------------------------------------------------

/// **THE BRIEF'S CASE, and the one that decides whether this field is legal to write at
/// all.** A record carrying `written_by` from a build that does not exist yet must REPLAY —
/// not be skipped, not be classified, not be reported. Its shape is one this build knows;
/// the stamp is an unknown field on a known variant, and serde ignores those.
///
/// This is the same property in the OPPOSITE direction to the rest of this file: everywhere
/// else here pins that today's reader still reads yesterday's bytes. This pins that
/// yesterday's reader — every published build through v1.0.2 — still reads today's.
#[test]
fn a_record_carrying_a_written_by_from_a_newer_build_replays_on_this_build() {
    let unstamped = r#"{"event":"TurnStarted","turn_id":"turn_00000000000000000000000000000004","session_id":"s4","at":1788223894000}"#;
    let stamped = r#"{"event":"TurnStarted","turn_id":"turn_00000000000000000000000000000004","session_id":"s4","at":1788223894000,"written_by":9999}"#;

    let (control, control_path) = open_edited("v1-current", |mut l| {
        l.push(unstamped.to_string());
        l
    });
    let (from_future, future_path) = open_edited("v1-current", |mut l| {
        l.push(stamped.to_string());
        l
    });

    let ch = control.history_health();
    let sh = from_future.history_health();
    assert_eq!(ch.skipped, 0, "positive control: the unstamped record replays");
    assert_eq!(
        sh.skipped, 0,
        "and so does the identical record carrying a written_by this build has never seen"
    );
    assert_eq!(
        sh.records_applied, ch.records_applied,
        "the stamp costs nothing: same bytes, same record count, stamped or not"
    );
    assert_eq!(
        projection_lines(&future_path, "v1-current"),
        projection_lines(&control_path, "v1-current"),
        "and the CONVERSATION the two produce is identical, character for character"
    );
}

/// A known tag, a shape that does not fit, and a stamp ABOVE this build's: the format moved
/// on. `FromFuture` — calm, expected, nothing went wrong.
///
/// Positive control: the same bytes without the stamp are `Ambiguous`, which is what this
/// build said about every such record before point 18.
#[test]
fn a_stamp_above_this_build_turns_an_undetermined_record_into_a_calm_one() {
    let bad_shape = r#"{"event":"TurnStarted","turn_id":"turn_00000000000000000000000000000004","at":1788223894000"#;
    let (from_future, _) = open_edited("v1-current", |mut l| {
        l.push(format!(r#"{bad_shape},"written_by":2}}"#));
        l
    });
    let (control, _) = open_edited("v1-current", |mut l| {
        l.push(format!("{bad_shape}}}"));
        l
    });

    let fh = from_future.history_health();
    assert_eq!(fh.from_future, 1, "the stamp says a newer build wrote it, so it is the future");
    assert_eq!(fh.ambiguous, 0, "and nothing is left undetermined");
    assert_eq!(fh.damaged, 0);
    assert_eq!(from_future.skipped_records()[0].kind, SkipKind::FromFuture);
    assert_eq!(from_future.skipped_records()[0].tag.as_deref(), Some("TurnStarted"));

    assert_eq!(
        control.history_health().ambiguous,
        1,
        "positive control: the identical record with no stamp is undetermined"
    );
    assert_eq!(control.history_health().from_future, 0);
}

/// A known tag, a shape that does not fit, and a stamp AT OR BELOW this build's: the writer
/// and the reader agree about the shapes, so the bytes are damaged. `Damaged` — the loud one.
///
/// This is the half a rollback needs. Without it a damaged record on a build that also
/// understands newer records is waved through as "the future", and the CEO is told to wait
/// for an update that is never going to bring it back.
#[test]
fn a_stamp_at_or_below_this_build_turns_an_undetermined_record_into_a_loud_one() {
    let bad_shape = r#"{"event":"TurnStarted","turn_id":"turn_00000000000000000000000000000004","at":1788223894000"#;
    let (damaged, _) = open_edited("v1-current", |mut l| {
        l.push(format!(r#"{bad_shape},"written_by":1}}"#));
        l
    });
    let (control, _) = open_edited("v1-current", |mut l| {
        l.push(format!("{bad_shape}}}"));
        l
    });

    let dh = damaged.history_health();
    assert_eq!(dh.damaged, 1, "the writer knew the same shapes this build does, so this is damage");
    assert_eq!(dh.ambiguous, 0);
    assert_eq!(dh.from_future, 0);
    assert_eq!(damaged.skipped_records()[0].kind, SkipKind::Damaged);

    assert_eq!(
        control.history_health().ambiguous,
        1,
        "positive control: the identical record with no stamp is still undetermined"
    );
}

/// A stamp that is not a plain non-negative integer is not a stamp. Damage that lands on the
/// `written_by` value itself must not be read as evidence about anything — the record falls
/// back to `Ambiguous`, which is exactly what this build said before the field existed.
#[test]
fn a_stamp_that_is_not_a_plain_number_decides_nothing() {
    let bad_shape = r#"{"event":"TurnStarted","turn_id":"turn_00000000000000000000000000000004","at":1788223894000"#;
    for junk in [r#""written_by":"2""#, r#""written_by":-1"#, r#""written_by":null"#, r#""written_by":2.5"#] {
        let (ledger, _) = open_edited("v1-current", |mut l| {
            l.push(format!("{bad_shape},{junk}}}"));
            l
        });
        let h = ledger.history_health();
        assert_eq!(h.ambiguous, 1, "a non-integer stamp decides nothing: {junk}");
        assert_eq!(h.from_future, 0, "and is never read as the future: {junk}");
    }
}

/// **The write side.** Every record THIS build appends carries the stamp — otherwise point
/// 25's gate has nothing to read, and point 18 is a reader with no writer.
#[test]
fn every_record_this_build_appends_carries_its_writer_schema_version() {
    let dir = scratch("stamped-writes");
    let path = dir.join("ledger.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("stamped", &EntityId::parse("richos").unwrap()).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "the write above is on disk");
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("every appended line is JSON");
        assert_eq!(
            v.get("written_by").and_then(|x| x.as_u64()),
            Some(u64::from(richos_core::skip::WRITER_SCHEMA_VERSION)),
            "an appended record with no writer version: {line}"
        );
    }
    assert!(!thread.is_empty());

    // And the file this build just wrote reads back clean ON THIS BUILD — the stamp is not a
    // field its own reader trips over.
    let reread = Ledger::open(&path).unwrap();
    assert_eq!(
        reread.history_health().skipped,
        0,
        "our own stamped file has nothing unreadable in it"
    );
}

/// **THE PROVENANCE FIELD NEVER DEFAULTS TO A CLAIM — `PromptReceived::rich_audible`.**
///
/// Added 2026-09-17 to close the gap the voice pipeline's own handoff named (`c712ccd5`): a
/// spoken turn carried `source: "jam"` and nothing else, so an echo-born turn and a genuine one
/// were indistinguishable after the fact.
///
/// **The reason it is `Option<bool>` and not a `bool` is exactly what this test holds.** Slice 3b
/// declined an always-`None` field because it *"reads as never echo-born, worse than the gap"* —
/// and a defaulting `bool` is worse still: every record in `v1-current.jsonl` was written before
/// this field existed, and `#[serde(default)]` on a `bool` would deserialize all of them to
/// `false`, silently asserting that Rich was PROVABLY SILENT during audio nobody measured.
///
/// So: absent means `None`, `None` means "not recorded", and it is never read as "no".
#[test]
fn an_old_ledger_loads_with_no_provenance_claim_at_all() {
    let src = fixtures().join("v1-current.jsonl");
    let raw = std::fs::read_to_string(&src).unwrap();

    // PREMISE, checked rather than assumed: the fixture predates the field.
    assert!(
        !raw.contains("rich_audible"),
        "the fixture already carries the field, so this test proves nothing about old records"
    );
    let spoken = raw
        .lines()
        .filter(|l| l.contains("\"PromptReceived\"") && l.contains("\"source\":\"jam\""))
        .count();
    assert!(spoken >= 1, "premise: the fixture has at least one spoken turn to read back");

    let dir = scratch("v1-current-provenance");
    let dst = dir.join("ledger.jsonl");
    std::fs::copy(&src, &dst).unwrap();
    let ledger = Ledger::open(&dst).unwrap();

    let turns: Vec<_> = ledger.turns().iter().collect();
    assert!(!turns.is_empty(), "premise: the fixture projects turns");
    for t in &turns {
        assert_eq!(
            t.rich_audible, None,
            "turn {} claims a measurement of Rich's audible window that nobody took",
            t.id
        );
    }
}

/// The write side, both values, through the real API and back off disk.
///
/// `Some(true)` and `Some(false)` are two different MEASUREMENTS and must survive as two
/// different values; a typed turn must write the field at all, so the bytes of an ordinary
/// message do not grow a provenance claim.
#[test]
fn a_spoken_turn_records_its_provenance_and_a_typed_turn_records_none() {
    let dir = scratch("provenance-round-trip");
    let path = dir.join("ledger.jsonl");
    let mut ledger = Ledger::open(&path).unwrap();
    let entity = EntityId::parse("richos").unwrap();
    let thread = ledger.create_thread("provenance", &entity).unwrap();
    let binding = ledger.thread_binding(&thread).unwrap();

    let audible = ledger
        .record_prompt_received_spoken(&binding, "stop counting", Source::Jam, true)
        .unwrap();
    let silent = ledger
        .record_prompt_received_spoken(&binding, "book the flight", Source::Jam, false)
        .unwrap();
    let typed = ledger.record_prompt_received(&binding, "typed message", Source::Text).unwrap();

    // ---- on disk -----------------------------------------------------------------------
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut by_turn: Vec<(String, Option<bool>)> = Vec::new();
    for line in raw.lines().filter(|l| l.contains("\"PromptReceived\"")) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        by_turn.push((
            v["turn_id"].as_str().unwrap().to_string(),
            v.get("rich_audible").map(|x| x.as_bool().unwrap()),
        ));
    }
    assert_eq!(
        by_turn,
        vec![
            (audible.clone(), Some(true)),
            (silent.clone(), Some(false)),
            (typed.clone(), None),
        ],
        "the bytes do not carry the three states apart"
    );
    // ABSENT, not `null` and not `false`: `skip_serializing_if` is what makes a typed turn's
    // record byte-identical to what it was before this field existed.
    let typed_line = raw
        .lines()
        .find(|l| l.contains(&typed) && l.contains("\"PromptReceived\""))
        .unwrap();
    assert!(
        !typed_line.contains("rich_audible"),
        "a typed turn grew a provenance field: {typed_line}"
    );

    // ---- and back through the reader ---------------------------------------------------
    let reread = Ledger::open(&path).unwrap();
    assert_eq!(reread.history_health().skipped, 0, "our own file has nothing unreadable in it");
    let got: Vec<(String, Option<bool>)> =
        reread.turns().iter().map(|t| (t.id.clone(), t.rich_audible)).collect();
    assert_eq!(got, vec![(audible, Some(true)), (silent, Some(false)), (typed, None)]);
}
