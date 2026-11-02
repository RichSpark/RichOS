//! THE HOME SCREEN'S PICTURE, COMPILED FROM THE CUSTOMER'S OWN CORPUS.
//!
//! `app/ui/home.js` (the banner block, lines 240-285) names the seam this file closes, and
//! names it precisely because it was not built:
//!
//! > What is missing is a command — call it `home_field_data` — that compiles that corpus
//! > into the `{meta, nodes, links, sources, ...}` structure `home/field-*.js` reads, and
//! > stamps its `meta` with `"synthetic": false` plus the corpus root it came from.
//!
//! This module is the compiling half. The Tauri command is `main.rs::home_field_data`; the
//! preference — WHEN the picture stops being the demonstration and becomes his — is a
//! constant in `home.js` and is deliberately not decided here. See "What this does not
//! decide" below.
//!
//! # The structure is NOT designed here
//!
//! It is `app/ui/home/field-prep.js`'s, exactly. `prepare(loro, REF, V)` reads
//! `loro.nodes[]` (`id, type, domain, cluster, degree, significance, activity, recency,
//! createdDay`), `loro.links[]` (`s, t, class, cross`), `loro.domains[]`
//! (`id, label, nodeCount, clusters[{id, label, nodeCount}]`), `loro.sources[]`
//! (`id, kind, domain, ingestedDay, primaryNodeId, derivedCount`) and
//! `loro.meta.horizonDays`; `field-engine.js` additionally reads `loro.bragSignals` and
//! `loro.specialists`. Every key emitted below is one of those. Nothing new is invented and
//! nothing consumed is left out.
//!
//! # NO CORPUS CONTENT CROSSES THIS BOUNDARY
//!
//! The rule `loro.rs` states for itself, applied to a payload that reaches a webview. A
//! record's title, its ref and its file path are the CEO's own words and file names, and the
//! field renders **none of them** — `field-engine.js:969-970` labels a node
//! `"<Type> · <Domain label>"` and nothing else, and a cluster label is never drawn at all.
//! So what leaves here is SHAPE: how many objects, of which kinds, in which company, drawn
//! out of how many distinct sources, and which of them came from the same source. Node,
//! cluster and source ids are opaque ordinals. The one identifying string in the payload is
//! the company label, which is already on the entity row directly above the picture.
//!
//! # WHAT THE READ SEAM CANNOT SUPPLY, said plainly rather than filled in
//!
//! Measured against the CEO's own corpus on 2026-09-06 (`/Users/alex/ab/richos-hq`, 713
//! records, `layout: repo`, 6 partitions):
//!
//!   * **There is no enumeration.** `loro-context.mjs` has three verbs — `compile`, `fetch`
//!     and `corpus`. `compile` is topic-ranked behind a relative relevance floor
//!     (`loro/lib/relevance.js`, `FLOOR_FRACTION: 0.15`), so it answers a QUERY; asking it
//!     for everything is not a thing it does, and `corpus` returns a census and no records.
//!     So the picture is the UNION of one query per company he has (see [`field_topics`]),
//!     and against his corpus that union is **168 objects of 713 records** — 6 compiles,
//!     2.795 s wall clock, measured 2026-09-06. `meta.query.topics` lists every question
//!     that was asked, and `meta.counts.records` beside `meta.counts.objects` puts the gap
//!     in front of the caller instead of hiding it.
//!   * **There are no dates.** A slice item carries `ref, kind, title, scope, kindInferred,
//!     company, score, confidence, provenance` — no observation date. So `createdDay` is 0
//!     against a `horizonDays` far beyond it, which makes `field-prep.js`'s
//!     `createdDay >= horizonDays - 14` false for every node: **nothing is drawn as new**,
//!     rather than everything being drawn as new off an invented date.
//!   * **There is no link graph.** Inter-record citations are not exposed. The links emitted
//!     here are the one real relation available — two objects compiled out of the same
//!     source file — and they are `class: "membership"`. No `cross: true` link is emitted at
//!     all, because a cross-domain relation would be a claim nothing here can support.
//!   * **There is no per-company census.** `corpus` counts the whole corpus by kind and by
//!     nothing else, so `domains[].nodeCount` counts the objects actually drawn and not his
//!     corpus's true per-company size.
//!   * **loro knows nothing about specialists, tasks or minutes.** `bragSignals` therefore
//!     carries real numbers for the things the corpus does know and **zero** for the ones it
//!     does not, and `meta.absent` names every one of them.
//!
//! # What this does not decide
//!
//! Whether the picture is good enough to replace the demonstration. It reports counts and
//! stops. The CEO's instruction for this work is explicit — *"The demo is definitely needed,
//! initially, for the user"* — and how a handover should look is the subject of
//! `richos-hq/design/mockups/rounds/round-11.4` (v1 First Light, v2 The Ghost, v3 Named,
//! v4 Arrival, v5 The Compass), which is unruled. `home.js`'s `HOME_FIELD_MIN_OBJECTS` is
//! the one place that preference lives.

use crate::entity::EntityRegistry;
use crate::loro::{LoroError, LoroRoot, LoroTools};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;

/// THE QUERY OF LAST RESORT — used only when the corpus names no partition and this install
/// has registered no company, so there is no name of his to ask about.
///
/// `compile` ranks against a topic and drops anything under 15% of its lane's top score, so
/// there is no "every record" request to make; every compile is a query. Which queries were
/// run is copied verbatim into `meta.query.topics`, so anyone wanting to know why an object
/// is not on the screen can re-run them by hand.
pub const FALLBACK_TOPIC: &str = "the whole of this company's memory: its decisions, commitments, people, \
     organizations, projects, lessons, principles, constraints, risks and strategy";

/// THE TOPICS TO ASK, TAKEN FROM HIS OWN NAMES RATHER THAN FROM A STRING TYPED HERE.
///
/// This is the difference between a picture of his memory and a picture of one sentence
/// somebody in this repository happened to write. Measured against the CEO's corpus, all with
/// `--company all --budget-chars 2000000 --max-items 5000`, on 2026-09-06:
///
/// | topic | items returned | considered |
/// |---|---|---|
/// | `"the whole of this company's memory: its decisions, commitments, …"` | 28 | 32 |
/// | `"everything this company knows"` | 63 | 89 |
/// | `"decision commitment lesson principle constraint strategy goal risk fact entity"` | 13 | 13 |
/// | `"richos"` — **the name of a partition the corpus actually has** | **127** | 326 |
///
/// A generic sentence cannot know that his corpus is about RichOS; the corpus's own partition
/// list does, and it is data he created rather than a guess. So the topics are the company ids
/// the census reports, plus this install's registered entity ids and display names, and the
/// invented sentence above is reached only when both of those are empty.
///
/// Deduplicated case-insensitively and sorted, so the same corpus asks the same questions in
/// the same order on every launch.
pub fn field_topics(census: &Census, registry: &EntityRegistry) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let s = s.trim();
        if s.is_empty() {
            return;
        }
        if !out.iter().any(|x: &String| x.eq_ignore_ascii_case(s)) {
            out.push(s.to_string());
        }
    };
    for c in &census.companies {
        push(c);
    }
    for e in registry.entities() {
        push(e.id.as_str());
        push(&e.display_name);
    }
    out.sort();
    if out.is_empty() {
        out.push(FALLBACK_TOPIC.to_string());
    }
    out
}

/// A budget far above anything one compile has ever spent. Measured against the CEO's corpus:
/// `usedChars` 34,324 of 4,000,000 on the broadest topic tried, `unspentChars` 3,965,676. The
/// cap exists so a runaway corpus cannot hand the webview an unbounded payload; it does not
/// select.
pub const FIELD_BUDGET_CHARS: usize = 2_000_000;

/// Likewise a ceiling and not a selector: 5,000 slots against a 713-record corpus.
pub const FIELD_MAX_ITEMS: usize = 5_000;

/// `horizonDays` is the field's day axis, and this corpus has no dates on it. Putting the
/// horizon far past every node's `createdDay` (which is 0) is what makes `field-prep.js`'s
/// `n.createdDay >= loro.meta.horizonDays - 14` **false for every node**, so no object is
/// drawn with the "new" flag on the strength of a date nobody supplied. Sources take
/// `ingestedDay = HORIZON_DAYS`, which is the one dated fact that IS true: the corpus holds
/// them now, at the fingerprint stamped in `meta`.
pub const HORIZON_DAYS: i64 = 40_000;

/// The lane every item with `company: null` belongs to. `CONTEXT-CONTRACT.md` §6c: the CEO
/// layer is "a legitimate permanent state, not an error", so it is a domain like any other.
pub const CEO_LANE: &str = "ceo";

/// LORO'S KINDS → THE FIELD'S TEN TYPES, stated as data so it can be read rather than
/// reverse-engineered, and copied into `meta.typeMap` so it is readable from the payload too.
///
/// `field-prep.js`'s `TYPES` is a closed list of ten and `field-engine.js:969` looks a node's
/// type up in `TYPE_WORD` — an unmapped type renders the literal word `undefined` on the
/// picture. So every kind loro can emit is mapped, and the residue goes to `memory`, which is
/// what the field's own vocabulary calls an ordinary remembered thing.
pub const TYPE_MAP: &[(&str, &str)] = &[
    ("decision", "decision"),
    ("commitment", "commitment"),
    ("lesson", "lesson"),
    ("entity", "organization"),
    ("goal", "initiative"),
    ("strategy", "initiative"),
    ("principle", "theme"),
    ("constraint", "theme"),
    ("preference", "theme"),
    ("fact", "memory"),
    ("metric", "memory"),
    ("risk", "memory"),
    ("event", "memory"),
    ("passage", "memory"),
];

/// The field type for a loro kind. An unmapped kind is `memory` — never `undefined`, which is
/// what `TYPE_WORD[...]` would otherwise put on the screen.
pub fn field_type(kind: &str) -> &'static str {
    TYPE_MAP.iter().find(|(k, _)| *k == kind).map(|(_, t)| *t).unwrap_or("memory")
}

// ---------------------------------------------------------------------------
// what the two loro verbs answer
// ---------------------------------------------------------------------------

/// `loro-context.mjs corpus --format json`, parsed down to the fields this consumer reads.
/// Unknown keys are ignored and every field defaults, which is `CONTEXT-CONTRACT.md` §2's
/// forward-compatibility rule — the same posture [`crate::loro::Slice`] takes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Census {
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub root_source: String,
    #[serde(default)]
    pub companies: Vec<String>,
    #[serde(default)]
    pub retired_companies: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub fingerprint: String,
    /// `total` plus one entry per kind. The whole corpus, which is the ONLY whole-corpus fact
    /// this seam exposes.
    #[serde(default)]
    pub counts: BTreeMap<String, u64>,
}

impl Census {
    pub fn record_count(&self) -> u64 {
        self.counts.get("total").copied().unwrap_or(0)
    }
}

/// One compiled item, parsed down to what SHAPE needs. `title` is deliberately absent from
/// this struct: it is the CEO's own words, nothing in the field renders it, and a field that
/// is never parsed cannot leak.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldItem {
    #[serde(default)]
    pub r#ref: String,
    #[serde(default)]
    pub kind: String,
    /// The item's lane; `None` is the CEO layer.
    #[serde(default)]
    pub company: Option<String>,
    /// Loro's own confidence in the record, `0.0..=1.0`. Used as the field's `significance`,
    /// which scales a decision's radius and its chance of carrying a bright fibre. It is a
    /// number loro states about the record — `score` is not, and the compiler says so in its
    /// own notes: scores are per-lane and NOT comparable across lanes.
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// `wiki` | `records` | `memory` | `entities`.
    #[serde(default)]
    pub source: Option<String>,
    /// The file the record was compiled out of. Used ONLY to group objects that came from the
    /// same place, and never emitted.
    #[serde(default)]
    pub path: Option<String>,
}

impl FieldItem {
    /// The lane this item belongs to, with the CEO layer named.
    pub fn lane(&self) -> &str {
        self.company.as_deref().unwrap_or(CEO_LANE)
    }

    /// The group this item belongs to: the file it was compiled out of, or its own ref when
    /// the compiler could not name one. Two objects with the same key genuinely came from the
    /// same source; nothing weaker is treated as a relation.
    pub fn group_key(&self) -> String {
        match self.provenance.path.as_deref() {
            Some(p) if !p.trim().is_empty() => p.trim().to_string(),
            _ => format!("ref:{}", self.r#ref),
        }
    }
}

/// The subset of a compile this module reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompiledSlice {
    #[serde(default)]
    schema_version: u64,
    #[serde(default)]
    items: Vec<FieldItem>,
}

// ---------------------------------------------------------------------------
// running the two verbs
// ---------------------------------------------------------------------------

/// Ask the corpus what it holds — `loro-context.mjs corpus --format json`.
///
/// Measured on the CEO's corpus, three consecutive runs: 0.07 s, 0.06 s, 0.06 s (`loro.rs`'s
/// own measurement of the same call).
pub fn probe_census(tools: &LoroTools, root: &LoroRoot) -> Result<Census, LoroError> {
    let (flag, path) = root.args();
    let out = Command::new(tools.node())
        .arg(tools.context_bin())
        .arg("corpus")
        .arg(flag)
        .arg(path)
        .arg("--format")
        .arg("json")
        .output()
        .map_err(|e| LoroError::LaneMap(format!("could not ask the corpus what it holds: {e}")))?;
    if !out.status.success() {
        let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(LoroError::LaneMap(format!(
            "loro corpus exited {code}: {}",
            why.lines().next().unwrap_or("").trim()
        )));
    }
    serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
        .map_err(|e| LoroError::LaneMap(format!("the corpus census did not parse: {e}")))
}

/// The argv for the one compile this module runs, exposed so a test can assert what it is
/// capable of asking for — in particular that it names the READ entry point, carries no write
/// verb, and narrows to `--company all` rather than to a single lane.
///
/// `--company all` is ONE subprocess for every partition (measured: 0.17 s warm against the
/// CEO's six-partition corpus, against 1.00 s for a single-lane compile cold). The compiler
/// always includes the CEO layer alongside whatever lanes are named, so there is no second
/// call for it.
pub fn field_argv(tools: &LoroTools, root: &LoroRoot) -> Vec<String> {
    let (root_flag, root_path) = root.args();
    vec![
        tools.context_bin().display().to_string(),
        "compile".into(),
        root_flag.into(),
        root_path.display().to_string(),
        // §1: the topic is natural language and "must not go through shell quoting" — stdin.
        "--topic-stdin".into(),
        "--company".into(),
        "all".into(),
        "--budget-chars".into(),
        FIELD_BUDGET_CHARS.to_string(),
        "--max-items".into(),
        FIELD_MAX_ITEMS.to_string(),
        "--audience".into(),
        "rich".into(),
        "--format".into(),
        "json".into(),
    ]
}

/// Run every topic and return the union of what they surfaced, in order.
///
/// **One failed topic does not lose the picture.** A compile that exits non-zero is skipped
/// and the rest are still asked; only a run in which EVERY topic failed is an error, and it
/// carries the first reason. The alternative — refusing the whole field because one query of
/// six could not be answered — would throw away objects that were successfully compiled.
///
/// Measured against the CEO's corpus (6 partitions, so 6 compiles): see the handoff for the
/// wall-clock figure. Each compile is a fresh `node` process; the field loads on an idle
/// callback with the demonstration already on screen, so this is never on the boot path.
pub fn collect_items(
    tools: &LoroTools,
    root: &LoroRoot,
    topics: &[String],
) -> Result<Vec<FieldItem>, LoroError> {
    let mut out: Vec<FieldItem> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut first_error: Option<LoroError> = None;
    let mut answered = 0usize;
    for topic in topics {
        match compile_one(tools, root, topic) {
            Ok(items) => {
                answered += 1;
                for it in items {
                    if seen.insert(it.r#ref.clone()) {
                        out.push(it);
                    }
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    if answered == 0 {
        return Err(first_error.unwrap_or_else(|| LoroError::LaneMap("there was no topic to ask".into())));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// the kept compile — so a big corpus is compiled once, not on every launch
// ---------------------------------------------------------------------------

/// The compile the last launch finished, kept on disk next to the app's other data.
///
/// WHY. Compiling a user's records is far slower than linear: measured on the test VM, 170 records
/// under 1 s, 1,000 records 39 s, 2,600 records 408 s, while the home screen waits 8 s. The compile
/// therefore finishes in the background (the screen draws his records the moment it does) and its
/// result is kept here, so the next launch draws them at once.
///
/// WHAT MAKES IT STALE. The corpus's own fingerprint (`loro-context corpus`, 0.06-0.07 s), which
/// changes when any record changes (`engine/loro/tests/run.js`, "the fingerprint CHANGES when a record
/// changes"), and the topics asked. A different fingerprint or topic list is a recompile of the whole
/// corpus; the loro compiler gives no per-record answer to refresh one record at a time. Items, not the
/// finished field, are kept: the field is `build_field`'s pure output and the company labels it uses
/// come from the registry of the day.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptCompile {
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub items: Vec<FieldItem>,
}

/// What [`items_kept_or_compiled`] did, for the log line and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemsFrom {
    Kept,
    Compiled,
}

/// The kept compile at `path`, or `None` for a missing or unreadable file (which is a recompile,
/// never an error: a cache is allowed to be absent).
pub fn read_kept(path: &std::path::Path) -> Option<KeptCompile> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Write `kept` to `path` atomically (a temp file in the same folder, then a rename), so a quit in
/// the middle never leaves half a file that the next launch would read as his records.
pub fn write_kept(path: &std::path::Path, kept: &KeptCompile) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(kept).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

/// The items for this census and topics: the kept ones when the corpus is byte-for-byte what they
/// were compiled from, otherwise whatever `compile` answers, which is then kept. A failed keep is not
/// a failed compile; the items are still returned.
///
/// An EMPTY fingerprint (a census that did not report one) is never a match: nothing is known about
/// what the kept items were compiled from.
pub fn items_kept_or_compiled(
    path: &std::path::Path,
    census: &Census,
    topics: &[String],
    compile: impl FnOnce() -> Result<Vec<FieldItem>, LoroError>,
) -> Result<(Vec<FieldItem>, ItemsFrom), LoroError> {
    if !census.fingerprint.is_empty() {
        if let Some(kept) = read_kept(path) {
            if kept.fingerprint == census.fingerprint && kept.topics == topics {
                return Ok((kept.items, ItemsFrom::Kept));
            }
        }
    }
    let items = compile()?;
    if !census.fingerprint.is_empty() {
        let kept = KeptCompile { fingerprint: census.fingerprint.clone(), topics: topics.to_vec(), items: items.clone() };
        if let Err(e) = write_kept(path, &kept) {
            eprintln!("[richos] home field: could not keep the compile ({e}); the next launch compiles again");
        }
    }
    Ok((items, ItemsFrom::Compiled))
}

/// One compile, one topic.
pub fn compile_one(tools: &LoroTools, root: &LoroRoot, topic: &str) -> Result<Vec<FieldItem>, LoroError> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(tools.node())
        .args(field_argv(tools, root))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| LoroError::LaneMap(format!("could not start the loro compiler: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        // A broken pipe here is not fatal on its own — the exit code below decides.
        let _ = stdin.write_all(topic.as_bytes());
    }
    let out = child
        .wait_with_output()
        .map_err(|e| LoroError::LaneMap(format!("the loro compiler did not complete: {e}")))?;
    if !out.status.success() {
        let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(LoroError::LaneMap(format!(
            "the loro compiler exited {code}: {}",
            why.lines().next().unwrap_or("").trim()
        )));
    }
    parse_items(&String::from_utf8_lossy(&out.stdout))
}

/// Parse and version-check one compiler stdout. Split out from [`collect_items`] so every
/// branch is testable without a corpus, a child process or a byte of the CEO's memory — the
/// same split `CliContextCompiler::interpret` makes, for the same reason.
pub fn parse_items(stdout: &str) -> Result<Vec<FieldItem>, LoroError> {
    let slice: CompiledSlice = serde_json::from_str(stdout)
        .map_err(|e| LoroError::LaneMap(format!("the loro slice did not parse: {e}")))?;
    if slice.schema_version != crate::loro::SUPPORTED_SLICE_SCHEMA {
        // §2: assert the version and treat anything else as unsupported rather than
        // mis-parsing it. A mis-parsed slice here does not fail loudly — it draws a picture of
        // a shape the corpus does not have and calls it the customer's.
        return Err(LoroError::LaneMap(format!(
            "slice schemaVersion {} is not supported (this build reads {})",
            slice.schema_version,
            crate::loro::SUPPORTED_SLICE_SCHEMA
        )));
    }
    Ok(slice.items)
}

// ---------------------------------------------------------------------------
// the pure half — a census and some items in, the field's own structure out
// ---------------------------------------------------------------------------

/// `grp-` plus 64 bits of FNV-1a over the lane and the group key (the file the records came from, or the
/// record's own ref). Stable across launches and across added records; see `build_field`.
fn stable_group_id(lane: &str, key: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in lane.bytes().chain(std::iter::once(0u8)).chain(key.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("grp-{h:016x}")
}

/// Build the dataset `home/field-*.js` consumes. **Pure**: no clock, no filesystem, no
/// process, no randomness — the same inputs give the same bytes, which is what makes the
/// whole of this file testable without the CEO's corpus.
///
/// `registry` supplies the company LABEL for a lane it recognizes. A lane with no registered
/// entity keeps its own id as its label rather than acquiring a prettier name by inference.
pub fn build_field(census: &Census, items: &[FieldItem], registry: &EntityRegistry) -> Value {
    // ---- de-duplicate by ref, deterministically ------------------------------------
    // A compile can return the same record in two lanes (the CEO layer plus a company's).
    // First occurrence wins, and the input order is the compiler's own ranked order.
    let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
    let mut kept: Vec<&FieldItem> = Vec::new();
    for it in items {
        if seen.insert(it.r#ref.as_str(), ()).is_none() {
            kept.push(it);
        }
    }

    // ---- lanes → domains, in the census's own order, CEO layer first ---------------
    let mut lanes: Vec<String> = Vec::new();
    if kept.iter().any(|i| i.lane() == CEO_LANE) {
        lanes.push(CEO_LANE.to_string());
    }
    for c in &census.companies {
        if c != CEO_LANE && kept.iter().any(|i| i.lane() == c) {
            lanes.push(c.clone());
        }
    }
    // A lane the census did not list but the compiler did produce is still drawn: refusing to
    // draw an object because a second call disagreed about the partition list would be the
    // compiler's answer overruled by a summary.
    for it in &kept {
        let l = it.lane().to_string();
        if !lanes.contains(&l) {
            lanes.push(l);
        }
    }

    let label_for = |lane: &str| -> String {
        if lane == CEO_LANE {
            return "You".to_string();
        }
        registry
            .entities()
            .iter()
            .find(|e| e.id.as_str() == lane)
            .map(|e| e.display_name.clone())
            .unwrap_or_else(|| lane.to_string())
    };

    // ---- groups: the objects compiled out of the SAME source file ------------------
    // The one real relation this seam exposes. Ordered by (lane, group key) so the ids are
    // deterministic, and the keys themselves never leave this function.
    let mut group_order: Vec<(String, String)> = Vec::new();
    for it in &kept {
        let k = (it.lane().to_string(), it.group_key());
        if !group_order.contains(&k) {
            group_order.push(k);
        }
    }
    group_order.sort();
    // A GROUP'S ID IS A HASH OF ITS OWN KEY, not its place in the sorted list: the picture lays a group
    // out from this id (`field-ghost.js` hosts it and seeds its random stream from it), so an ordinal
    // renumbered every group after a new source and moved records already on the screen (reviewer
    // fixture: 90 of 170 records moved more than 1 brain unit when record 171 was appended). The hash is
    // one-way, so the file's name still does not leave this function.
    let group_id: BTreeMap<(String, String), String> = group_order
        .iter()
        .map(|k| (k.clone(), stable_group_id(&k.0, &k.1)))
        .collect();

    // ---- nodes ---------------------------------------------------------------------
    // Ordered by (lane, group, ref) so a node's ordinal is a function of the corpus and not of
    // the order one compile happened to rank in.
    let mut ordered: Vec<&FieldItem> = kept.clone();
    ordered.sort_by(|a, b| {
        (a.lane(), a.group_key(), a.r#ref.as_str()).cmp(&(b.lane(), b.group_key(), b.r#ref.as_str()))
    });

    let node_id: BTreeMap<&str, String> = ordered
        .iter()
        .enumerate()
        .map(|(n, it)| (it.r#ref.as_str(), format!("obj-{:05}", n + 1)))
        .collect();

    // ---- links: a star per group, on its highest-confidence object ------------------
    // `class: "membership"`, because that is what it is — these objects came out of the same
    // page. No `cross: true` link is emitted anywhere: `field-prep.js` bends a cross link
    // further and gives it its own color, so one drawn here would be a visible claim about a
    // relation this seam cannot see.
    let mut primary: BTreeMap<String, &FieldItem> = BTreeMap::new();
    for it in &ordered {
        let key = group_id[&(it.lane().to_string(), it.group_key())].clone();
        primary
            .entry(key)
            .and_modify(|cur| {
                if it.confidence > cur.confidence || (it.confidence == cur.confidence && it.r#ref < cur.r#ref) {
                    *cur = it;
                }
            })
            .or_insert(it);
    }

    let mut links: Vec<Value> = Vec::new();
    let mut degree: BTreeMap<String, i64> = BTreeMap::new();
    for it in &ordered {
        let key = group_id[&(it.lane().to_string(), it.group_key())].clone();
        let hub = primary[&key];
        if hub.r#ref == it.r#ref {
            continue;
        }
        let s = node_id[hub.r#ref.as_str()].clone();
        let t = node_id[it.r#ref.as_str()].clone();
        *degree.entry(s.clone()).or_insert(0) += 1;
        *degree.entry(t.clone()).or_insert(0) += 1;
        links.push(json!({
            "s": s,
            "t": t,
            "kind": "same_source",
            "class": "membership",
            "weight": 1.0,
            "cross": false,
        }));
    }

    let nodes: Vec<Value> = ordered
        .iter()
        .map(|it| {
            let id = node_id[it.r#ref.as_str()].clone();
            json!({
                "id": id,
                // The field's own vocabulary — see TYPE_MAP, which is also in `meta.typeMap`.
                "type": field_type(&it.kind),
                // AND THE KIND LORO ACTUALLY SAID, kept beside it so the mapping above can be
                // audited from the payload rather than trusted.
                "loroKind": it.kind,
                "domain": it.lane(),
                "cluster": group_id[&(it.lane().to_string(), it.group_key())].clone(),
                "degree": degree.get(&id).copied().unwrap_or(0),
                "significance": it.confidence,
                // Unknown, and therefore zero rather than guessed. `activity` halves a node's
                // share of the river's packets; `recency` is carried and not drawn.
                "activity": 0.0,
                "recency": 0.0,
                // See HORIZON_DAYS: zero against a horizon of 40,000 is what keeps every node
                // out of `field-prep.js`'s "created in the last fortnight" flag.
                "createdDay": 0,
            })
        })
        .collect();

    // ---- domains and their clusters ------------------------------------------------
    let domains: Vec<Value> = lanes
        .iter()
        .map(|lane| {
            let mut clusters: Vec<Value> = Vec::new();
            for ((l, _), gid) in group_id.iter() {
                if l != lane {
                    continue;
                }
                let n = ordered
                    .iter()
                    .filter(|it| &group_id[&(it.lane().to_string(), it.group_key())] == gid)
                    .count();
                if n == 0 {
                    continue;
                }
                // The label is never drawn (`field-engine.js` labels domains and never
                // clusters), and a source file's name is the CEO's. An ordinal it is.
                clusters.push(json!({ "id": gid, "label": format!("Group {}", clusters.len() + 1), "nodeCount": n }));
            }
            let node_count = ordered.iter().filter(|it| it.lane() == lane).count();
            json!({ "id": lane, "label": label_for(lane), "nodeCount": node_count, "clusters": clusters })
        })
        .collect();

    // ---- sources: the river, one per distinct source file --------------------------
    // `kind: "document"` for every one of them, and that is a statement rather than a
    // placeholder: every loro source IS a document on disk — a wiki page, a record file, a
    // memory file. `field-engine.js`'s SOURCE_WORD maps `document` to "a document", so the
    // ticker reads "Learned from a document · <company> → N new memories", and N is the real
    // number of objects that came out of that file.
    let sources: Vec<Value> = group_id
        .iter()
        .filter_map(|((lane, _), gid)| {
            let hub = primary.get(gid)?;
            let derived = ordered
                .iter()
                .filter(|it| &group_id[&(it.lane().to_string(), it.group_key())] == gid)
                .count();
            Some(json!({
                "id": format!("src-{}", &gid[4..]),
                "kind": "document",
                "domain": lane,
                "day": HORIZON_DAYS,
                // The one dated fact that is true: the corpus holds it now, at the
                // fingerprint stamped in `meta`.
                "ingestedDay": HORIZON_DAYS,
                "primaryNodeId": node_id[hub.r#ref.as_str()].clone(),
                "derivedCount": derived,
            }))
        })
        .collect();

    // ---- the HUD's numbers: real where loro knows, zero where it does not -----------
    let count_of = |kind: &str| census.counts.get(kind).copied().unwrap_or(0);
    let brag = json!({
        "note": "Compiled from this install's own loro corpus. A zero is something loro does not know; see meta.absent.",
        // NOT IN LORO. Zero is what this corpus knows about them, and `meta.absent` says so.
        "specialistsManaged": 0,
        "specialistsActiveNow": 0,
        "tasksCompleted": 0,
        "tasksHandledWithoutCeo": 0,
        "ceoMinutesSaved": 0,
        "monthsWorkingTogether": 0,
        "correctionsLearnedFrom": 0,
        "conversationsHeld": 0,
        "activeInitiatives": 0,
        "commitmentsOpen": 0,
        "peopleKnown": 0,
        "organizationsKnown": 0,
        // REAL, from the census — the whole corpus, not the drawn subset.
        "sourcesUnderstood": sources.len(),
        "memoryObjects": census.record_count(),
        "decisionsRemembered": count_of("decision"),
        "relationshipsUnderstood": links.len(),
        "strategicThemes": count_of("strategy") + count_of("principle"),
        "commitmentsTracked": count_of("commitment"),
        "lessonsAccumulated": count_of("lesson"),
    });

    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    for it in &ordered {
        *by_type.entry(field_type(&it.kind).to_string()).or_insert(0) += 1;
    }

    json!({
        "meta": {
            "name": "RichOS company memory",
            "version": 1,
            // THE ONE FIELD THE BANNER READS. `home.js::fieldDataIsCustomers` takes the banner
            // down on `meta.synthetic === false` and on nothing else.
            "synthetic": false,
            "corpusRoot": census.root,
            "corpusLayout": census.layout,
            "corpusRootSource": census.root_source,
            "fingerprint": census.fingerprint,
            "horizonDays": HORIZON_DAYS,
            "counts": {
                // HIS CORPUS'S TRUE SIZE...
                "records": census.record_count(),
                // ...AND WHAT IS ACTUALLY DRAWN. The two are different numbers and the gap is
                // the caller's to judge — see the first line of `meta.absent`.
                "objects": ordered.len(),
                "links": links.len(),
                "sources": sources.len(),
                "domains": domains.len(),
                "byType": by_type,
                "byKind": census.counts,
            },
            "query": {
                // EVERY QUESTION THAT WAS ASKED, so the picture can be reproduced by hand.
                "topics": field_topics(census, registry),
                "company": "all",
                "budgetChars": FIELD_BUDGET_CHARS,
                "maxItems": FIELD_MAX_ITEMS,
                "audience": "rich",
            },
            "typeMap": TYPE_MAP.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>(),
            "absent": [
                "an enumeration: loro-context.mjs compiles a topic behind a relevance floor and has no verb that returns every record, so counts.objects is what one query surfaced of counts.records",
                "dates: a slice item carries no observation date, so every node's createdDay is 0 and nothing is drawn as new",
                "a link graph: inter-record citations are not exposed, so the only links here join objects compiled out of the same source file, and no cross-domain link is drawn",
                "a per-company census: the corpus verb counts the whole corpus by kind and by nothing else, so domains[].nodeCount counts drawn objects rather than the corpus's true share",
                "specialists, tasks and minutes: loro holds none of these, so those bragSignals are zero rather than estimated",
            ],
        },
        "bragSignals": brag,
        // Loro knows of no specialists. An empty list is the honest answer, and the "Working
        // now" aside simply carries nothing.
        "specialists": [],
        "domains": domains,
        "nodes": nodes,
        "links": links,
        "tasks": [],
        "sources": sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, EntityRegistry};

    fn item(r: &str, kind: &str, company: Option<&str>, path: &str, conf: f64) -> FieldItem {
        FieldItem {
            r#ref: r.into(),
            kind: kind.into(),
            company: company.map(str::to_string),
            confidence: conf,
            provenance: Provenance { source: Some("wiki".into()), path: Some(path.into()) },
        }
    }

    fn registry() -> EntityRegistry {
        EntityRegistry::new(vec![Entity::new("northwind", "Northwind Traders", &[]).unwrap()]).unwrap()
    }

    /// A loro checkout on disk with both entry points, which is what `LoroTools::locate`
    /// requires and the only way to build one — deliberately, so a test cannot assert about a
    /// tools directory the product would refuse.
    fn tools_dir() -> LoroTools {
        let dir = std::env::temp_dir().join(format!("home-field-tools-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("loro-context.mjs"), "//").unwrap();
        std::fs::write(dir.join("bin").join("loro-write.mjs"), "//").unwrap();
        LoroTools::locate(&dir).unwrap()
    }

    fn census() -> Census {
        Census {
            root: "/Users/you/RichOS/corpus".into(),
            layout: "corpus".into(),
            root_source: "--corpus".into(),
            companies: vec!["northwind".into(), "kestrel".into()],
            retired_companies: vec![],
            sources: vec!["records".into(), "wiki".into()],
            fingerprint: "sha256:deadbeef".into(),
            counts: BTreeMap::from([
                ("total".to_string(), 400u64),
                ("decision".to_string(), 30),
                ("lesson".to_string(), 4),
                ("commitment".to_string(), 2),
                ("strategy".to_string(), 5),
                ("principle".to_string(), 6),
            ]),
        }
    }

    #[test]
    fn a_kept_compile_is_used_while_the_corpus_is_unchanged_and_dropped_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("home-field-kept-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("home-field-cache.json");
        std::fs::remove_file(&path).ok();
        let topics = vec!["northwind".to_string()];
        let c = census();
        let mut calls = 0;
        let first = items_kept_or_compiled(&path, &c, &topics, || {
            calls += 1;
            Ok(vec![item("a", "decision", Some("northwind"), "p", 0.9)])
        })
        .unwrap();
        assert_eq!((first.0.len(), first.1, calls), (1, ItemsFrom::Compiled, 1));

        // the next launch: same corpus, nothing compiled, the same items
        let second = items_kept_or_compiled(&path, &c, &topics, || panic!("compiled a corpus that had not changed")).unwrap();
        assert_eq!((second.0.len(), second.1, second.0[0].r#ref.as_str()), (1, ItemsFrom::Kept, "a"));

        // a record changed: the fingerprint moves, so it is compiled again and the new items are kept
        let mut changed = census();
        changed.fingerprint = "sha256:cafe".into();
        let third = items_kept_or_compiled(&path, &changed, &topics, || Ok(vec![item("b", "fact", None, "q", 0.5), item("c", "fact", None, "r", 0.5)])).unwrap();
        assert_eq!((third.0.len(), third.1), (2, ItemsFrom::Compiled));
        let fourth = items_kept_or_compiled(&path, &changed, &topics, || panic!("not kept")).unwrap();
        assert_eq!(fourth.0.len(), 2);

        // other topics are another question; a census with no fingerprint is never a match; a torn file is a recompile
        assert_eq!(items_kept_or_compiled(&path, &changed, &["x".to_string()], || Ok(vec![])).unwrap().1, ItemsFrom::Compiled);
        let mut none = census();
        none.fingerprint = String::new();
        assert_eq!(items_kept_or_compiled(&path, &none, &topics, || Ok(vec![])).unwrap().1, ItemsFrom::Compiled);
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(items_kept_or_compiled(&path, &c, &topics, || Ok(vec![])).unwrap().1, ItemsFrom::Compiled);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_groups_id_does_not_change_when_another_source_is_added() {
        let a = item("a", "decision", Some("northwind"), "p/b.md", 0.9);
        let b = item("b", "fact", Some("northwind"), "p/d.md", 0.9);
        let ids = |items: &[FieldItem]| -> Vec<String> {
            let f = build_field(&census(), items, &registry());
            f["nodes"].as_array().unwrap().iter().map(|n| n["cluster"].as_str().unwrap().to_string()).collect()
        };
        let before = ids(&[a.clone(), b.clone()]);
        // a source that sorts BETWEEN the two, which an ordinal would have renumbered
        let after = ids(&[a.clone(), item("c", "fact", Some("northwind"), "p/c.md", 0.9), b.clone()]);
        assert!(after.contains(&before[0]) && after.contains(&before[1]), "an existing group changed id: {before:?} vs {after:?}");
        assert_ne!(before[0], before[1]);
    }

    #[test]
    fn every_loro_kind_maps_into_the_fields_ten_types() {
        // `field-prep.js`'s TYPES, verbatim. An unmapped type reaches `TYPE_WORD[...]` at
        // field-engine.js:969 and renders the literal word `undefined` on the picture.
        const TYPES: &[&str] = &[
            "memory",
            "conversation",
            "decision",
            "commitment",
            "lesson",
            "person",
            "organization",
            "project",
            "initiative",
            "theme",
        ];
        for (kind, mapped) in TYPE_MAP {
            assert!(TYPES.contains(mapped), "{kind} maps to {mapped}, which is not one of the field's ten types");
        }
        // And the residue, which is what a kind added to loro tomorrow will take.
        assert_eq!(field_type("a-kind-loro-does-not-have-yet"), "memory");
        assert!(TYPES.contains(&field_type("")));
    }

    #[test]
    fn the_banner_comes_down_only_on_a_positive_false() {
        let f = build_field(&census(), &[item("a", "decision", Some("northwind"), "p/1.md", 0.9)], &registry());
        assert_eq!(f["meta"]["synthetic"], json!(false));
        assert_eq!(f["meta"]["corpusRoot"], json!("/Users/you/RichOS/corpus"));
        assert_eq!(f["meta"]["fingerprint"], json!("sha256:deadbeef"));
    }

    #[test]
    fn counts_report_the_corpus_and_the_drawing_separately() {
        // The whole point of the pair: 400 records in his corpus, 3 objects on the screen. A
        // caller that could only see one of those numbers could not judge density at all.
        let items = vec![
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
            item("b", "passage", Some("northwind"), "p/1.md", 0.8),
            item("c", "lesson", None, "p/2.md", 0.7),
        ];
        let f = build_field(&census(), &items, &registry());
        assert_eq!(f["meta"]["counts"]["records"], json!(400));
        assert_eq!(f["meta"]["counts"]["objects"], json!(3));
        assert_eq!(f["nodes"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn objects_from_the_same_file_are_joined_and_nothing_else_is() {
        let items = vec![
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
            item("b", "passage", Some("northwind"), "p/1.md", 0.8),
            item("c", "passage", Some("northwind"), "p/2.md", 0.8),
        ];
        let f = build_field(&census(), &items, &registry());
        let links = f["links"].as_array().unwrap();
        // Two objects out of `p/1.md` -> one link. `p/2.md` holds one object -> none.
        assert_eq!(links.len(), 1);
        assert_eq!(links[0]["class"], json!("membership"));
        // The star sits on the higher-confidence object.
        let nodes = f["nodes"].as_array().unwrap();
        let hub = nodes.iter().find(|n| n["id"] == links[0]["s"]).unwrap();
        assert_eq!(hub["significance"], json!(0.9));
        assert_eq!(hub["degree"], json!(1));
        // NO CROSS-DOMAIN LINK IS EVER EMITTED.
        assert!(links.iter().all(|l| l["cross"] == json!(false)));
    }

    #[test]
    fn nothing_is_drawn_as_new_when_no_date_was_supplied() {
        // `field-prep.js`: `if (n.createdDay >= loro.meta.horizonDays - 14) f |= 4;` — the
        // flag that pulses a node as recently created. Re-derived here rather than trusted:
        // 0 >= 40000 - 14 is false, for every node, at every corpus size.
        let f = build_field(&census(), &[item("a", "decision", None, "p/1.md", 0.9)], &registry());
        let horizon = f["meta"]["horizonDays"].as_i64().unwrap();
        for n in f["nodes"].as_array().unwrap() {
            assert!(n["createdDay"].as_i64().unwrap() < horizon - 14, "a node with no date was drawn as new");
        }
        // And the river's sources ARE current, which is the one dated fact that is true:
        // `field-engine.js:657` keeps a source when `ingestedDay >= horizonDays - 21`.
        for s in f["sources"].as_array().unwrap() {
            assert!(s["ingestedDay"].as_i64().unwrap() >= horizon - 21, "a source the corpus holds now was drawn as stale");
        }
    }

    #[test]
    fn no_corpus_content_reaches_the_payload() {
        // The refs, titles and file paths are the CEO's own words and file names, and the
        // field renders none of them. This asserts they are not merely unrendered but ABSENT.
        let items = vec![
            item("wiki:the-acme-acquisition.md#price", "decision", Some("northwind"), "wiki/the-acme-acquisition.md", 0.9),
            item("mem:secret-ref", "passage", None, "ceo/records/private.md", 0.8),
        ];
        let payload = serde_json::to_string(&build_field(&census(), &items, &registry())).unwrap();
        for leak in ["acme", "acquisition", "secret-ref", "private.md", "wiki/the-"] {
            assert!(!payload.contains(leak), "the payload carries corpus content: {leak}");
        }
        // What DOES cross is the shape, plus the company label that is already on the entity
        // row above the picture.
        assert!(payload.contains("Northwind Traders"));
        assert!(payload.contains("obj-00001"));
    }

    #[test]
    fn a_registered_company_gets_its_name_and_an_unregistered_lane_keeps_its_id() {
        let items = vec![
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
            item("b", "decision", Some("kestrel"), "p/2.md", 0.9),
            item("c", "decision", None, "p/3.md", 0.9),
        ];
        let f = build_field(&census(), &items, &registry());
        let domains = f["domains"].as_array().unwrap();
        // CEO layer first, then the census's own order.
        assert_eq!(domains[0]["id"], json!("ceo"));
        assert_eq!(domains[0]["label"], json!("You"));
        assert_eq!(domains[1]["id"], json!("northwind"));
        assert_eq!(domains[1]["label"], json!("Northwind Traders"));
        // Not registered here: it keeps its own id rather than acquiring a prettier name.
        assert_eq!(domains[2]["id"], json!("kestrel"));
        assert_eq!(domains[2]["label"], json!("kestrel"));
    }

    #[test]
    fn the_field_carries_every_key_field_prep_consumes() {
        let items = vec![
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
            item("b", "passage", Some("northwind"), "p/1.md", 0.8),
        ];
        let f = build_field(&census(), &items, &registry());
        for k in ["meta", "domains", "nodes", "links", "sources", "bragSignals", "specialists", "tasks"] {
            assert!(f.get(k).is_some(), "the dataset is missing `{k}`");
        }
        for n in f["nodes"].as_array().unwrap() {
            for k in ["id", "type", "domain", "cluster", "degree", "significance", "activity", "recency", "createdDay"] {
                assert!(n.get(k).is_some(), "a node is missing `{k}`, which field-prep.js reads");
            }
        }
        for d in f["domains"].as_array().unwrap() {
            for k in ["id", "label", "nodeCount", "clusters"] {
                assert!(d.get(k).is_some(), "a domain is missing `{k}`");
            }
            for c in d["clusters"].as_array().unwrap() {
                for k in ["id", "label", "nodeCount"] {
                    assert!(c.get(k).is_some(), "a cluster is missing `{k}`");
                }
            }
        }
        for s in f["sources"].as_array().unwrap() {
            for k in ["id", "kind", "domain", "ingestedDay", "primaryNodeId", "derivedCount"] {
                assert!(s.get(k).is_some(), "a source is missing `{k}`");
            }
        }
        // Every source names a node that exists — `field-prep.js` filters on
        // `index.has(s.primaryNodeId)` and would silently drop a river that named nothing.
        let ids: Vec<&str> = f["nodes"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap()).collect();
        for s in f["sources"].as_array().unwrap() {
            assert!(ids.contains(&s["primaryNodeId"].as_str().unwrap()));
        }
        // And every link names two.
        for l in f["links"].as_array().unwrap() {
            assert!(ids.contains(&l["s"].as_str().unwrap()) && ids.contains(&l["t"].as_str().unwrap()));
        }
    }

    #[test]
    fn a_domains_node_count_is_its_own_nodes() {
        let items = vec![
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
            item("b", "passage", Some("northwind"), "p/2.md", 0.8),
            item("c", "lesson", None, "p/3.md", 0.7),
        ];
        let f = build_field(&census(), &items, &registry());
        let total: u64 = f["domains"].as_array().unwrap().iter().map(|d| d["nodeCount"].as_u64().unwrap()).sum();
        assert_eq!(total, f["meta"]["counts"]["objects"].as_u64().unwrap());
        for d in f["domains"].as_array().unwrap() {
            let c: u64 = d["clusters"].as_array().unwrap().iter().map(|c| c["nodeCount"].as_u64().unwrap()).sum();
            assert_eq!(c, d["nodeCount"].as_u64().unwrap(), "a domain's clusters do not add up to it");
        }
    }

    #[test]
    fn the_same_record_in_two_lanes_is_drawn_once() {
        let items = vec![
            item("a", "decision", None, "p/1.md", 0.9),
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
        ];
        let f = build_field(&census(), &items, &registry());
        assert_eq!(f["meta"]["counts"]["objects"], json!(1));
    }

    #[test]
    fn the_same_inputs_give_the_same_bytes() {
        let items = vec![
            item("b", "passage", Some("northwind"), "p/2.md", 0.8),
            item("a", "decision", Some("northwind"), "p/1.md", 0.9),
            item("c", "lesson", None, "p/3.md", 0.7),
        ];
        let one = serde_json::to_string(&build_field(&census(), &items, &registry())).unwrap();
        let two = serde_json::to_string(&build_field(&census(), &items, &registry())).unwrap();
        assert_eq!(one, two);
    }

    #[test]
    fn the_questions_are_his_own_names_and_the_invented_one_is_the_last_resort() {
        // His partition list, plus this install's registered companies. Sorted, so the same
        // corpus asks the same questions in the same order on every launch.
        let topics = field_topics(&census(), &registry());
        assert_eq!(topics, vec!["Northwind Traders", "kestrel", "northwind"]);

        // A corpus with no partitions on an install that has registered nothing has no name
        // of his to ask about, and only then is the sentence in this file used.
        let bare = Census { companies: vec![], ..Default::default() };
        assert_eq!(field_topics(&bare, &EntityRegistry::new(vec![]).unwrap()), vec![FALLBACK_TOPIC]);

        // ...and it stops being used the moment there IS a name.
        let topics = field_topics(&bare, &registry());
        assert!(!topics.contains(&FALLBACK_TOPIC.to_string()), "an invented topic outlived the names it stands in for");
        assert_eq!(topics, vec!["Northwind Traders", "northwind"]);
    }

    #[test]
    fn the_argv_reads_and_cannot_write() {
        let tools = tools_dir();
        let root = LoroRoot::Root("/corpus".into());
        let argv = field_argv(&tools, &root);
        assert!(argv[0].ends_with("loro-context.mjs"), "the argv does not name the READ entry point: {argv:?}");
        assert_eq!(argv[1], "compile");
        for verb in ["write", "loro-write.mjs", "put", "set", "delete", "confirm"] {
            assert!(!argv.iter().any(|a| a == verb), "the argv carries a write verb: {verb}");
        }
        // ONE subprocess for every partition, and the CEO layer comes with it.
        let i = argv.iter().position(|a| a == "--company").expect("the compile does not narrow");
        assert_eq!(argv[i + 1], "all");
        // The topic goes over stdin, never as a flag — `CONTEXT-CONTRACT.md` §1.
        assert!(argv.iter().any(|a| a == "--topic-stdin"));
        assert!(!argv.iter().any(|a| a == "--topic"));
    }

    #[test]
    fn an_unsupported_schema_is_refused_rather_than_mis_parsed() {
        let bad = r#"{"schemaVersion":2,"items":[]}"#;
        assert!(parse_items(bad).is_err());
        let good = r#"{"schemaVersion":1,"items":[{"ref":"a","kind":"decision","company":null,"confidence":0.8,"provenance":{"source":"wiki","path":"p/1.md"}}]}"#;
        assert_eq!(parse_items(good).unwrap().len(), 1);
        // A `null` path is the compiler's own shape for a record it cannot place, and it must
        // not be a parse failure — `provenance.link` is `null` on most real items.
        let nulls = r#"{"schemaVersion":1,"items":[{"ref":"a","kind":"fact","provenance":{"source":null,"path":null}}]}"#;
        let items = parse_items(nulls).unwrap();
        assert_eq!(items[0].group_key(), "ref:a");
    }

    /// THE ONE TEST THAT TOUCHES A REAL CORPUS, and therefore the one that is `#[ignore]`d:
    /// a corpus is the CEO's own record, lives outside every repository, and a clean checkout
    /// on a runner has none. Run it by hand against one:
    ///
    /// ```text
    /// RICHOS_LORO_DIR=<checkout>/loro LORO_ROOT=<checkout> \
    ///   cargo test -p richos-core --lib home_field -- --ignored --nocapture
    /// ```
    ///
    /// It prints the counts rather than asserting a number, because the numbers belong to
    /// whichever corpus it was pointed at. What it DOES assert is the invariant that holds
    /// for every corpus: the payload says it is not synthetic, it names the root it came
    /// from, and it never claims to have drawn more objects than the corpus holds records.
    #[test]
    #[ignore]
    fn against_a_real_corpus_when_one_is_configured() {
        let (Ok(dir), Ok(root)) = (std::env::var("RICHOS_LORO_DIR"), std::env::var("LORO_ROOT")) else {
            eprintln!("no corpus configured — set RICHOS_LORO_DIR and LORO_ROOT");
            return;
        };
        let tools = LoroTools::locate(&dir).expect("a loro checkout at RICHOS_LORO_DIR");
        let root = LoroRoot::Root(root.into());
        let reg = EntityRegistry::new(vec![]).unwrap();
        let census = probe_census(&tools, &root).expect("the corpus census");
        let topics = field_topics(&census, &reg);
        let started = std::time::Instant::now();
        let items = collect_items(&tools, &root, &topics).expect("at least one topic answered");
        let elapsed = started.elapsed();
        let f = build_field(&census, &items, &reg);
        eprintln!("topics={topics:?} in {elapsed:?}");
        eprintln!(
            "records={} objects={} links={} sources={} domains={} fingerprint={}",
            f["meta"]["counts"]["records"],
            f["meta"]["counts"]["objects"],
            f["meta"]["counts"]["links"],
            f["meta"]["counts"]["sources"],
            f["meta"]["counts"]["domains"],
            f["meta"]["fingerprint"],
        );
        assert_eq!(f["meta"]["synthetic"], json!(false));
        assert!(!f["meta"]["corpusRoot"].as_str().unwrap_or("").is_empty(), "the payload does not name the corpus it came from");
        assert!(
            f["meta"]["counts"]["objects"].as_u64().unwrap() <= f["meta"]["counts"]["records"].as_u64().unwrap(),
            "more objects were drawn than the corpus holds records"
        );
    }

    #[test]
    fn an_empty_compile_is_an_empty_field_and_not_a_panic() {
        let f = build_field(&census(), &[], &registry());
        assert_eq!(f["meta"]["counts"]["objects"], json!(0));
        assert_eq!(f["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(f["domains"].as_array().unwrap().len(), 0);
        // And it still says whose corpus produced nothing, which is what lets a caller tell
        // "your corpus is empty" from "there is no corpus".
        assert_eq!(f["meta"]["counts"]["records"], json!(400));
    }
}
