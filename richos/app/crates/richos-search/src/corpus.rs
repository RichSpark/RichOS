//! The one folder the CEO included, read into memory as documents.
//!
//! Search PRD v3.1, "Indexing and source access": *"Index only the explicitly configured
//! folder's supported files. Repository roots are not an archive grant. Exclude Git internals,
//! dependencies and generated output; preserve linked-path restrictions."*
//!
//! - **Supported files** in slice one are Markdown: `.md` and `.markdown`, any case.
//! - **Skipped folders**: any whose name starts with a dot (`.git`, `.obsidian`, …) and the
//!   dependency and build-output names in [`SKIPPED_DIRS`].
//! - **Links are not followed.** A symbolic link, to a file or a folder, is skipped and
//!   counted: a link is the one way a folder can reach outside itself, and the PRD's
//!   *"preserve linked-path restrictions"* is kept by never crossing one.
//! - **Files that cannot be searched are reported, never silently dropped**: too large
//!   ([`MAX_FILE_BYTES`]) or not UTF-8 text each appear in [`Corpus::skipped`] with the reason.
use crate::chunk::{self, Passage};
use crate::text::fold;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const SKIPPED_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "__pycache__", "vendor"];
/// A Markdown file larger than this is reported as skipped. 16 MB is far above any single
/// conversation the archive's lower bound implies (30+ MB over 1,500+ files, about 20 KB each).
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

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
}

impl Document {
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub rel_path: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct Corpus {
    /// Canonical folder path.
    pub root: PathBuf,
    /// Documents in path order.
    pub docs: Vec<Document>,
    pub skipped: Vec<Skipped>,
    /// Symbolic links met and not followed.
    pub links_skipped: usize,
}

impl Corpus {
    pub fn by_id(&self) -> BTreeMap<&str, usize> {
        self.docs.iter().enumerate().map(|(i, d)| (d.id.as_str(), i)).collect()
    }
    pub fn passage_count(&self) -> usize {
        self.docs.iter().map(|d| d.passages.len()).sum()
    }
}

fn supported(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.') || SKIPPED_DIRS.contains(&name)
}

/// Validate the configured folder: absolute, existing, a folder. The error names the path and
/// what is wrong with it, for the status line (PRD: *"An invalid or missing configured folder
/// gets a named error, never a guessed default."*).
pub fn validate_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err(format!("The search folder must be an absolute path: {}", root.display()));
    }
    let canonical = root.canonicalize().map_err(|_| format!("Search folder not found: {}", root.display()))?;
    if !canonical.is_dir() {
        return Err(format!("The search folder is not a folder: {}", root.display()));
    }
    Ok(canonical)
}

/// Every supported file under `root`, in path order, as `(relative path, absolute path)`.
pub fn list(root: &Path) -> Result<(Vec<(String, PathBuf)>, usize), String> {
    let mut files = Vec::new();
    let mut links = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_symlink() {
                links += 1;
            } else if kind.is_dir() {
                if !skipped_dir(&name) {
                    stack.push(path);
                }
            } else if kind.is_file() && supported(&path) && !name.starts_with('.') {
                if let Some(rel) = relative(root, &path) {
                    files.push((rel, path));
                }
            }
        }
    }
    files.sort();
    Ok((files, links))
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
    let root = validate_root(root)?;
    let (files, links_skipped) = list(&root)?;
    let mut corpus = Corpus { root, docs: Vec::with_capacity(files.len()), skipped: Vec::new(), links_skipped };
    for (rel, path) in files {
        match read_document(&rel, &path) {
            Ok(doc) => corpus.docs.push(doc),
            Err(reason) => corpus.skipped.push(Skipped { rel_path: rel, reason }),
        }
    }
    Ok(corpus)
}

/// Resolve a document's file for opening or revalidation: the path is rebuilt from the
/// canonical folder and the stored relative path, canonicalized again, and refused unless it is
/// still a regular file inside the folder. Nothing the renderer sends is a path.
pub fn resolve_inside(root: &Path, rel_path: &str) -> Result<PathBuf, String> {
    let candidate = root.join(rel_path);
    let meta = std::fs::symlink_metadata(&candidate).map_err(|_| "That document is no longer in the search folder.".to_string())?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("That document is no longer a file in the search folder.".into());
    }
    let canonical = candidate.canonicalize().map_err(|_| "That document is no longer in the search folder.".to_string())?;
    if !canonical.starts_with(root) {
        return Err("That document is outside the search folder.".into());
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
    fn source_ids_are_stable_and_carry_no_path() {
        assert_eq!(source_id("chats/a.md"), source_id("chats/a.md"));
        assert_ne!(source_id("chats/a.md"), source_id("chats/b.md"));
        assert!(!source_id("chats/a.md").contains('/'));
    }
}
