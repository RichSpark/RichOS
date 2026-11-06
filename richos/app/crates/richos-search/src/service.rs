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
use crate::corpus::{self, Corpus, Document, FileVersion, MetadataSource};
use crate::embed::Embedder;
use crate::manifest::{point_id, Manifest, SourceEntry, SourceState};
use crate::query::{self, ParsedQuery};
use crate::rank::{self, Ranked, SemanticHit, Why};
use crate::vector::VectorIndex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, RwLock};
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
pub type Grant = Box<dyn Fn() -> bool + Send + Sync>;
/// Local relevance scores for the already retrieved sources, in candidate order.
pub type Reranker = Arc<ScorePassages>;
type ScorePassages = dyn Fn(&str, &[String], &dyn Fn() -> bool) -> Option<Vec<f32>> + Send + Sync;

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
    /// Use source identity and version, since a catalog edit can move a document's position.
    points: RwLock<HashMap<u64, (Arc<Document>, usize)>>,
}

struct IndexWriter {
    meaning: Arc<Meaning>,
    manifest: Manifest,
    dir: PathBuf,
}

struct MediaUpdate {
    path: String,
    doc: Result<Option<Arc<Document>>, String>,
}

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    query: String,
    super_finder: bool,
    strict: bool,
    generation: u64,
}

struct CachedRanking {
    snapshot: Arc<SearchableCorpus>,
    rows: Vec<Ranked>,
    query_vector: Option<Vec<f32>>,
    selected_passages: HashMap<usize, usize>,
}

struct SearchableCorpus {
    corpus: Arc<Corpus>,
    lexical: Arc<rank::LexicalIndex>,
    positions: Option<rank::UpdatedPositions>,
}

struct Inner {
    config: Config,
    metadata: Option<MetadataSource>,
    reranker: Option<Reranker>,
    corpus: RwLock<Option<Arc<SearchableCorpus>>>,
    folder: RwLock<FolderStatus>,
    meaning_status: RwLock<MeaningStatus>,
    meaning: RwLock<Option<Arc<Meaning>>>,
    generation: AtomicU64,
    stop: AtomicBool,
    grant: Option<Grant>,
    access: Mutex<()>,
    last: Mutex<Option<(CacheKey, Arc<CachedRanking>)>>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    initial: (Mutex<bool>, Condvar),
}

impl Inner {
    fn initial_ready(&self) {
        *self.initial.0.lock().unwrap() = true;
        self.initial.1.notify_all();
    }
}

struct InitialRead<'a>(&'a Inner);

impl Drop for InitialRead<'_> {
    fn drop(&mut self) { self.0.initial_ready(); }
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
        Self::start_with_grant(config, None, None, None, || {})
    }

    /// Read the corpus right away, then wait for the host's previous index to close.
    /// Background reads and embeddings check the grant even when nobody is searching.
    pub fn start_with_grant(
        config: Config,
        metadata: Option<MetadataSource>,
        reranker: Option<Reranker>,
        grant: Option<Grant>,
        before_index: impl FnOnce() + Send + 'static,
    ) -> Self {
        let service = SearchService(Arc::new(Inner {
            folder: RwLock::new(if config.root.is_some() { FolderStatus::Loading } else { FolderStatus::None }),
            config,
            metadata,
            reranker,
            corpus: RwLock::new(None),
            meaning_status: RwLock::new(MeaningStatus::Starting),
            meaning: RwLock::new(None),
            generation: AtomicU64::new(1),
            stop: AtomicBool::new(false),
            grant,
            access: Mutex::new(()),
            last: Mutex::new(None),
            worker: Mutex::new(None),
            initial: (Mutex::new(false), Condvar::new()),
        }));
        service.spawn(false, before_index);
        service
    }

    fn spawn(&self, rebuild: bool, before_index: impl FnOnce() + Send + 'static) {
        let me = self.clone();
        let handle = std::thread::Builder::new()
            .name("richos-search".into())
            .spawn(move || {
                lower_this_thread();
                let _initial = InitialRead(&me.0);
                me.run(rebuild, before_index);
            })
            .expect("the search thread could not start");
        *self.0.worker.lock().unwrap() = Some(handle);
    }

    /// Stop background work and wait for it; the index is flushed as it closes.
    pub fn shutdown(&self) {
        self.request_stop();
        let mut worker = self.0.worker.lock().unwrap();
        if let Some(handle) = worker.take() {
            if handle.join().is_err() {
                eprintln!("document search: the search thread ended in a panic");
            }
        }
        // A query can hold a reference to the old index while its result is being discarded.
        // Release that reference before allowing a replacement to open the same index.
        let _access = self.0.access.lock().unwrap();
        *self.0.meaning.write().unwrap() = None;
    }

    pub fn request_stop(&self) {
        self.0.stop.store(true, Ordering::SeqCst);
    }

    /// Explicit Reindex: re-read the folder and rebuild the meaning index from nothing.
    pub fn reindex(&self) {
        self.shutdown();
        self.0.stop.store(false, Ordering::SeqCst);
        *self.0.initial.0.lock().unwrap() = false;
        *self.0.corpus.write().unwrap() = None;
        *self.0.meaning_status.write().unwrap() = MeaningStatus::Starting;
        if self.0.config.root.is_some() {
            *self.0.folder.write().unwrap() = FolderStatus::Loading;
        }
        self.bump();
        self.spawn(true, || {});
    }

    /// Wait (for tests and the harness) until the initial corpus and index have been processed.
    pub fn wait_idle(&self) {
        let ready = self.0.initial.0.lock().unwrap();
        let _ready = self.0.initial.1.wait_while(ready, |ready| !*ready).unwrap();
    }

    fn bump(&self) {
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        *self.0.last.lock().unwrap() = None;
    }

    pub fn stopping(&self) -> bool {
        self.0.stop.load(Ordering::SeqCst)
    }

    fn background_stopping(&self) -> bool {
        if self.stopping() {
            return true;
        }
        if self.0.grant.as_ref().is_some_and(|authorized| !authorized()) {
            if !self.0.stop.swap(true, Ordering::SeqCst) {
                eprintln!("[richos] document search: background grant revoked");
            }
            return true;
        }
        self.stopping()
    }

    fn set_meaning(&self, status: MeaningStatus) {
        *self.0.meaning_status.write().unwrap() = status;
    }

    fn run(&self, rebuild: bool, before_index: impl FnOnce()) {
        let Some(root) = self.0.config.root.clone() else { return };
        let metadata = self.0.metadata.as_ref();
        let changes = metadata.map(|source| (source.watch)());
        let pending = match corpus::load_until_stopped(&root, metadata, || self.background_stopping()) {
            Ok(Some(c)) => c,
            Ok(None) => return,
            Err(message) => {
                *self.0.folder.write().unwrap() = FolderStatus::Error { message };
                self.set_meaning(MeaningStatus::Unavailable { reason: "No search folder to index.".into() });
                return;
            }
        };
        let base = Arc::new(pending.corpus);
        let Some(lexical) = rank::LexicalIndex::build(&base, || self.background_stopping()) else { return };
        self.publish_corpus(SearchableCorpus { corpus: base.clone(), lexical: Arc::new(lexical), positions: None });

        let (mut writer, initial_media) = std::thread::scope(|scope| {
            let reader = metadata.map(|source| {
                let root = &base.root;
                let paths = &pending.media;
                scope.spawn(move || {
                    lower_this_thread();
                    self.read_media(root, paths, source)
                })
            });
            before_index();
            let pending_paths: HashSet<_> = pending.media.iter().map(String::as_str).collect();
            let writer = if self.background_stopping() {
                None
            } else {
                match self.build_meaning(rebuild, &base, &pending_paths) {
                    Ok(writer) => writer,
                    Err(reason) => {
                        self.meaning_failed(reason);
                        None
                    }
                }
            };
            let initial = reader.map(|reader| reader.join().expect("metadata reader panicked")).unwrap_or_default();
            (writer, initial)
        });
        self.update_media_index(&mut writer, &initial_media);
        self.0.initial_ready();
        if let (Some(source), Some(changes)) = (metadata, changes) {
            while !self.stopping() {
                let paths = match changes.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(paths) => paths,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let paths: BTreeSet<_> = paths.into_iter().chain(changes.try_iter().flatten())
                    .filter_map(|path| corpus::catalog_relative(&base.root, &path, source)).collect();
                if !paths.is_empty() {
                    let updates = self.read_media(&base.root, &paths.into_iter().collect::<Vec<_>>(), source);
                    self.update_media_index(&mut writer, &updates);
                }
            }
        }
    }

    fn publish_corpus(&self, snapshot: SearchableCorpus) {
        let corpus = &snapshot.corpus;
        let status = FolderStatus::Ready {
            files: corpus.docs.len(),
            passages: corpus.passage_count(),
            skipped: corpus.skipped.len(),
            links_skipped: corpus.links_skipped,
        };
        *self.0.corpus.write().unwrap() = Some(Arc::new(snapshot));
        *self.0.folder.write().unwrap() = status;
        self.bump();
    }

    fn meaning_failed(&self, reason: String) {
        *self.0.meaning.write().unwrap() = None;
        self.set_meaning(MeaningStatus::Unavailable { reason });
        self.bump();
    }

    fn build_meaning(&self, rebuild: bool, corpus: &Corpus, pending_media: &HashSet<&str>) -> Result<Option<IndexWriter>, String> {
        let cache = self.0.config.cache.clone()?;
        let embedder = (self.0.config.make_embedder)()?;
        embedder.prepare(&|| self.background_stopping())?;
        if self.background_stopping() {
            return Ok(None);
        }
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
        let mut keep = HashMap::new();
        manifest.sources.retain(|id, entry| {
            if pending_media.contains(entry.rel_path.as_str()) {
                // Reuse the vectors once the portable or catalog note verifies.
                return true;
            }
            let current = by_id.get(id.as_str()).map(|&i| &corpus.docs[i]);
            match current {
                Some(doc) if entry.state == SourceState::Complete && entry.hash == doc.hash && entry.points.len() == doc.passages.len() => {
                    for (p, point) in entry.points.iter().enumerate() {
                        keep.insert(*point, (doc.clone(), p));
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
        let mut passages_done = keep.len();
        *meaning.points.write().unwrap() = keep;
        *self.0.meaning.write().unwrap() = Some(meaning.clone());

        let todo: Vec<usize> = (0..corpus.docs.len()).filter(|&i| !manifest.sources.contains_key(&corpus.docs[i].id)).collect();
        let files_total = corpus.docs.len();
        let passages_total = corpus.passage_count();
        let mut files_done = files_total - todo.len();
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
            if self.background_stopping() {
                return Ok(None);
            }
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
                    if self.background_stopping() {
                        return Ok(None);
                    }
                    let text = embedding_text(&doc.title, &passage.heading, passage.text(&doc.text));
                    let vector = meaning.embedder.embed_for_index(&[&text], &|| self.background_stopping())?.pop().ok_or("The embedding model returned nothing.")?;
                    if self.background_stopping() {
                        return Ok(None);
                    }
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
                            points.insert(*point, (doc.clone(), p));
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
        if !todo.is_empty() && !self.background_stopping() {
            meaning.index.optimize()?;
        }
        self.set_meaning(report(files_done, passages_done));
        Ok(Some(IndexWriter { meaning, manifest, dir }))
    }

    fn read_media(&self, root: &Path, paths: &[String], source: &MetadataSource) -> Vec<MediaUpdate> {
        let mut updates = Vec::new();
        for batch in paths.chunks(8) {
            let start = updates.len();
            for path in batch {
                if self.background_stopping() {
                    return updates;
                }
                let doc = if root.join(path).exists() {
                    corpus::read_metadata_document(root, path, source)
                } else {
                    Ok(None)
                };
                updates.push(MediaUpdate { path: path.clone(), doc });
            }
            let Some(snapshot) = self.0.corpus.read().unwrap().clone() else { break };
            let mut next = (*snapshot.corpus).clone();
            for update in &updates[start..] {
                let position = next.docs.binary_search_by(|d| d.rel_path.cmp(&update.path));
                next.skipped.retain(|s| s.rel_path != update.path);
                match (&update.doc, position) {
                    (Ok(Some(doc)), Ok(i)) => next.docs[i] = doc.clone(),
                    (Ok(Some(doc)), Err(i)) => next.docs.insert(i, doc.clone()),
                    (_, Ok(i)) => { next.docs.remove(i); }
                    (_, Err(_)) => {}
                }
                if let Err(reason) = &update.doc {
                    next.skipped.push(corpus::Skipped { rel_path: update.path.clone(), reason: reason.clone() });
                }
            }
            if !self.stopping() {
                let positions = rank::UpdatedPositions::for_corpus(&next);
                self.publish_corpus(SearchableCorpus {
                    corpus: Arc::new(next),
                    lexical: snapshot.lexical.clone(),
                    positions: Some(positions),
                });
            }
        }
        updates
    }

    fn update_media_index(&self, writer: &mut Option<IndexWriter>, updates: &[MediaUpdate]) {
        if updates.is_empty() || self.stopping() {
            return;
        }
        if let Some(index) = writer {
            if let Err(reason) = self.write_media(index, updates) {
                *writer = None;
                self.meaning_failed(reason);
            }
        }
    }

    fn write_media(&self, writer: &mut IndexWriter, updates: &[MediaUpdate]) -> Result<(), String> {
        let IndexWriter { meaning, manifest, dir } = writer;
        for update in updates {
            if self.background_stopping() {
                return Ok(());
            }
            let id = corpus::source_id(&update.path);
            let doc = update.doc.as_ref().ok().and_then(Option::as_ref);
            let old = manifest.sources.get(&id);
            if let Some((doc, entry)) = doc.zip(old).filter(|(doc, entry)| {
                entry.state == SourceState::Complete && entry.hash == doc.hash && entry.points.len() == doc.passages.len()
            }) {
                let mut points = meaning.points.write().unwrap();
                for (i, &id) in entry.points.iter().enumerate() {
                    points.insert(id, (doc.clone(), i));
                }
                continue;
            }
            if old.is_none() && doc.is_none() {
                continue;
            }
            let stale = old.map(|entry| entry.points.clone()).unwrap_or_default();
            let fresh: Vec<_> = doc.map(|doc| (0..doc.passages.len()).map(|i| point_id(&id, &doc.hash, i)).collect()).unwrap_or_default();
            let mut pending = stale.clone();
            pending.extend_from_slice(&fresh);
            let locators = doc.map(|doc| doc.passages.iter().map(|p| (p.start, p.end, p.line)).collect()).unwrap_or_default();
            manifest.sources.insert(id.clone(), SourceEntry {
                rel_path: update.path.clone(),
                hash: doc.map(|doc| doc.hash.clone()).unwrap_or_default(),
                state: SourceState::Indexing,
                points: pending,
                locators,
            });
            manifest.save(dir)?;
            {
                let mut points = meaning.points.write().unwrap();
                for id in &stale {
                    points.remove(id);
                }
            }
            meaning.index.delete(&stale)?;
            if let Some(doc) = doc {
                for (passage, &id) in doc.passages.iter().zip(&fresh) {
                    if self.0.config.busy.as_ref().is_some_and(|busy| busy()) {
                        std::thread::sleep(BUSY_YIELD);
                    }
                    if self.background_stopping() {
                        return Ok(());
                    }
                    let text = embedding_text(&doc.title, &passage.heading, passage.text(&doc.text));
                    let vector = meaning.embedder.embed_for_index(&[&text], &|| self.background_stopping())?.pop().ok_or("The embedding model returned nothing.")?;
                    if self.background_stopping() {
                        return Ok(());
                    }
                    meaning.index.upsert(vec![(id, vector)])?;
                }
                meaning.index.flush()?;
                if let Some(entry) = manifest.sources.get_mut(&id) {
                    entry.state = SourceState::Complete;
                    entry.points = fresh.clone();
                }
                manifest.save(dir)?;
                let mut points = meaning.points.write().unwrap();
                for (i, id) in fresh.into_iter().enumerate() {
                    points.insert(id, (doc.clone(), i));
                }
            } else {
                meaning.index.flush()?;
                manifest.sources.remove(&id);
                manifest.save(dir)?;
            }
        }
        if let Some(snapshot) = self.0.corpus.read().unwrap().as_ref() {
            self.set_meaning(MeaningStatus::Ready { files: snapshot.corpus.docs.len(), passages: snapshot.corpus.passage_count() });
        }
        self.bump();
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
        self.search_while_current(request, &|| false).expect("a search without cancellation returns a response")
    }

    pub fn search_while_current(&self, request: &SearchRequest, obsolete: &dyn Fn() -> bool) -> Option<SearchResponse> {
        let started = Instant::now();
        if obsolete() { return None; }
        let _access = self.0.access.lock().unwrap();
        if obsolete() { return None; }
        let parsed = query::parse(&request.query);
        let requested = Mode { super_finder: request.super_finder, strict: request.strict };
        let generation = self.0.generation.load(Ordering::SeqCst);
        let stopped = self.stopping();
        let corpus = if stopped { None } else { self.0.corpus.read().unwrap().clone() };
        let meaning = if request.super_finder && !stopped { self.meaning_for_query() } else { None };
        let effective = Mode { super_finder: meaning.is_some(), strict: request.strict };
        let mut response = SearchResponse {
            requested,
            effective,
            status: Status { generation, ..self.status() },
            total: 0,
            offset: request.offset,
            hits: Vec::new(),
            dropped: 0,
            empty: None,
            spans: parsed.spans.clone(),
            words: parsed.words.clone(),
            elapsed_ms: 0.0,
        };
        let Some(snapshot) = corpus else {
            response.empty = Some(Empty::NoFolder);
            return Some(finish(response, started));
        };
        let corpus = snapshot.corpus.as_ref();
        if parsed.is_empty() {
            response.empty = Some(Empty::NoQuery);
            return Some(finish(response, started));
        }
        if request.strict && parsed.unmatched_quote {
            response.empty = Some(Empty::QuoteMismatch);
            return Some(finish(response, started));
        }
        let key = CacheKey {
            query: request.query.trim().to_string(),
            super_finder: effective.super_finder,
            strict: request.strict,
            generation: response.status.generation,
        };
        let cached = self.0.last.lock().unwrap().as_ref()
            .filter(|(k, ranking)| *k == key && Arc::ptr_eq(&ranking.snapshot, &snapshot))
            .map(|(_, ranking)| ranking.clone());
        let ranked = match cached {
            Some(r) => r,
            None => {
                let lexical = snapshot.lexical.search(corpus, &parsed, request.strict, snapshot.positions.as_ref());
                if obsolete() { return None; }
                let semantic = meaning.as_ref().map(|m| semantic_hits(m, corpus, &request.query, &self.0.config.tuning));
                if obsolete() { return None; }
                let (semantic, query_vector) = match semantic {
                    Some(Ok((hits, query_vector))) => (Some(hits), Some(query_vector)),
                    Some(Err(_)) => {
                        // The model failed on this query: answer with text and say so.
                        response.effective.super_finder = false;
                        (None, None)
                    }
                    None => (None, None),
                };
                let mut rows = rank::rank(corpus, &parsed, request.strict, &lexical, semantic.as_deref());
                let selected_passages = if parsed.spans.is_empty() && !parsed.unmatched_quote && rows.len() > 1 {
                    self.0.reranker.as_ref().zip(meaning.as_deref()).zip(query_vector.as_deref())
                        .map(|((reranker, meaning), vector)| {
                            rerank_sources(&mut rows, corpus, &parsed, request.query.trim(), (meaning, vector), reranker, obsolete)
                        }).unwrap_or_default()
                } else {
                    HashMap::new()
                };
                if obsolete() { return None; }
                let ranked = Arc::new(CachedRanking { snapshot: snapshot.clone(), rows, query_vector, selected_passages });
                let mut last = self.0.last.lock().unwrap();
                if response.effective.super_finder == effective.super_finder
                    && self.0.generation.load(Ordering::SeqCst) == generation
                {
                    *last = Some((key, ranked.clone()));
                }
                ranked
            }
        };
        response.total = ranked.rows.len();
        if ranked.rows.is_empty() {
            response.empty = Some(if request.strict && !parsed.spans.is_empty() { Empty::NoExact } else { Empty::NoMatch });
            return Some(finish(response, started));
        }
        // The page is filled from the rows after a dropped one, so an empty page means no
        // current document is left from `offset` on, never that the first rows looked at had
        // left the folder (fifth review: the first twelve deleted gave an empty first page and
        // "No documents match." while two still matched). `offset + hits + dropped` is where
        // this page stopped looking, the next page's offset.
        let limit = request.limit.clamp(1, MAX_PAGE);
        for row in ranked.rows.iter().skip(request.offset) {
            if obsolete() { return None; }
            if self.stopping() || response.hits.len() == limit {
                break;
            }
            let doc = &corpus.docs[row.doc];
            if !still_current(&corpus.root, doc) {
                response.dropped += 1;
                continue;
            }
            let passage = ranked.selected_passages.get(&row.doc).copied().unwrap_or_else(|| {
                selected_passage(doc, row, &parsed, meaning.as_deref().zip(ranked.query_vector.as_deref()))
            });
            response.hits.push(hit(doc, row, &parsed, passage));
        }
        (!obsolete()).then(|| finish(response, started))
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
        let vectors = meaning.embedder.embed_query(&all).ok()?;
        let (q, rest) = vectors.split_first()?;
        let dot = |v: &Vec<f32>| v.iter().zip(q).map(|(a, b)| a * b).sum::<f32>();
        rest.iter().enumerate().max_by(|a, b| dot(a.1).total_cmp(&dot(b.1)).then(b.0.cmp(&a.0))).map(|(i, _)| i)
    }

    /// The file behind a result, for Open original: found by its source ID, resolved inside the
    /// folder, and refused unless it is still the version that was searched.
    pub fn resolve_for_open(&self, source_id: &str) -> Result<PathBuf, String> {
        let _access = self.0.access.lock().unwrap();
        if self.stopping() {
            return Err("That document is not in the search folder.".into());
        }
        let snapshot = self.0.corpus.read().unwrap().clone().ok_or("No search folder is configured.")?;
        let corpus = &snapshot.corpus;
        let doc = corpus.docs.iter().find(|d| d.id == source_id).ok_or("That document is not in the search folder.")?;
        let path = corpus::resolve_inside(&corpus.root, &doc.rel_path)?;
        let current = if let Some(version) = &doc.metadata_version {
            std::fs::metadata(&path).map(|meta| FileVersion::of(&meta) == *version)
        } else {
            std::fs::read(&path).map(|bytes| corpus::content_hash(&bytes) == doc.hash)
        }.map_err(|_| "That document could not be read.".to_string())?;
        if !current {
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

fn semantic_hits(meaning: &Meaning, corpus: &Corpus, query: &str, tuning: &Tuning) -> Result<(Vec<SemanticHit>, Vec<f32>), String> {
    let mut vector = meaning.embedder.embed_query(&[query.trim()])?.pop().ok_or("no query vector")?;
    let nearest = meaning.index.nearest(vector.clone(), tuning.semantic_passages)?;
    crate::embed::normalize(&mut vector);
    let points = meaning.points.read().unwrap();
    let mut out: Vec<SemanticHit> = Vec::new();
    for (id, score) in nearest {
        if score <= tuning.min_score {
            continue;
        }
        // A point not in the map belongs to a file that is not complete; it is never returned.
        let Some((indexed, passage)) = points.get(&id) else { continue };
        let Some(doc) = corpus.position(&indexed.rel_path) else { continue };
        if corpus.docs[doc].hash != indexed.hash {
            continue;
        }
        if out.iter().any(|h| h.doc == doc) {
            continue;
        }
        out.push(SemanticHit { doc, passage: *passage });
        if out.len() >= tuning.semantic_sources {
            break;
        }
    }
    Ok((out, vector))
}

fn passage_scores(meaning: &Meaning, doc: &corpus::Document, query: &[f32]) -> Option<Vec<f32>> {
    if doc.passages.len() < 2 {
        return None;
    }
    let ids: Vec<_> = (0..doc.passages.len()).map(|i| point_id(&doc.id, &doc.hash, i)).collect();
    if !meaning.points.read().unwrap().contains_key(&ids[0]) {
        return None;
    }
    let mut by_id = HashMap::with_capacity(ids.len());
    // Bound the number of vectors held by Qdrant's response, even for very long files.
    for batch in ids.chunks(256) {
        by_id.extend(meaning.index.score_passages(batch, query).ok()?);
    }
    ids.iter().map(|id| by_id.get(id).copied()).collect()
}

/// Is the file still the one that was read: inside the folder, a regular file, same content?
fn still_current(root: &Path, doc: &corpus::Document) -> bool {
    let Ok(path) = corpus::resolve_inside(root, &doc.rel_path) else { return false };
    same_file_version(&path, doc)
}

fn same_file_version(path: &Path, doc: &corpus::Document) -> bool {
    if let Some(version) = &doc.metadata_version {
        return std::fs::metadata(path).is_ok_and(|meta| FileVersion::of(&meta) == *version);
    }
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() == doc.size => {}
        _ => return false,
    }
    std::fs::read(path).map(|bytes| corpus::content_hash(&bytes) == doc.hash).unwrap_or(false)
}

/// The heading a result shows with its passage: the nearest heading, and, when an outer heading
/// it sits under holds a quoted span or a word of the query, the path from the outermost such
/// heading down to it ("Refunds within 30 days › Final decision"). Ranking matches the whole
/// heading path ([`rank::best_passage`]), so the words that chose the passage are in what the
/// palette and Rich are given (rv-20261014T110606Z-1e6b535c-f031 #2: only "Final decision" was
/// shown, and the quoted "30 days" was nowhere in the result).
fn shown_heading(doc: &corpus::Document, p: usize, parsed: &ParsedQuery) -> String {
    let passage = &doc.passages[p];
    let Some(folded) = doc.folded_passages.get(p) else { return passage.heading.clone() };
    let outer = passage.headings.len().saturating_sub(1);
    let matches = |f: &String| parsed.spans.iter().any(|s| crate::text::has_span(f, s)) || parsed.words.iter().any(|w| crate::text::has_word(f, w));
    match folded.iter().take(outer).position(matches) {
        Some(first) => passage.headings[first..].join(" › "),
        None => passage.heading.clone(),
    }
}

/// The bytes shown for passage `p`: the passage, or, when a quote of the query runs from it into
/// the next passage ([`rank::crosses_into_next`]), the text around that cut, from a word start at
/// most half of [`MAX_PASSAGE_CHARS`] before it, so the whole quote is delivered
/// (rv-20261014T132419Z-a81de6d6-6891 #3).
fn evidence(doc: &corpus::Document, p: usize, parsed: &ParsedQuery) -> (usize, usize) {
    let passage = &doc.passages[p];
    let Some(next) = doc.passages.get(p + 1).filter(|_| rank::crosses_into_next(doc, parsed, p)) else {
        return (passage.start, passage.end);
    };
    let tail = doc.text[passage.start..passage.end].char_indices().rev().nth(MAX_PASSAGE_CHARS / 2 - 1).map_or(passage.start, |(i, _)| passage.start + i);
    let start = if tail > passage.start {
        doc.text[tail..passage.end].char_indices().find(|(_, c)| c.is_whitespace()).map_or(tail, |(i, c)| tail + i + c.len_utf8())
    } else {
        tail
    };
    let end = doc.text[start..next.end].char_indices().nth(MAX_PASSAGE_CHARS).map_or(next.end, |(i, _)| start + i);
    (start, end)
}

fn selected_passage(doc: &corpus::Document, row: &Ranked, parsed: &ParsedQuery, meaning: Option<(&Meaning, &[f32])>) -> usize {
    // A word-only result shows its word evidence. Quoted spans have their own rule.
    let scores = if row.semantic_passage.is_some() && parsed.spans.is_empty() {
        meaning.and_then(|(meaning, vector)| passage_scores(meaning, doc, vector))
    } else {
        None
    };
    rank::best_passage_with_scores(doc, parsed, row.semantic_passage, scores.as_deref())
}

fn rerank_sources(
    rows: &mut [Ranked], corpus: &Corpus, parsed: &ParsedQuery, query: &str,
    meaning: (&Meaning, &[f32]), reranker: &Reranker, obsolete: &dyn Fn() -> bool,
) -> HashMap<usize, usize> {
    const SHORTLIST: usize = 16;
    let count = rows.len().min(SHORTLIST);
    let mut passages = HashMap::with_capacity(count);
    let mut candidates = Vec::with_capacity(count);
    for row in &rows[..count] {
        if obsolete() { return passages; }
        let doc = &corpus.docs[row.doc];
        let passage = selected_passage(doc, row, parsed, Some(meaning));
        passages.insert(row.doc, passage);
        let text = doc.passages.get(passage).map_or("", |p| p.text(&doc.text));
        candidates.push(format!("{}\n{}", doc.title, bounded_passage(text)));
    }
    if let Some(scores) = reranker(query, &candidates, obsolete).filter(|scores| {
        scores.len() == count && scores.iter().all(|score| score.is_finite())
    }) {
        let mut scored: Vec<_> = scores.into_iter().zip(rows[..count].iter().cloned()).collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (row, (_, ranked)) in rows.iter_mut().zip(scored) {
            *row = ranked;
        }
    }
    passages
}

fn bounded_passage(text: &str) -> String {
    if text.chars().count() > MAX_PASSAGE_CHARS {
        text.chars().take(MAX_PASSAGE_CHARS).collect::<String>() + "…"
    } else {
        text.to_string()
    }
}

fn hit(doc: &corpus::Document, row: &Ranked, parsed: &ParsedQuery, p: usize) -> DocHit {
    let (heading, text, line) = match doc.passages.get(p) {
        Some(passage) => {
            let (start, end) = evidence(doc, p, parsed);
            let line = if start == passage.start { passage.line } else { 1 + doc.text[..start].matches('\n').count() as u32 };
            (shown_heading(doc, p, parsed), bounded_passage(&doc.text[start..end]), line)
        }
        None => (String::new(), String::new(), 1),
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::ConceptEmbedder;

    #[test]
    fn cached_rows_are_only_reused_with_the_snapshot_that_ranked_them() {
        let base = std::env::temp_dir().join(format!("richos-search-snapshot-cache-{}", std::process::id()));
        let root = base.join("documents");
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original.md");
        std::fs::write(&original, "unique fixture phrase").unwrap();
        let service = SearchService::start(Config {
            root: Some(root.clone()),
            cache: Ok(base.join("index")),
            make_embedder: Box::new(|| Ok(Box::new(ConceptEmbedder::stand_in()))),
            busy: None,
            tuning: Tuning::default(),
        });
        service.wait_idle();
        let request = SearchRequest { query: "fixture phrase".into(), super_finder: false, strict: false, offset: 0, limit: 1 };
        let initial = service.search(&request);
        assert_eq!(initial.hits[0].rel_path, "original.md");

        std::fs::rename(original, root.join("replacement.md")).unwrap();
        std::fs::write(root.join("alphabetically-first.md"), "unrelated content").unwrap();
        let corpus = Arc::new(corpus::load(&root).unwrap());
        let lexical = Arc::new(rank::LexicalIndex::build(&corpus, || false).unwrap());
        // Reproduce the instant after publication and before its generation bump.
        *service.0.corpus.write().unwrap() = Some(Arc::new(SearchableCorpus { corpus, lexical, positions: None }));
        let replacement = service.search(&request);
        assert_eq!(replacement.status.generation, initial.status.generation);
        assert_eq!(replacement.hits[0].rel_path, "replacement.md");

        service.shutdown();
        std::fs::remove_dir_all(base).unwrap();
    }
}
