//! THE INTAKE LOG SURVIVES A RECORD IT CANNOT READ, AND SAYS SO.
//!
//! `steering.rs`'s intake log is the one file that holds what the CEO TYPED before it
//! becomes anything: a steering message he entered while Rich was working (UX §9.2) or a
//! stop he pressed (§9.3), fsync'd the instant he acted and not yet drained into the
//! ledger. Until 2026-09-05 the replay dropped any record it could not parse with no
//! count, no log line and no trace — a message he sent that never arrives and that nobody
//! ever learns was lost.
//!
//! **Every test here is worthless without `nothing_at_all_is_skipped_on_an_untouched_log`.**
//! A reader that skips everything passes every "the damage was reported" test in this file
//! and fails the only one that matters, so that test comes first and is the anti-vacuous
//! floor for the rest.
//!
//! Every fixture is built by the REAL writer (`IntakeLog::steer` / `::stop` /
//! `::mark_drained`) and then edited, so no test here can pass against a shape the product
//! does not actually write.

use richos_core::skip::SkipKind;
use richos_core::steering::{IntakeLog, IntakeRecord, KNOWN_INTAKE_TAGS};
use std::io::Write;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("richos-intake-fc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}.jsonl"))
}

/// A genuine, four-record intake log: two steering messages the CEO typed, a stop he
/// pressed, and the drain marker for the first one. Written by the product's own writer.
fn write_real_log(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let mut log = IntakeLog::open(path).unwrap();
    let first = log.steer("thr_1", "turn_1", None, "also check the invoice").unwrap();
    log.steer("thr_1", "turn_1", None, "and call the bank").unwrap();
    log.stop("turn_1").unwrap();
    log.mark_drained(first.id()).unwrap();
}

fn lines_of(path: &PathBuf) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Build the real log, hand its lines to `edit`, write the result back, and reopen.
fn open_edited(name: &str, edit: impl FnOnce(Vec<String>) -> Vec<String>) -> (IntakeLog, PathBuf) {
    let path = scratch(name);
    write_real_log(&path);
    let edited = edit(lines_of(&path));
    let mut f = std::fs::File::create(&path).unwrap();
    for l in &edited {
        writeln!(f, "{l}").unwrap();
    }
    drop(f);
    let log = IntakeLog::open(&path).unwrap();
    (log, path)
}

// -------------------------------------------------------------------------------------
// THE ANTI-VACUOUS TEST. Everything below it is worthless without this one.
// -------------------------------------------------------------------------------------

/// An intake log nobody has damaged skips NOTHING and says NOTHING. A change that made
/// every record skip would satisfy every other test in this file, and would be the exact
/// failure this work exists to prevent.
#[test]
fn nothing_at_all_is_skipped_on_an_untouched_log() {
    let (log, path) = open_edited("clean", |l| l);
    let h = log.health();
    assert!(h.is_clean(), "an untouched log skipped something: {:?}", log.skipped_records());
    assert_eq!(h.records_read, 4, "two steers, one stop, one drain marker");
    assert_eq!(h.records_applied, 4, "every record must be APPLIED, not skipped");
    assert_eq!(h.skipped, 0);
    assert_eq!(h.headline, "", "a clean intake log says nothing at all");
    assert_eq!(h.detail, "");
    // And the replay still means what it always meant: the undrained records come back.
    assert_eq!(log.pending().len(), 2, "the second steer and the stop are still pending");
    let _ = std::fs::remove_file(path);
}

/// The same claim from the other end: reopening a log the product wrote, unedited, is
/// byte-for-byte the same file afterwards. The reader never rewrites what it read.
#[test]
fn replaying_a_log_leaves_the_file_exactly_as_it_was() {
    let path = scratch("untouched-bytes");
    write_real_log(&path);
    let before = std::fs::read(&path).unwrap();
    let before_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    let log = IntakeLog::open(&path).unwrap();
    assert!(log.health().is_clean());
    assert_eq!(std::fs::read(&path).unwrap(), before, "the replay rewrote the file");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before_mtime);
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// THE KNOWN-TAG TABLE
// -------------------------------------------------------------------------------------

/// EXHAUSTIVE on purpose. Adding a variant to `IntakeRecord` without adding its name to
/// `KNOWN_INTAKE_TAGS` fails to COMPILE here — which is the point, because a missing entry
/// would make this build report a genuinely DAMAGED record of that type as a benign one
/// from the future, and quietly wait for an update that is never coming.
fn tag_of(r: &IntakeRecord) -> &'static str {
    match r {
        IntakeRecord::Steer { .. } => "steer",
        IntakeRecord::Channel { .. } => "channel",
        IntakeRecord::Desk { .. } => "desk",
        IntakeRecord::Stop { .. } => "stop",
        IntakeRecord::Drained { .. } => "drained",
    }
}

#[test]
fn the_known_tag_table_matches_the_record_type_exactly() {
    let samples = [
        IntakeRecord::Steer {
            id: 1,
            thread_id: "t".into(),
            steering_turn_id: "turn".into(),
            entity_id: None,
            text: "x".into(),
            at: 1,
        },
        IntakeRecord::Channel {
            id: 2,
            thread_id: "t".into(),
            entity_id: None,
            text: "x".into(),
            at: 1,
            channel: "phone".into(),
            client_id: None,
            voice_note: false,
            source: None,
        },
        IntakeRecord::Desk {
            id: 3,
            thread_id: "t".into(),
            entity_id: None,
            text: "x".into(),
            at: 1,
        },
        IntakeRecord::Stop { id: 4, turn_id: "turn".into(), at: 1 },
        IntakeRecord::Drained { through: 1 },
    ];
    let mut seen: Vec<String> = Vec::new();
    for r in &samples {
        let tag = tag_of(r);
        // The tag the exhaustive match names is the tag serde actually writes.
        let written = serde_json::to_value(r).unwrap();
        assert_eq!(serde_json::from_value::<IntakeRecord>(written.clone()).unwrap(), *r);
        assert_eq!(written.get("record").unwrap().as_str().unwrap(), tag);
        assert!(KNOWN_INTAKE_TAGS.contains(&tag), "{tag} is missing from KNOWN_INTAKE_TAGS");
        seen.push(tag.to_string());
    }
    seen.sort();
    let mut table: Vec<String> = KNOWN_INTAKE_TAGS.iter().map(|s| s.to_string()).collect();
    table.sort();
    assert_eq!(
        seen, table,
        "KNOWN_INTAKE_TAGS must hold exactly the tags IntakeRecord writes — no more, no fewer"
    );
}

// -------------------------------------------------------------------------------------
// THE THREE KINDS, TOLD APART
// -------------------------------------------------------------------------------------

/// A torn final record: the process died mid-append. Expected, benign, and it costs only
/// itself — which is what the original comment claimed and was right about.
#[test]
fn a_torn_final_record_is_damage_that_costs_only_itself() {
    let (log, path) = open_edited("torn", |mut l| {
        let last = l.pop().unwrap();
        l.push(last[..last.len() / 2].to_string());
        l
    });
    let h = log.health();
    assert_eq!(h.damaged, 1);
    assert_eq!(h.from_future, 0, "a torn write is NOT a record from a newer version");
    assert_eq!(h.records_read, 4);
    assert_eq!(h.records_applied, 3);
    assert_eq!(log.skipped_records()[0].kind, SkipKind::Damaged);
    assert!(log.skipped_records()[0].detail.contains("not valid JSON"));
    // AND THE OTHER HALF OF THE CLAIM: everything before it still loaded. The torn line is
    // the DRAIN MARKER, so `drained_through` stays 0 and all three requests come back
    // pending — the intake log's at-least-once posture doing exactly what it promises. A
    // record is re-presented, never lost, when the marker that would have retired it is the
    // thing that died.
    assert_eq!(log.pending().len(), 3, "the torn drain marker cost the intact records nothing");
    let _ = std::fs::remove_file(path);
}

/// The case the old `else { continue }` was silently wrong about: a record written by a
/// NEWER RichOS. It is expected, it is recoverable by updating, and it must be named as
/// such rather than treated as damage or dropped without a word.
#[test]
fn a_record_from_a_newer_version_is_reported_as_such_and_takes_nothing_with_it() {
    let (log, path) = open_edited("future", |mut l| {
        l.insert(1, r#"{"record":"defer","id":9,"until":"tomorrow","at":1788223893900}"#.to_string());
        l
    });
    let h = log.health();
    assert_eq!(h.from_future, 1);
    assert_eq!(h.damaged, 0, "a type this build does not know is not damage");
    assert_eq!(h.ambiguous, 0);
    assert_eq!(h.records_read, 5);
    assert_eq!(h.records_applied, 4);
    assert_eq!(log.skipped_records()[0].tag.as_deref(), Some("defer"));
    assert_eq!(log.pending().len(), 2, "everything this build DOES know still replayed");
    let _ = std::fs::remove_file(path);
}

/// The honest "I cannot tell": a tag this build knows, on a payload that does not fit it.
/// A newer RichOS that added a required field to `steer` writes exactly this, and so does a
/// `steer` whose bytes were mangled. The build says so instead of picking one.
#[test]
fn a_known_record_with_an_unknown_shape_is_undetermined_rather_than_guessed() {
    let (log, path) = open_edited("ambiguous", |mut l| {
        l.push(r#"{"record":"steer","id":40,"thread_id":"thr_1"}"#.to_string());
        l
    });
    let h = log.health();
    assert_eq!(h.ambiguous, 1);
    assert_eq!(h.damaged, 0);
    assert_eq!(h.from_future, 0);
    let r = &log.skipped_records()[0];
    assert_eq!(r.kind, SkipKind::Ambiguous);
    assert_eq!(r.tag.as_deref(), Some("steer"));
    assert!(
        r.detail.contains("cannot tell") && r.detail.contains("no writer version"),
        "the ambiguity must be stated, not resolved by guessing: {}",
        r.detail
    );
    assert!(h.detail.contains("cannot tell which"), "and the CEO is told the same: {}", h.detail);
    let _ = std::fs::remove_file(path);
}

/// A tag no build of RichOS could have written is DAMAGE, not the future. Without this the
/// app would tell the CEO to update and wait for a record that is never coming back.
#[test]
fn a_tag_no_build_could_have_written_is_damage_not_the_future() {
    let long = format!(r#"{{"record":"{}","id":9}}"#, "a".repeat(65));
    for bad in [
        r#"{"record":"","id":9}"#.to_string(),
        r#"{"record":"two words","id":9}"#.to_string(),
        r#"{"record":"steer-message","id":9}"#.to_string(),
        r#"{"record":"9leading","id":9}"#.to_string(),
        r#"{"record":42,"id":9}"#.to_string(),
        r#"{"id":9,"text":"no tag at all"}"#.to_string(),
        long,
    ] {
        let (log, path) = open_edited("badtag", |mut l| {
            l.push(bad.clone());
            l
        });
        let h = log.health();
        assert_eq!(h.damaged, 1, "{bad} was not called damage");
        assert_eq!(h.from_future, 0, "{bad} was waved through as the future");
        assert_eq!(log.skipped_records()[0].tag, None, "no tag is kept from a damaged line");
        let _ = std::fs::remove_file(path);
    }
}

/// JSON that is not a record shape at all.
#[test]
fn json_that_is_not_a_record_at_all_is_damage() {
    for junk in ["[1,2,3]", "42", "\"a string\"", "null", "true"] {
        let (log, path) = open_edited("notobject", |mut l| {
            l.push(junk.to_string());
            l
        });
        assert_eq!(log.health().damaged, 1, "{junk}");
        assert_eq!(log.pending().len(), 2, "{junk}: the real records still replayed");
        let _ = std::fs::remove_file(path);
    }
}

// -------------------------------------------------------------------------------------
// ONE BAD BYTE USED TO TAKE EVERY REQUEST AFTER IT
// -------------------------------------------------------------------------------------

/// `lines().map_while(Result::ok)` ends the ITERATOR at the first non-UTF-8 line. So a
/// single damaged byte early in the file discarded every record after it — not one lost
/// message but all of them, and still without a word. The reader keeps going PAST it now,
/// which is the half of this that actually saves the CEO's words.
#[test]
fn a_line_that_is_not_valid_utf8_costs_only_itself_and_the_reader_carries_on() {
    let path = scratch("badbyte");
    write_real_log(&path);
    let original = lines_of(&path);
    let mut bytes: Vec<u8> = Vec::new();
    // The bad byte goes FIRST, so "the reader carried on" is the only way the rest survives.
    bytes.extend_from_slice(&[0xff, 0xfe, 0xff]);
    bytes.push(b'\n');
    for l in &original {
        bytes.extend_from_slice(l.as_bytes());
        bytes.push(b'\n');
    }
    std::fs::write(&path, &bytes).unwrap();

    let log = IntakeLog::open(&path).unwrap();
    let h = log.health();
    assert_eq!(h.damaged, 1);
    assert_eq!(h.records_applied, 4, "every record after the bad byte still replayed");
    assert_eq!(log.pending().len(), 2, "the CEO's undrained words survived a bad byte above them");
    assert!(log.skipped_records()[0].detail.contains("not valid UTF-8"));
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// NEVER FATAL
// -------------------------------------------------------------------------------------

/// This is a read path on the CEO's own machine. Refusing to start is not an improvement
/// over dropping a message, so even a file with nothing readable in it opens.
#[test]
fn a_log_with_nothing_readable_in_it_still_opens() {
    let path = scratch("allbad");
    std::fs::write(&path, "not json\n{\"record\":\n\x00\x01\x02\n[]\n").unwrap();
    let log = IntakeLog::open(&path).expect("opening a damaged intake log must never fail");
    let h = log.health();
    assert_eq!(h.records_applied, 0);
    assert_eq!(h.skipped, 4);
    assert!(log.pending().is_empty());
    assert!(!h.headline.is_empty(), "and it is not silent about it");
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// AN ID IS A DE-DUPLICATION KEY, SO IT MUST NEVER BE ISSUED TWICE
// -------------------------------------------------------------------------------------

/// `Spine::drain_intake` treats "the ledger already holds a turn for this intake id" as
/// proof the record was handled. Two records with one id therefore DISCARD a real message.
///
/// The old derivation was `max(parsed ids).unwrap_or(drained_through) + 1`, which throws
/// `drained_through` away the moment any record parses. A file whose highest-id record is
/// unreadable then re-issued an id the file had already used.
#[test]
fn an_unreadable_record_can_never_make_the_counter_re_issue_an_id() {
    let path = scratch("collide");
    // Steer{1}, Steer{2}, Drained{2} — then Steer{2} is damaged in place.
    let mut log = IntakeLog::open(&path).unwrap();
    log.steer("thr_1", "turn_1", None, "first").unwrap();
    let second = log.steer("thr_1", "turn_1", None, "second").unwrap();
    log.mark_drained(second.id()).unwrap();
    drop(log);

    let mut lines = lines_of(&path);
    lines[1] = r#"{"record":"steer","id":2,"thread_id":"thr_1"}"#.to_string();
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();

    let mut reopened = IntakeLog::open(&path).unwrap();
    assert_eq!(reopened.health().ambiguous, 1, "the damaged record is reported");
    let fresh = reopened.steer("thr_1", "turn_2", None, "third").unwrap();
    assert!(
        fresh.id() > 2,
        "the counter re-issued id {} — a colliding id makes turn_for_intake match the wrong \
         turn and throw a real message away as already handled",
        fresh.id()
    );
    let _ = std::fs::remove_file(path);
}

/// The same guarantee for a record from a NEWER build, whose id is a real durable number
/// even though this build cannot fold the record around it.
#[test]
fn an_id_on_a_record_from_the_future_still_advances_the_counter() {
    let (mut log, path) = open_edited("futureid", |mut l| {
        l.push(r#"{"record":"defer","id":900,"at":1}"#.to_string());
        l
    });
    assert_eq!(log.health().from_future, 1);
    let fresh = log.steer("thr_1", "turn_2", None, "next").unwrap();
    assert!(fresh.id() > 900, "the counter must clear an id the file already used, got {}", fresh.id());
    let _ = std::fs::remove_file(path);
}

/// A `Damaged` line's numbers are not facts, so nothing is salvaged from one — including a
/// number that would otherwise shove the counter to the top of its range and leave no ids
/// for the CEO's next twenty years of messages.
#[test]
fn nothing_is_salvaged_from_a_damaged_record() {
    let (mut log, path) = open_edited("nosalvage", |mut l| {
        l.push(format!(r#"{{"record":"two words","id":{}}}"#, u64::MAX));
        l
    });
    assert_eq!(log.health().damaged, 1);
    let fresh = log.steer("thr_1", "turn_2", None, "next").unwrap();
    assert!(fresh.id() < 100, "a damaged line's number was trusted: {}", fresh.id());
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// COMPACTION MUST NOT TURN "CANNOT READ" INTO "DELETED"
// -------------------------------------------------------------------------------------

/// A record from a newer build is the RECOVERABLE case: update, and it comes back. That is
/// only true while it is still on disk. `compact` replaces the whole file with one marker
/// and a skipped record is not in `pending`, so without the guard the next compaction would
/// destroy it permanently — and the app would still be telling the CEO to update.
#[test]
fn compaction_never_deletes_a_record_nothing_could_read() {
    let path = scratch("compact-future");
    let _ = std::fs::remove_file(&path);
    {
        let mut log = IntakeLog::open(&path).unwrap();
        let r = log.steer("thr_1", "turn_1", None, "seed").unwrap();
        log.mark_drained(r.id()).unwrap();
    }
    // A record only a newer RichOS understands, appended by that newer RichOS.
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, r#"{{"record":"defer","id":800,"at":1}}"#).unwrap();
    drop(f);

    let mut log = IntakeLog::open(&path).unwrap();
    assert_eq!(log.health().from_future, 1);
    // Enough fully-drained traffic to cross COMPACT_AFTER_LINES = 256 several times over.
    for i in 0..200 {
        let r = log.steer("thr_1", "turn_1", None, &format!("msg {i}")).unwrap();
        log.mark_drained(r.id()).unwrap();
    }
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(r#""record":"defer""#),
        "compaction deleted a record this build could not read; an update can never bring it back"
    );
    // And reopening still finds it, still unread, still reported.
    assert_eq!(IntakeLog::open(&path).unwrap().health().from_future, 1);
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// WHAT THE CEO IS TOLD
// -------------------------------------------------------------------------------------

/// Skipping must be visible where it can be acted on: a structured record a caller can read
/// and a sentence a human can. And the sentence must be about THIS file — something he
/// typed that never reached Rich — not about the conversation failing to load.
#[test]
fn a_skipped_record_is_reported_in_words_the_ceo_can_read() {
    let (log, path) = open_edited("words", |mut l| {
        l.push(r#"{"record":"defer","id":9,"at":1788223893900}"#.to_string());
        l
    });
    let h = log.health();
    assert_eq!(h.skipped, 1);
    assert_eq!(
        h.headline,
        "Something you typed while Rich was working was written by a newer version of RichOS.",
        "the calm headline: a record from a newer version is expected, not an incident"
    );
    assert!(h.detail.contains("1 request was written by a newer version"), "{}", h.detail);
    assert!(h.detail.contains("Updating will bring it back."), "{}", h.detail);
    assert!(h.detail.contains("Everything else was read: 4 of 5 requests."), "{}", h.detail);
    assert!(h.detail.contains("Nothing was deleted"), "{}", h.detail);
    for word in ["serde", "Err(", "panic", "unwrap", ".jsonl", "JSON", "u64"] {
        assert!(!h.detail.contains(word), "the CEO's sentence contains {word:?}: {}", h.detail);
        assert!(!h.headline.contains(word), "the headline contains {word:?}");
    }
    let _ = std::fs::remove_file(path);
}

/// Damage gets a DIFFERENT sentence from a record written by a newer version, and it names
/// the consequence that actually matters: he was waiting on something that never arrived.
#[test]
fn damage_gets_its_own_sentence_and_names_the_consequence() {
    let (log, path) = open_edited("damage-words", |mut l| {
        l.push("{\"record\":\"steer\",\"id\":".to_string());
        l
    });
    let h = log.health();
    assert_eq!(h.headline, "Something you typed while Rich was working could not be read.");
    assert!(h.detail.contains("never reached him — please ask again"), "{}", h.detail);
    let _ = std::fs::remove_file(path);
}

/// Plurals are composed, not glued: two damaged records must not read "1 requests".
#[test]
fn the_sentences_agree_with_their_own_numbers() {
    let (log, path) = open_edited("plurals", |mut l| {
        l.push("{\"record\":\"steer\",\"id\":".to_string());
        l.push("[]".to_string());
        l.push(r#"{"record":"defer","id":7,"at":1}"#.to_string());
        l.push(r#"{"record":"defer","id":8,"at":1}"#.to_string());
        l
    });
    let h = log.health();
    assert_eq!(h.damaged, 2);
    assert_eq!(h.from_future, 2);
    assert_eq!(h.headline, "Some things you typed while Rich was working could not be read.");
    assert!(h.detail.contains("2 requests were written by a newer version"), "{}", h.detail);
    assert!(h.detail.contains("2 requests are damaged"), "{}", h.detail);
    assert!(h.detail.contains("bring them back"), "{}", h.detail);
    assert!(!h.detail.contains("1 requests") && !h.detail.contains("2 request "), "{}", h.detail);
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// NOTHING THE CEO TYPED EVER REACHES A LOG OR A REPORT
// -------------------------------------------------------------------------------------

/// serde's own messages quote the offending value — `invalid type: string "…"`. In this
/// file that value is literally what the CEO typed into the composer. The classifier
/// composes its sentences from the line's STRUCTURE and never consults the parser error,
/// and this is what holds it.
///
/// The assertion covers every string that leaves this module: the structured record, the
/// two health sentences, and the exact operator line `IntakeLog::report_skipped` prints —
/// reconstructed here from the same four components it formats, because those components
/// are the only thing it has to print.
#[test]
fn a_skipped_records_report_never_contains_one_word_of_what_he_typed() {
    const SECRET: &str = "ACQUISITION-PRICE-IS-FORTY-MILLION";
    let cases = [
        // from a newer version, his words in a field
        format!(r#"{{"record":"defer","id":9,"note":"{SECRET}","at":1}}"#),
        // known tag, wrong shape, his words where a number belongs
        format!(r#"{{"record":"steer","id":"{SECRET}","thread_id":"t"}}"#),
        // damaged, his words inside
        format!(r#"{{"record":"steer","id":9,"text":"{SECRET}""#),
        // damaged, his words AS the tag
        format!(r#"{{"record":"{SECRET} and more","at":1}}"#),
        // valid JSON, not an object, his words inside
        format!(r#"["{SECRET}"]"#),
    ];
    for case in cases {
        let (log, path) = open_edited("secret", |mut l| {
            l.push(case.clone());
            l
        });
        let r = log.skipped_records().last().expect("the record was skipped");
        let h = log.health();
        // Exactly what report_skipped formats, from exactly the fields it formats.
        let operator_line = format!(
            "[richos] INTAKE RECORD SKIPPED ({}): line {} ({} bytes) — {}",
            r.kind.label(),
            r.line,
            r.bytes,
            r.detail
        );
        for printed in [operator_line, format!("{:?}", r), h.headline.clone(), h.detail.clone()] {
            assert!(!printed.contains(SECRET), "leaked what he typed: {printed}");
            assert!(!printed.contains("FORTY"), "leaked what he typed: {printed}");
            assert!(!printed.contains("MILLION"), "leaked what he typed: {printed}");
        }
        let _ = std::fs::remove_file(path);
    }
}

/// The anti-vacuous half of the secrecy test: the same planted string in a record that
/// PARSES is not reported at all, so the test above is not passing because nothing is ever
/// reported.
#[test]
fn a_log_carrying_the_same_words_in_a_readable_record_reports_nothing_at_all() {
    const SECRET: &str = "ACQUISITION-PRICE-IS-FORTY-MILLION";
    let path = scratch("secret-clean");
    let _ = std::fs::remove_file(&path);
    let mut log = IntakeLog::open(&path).unwrap();
    log.steer("thr_1", "turn_1", None, SECRET).unwrap();
    drop(log);
    let reopened = IntakeLog::open(&path).unwrap();
    let h = reopened.health();
    assert!(h.is_clean(), "a readable record must not be reported");
    assert_eq!(h.headline, "");
    assert_eq!(h.detail, "");
    assert!(reopened.skipped_records().is_empty());
    // And his words ARE still there, in the one place they belong.
    match &reopened.pending()[0] {
        IntakeRecord::Steer { text, .. } => assert_eq!(text, SECRET),
        other => panic!("expected the steering record, got {other:?}"),
    }
    let _ = std::fs::remove_file(path);
}
