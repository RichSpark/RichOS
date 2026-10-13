//! The search service the palette and Rich's tool both call: one folder, one text list, one
//! meaning index, and the truth about which of them is ready.
//!
//! Search PRD v3.1. The parts this file owns:
//!
//! - **Text first.** The folder is read into memory on a background thread at start; text
//!   search answers from then on, whatever happens to the model or the index (*"Text search
//!   must work independently of Edge availability and model startup."*).
//! - **Meaning second.** On the same thread, after the text: load the embedder, open the index
//!   for this folder and this embedder, reconcile it with the folder by content hash
//!   (*"replace changed files' passage sets, remove missing files and keep compatible unchanged
//!   work … on startup redo any file still marked indexing"*), then index what is missing, in
//!   small batches, flushing before a batch is marked complete, and optimize once at the end.
//! - **Truthful status.** [`Status`] says whether meaning search is ready, indexing (with
//!   counts) or unavailable (with the reason), and where the index is kept.
//! - **Truthful fallback.** A request with Super Finder checked while meaning search is
//!   unavailable is answered with text matches and says so ([`SearchResponse::effective`]).
//! - **Current sources only.** Every result on a returned page is checked against the file on
//!   disk (still inside the folder, still a regular file, same content hash) and dropped if not;
//!   opening a result checks the same again.
use crate::chunk::CHUNKER_IDENTITY;
use crate::corpus::{self, Corpus};
use crate::embed::Embedder;
use crate::manifest::{point_id, Manifest, SourceEntry, SourceState};
use crate::query::{self, ParsedQuery};
use crate::rank::{self, Ranked, SemanticHit, Why};
use crate::vector::VectorIndex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

/// Retrieval settings the harness may vary; the app uses [`Tuning::default`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// Passages asked of the vector index per query.
    pub semantic_passages: usize,
    /// Distinct documents kept from them, best first.
    pub semantic_sources: usize,
    /// A passage scoring at or below this cosine similarity is not a meaning match. Zero keeps
    /// every passage with any similarity; the harness measures whether a floor helps.
    pub min_score: f32,
    /// Files indexed between two flushes of the index and the manifest.
    pub batch_files: usize,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning { semantic_passages: 120, semantic_sources: 40, min_score: 0.0, batch_files: 16 }
    }
}

/// Longest passage text a result carries (characters). Passages are at most
/// [`crate::chunk::MAX_CHARS`] already; this bounds the reply even if that changes.
pub const MAX_PASSAGE_CHARS: usize = 1400;
/// Largest page a caller may ask for.
pub const MAX_PAGE: usize = 50;
/// How long indexing steps aside before each passage while [`Config::busy`] says a turn is
/// running.
pub const BUSY_YIELD: std::time::Duration = std::time::Duration::from_millis(100);

pub type MakeEmbedder = Box<dyn Fn() -> Result<Box<dyn Embedder>, String> + Send + Sync>;
pub type Busy = Arc<dyn Fn() -> bool + Send + Sync>;

pub struct Config {
    /// `RICHOS_SEARCH_DOCUMENTS_ROOT`, or `None` when unset.
    pub root: Option<PathBuf>,
    /// The resolved `search` cache folder, or why it could not be resolved.
    pub cache: Result<PathBuf, String>,
    pub make_embedder: MakeEmbedder,
    /// Asked before each passage while indexing; `true` (a turn is running) makes indexing
    /// yield [`BUSY_YIELD`] first. Information, not a scheduler (PRD: *"`work_gate.rs` supplies
    /// busy information, not a scheduler"*): indexing slows while he is talking to Rich and
    /// never stops, and its thread already runs at utility priority.
    pub busy: Option<Busy>,
    pub tuning: Tuning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FolderStatus {
    /// No folder configured: the palette says "No search folder configured".
    None,
    Loading,
    Ready { files: usize, passages: usize, skipped: usize, links_skipped: usize },
    Error { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MeaningStatus {
    /// Waiting for the text to load, or loading the model.
    Starting,
    Indexing { files_done: usize, files_total: usize, passages_done: usize, passages_total: usize },
    Ready { files: usize, passages: usize },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    pub folder: FolderStatus,
    pub meaning: MeaningStatus,
    /// Where the index is kept (resolved `search` cache), or why it could not be resolved.
    pub index_location: String,
    /// Changes whenever the searchable set changes; part of a result's identity.
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SearchRequest {
    pub query: String,
    pub super_finder: bool,
    pub strict: bool,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    12
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Mode {
    pub super_finder: bool,
    pub strict: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PassageView {
    pub heading: String,
    pub text: String,
    /// 1-based line of the passage in its file.
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DocHit {
    pub source_id: String,
    pub title: String,
    /// Path inside the search folder, for display.
    pub rel_path: String,
    pub modified_ms: u64,
    pub why: Why,
    /// The quoted spans this document contains, as folded.
    pub exact: Vec<String>,
    pub passage: PassageView,
}

/// Why a search returned no documents when it did, said in words by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Empty {
    /// Nothing typed.
    NoQuery,
    /// No folder, or the folder is unusable: see the status.
    NoFolder,
    /// Require quoted text is checked and a quote mark has no partner.
    QuoteMismatch,
    /// Require quoted text is checked and no document has every quoted span.
    NoExact,
    NoMatch,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchResponse {
    pub requested: Mode,
    /// What was actually run: Super Finder is off here when it was asked for and meaning search
    /// is unavailable, which the caller must say (*"Super Finder unavailable; showing text
    /// matches"*).
    pub effective: Mode,
    pub status: Status,
    /// Documents in the whole ranked list (the page is `hits`).
    pub total: usize,
    pub offset: usize,
    pub hits: Vec<DocHit>,
    /// Ranked results passed over while filling this page because their file changed or left
    /// the folder. The page holds `limit` current results unless the list ran out first.
    pub dropped: usize,
    pub empty: Option<Empty>,
    /// The quoted spans and words the query was read as, for marking the excerpts.
    pub spans: Vec<String>,
    pub words: Vec<String>,
    /// Time in the service, query embedding included.
    pub elapsed_ms: f64,
}

/// What the meaning half holds once its model is loaded and its index open.
struct Meaning {
    index: VectorIndex,
    embedder: Box<dyn Embedder>,
    /// Point id -> (document index in the corpus, passage number), complete sources only.
    points: RwLock<HashMap<u64, (usize, usize)>>,
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    query: String,
    super_finder: bool,
    strict: bool,
    generation: u64,
}

struct Inner {
    config: Config,
    corpus: RwLock<Option<Arc<Corpus>>>,
    folder: RwLock<FolderStatus>,
    meaning_status: RwLock<MeaningStatus>,
    meaning: RwLock<Option<Arc<Meaning>>>,
    generation: AtomicU64,
    stop: AtomicBool,
    last: Mutex<Option<(CacheKey, Arc<Vec<Ranked>>)>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct SearchService(Arc<Inner>);

/// The index folder for one search folder and one embedder: a different model or chunking, or
/// a different folder, is a different index, so vectors are never mixed.
pub fn index_dir(cache: &Path, root: &Path, identity: &str) -> PathBuf {
    let digest = Sha256::digest(format!("{identity}\0{}", root.display()).as_bytes());
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    cache.join(format!("index-{hex}"))
}

#[cfg(target_os = "macos")]
fn lower_this_thread() {
    // QOS_CLASS_UTILITY (sys/qos.h): long-running work the user is not waiting on. The kernel
    // runs it behind interactive work and on efficiency cores where it can.
    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    unsafe {
        pthread_set_qos_class_self_np(0x11, 0);
    }
}

#[cfg(not(target_os = "macos"))]
fn lower_this_thread() {}

impl SearchService {
    /// Start the service: reading the folder and building the meaning index happen on a
    /// background thread; this returns at once.
    pub fn start(config: Config) -> Self {
        let service = SearchService(Arc::new(Inner {
            folder: RwLock::new(if config.root.is_some() { FolderStatus::Loading } else { FolderStatus::None }),
            config,
            corpus: RwLock::new(None),
            meaning_status: RwLock::new(MeaningStatus::Starting),
            meaning: RwLock::new(None),
            generation: AtomicU64::new(1),
            stop: AtomicBool::new(false),
            last: Mutex::new(None),
            worker: Mutex::new(None),
        }));
        service.spawn(false);
        service
    }

    fn spawn(&self, rebuild: bool) {
        let me = self.clone();
        let handle = std::thread::Builder::new()
            .name("richos-search".into())
            .spawn(move || {
                lower_this_thread();
                me.run(rebuild);
            })
            .expect("the search thread could not start");
        *self.0.worker.lock().unwrap() = Some(handle);
    }

    /// Stop background work and wait for it; the index is flushed as it closes.
    pub fn shutdown(&self) {
        self.0.stop.store(true, Ordering::SeqCst);
        let handle = self.0.worker.lock().unwrap().take();
        if let Some(handle) = handle {
            if handle.join().is_err() {
                eprintln!("document search: the search thread ended in a panic");
            }
        }
        *self.0.meaning.write().unwrap() = None;
    }

    /// Explicit Reindex: re-read the folder and rebuild the meaning index from nothing.
    pub fn reindex(&self) {
        self.shutdown();
        self.0.stop.store(false, Ordering::SeqCst);
        *self.0.corpus.write().unwrap() = None;
        *self.0.meaning_status.write().unwrap() = MeaningStatus::Starting;
        if self.0.config.root.is_some() {
            *self.0.folder.write().unwrap() = FolderStatus::Loading;
        }
        self.bump();
        self.spawn(true);
    }

    /// Wait (for tests and the harness) until background work has finished.
    pub fn wait_idle(&self) {
        let handle = self.0.worker.lock().unwrap().take();
        if let Some(handle) = handle {
            if handle.join().is_err() {
                eprintln!("document search: the search thread ended in a panic");
            }
        }
    }

    fn bump(&self) {
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        *self.0.last.lock().unwrap() = None;
    }

    fn stopping(&self) -> bool {
        self.0.stop.load(Ordering::SeqCst)
    }

    fn set_meaning(&self, status: MeaningStatus) {
        *self.0.meaning_status.write().unwrap() = status;
    }

    fn run(&self, rebuild: bool) {
        let Some(root) = self.0.config.root.clone() else { return };
        match corpus::load(&root) {
            Ok(c) => {
                *self.0.folder.write().unwrap() = FolderStatus::Ready {
                    files: c.docs.len(),
                    passages: c.passage_count(),
                    skipped: c.skipped.len(),
                    links_skipped: c.links_skipped,
                };
                *self.0.corpus.write().unwrap() = Some(Arc::new(c));
                self.bump();
            }
            Err(message) => {
                *self.0.folder.write().unwrap() = FolderStatus::Error { message };
                self.set_meaning(MeaningStatus::Unavailable { reason: "No search folder to index.".into() });
                return;
            }
        }
        if let Err(reason) = self.build_meaning(rebuild) {
            *self.0.meaning.write().unwrap() = None;
            self.set_meaning(MeaningStatus::Unavailable { reason });
            self.bump();
        }
    }

    fn build_meaning(&self, rebuild: bool) -> Result<(), String> {
        let corpus = self.0.corpus.read().unwrap().clone().ok_or("The folder was not read.")?;
        let cache = self.0.config.cache.clone()?;
        let embedder = (self.0.config.make_embedder)()?;
        let identity = format!("{}|{}", embedder.identity(), CHUNKER_IDENTITY);
        let dir = index_dir(&cache, &corpus.root, &identity);
        if rebuild && dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("The old search index could not be removed: {e}"))?;
        }
        let (index, mut manifest) = open_index(&dir, embedder.dimension(), &identity)?;
        let meaning = Arc::new(Meaning { index, embedder, points: RwLock::new(HashMap::new()) });

        // Reconcile what the index holds with the folder as it is now.
        let by_id = corpus.by_id();
        let mut stale: Vec<u64> = Vec::new();
        let mut keep: HashMap<u64, (usize, usize)> = HashMap::new();
        manifest.sources.retain(|id, entry| {
            let current = by_id.get(id.as_str()).map(|&i| &corpus.docs[i]);
            match current {
                Some(doc) if entry.state == SourceState::Complete && entry.hash == doc.hash && entry.points.len() == doc.passages.len() => {
                    let i = by_id[id.as_str()];
                    for (p, point) in entry.points.iter().enumerate() {
                        keep.insert(*point, (i, p));
                    }
                    true
                }
                _ => {
                    stale.extend(entry.points.iter().copied());
                    false
                }
            }
        });
        meaning.index.delete(&stale)?;
        meaning.index.flush()?;
        manifest.save(&dir)?;
        *meaning.points.write().unwrap() = keep;
        *self.0.meaning.write().unwrap() = Some(meaning.clone());

        let todo: Vec<usize> = (0..corpus.docs.len()).filter(|&i| !manifest.sources.contains_key(&corpus.docs[i].id)).collect();
        let files_total = corpus.docs.len();
        let passages_total = corpus.passage_count();
        let mut files_done = files_total - todo.len();
        let mut passages_done: usize = manifest.sources.values().map(|e| e.points.len()).sum();
        self.bump();
        let report = |files_done: usize, passages_done: usize| {
            if files_done == files_total {
                MeaningStatus::Ready { files: files_total, passages: passages_total }
            } else {
                MeaningStatus::Indexing { files_done, files_total, passages_done, passages_total }
            }
        };
        self.set_meaning(report(files_done, passages_done));

        for batch in todo.chunks(self.0.config.tuning.batch_files.max(1)) {
            // Mark the batch indexing, with the points it will write, before writing any.
            for &i in batch {
                let doc = &corpus.docs[i];
                manifest.sources.insert(
                    doc.id.clone(),
                    SourceEntry {
                        rel_path: doc.rel_path.clone(),
                        hash: doc.hash.clone(),
                        state: SourceState::Indexing,
                        points: (0..doc.passages.len()).map(|p| point_id(&doc.id, &doc.hash, p)).collect(),
                        locators: doc.passages.iter().map(|p| (p.start, p.end, p.line)).collect(),
                    },
                );
            }
            manifest.save(&dir)?;
            for &i in batch {
                let doc = &corpus.docs[i];
                for (p, passage) in doc.passages.iter().enumerate() {
                    if self.0.config.busy.as_ref().is_some_and(|busy| busy()) {
                        std::thread::sleep(BUSY_YIELD);
                    }
                    if self.stopping() {
                        return Ok(());
                    }
                    let text = embedding_text(&doc.title, &passage.heading, passage.text(&doc.text));
                    let vector = meaning.embedder.embed(&[&text])?.pop().ok_or("The embedding model returned nothing.")?;
                    meaning.index.upsert(vec![(point_id(&doc.id, &doc.hash, p), vector)])?;
                }
            }
            meaning.index.flush()?;
            {
                let mut points = meaning.points.write().unwrap();
                for &i in batch {
                    let doc = &corpus.docs[i];
                    if let Some(entry) = manifest.sources.get_mut(&doc.id) {
                        entry.state = SourceState::Complete;
                        for (p, point) in entry.points.iter().enumerate() {
                            points.insert(*point, (i, p));
                        }
                    }
                    files_done += 1;
                    passages_done += doc.passages.len();
                }
            }
            manifest.save(&dir)?;
            self.bump();
            self.set_meaning(report(files_done, passages_done));
        }
        if !todo.is_empty() && !self.stopping() {
            meaning.index.optimize()?;
        }
        self.set_meaning(report(files_done, passages_done));
        Ok(())
    }

    pub fn status(&self) -> Status {
        Status {
            folder: self.0.folder.read().unwrap().clone(),
            meaning: self.0.meaning_status.read().unwrap().clone(),
            index_location: match &self.0.config.cache {
                Ok(p) => p.display().to_string(),
                Err(e) => e.clone(),
            },
            generation: self.0.generation.load(Ordering::SeqCst),
        }
    }

    /// The meaning half, when it can answer: a model loaded and at least the reconciled part of
    /// an index open. While indexing, it answers from the files already complete.
    fn meaning_for_query(&self) -> Option<Arc<Meaning>> {
        match &*self.0.meaning_status.read().unwrap() {
            MeaningStatus::Ready { .. } | MeaningStatus::Indexing { .. } => self.0.meaning.read().unwrap().clone(),
            _ => None,
        }
    }

    pub fn search(&self, request: &SearchRequest) -> SearchResponse {
        let started = Instant::now();
        let parsed = query::parse(&request.query);
        let requested = Mode { super_finder: request.super_finder, strict: request.strict };
        let corpus = self.0.corpus.read().unwrap().clone();
        let meaning = if request.super_finder { self.meaning_for_query() } else { None };
        let effective = Mode { super_finder: meaning.is_some(), strict: request.strict };
        let mut response = SearchResponse {
            requested,
            effective,
            status: self.status(),
            total: 0,
            offset: request.offset,
            hits: Vec::new(),
            dropped: 0,
            empty: None,
            spans: parsed.spans.clone(),
            words: parsed.words.clone(),
            elapsed_ms: 0.0,
        };
        let Some(corpus) = corpus else {
            response.empty = Some(Empty::NoFolder);
            return finish(response, started);
        };
        if parsed.is_empty() {
            response.empty = Some(Empty::NoQuery);
            return finish(response, started);
        }
        if request.strict && parsed.unmatched_quote {
            response.empty = Some(Empty::QuoteMismatch);
            return finish(response, started);
        }
        let key = CacheKey {
            query: request.query.trim().to_string(),
            super_finder: effective.super_finder,
            strict: request.strict,
            generation: response.status.generation,
        };
        let cached = self.0.last.lock().unwrap().as_ref().filter(|(k, _)| *k == key).map(|(_, r)| r.clone());
        let ranked = match cached {
            Some(r) => r,
            None => {
                let lexical = rank::lexical(&corpus, &parsed, request.strict);
                let semantic = meaning.as_ref().map(|m| semantic_hits(m, &request.query, &self.0.config.tuning));
                let semantic = match semantic {
                    Some(Ok(hits)) => Some(hits),
                    Some(Err(_)) => {
                        // The model failed on this query: answer with text and say so.
                        response.effective.super_finder = false;
                        None
                    }
                    None => None,
                };
                let ranked = Arc::new(rank::rank(&corpus, &parsed, request.strict, &lexical, semantic.as_deref()));
                if response.effective.super_finder == effective.super_finder {
                    *self.0.last.lock().unwrap() = Some((key, ranked.clone()));
                }
                ranked
            }
        };
        response.total = ranked.len();
        if ranked.is_empty() {
            response.empty = Some(if request.strict && !parsed.spans.is_empty() { Empty::NoExact } else { Empty::NoMatch });
            return finish(response, started);
        }
        // The page is filled from the rows after a dropped one, so an empty page means no
        // current document is left from `offset` on, never that the first rows looked at had
        // left the folder (fifth review: the first twelve deleted gave an empty first page and
        // "No documents match." while two still matched). `offset + hits + dropped` is where
        // this page stopped looking, the next page's offset.
        let limit = request.limit.clamp(1, MAX_PAGE);
        for row in ranked.iter().skip(request.offset) {
            if response.hits.len() == limit {
                break;
            }
            let doc = &corpus.docs[row.doc];
            if !still_current(&corpus.root, doc) {
                response.dropped += 1;
                continue;
            }
            response.hits.push(hit(doc, row, &parsed));
        }
        finish(response, started)
    }

    /// Which of `texts` is closest in meaning to `query`, by the same model the index uses:
    /// for a passage matched by meaning alone, which stretch of it to show when only part fits.
    /// `None` when meaning search is not available or the model fails; the caller then keeps
    /// the passage's beginning.
    pub fn closest_in_meaning(&self, query: &str, texts: &[&str]) -> Option<usize> {
        let meaning = self.meaning_for_query()?;
        let mut all = Vec::with_capacity(texts.len() + 1);
        all.push(query.trim());
        all.extend_from_slice(texts);
        let vectors = meaning.embedder.embed(&all).ok()?;
        let (q, rest) = vectors.split_first()?;
        let dot = |v: &Vec<f32>| v.iter().zip(q).map(|(a, b)| a * b).sum::<f32>();
        rest.iter().enumerate().max_by(|a, b| dot(a.1).total_cmp(&dot(b.1)).then(b.0.cmp(&a.0))).map(|(i, _)| i)
    }

    /// The file behind a result, for Open original: found by its source ID, resolved inside the
    /// folder, and refused unless it is still the version that was searched.
    pub fn resolve_for_open(&self, source_id: &str) -> Result<PathBuf, String> {
        let corpus = self.0.corpus.read().unwrap().clone().ok_or("No search folder is configured.")?;
        let doc = corpus.docs.iter().find(|d| d.id == source_id).ok_or("That document is not in the search folder.")?;
        let path = corpus::resolve_inside(&corpus.root, &doc.rel_path)?;
        let bytes = std::fs::read(&path).map_err(|_| "That document could not be read.".to_string())?;
        if corpus::content_hash(&bytes) != doc.hash {
            return Err("That document changed after it was searched. Search again to see it as it is now.".into());
        }
        Ok(path)
    }
}

fn finish(mut response: SearchResponse, started: Instant) -> SearchResponse {
    response.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    response
}

fn open_index(dir: &Path, dimension: usize, identity: &str) -> Result<(VectorIndex, Manifest), String> {
    match Manifest::load(dir, identity) {
        Ok(Some(m)) => Ok((VectorIndex::open(dir, dimension)?, m)),
        Ok(None) => {
            // No manifest: whatever points a shard holds cannot be accounted for. Start clean.
            if let Err(error) = std::fs::remove_dir_all(dir.join("shard")) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("search index: an unaccounted shard could not be removed: {error}");
                }
            }
            Ok((VectorIndex::open(dir, dimension)?, Manifest::new(identity)))
        }
        Err(_) => {
            // A damaged or foreign manifest: the index is derived data, so rebuild it.
            if let Err(error) = std::fs::remove_dir_all(dir) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("search index: a damaged index could not be removed: {error}");
                }
            }
            Ok((VectorIndex::open(dir, dimension)?, Manifest::new(identity)))
        }
    }
}

/// What is embedded for a passage: the file's title and the heading it sits under give a short
/// passage its subject.
pub fn embedding_text(title: &str, heading: &str, passage: &str) -> String {
    match (title.is_empty(), heading.is_empty() || heading == title) {
        (false, false) => format!("{title}. {heading}.\n{passage}"),
        (false, true) => format!("{title}.\n{passage}"),
        _ => passage.to_string(),
    }
}

fn semantic_hits(meaning: &Meaning, query: &str, tuning: &Tuning) -> Result<Vec<SemanticHit>, String> {
    let vector = meaning.embedder.embed(&[query.trim()])?.pop().ok_or("no query vector")?;
    let nearest = meaning.index.nearest(vector, tuning.semantic_passages)?;
    let points = meaning.points.read().unwrap();
    let mut out: Vec<SemanticHit> = Vec::new();
    for (id, score) in nearest {
        if score <= tuning.min_score {
            continue;
        }
        // A point not in the map belongs to a file that is not complete; it is never returned.
        let Some(&(doc, passage)) = points.get(&id) else { continue };
        if out.iter().any(|h| h.doc == doc) {
            continue;
        }
        out.push(SemanticHit { doc, passage });
        if out.len() >= tuning.semantic_sources {
            break;
        }
    }
    Ok(out)
}

/// Is the file still the one that was read: inside the folder, a regular file, same content?
fn still_current(root: &Path, doc: &corpus::Document) -> bool {
    let Ok(path) = corpus::resolve_inside(root, &doc.rel_path) else { return false };
    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() == doc.size => {}
        _ => return false,
    }
    std::fs::read(&path).map(|bytes| corpus::content_hash(&bytes) == doc.hash).unwrap_or(false)
}

fn hit(doc: &corpus::Document, row: &Ranked, parsed: &ParsedQuery) -> DocHit {
    let p = rank::best_passage(doc, parsed, row.semantic_passage);
    let (heading, text, line) = match doc.passages.get(p) {
        Some(passage) => (passage.heading.clone(), passage.text(&doc.text).to_string(), passage.line),
        None => (String::new(), String::new(), 1),
    };
    let text: String = if text.chars().count() > MAX_PASSAGE_CHARS { text.chars().take(MAX_PASSAGE_CHARS).collect::<String>() + "…" } else { text };
    DocHit {
        source_id: doc.id.clone(),
        title: doc.title.clone(),
        rel_path: doc.rel_path.clone(),
        modified_ms: doc.modified_ms,
        why: row.why,
        exact: parsed
            .spans
            .iter()
            .filter(|s| crate::text::has_span(&doc.folded_title, s) || crate::text::has_span(&doc.folded_text, s))
            .cloned()
            .collect(),
        passage: PassageView { heading, text, line },
    }
}
