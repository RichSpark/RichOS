//! THE LANE MAP, MADE REAL — and kept from breaking the thing it is supposed to improve.
//!
//! Open item 3.5 shipped the entity→lane map **empty by default**, deliberately, so that
//! shipping could not ratify a memory layout the CEO had not approved. He approved it on
//! 2026-09-01 (`wiki/ceo-decisions.md` §5, with one amendment: `person/` becomes `ceo/`),
//! so the map can now be real.
//!
//! # What a naive "make it real" would have done, measured
//!
//! Filling the map in with the six companies and stopping there breaks every re-prime.
//! The CEO's only corpus today is the repo-layout dogfood corpus at `richos-hq` — 573
//! records, **zero company partitions** — and loro refuses a lane it does not have:
//!
//! ```text
//! $ node loro/bin/loro-context.mjs compile --root /Users/alex/ab/richos-hq \
//!       --company femcboost --topic-stdin --budget-chars 1200 --audience rich --format json
//! exit 2
//! loro --company: no such company partition "femcboost" in this corpus. Known: (none).
//! Refusing to compile an empty lane and call it an answer — a mistyped company id must
//! not look like a company with nothing in it.
//! ```
//!
//! Exit 2 maps to [`LoroTier::Unavailable`], on every rotation, for every mapped entity —
//! trading a working 890-char slice for a permanent "loro could not be consulted". So the
//! map is real AND it is **reconciled against the corpus that will actually be compiled**:
//! a mapping whose lane does not exist is dropped, loudly and by name, and that entity
//! reads the CEO layer. The moment `companies/femcboost/` exists in his corpus, the same
//! map narrows to it with no configuration at all.

use richos_core::entity::{Entity, EntityRegistry};
use richos_core::loro::{CliContextCompiler, CorpusLanes, LaneMap, LoroRoot, LoroTools};
use richos_core::reprime::{LoroTier, SliceRequest};
use std::path::PathBuf;

/// Held by any test here that sets or clears the loro environment variables.
static LORO_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// THE REGISTRY THESE TESTS RUN AGAINST — a local fixture, since 2026-09-04.
///
/// It used to be `registry()`, a `const` table compiled into the
/// shipping binary. The registry is per-user now (`entity.rs` rule 4) and there is no
/// compiled-in company list to reach for, so the six ids this file's assertions were written
/// against are declared here.
///
/// **They keep their names on purpose.** Much of this file is the durable record of a
/// measurement taken on 2026-09-01 against a real corpus — a `femcboost` thread primed with
/// `richos`'s own signing decisions — and renaming the subject of a measurement makes the
/// record wrong. Nothing here reaches the app; it is a test fixture and the paths below
/// exist on no machine this test runs on.
fn registry() -> EntityRegistry {
    EntityRegistry::new(vec![
        Entity::new("femcboost", "FemcBoost", &["/fixture/ab/femcboost"]).unwrap(),
        Entity::new("deeply", "Deeply", &["/fixture/ab/deeply"]).unwrap(),
        Entity::new("prospects", "Prospects", &["/fixture/ab/prospects"]).unwrap(),
        Entity::new("richos", "RichOS", &["/fixture/ab/richos", "/fixture/ab/richos-hq"]).unwrap(),
        Entity::new("gpt-exporter", "GPT Exporter", &["/fixture/ab/gpt-exporter"]).unwrap(),
        Entity::new("webinar-booster", "Webinar Booster", &["/fixture/ab/webinar-booster"]).unwrap(),
    ])
    .unwrap()
}

/// A tools directory that exists on disk (`LoroTools::locate` checks both entry points)
/// but is never executed — every test here drives `interpret`/`argv`, not a child process.
fn tools(tag: &str) -> (LoroTools, PathBuf) {
    let dir = std::env::temp_dir().join(format!("richos-lane-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("bin").join("loro-context.mjs"), "//").unwrap();
    std::fs::write(dir.join("bin").join("loro-write.mjs"), "//").unwrap();
    (LoroTools::locate(&dir).unwrap(), dir)
}

fn compiler(tag: &str, lanes: LaneMap) -> (CliContextCompiler, PathBuf) {
    let (t, dir) = tools(tag);
    (CliContextCompiler::new(t, LoroRoot::Corpus(PathBuf::from("/nowhere/corpus")), lanes), dir)
}

fn req<'a>(entity: &'a str, topic: &'a str) -> SliceRequest<'a> {
    SliceRequest { thread_id: "t1", entity_id: entity, topic, budget_chars: 1200 }
}

/// The default map is ONE LANE PER REGISTERED ENTITY, named after it — and it enumerates
/// the registry in force rather than a compiled-in list.
#[test]
fn the_default_lane_map_is_the_registered_entities_and_nothing_else() {
    // An install that has registered nothing gets no narrowing at all, which is the same
    // posture as `RICHOS_LORO_LANES=` and claims no partition exists.
    assert_eq!(LaneMap::same_name_lanes(&EntityRegistry::empty()).len(), 0);

    let m = LaneMap::same_name_lanes(&registry());
    assert_eq!(m.len(), 6);
    for id in ["femcboost", "deeply", "prospects", "richos", "gpt-exporter", "webinar-booster"] {
        assert_eq!(m.lane_for(id), Some(id), "{id} is mapped to its own lane");
    }
    // Not a rule, an enumeration: an id that is not a registered entity is not mapped,
    // and the map does not grow one by pattern.
    assert_eq!(m.lane_for("northwind"), None);
    assert_eq!(m.lane_for("halstead"), None);
}

#[test]
fn a_lane_keyed_by_something_that_is_not_a_registered_entity_is_a_typo_and_is_refused() {
    let reg = registry();
    assert!(LaneMap::parse("femcboost=fb,richos=rx").unwrap().validate_against(&reg).is_ok());
    // `femcbost` is one keystroke from a real entity and would silently map nothing —
    // looking exactly like a working configuration until the day it mattered.
    let e = LaneMap::parse("femcbost=fb").unwrap().validate_against(&reg).unwrap_err();
    let why = e.to_string();
    assert!(why.contains("femcbost"), "{why}");
    assert!(why.contains("not a registered entity"), "{why}");
}

#[test]
fn a_lane_the_corpus_does_not_have_is_dropped_and_that_entity_reads_the_ceo_layer() {
    // THE REGRESSION GUARD. This is the CEO's corpus today: partitions, none.
    let (mut c, dir) = compiler("drop", LaneMap::same_name_lanes(&registry()));
    let corpus = CorpusLanes::new(&[], &[]);
    let dropped = c.reconcile_lanes(&corpus);

    assert_eq!(dropped.len(), 6, "every lane is dropped against an unpartitioned corpus: {dropped:?}");
    assert!(c.lanes().is_empty(), "nothing survives to be sent as --company");
    // And the argv that would have exited 2 no longer carries the flag at all.
    let argv = c.argv(&req("femcboost", "how do we price it"));
    assert!(!argv.iter().any(|a| a == "--company"), "{argv:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_lane_the_corpus_does_have_survives_and_narrows_the_compile() {
    let (mut c, dir) = compiler("keep", LaneMap::same_name_lanes(&registry()));
    let corpus = CorpusLanes::new(&["femcboost".into(), "richos".into()], &[]);
    let dropped = c.reconcile_lanes(&corpus);

    assert_eq!(dropped.len(), 4, "the four with no partition are dropped: {dropped:?}");
    assert_eq!(c.lanes().lane_for("femcboost"), Some("femcboost"));
    assert_eq!(c.lanes().lane_for("richos"), Some("richos"));
    assert_eq!(c.lanes().lane_for("deeply"), None);

    let narrow = c.argv(&req("femcboost", "pricing"));
    let i = narrow.iter().position(|a| a == "--company").expect("mapped entity narrows");
    assert_eq!(narrow[i + 1], "femcboost");
    // ...and the entity whose lane was dropped still sends no --company, so it compiles
    // rather than exiting 2. What it may READ is still walled by the lane re-assertion.
    let wide = c.argv(&req("deeply", "pricing"));
    assert!(!wide.iter().any(|a| a == "--company"), "{wide:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reconciliation_names_every_mapping_it_dropped_and_why() {
    // A silent drop is the failure this whole map is trying to avoid one level up. The
    // reason string is what an operator reads at boot.
    let (mut c, dir) = compiler("names", LaneMap::parse("femcboost=fb,richos=rx").unwrap());
    let dropped = c.reconcile_lanes(&CorpusLanes::new(&["rx".into()], &["fb".into()]));
    assert_eq!(dropped.len(), 1, "{dropped:?}");
    assert!(dropped[0].contains("femcboost"), "{dropped:?}");
    assert!(dropped[0].contains("\"fb\""), "{dropped:?}");
    // A RETIRED lane is named as retired, not as missing — different facts, and the
    // operator's next move differs.
    assert!(dropped[0].contains("retired"), "{dropped:?}");
    assert_eq!(c.lanes().lane_for("richos"), Some("rx"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_entity_left_unmapped_against_a_partitioned_corpus_is_reported_not_hidden() {
    // The genuinely ambiguous state: the corpus HAS lanes, and this entity is bound to
    // none of them. The compile widens (no --company), so the lane re-assertion is what
    // holds — and it holds by refusing. That is fail-closed and correct, but an operator
    // must be told before the CEO meets it as "loro could not be consulted".
    let (mut c, dir) = compiler("unmapped", LaneMap::parse("femcboost=femcboost").unwrap());
    let corpus = CorpusLanes::new(&["femcboost".into(), "deeply".into()], &[]);
    c.reconcile_lanes(&corpus);
    let unmapped = c.entities_with_no_lane(&registry(), &corpus);
    assert!(unmapped.contains(&"deeply".to_string()), "{unmapped:?}");
    assert!(!unmapped.contains(&"femcboost".to_string()), "{unmapped:?}");
    // Nothing is claimed about an entity when the corpus has no partitions at all —
    // there, reading everything IS reading the CEO layer.
    assert!(c.entities_with_no_lane(&registry(), &CorpusLanes::new(&[], &[])).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_cross_entity_guard_still_refuses_after_a_lane_is_dropped() {
    // Dropping a lane must never read as dropping the wall. `--company` is an ATTENTION
    // control, not a privacy control (`loro-structure.md`), and the privacy work is done
    // by the re-assertion on the finished slice — which, with no lane, allows the CEO
    // layer and refuses every company item.
    let (mut c, dir) = compiler("wall", LaneMap::same_name_lanes(&registry()));
    c.reconcile_lanes(&CorpusLanes::new(&[], &[]));
    let json = format!(
        r#"{{"schemaVersion":1,"compiler":"x","thin":false,"coverage":"direct","text":{:?},
            "items":[{{"ref":"rec:companies/deeply/records/margin","kind":"decision","kindInferred":false,
            "title":"margins","scope":"org-shared","company":"deeply"}}],
            "corpus":{{"recordCount":3,"fingerprint":"sha256:abc","layout":"corpus","rootSource":"--corpus"}},
            "budget":{{"chars":1200,"usedChars":10,"itemsIncluded":1,"withheldByScope":0}},"notes":[]}}"#,
        "COMPANY MEMORY (loro) — bearing on: \"pricing\"\n• [decision] deeply margins are 40%"
    );
    match c.interpret(&json, &req("femcboost", "pricing")) {
        LoroTier::Unavailable(why) => {
            assert!(why.contains("\"deeply\""), "{why}");
            assert!(why.contains("femcboost"), "{why}");
        }
        other => panic!("a cross-entity slice must be REFUSED after reconciliation too, got {other:?}"),
    }
    // ...and the CEO layer still gets through, because ECS §3.5's default read set is the
    // CEO layer plus the active entity.
    let ceo_layer = format!(
        r#"{{"schemaVersion":1,"compiler":"x","thin":false,"coverage":"direct","text":{:?},
            "items":[{{"ref":"rec:ceo/records/principle","kind":"principle","kindInferred":false,
            "title":"ask","scope":"ceo-private","company":null}}],
            "corpus":{{"recordCount":3,"fingerprint":"sha256:abc","layout":"repo","rootSource":"--root"}},
            "budget":{{"chars":1200,"usedChars":10,"itemsIncluded":1,"withheldByScope":0}},"notes":[]}}"#,
        "COMPANY MEMORY (loro) — bearing on: \"pricing\"\n• [principle] ask, never infer"
    );
    assert!(c.interpret(&ceo_layer, &req("femcboost", "pricing")).is_slice());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn no_corpus_configured_is_not_an_error_and_not_a_crash() {
    // The state most machines are in, including a fresh install. `from_env` must say
    // "nothing configured" rather than failing — and it must not read a stale process
    // environment to do it.
    // Process environment is shared by every test in this binary and libtest runs them side
    // by side; whatever sets or clears these holds LORO_ENV for as long as its value is in
    // place (audit `docs/verification/2026-09-29-load-sensitive-checks-audit.md`,
    // `env-mutation-unguarded`).
    let _env = LORO_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let saved: Vec<(&str, Option<String>)> = ["LORO_CORPUS", "LORO_ROOT", "RICHOS_LORO_DIR", "RICHOS_LORO_LANES"]
        .iter()
        .map(|k| (*k, std::env::var(k).ok()))
        .collect();
    for (k, _) in &saved {
        unsafe { std::env::remove_var(k) };
    }
    let got = CliContextCompiler::from_env(&registry());
    for (k, v) in &saved {
        match v {
            Some(v) => unsafe { std::env::set_var(k, v) },
            None => unsafe { std::env::remove_var(k) },
        }
    }
    match got {
        Ok(None) => {}
        other => panic!("an install with no corpus must be Ok(None), got {:?}", other.map(|o| o.is_some())),
    }
}

/// The failure NEITHER the lane map NOR the cross-entity guard can see, and the one line
/// that answers it.
///
/// An in-repo corpus is one product's own record: no partitions, and `company: null` on
/// every item — legitimately the CEO layer. So the lane map narrows nothing and
/// `Slice::foreign_lane` refuses nothing; both are working correctly. A thread bound to a
/// different entity then receives that product's record under a heading reading
/// `COMPANY MEMORY (loro)`.
///
/// Measured against the CEO's only corpus on 2026-09-01 (`richos-hq`, 573 records,
/// `layout: repo`): a `femcboost` thread asking "how should we price the coach product"
/// was primed with the RichOS audio-capture decision, Wispr Flow's pricing and
/// code-signing certificate authorities. Nothing fabricated, nothing leaked — the corpus
/// holds one company and it is not FemcBoost.
#[test]
fn a_thread_reading_another_companys_in_repo_record_is_told_whose_it_is() {
    let (mut c, dir) = compiler("origin", LaneMap::same_name_lanes(&registry()));
    c.reconcile_lanes(&CorpusLanes::new(&[], &[]));
    c.set_repo_corpus_owner(Some("richos".into()));

    let json = slice(
        r#"{"ref":"rec:ceo/records/signing","kind":"decision","kindInferred":false,
        "title":"Signing","scope":"org-shared","company":null}"#,
    );

    // The OWNER reads its own record with no caveat — RichOS reading RichOS is right.
    match c.interpret(&json, &req("richos", "pricing")) {
        LoroTier::Slice(t) => {
            assert!(t.starts_with("COMPANY MEMORY (loro)"), "the owner gets loro's text verbatim: {t}");
            assert!(!t.contains("PROVENANCE"), "{t}");
        }
        other => panic!("expected a slice, got {other:?}"),
    }

    // ANY OTHER entity is told, in one sentence, whose record this is.
    match c.interpret(&json, &req("femcboost", "pricing")) {
        LoroTier::Slice(t) => {
            assert!(t.starts_with("{\"owner\":") || t.starts_with("{\"partitioned\":") || t.starts_with("{\"source\":"), "{t}");
            assert!(t.contains("\"owner\":\"richos\""), "{t}");
            assert!(t.contains("\"partitioned\":false"), "{t}");
            assert!(t.contains("\"requested_entity\":\"femcboost\""), "{t}");
            // loro's own text is carried whole underneath it, never edited.
            assert!(t.contains("COMPANY MEMORY (loro) — bearing on: \"pricing\""), "{t}");
        }
        other => panic!("expected a slice, got {other:?}"),
    }

    // A PROVISIONED corpus says nothing at all — there the partitions carry the answer.
    let (mut clean, dir2) = compiler("origin2", LaneMap::same_name_lanes(&registry()));
    let provisioned = CorpusLanes::with_layout(&["femcboost".into()], &[], "corpus", "/nowhere");
    clean.reconcile_lanes(&provisioned);
    assert_eq!(provisioned.repo_layout_root(), None);
    assert!(clean.corpus_provenance_line("femcboost").is_none());
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(dir2);
}

/// A repo-layout corpus reports its own root, and the registry is what names the owner —
/// which it could not do for `richos-hq` before the multi-root registry landed today.
#[test]
fn the_registry_is_what_names_the_owner_of_an_in_repo_corpus() {
    let c = CorpusLanes::with_layout(&[], &[], "repo", "/fixture/ab/richos-hq");
    let repo = c.repo_layout_root().expect("a repo-layout corpus names its root");
    let registry = registry();
    let owner = registry.resolve_root(repo).expect("richos-hq is registered");
    assert_eq!(owner.id.as_str(), "richos");
}

/// The caveat is conditioned on the ABSENCE OF PARTITIONS, not on the layout name.
///
/// `layout: "repo"` used to imply "no partitions"; since 2026-09-01 it does not — the
/// dogfood layout carries `ceo/` + `companies/<id>/`, because a `corpus`-layout root is
/// refused inside a product checkout and the CEO's only corpus is one. The provenance
/// sentence says *"it is not partitioned by company and holds no `<entity>` partition"*,
/// which against a partitioned corpus is false, and a false caveat tells a fresh Rich to
/// discount memory that is correctly his.
#[test]
fn a_partitioned_repo_layout_corpus_owes_no_provenance_caveat() {
    let partitioned =
        CorpusLanes::with_layout(&["femcboost".into(), "richos".into()], &[], "repo", "/fixture/ab/richos-hq");
    assert_eq!(partitioned.layout(), "repo");
    assert_eq!(
        partitioned.repo_layout_root(),
        None,
        "a partitioned corpus answers with its partitions, not with a sentence about not having any"
    );

    let (mut c, dir) = compiler("origin3", LaneMap::same_name_lanes(&registry()));
    c.reconcile_lanes(&partitioned);
    // The app sets the owner only when `repo_layout_root()` answers, so nothing is set here.
    assert!(c.corpus_provenance_line("femcboost").is_none());
    let _ = std::fs::remove_dir_all(dir);
}

fn slice(items: &str) -> String {
    format!(
        r#"{{"schemaVersion":1,"compiler":"loro-context-compiler/1.4.0","thin":false,"coverage":"direct",
            "text":{:?},"items":[{items}],
            "corpus":{{"recordCount":573,"fingerprint":"sha256:abc","layout":"repo","rootSource":"--root"}},
            "budget":{{"chars":1200,"usedChars":50,"itemsIncluded":1,"withheldByScope":0}},"notes":[]}}"#,
        "COMPANY MEMORY (loro) — bearing on: \"pricing\"\n• [decision] Signing — Developer ID, notarized."
    )
}
