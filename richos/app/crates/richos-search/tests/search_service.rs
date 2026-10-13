//! The search service end to end over the synthetic stand-in folder (`tests/fixtures/standin`),
//! with the stand-in embedder so no model is needed: the four checkbox combinations, the
//! truthful fallback, current-source checks, restart reconciliation and Open original.
use richos_search::embed::{ConceptEmbedder, Embedder};
use richos_search::manifest::{point_id, Manifest, SourceEntry, SourceState};
use richos_search::rank::Why;
use richos_search::service::{index_dir, Config, Empty, MeaningStatus, SearchRequest, SearchService, Tuning};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn fixture_copy(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("richos-search-service-{name}-{}", std::process::id()));
    if let Err(error) = std::fs::remove_dir_all(&base) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!("fixture cleanup: {error}");
        }
    }
    let folder = base.join("folder");
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/standin"), &folder);
    let cache = base.join("cache");
    (folder, cache)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// The stand-in embedder, counting every call so a test can prove a mode never embedded.
struct Counting(Arc<AtomicUsize>, ConceptEmbedder);
impl Embedder for Counting {
    fn identity(&self) -> String {
        self.1.identity()
    }
    fn dimension(&self) -> usize {
        self.1.dimension()
    }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        self.1.embed(texts)
    }
}

fn start(folder: &Path, cache: &Path, calls: Arc<AtomicUsize>) -> SearchService {
    let service = SearchService::start(Config {
        root: Some(folder.to_path_buf()),
        cache: Ok(cache.to_path_buf()),
        make_embedder: Box::new(move || Ok(Box::new(Counting(calls.clone(), ConceptEmbedder::stand_in())) as Box<dyn Embedder>)),
        busy: None,
        tuning: Tuning::default(),
    });
    service.wait_idle();
    service
}

fn ask(service: &SearchService, query: &str, super_finder: bool, strict: bool) -> richos_search::SearchResponse {
    service.search(&SearchRequest { query: query.into(), super_finder, strict, offset: 0, limit: 12 })
}

fn files(r: &richos_search::SearchResponse) -> Vec<String> {
    r.hits.iter().map(|h| h.rel_path.clone()).collect()
}

#[test]
fn the_four_combinations_follow_the_table_and_off_never_embeds() {
    let (folder, cache) = fixture_copy("four");
    let calls = Arc::new(AtomicUsize::new(0));
    let service = start(&folder, &cache, calls.clone());
    assert!(matches!(service.status().meaning, MeaningStatus::Ready { files: 10, .. }), "{:?}", service.status().meaning);

    // On, not strict: a document with none of the words is found by meaning.
    let on = ask(&service, "the app felt slow on first open", true, false);
    assert!(on.effective.super_finder);
    let iphone = on.hits.iter().find(|h| h.rel_path == "2025/iphone-build-three-seconds.md").expect("found by meaning");
    assert_eq!(iphone.why, Why::Meaning);

    // Off, not strict: ordinary text search, which cannot find it, and no embedding at all.
    let before = calls.load(Ordering::SeqCst);
    let off = ask(&service, "the app felt slow on first open", false, false);
    assert!(!off.effective.super_finder);
    assert!(!files(&off).contains(&"2025/iphone-build-three-seconds.md".to_string()));
    let off_strict = ask(&service, "cancel \"30 days\"", false, true);
    assert_eq!(calls.load(Ordering::SeqCst), before, "Super Finder off must not embed the query");
    // Off, strict: only documents containing "30 days" literally.
    assert_eq!(files(&off_strict), vec!["2025/refund-window.md"]);
    assert_eq!(off_strict.hits[0].exact, vec!["30 days"]);

    // On, strict: same requirement, meaning and words rank the eligible ones.
    let on_strict = ask(&service, "cancel \"30 days\"", true, true);
    assert_eq!(files(&on_strict), vec!["2025/refund-window.md"]);
    // On, loose: the exact one first, related wording ("thirty days") allowed below it.
    let on_loose = ask(&service, "cancel \"30 days\"", true, false);
    assert_eq!(on_loose.hits[0].rel_path, "2025/refund-window.md");
    assert!(files(&on_loose).contains(&"2024/refund-thirty-days-draft.md".to_string()));
    // A complete identifier, hyphen included.
    assert_eq!(files(&ask(&service, "\"INV-2049\"", true, true)), vec!["2025/invoice-inv-2049.md"]);
    // Two spans in different passages of the same document.
    assert_eq!(files(&ask(&service, "\"pricing\" \"support cost\"", false, true)), vec!["2025/pricing-and-support-cost.md"]);
    // An unpaired quote with Require quoted text checked is said, never relaxed.
    let broken = ask(&service, "cancel \"30 days", true, true);
    assert_eq!(broken.empty, Some(Empty::QuoteMismatch));
    assert!(broken.hits.is_empty());
    assert_eq!(ask(&service, "\"ninety-one days\"", true, true).empty, Some(Empty::NoExact));
    service.shutdown();
    std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
}

#[test]
fn a_late_answer_in_a_long_file_is_the_excerpt() {
    let (folder, cache) = fixture_copy("late");
    let service = start(&folder, &cache, Arc::new(AtomicUsize::new(0)));
    let r = ask(&service, "which subject line won", false, false);
    let hit = r.hits.iter().find(|h| h.rel_path == "2024/onboarding-email-brainstorm.md").unwrap();
    assert!(hit.passage.text.contains("Your first week, in three steps"), "{}", hit.passage.text);
    assert!(hit.passage.line > 200, "line {}", hit.passage.line);
    service.shutdown();
    std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
}

#[test]
fn text_search_works_when_the_model_is_unavailable_and_says_so() {
    let (folder, cache) = fixture_copy("nomodel");
    let service = SearchService::start(Config {
        root: Some(folder.clone()),
        cache: Ok(cache),
        make_embedder: Box::new(|| Err("Apple's sentence embedding for 'en' is not available on this Mac.".into())),
        busy: None,
        tuning: Tuning::default(),
    });
    service.wait_idle();
    assert!(matches!(service.status().meaning, MeaningStatus::Unavailable { .. }));
    let r = ask(&service, "refund", true, false);
    assert!(r.requested.super_finder && !r.effective.super_finder, "the checked box is kept; the fallback is reported");
    assert!(!r.hits.is_empty());
    service.shutdown();
    std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
}

#[test]
fn no_folder_and_a_missing_folder_are_named() {
    let none = SearchService::start(Config {
        root: None,
        cache: Err("unused".into()),
        make_embedder: Box::new(|| Err("unused".into())),
        busy: None,
        tuning: Tuning::default(),
    });
    none.wait_idle();
    assert_eq!(serde_json::to_value(none.status().folder).unwrap()["state"], "none");
    assert_eq!(ask(&none, "refund", true, false).empty, Some(Empty::NoFolder));
    let missing = SearchService::start(Config {
        root: Some(std::env::temp_dir().join("richos-search-no-such-folder-anywhere")),
        cache: Err("unused".into()),
        make_embedder: Box::new(|| Err("unused".into())),
        busy: None,
        tuning: Tuning::default(),
    });
    missing.wait_idle();
    let status = serde_json::to_value(missing.status().folder).unwrap();
    assert_eq!(status["state"], "error");
    assert!(status["message"].as_str().unwrap().contains("not found"));
}

#[test]
fn changed_and_deleted_files_are_dropped_before_return_and_open() {
    let (folder, cache) = fixture_copy("current");
    let service = start(&folder, &cache, Arc::new(AtomicUsize::new(0)));
    let before = ask(&service, "refund", false, false);
    let window = before.hits.iter().find(|h| h.rel_path == "2025/refund-window.md").unwrap().source_id.clone();
    assert!(service.resolve_for_open(&window).unwrap().ends_with("2025/refund-window.md"));
    std::fs::write(folder.join("2025/refund-window.md"), "# Changed\nrefund rules changed completely, and longer now").unwrap();
    std::fs::remove_file(folder.join("2024/refund-thirty-days-draft.md")).unwrap();
    let after = ask(&service, "refund", false, false);
    assert!(!files(&after).contains(&"2025/refund-window.md".to_string()), "no excerpt absent from the current file");
    assert!(!files(&after).contains(&"2024/refund-thirty-days-draft.md".to_string()));
    assert_eq!(after.dropped, 2);
    assert!(service.resolve_for_open(&window).is_err());
    assert!(service.resolve_for_open("doc-not-a-real-id").is_err());
    service.shutdown();
    std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
}

#[test]
fn restart_redoes_an_interrupted_file_and_keeps_completed_work() {
    let (folder, cache) = fixture_copy("restart");
    let calls = Arc::new(AtomicUsize::new(0));
    let service = start(&folder, &cache, calls.clone());
    let identity = format!("{}|{}", ConceptEmbedder::stand_in().identity(), richos_search::chunk::CHUNKER_IDENTITY);
    let dir = index_dir(&cache, &folder.canonicalize().unwrap(), &identity);
    service.shutdown();

    // What a kill in the middle of a batch leaves: one file marked indexing.
    let mut manifest = Manifest::load(&dir, &identity).unwrap().unwrap();
    let total_files = manifest.sources.len();
    let (id, entry) = manifest.sources.iter().find(|(_, e)| e.rel_path == "2025/refund-window.md").map(|(k, v)| (k.clone(), v.clone())).unwrap();
    manifest.sources.insert(id.clone(), SourceEntry { state: SourceState::Indexing, ..entry.clone() });
    manifest.save(&dir).unwrap();
    // And a file changed while the app was not running.
    std::fs::write(folder.join("2024/lisbon-trip.md"), "# Lisbon\nThe flight moved to Tuesday.").unwrap();

    calls.store(0, Ordering::SeqCst);
    let again = start(&folder, &cache, calls.clone());
    assert!(matches!(again.status().meaning, MeaningStatus::Ready { files: 10, .. }));
    // Only the two files were embedded again (one call per passage), not the whole folder.
    let redone = calls.load(Ordering::SeqCst);
    assert!((2..=3).contains(&redone), "{redone} embedding calls");
    let manifest = Manifest::load(&dir, &identity).unwrap().unwrap();
    assert_eq!(manifest.sources.len(), total_files, "no duplicate source after reconciliation");
    assert!(manifest.sources.values().all(|e| e.state == SourceState::Complete));
    let lisbon = manifest.sources.values().find(|e| e.rel_path == "2024/lisbon-trip.md").unwrap();
    assert_eq!(lisbon.points[0], point_id(&richos_search::corpus::source_id("2024/lisbon-trip.md"), &lisbon.hash, 0));
    let r = ask(&again, "flight tuesday", true, false);
    assert!(r.hits.iter().any(|h| h.passage.text.contains("Tuesday")));
    again.shutdown();
    std::fs::remove_dir_all(folder.parent().unwrap()).unwrap();
}
