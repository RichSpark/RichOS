//! THE HOME SCREEN'S PICTURE, READ FROM EVERY CONNECTED COMPANY'S REPOSITORIES.
//!
//! The CEO, 2026-11-13: round 73 (`richos-hq/design/mockups/rounds/round-73`) is the home screen
//! standard, and its picture is round 72's: the user's own data for every connected company, drawn
//! the way round 71 built it (`round-71/build/build-data.py`), with smooth curves. This file is
//! that builder, ported.
//!
//! # Why not the loro compile (`home_field.rs`)
//!
//! Measured on the CEO's Mac on 2026-11-13: connecting a company only records its folders in
//! `entities.json`, and the loro corpus partitions for five of his six companies held no file; loro
//! reads `wiki/`, `loro/memory`, records and entities (899 records) against 3,583 tracked documents
//! in one company's two repositories; and the compile answers ranked queries behind a relevance
//! floor, so the picture held 186 objects. The compile is a question-answerer, not an enumeration.
//! So when a company has a repository, the picture reads the repository itself. Loro's compile,
//! which is Rich's memory, is not touched.
//!
//! # How a repository becomes the picture (round 71's reading, unchanged)
//!
//!   * An OBJECT is a tracked Markdown document, a numbered ruling in a decision register (a file
//!     whose name holds "decisions", one object per numbered `## N.` or `## §N` heading, at least
//!     three of them), or a day the repository has commits.
//!   * A SOURCE on the river is a tracked document or source-code file.
//!   * A THREAD joins a document to its folder's anchor, a folder's anchor to its parent's, a
//!     ruling to its register, a day to the documents first written that day and to the day
//!     before; and a document to another it names (a Markdown link, a repository path, a `§N`
//!     citation of a ruling). A thread whose two ends are in different companies is `cross`.
//!
//! # NO CORPUS CONTENT CROSSES THIS BOUNDARY
//!
//! `home_field.rs`'s rule, kept: no title, file name, folder name or path is in the output. Node,
//! cluster and source ids are ordinals or one-way hashes, cluster labels are "Group N", and the
//! only names are the companies' own display names, which are on the screen already.
//!
//! # A folder that is not there
//!
//! Skipped, never fatal, and named by company id in `meta.unreadFolders` (a saved folder can point
//! at a repository that moved). The rest of the picture is still drawn; connecting the moved folder
//! in the app's Connected repositories sheet puts it back.

use crate::entity::{EntityRegistry, EntityStatus};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Round 71's `CODE_EXT`: what counts as a source-code file on the river.
const CODE_EXT: &[&str] = &[
    "rs", "ts", "js", "mjs", "cjs", "svelte", "swift", "kt", "kts", "py", "sh", "sql", "html", "css", "scss", "toml", "yaml",
    "yml", "go", "rb", "java", "tsx", "jsx",
];
/// Round 71's `SKIP_PARTS`: vendored and generated trees are not his.
const SKIP_PARTS: &[&str] = &["node_modules/", "third-party/", "third_party/", "vendor/", "/dist/", "ui-dist/", ".min."];
/// A document is read for its references only this far (round 71: `read(200000)`).
const READ_CHARS: usize = 200_000;
/// Where the biggest company's arc is centered, as a fraction of the picture's lobe, counter-clockwise
/// from the lower right: where round 71 put it, the crown of the approved composition (the middle of
/// its biggest company's arc, 0.638).
const CROWN: f64 = 0.638;

// ---------------------------------------------------------------------------
// days
// ---------------------------------------------------------------------------

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY-MM-DD` as days since 1970-01-01, or `None` for anything that is not a real date.
pub fn parse_day(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let len = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize];
    if d > len {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Today, in days since 1970-01-01 (UTC).
pub fn today() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    secs.div_euclid(86_400)
}

// ---------------------------------------------------------------------------
// what is read from a repository
// ---------------------------------------------------------------------------

/// One tracked file: its path in the repository, the first and last day a commit touched it, and
/// how many commits did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRec {
    pub path: String,
    pub first: i64,
    pub last: i64,
    pub commits: u32,
}

/// Everything the picture needs from one repository. Built by [`read_repo`] from `git`, or by hand
/// in a test.
#[derive(Debug, Clone, Default)]
pub struct RepoRead {
    /// The folder's own name, used to resolve a reference that names another repository
    /// (`other-repo/docs/x.md`). Never emitted.
    pub name: String,
    pub docs: Vec<FileRec>,
    pub code: Vec<FileRec>,
    /// Commit day -> commits that day.
    pub sessions: BTreeMap<i64, u32>,
    pub commits: u32,
    /// The text of each document, as far as [`READ_CHARS`]. Read only to find rulings and
    /// references; never emitted.
    pub texts: HashMap<String, String>,
}

/// One company and its repositories.
#[derive(Debug, Clone, Default)]
pub struct CompanyRead {
    pub id: String,
    pub label: String,
    pub repos: Vec<RepoRead>,
}

/// A company's folders, as saved: `roots` plus `connected_repositories`, each once, in that order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanyFolders {
    pub id: String,
    pub label: String,
    pub folders: Vec<PathBuf>,
}

/// Every active company and its folders, in the registry's order. An archived company is not on the
/// home screen.
pub fn company_folders(registry: &EntityRegistry) -> Vec<CompanyFolders> {
    registry
        .entities()
        .iter()
        .filter(|e| e.status == EntityStatus::Active)
        .map(|e| {
            let mut folders: Vec<PathBuf> = Vec::new();
            for p in e.roots.iter().chain(e.connected_repositories.iter()) {
                if !folders.contains(p) {
                    folders.push(p.clone());
                }
            }
            CompanyFolders { id: e.id.as_str().to_string(), label: e.display_name.clone(), folders }
        })
        .collect()
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-c").arg("core.quotePath=false").arg("-C").arg(root).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The commit a repository is at, or `None` for a folder that is not a repository (or has no commit).
pub fn repo_head(root: &Path) -> Option<String> {
    git(root, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn ext_of(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    match base.rfind('.') {
        Some(i) if i > 0 => base[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Read one repository: its tracked files, one pass over its history, and its documents' text.
/// `None` when the folder is not a repository `git` can read.
pub fn read_repo(root: &Path) -> Option<RepoRead> {
    let listed = git(root, &["ls-files"])?;
    let tracked: Vec<String> = listed.lines().filter(|f| !f.is_empty() && !SKIP_PARTS.iter().any(|s| f.contains(s))).map(str::to_string).collect();
    let tracked_set: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
    let log = git(root, &["log", "--no-renames", "--name-only", "--format=@%ad", "--date=short"]).unwrap_or_default();
    let mut stat: HashMap<String, (i64, i64, u32)> = HashMap::new(); // last, first, commits (log is newest first)
    let mut sessions: BTreeMap<i64, u32> = BTreeMap::new();
    let mut commits = 0u32;
    let mut cur: Option<i64> = None;
    for line in log.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(day) = line.strip_prefix('@') {
            cur = parse_day(day);
            if let Some(d) = cur {
                *sessions.entry(d).or_insert(0) += 1;
            }
            commits += 1;
            continue;
        }
        let Some(d) = cur else { continue };
        if tracked_set.contains(line) {
            stat.entry(line.to_string()).and_modify(|s| { s.1 = d; s.2 += 1; }).or_insert((d, d, 1));
        }
    }
    let head = git(root, &["log", "-1", "--format=%ad", "--date=short"]).and_then(|s| parse_day(s.trim())).unwrap_or_else(today);
    let mut repo = RepoRead {
        name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        sessions,
        commits,
        ..Default::default()
    };
    for f in tracked {
        let (last, first, n) = stat.get(&f).copied().unwrap_or((head, head, 1));
        let ext = ext_of(&f);
        let rec = FileRec { path: f.clone(), first, last, commits: n };
        if ext == "md" {
            if let Ok(bytes) = std::fs::read(root.join(&f)) {
                let text: String = String::from_utf8_lossy(&bytes).chars().take(READ_CHARS).collect();
                repo.texts.insert(f.clone(), text);
            }
            repo.docs.push(rec);
        } else if CODE_EXT.contains(&ext.as_str()) {
            repo.code.push(rec);
        }
    }
    Some(repo)
}

// ---------------------------------------------------------------------------
// the reading, ported: round 71's `classify` and its three text patterns
// ---------------------------------------------------------------------------

/// Round 71's `classify`: a document's kind from where it lives.
pub fn classify(path: &str) -> &'static str {
    let p = path.to_lowercase();
    let base = p.rsplit('/').next().unwrap_or(&p).to_string();
    let any = |ks: &[&str]| ks.iter().any(|k| p.contains(k));
    if any(&["decision", "signoff", "ruling", "/adr", "verdict"]) {
        return "decision";
    }
    if any(&["postmortem", "post-mortem", "lesson", "incident", "retro", "learning/", "root-cause", "rca"]) {
        return "lesson";
    }
    if any(&["todo", "open-items", "follow-up", "followup", "checklist"]) {
        return "commitment";
    }
    if p.starts_with("dossiers/") {
        let parts = p.split('/').count();
        if base == "dossier.md" {
            return if parts >= 4 { "person" } else { "organization" };
        }
        return "memory";
    }
    if any(&["inbox", "ceo-input", "transcript", "handoff", "briefs/", "conversation", "escalations/", "cold-open", "prompts/"]) {
        return "conversation";
    }
    if any(&["plans/", "prd", "roadmap", "sprint", "/spec", "strategy"]) {
        return "initiative";
    }
    if any(&["design/mockups", "rounds/", "design-system", "mockup"]) {
        return "project";
    }
    if any(&["wiki", "skills/", "principle", "doctrine", "runbook", "operations/", "development/", "architecture"]) || base == "claude.md" || base == "agents.md" {
        return "theme";
    }
    "memory"
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `\]\(([^)#\s]+\.md)`: the targets of Markdown links to `.md` files.
pub fn md_links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        let run_len = after.find(|c: char| c == ')' || c == '#' || c.is_whitespace()).unwrap_or(after.len());
        let run = &after[..run_len];
        // greedy with backtracking: the longest prefix ending in ".md" with at least one char before it
        let mut taken = 0;
        for (j, _) in run.match_indices(".md") {
            if j >= 1 {
                taken = j + 3;
            }
        }
        if taken > 0 {
            out.push(run[..taken].to_string());
            rest = &after[taken..];
        } else {
            rest = after;
        }
    }
    out
}

fn path_char(c: char) -> bool {
    is_word(c) || c == '.' || c == '-' || c == '/'
}

/// `(?<![\w/.-])((?:[\w.-]+/)+[\w.-]+\.md)`: repository paths to `.md` files written in the text.
pub fn md_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut k = 0;
    while k < chars.len() {
        if !path_char(chars[k].1) || (k > 0 && path_char(chars[k - 1].1)) {
            k += 1;
            continue;
        }
        let start = chars[k].0;
        let mut e = k;
        while e < chars.len() && path_char(chars[e].1) {
            e += 1;
        }
        let end = if e < chars.len() { chars[e].0 } else { text.len() };
        let run = &text[start..end];
        if !run.starts_with('/') {
            let mut best: Option<usize> = None;
            for (j, _) in run.match_indices(".md") {
                let cand = &run[..j + 3];
                let segs: Vec<&str> = cand.split('/').collect();
                if segs.len() >= 2 && segs.iter().all(|s| !s.is_empty()) && segs.last().map(|s| s.len() > 3).unwrap_or(false) {
                    best = Some(j + 3);
                }
            }
            if let Some(b) = best {
                out.push(run[..b].to_string());
            }
        }
        k = e;
    }
    out
}

/// `§(\d{1,3})\b`: the ruling numbers a text cites.
pub fn section_cites(text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices('§') {
        let after = &text[i + '§'.len_utf8()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() || digits.len() > 3 {
            continue;
        }
        if after[digits.len()..].chars().next().map(is_word).unwrap_or(false) {
            continue;
        }
        if let Ok(n) = digits.parse() {
            out.push(n);
        }
    }
    out
}

/// `^## §?(\d+)[.:\s].*$`: each numbered ruling's number and byte offset of its heading line.
pub fn ruling_heads(text: &str) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        let Some(rest) = line.strip_prefix("## ") else { continue };
        let rest = rest.strip_prefix('§').unwrap_or(rest);
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        match rest[digits.len()..].chars().next() {
            Some(c) if c == '.' || c == ':' || c.is_whitespace() => {}
            _ => continue,
        }
        if let Ok(n) = digits.parse() {
            out.push((n, at));
        }
    }
    out
}

/// The first `20\d\d-\d\d-\d\d` in a text that is a real date.
fn first_date(text: &str) -> Option<Option<i64>> {
    let b = text.as_bytes();
    let mut i = 0;
    while i + 10 <= b.len() {
        if b[i] == b'2' && b[i + 1] == b'0' && b[i + 2..i + 4].iter().all(u8::is_ascii_digit) && b[i + 4] == b'-' && b[i + 5..i + 7].iter().all(u8::is_ascii_digit) && b[i + 7] == b'-' && b[i + 8..i + 10].iter().all(u8::is_ascii_digit) {
            return Some(text.get(i..i + 10).and_then(parse_day));
        }
        i += 1;
    }
    None
}

fn dirname(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `os.path.normpath(os.path.join(here, t))` for repository-relative paths.
fn join_norm(here: &str, t: &str) -> String {
    let mut parts: Vec<&str> = if t.starts_with('/') { Vec::new() } else { here.split('/').filter(|s| !s.is_empty()).collect() };
    for seg in t.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().map(|s| *s != "..").unwrap_or(false) {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn stable_id(prefix: &str, a: &str, b: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in a.bytes().chain(std::iter::once(0u8)).chain(b.bytes()) {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{prefix}{h:016x}")
}

/// The 32-bit placement seed of a cluster id (`cl-` + 16 hex digits): its low half.
fn seed_of(cluster: &str) -> u32 {
    u64::from_str_radix(cluster.trim_start_matches("cl-"), 16).map(|h| h as u32).unwrap_or(0)
}

/// An object's own placement key: a one-way hash of what it is (its company, its repository's folder
/// name and its path, ruling number or day), so the same object has the same key on every build
/// whatever was added around it. `field-prep.js` places each object from a stream seeded with it.
fn key_of(company: &str, repo: &str, what: &str) -> u32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in company.bytes().chain(std::iter::once(0u8)).chain(repo.bytes()).chain(std::iter::once(0u8)).chain(what.bytes()) {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (h ^ (h >> 32)) as u32
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------------------
// the pure half: what was read in, the field's own structure out
// ---------------------------------------------------------------------------

struct Node {
    ntype: &'static str,
    company: usize,
    cluster: String,
    key: u32,
    created: i64,
    recency: f64,
    significance: f64,
    activity: f64,
    degree: u32,
}

/// WHAT A COMPANY'S GROUPS WERE DECIDED TO BE, kept beside the picture so the next build decides
/// nothing again (review rv-20261114T004049Z-3a3331d3-4a8e: the six biggest folders were ranked on
/// every build, so when a seventh overtook the sixth, 600 of 2,900 objects changed group and 1,684
/// moved). A company's groups are chosen once, the first time it is drawn: which top folders are
/// split by their second level (and, for a top with more than twelve of those, which quarter each
/// second-level folder is in), and which groups stand on their own (at most six; the rest share
/// one). After that a folder that grows, or a new one, changes neither. Folder names are kept only
/// as one-way hashes, and nothing here is sent to the page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Groups {
    companies: BTreeMap<String, CompanyGroups>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct CompanyGroups {
    /// Hash of a split top folder -> `None` (one group per second-level folder) or the quarter of
    /// each second-level folder known when the company was first drawn (hash -> 0..=3).
    split: BTreeMap<String, Option<BTreeMap<String, u8>>>,
    /// The cluster ids that stand on their own.
    own: Vec<String>,
}

fn name_hash(name: &str) -> String {
    stable_id("", name, "")
}

/// The order the companies stand in around the picture: the biggest with its arc's middle as near
/// [`CROWN`] as the others allow, the others in the registry's order, as many before it as bring it
/// nearest, the rest after it. `sizes` is each company's object count, in registry order.
pub fn arc_order(sizes: &[usize]) -> Vec<usize> {
    let total: usize = sizes.iter().sum();
    if sizes.is_empty() || total == 0 {
        return (0..sizes.len()).collect();
    }
    let big = (0..sizes.len()).max_by(|&a, &b| sizes[a].cmp(&sizes[b]).then(b.cmp(&a))).unwrap();
    let others: Vec<usize> = (0..sizes.len()).filter(|&i| i != big).collect();
    let frac = |n: usize| n as f64 / total as f64;
    let half = frac(sizes[big]) / 2.0;
    let mut best = (f64::INFINITY, 0usize);
    let mut before = 0usize;
    for split in 0..=others.len() {
        if split > 0 {
            before += sizes[others[split - 1]];
        }
        let miss = (frac(before) + half - CROWN).abs();
        if miss < best.0 - 1e-12 {
            best = (miss, split);
        }
    }
    let mut out: Vec<usize> = others[..best.1].to_vec();
    out.push(big);
    out.extend_from_slice(&others[best.1..]);
    out
}

/// Build the dataset `home/field-*.js` reads, from what was read out of every company's
/// repositories. **Pure**: no clock, no file, no process; `today` is days since 1970-01-01.
/// `previous` is the company order of the picture drawn last time (empty for the first): companies
/// already drawn keep that order, so a company overtaking another moves no territory; a new company
/// joins at the end.
pub fn build_repo_field(companies: &[CompanyRead], unread: &[(String, String)], today: i64, previous: &[String]) -> Value {
    build_repo_field_with(companies, unread, today, previous, &mut Groups::default())
}

/// [`build_repo_field`] with the groups decided on earlier builds: a company in `groups` keeps
/// them, and a company drawn for the first time is added.
pub fn build_repo_field_with(companies: &[CompanyRead], unread: &[(String, String)], today: i64, previous: &[String], groups: &mut Groups) -> Value {
    let all_days: Vec<i64> = companies.iter().flat_map(|c| c.repos.iter().flat_map(|r| r.sessions.keys().copied())).collect();
    let day0 = all_days.iter().copied().min().unwrap_or(today).min(today);
    let horizon = (today - day0).max(0);
    let dnum = |d: i64| (d - day0).clamp(0, horizon);

    let mut nodes: Vec<Node> = Vec::new();
    let mut links: Vec<(usize, usize, &'static str)> = Vec::new();
    let mut link_seen: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut add_link = |nodes: &mut Vec<Node>, links: &mut Vec<(usize, usize, &'static str)>, a: usize, b: usize, cls: &'static str| -> bool {
        if a == b || !link_seen.insert((a.min(b), a.max(b))) {
            return false;
        }
        links.push((a, b, cls));
        nodes[a].degree += 1;
        nodes[b].degree += 1;
        true
    };
    let add_node = |nodes: &mut Vec<Node>, company: usize, ntype: &'static str, cluster: String, key: u32, created: i64, last: i64, commits: u32, recent: f64| -> usize {
        let recency = if last <= today { round3(1.0 - (today - last) as f64 / horizon.max(1) as f64) } else { 1.0 };
        let significance = round3(((commits as f64).ln_1p() / 40f64.ln_1p()).min(1.0));
        nodes.push(Node { ntype, company, cluster, key, created: dnum(created), recency, significance, activity: round3(recent), degree: 0 });
        nodes.len() - 1
    };

    // (company, repo, path) -> node
    let mut doc_node: HashMap<(usize, usize, String), usize> = HashMap::new();
    let mut ruling_node: HashMap<(usize, usize, String, u32), usize> = HashMap::new();
    let mut registers: Vec<(usize, usize, String)> = Vec::new();
    let mut session_node: HashMap<(usize, usize, i64), usize> = HashMap::new();
    let mut per_company: Vec<Value> = Vec::new();

    for (ci, c) in companies.iter().enumerate() {
        // every document of the company, with its repository index
        let docs: Vec<(usize, &FileRec)> = c.repos.iter().enumerate().flat_map(|(ri, r)| r.docs.iter().map(move |d| (ri, d))).collect();

        // ---- clusters: the folders a company's documents live in, at most seven per company ----
        let top_key = |p: &str| -> String { if p.contains('/') { p.split('/').next().unwrap().to_string() } else { ".".to_string() } };
        let mut tops: HashMap<String, usize> = HashMap::new();
        for (_, d) in &docs {
            *tops.entry(top_key(&d.path)).or_insert(0) += 1;
        }
        let seconds_of = |t: &str| -> Vec<String> {
            let mut s: Vec<String> = docs.iter().filter(|(_, x)| top_key(&x.path) == t && x.path.matches('/').count() >= 2).map(|(_, x)| x.path.split('/').nth(1).unwrap().to_string()).collect();
            s.sort();
            s.dedup();
            s
        };
        // Decided once, the first time the company is drawn (round 71's rule: a top folder holding
        // more than 40% of the documents is split by its second level, in quarters past twelve).
        let first_time = !groups.companies.contains_key(&c.id);
        let decided = groups.companies.entry(c.id.clone()).or_default();
        if first_time {
            for (t, n) in &tops {
                if *n as f64 > 0.4 * docs.len() as f64 {
                    let seconds = seconds_of(t);
                    let quarters = (seconds.len() > 12).then(|| seconds.iter().enumerate().map(|(at, s)| (name_hash(s), (at * 4 / seconds.len()).min(3) as u8)).collect());
                    decided.split.insert(name_hash(t), quarters);
                }
            }
        }
        let split = decided.split.clone();
        let cluster_key = |p: &str| -> String {
            let parts: Vec<&str> = p.split('/').collect();
            let t = top_key(p);
            if parts.len() > 2 {
                match split.get(&name_hash(&t)) {
                    Some(Some(quarters)) => {
                        // a second-level folder made since takes a quarter from its own name
                        let q = quarters.get(&name_hash(parts[1])).copied().unwrap_or_else(|| (seed_of(&stable_id("cl-", &t, parts[1])) % 4) as u8);
                        return format!("{t}/{} of 4", q + 1);
                    }
                    Some(None) => return format!("{t}/{}", parts[1]),
                    None => {}
                }
            }
            t
        };
        let keys: Vec<String> = docs.iter().map(|(_, d)| cluster_key(&d.path)).collect();
        let mut keyed: Vec<(String, usize)> = Vec::new();
        for k in &keys {
            match keyed.iter_mut().find(|(x, _)| x == k) {
                Some(e) => e.1 += 1,
                None => keyed.push((k.clone(), 1)),
            }
        }
        // The six that stand on their own: `Counter.most_common(6)` (by count, ties in first-seen
        // order) the first time. Later only a free place is filled, by a group that has no place yet,
        // so a folder overtaking another moves nothing.
        let mut ranked = keyed.clone();
        ranked.sort_by_key(|k| std::cmp::Reverse(k.1));
        for (k, _) in ranked {
            let id = stable_id("cl-", &c.id, &k);
            if decided.own.len() < 6 && !decided.own.contains(&id) {
                decided.own.push(id);
            }
        }
        let own = decided.own.clone();
        let cluster_of = |k: &str| -> String {
            let id = stable_id("cl-", &c.id, k);
            if own.contains(&id) { id } else { stable_id("cl-", &c.id, "\u{0}everything-else") }
        };
        let doc_cluster: HashMap<(usize, String), String> = docs.iter().zip(keys.iter()).map(|((ri, d), k)| ((*ri, d.path.clone()), cluster_of(k))).collect();
        let else_cluster = stable_id("cl-", &c.id, "\u{0}everything-else");

        // ---- documents ----
        let mut sorted: Vec<(usize, &FileRec)> = docs.clone();
        sorted.sort_by_key(|d| (d.0, &d.1.path));
        for (ri, d) in &sorted {
            let recent = if today - d.last <= 30 { (d.commits as f64 / 12.0).min(1.0) } else { 0.0 };
            let key = key_of(&c.id, &c.repos[*ri].name, &d.path);
            let i = add_node(&mut nodes, ci, classify(&d.path), doc_cluster[&(*ri, d.path.clone())].clone(), key, d.first, d.last, d.commits, recent);
            doc_node.insert((ci, *ri, d.path.clone()), i);
        }
        let node_of = |ri: usize, d: &FileRec| doc_node[&(ci, ri, d.path.clone())];

        // ---- the company's own root: the most-worked document at the top of its first repository ----
        let top_docs: Vec<(usize, &FileRec)> = { let t: Vec<_> = docs.iter().filter(|(_, d)| !d.path.contains('/')).copied().collect(); if t.is_empty() { docs.clone() } else { t } };
        let root_hub: Option<usize> = top_docs
            .iter()
            .max_by_key(|(_, d)| { let b = basename(&d.path).to_lowercase(); (b == "claude.md" || b == "readme.md", d.commits) })
            .map(|(ri, d)| node_of(*ri, d));

        // ---- membership: the folder tree ----
        let mut by_dir: BTreeMap<(usize, String), Vec<&FileRec>> = BTreeMap::new();
        for (ri, d) in &docs {
            by_dir.entry((*ri, dirname(&d.path).to_string())).or_default().push(d);
        }
        let mut anchor: HashMap<(usize, String), &FileRec> = HashMap::new();
        for (key, members) in &by_dir {
            let named: Vec<&&FileRec> = members.iter().filter(|m| matches!(basename(&m.path).to_lowercase().as_str(), "readme.md" | "index.md" | "dossier.md" | "claude.md" | "notes.md")).collect();
            let pool: Vec<&&FileRec> = if named.is_empty() { members.iter().collect() } else { named };
            // max by (commits, -len(path)); Python's max keeps the first of equals
            let mut a = pool[0];
            for m in &pool[1..] {
                if (m.commits, std::cmp::Reverse(m.path.len())) > (a.commits, std::cmp::Reverse(a.path.len())) {
                    a = m;
                }
            }
            anchor.insert(key.clone(), *a);
        }
        for ((ri, dpath), members) in &by_dir {
            let a = anchor[&(*ri, dpath.clone())];
            let an = node_of(*ri, a);
            let mut rest: Vec<&FileRec> = members.iter().filter(|m| m.path != a.path).copied().collect();
            rest.sort_by_key(|m| (m.first, &m.path));
            if rest.len() > 48 {
                for run in rest.chunks(40) {
                    let mut lead = run[0];
                    for m in &run[1..] {
                        if m.commits > lead.commits {
                            lead = m;
                        }
                    }
                    let ln = node_of(*ri, lead);
                    add_link(&mut nodes, &mut links, ln, an, "membership");
                    for m in run {
                        if m.path != lead.path {
                            add_link(&mut nodes, &mut links, node_of(*ri, m), ln, "membership");
                        }
                    }
                }
            } else {
                for m in &rest {
                    add_link(&mut nodes, &mut links, node_of(*ri, m), an, "membership");
                }
            }
            let mut up = dpath.as_str();
            let mut parent = None;
            while !up.is_empty() {
                up = dirname(up);
                if let Some(p) = anchor.get(&(*ri, up.to_string())) {
                    parent = Some(node_of(*ri, p));
                    break;
                }
            }
            match (parent, root_hub) {
                (Some(p), _) => {
                    add_link(&mut nodes, &mut links, an, p, "membership");
                }
                (None, Some(h)) if h != an => {
                    add_link(&mut nodes, &mut links, an, h, "membership");
                }
                _ => {}
            }
        }

        // ---- decision registers: one object per numbered ruling ----
        let mut n_rulings = 0usize;
        for (ri, d) in &docs {
            if !basename(&d.path).to_lowercase().contains("decisions") {
                continue;
            }
            let Some(text) = c.repos[*ri].texts.get(&d.path) else { continue };
            let heads = ruling_heads(text);
            if heads.len() < 3 {
                continue;
            }
            let dn = node_of(*ri, d);
            registers.push((ci, *ri, d.path.clone()));
            for (hi, (num, at)) in heads.iter().enumerate() {
                let end = heads.get(hi + 1).map(|h| h.1).unwrap_or(text.len());
                let body = &text[*at..end];
                let when = match first_date(body) {
                    Some(Some(day)) => d.first.max(today.min(day)),
                    _ => d.first,
                };
                let weight = 6 + (body.chars().count() / 600).min(30) as u32;
                let cl = nodes[dn].cluster.clone();
                let key = key_of(&c.id, &c.repos[*ri].name, &format!("{}\u{0}ruling {num}", d.path));
                let i = add_node(&mut nodes, ci, "decision", cl, key, when, when, weight, 0.0);
                ruling_node.insert((ci, *ri, d.path.clone(), *num), i);
                add_link(&mut nodes, &mut links, i, dn, "membership");
                n_rulings += 1;
            }
        }

        // ---- working days ----
        let mut created_on: HashMap<(usize, i64), Vec<&FileRec>> = HashMap::new();
        for (ri, d) in &docs {
            created_on.entry((*ri, d.first)).or_default().push(d);
        }
        let mut days = 0usize;
        for (ri, r) in c.repos.iter().enumerate() {
            let mut last_of_repo: Option<usize> = None;
            for (&day, &n) in &r.sessions {
                days += 1;
                let made = created_on.get(&(ri, day)).cloned().unwrap_or_default();
                // the group of the first of them by path, which later commits to a document never change
                let cl = match made.iter().min_by_key(|m| &m.path) {
                    Some(m) => nodes[node_of(ri, m)].cluster.clone(),
                    None => root_hub.map(|h| nodes[h].cluster.clone()).unwrap_or_else(|| else_cluster.clone()),
                };
                let recent = if today - day <= 30 { (n as f64 / 60.0).min(1.0) } else { 0.0 };
                let key = key_of(&c.id, &r.name, &format!("\u{0}day {day}"));
                let i = add_node(&mut nodes, ci, "memory", cl, key, day, day, n, recent);
                session_node.insert((ci, ri, day), i);
                let mut made_sorted = made.clone();
                made_sorted.sort_by_key(|m| std::cmp::Reverse(m.commits));
                for m in made_sorted.iter().take(8) {
                    add_link(&mut nodes, &mut links, i, node_of(ri, m), "membership");
                }
                match (last_of_repo, root_hub) {
                    (Some(prev), _) => {
                        add_link(&mut nodes, &mut links, i, prev, "relationship");
                    }
                    (None, Some(h)) => {
                        add_link(&mut nodes, &mut links, i, h, "membership");
                    }
                    _ => {}
                }
                last_of_repo = Some(i);
            }
        }

        per_company.push(json!({
            "id": c.id,
            "documents": docs.len(),
            "rulings": n_rulings,
            "workingDays": days,
            "commits": c.repos.iter().map(|r| r.commits).sum::<u32>(),
            "sourceFiles": c.repos.iter().map(|r| r.code.len()).sum::<usize>(),
            "repositories": c.repos.len(),
        }));
    }

    // ---- relationships: one document naming another ----
    let mut by_base: HashMap<(usize, usize, String), Vec<String>> = HashMap::new();
    for (ci, c) in companies.iter().enumerate() {
        for (ri, r) in c.repos.iter().enumerate() {
            for d in &r.docs {
                by_base.entry((ci, ri, basename(&d.path).to_string())).or_default().push(d.path.clone());
            }
        }
    }
    let mut repo_names: HashMap<String, (usize, usize)> = HashMap::new();
    for (ci, c) in companies.iter().enumerate() {
        for (ri, r) in c.repos.iter().enumerate() {
            if !r.name.is_empty() {
                repo_names.insert(r.name.clone(), (ci, ri));
            }
        }
    }
    let mut main_register: HashMap<(usize, usize), String> = HashMap::new();
    for (ci, ri, p) in &registers {
        main_register.entry((*ci, *ri)).or_insert_with(|| p.clone());
    }
    let ceo_register = registers.iter().find(|(_, _, p)| p.ends_with("ceo-decisions.md")).cloned();
    let (mut n_ref, mut n_cite) = (0usize, 0usize);
    for (ci, c) in companies.iter().enumerate() {
        for (ri, r) in c.repos.iter().enumerate() {
            for d in &r.docs {
                let Some(text) = r.texts.get(&d.path) else { continue };
                let here = dirname(&d.path);
                let me = doc_node[&(ci, ri, d.path.clone())];
                let mut found: Vec<usize> = Vec::new();
                for t in md_links(text) {
                    if t.contains("://") {
                        continue;
                    }
                    let cand = join_norm(here, &t);
                    let bare = t.trim_start_matches(['.', '/']).to_string();
                    if let Some(&n) = doc_node.get(&(ci, ri, cand)) {
                        found.push(n);
                    } else if let Some(&n) = doc_node.get(&(ci, ri, bare)) {
                        found.push(n);
                    }
                }
                for t in md_paths(text) {
                    let t = t.trim_start_matches(['.', '/']).to_string();
                    if let Some(&n) = doc_node.get(&(ci, ri, t.clone())) {
                        found.push(n);
                        continue;
                    }
                    let first = t.split('/').next().unwrap_or("");
                    if let Some(&(oc, or)) = repo_names.get(first) {
                        if let Some(&n) = doc_node.get(&(oc, or, t[first.len() + 1..].to_string())) {
                            found.push(n);
                            continue;
                        }
                    }
                    if let Some(same) = by_base.get(&(ci, ri, basename(&t).to_string())) {
                        if same.len() == 1 {
                            found.push(doc_node[&(ci, ri, same[0].clone())]);
                        }
                    }
                }
                let mut used = 0;
                let mut seen_t: BTreeSet<usize> = BTreeSet::new();
                for t in found {
                    if !seen_t.insert(t) {
                        continue;
                    }
                    if used >= 4 {
                        break;
                    }
                    if add_link(&mut nodes, &mut links, me, t, "relationship") {
                        used += 1;
                        n_ref += 1;
                    }
                }
                let reg = main_register.get(&(ci, ri)).map(|p| (ci, ri, p.clone())).or_else(|| ceo_register.clone());
                if let Some((rc, rr, rp)) = reg {
                    if !(rc == ci && rr == ri && rp == d.path) {
                        let mut cites = 0;
                        let mut seen_n: BTreeSet<u32> = BTreeSet::new();
                        for n in section_cites(text) {
                            if !seen_n.insert(n) {
                                continue;
                            }
                            if let Some(&t) = ruling_node.get(&(rc, rr, rp.clone(), n)) {
                                if cites < 2 && add_link(&mut nodes, &mut links, me, t, "relationship") {
                                    cites += 1;
                                    n_cite += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ---- sources: every document and source file ----
    let mut sources: Vec<Value> = Vec::new();
    for (ci, c) in companies.iter().enumerate() {
        for (ri, r) in c.repos.iter().enumerate() {
            for d in &r.docs {
                sources.push(json!({ "id": format!("s{:05}", sources.len()), "kind": "document", "domain": c.id,
                    "ingestedDay": dnum(d.last), "primaryNodeId": format!("n{:05}", doc_node[&(ci, ri, d.path.clone())]), "derivedCount": 1 }));
            }
            for f in &r.code {
                let Some(&n) = session_node.get(&(ci, ri, f.last)) else { continue };
                sources.push(json!({ "id": format!("s{:05}", sources.len()), "kind": "a source file", "domain": c.id,
                    "ingestedDay": dnum(f.last), "primaryNodeId": format!("n{:05}", n), "derivedCount": 1 }));
            }
        }
    }

    // ---- territories, in arc order ----
    let sizes: Vec<usize> = (0..companies.len()).map(|ci| nodes.iter().filter(|n| n.company == ci).count()).collect();
    let order: Vec<usize> = {
        let kept: Vec<usize> = previous.iter().filter_map(|id| companies.iter().position(|c| &c.id == id)).collect();
        if kept.is_empty() {
            arc_order(&sizes)
        } else {
            let mut o = kept.clone();
            o.extend((0..companies.len()).filter(|i| !kept.contains(i)));
            o
        }
    };
    let domains: Vec<Value> = order
        .into_iter()
        .filter(|&ci| sizes[ci] > 0)
        .map(|ci| {
            let mut cl: Vec<(String, usize)> = Vec::new();
            for n in nodes.iter().filter(|n| n.company == ci) {
                match cl.iter_mut().find(|(k, _)| *k == n.cluster) {
                    Some(e) => e.1 += 1,
                    None => cl.push((n.cluster.clone(), 1)),
                }
            }
            // in the order of their stable ids, not their sizes: a folder that grows past another does
            // not swap the two clusters' strands
            cl.sort();
            let clusters: Vec<Value> = cl.iter().enumerate().map(|(i, (k, v))| json!({ "id": k, "label": format!("Group {}", i + 1), "nodeCount": v })).collect();
            json!({ "id": companies[ci].id, "label": companies[ci].label, "nodeCount": sizes[ci], "clusters": clusters })
        })
        .collect();

    let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
    for n in &nodes {
        *by_type.entry(n.ntype).or_insert(0) += 1;
    }
    let cross = links.iter().filter(|(a, b, _)| nodes[*a].company != nodes[*b].company).count();
    let node_values: Vec<Value> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| json!({ "id": format!("n{:05}", i), "type": n.ntype, "domain": companies[n.company].id, "cluster": n.cluster,
            "createdDay": n.created, "recency": n.recency, "significance": n.significance, "activity": n.activity, "degree": n.degree,
            // each object is placed from its own stream (`field-prep.js` `placeKeyed`), so new work
            // moves nothing that was already drawn
            "layoutSeed": seed_of(&n.cluster), "layoutKey": n.key }))
        .collect();
    let link_values: Vec<Value> = links
        .iter()
        .map(|(a, b, cls)| json!({ "s": format!("n{:05}", a), "t": format!("n{:05}", b), "class": cls, "cross": nodes[*a].company != nodes[*b].company }))
        .collect();
    let months = (horizon as f64 / 30.4).round() as i64;
    let count = |t: &str| by_type.get(t).copied().unwrap_or(0);
    json!({
        "meta": {
            "name": "RichOS company memory, from the connected repositories",
            "version": 1,
            // THE ONE FIELD THE BANNER READS (`home.js::fieldDataIsCustomers`).
            "synthetic": false,
            "source": "repositories",
            "horizonDays": horizon,
            "monthsOfHistory": months,
            "counts": {
                "records": nodes.len(),
                "objects": nodes.len(),
                "links": links.len(),
                "sources": sources.len(),
                "domains": domains.len(),
                "crossCompanyLinks": cross,
                "documentReferences": n_ref,
                "rulingCitations": n_cite,
                "byType": by_type,
                "perCompany": per_company,
            },
            // A saved folder that could not be read: the company and why, never its path.
            "unreadFolders": unread.iter().map(|(c, why)| json!({ "company": c, "reason": why })).collect::<Vec<_>>(),
            "absent": ["tasks handled without you", "hours of attention saved", "specialists: the home screen's team comes from home_team"],
        },
        "bragSignals": {
            "note": "Read from the connected repositories. A zero is something they do not say.",
            "specialistsManaged": 0,
            "specialistsActiveNow": 0,
            "tasksCompleted": 0,
            "tasksHandledWithoutCeo": 0,
            "ceoMinutesSaved": 0,
            "monthsWorkingTogether": months,
            "sourcesUnderstood": sources.len(),
            "memoryObjects": nodes.len(),
            "decisionsRemembered": count("decision"),
            "lessonsAccumulated": count("lesson"),
            "relationshipsUnderstood": links.len(),
            "commitmentsTracked": count("commitment"),
        },
        "specialists": [],
        "tasks": [],
        "domains": domains,
        "nodes": node_values,
        "links": link_values,
        "sources": sources,
    })
}

// ---------------------------------------------------------------------------
// reading every company, and keeping the result
// ---------------------------------------------------------------------------

/// Read every company's folders. A folder that does not exist, or that `git` cannot read as a
/// repository, is skipped and reported as `(company id, reason)`.
pub fn read_companies(companies: &[CompanyFolders]) -> (Vec<CompanyRead>, Vec<(String, String)>) {
    let mut out = Vec::new();
    let mut unread = Vec::new();
    for c in companies {
        let mut repos = Vec::new();
        for f in &c.folders {
            if !f.is_dir() {
                unread.push((c.id.clone(), "missing".to_string()));
                continue;
            }
            match read_repo(f) {
                Some(r) => repos.push(r),
                None => unread.push((c.id.clone(), "not a repository".to_string())),
            }
        }
        out.push(CompanyRead { id: c.id.clone(), label: c.label.clone(), repos });
    }
    (out, unread)
}

/// What the kept picture was built from: the day (dates are relative to it), every company and
/// folder, and each folder's commit. Nothing committed and the same day is the same picture.
pub fn identity(companies: &[CompanyFolders], today: i64) -> String {
    let mut s = format!("v1|{today}");
    for c in companies {
        s.push_str(&format!("|{}={}", c.id, c.label));
        for f in &c.folders {
            let head = if f.is_dir() { repo_head(f).unwrap_or_default() } else { "missing".into() };
            s.push_str(&format!(";{}@{}", f.display(), head));
        }
    }
    s
}

#[derive(Serialize, Deserialize)]
struct Kept {
    identity: String,
    field: Value,
    /// What each company's groups were decided to be; a kept file from before this has none.
    #[serde(default)]
    groups: Groups,
}

/// The company order of a kept picture.
fn order_of(field: &Value) -> Vec<String> {
    field["domains"].as_array().map(|d| d.iter().filter_map(|x| x["id"].as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// Read, build and keep. `None` when no folder is a readable repository.
fn build_and_keep(path: &Path, companies: &[CompanyFolders], today: i64, id: String, previous: &[String], mut groups: Groups) -> Option<Value> {
    let (read, unread) = read_companies(companies);
    if read.iter().all(|c| c.repos.is_empty()) {
        return None;
    }
    let field = build_repo_field_with(&read, &unread, today, previous, &mut groups);
    let tmp = path.with_extension("json.tmp");
    let kept = Kept { identity: id, field, groups };
    let wrote = serde_json::to_vec(&kept).ok().map(|bytes| std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path)));
    if !matches!(wrote, Some(Ok(()))) {
        eprintln!("[richos] home field: could not keep the repository picture; the next launch reads the repositories again");
    }
    Some(kept.field)
}

/// One refresh at a time, behind the answer.
static REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What [`field_kept_or_built`] answered from, for the log line and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFrom {
    /// The kept picture, and it is current.
    Kept,
    /// The kept picture while a fresh one is read behind it, drawn at the next launch.
    KeptRefreshing,
    /// Read now: there was no kept picture.
    Built,
}

/// The picture for these companies. Nothing committed since it was built (and the same day): the
/// kept one. Something changed: still the kept one, at once, while the repositories are read again
/// behind it and kept for the next launch (a team commits all day, and reading every history and
/// document on every launch would be the slow path every time). No kept picture: read now. `None`
/// when no folder of any company is a readable repository, which leaves the home screen on its loro
/// path.
pub fn field_kept_or_built(path: &Path, companies: &[CompanyFolders], today: i64) -> Option<(Value, FieldFrom)> {
    if !companies.iter().any(|c| c.folders.iter().any(|f| f.is_dir())) {
        return None;
    }
    let id = identity(companies, today);
    if let Some(kept) = std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<Kept>(&b).ok()) {
        if kept.identity == id {
            return Some((kept.field, FieldFrom::Kept));
        }
        if !REFRESHING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let (path, companies, previous, groups) = (path.to_path_buf(), companies.to_vec(), order_of(&kept.field), kept.groups.clone());
            std::thread::spawn(move || {
                let _ = build_and_keep(&path, &companies, today, id, &previous, groups);
                REFRESHING.store(false, std::sync::atomic::Ordering::SeqCst);
            });
        }
        return Some((kept.field, FieldFrom::KeptRefreshing));
    }
    build_and_keep(path, companies, today, id, &[], Groups::default()).map(|f| (f, FieldFrom::Built))
}

/// Wait for a refresh started by [`field_kept_or_built`] to finish. Tests only.
#[cfg(test)]
fn wait_for_refresh() {
    while REFRESHING.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, EntityRegistry};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("home-repos-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A repository on disk with `files` committed on `day` (one commit per call), using a fixed
    /// author and no user configuration.
    fn commit(repo: &Path, day: &str, files: &[(&str, &str)]) {
        let env = [("GIT_CONFIG_GLOBAL", "/dev/null"), ("GIT_CONFIG_NOSYSTEM", "1"), ("GIT_AUTHOR_NAME", "T"), ("GIT_AUTHOR_EMAIL", "t@example.com"),
            ("GIT_COMMITTER_NAME", "T"), ("GIT_COMMITTER_EMAIL", "t@example.com")];
        if !repo.join(".git").exists() {
            assert!(Command::new("git").args(["init", "-q"]).current_dir(repo).envs(env).status().unwrap().success());
        }
        for (p, body) in files {
            let f = repo.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, body).unwrap();
        }
        assert!(Command::new("git").args(["add", "-A"]).current_dir(repo).envs(env).status().unwrap().success());
        let date = format!("{day}T12:00:00");
        assert!(Command::new("git").args(["commit", "-q", "-m", "c"]).current_dir(repo).envs(env).env("GIT_AUTHOR_DATE", &date).env("GIT_COMMITTER_DATE", &date).status().unwrap().success());
    }

    fn registry(rows: &[(&str, &str, &[&Path])]) -> EntityRegistry {
        EntityRegistry::new(rows.iter().map(|(id, name, roots)| Entity::try_new(id, name, roots.iter().map(|p| p.to_path_buf()).collect()).unwrap()).collect()).unwrap()
    }

    fn domain<'a>(field: &'a Value, id: &str) -> Option<&'a Value> {
        field["domains"].as_array().unwrap().iter().find(|d| d["id"] == id)
    }

    fn day(s: &str) -> i64 {
        parse_day(s).unwrap()
    }

    /// CAUSE 1: connecting a company only recorded its folder; nothing was read from it. Every
    /// connected company's repository is now read, and each company is a domain of the picture.
    #[test]
    fn every_connected_companys_repository_is_read_into_the_picture() {
        let a = tmp("c1a");
        let b = tmp("c1b");
        commit(&a, "2026-10-01", &[("README.md", "# A\n\nSee docs/plan.md.\n"), ("docs/plan.md", "# Plan\n")]);
        commit(&a, "2026-10-02", &[("docs/notes.md", "# Notes\n")]);
        commit(&b, "2026-10-05", &[("README.md", "# B\n"), ("src/main.rs", "fn main() {}\n")]);
        let reg = registry(&[("alpha", "Alpha Co", &[a.as_path()]), ("beta", "Beta Co", &[b.as_path()])]);
        let path = tmp("c1k").join("home-field-repos.json");
        let (field, from) = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).expect("two readable repositories drew nothing");
        assert_eq!(from, FieldFrom::Built);
        let alpha = domain(&field, "alpha").expect("Alpha is not on the picture");
        let beta = domain(&field, "beta").expect("Beta is not on the picture");
        // Alpha: 3 documents + 2 working days; Beta: 1 document + 1 working day
        assert_eq!((alpha["nodeCount"].as_u64(), alpha["label"].as_str()), (Some(5), Some("Alpha Co")));
        assert_eq!((beta["nodeCount"].as_u64(), beta["label"].as_str()), (Some(2), Some("Beta Co")));
        assert_eq!(field["meta"]["synthetic"], false);
        // Beta's source file is on the river
        assert!(field["sources"].as_array().unwrap().iter().any(|s| s["domain"] == "beta" && s["kind"] == "a source file"));
        // kept: a second ask with nothing committed answers from the file
        let again = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).unwrap();
        assert_eq!(again, (field.clone(), FieldFrom::Kept));
        // a new commit: the kept picture answers at once, the new one is read behind it for the next ask
        commit(&b, "2026-10-06", &[("docs/more.md", "# More\n")]);
        let stale = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).unwrap();
        assert_eq!(stale, (field.clone(), FieldFrom::KeptRefreshing));
        wait_for_refresh();
        let (after, from) = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).unwrap();
        assert_eq!((domain(&after, "beta").unwrap()["nodeCount"].as_u64(), from), (Some(4), FieldFrom::Kept));
        // the companies keep the order they were first drawn in
        assert_eq!(order_of(&after), order_of(&field));
        for d in [a, b, path.parent().unwrap().to_path_buf()] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// CAUSE 2: loro took only `wiki/`, `loro/memory`, records and entities. Every tracked Markdown
    /// document is an object, wherever it lives, and so is every numbered ruling of a register.
    #[test]
    fn every_tracked_document_and_ruling_is_an_object_wherever_it_lives() {
        let r = tmp("c2");
        let register = "# Decisions\n\n## 1. One\nDecided 2026-10-03.\n\n## 2. Two\nText.\n\n## §3: Three\nText.\n";
        commit(&r, "2026-10-01", &[("notes.md", "n"), ("docs/plans/x.md", "x"), ("app/src/README.md", "r"), ("wiki/ceo-decisions.md", register), ("src/lib.rs", "")]);
        let reg = registry(&[("acme", "Acme", &[r.as_path()])]);
        let (read, unread) = read_companies(&company_folders(&reg));
        assert!(unread.is_empty());
        let field = build_repo_field(&read, &unread, day("2026-11-13"), &[]);
        // 4 documents + 3 rulings + 1 working day
        assert_eq!(field["meta"]["counts"]["objects"].as_u64(), Some(8));
        assert_eq!(field["meta"]["counts"]["byType"]["decision"].as_u64(), Some(4), "the register and its three rulings are decisions");
        assert_eq!(field["meta"]["counts"]["perCompany"][0]["rulings"].as_u64(), Some(3));
        let _ = std::fs::remove_dir_all(r);
    }

    /// CAUSE 3: the picture was one ranked compile per company name behind a relevance floor (186 of
    /// 899 records). Nothing is ranked or cut: every document of every company is drawn.
    #[test]
    fn nothing_is_ranked_or_cut_every_document_is_drawn() {
        let r = tmp("c3");
        let files: Vec<(String, String)> = (0..150).map(|i| (format!("area{}/doc{i}.md", i % 9), format!("# Unrelated {i}\n\nzebra{i} quartz{i}\n"))).collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect();
        commit(&r, "2026-09-01", &refs);
        let reg = registry(&[("acme", "Acme", &[r.as_path()])]);
        let (read, unread) = read_companies(&company_folders(&reg));
        let field = build_repo_field(&read, &unread, day("2026-11-13"), &[]);
        let docs = field["sources"].as_array().unwrap().iter().filter(|s| s["kind"] == "document").count();
        assert_eq!(docs, 150, "a document was dropped");
        assert_eq!(field["meta"]["counts"]["objects"].as_u64(), Some(151), "150 documents and one working day");
        let _ = std::fs::remove_dir_all(r);
    }

    /// CAUSE 5: a saved folder that does not exist (the repository moved) was silent. It is skipped,
    /// named by company in `meta.unreadFolders`, and the rest of the picture is still drawn.
    #[test]
    fn a_missing_folder_is_reported_by_company_and_the_rest_is_drawn() {
        let ok = tmp("c5");
        commit(&ok, "2026-10-01", &[("README.md", "# Ok\n")]);
        let gone = std::env::temp_dir().join(format!("home-repos-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&gone);
        let reg = registry(&[("present", "Present", &[ok.as_path()]), ("moved", "Moved", &[gone.as_path()])]);
        let path = tmp("c5k").join("home-field-repos.json");
        let (field, _) = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).expect("one missing folder lost the whole picture");
        assert!(domain(&field, "present").is_some());
        assert_eq!(field["meta"]["unreadFolders"], json!([{ "company": "moved", "reason": "missing" }]));
        assert!(!serde_json::to_string(&field).unwrap().contains(&gone.display().to_string()), "the folder's path left the boundary");
        // and no readable repository at all is the loro path, not an empty picture
        let reg2 = registry(&[("moved", "Moved", &[gone.as_path()])]);
        assert!(field_kept_or_built(&path, &company_folders(&reg2), day("2026-11-13")).is_none());
        for d in [ok, path.parent().unwrap().to_path_buf()] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// Shape only: no title, file name, folder name or path crosses the boundary.
    #[test]
    fn no_file_name_folder_name_or_text_leaves_the_builder() {
        let r = tmp("priv");
        commit(&r, "2026-10-01", &[("secret-plans/merger-with-globex.md", "# Merger with Globex\nconfidential\n"), ("README.md", "see secret-plans/merger-with-globex.md")]);
        let reg = registry(&[("acme", "Acme", &[r.as_path()])]);
        let (read, unread) = read_companies(&company_folders(&reg));
        let out = serde_json::to_string(&build_repo_field(&read, &unread, day("2026-11-13"), &[])).unwrap();
        for word in ["secret", "merger", "globex", "Merger", "confidential", "README", "home-repos-priv"] {
            assert!(!out.contains(word), "{word} left the builder");
        }
        let _ = std::fs::remove_dir_all(r);
    }

    #[test]
    fn threads_follow_folders_references_days_and_cross_companies() {
        let mut texts = HashMap::new();
        // README already reaches docs/plan.md through the folder tree, so a link between them is not a
        // second thread; plan -> decisions is new, and so is README -> the other repository's spec.
        texts.insert("README.md".to_string(), "Top. Read [the plan](docs/plan.md) and other/docs/spec.md.".to_string());
        texts.insert("docs/plan.md".to_string(), "Per §2, see [the register](../decisions.md).".to_string());
        texts.insert("decisions.md".to_string(), "## 1. a\n## 2. b\n## 3. c\n".to_string());
        let rec = |p: &str, d: &str| FileRec { path: p.into(), first: day(d), last: day(d), commits: 1 };
        let a = RepoRead { name: "mine".into(), docs: vec![rec("README.md", "2026-10-01"), rec("docs/plan.md", "2026-10-02"), rec("decisions.md", "2026-10-01")],
            sessions: BTreeMap::from([(day("2026-10-01"), 1), (day("2026-10-02"), 1)]), commits: 2, texts, ..Default::default() };
        let b = RepoRead { name: "other".into(), docs: vec![rec("docs/spec.md", "2026-10-03")], sessions: BTreeMap::from([(day("2026-10-03"), 1)]), commits: 1, ..Default::default() };
        let field = build_repo_field(&[CompanyRead { id: "a".into(), label: "A".into(), repos: vec![a] }, CompanyRead { id: "b".into(), label: "B".into(), repos: vec![b] }], &[], day("2026-11-13"), &[]);
        let c = &field["meta"]["counts"];
        assert_eq!(c["documentReferences"].as_u64(), Some(2), "the relative link and the cross-repository path");
        assert_eq!(c["rulingCitations"].as_u64(), Some(1), "§2 cites ruling 2");
        assert!(c["crossCompanyLinks"].as_u64().unwrap() >= 1, "a reference into the other company is cross");
        assert_eq!(field["meta"]["horizonDays"].as_i64(), Some(day("2026-11-13") - day("2026-10-01")));
    }

    /// Sage 2026-11-13, fact 1: new work must not rearrange the picture. Every object carries its
    /// cluster's placement seed and a placement key of its own, the same on every build whatever was
    /// added around it, and a company drawn before keeps its place in the order when another
    /// overtakes it.
    #[test]
    fn placement_seeds_and_company_order_survive_new_work() {
        let rec = |p: &str| FileRec { path: p.into(), first: day("2026-10-01"), last: day("2026-10-01"), commits: 1 };
        let company = |id: &str, n: usize| CompanyRead { id: id.into(), label: id.to_uppercase(), repos: vec![RepoRead {
            docs: (0..n).map(|i| rec(&format!("docs/d{i}.md"))).collect(), sessions: BTreeMap::from([(day("2026-10-01"), 1)]), commits: 1, ..Default::default() }] };
        let before = build_repo_field(&[company("small", 2), company("big", 9)], &[], day("2026-11-13"), &[]);
        let seed = |f: &Value, cid: &str| f["nodes"].as_array().unwrap().iter().find(|n| n["domain"] == cid).unwrap()["layoutSeed"].as_u64().unwrap();
        // "small" grows past "big"
        let after = build_repo_field(&[company("small", 20), company("big", 9)], &[], day("2026-11-13"), &order_of(&before));
        assert_eq!(order_of(&after), order_of(&before));
        assert_eq!((seed(&after, "small"), seed(&after, "big")), (seed(&before, "small"), seed(&before, "big")));
        assert_ne!(seed(&before, "small"), seed(&before, "big"));
        // an object's key is its own: the documents drawn before keep theirs when 18 more are added,
        // although every ordinal id after them changed
        let keys = |f: &Value| -> Vec<u64> { f["nodes"].as_array().unwrap().iter().filter(|n| n["domain"] == "big").map(|n| n["layoutKey"].as_u64().unwrap()).collect() };
        assert_eq!(keys(&after), keys(&before));
        let mut distinct = keys(&before);
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), keys(&before).len(), "two objects share a placement key");
        // a company connected later joins at the end
        let more = build_repo_field(&[company("small", 20), company("big", 9), company("new", 50)], &[], day("2026-11-13"), &order_of(&after));
        assert_eq!(order_of(&more).last().map(String::as_str), Some("new"));
    }

    /// Review rv-20261114T004049Z-3a3331d3-4a8e: the six biggest folders were ranked on every build,
    /// so a seventh folder overtaking the sixth moved every document of both into another group.
    /// The groups are decided once and kept with the picture.
    #[test]
    fn a_folder_overtaking_another_changes_no_objects_group() {
        let rec = |p: String| FileRec { path: p, first: day("2026-10-01"), last: day("2026-10-01"), commits: 1 };
        // seven folders, the seventh the smallest
        let company = |sizes: [usize; 7]| CompanyRead { id: "acme".into(), label: "Acme".into(), repos: vec![RepoRead {
            docs: sizes.iter().enumerate().flat_map(|(f, n)| (0..*n).map(move |i| (f, i))).map(|(f, i)| rec(format!("area{f}/d{i}.md"))).collect(),
            sessions: BTreeMap::from([(day("2026-10-01"), 1)]), commits: 1, ..Default::default() }] };
        let clusters = |f: &Value| -> HashMap<u64, String> { f["nodes"].as_array().unwrap().iter().map(|n| (n["layoutKey"].as_u64().unwrap(), n["cluster"].as_str().unwrap().to_string())).collect() };
        let mut groups = Groups::default();
        let before = clusters(&build_repo_field_with(&[company([10, 9, 8, 7, 6, 5, 4])], &[], day("2026-11-13"), &[], &mut groups));
        // the seventh folder grows past the sixth and the fifth
        let after = clusters(&build_repo_field_with(&[company([10, 9, 8, 7, 6, 5, 7])], &[], day("2026-11-13"), &[], &mut groups));
        let changed = before.iter().filter(|(key, cluster)| after.get(*key) != Some(*cluster)).count();
        assert_eq!(changed, 0, "{changed} of {} objects changed group when one folder overtook another", before.len());
        assert_eq!(after.len(), before.len() + 3);
        // and without the kept groups the same growth does move them: the defect, kept visible
        let cold = clusters(&build_repo_field(&[company([10, 9, 8, 7, 6, 5, 7])], &[], day("2026-11-13"), &[]));
        assert!(before.iter().any(|(key, cluster)| cold.get(key) != Some(cluster)), "the fixture no longer crosses the ranking");
    }

    /// The same through the kept file: the groups decided at the first build are read back for the
    /// build behind a later launch.
    #[test]
    fn the_kept_picture_keeps_its_groups_across_builds() {
        let r = tmp("grp");
        let mut files: Vec<(String, String)> = Vec::new();
        for (f, n) in [10, 9, 8, 7, 6, 5, 4].iter().enumerate() {
            for i in 0..*n {
                files.push((format!("area{f}/d{i}.md"), format!("# {f} {i}\n")));
            }
        }
        let refs: Vec<(&str, &str)> = files.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect();
        commit(&r, "2026-10-01", &refs);
        let reg = registry(&[("acme", "Acme", &[r.as_path()])]);
        let path = tmp("grpk").join("home-field-repos.json");
        let clusters = |f: &Value| -> HashMap<u64, String> { f["nodes"].as_array().unwrap().iter().map(|n| (n["layoutKey"].as_u64().unwrap(), n["cluster"].as_str().unwrap().to_string())).collect() };
        let (first, _) = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).unwrap();
        commit(&r, "2026-10-01", &[("area6/n1.md", "n"), ("area6/n2.md", "n"), ("area6/n3.md", "n")]);
        assert!(field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).is_some());
        wait_for_refresh();
        let (second, from) = field_kept_or_built(&path, &company_folders(&reg), day("2026-11-13")).unwrap();
        assert_eq!(from, FieldFrom::Kept);
        let (before, after) = (clusters(&first), clusters(&second));
        assert_eq!(before.iter().filter(|(key, cluster)| after.get(*key) != Some(*cluster)).count(), 0);
        assert_eq!(after.len(), before.len() + 3);
        for d in [r, path.parent().unwrap().to_path_buf()] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn the_biggest_company_stands_at_the_crown() {
        // round 71's six companies, in his registry's order
        let sizes = [908, 649, 1620, 3896, 19, 94];
        let order = arc_order(&sizes);
        assert_eq!(order.len(), 6);
        let total: usize = sizes.iter().sum();
        let before: usize = order.iter().take_while(|&&i| i != 3).map(|&i| sizes[i]).sum();
        let middle = (before as f64 + sizes[3] as f64 / 2.0) / total as f64;
        assert!((middle - CROWN).abs() < 0.1, "the biggest company's middle is at {middle}, not near the crown");
        assert_eq!(arc_order(&[5]), vec![0]);
        assert_eq!(arc_order(&[]), Vec::<usize>::new());
    }

    #[test]
    fn the_text_patterns_match_round_71s_expressions() {
        assert_eq!(md_links("a [x](docs/a.md) [y](https://e.com/b.md) [z](c.md#top) [w](d.txt)"), vec!["docs/a.md", "https://e.com/b.md", "c.md"]);
        assert_eq!(md_paths("see wiki/a.md, /abs/b.md and x/y/c.md. not d.md or e/f.txt"), vec!["wiki/a.md", "x/y/c.md"]);
        assert_eq!(section_cites("§12 and §1234 and §7a and §3."), vec![12, 3]);
        assert_eq!(ruling_heads("## 1. a\n## §2: b\n## x\n## 33 c\n##4. no\n").iter().map(|h| h.0).collect::<Vec<_>>(), vec![1, 2, 33]);
        assert_eq!(join_norm("docs/sub", "../a.md"), "docs/a.md");
        assert_eq!(parse_day("2026-02-30"), None);
        assert_eq!(parse_day("1970-01-02"), Some(1));
        assert_eq!(classify("wiki/ceo-decisions.md"), "decision");
        assert_eq!(classify("docs/plans/x.md"), "initiative");
    }
}
