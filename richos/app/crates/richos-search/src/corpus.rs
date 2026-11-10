//! The one folder the CEO included, read into memory as documents.
//!
//! Search PRD v3.1, "Indexing and source access": *"Index only the explicitly configured
//! folder's supported files. Repository roots are not an archive grant. Exclude Git internals,
//! dependencies and generated output; preserve linked-path restrictions."*
//!
//! - **Supported files** in slice one are Markdown: `.md` and `.markdown`, any case.
//! - **Skipped folders**: operating system folders, app internals, hidden folders and the
//!   dependency and build-output names in [`SKIPPED_DIRS`]. The folder picker can use the
//!   same [`directory_exclusion`] rule as the scan and opening paths.
//! - **Links are not followed.** A symbolic link, to a file or a folder, is skipped and
//!   counted: a link is the one way a folder can reach outside itself, and the PRD's
//!   *"preserve linked-path restrictions"* is kept by never crossing one.
//! - **Files that cannot be searched are reported, never silently dropped**: too large
//!   ([`MAX_FILE_BYTES`]) or not UTF-8 text each appear in [`Corpus::skipped`] with the reason.
use crate::chunk::{self, Passage};
use crate::text::fold;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::UNIX_EPOCH;

pub const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "__pycache__", "vendor", "Pods"];
/// A Markdown file larger than this is reported as skipped. 16 MB is far above any single
/// conversation the archive's lower bound implies (30+ MB over 1,500+ files, about 20 KB each).
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub type ReadMetadata = dyn Fn(&Path, &Metadata, Option<&Path>) -> Result<Option<String>, String> + Send + Sync;

/// The desktop supplies its format readers. Plain Markdown needs no adapter.
#[derive(Clone)]
pub struct MetadataSource {
    pub extensions: &'static [&'static str],
    pub read: Arc<ReadMetadata>,
    pub watch: Arc<dyn Fn() -> mpsc::Receiver<Vec<PathBuf>> + Send + Sync>,
}

/// An inexpensive file version for metadata-only hits. Media can be large, so a
/// result never re-reads or hashes its payload merely to check its identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileVersion {
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FileVersion {
    pub(crate) fn of(meta: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            identity: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
        }
    }
}

/// A stable identifier for one file: derived from its path inside the folder, so it survives
/// a restart and a Reindex, and carries no path itself (the renderer sends it back to open the
/// file; the host resolves it, PRD "Open original").
pub fn source_id(rel_path: &str) -> String {
    let digest = Sha256::digest(rel_path.as_bytes());
    let hex: String = digest.iter().take(10).map(|b| format!("{b:02x}")).collect();
    format!("doc-{hex}")
}

pub fn content_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone, Debug)]
pub struct Document {
    pub id: String,
    /// Path inside the folder, `/`-separated.
    pub rel_path: String,
    pub title: String,
    pub modified_ms: u64,
    pub size: u64,
    pub hash: String,
    pub text: String,
    /// [`fold`] of the title and of the whole text, matched by the text list.
    pub folded_title: String,
    pub folded_text: String,
    pub passages: Vec<Passage>,
    /// For each passage, the [`fold`] of every heading on its path (outermost first) and then of
    /// its text, one segment each, for choosing the excerpt. The headings are part of what a
    /// passage is matched on because they are part of the document text eligibility reads and
    /// say what the passage is under: a quote found only in `## Refunds within 30 days` picks the
    /// passage under it (fifth review), or under a `### Final decision` below it (sixth review),
    /// never an unrelated agenda with more of the other words, and never a passage in the next
    /// `##` section. They stay separate segments because a quoted span must sit inside one of
    /// them: `## Annual plan` over `### Pricing` is not the phrase "annual plan pricing"
    /// (review rv-20261013T151528Z-71779eae-6117).
    pub folded_passages: Vec<Vec<String>>,
    pub(crate) metadata_version: Option<FileVersion>,
}

impl Document {
    /// A ledger or memory record already has a stable identity and display title.
    pub fn from_record(key: &str, title: &str, text: String, at: u64) -> Self {
        let hash = record_revision(title, &text);
        let size = text.len() as u64;
        let mut document = Self::from_text(key, text, at, size, hash);
        document.title = title.to_owned();
        document.folded_title = fold(title);
        document
    }

    pub fn from_text(rel_path: &str, text: String, modified_ms: u64, size: u64, hash: String) -> Self {
        let stem = Path::new(rel_path).file_stem().and_then(|s| s.to_str()).unwrap_or(rel_path);
        let title = chunk::title(&text, stem);
        let passages = chunk::passages(&text);
        let folded_passages = passages.iter().map(|p| p.headings.iter().map(|h| fold(h)).chain([fold(p.text(&text))]).collect()).collect();
        Document {
            id: source_id(rel_path),
            rel_path: rel_path.to_string(),
            folded_title: fold(&title),
            folded_text: fold(&text),
            title,
            modified_ms,
            size,
            hash,
            text,
            passages,
            folded_passages,
            metadata_version: None,
        }
    }
}

pub fn record_revision(title: &str, text: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(title.len().to_le_bytes());
    hash.update(title);
    hash.update(text);
    hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub rel_path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct Corpus {
    /// Canonical folder path for the single-folder pilot.
    pub root: PathBuf,
    /// Opaque path prefixes for the multi-folder corpus. A document's key uses
    /// this prefix, so identical relative names in different roots cannot collide.
    pub(crate) roots: BTreeMap<String, PathBuf>,
    pub(crate) excluded: Vec<PathBuf>,
    pub(crate) automatic: bool,
    /// Documents in path order.
    pub docs: Vec<Arc<Document>>,
    pub skipped: Vec<Skipped>,
    /// Folders that could not be enumerated, separate from known skipped files.
    pub unavailable: Vec<Skipped>,
    /// Symbolic links met and not followed.
    pub links_skipped: usize,
}

pub struct SearchFolder {
    pub path: PathBuf,
    pub files: usize,
    pub direct: bool,
    pub unavailable: Option<&'static str>,
}

impl SearchFolder {
    pub fn id(&self) -> String {
        let path = self.path.to_string_lossy();
        if self.direct { source_id(&format!("direct:{path}")) } else { source_id(&path) }
    }

    pub fn scope(&self) -> FolderScope {
        if self.direct {
            FolderScope::with_direct(Vec::new(), vec![self.path.clone()])
        } else {
            FolderScope::normalized(vec![self.path.clone()])
        }
    }
}

impl Corpus {
    pub fn by_id(&self) -> BTreeMap<&str, usize> {
        self.docs.iter().enumerate().map(|(i, d)| (d.id.as_str(), i)).collect()
    }
    pub fn passage_count(&self) -> usize {
        self.docs.iter().map(|d| d.passages.len()).sum()
    }

    pub(crate) fn position(&self, path: &str) -> Option<usize> {
        self.docs.binary_search_by(|d| d.rel_path.as_str().cmp(path)).ok()
    }

    pub fn origin<'a>(&'a self, key: &'a str) -> Option<(&'a Path, &'a str)> {
        if self.roots.is_empty() {
            return self.root.is_absolute().then_some((&self.root, key));
        }
        let (prefix, relative) = key.split_once('/')?;
        self.roots.get(prefix).map(|root| (root.as_path(), relative))
    }

    pub fn resolve_key(&self, key: &str) -> Result<PathBuf, String> {
        let (root, relative) = self.origin(key).ok_or("That document is no longer in a search folder.")?;
        if self.excluded.iter().any(|prefix| root.starts_with(prefix)
            || prefix.strip_prefix(root).is_ok_and(|tail| Path::new(relative).starts_with(tail))) {
            return Err("That document is in a folder excluded from search.".into());
        }
        let path = resolve_inside(root, relative)?;
        if self.automatic && path.parent().is_some_and(|parent|
            parent.ancestors().take_while(|dir| dir.starts_with(root)).any(has_git_marker)
        ) {
            return Err("Choose this project folder explicitly to search it.".into());
        }
        Ok(path)
    }

    pub(crate) fn in_scope(&self, key: &str, scope: &FolderScope) -> bool {
        self.origin(key).is_some_and(|(root, relative)| {
            scope.roots().iter().any(|allowed|
                root.starts_with(allowed) || allowed.strip_prefix(root).is_ok_and(|tail| Path::new(relative).starts_with(tail)))
                || scope.direct.iter().any(|dir| dir.strip_prefix(root).is_ok_and(|tail| Path::new(relative).parent() == Some(tail)))
        })
    }

    /// Broad discovery presents disjoint child folders. Files directly in a
    /// root have their own choice so that unchecking a child actually excludes it.
    pub(crate) fn folder_options(&self) -> Vec<SearchFolder> {
        let mut counts = BTreeMap::new();
        if !self.automatic {
            let roots = self.roots.values().map(PathBuf::as_path)
                .chain(self.root.is_absolute().then_some(self.root.as_path()));
            counts.extend(roots.map(|root| ((root.to_owned(), false), 0)));
        }
        for doc in &self.docs {
            let Some((root, relative)) = self.origin(&doc.rel_path) else { continue };
            let key = match (self.automatic, relative.split_once('/')) {
                (true, Some((child, _))) => (root.join(child), false),
                (true, None) => (root.to_owned(), true),
                (false, _) => (root.to_owned(), false),
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        let mut options: Vec<_> = counts.into_iter().map(|((path, direct), files)| SearchFolder {
            path, files, direct, unavailable: None,
        }).collect();
        options.extend(self.unavailable.iter().filter_map(|skipped| {
            let path = PathBuf::from(&skipped.rel_path);
            let path = if path.is_absolute() { path } else if self.root.is_absolute() { self.root.join(path) } else { return None };
            let on_volume = path.strip_prefix("/").is_ok_and(|relative| relative.starts_with("Volumes"));
            let unavailable = if on_volume && matches!(path.try_exists(), Ok(false)) { "Offline" } else { "No access" };
            Some(SearchFolder { path, files: 0, direct: false, unavailable: Some(unavailable) })
        }));
        options
    }

    pub(crate) fn display_key(&self, key: &str) -> Option<String> {
        if self.roots.is_empty() { return None; }
        let (root, relative) = self.origin(key)?;
        Some(format!("{}/{relative}", root.file_name()?.to_string_lossy()))
    }

    pub(crate) fn catalog_key(&self, configured: Option<&Path>, path: &Path, source: &MetadataSource) -> Option<String> {
        if self.roots.is_empty() {
            return catalog_relative(&self.root, configured?, path, source);
        }
        self.roots.iter().find_map(|(prefix, root)| {
            let relative = catalog_relative(root, root, path, source)?;
            (!self.excluded.iter().any(|excluded| path.starts_with(excluded))).then(|| format!("{prefix}/{relative}"))
        })
    }

    pub(crate) fn key_media_document(&self, key: &str, doc: &mut Arc<Document>) {
        if !self.roots.is_empty() {
            if let Some((root, relative)) = self.origin(key) {
                let doc = Arc::make_mut(doc);
                doc.id = source_id(&root.join(relative).to_string_lossy());
                key.clone_into(&mut doc.rel_path);
            }
        }
    }
}

fn supported(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn supports_metadata(source: &MetadataSource, path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|ext| source.extensions.iter().any(|s| s.eq_ignore_ascii_case(ext)))
}

/// Stable reasons for a locked folder. OS access and user selection are checked separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryExclusion {
    System,
    AppBundle,
    Hidden,
    DependencyOrGenerated,
}

/// Accepts an absolute path, inspecting its ancestry as well as the final component.
/// Checking the ancestry makes an explicitly selected child obey the same rules as a
/// child encountered when walking a broader folder. Check the canonical path as well
/// when opening a selected root so an alias cannot grant an excluded destination.
pub fn directory_exclusion(path: &Path) -> Option<DirectoryExclusion> {
    let components = path.components().filter_map(|part| match part {
        Component::Normal(name) => Some(name),
        _ => None,
    });
    if cfg!(target_os = "macos") {
        let mut ancestry = components.clone();
        if let Some(first) = ancestry.next() {
            let system = ["System", "Applications", "Library"].iter().any(|name| first.eq_ignore_ascii_case(name));
            let user_library = first.eq_ignore_ascii_case("Users") && ancestry.nth(1).is_some_and(|name| name.eq_ignore_ascii_case("Library"));
            if system || user_library {
                return Some(DirectoryExclusion::System);
            }
        }
    }
    for name in components {
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            return Some(DirectoryExclusion::Hidden);
        }
        if name.rsplit_once('.').is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("app")) {
            return Some(DirectoryExclusion::AppBundle);
        }
        if SKIPPED_DIRS.iter().any(|part| name.eq_ignore_ascii_case(part)) {
            return Some(DirectoryExclusion::DependencyOrGenerated);
        }
    }
    None
}

/// Validate the configured folder: absolute, existing, a folder. The error names the path and
/// what is wrong with it, for the status line (PRD: *"An invalid or missing configured folder
/// gets a named error, never a guessed default."*).
pub fn validate_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err(format!("The search folder must be an absolute path: {}", root.display()));
    }
    if root.parent().is_none() || directory_exclusion(root).is_some() {
        return Err(format!("This folder is excluded from search: {}", root.display()));
    }
    let canonical = root.canonicalize().map_err(|_| format!("Search folder not found: {}", root.display()))?;
    if !canonical.is_dir() {
        return Err(format!("The search folder is not a folder: {}", root.display()));
    }
    if canonical.parent().is_none() || directory_exclusion(&canonical).is_some() {
        return Err(format!("This folder is excluded from search: {}", root.display()));
    }
    Ok(canonical)
}

/// The currently reachable part of a saved folder choice. Roots are canonical
/// and disjoint, so a parent and child selected together do not index files twice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderScope {
    roots: Vec<PathBuf>,
    direct: Vec<PathBuf>,
}

impl FolderScope {
    /// An unavailable folder stays in the caller's saved choices. Returning its
    /// error alongside the other roots lets the caller report partial coverage.
    pub fn resolve(paths: &[PathBuf]) -> (Self, Vec<(PathBuf, String)>) {
        let mut roots = Vec::new();
        let mut unavailable = Vec::new();
        for path in paths {
            match validate_root(path) {
                Ok(root) if root == *path => roots.push(root),
                Ok(_) => unavailable.push((path.clone(), "The selected path reaches a different folder through a link.".into())),
                Err(reason) => unavailable.push((path.clone(), reason)),
            }
        }
        (Self::normalized(roots), unavailable)
    }

    pub fn normalized(mut roots: Vec<PathBuf>) -> Self {
        roots.sort_unstable();
        let mut distinct: Vec<PathBuf> = Vec::new();
        for root in roots {
            if distinct.last().is_none_or(|parent| !root.starts_with(parent)) {
                distinct.push(root);
            }
        }
        Self { roots: distinct, direct: Vec::new() }
    }

    pub(crate) fn with_direct(roots: Vec<PathBuf>, mut direct: Vec<PathBuf>) -> Self {
        let mut scope = Self::normalized(roots);
        direct.retain(|dir| !scope.contains_path(dir));
        direct.sort_unstable();
        direct.dedup();
        scope.direct = direct;
        scope
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty() && self.direct.is_empty()
    }

    pub(crate) fn contains_path(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }

    /// Apply a company or request limit without ever granting a new subtree. An
    /// explicit empty limit is an empty grant; inheritance is handled by the caller.
    pub fn intersect(&self, limit: &Self) -> Self {
        let mut roots = Vec::new();
        for base in &self.roots {
            for bound in &limit.roots {
                if base.starts_with(bound) {
                    roots.push(base.clone());
                } else if bound.starts_with(base) {
                    roots.push(bound.clone());
                }
            }
        }
        let direct = self.direct.iter().filter(|dir| limit.contains_path(dir) || limit.direct.contains(dir))
            .chain(limit.direct.iter().filter(|dir| self.contains_path(dir))).cloned().collect();
        Self::with_direct(roots, direct)
    }
}

struct DirectoryEntries {
    files: Vec<(String, PathBuf)>,
    links_skipped: usize,
    unavailable: Vec<Skipped>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WalkPolicy {
    Pilot,
    Chosen,
    All,
}

fn has_git_marker(dir: &Path) -> bool {
    dir.join(".git").symlink_metadata().is_ok()
}

/// Every supported file under `root`, in path order, as `(relative path, absolute path)`.
fn list(root: &Path, metadata: Option<&MetadataSource>, excluded: &[PathBuf], policy: WalkPolicy, stopped: &impl Fn() -> bool) -> Result<Option<DirectoryEntries>, String> {
    let mut files = Vec::new();
    let mut links = 0usize;
    let mut unavailable = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if stopped() {
            return Ok(None);
        }
        // Broad discovery is not permission to search a project checkout. A
        // linked worktree has a .git file instead of a directory.
        if policy == WalkPolicy::All && has_git_marker(&dir) {
            if dir == root {
                return Err("Choose this project folder explicitly to search it.".into());
            }
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if policy != WalkPolicy::Pilot && dir != root => {
                unavailable.push(Skipped { rel_path: relative(root, &dir).unwrap_or_default(), reason: error.to_string() });
                continue;
            }
            Err(error) => return Err(format!("Cannot read {}: {error}", dir.display())),
        };
        for entry in entries.flatten() {
            if stopped() {
                return Ok(None);
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_symlink() {
                links += 1;
            } else if kind.is_dir() {
                if directory_exclusion(&path).is_none() && !excluded.iter().any(|prefix| path.starts_with(prefix)) {
                    stack.push(path);
                }
            } else if kind.is_file() && !name.starts_with('.') && (supported(&path) || metadata.is_some_and(|m| supports_metadata(m, &path))) {
                if let Some(rel) = relative(root, &path) {
                    files.push((rel, path));
                }
            }
        }
    }
    files.sort();
    Ok(Some(DirectoryEntries { files, links_skipped: links, unavailable }))
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Some(parts.join("/"))
}

pub fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Read one file as a document, or say why it cannot be searched.
pub fn read_document(rel_path: &str, path: &Path) -> Result<Document, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("cannot read: {e}"))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("not a regular file".into());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("larger than {} MB", MAX_FILE_BYTES / (1024 * 1024)));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read: {e}"))?;
    let hash = content_hash(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_string())?;
    Ok(Document::from_text(rel_path, text, modified_ms(&meta), meta.len(), hash))
}

/// Read the whole folder. Text search is usable as soon as this returns; it needs no model and
/// no index (PRD: *"Text search must work independently of Edge availability and model
/// startup."*).
pub fn load(root: &Path) -> Result<Corpus, String> {
    Ok(load_until_stopped(root, None, || false)?.expect("no cancellation requested").corpus)
}

pub(crate) struct PendingCorpus {
    pub corpus: Corpus,
    pub media: Vec<String>,
}

/// Cancellation discards the partial corpus. Check before opening each directory and file.
pub(crate) fn load_until_stopped(root: &Path, metadata: Option<&MetadataSource>, stopped: impl Fn() -> bool) -> Result<Option<PendingCorpus>, String> {
    if stopped() {
        return Ok(None);
    }
    let root = validate_root(root)?;
    load_root(root, metadata, &[], WalkPolicy::Pilot, &stopped)
}

fn load_root(root: PathBuf, metadata: Option<&MetadataSource>, excluded: &[PathBuf], policy: WalkPolicy, stopped: &impl Fn() -> bool) -> Result<Option<PendingCorpus>, String> {
    let Some(DirectoryEntries { files, links_skipped, unavailable }) = list(&root, metadata, excluded, policy, stopped)? else { return Ok(None) };
    let mut corpus = Corpus { root, docs: Vec::with_capacity(files.len()), links_skipped, unavailable, ..Corpus::default() };
    let mut media = Vec::new();
    for (rel, path) in files {
        if stopped() {
            return Ok(None);
        }
        if !supported(&path) {
            media.push(rel);
            continue;
        }
        match resolve_inside(&corpus.root, &rel).and_then(|path| read_document(&rel, &path)) {
            Ok(doc) => corpus.docs.push(Arc::new(doc)),
            Err(reason) => corpus.skipped.push(Skipped { rel_path: rel, reason }),
        }
    }
    Ok((!stopped()).then_some(PendingCorpus { corpus, media }))
}

/// Load a disjoint set of selected folders as one searchable corpus. Each file
/// keeps its source identity even if unrelated folders are added to the choice.
pub fn load_scope(configured: &[PathBuf], excluded: &[PathBuf]) -> Corpus {
    load_scope_until_stopped(configured, excluded, false, None, || false)
        .expect("no cancellation requested").corpus
}

/// Discover mounted volumes on the search worker. The desktop does not touch
/// removable storage while opening a conversation or starting the app.
pub(crate) fn automatic_roots(home: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = home.to_vec();
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::MetadataExt;
        let mount = Path::new("/").join("Volumes");
        if let (Ok(parent), Ok(entries)) = (mount.metadata(), std::fs::read_dir(&mount)) {
            roots.extend(entries.flatten().filter_map(|entry| {
                let path = entry.path();
                let meta = path.symlink_metadata().ok()?;
                (meta.is_dir() && meta.dev() != parent.dev()).then_some(path)
            }));
        }
    }
    roots.sort_unstable();
    roots.dedup();
    roots
}

pub(crate) fn load_scope_until_stopped(configured: &[PathBuf], excluded: &[PathBuf], automatic: bool, metadata: Option<&MetadataSource>, stopped: impl Fn() -> bool) -> Option<PendingCorpus> {
    let (scope, unresolved) = FolderScope::resolve(configured);
    let mut corpus = Corpus {
        automatic,
        unavailable: unresolved.into_iter().map(|(path, reason)| Skipped {
            rel_path: path.to_string_lossy().into_owned(), reason,
        }).collect(),
        ..Corpus::default()
    };
    corpus.excluded = excluded.iter().map(|path| path.canonicalize().unwrap_or_else(|_| path.clone())).collect();
    let mut media = Vec::new();
    for root in scope.roots() {
        if stopped() { return None; }
        if corpus.excluded.iter().any(|path| root.starts_with(path)) {
            corpus.unavailable.push(Skipped { rel_path: root.to_string_lossy().into_owned(), reason: "This folder is excluded from search.".into() });
            continue;
        }
        let policy = if automatic { WalkPolicy::All } else { WalkPolicy::Chosen };
        let found = match load_root(root.clone(), metadata, &corpus.excluded, policy, &stopped) {
            Ok(Some(found)) => found,
            Ok(None) => return None,
            Err(reason) => {
                corpus.unavailable.push(Skipped { rel_path: root.to_string_lossy().into_owned(), reason });
                continue;
            }
        };
        let prefix = source_id(&root.to_string_lossy());
        let key = |relative: &str| format!("{prefix}/{relative}");
        corpus.links_skipped += found.corpus.links_skipped;
        for mut doc in found.corpus.docs {
            let doc_mut = Arc::make_mut(&mut doc);
            doc_mut.id = source_id(&root.join(&doc_mut.rel_path).to_string_lossy());
            doc_mut.rel_path = key(&doc_mut.rel_path);
            corpus.docs.push(doc);
        }
        for mut skipped in found.corpus.skipped {
            skipped.rel_path = key(&skipped.rel_path);
            corpus.skipped.push(skipped);
        }
        for mut dir in found.corpus.unavailable {
            dir.rel_path = root.join(&dir.rel_path).to_string_lossy().into_owned();
            corpus.unavailable.push(dir);
        }
        media.extend(found.media.iter().map(|relative| key(relative)));
        corpus.roots.insert(prefix, root.clone());
    }
    corpus.docs.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    corpus.skipped.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    media.sort_unstable();
    (!stopped()).then_some(PendingCorpus { corpus, media })
}

pub(crate) fn catalog_relative(root: &Path, configured: &Path, path: &Path, source: &MetadataSource) -> Option<String> {
    // Only the selected root may be an alias. Descendants must still pass resolve_inside.
    let tail = path.strip_prefix(root).or_else(|_| path.strip_prefix(configured)).ok()?;
    let names: Vec<_> = tail.components().map(|part| match part {
        Component::Normal(name) => name.to_str(),
        _ => None,
    }).collect::<Option<_>>()?;
    let (leaf, _) = names.split_last()?;
    if leaf.starts_with('.') || directory_exclusion(root.join(tail).parent()?).is_some() || supported(path) || !supports_metadata(source, path) {
        return None;
    }
    Some(names.join("/"))
}

pub(crate) fn read_metadata_document(root: &Path, rel: &str, source: &MetadataSource, cache: Option<&Path>) -> Result<Option<Arc<Document>>, String> {
    let path = resolve_inside(root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let version = FileVersion::of(&meta);
    let Some(text) = (source.read)(&path, &meta, cache)?.filter(|s| !s.trim().is_empty()) else { return Ok(None) };
    if resolve_inside(root, rel)? != path || std::fs::metadata(&path).ok().map(|m| FileVersion::of(&m)) != Some(version.clone()) {
        return Err("The file changed while its metadata was being read.".into());
    }
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or(rel);
    if text.len() > 256 * 1024 {
        return Err("The file metadata exceeds the search limit.".into());
    }
    let text = format!("# {name}\n\n## File metadata\n\n{text}");
    let hash = content_hash(text.as_bytes());
    let mut doc = Document::from_text(rel, text, modified_ms(&meta), meta.len(), hash);
    doc.metadata_version = Some(version);
    Ok(Some(Arc::new(doc)))
}

/// Resolve a document's file for opening or revalidation: the path is rebuilt from the
/// canonical folder and the stored relative path, canonicalized again, and refused unless it is
/// still a regular file inside the folder. Nothing the renderer sends is a path.
pub fn resolve_inside(root: &Path, rel_path: &str) -> Result<PathBuf, String> {
    if Path::new(rel_path).components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err("That document is outside the search folder.".into());
    }
    let candidate = root.join(rel_path);
    if candidate.parent().is_some_and(|parent| directory_exclusion(parent).is_some()) {
        return Err("That document is in a folder excluded from search.".into());
    }
    let meta = std::fs::symlink_metadata(&candidate).map_err(|_| "That document is no longer in the search folder.".to_string())?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("That document is no longer a file in the search folder.".into());
    }
    let canonical = candidate.canonicalize().map_err(|_| "That document is no longer in the search folder.".to_string())?;
    if !canonical.starts_with(root) {
        return Err("That document is outside the search folder.".into());
    }
    if canonical != candidate {
        return Err("That document is reached through a symbolic link.".into());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("richos-search-corpus-{name}-{}", std::process::id()));
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("fixture cleanup: {error}");
            }
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn only_markdown_inside_the_folder_and_nothing_through_a_link() {
        let root = scratch("walk");
        let outside = scratch("walk-outside");
        std::fs::write(outside.join("secret.md"), "# Secret\nnot in the folder").unwrap();
        std::fs::create_dir_all(root.join("chats/2024")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("chats/2024/a.md"), "# A\nalpha").unwrap();
        std::fs::write(root.join("b.MARKDOWN"), "beta").unwrap();
        std::fs::write(root.join("notes.txt"), "not markdown").unwrap();
        std::fs::write(root.join(".git/c.md"), "git internals").unwrap();
        std::fs::write(root.join("node_modules/x/d.md"), "dependency").unwrap();
        std::fs::write(root.join("bad.md"), [0xff, 0xfe, 0x00]).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.join("secret.md"), root.join("link.md")).unwrap();
            std::os::unix::fs::symlink(&outside, root.join("linked-folder")).unwrap();
        }
        let corpus = load(&root).unwrap();
        let paths: Vec<_> = corpus.docs.iter().map(|d| d.rel_path.as_str()).collect();
        assert_eq!(paths, vec!["b.MARKDOWN", "chats/2024/a.md"]);
        assert_eq!(corpus.skipped, vec![Skipped { rel_path: "bad.md".into(), reason: "not UTF-8 text".into() }]);
        #[cfg(unix)]
        assert_eq!(corpus.links_skipped, 2);
        assert_eq!(corpus.docs[1].title, "A");
        assert_eq!(corpus.docs[0].title, "b");
        assert!(resolve_inside(&corpus.root, "chats/2024/a.md").is_ok());
        assert!(resolve_inside(&corpus.root, "../walk-outside/secret.md").is_err());
        #[cfg(unix)]
        assert!(resolve_inside(&corpus.root, "link.md").is_err());
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn a_missing_or_relative_folder_is_a_named_error() {
        assert!(validate_root(Path::new("relative/path")).unwrap_err().contains("absolute"));
        let missing = std::env::temp_dir().join("richos-search-definitely-missing-folder");
        assert!(validate_root(&missing).unwrap_err().contains("not found"));
    }

    #[test]
    fn automatic_folder_choices_keep_root_files_and_children_separate() {
        let root = scratch("automatic-sources");
        let child = root.join("Contracts");
        let missing = root.join("Disconnected");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("direct.md"), "Notes at the root").unwrap();
        std::fs::write(child.join("agreement.md"), "Notes in Contracts").unwrap();
        let corpus = load_scope_until_stopped(&[root.clone(), missing.clone()], &[], true, None, || false).unwrap().corpus;
        let options = corpus.folder_options();
        let direct = options.iter().find(|folder| folder.direct).unwrap();
        let contracts = options.iter().find(|folder| folder.path == child).unwrap();
        assert_eq!(direct.files, 1);
        assert_eq!(contracts.files, 1);
        assert_ne!(direct.id(), contracts.id());
        assert!(direct.scope().intersect(&contracts.scope()).is_empty());
        assert_eq!(corpus.docs.iter().filter(|doc| corpus.in_scope(&doc.rel_path, &direct.scope())).count(), 1);
        assert_eq!(corpus.docs.iter().filter(|doc| corpus.in_scope(&doc.rel_path, &contracts.scope())).count(), 1);
        assert!(options.iter().any(|folder| folder.path == missing && folder.unavailable.is_some()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn excluded_folders_cannot_be_granted_directly_or_through_an_alias() {
        let root = scratch("excluded");
        let hidden = root.join(".git/objects");
        let generated = root.join("project/TARGET/logs");
        let bundle = root.join("Tool.app/Contents");
        let docs = root.join("project/notes");
        for folder in [&hidden, &generated, &bundle, &docs] {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(folder.join("notes.md"), "a witness").unwrap();
        }
        for folder in [&hidden, &generated, &bundle] {
            assert!(validate_root(folder).unwrap_err().contains("excluded"));
            let path = folder.join("notes.md");
            assert!(resolve_inside(&root, path.strip_prefix(&root).unwrap().to_str().unwrap()).is_err());
        }
        #[cfg(unix)]
        {
            let alias = root.join("elsewhere");
            std::os::unix::fs::symlink(&generated, &alias).unwrap();
            assert!(validate_root(&alias).unwrap_err().contains("excluded"));
        }
        let corpus = load(&root).unwrap();
        assert_eq!(corpus.docs.len(), 1);
        assert_eq!(corpus.docs[0].rel_path, "project/notes/notes.md");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_folder_limit_can_only_narrow_the_reachable_global_selection() {
        let root = scratch("scope");
        let parent = root.join("archive");
        let child = parent.join("chats");
        let other = root.join("archive-other");
        let disconnected = root.join("disconnected");
        for folder in [&child, &other] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let (global, unavailable) = FolderScope::resolve(&[child.clone(), parent.clone(), disconnected.clone()]);
        assert_eq!(global.roots(), &[parent]);
        assert_eq!(unavailable.len(), 1);
        assert_eq!(unavailable[0].0, disconnected);

        let (company, _) = FolderScope::resolve(&[child.clone(), other.clone()]);
        let (broader_request, _) = FolderScope::resolve(std::slice::from_ref(&root));
        let allowed = global.intersect(&company);
        assert_eq!(allowed.roots(), &[child]);
        assert_eq!(allowed.intersect(&broader_request), allowed);
        assert!(allowed.intersect(&FolderScope::default()).roots().is_empty());
        let (unrelated, _) = FolderScope::resolve(&[other]);
        assert!(allowed.intersect(&unrelated).roots().is_empty());
        #[cfg(unix)]
        {
            let alias = root.join("archive-link");
            std::os::unix::fs::symlink(allowed.roots()[0].as_path(), &alias).unwrap();
            let (scope, errors) = FolderScope::resolve(&[alias]);
            assert!(scope.roots().is_empty());
            assert_eq!(errors.len(), 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scoped_corpus_keeps_equal_relative_paths_distinct_and_reports_missing_roots() {
        let root = scratch("scope-corpus");
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        let cache = alpha.join("index");
        let absent = root.join("unplugged");
        for folder in [&alpha, &beta, &cache] {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(folder.join("same.md"), folder.file_name().unwrap().to_string_lossy().as_bytes()).unwrap();
        }
        let scoped = load_scope(&[alpha.clone(), beta.clone(), absent.clone()], std::slice::from_ref(&cache));
        assert_eq!(scoped.docs.len(), 2);
        assert_ne!(scoped.docs[0].id, scoped.docs[1].id);
        assert_eq!(scoped.unavailable.len(), 1);
        assert_eq!(Path::new(&scoped.unavailable[0].rel_path), absent);
        for doc in &scoped.docs {
            let expected = root.join(doc.text.trim()).join("same.md");
            assert_eq!(scoped.resolve_key(&doc.rel_path).unwrap(), expected);
        }
        let alpha_key = &scoped.docs.iter().find(|d| d.text == "alpha").unwrap().rel_path;
        let (prefix, _) = alpha_key.split_once('/').unwrap();
        assert!(scoped.resolve_key(&format!("{prefix}/index/same.md")).is_err());
        let combined = load_scope(&[root.clone(), alpha], &[cache]);
        assert_eq!(combined.docs.len(), 2);
        assert!(combined.docs.iter().all(|doc| scoped.docs.iter().any(|other| other.id == doc.id)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_folder_discovery_needs_an_explicit_choice_to_include_a_repository() {
        let home = scratch("automatic");
        let project = home.join("workspace");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(home.join("notes.md"), "ordinary notes").unwrap();
        std::fs::write(project.join("details.md"), "project notes").unwrap();
        std::fs::write(project.join(".git"), "gitdir: fixture").unwrap();
        let automatic = load_scope_until_stopped(std::slice::from_ref(&home), &[], true, None, || false).unwrap().corpus;
        assert_eq!(automatic.docs.len(), 1);
        let (prefix, _) = automatic.docs[0].rel_path.split_once('/').unwrap();
        assert!(automatic.resolve_key(&format!("{prefix}/workspace/details.md")).is_err());

        let chosen = load_scope(&[project], &[]);
        assert_eq!(chosen.docs.len(), 1);
        assert!(chosen.resolve_key(&chosen.docs[0].rel_path).is_ok());
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn source_ids_are_stable_and_carry_no_path() {
        assert_eq!(source_id("chats/a.md"), source_id("chats/a.md"));
        assert_ne!(source_id("chats/a.md"), source_id("chats/b.md"));
        assert!(!source_id("chats/a.md").contains('/'));
    }
}
