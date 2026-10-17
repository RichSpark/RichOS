//! Explicit evidence lookup protocol parsing and bounded process arguments.
use richos_core::reprime::{EvidenceLookup, EvidenceRequest, EvidenceTier, SliceCoverage};
static LEDGER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

use richos_core::evidence::{CliEvidenceLookup, EvidenceTools, EvidenceZone, SUPPORTED_LOOKUP_SCHEMA};

/// A `CliEvidenceLookup` whose entry point is a real file (so `locate` is satisfied) but which
/// is never executed — every test below drives `interpret` directly.
fn cli() -> CliEvidenceLookup {
    let dir = std::env::temp_dir().join(format!(
        "richos-evidence-bin-{}-{}",
        std::process::id(),
        LEDGER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("richos-evidence.mjs");
    std::fs::write(&bin, "// not executed by these tests\n").unwrap();
    CliEvidenceLookup::new(
        EvidenceTools::locate(&bin).unwrap(),
        EvidenceZone::Zone(dir.join("zone")),
    )
}

fn wire(body: &str) -> String {
    format!("{{\"schemaVersion\":{SUPPORTED_LOOKUP_SCHEMA},\"lookup\":\"richos-evidence/1.0.0\",{body}}}")
}

#[test]
fn an_available_result_becomes_the_block_verbatim_with_its_spoken_form() {
    let out = cli().interpret(&wire(
        "\"consulted\":true,\"available\":true,\"text\":\"FROM YOUR FILES (evidence — not company \
         memory)\\n1. Coach pricing model\",\"spokenText\":\"From your files: Coach pricing model. \
         Want me to read you what it says?\",\"budget\":{\"chars\":48,\"takenFromMemoryBudget\":false}",
    ));
    match out {
        EvidenceTier::Block { text, spoken } => {
            assert!(text.starts_with("FROM YOUR FILES (evidence — not company memory)"));
            assert!(text.contains("1. Coach pricing model"));
            assert!(spoken.starts_with("From your files:"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unsupported_schema_is_refused_rather_than_mis_parsed() {
    let out = cli().interpret(
        "{\"schemaVersion\":2,\"consulted\":true,\"available\":true,\"text\":\"FROM YOUR FILES\"}",
    );
    assert!(matches!(out, EvidenceTier::Unavailable(ref r) if r.contains("schemaVersion 2")), "{out:?}");
    // POSITIVE CONTROL: the identical object at schema 1 IS accepted, so the refusal is the
    // version and not the shape.
    let ok = cli().interpret(&wire("\"consulted\":true,\"available\":true,\"text\":\"FROM YOUR FILES\""));
    assert!(ok.is_block(), "{ok:?}");
}

#[test]
fn a_block_that_claims_the_memory_budget_is_refused_whole() {
    let out = cli().interpret(&wire(
        "\"consulted\":true,\"available\":true,\"text\":\"FROM YOUR FILES\",\
         \"budget\":{\"chars\":15,\"takenFromMemoryBudget\":true}",
    ));
    assert!(
        matches!(out, EvidenceTier::Unavailable(ref r) if r.contains("memory lanes' budget")),
        "{out:?}"
    );
    // POSITIVE CONTROL: `false` on the same object is accepted.
    let ok = cli().interpret(&wire(
        "\"consulted\":true,\"available\":true,\"text\":\"FROM YOUR FILES\",\
         \"budget\":{\"chars\":15,\"takenFromMemoryBudget\":false}",
    ));
    assert!(ok.is_block(), "{ok:?}");
}

#[test]
fn an_available_result_with_empty_text_never_renders_a_bare_heading() {
    // `evidence-lookup.js`'s caller contract: an unavailable result renders nothing at all,
    // "never a heading with an empty body, which reads as 'your files say nothing about this'
    // when the truth is 'I did not look'."
    let out = cli().interpret(&wire("\"consulted\":true,\"available\":true,\"text\":\"   \""));
    assert!(matches!(out, EvidenceTier::NothingFound(_)), "{out:?}");
    assert!(out.text().is_none());
}

#[test]
fn unparseable_output_is_an_unknown_and_never_a_nothing() {
    let out = cli().interpret("this is not json");
    assert!(matches!(out, EvidenceTier::Unavailable(ref r) if r.contains("did not parse")), "{out:?}");
}

#[test]
fn a_refusal_the_module_reported_is_carried_with_its_reason() {
    let out = cli().interpret(&wire(
        "\"consulted\":true,\"available\":false,\"reason\":\"audience\",\
         \"detail\":\"the evidence lookup serves \\\"rich\\\" only in v1\",\"text\":\"\"",
    ));
    assert!(
        matches!(out, EvidenceTier::NothingFound(ref r) if r.contains("only in v1")),
        "{out:?}"
    );
}

#[test]
fn the_argv_names_the_lookup_verb_the_zone_and_the_compilers_own_label_and_nothing_else() {
    let c = cli();
    let argv = c.argv(&EvidenceRequest {
        thread_id: "thr_1",
        entity_id: "femcboost",
        topic: "what is our coach pricing model",
        coverage: SliceCoverage::NoneRecorded,
    });
    assert_eq!(argv[1], "lookup");
    assert!(argv.contains(&"--zone".to_string()));
    assert!(argv.contains(&"--coverage".to_string()));
    assert!(argv.contains(&"none".to_string()), "{argv:?}");
    assert!(argv.contains(&"--topic-stdin".to_string()), "the topic never goes through quoting");
    assert!(argv.contains(&"rich".to_string()), "v1 serves one audience");
    // NO WRITE VERB, and no topic on the command line.
    for banned in ["write", "promote", "correct", "supersede", "--apply"] {
        assert!(!argv.iter().any(|a| a == banned), "{banned} must not be reachable: {argv:?}");
    }
    assert!(
        !argv.iter().any(|a| a.contains("coach pricing")),
        "the topic goes on stdin, never in argv: {argv:?}"
    );
}

#[test]
fn an_unknown_label_never_reaches_the_process_at_all() {
    let c = cli();
    let req = EvidenceRequest {
        thread_id: "thr_1",
        entity_id: "femcboost",
        topic: "what is our coach pricing model",
        coverage: SliceCoverage::Unknown,
    };
    // The gate refuses before a process is spawned — the fake bin above is not executable
    // JavaScript, so reaching `run` would fail loudly rather than return this.
    let out = c.look_up(&req);
    assert!(
        matches!(out, EvidenceTier::NotConsulted(ref r) if r.contains("absence of the signal")),
        "{out:?}"
    );
    // And argv would carry no `--coverage` at all, so nothing downstream can infer one.
    assert!(!c.argv(&req).contains(&"--coverage".to_string()));
}

#[test]
fn an_empty_topic_is_refused_rather_than_ranked_against_nothing() {
    let out = cli().look_up(&EvidenceRequest {
        thread_id: "thr_1",
        entity_id: "femcboost",
        topic: "   ",
        coverage: SliceCoverage::NoneRecorded,
    });
    assert!(matches!(out, EvidenceTier::Unavailable(ref r) if r.contains("no topic")), "{out:?}");
}

#[test]
fn the_in_repo_dogfood_root_has_no_evidence_zone_and_is_refused_one() {
    // `LoroRoot::Root` is `wiki/` + `loro/` inside the product repository. Pointing the lookup
    // at one would ask it to read the checkout; there is no `ceo/evidence/` there and there
    // never will be.
    use richos_core::loro::LoroRoot;
    assert_eq!(EvidenceZone::from_loro_root(&LoroRoot::Root("/fixture/repo".into())), None);
    // POSITIVE CONTROL: a provisioned corpus DOES yield one.
    assert_eq!(
        EvidenceZone::from_loro_root(&LoroRoot::Corpus("/fixture/corpus".into())),
        Some(EvidenceZone::Corpus("/fixture/corpus".into()))
    );
}

#[test]
fn an_entry_point_that_is_not_there_is_refused_at_construction_not_once_per_turn() {
    let missing = std::env::temp_dir().join("richos-evidence-does-not-exist.mjs");
    let _ = std::fs::remove_file(&missing);
    assert!(EvidenceTools::locate(&missing).is_err());
    // POSITIVE CONTROL: a file that IS there is accepted.
    let there = std::env::temp_dir().join(format!("richos-evidence-present-{}.mjs", std::process::id()));
    std::fs::write(&there, "//\n").unwrap();
    assert!(EvidenceTools::locate(&there).is_ok());
    let _ = std::fs::remove_file(&there);
}
