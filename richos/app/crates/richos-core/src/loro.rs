//! THE READ SEAM, WIRED — company memory reaching a re-prime, and the guard on the way in.
//!
//! `reprime.rs` has carried a `LoroContextCompiler` trait since the continuity foundation
//! landed, and nothing ever implemented it. The compiler itself has been complete and
//! versioned the whole time, one directory away, behind `loro/CONTEXT-CONTRACT.md`. So
//! every re-prime asserted a fresh Rich into existence with **no company memory at all**
//! while the thing that would have supplied it sat there callable. This module is the
//! implementation, and it is the only place in richos-core that knows loro exists.
//!
//! # What crosses the boundary, and what must not
//!
//! `richos` GOES PUBLIC. Loro's content is the CEO's second brain and is private by
//! construction — it is not in this repo and never will be. Everything here is *mechanism*:
//! how to invoke a binary that lives elsewhere, how to parse a documented JSON shape, and
//! what to refuse. **No corpus content, no corrections, no speech.** The corpus root is
//! read from the environment at runtime and is never defaulted — `CONTEXT-CONTRACT.md` §1
//! is explicit that a default root means "a customer's Rich silently answering out of the
//! VENDOR's company memory, and exiting 0 either way", which it correctly calls a larger
//! failure than an error, not a smaller one.
//!
//! # The read-only invariant is not weakened, and could not be from here
//!
//! Loro's compiler is read-only by structural proof: `assertNoSideEffects` scans every
//! module under `loro/lib/**` for a filesystem write (`privacy.js`), which is why a
//! persistent index cannot live in the compiler at all. Nothing in this file can change
//! that — it invokes `bin/loro-context.mjs` as a child process and reads stdout. What it
//! CAN do is smuggle a write in through the same door, so it doesn't: [`CliContextCompiler`]
//! holds no writer, cannot name one, and a test asserts its argv contains no write verb.
//! Writing is a different type reaching a different binary (`correction.rs`), and it only
//! ever runs after the CEO has said yes.

use crate::reprime::{CompiledTier, LoroContextCompiler, LoroTier, SliceCoverage, SliceRequest};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The slice schema this build understands. `CONTEXT-CONTRACT.md` §2's forward-compat rule:
/// fields may be ADDED within a version and unknown ones must be ignored (which
/// `#[serde(default)]` + serde's default of ignoring unknown keys does); a REMOVED or
/// retyped field bumps this, and *"a consumer that cares should assert
/// `schemaVersion === 1` and treat anything higher as unsupported — degrade to no slice —
/// rather than mis-parsing it."* This consumer cares.
pub const SUPPORTED_SLICE_SCHEMA: u64 = 1;

/// The memory-scope wall this consumer reads at (`CONTEXT-CONTRACT.md` §4). `rich` is the
/// re-prime's audience by name in the contract's own table — *"Rich is the CEO's own
/// executive function"* — and it is a constant rather than a setting because widening it is
/// a privacy decision and narrowing it silently would hide the CEO's own memory from him.
pub const REPRIME_AUDIENCE: &str = "rich";

#[derive(Debug, thiserror::Error)]
pub enum LoroError {
    #[error("loro tools not found: {0}")]
    ToolsNotFound(String),
    #[error("loro corpus root not configured: {0}")]
    NoRoot(String),
    #[error("loro lane map: {0}")]
    LaneMap(String),
    /// A CORPUS RESOLVED AND THE PROGRAM THAT READS IT DID NOT.
    ///
    /// Split out of [`Self::ToolsNotFound`] on 2026-09-01 because first-run provisioning
    /// makes it an ordinary, expected state rather than a misconfiguration: the corpus is
    /// created by the product, and the compiler — 15 node files — ships from nowhere yet
    /// (`BLOCKED.md`). "This install has no corpus", "this install has a corpus it cannot
    /// read", and "this install is misconfigured" are three different sentences, and a
    /// caller that cannot tell them apart writes the wrong one on the boot line.
    #[error("loro corpus at {root} — but the memory compiler is not installed. Looked in: {tried}. \
             RICHOS_LORO_DIR names it explicitly.")]
    CompilerNotInstalled { root: String, tried: String },
}

// ---------------------------------------------------------------------------
// configuration — all of it explicit, none of it inferred
// ---------------------------------------------------------------------------

/// Where the CEO's memory is. `CONTEXT-CONTRACT.md` §1: *"There is no fallback."*
///
/// Two shapes, and they are not interchangeable. A provisioned [`LoroRoot::Corpus`] is
/// `ceo/` + `companies/<id>/` and is the real thing. A [`LoroRoot::Root`] is the in-repo
/// dogfood layout (`wiki/` + `loro/`) and the slice it produces SAYS SO, carrying
/// `corpus.layout: "repo"` and a note naming it as RichOS's own memory — because, as the
/// contract puts it, the wrong company's memory is byte-honest too and only provenance
/// distinguishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoroRoot {
    Corpus(PathBuf),
    Root(PathBuf),
}

impl LoroRoot {
    /// Resolve from the environment in the contract's own precedence order: `LORO_CORPUS`,
    /// then `LORO_ROOT`. `None` is a legitimate, common state — an install with no corpus —
    /// and it produces [`LoroTier::NotWired`], never a guess.
    pub fn from_env() -> Option<Self> {
        if let Ok(v) = std::env::var("LORO_CORPUS") {
            let v = v.trim();
            if !v.is_empty() {
                return Some(LoroRoot::Corpus(PathBuf::from(v)));
            }
        }
        if let Ok(v) = std::env::var("LORO_ROOT") {
            let v = v.trim();
            if !v.is_empty() {
                return Some(LoroRoot::Root(PathBuf::from(v)));
            }
        }
        None
    }

    /// The two CLI arguments naming this root.
    pub fn args(&self) -> (&'static str, &Path) {
        match self {
            LoroRoot::Corpus(p) => ("--corpus", p.as_path()),
            LoroRoot::Root(p) => ("--root", p.as_path()),
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            LoroRoot::Corpus(p) | LoroRoot::Root(p) => p.as_path(),
        }
    }
}

/// The loro checkout holding `bin/loro-context.mjs` and `bin/loro-write.mjs`.
///
/// **Deliberately not inferred from this checkout.** richos has no `loro/` directory and
/// never has — the vocabulary and the corpus are the CEO's and live outside a repo that
/// goes public, which is exactly the trap the transcription E2E fell into on 2026-08-29
/// (open-items 3.3e: a precondition a clean checkout can never satisfy). `RICHOS_LORO_DIR`
/// names it, or nothing does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoroTools {
    dir: PathBuf,
    node: String,
}

impl LoroTools {
    /// Verify that `dir` really is a loro checkout — BOTH entry points present. A directory
    /// that exists but holds neither is the failure that would otherwise surface as a
    /// non-zero exit on every compile, once per rotation, forever.
    pub fn locate(dir: impl AsRef<Path>) -> Result<Self, LoroError> {
        let dir = dir.as_ref().to_path_buf();
        let ctx = dir.join("bin").join("loro-context.mjs");
        let wri = dir.join("bin").join("loro-write.mjs");
        if !ctx.is_file() {
            return Err(LoroError::ToolsNotFound(format!("{} is not a file", ctx.display())));
        }
        if !wri.is_file() {
            return Err(LoroError::ToolsNotFound(format!("{} is not a file", wri.display())));
        }
        let node = std::env::var("RICHOS_NODE_BIN").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "node".into());
        Ok(LoroTools { dir, node })
    }

    pub fn from_env() -> Option<Result<Self, LoroError>> {
        let v = std::env::var("RICHOS_LORO_DIR").ok()?;
        let v = v.trim().to_string();
        if v.is_empty() {
            return None;
        }
        Some(LoroTools::locate(v))
    }

    pub fn context_bin(&self) -> PathBuf {
        self.dir.join("bin").join("loro-context.mjs")
    }

    pub fn write_bin(&self) -> PathBuf {
        self.dir.join("bin").join("loro-write.mjs")
    }

    pub fn node(&self) -> &str {
        &self.node
    }

    /// Override which `node` runs the compiler.
    ///
    /// `locate` needs this because a GUI launch's `PATH` is `/usr/bin:/bin:/usr/sbin:/sbin`
    /// and the bare name resolved by [`Self::locate`] would not be found there — see
    /// [`resolve_node_bin`], which does the finding.
    pub fn set_node(&mut self, node: String) {
        self.node = node;
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// ENTITY → loro company lane, stated by an operator and never guessed.
///
/// # Why this is a map and not `entity_id == company_id`
///
/// Because name equality is an inference, and this is the one place an inference costs the
/// most. The re-prime's own identity assertion tells the successor *"do not assume, infer or
/// carry over anything from another entity area, however related a name looks"*; a compiler
/// that binds an entity to a loro partition because the two strings match is doing precisely
/// the thing the sentence forbids, one layer down, in the payload that carries the sentence.
///
/// # It was empty by default until 2026-09-01, and why it no longer is
///
/// `loro-structure.md`'s "one loro, two homes" — the CEO layer plus N company partitions —
/// was READY-FOR-CEO and unratified (open-items 1.6/3.5). Shipping a hard-coded
/// entity-is-a-company rule would have ratified it in code while the register still said
/// OPEN, so the map was empty by default and the seam simply refused to narrow.
///
/// **He ratified it on 2026-09-01** (`wiki/ceo-decisions.md` §5: *"The proposals there make
/// sense… The only thing I'd change is replace `person/` with `ceo/`"*), so the default
/// stopped being empty and became one lane per registered entity, named after it —
/// [`LaneMap::same_name_lanes`].
///
/// # Where the enumeration comes from, since 2026-09-04
///
/// It used to be built from `EntityRegistry::CEOS_COMPANIES`, a `const` table of one man's
/// six companies. The registry is now per-user (`entity.rs` rule 4), so the enumeration is
/// built from THE REGISTRY IN FORCE and the default is empty on an install that has
/// registered nothing. It is still an enumeration rather than an inference: the map holds
/// exactly one pair per registered entity, `lane_for` answers `None` for anything else, and
/// no unregistered entity acquires a lane by looking like a registered one. What changed is
/// whose list it enumerates — and it is the same list `provision` creates the partitions
/// from, so the two halves cannot disagree.
///
/// # And the map is reconciled against the corpus before anything is sent
///
/// A mapping is a claim that a partition exists, and loro refuses a lane it does not have
/// — `exit 2: no such company partition "femcboost" in this corpus. Known: (none).` The
/// CEO's corpus today has **zero** partitions, so a map that were merely filled in would
/// turn every re-prime into `LoroTier::Unavailable`. See
/// [`CliContextCompiler::reconcile_lanes`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaneMap(BTreeMap<String, String>);

impl LaneMap {
    /// Parse `entity=lane,entity=lane`. An entry with an empty half is a usage error, not a
    /// silently-dropped mapping — a typo that quietly maps nothing would look exactly like
    /// a working configuration until the day it mattered.
    pub fn parse(spec: &str) -> Result<Self, LoroError> {
        let mut map = BTreeMap::new();
        for pair in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let Some((entity, lane)) = pair.split_once('=') else {
                return Err(LoroError::LaneMap(format!("{pair:?} is not entity=lane")));
            };
            let (entity, lane) = (entity.trim(), lane.trim());
            if entity.is_empty() || lane.is_empty() {
                return Err(LoroError::LaneMap(format!("{pair:?} has an empty half")));
            }
            if map.insert(entity.to_string(), lane.to_string()).is_some() {
                return Err(LoroError::LaneMap(format!("entity {entity:?} is mapped twice")));
            }
        }
        Ok(LaneMap(map))
    }

    /// The ratified default: **each entity registered on THIS install**, mapped to a loro
    /// company lane of the same name (`wiki/ceo-decisions.md` §5).
    ///
    /// **Same-name is still a STATED pair, not an inferred rule**, and the statement is the
    /// registration: somebody put that company in `entities.json` (or answered the picker),
    /// and `provision` creates the corpus partition from the same list. The map holds one
    /// pair per registered entity and `lane_for` answers `None` for anything unregistered,
    /// however much it looks like a registered one.
    ///
    /// An install that has registered nothing gets an EMPTY map, which is the same map the
    /// operator gets from `RICHOS_LORO_LANES=` — no narrowing, and no claim that a partition
    /// exists.
    ///
    /// It is also inert until the corpus is partitioned — see
    /// [`CliContextCompiler::reconcile_lanes`].
    pub fn same_name_lanes(registry: &crate::entity::EntityRegistry) -> Self {
        LaneMap(registry.entities().iter().map(|e| (e.id.to_string(), e.id.to_string())).collect())
    }

    /// `RICHOS_LORO_LANES` when the operator sets it; [`Self::same_name_lanes`] otherwise.
    ///
    /// An explicit setting still wins outright — including an explicitly EMPTY one, which
    /// is how an operator turns lane narrowing off entirely without editing the binary.
    pub fn from_env(registry: &crate::entity::EntityRegistry) -> Result<Self, LoroError> {
        match std::env::var("RICHOS_LORO_LANES") {
            Ok(v) if !v.trim().is_empty() => LaneMap::parse(&v),
            Ok(_) => Ok(LaneMap::default()),
            Err(_) => Ok(LaneMap::same_name_lanes(registry)),
        }
    }

    /// Every mapped id must be a REGISTERED entity.
    ///
    /// `femcbost=fb` is one keystroke from a real mapping and, unchecked, maps nothing
    /// while looking exactly like a working configuration — the same failure the
    /// empty-half check in [`Self::parse`] refuses, one level up. The lane half is NOT
    /// checked here: which partitions exist is a fact about the corpus, not about this
    /// process, and it is answered by [`CliContextCompiler::reconcile_lanes`].
    pub fn validate_against(&self, registry: &crate::entity::EntityRegistry) -> Result<(), LoroError> {
        for entity in self.0.keys() {
            let known = crate::entity::EntityId::parse(entity).is_ok_and(|id| registry.contains(&id));
            if !known {
                return Err(LoroError::LaneMap(format!(
                    "{entity:?} is not a registered entity — a lane keyed by a typo maps nothing \
                     and looks like a working configuration"
                )));
            }
        }
        Ok(())
    }

    pub fn lane_for(&self, entity_id: &str) -> Option<&str> {
        self.0.get(entity_id).map(String::as_str)
    }

    /// The entity ids this map carries, in id order.
    pub fn entities(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// The company partitions a corpus ACTUALLY has, read from the corpus rather than assumed.
///
/// `loro-context.mjs corpus --format json` reports `companies` and `retiredCompanies`
/// (`bin/loro-context.mjs`, the `corpus` command). Measured on the CEO's real corpus,
/// three consecutive runs: **0.07 s, 0.06 s, 0.06 s** — cheap enough to run once at boot,
/// and the only thing that can tell a mapping from a wish.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorpusLanes {
    companies: Vec<String>,
    retired: Vec<String>,
    /// `"repo"` or `"corpus"`, as the corpus reported it. Empty when the caller built this
    /// with [`CorpusLanes::new`] and said nothing about the layout.
    layout: String,
    /// The corpus's own root, as the corpus reported it.
    root: PathBuf,
}

/// The subset of `corpus --format json` this consumer reads. Everything else in that
/// summary — coverage, counts, fingerprint — is an operator diagnostic, not a lane fact.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusSummary {
    #[serde(default)]
    companies: Vec<String>,
    #[serde(default)]
    retired_companies: Vec<String>,
    /// `"repo"` (an in-repo dogfood checkout: `wiki/` + `loro/`) or `"corpus"` (a
    /// provisioned `ceo/` + `companies/<id>/`). Read because those two are not the same
    /// object wearing different clothes — see [`CorpusLanes::repo_layout_root`].
    #[serde(default)]
    layout: String,
    #[serde(default)]
    root: String,
}

impl CorpusLanes {
    pub fn new(companies: &[String], retired: &[String]) -> Self {
        CorpusLanes { companies: companies.to_vec(), retired: retired.to_vec(), layout: String::new(), root: PathBuf::new() }
    }

    /// As [`Self::new`], plus the layout and root the corpus reported.
    pub fn with_layout(companies: &[String], retired: &[String], layout: &str, root: impl Into<PathBuf>) -> Self {
        CorpusLanes {
            companies: companies.to_vec(),
            retired: retired.to_vec(),
            layout: layout.to_string(),
            root: root.into(),
        }
    }

    /// The root of this corpus when it is an IN-REPO DOGFOOD checkout (`layout: "repo"`),
    /// and `None` when it is a provisioned corpus.
    ///
    /// # Why a consumer must care about the layout, not just the lanes
    ///
    /// A repo-layout corpus is one product's own record — `wiki/` + `loro/` of a single
    /// checkout — with **no company partitions and no company field on any item**. Every
    /// item is `company: null`, which is legitimately the CEO layer, so the lane map has
    /// nothing to narrow and [`Slice::foreign_lane`] has nothing to refuse: both work
    /// perfectly and neither can see the problem. The problem is that a thread bound to a
    /// DIFFERENT entity then receives that product's record under a heading reading
    /// `COMPANY MEMORY (loro)`.
    ///
    /// Measured, 2026-09-01, against the CEO's only corpus (`richos-hq`, 573 records,
    /// `layout: repo`): a `femcboost` thread asking *"how should we price the coach
    /// product"* was primed with three RichOS items — audio-capture click cost, Wispr Flow
    /// pricing, code-signing certificate authorities. Nothing was fabricated and nothing
    /// leaked across a partition; the corpus simply has one company in it and it is not
    /// FemcBoost.
    ///
    /// This is the fact [`CliContextCompiler::set_repo_corpus_owner`] is given so the
    /// payload can SAY so instead of presenting it as the entity's own memory.
    ///
    /// # A PARTITIONED repo-layout corpus returns `None`, and must
    ///
    /// `layout: "repo"` stopped meaning "unpartitioned" on 2026-09-01: the dogfood layout
    /// grew `ceo/` + `companies/<id>/` because a `corpus`-layout root is refused inside a
    /// product checkout and the CEO's only corpus is one. Every sentence above is
    /// conditioned on there being no partitions — the caveat it feeds says, in the payload,
    /// *"it is not partitioned by company and holds no `<entity>` partition"*. Against a
    /// partitioned corpus that sentence is simply false, and a false caveat is worse than
    /// none: it tells a fresh Rich to discount memory that is correctly his to read. So the
    /// condition is the ABSENCE OF PARTITIONS, which is what the caveat actually claims,
    /// not the layout name, which used to imply it.
    pub fn repo_layout_root(&self) -> Option<&Path> {
        (self.layout == "repo" && self.is_unpartitioned() && self.root.as_os_str().len() > 0)
            .then(|| self.root.as_path())
    }

    pub fn layout(&self) -> &str {
        &self.layout
    }

    /// Ask the corpus what partitions it has.
    ///
    /// A failure here is NOT fatal and must not be treated as "no partitions": those are
    /// different facts, and the caller decides. `Err` carries the reason so a boot line can
    /// print it.
    pub fn probe(tools: &LoroTools, root: &LoroRoot) -> Result<Self, LoroError> {
        let (flag, path) = root.args();
        let out = crate::runtime::interpreter_command(tools.node())
            .arg(tools.context_bin())
            .arg("corpus")
            .arg(flag)
            .arg(path)
            .arg("--format")
            .arg("json")
            .output()
            .map_err(|e| LoroError::LaneMap(format!("could not ask the corpus which lanes it has: {e}")))?;
        if !out.status.success() {
            let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
            let why = String::from_utf8_lossy(&out.stderr);
            return Err(LoroError::LaneMap(format!(
                "loro corpus exited {code}: {}",
                why.lines().next().unwrap_or("").trim()
            )));
        }
        let summary: CorpusSummary = serde_json::from_str(&String::from_utf8_lossy(&out.stdout))
            .map_err(|e| LoroError::LaneMap(format!("the corpus summary did not parse: {e}")))?;
        Ok(CorpusLanes {
            companies: summary.companies,
            retired: summary.retired_companies,
            layout: summary.layout,
            root: PathBuf::from(summary.root),
        })
    }

    pub fn has(&self, lane: &str) -> bool {
        self.companies.iter().any(|c| c == lane)
    }

    pub fn is_retired(&self, lane: &str) -> bool {
        self.retired.iter().any(|c| c == lane)
    }

    /// True when the corpus has no partitions at all — the CEO's state today, and the one
    /// in which "read everything" and "read the CEO layer" are the same read.
    pub fn is_unpartitioned(&self) -> bool {
        self.companies.is_empty()
    }

    pub fn companies(&self) -> &[String] {
        &self.companies
    }
}

// ---------------------------------------------------------------------------
// the slice — the documented shape, and only the documented shape
// ---------------------------------------------------------------------------

/// The compiled slice, parsed down to the fields `CONTEXT-CONTRACT.md` §2 promises.
///
/// Everything is `#[serde(default)]` and unknown keys are ignored, which is §2's
/// forward-compatibility rule as code: within `schemaVersion: 1` fields may be added, and a
/// consumer that breaks on a new field is a consumer that breaks on the next ranking
/// improvement.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    #[serde(default)]
    pub schema_version: u64,
    #[serde(default)]
    pub compiler: String,
    #[serde(default)]
    pub thin: bool,
    #[serde(default)]
    pub coverage: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub items: Vec<SliceItem>,
    #[serde(default)]
    pub corpus: SliceCorpus,
    #[serde(default)]
    pub budget: SliceBudget,
    #[serde(default)]
    pub notes: Vec<String>,
}

// `rename_all` is REQUIRED here, not cosmetic: the contract's field is `kindInferred` and
// without it serde looks for `kind_inferred`, finds nothing, and silently defaults an
// INFERRED kind to `false` — a guess flattened into a declaration by a missing attribute.
// Every other field on this struct is a single word, so nothing else moves.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceItem {
    #[serde(default)]
    pub r#ref: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub scope: String,
    /// `true` = the kind was GUESSED from prose rather than declared by a promoted record
    /// (`CONTEXT-CONTRACT.md` §2, `record.js` invariant 2). Read here, not merely ignored,
    /// because a correction desk that proposed `kind: decision` off a guess would be
    /// asserting an adjudicated claim the corpus never made.
    #[serde(default)]
    pub kind_inferred: bool,
    /// The item's LANE. `null` is the CEO layer — `CONTEXT-CONTRACT.md` §6c calls that
    /// "a legitimate permanent state, not an error".
    #[serde(default)]
    pub company: Option<String>,
    /// WHERE THIS CAME FROM. Parsed since 2026-09-17; the compiler has always emitted it on
    /// every item (`loro/lib/compile.js` copies `rec.provenance` into `items[]`) and this
    /// consumer dropped the whole object on the floor.
    ///
    /// It stopped being optional the moment the Workspace source became real. An answer
    /// built on the CEO's own calendar has to be able to say *"from your Google Calendar,
    /// Tuesday"* and offer the deep link back to the item in his own cloud; with the
    /// provenance discarded, the app could name neither, and the only thing left to cite
    /// was a `rec:…` path. See [`SliceItemProvenance`] for which field answers which
    /// question.
    #[serde(default)]
    pub provenance: SliceItemProvenance,
}

/// Where one slice item came from — the `items[].provenance` object of the slice contract.
///
/// Two of these fields are routinely confused and are deliberately named apart:
/// [`Self::source`] is the loro FAMILY that read the record (`records` / `wiki` / `memory` /
/// `entities`), and [`Self::origin`] is where the material came from OUTSIDE loro
/// (`workspace:google:calendar`, `conversation`). A promoted Calendar entry has
/// `source = "records"` and `origin = "workspace:google:calendar"`, and answering "where did
/// this come from" with the first one would be true and useless.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceItemProvenance {
    /// The loro source family that read this record.
    #[serde(default)]
    pub source: String,
    /// The corpus-relative file the record lives in — the answer to "show me".
    #[serde(default)]
    pub path: Option<String>,
    /// The section anchor within `path`, for a record compiled out of prose.
    #[serde(default)]
    pub anchor: Option<String>,
    /// A link carried on the record itself. Always `None` for a record FILE today:
    /// `loro/lib/sources.js recordFileRecords` does not read a `link:` field, so a deep link
    /// into a vendor's cloud travels in the record's BODY, where Rich can read it out. Parsed
    /// anyway rather than pretended away — the field is in the contract.
    #[serde(default)]
    pub link: Option<String>,
    /// `rich_inferred` (Rich promoted this from an observation) or
    /// `explicit_ceo_instruction` (the CEO said "remember this"). The difference decides how
    /// hard a correction should be to make, so it is never flattened.
    #[serde(default)]
    pub method: Option<String>,
    /// The identity of the thing this was promoted FROM — for the Workspace source, the
    /// vendor item id (`google:calendar:evt_…`), which is what resolves the evidence file.
    #[serde(default, rename = "ref")]
    pub source_ref: Option<String>,
    /// The writer's `--source-label`: `workspace:google:calendar`, `conversation`.
    #[serde(default)]
    pub origin: Option<String>,
}

impl SliceItemProvenance {
    /// Did this item come from the Workspace source (Calendar / Drive / Gmail), whichever
    /// vendor? Answered from [`Self::origin`], which the WRITER stated, and never from the
    /// shape of a ref: a prefix match on `google:` would also claim anything else a future
    /// source happened to key that way.
    pub fn is_workspace(&self) -> bool {
        self.origin.as_deref().is_some_and(|o| o == "workspace" || o.starts_with("workspace:"))
    }

    /// The vendor and source behind a Workspace item — `("google", "calendar")` — or `None`
    /// for anything else. The label is `workspace:<vendor>:<source>`.
    pub fn workspace_source(&self) -> Option<(&str, &str)> {
        let rest = self.origin.as_deref()?.strip_prefix("workspace:")?;
        rest.split_once(':')
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceCorpus {
    #[serde(default)]
    pub record_count: u64,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub root_source: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceBudget {
    #[serde(default)]
    pub chars: usize,
    #[serde(default)]
    pub used_chars: usize,
    #[serde(default)]
    pub items_included: usize,
    #[serde(default)]
    pub withheld_by_scope: usize,
}

/// An item in a compiled slice that belongs to a lane this thread is not entitled to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignLane {
    pub item_ref: String,
    pub found: String,
    pub expected: Option<String>,
}

impl Slice {
    /// **The cross-entity re-assertion, run on the FINISHED artifact.**
    ///
    /// The same posture loro takes on its own privacy wall — `CONTEXT-CONTRACT.md` §4:
    /// *"compile filters before ranking and re-asserts on the finished slice"* — applied on
    /// this side of the boundary, because a re-prime is the highest-leverage cross-entity
    /// leak in the app (`reprime.rs`: whatever lands there is asserted to a fresh session as
    /// authoritative).
    ///
    /// `expected` is the lane this thread's entity is mapped to, or `None` for a corpus with
    /// no partitions at all. CEO-layer items (`company: null`) are always allowed: ECS
    /// §3.5's default read set is *"the CEO layer plus the active entity"*, and
    /// `loro-structure.md` says the same in loro's own words.
    ///
    /// A violation returns the offending item rather than filtering it. **Filtering would be
    /// worse than useless here**: `items` is only a description of what is in `text`, and
    /// `text` is the string that gets injected — dropping the row would leave the foreign
    /// company's memory in the prompt and remove the only evidence that it was there.
    pub fn foreign_lane(&self, expected: Option<&str>) -> Option<ForeignLane> {
        self.items.iter().find_map(|item| match item.company.as_deref() {
            None => None,
            Some(found) if Some(found) == expected => None,
            Some(found) => Some(ForeignLane {
                item_ref: item.r#ref.clone(),
                found: found.to_string(),
                expected: expected.map(str::to_string),
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// PROVENANCE — which record an assertion came from, retained because nothing else keeps it
// ---------------------------------------------------------------------------

/// One record that was actually PUT IN FRONT OF RICH, kept so a later correction can say
/// which record it is a correction OF.
///
/// # Why this type exists at all
///
/// It closes a gap that was measured rather than assumed. A slice is compiled at re-prime
/// time, `interpret` returns [`LoroTier::Slice`] carrying **only `slice.text`**, and
/// `spine.rs` injects that string and drops it: the ledger records the priming turn as the
/// literal placeholder `"[re-prime]"` (`Spine::prime_lease_if_needed`) precisely so a
/// rotation stays invisible, so the slice survives nowhere. Meanwhile `items[]` — the ref,
/// the kind, the scope of everything in that text — was parsed, used once for the lane
/// re-assertion, and thrown away.
///
/// That is why `correction.rs`'s desk had no proposer. `CorrectionDesk::propose` takes a
/// record reference, and **a proposal against the wrong record is a corruption the CEO
/// would have to catch by reading `--dry-run` bytes.** Nothing in the app could supply that
/// reference honestly, so nothing proposed. This is the minimum provenance the resolution
/// needs and deliberately not one field more: no bodies, no corpus content beyond the line
/// that was already going into the prompt, and nothing retained for a slice that was
/// REFUSED — the recording happens after the lane re-assertion, never before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceRecord {
    /// The stable handle — `rec:…`, `mem:…`, `wiki:…`, `entity:…`. This is what a proposal
    /// is filed against, and it is copied, never derived.
    pub record_ref: String,
    pub kind: String,
    pub kind_inferred: bool,
    pub title: String,
    pub scope: String,
    pub company: Option<String>,
    /// Where the record came from, copied from the slice item. A correction filed against a
    /// promoted Workspace record is a correction of what RICH inferred from the CEO's own
    /// cloud, not of something the CEO asserted, and `provenance.method` is the only thing
    /// that can tell those apart after the fact.
    pub provenance: SliceItemProvenance,
    /// The item AS RENDERED into the prompt — `• [kind] Title — body… (ref: id)`.
    ///
    /// `None` when the ref could not be found in `text`, which is a real state rather than
    /// a defect: `slice.text` is truncated to the budget AS A WHOLE
    /// (`loro/lib/compile.js` truncates the joined heading + lines), so the LAST line can
    /// lose its own `(ref: …)` suffix. `items[]` is authoritative about what is in the
    /// slice; the text is authoritative about what Rich actually read. When they disagree
    /// the honest answer is that this record has no quotable line, and a resolver falls
    /// back to the title rather than inventing one.
    pub line: Option<String>,
}

impl SliceRecord {
    /// The text a correction is matched against: the rendered line if there is one, the
    /// title if there is not. Never both concatenated — a match must be attributable.
    ///
    /// **The machine furniture is stripped**, and that is not tidiness. `renderItem` writes
    /// `• [kind] Title — body… (ref: id)`, so leaving it in would put the kind name and
    /// every word of the record's own id into the text a correction resolves against: a
    /// record filed at `rec:ceo/records/ship-date` would answer to the word "date", and
    /// two records could collide on nothing but their storage paths. Rich asserts the
    /// title and the body; he does not assert the ref.
    pub fn matchable(&self) -> &str {
        let Some(line) = self.line.as_deref() else { return &self.title };
        let body = match line.find("] ") {
            Some(i) if line.trim_start().starts_with('•') => &line[i + 2..],
            _ => line,
        };
        match body.rfind(" (ref: ") {
            Some(i) if body.ends_with(')') => body[..i].trim_end(),
            _ => body.trim_end(),
        }
    }

    /// The line AS RICH READ IT, machine furniture and all. This is what a surface quotes
    /// back to the CEO, and it is deliberately not the same string as [`Self::matchable`]:
    /// the ref is exactly what makes the evidence checkable by hand.
    pub fn evidence(&self) -> &str {
        self.line.as_deref().unwrap_or(&self.title)
    }

    /// Can the loro writer supersede this ref at all?
    ///
    /// `wiki:` and `entity:` cannot be written (`loro-writer.md`, "Which refs the writer
    /// can address": a `wiki:` ref exits 5 — *"a machine rewriting the CEO's synthesis is
    /// not a correction, it is a substitution"* — and `entity:` is generated vocabulary,
    /// not a belief). A detector that proposed against one would produce a proposal
    /// guaranteed to be refused at `--dry-run`, which is a false proposal with extra steps.
    pub fn is_supersedable(&self) -> bool {
        self.record_ref.starts_with("rec:") || self.record_ref.starts_with("mem:")
    }
}

impl Slice {
    /// The slice's items paired with the lines they were rendered as.
    ///
    /// The pairing key is the `(ref: …)` suffix `loro/lib/compile.js:renderItem` writes,
    /// and it is exact rather than approximate: that function emits `rec.id`, and
    /// `items[].ref` is the same `rec.id`, so the two agree by construction.
    pub fn records(&self) -> Vec<SliceRecord> {
        let lines: Vec<&str> = self.text.lines().collect();
        self.items
            .iter()
            .map(|item| SliceRecord {
                line: lines
                    .iter()
                    .find(|l| l.trim_end().ends_with(&format!("(ref: {})", item.r#ref)))
                    .map(|l| l.trim().to_string()),
                record_ref: item.r#ref.clone(),
                kind: item.kind.clone(),
                kind_inferred: item.kind_inferred,
                title: item.title.clone(),
                scope: item.scope.clone(),
                company: item.company.clone(),
                provenance: item.provenance.clone(),
            })
            .collect()
    }

    /// The items that came from the Workspace source, in slice order.
    ///
    /// A convenience with a purpose: a surface that wants to show the CEO *"this came from
    /// your calendar"* must not have to re-implement the origin test, and a caller that
    /// filters on the wrong field silently shows him the wrong provenance.
    pub fn workspace_items(&self) -> impl Iterator<Item = &SliceItem> {
        self.items.iter().filter(|i| i.provenance.is_workspace())
    }
}

/// The slice that was injected into one thread's session, and what it was compiled from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectedSlice {
    pub thread_id: String,
    pub entity_id: String,
    /// The topic the slice was compiled for. Kept so a provenance can be recognized as
    /// belonging to a different conversation rather than silently answering about one.
    pub topic: String,
    /// `corpus.fingerprint` and `compiler` — the two things `CONTEXT-CONTRACT.md` §2 says
    /// to cache on, kept for the same reason: together they identify WHICH loro, compiled
    /// by WHICH ranker, this memory came from.
    pub fingerprint: String,
    pub compiler: String,
    pub at: u64,
    pub records: Vec<SliceRecord>,
}

/// What memory Rich was actually given, per thread. One entry per thread, replaced whole on
/// each compile — a slice is a snapshot, and two snapshots are not additive.
#[derive(Debug, Clone, Default)]
pub struct SliceProvenance {
    by_thread: BTreeMap<String, InjectedSlice>,
}

impl SliceProvenance {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what was injected. **Deliberately in-memory only.** This describes the
    /// CURRENT session's prompt; a resolution made against a slice from a session that no
    /// longer exists would attribute to Rich an assertion he was never given the memory to
    /// make. It dies with the process, exactly like the priming payload it describes.
    pub fn record(&mut self, injected: InjectedSlice) {
        self.by_thread.insert(injected.thread_id.clone(), injected);
    }

    /// Drop what was injected for a thread. Called when a NEW session is about to be primed
    /// for it: the entry described the previous session's prompt, and `record` only ever
    /// replaces it with an accepted slice. A later thin result, refusal or compiler failure
    /// would otherwise leave the old records resolvable, so a correction could cite memory the
    /// new session was never given (part 1 hunt finding 43).
    pub fn forget(&mut self, thread_id: &str) {
        self.by_thread.remove(thread_id);
    }

    pub fn for_thread(&self, thread_id: &str) -> Option<&InjectedSlice> {
        self.by_thread.get(thread_id)
    }

    /// The records currently resolvable for a thread — empty when nothing has been compiled
    /// for it, which is the ordinary state of an install with no corpus.
    pub fn records_for(&self, thread_id: &str) -> &[SliceRecord] {
        self.by_thread.get(thread_id).map(|s| s.records.as_slice()).unwrap_or(&[])
    }

    pub fn threads(&self) -> usize {
        self.by_thread.len()
    }
}

/// Shared the way `staging::SharedCandidateDesk` is, and for the same reason: the spine
/// writes it during a re-prime and the correction trigger reads it during a turn.
pub type SharedSliceProvenance = Arc<Mutex<SliceProvenance>>;

// ---------------------------------------------------------------------------
// the compiler
// ---------------------------------------------------------------------------

/// The shipped [`LoroContextCompiler`]: invokes `loro-context.mjs compile` and maps its
/// documented exit codes onto the four [`LoroTier`] states.
///
/// The mapping is `CONTEXT-CONTRACT.md` §3's own table, with one addition the contract
/// leaves to the caller:
///
/// | | contract | here |
/// |---|---|---|
/// | exit 0, `!thin` | use the slice | [`LoroTier::Slice`] — after the lane re-assertion |
/// | exit 0, `thin` | no slice | [`LoroTier::NothingRecorded`], carrying loro's OWN honest line |
/// | exit != 0 | no slice, never fail the turn | [`LoroTier::Unavailable`], carrying the code |
///
/// The contract maps a thin slice to "no slice, keep today's pull-loro-live line". That is
/// right about the injectable text and wrong about the honesty: a checked "loro holds
/// nothing on this" and an unconsulted loro are different facts and this build keeps them
/// different (see [`LoroTier`]). Loro already writes the correct sentence for the first case
/// — §5 — so the fix is to inject its sentence rather than to invent one.
pub struct CliContextCompiler {
    tools: LoroTools,
    root: LoroRoot,
    lanes: LaneMap,
    audience: String,
    /// Where the ITEMS of an accepted slice are retained, so a later correction can name
    /// the record it corrects (see [`SliceProvenance`]). Absent by default: a build with no
    /// correction trigger keeps nothing, and the read path is byte-identical either way.
    provenance: Option<SharedSliceProvenance>,
    /// The entity that OWNS this corpus when it is an in-repo dogfood checkout — see
    /// [`Self::set_repo_corpus_owner`]. `None` for a provisioned corpus, and `None` when
    /// the owner could not be determined, which is treated as "say nothing" rather than
    /// "guess a name".
    repo_corpus_owner: Option<String>,
}

impl CliContextCompiler {
    pub fn new(tools: LoroTools, root: LoroRoot, lanes: LaneMap) -> Self {
        CliContextCompiler {
            tools,
            root,
            lanes,
            audience: REPRIME_AUDIENCE.to_string(),
            provenance: None,
            repo_corpus_owner: None,
        }
    }

    /// Tell this compiler which entity's own record the corpus IS, when the corpus is an
    /// in-repo dogfood checkout rather than a partitioned one.
    ///
    /// The caller resolves it — `EntityRegistry::resolve_root(corpus.repo_layout_root())`
    /// — because the registry is the only thing that knows which repository belongs to
    /// which company, and it is now able to answer for a two-root venture. `None` when the
    /// corpus is provisioned, or when no registered entity owns that path: an unknown owner
    /// is left unstated rather than guessed.
    ///
    /// # What it changes, and it is one line
    ///
    /// Nothing about what is compiled, narrowed or refused. When a slice is accepted for an
    /// entity that is NOT the owner, [`Self::interpret`] prefixes one sentence naming whose
    /// record this is. That sentence is the difference between a fresh Rich reading RichOS's
    /// code-signing decisions as FemcBoost's company memory and reading them as RichOS's,
    /// surfaced for want of a FemcBoost partition.
    pub fn set_repo_corpus_owner(&mut self, owner: Option<String>) {
        self.repo_corpus_owner = owner;
    }

    /// The provenance sentence for `entity_id`, when one is owed. `None` for a provisioned
    /// corpus, and `None` when the reader IS the owner — RichOS reading RichOS's record is
    /// exactly right and needs no caveat.
    pub fn corpus_provenance_line(&self, entity_id: &str) -> Option<String> {
        let owner = self.repo_corpus_owner.as_deref()?;
        if owner == entity_id {
            return None;
        }
        Some(serde_json::json!({"source":"memory_corpus","owner":owner,"requested_entity":entity_id,"partitioned":false}).to_string())
    }

    /// Retain the items of every ACCEPTED slice here, keyed by thread.
    ///
    /// Attaching this changes nothing about what is compiled, what is injected or what is
    /// refused — it only stops the answer to *"which record did that come from?"* being
    /// thrown away one line after it was parsed.
    pub fn set_provenance_sink(&mut self, sink: SharedSliceProvenance) {
        self.provenance = Some(sink);
    }

    /// Build from the environment, or explain why not. `Ok(None)` = nothing configured,
    /// which is the ordinary state of an install with no corpus and is not an error.
    ///
    /// The lane map is validated against the registry here — a lane keyed by a typo is a
    /// configuration error worth refusing at boot, not a mapping that silently does
    /// nothing. It is NOT reconciled here: that needs the corpus, which is a child process
    /// away, and the caller decides what to do when the probe itself fails
    /// ([`Self::reconcile_lanes`]).
    pub fn from_env(registry: &crate::entity::EntityRegistry) -> Result<Option<Self>, LoroError> {
        let Some(root) = LoroRoot::from_env() else { return Ok(None) };
        let Some(tools) = LoroTools::from_env() else {
            return Err(LoroError::ToolsNotFound(
                "a corpus root is configured but RICHOS_LORO_DIR is not — richos ships no loro/ \
                 directory, so the tools cannot be inferred from this checkout"
                    .into(),
            ));
        };
        let lanes = LaneMap::from_env(registry)?;
        lanes.validate_against(registry)?;
        Ok(Some(CliContextCompiler::new(tools?, root, lanes)))
    }

    pub fn root(&self) -> &LoroRoot {
        &self.root
    }

    /// The loro checkout this compiler drives — exposed so a caller can run the same
    /// tools' `corpus` command for [`CorpusLanes::probe`] without configuring them twice
    /// and risking two different answers.
    pub fn tools(&self) -> &LoroTools {
        &self.tools
    }

    pub fn lanes(&self) -> &LaneMap {
        &self.lanes
    }

    /// Drop every mapping whose lane the corpus does not have, and return one sentence per
    /// drop.
    ///
    /// # Why this exists, measured rather than argued
    ///
    /// A `--company` loro cannot satisfy is not degraded, it is refused:
    ///
    /// ```text
    /// exit 2 — loro --company: no such company partition "femcboost" in this corpus.
    /// Known: (none). Refusing to compile an empty lane and call it an answer.
    /// ```
    ///
    /// That is the correct posture on loro's side and it is fatal on this one, because
    /// exit 2 maps to [`LoroTier::Unavailable`] on **every rotation**. The CEO's corpus
    /// today is the repo-layout dogfood corpus — 573 records, zero partitions — so a lane
    /// map that were merely filled in would trade a working 890-char slice for a permanent
    /// "loro could not be consulted". Reconciling makes the map real *and* inert until his
    /// corpus is actually partitioned, at which point the same map narrows with no
    /// configuration change at all.
    ///
    /// # What dropping a lane does NOT do
    ///
    /// It does not widen what may be READ. `--company` is an attention control, not a
    /// privacy control (`loro-structure.md`, "`--company` is NOT a privacy control"); the
    /// wall is [`Slice::foreign_lane`], re-asserted on the finished slice, and with no lane
    /// it allows the CEO layer and refuses every company item. A dropped lane compiles
    /// wider and reads no wider.
    pub fn reconcile_lanes(&mut self, corpus: &CorpusLanes) -> Vec<String> {
        let mut dropped = Vec::new();
        let mut kept = BTreeMap::new();
        for (entity, lane) in std::mem::take(&mut self.lanes).0 {
            if corpus.has(&lane) {
                kept.insert(entity, lane);
            } else {
                let why = if corpus.is_retired(&lane) {
                    format!("lane {lane:?} is retired in this corpus")
                } else if corpus.is_unpartitioned() {
                    "this corpus has no company partitions at all".to_string()
                } else {
                    format!("this corpus has no lane {lane:?} (it has: {})", corpus.companies().join(", "))
                };
                dropped.push(format!(
                    "entity {entity:?} -> lane {lane:?} dropped: {why}; {entity:?} reads the CEO layer"
                ));
            }
        }
        self.lanes = LaneMap(kept);
        dropped
    }

    /// Registered entities that will compile with NO lane against a corpus that HAS lanes.
    ///
    /// This is the one genuinely ambiguous state and it must not be silent. The compile
    /// widens (no `--company`), loro returns items from every partition, and the lane
    /// re-assertion refuses the slice whole — fail-closed and correct, but it reaches the
    /// CEO as "loro could not be consulted" every single turn. An operator needs to see it
    /// at boot, before he does.
    ///
    /// Empty when the corpus is unpartitioned: there, no lane IS the CEO layer, and
    /// reporting it would be noise on the only configuration that exists today.
    pub fn entities_with_no_lane(
        &self,
        registry: &crate::entity::EntityRegistry,
        corpus: &CorpusLanes,
    ) -> Vec<String> {
        if corpus.is_unpartitioned() {
            return Vec::new();
        }
        registry
            .entities()
            .iter()
            .filter(|e| self.lanes.lane_for(e.id.as_str()).is_none())
            .map(|e| e.id.to_string())
            .collect()
    }

    /// The argv for one compile, exposed so a test can assert what this type is capable of
    /// asking for — in particular that it names the READ entry point and carries no write
    /// verb (see the module doc).
    pub fn argv(&self, req: &SliceRequest<'_>) -> Vec<String> {
        let (root_flag, root_path) = self.root.args();
        let mut argv = vec![
            self.tools.context_bin().display().to_string(),
            "compile".into(),
            root_flag.into(),
            root_path.display().to_string(),
            // §1: the topic is multi-line natural language and "must not go through shell
            // quoting" — stdin, not a flag. Command does not use a shell, but the contract's
            // own instruction for Rust callers is --topic-stdin and it costs nothing to obey.
            "--topic-stdin".into(),
            "--budget-chars".into(),
            req.budget_chars.to_string(),
            "--audience".into(),
            self.audience.clone(),
            "--format".into(),
            "json".into(),
        ];
        if let Some(lane) = self.lanes.lane_for(req.entity_id) {
            argv.push("--company".into());
            argv.push(lane.to_string());
        }
        argv
    }

    fn run(&self, req: &SliceRequest<'_>) -> CompiledTier {
        use std::io::Write;
        use std::process::Stdio;

        let argv = self.argv(req);
        let mut child = match crate::runtime::interpreter_command(self.tools.node())
            .args(&argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return unreadable(format!("could not start the loro compiler: {e}")),
        };
        if let Some(mut stdin) = child.stdin.take() {
            // A broken pipe here is not fatal on its own — the exit code below decides.
            let _ = stdin.write_all(req.topic.as_bytes());
        }
        let out = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => return unreadable(format!("the loro compiler did not complete: {e}")),
        };
        if !out.status.success() {
            let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
            let why = String::from_utf8_lossy(&out.stderr);
            let why = why.lines().next().unwrap_or("").trim();
            return unreadable(format!("the loro compiler exited {code}: {why}"));
        }
        self.interpret_tier(&String::from_utf8_lossy(&out.stdout), req)
    }

    /// Parse + judge one compiler stdout. Split out from [`Self::run`] so every branch is
    /// testable without a corpus, a child process or a byte of the CEO's memory.
    ///
    /// The tier only. [`Self::interpret_tier`] is the same verdict WITH the compiler's own
    /// `coverage` label attached, which is what the evidence lookup hangs on.
    pub fn interpret(&self, stdout: &str, req: &SliceRequest<'_>) -> LoroTier {
        self.interpret_tier(stdout, req).tier
    }

    /// Parse + judge one compiler stdout, keeping the `coverage` label.
    ///
    /// **Every refusal carries [`SliceCoverage::Unknown`], and that is deliberate rather than
    /// lazy.** A slice that did not parse, whose schema is unsupported, that broke its own
    /// budget guarantee or that was refused by the lane re-assertion is a slice whose SELF-
    /// REPORT cannot be trusted either — including its report about whether it answered the
    /// question. Passing a refused slice's `coverage: "none"` through would let a slice that
    /// was thrown away still decide to open the CEO's files. `Unknown` never opens them.
    pub fn interpret_tier(&self, stdout: &str, req: &SliceRequest<'_>) -> CompiledTier {
        let slice: Slice = match serde_json::from_str(stdout) {
            Ok(s) => s,
            Err(e) => return unreadable(format!("the loro slice did not parse: {e}")),
        };
        let coverage = SliceCoverage::parse(&slice.coverage);
        if slice.schema_version != SUPPORTED_SLICE_SCHEMA {
            // §2: assert the version and treat anything else as unsupported rather than
            // mis-parsing it. Mis-parsing a memory slice does not fail loudly — it injects
            // subtly wrong company memory under a header calling it authoritative.
            return unreadable(format!(
                "slice schemaVersion {} is not supported (this build reads {SUPPORTED_SLICE_SCHEMA})",
                slice.schema_version
            ));
        }
        // THE LANE RE-ASSERTION. Before anything is injected, and before `thin` is even
        // consulted — a thin slice cannot carry a foreign item, but the guard must not
        // depend on that being true tomorrow.
        let expected = self.lanes.lane_for(req.entity_id);
        if let Some(bad) = slice.foreign_lane(expected) {
            return unreadable(format!(
                "the compiled slice carried company {:?} memory ({}) into entity {:?}{} — refused \
                 whole rather than filtered, because `items` only describes what is already in \
                 `text`",
                bad.found,
                bad.item_ref,
                req.entity_id,
                match &bad.expected {
                    Some(lane) => format!(", whose lane is {lane:?}"),
                    None => ", which is mapped to no lane".into(),
                }
            ));
        }
        // §7 guarantee 1 — `text.length <= budgetChars`, at every budget. Verified rather
        // than trusted: this string goes into a prompt the CEO is billed for on every
        // rotation, and the guarantee is cheap to check and expensive to assume.
        if slice.text.chars().count() > req.budget_chars {
            return unreadable(format!(
                "the slice broke its own budget guarantee: {} chars against a {}-char cap",
                slice.text.chars().count(),
                req.budget_chars
            ));
        }
        if slice.thin || slice.text.trim().is_empty() {
            // §5: loro's own thin line already says the honest thing, ending "Do not assume
            // company facts — ask the CEO or check a live system." Injecting that sentence
            // IS the empty-corpus answer. A blank text with thin unset would be a compiler
            // defect, so it degrades to the same honest state rather than to silence.
            let text = if slice.text.trim().is_empty() {
                format!(
                    "COMPANY MEMORY (loro): nothing recorded bears on \"{}\".",
                    req.topic
                )
            } else {
                slice.text.clone()
            };
            // THE COVERAGE LABEL IS THE COMPILER'S, KEPT UNCHANGED. A thin slice is
            // `coverage: "none"` (`compile.js:287`), which is the evidence lookup's main
            // door — memory genuinely holds nothing, so the CEO's own files are the next
            // honest place to look.
            return CompiledTier { tier: LoroTier::NothingRecorded(text), coverage };
        }
        // PROVENANCE, recorded LAST — after the schema check, after the lane re-assertion,
        // after the budget check. Everything above this line is a reason a slice must not be
        // trusted, and a record retained from a slice that was refused could be resolved
        // against later, which would file a proposal citing memory the CEO was never shown.
        if let Some(sink) = self.provenance.as_ref() {
            if let Ok(mut p) = sink.lock() {
                p.record(InjectedSlice {
                    thread_id: req.thread_id.to_string(),
                    entity_id: req.entity_id.to_string(),
                    topic: req.topic.to_string(),
                    fingerprint: slice.corpus.fingerprint.clone(),
                    compiler: slice.compiler.clone(),
                    at: crate::util::now_millis(),
                    records: slice.records(),
                });
            }
        }
        // THE PROVENANCE LINE, and it sits OUTSIDE the budget check above on purpose.
        //
        // `CONTEXT-CONTRACT.md` §7 guarantee 1 is a promise loro makes about ITS text, and
        // the check above holds it to that promise unaltered. This sentence is the APP's,
        // not loro's, and folding it into the cap would make a slice fail loro's guarantee
        // for something loro did not write. It is one line, it is only ever emitted when
        // the corpus is one company's own record and the reader is a different company,
        // and the alternative is an unmarked misattribution — a payload that is byte-honest
        // and reads as a lie.
        let tier = match self.corpus_provenance_line(req.entity_id) {
            Some(line) => LoroTier::Slice(format!("{line}\n\n{}", slice.text)),
            None => LoroTier::Slice(slice.text),
        };
        // `adjacent` reaches the caller on an ACCEPTED slice, and that is the second of the
        // two evidence doors: loro returned its nearest material and said, in its own
        // heading, that nothing squarely covers the question (`compile.js:144-145`).
        CompiledTier { tier, coverage }
    }
}

/// A slice that must not be trusted, with no coverage claim attached — see
/// [`CliContextCompiler::interpret_tier`] for why every refusal is `Unknown`.
fn unreadable(reason: String) -> CompiledTier {
    CompiledTier { tier: LoroTier::Unavailable(reason), coverage: SliceCoverage::Unknown }
}

impl LoroContextCompiler for CliContextCompiler {
    fn compile_slice(&self, req: &SliceRequest<'_>) -> LoroTier {
        self.compile_tier(req).tier
    }

    fn compile_tier(&self, req: &SliceRequest<'_>) -> CompiledTier {
        if req.topic.trim().is_empty() {
            // §1: "A slice is always TOPICAL — there is no compile all of loro." An empty
            // topic is a caller bug, and asking anyway would exit 2 on every rotation.
            return unreadable("no topic — the thread has nothing the CEO has said yet".into());
        }
        self.run(req)
    }
}

// ---------------------------------------------------------------------------
// WHERE IS THE CORPUS? — resolution for a launch that has no terminal
// ---------------------------------------------------------------------------

/// The inputs a launch supplies, injected rather than read, so the GUI condition (empty
/// environment, launchd's `PATH`) is a VALUE in a test instead of a mutation of the test
/// process. Exactly the shape `src-tauri/src/engine.rs`'s `LaunchPaths` has, and for
/// exactly the same reason: the one condition that matters is the one no developer's shell
/// ever produces.
#[derive(Debug, Default, Clone)]
pub struct CorpusPaths {
    /// `$LORO_CORPUS`.
    pub env_corpus: Option<String>,
    /// `$LORO_ROOT`.
    pub env_root: Option<String>,
    /// `$RICHOS_LORO_DIR`.
    pub env_tools: Option<String>,
    /// Exact engine selected by the app. Its component is authoritative when supplied.
    pub engine_dir: Option<PathBuf>,
    /// `$RICHOS_NODE_BIN`.
    pub env_node: Option<String>,
    /// `$HOME`.
    pub home: Option<PathBuf>,
    /// `$PATH`, as the process received it.
    pub path_var: Option<String>,
}

impl CorpusPaths {
    /// Read the real process. The only function in this section that touches global state.
    pub fn from_process() -> Self {
        fn var(name: &str) -> Option<String> {
            std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
        }
        CorpusPaths {
            env_corpus: var("LORO_CORPUS"),
            env_root: var("LORO_ROOT"),
            env_tools: var("RICHOS_LORO_DIR"),
            engine_dir: var("RICHOS_ENGINE_DIR").or_else(|| var("RICHOS_ENGINE_ROOT")).map(PathBuf::from),
            env_node: var("RICHOS_NODE_BIN"),
            home: var("HOME").map(PathBuf::from),
            path_var: var("PATH"),
        }
    }
}

/// Which candidate answered — printed at boot so an operator never has to guess which one a
/// running app is using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusSource {
    /// `$LORO_CORPUS` — explicit.
    EnvCorpus,
    /// `$LORO_ROOT` — explicit.
    EnvRoot,
    /// `~/Library/Application Support/RichOS/corpus`.
    AppSupportCorpus,
    /// `~/Library/Application Support/RichOS/loro-root`.
    AppSupportRoot,
    /// `~/RichOS/corpus`.
    HomeCorpus,
}

impl CorpusSource {
    pub fn as_str(self) -> &'static str {
        match self {
            CorpusSource::EnvCorpus => "LORO_CORPUS",
            CorpusSource::EnvRoot => "LORO_ROOT",
            CorpusSource::AppSupportCorpus => "the corpus pointer in Application Support",
            CorpusSource::AppSupportRoot => "the loro-root pointer in Application Support",
            CorpusSource::HomeCorpus => "~/RichOS/corpus",
        }
    }
}

/// A PROVISIONED corpus is `ceo/` + `companies/<id>/`. Both, or it is not one.
///
/// The check is what makes a searched candidate safe to use at all: a directory that
/// happens to be called `corpus` and holds neither is rejected rather than handed to loro,
/// which would refuse it once per rotation, forever.
pub fn provisioned_corpus_looks_valid(dir: &Path) -> bool {
    dir.join("ceo").is_dir() && dir.join("companies").is_dir()
}

/// An IN-REPO dogfood root is a checkout with `wiki/` + `loro/` — loro's own words for the
/// second shape (`--root`), and the shape it INSISTS on for a corpus that lives inside a
/// git checkout:
///
/// ```text
/// usage: loro corpus root: refusing a corpus inside the RichOS product repo (...). The
/// corpus is the CEO's private record and RichOS ships publicly ... For the in-repo dogfood
/// case use --root / LORO_ROOT, which says so out loud.
/// ```
///
/// MEASURED 2026-09-01 against the CEO's own corpus: `--corpus ~/ab/richos-hq` exits 2 with
/// that message and `--root ~/ab/richos-hq` exits 0 with a 1,149-char slice. So the two
/// pointers below are two DIFFERENT names, not one name with a guess about its shape —
/// resolving the wrong one would be a refusal on every turn.
pub fn repo_root_looks_valid(dir: &Path) -> bool {
    dir.join("wiki").is_dir() && dir.join("loro").is_dir()
}

/// The answer, plus the audit trail behind it.
#[derive(Debug, Clone)]
pub struct CorpusResolution {
    pub root: Option<LoroRoot>,
    pub source: Option<CorpusSource>,
    /// Every candidate considered and what became of it, in order. Reported when nothing
    /// resolved, so the boot log says what was looked for rather than only that it failed.
    pub tried: Vec<String>,
}

/// WHERE IS THE CEO'S MEMORY? — the same problem `engine.rs` solves for the engine
/// directory, and until 2026-09-01 it had the same answer: an environment variable, and
/// nothing else.
///
/// **That is a `cargo run` assumption, and a double-clicked `.app` does not meet it.**
/// LaunchServices gives a GUI process launchd's environment, which carries no `LORO_CORPUS`,
/// no `LORO_ROOT` and no `RICHOS_LORO_DIR` — so the shipped bundle booted with
/// `[richos] loro Tier C: no corpus configured` and every re-prime asserted a fresh Rich
/// with no company memory at all. MEASURED on the signed bundle, boot log quoted in
/// `docs/verification/installed-app-2026-09-01/`.
///
/// # The order, and why it is this order
///
/// It mirrors `engine.rs`, including its two governing rules:
///
/// - **An explicit statement is EXCLUSIVE.** If the operator named a root, that root is
///   used and resolution never falls through to one nobody named. A bad explicit path is an
///   error for loro to report, not a reason to guess — so an explicit value is NOT
///   validated here, and loro's own refusal is the message.
/// - **A searched candidate must LOOK like what it claims to be** — `ceo/` + `companies/`
///   for a provisioned corpus, `wiki/` + `loro/` for an in-repo root.
///
/// | # | candidate | why it is here |
/// |---|---|---|
/// | 1 | `$LORO_CORPUS` | the contract's own first name for a provisioned corpus. Explicit, exclusive, taken verbatim. |
/// | 2 | `$LORO_ROOT` | the contract's second name, for the in-repo dogfood shape. Also explicit, also exclusive. |
/// | 3 | `~/Library/Application Support/RichOS/corpus` | the per-user location an installer could populate, beside `engine.rs` candidate 7. |
/// | 4 | `~/Library/Application Support/RichOS/loro-root` | the same slot for the in-repo shape. **This is the one a double-clicked `.app` reaches on the CEO's dogfood machine**, where the corpus lives in a checkout and loro refuses `--corpus` for it. |
/// | 5 | `~/RichOS/corpus` | the drop-zone convention already documented at `tools/richos-service/companion-macos/README.md:66`. |
///
/// # What this does NOT do
///
/// - **It does not put a corpus on anybody's computer**, and it does not create either
///   pointer. Both are an operator act — the same posture as the engine install pointer.
/// - **It never infers the corpus from the checkout this binary sits in.** That is the one
///   inference `CONTEXT-CONTRACT.md` §1 names as worse than an error, and no candidate
///   above is derived from the executable, the working directory or the engine directory.
/// - **It resolves a directory. It reads no memory.** Nothing here opens a record.
pub fn resolve_corpus(p: &CorpusPaths) -> CorpusResolution {
    let mut tried = Vec::new();

    if let Some(v) = p.env_corpus.as_deref() {
        return CorpusResolution {
            root: Some(LoroRoot::Corpus(PathBuf::from(v))),
            source: Some(CorpusSource::EnvCorpus),
            tried,
        };
    }
    if let Some(v) = p.env_root.as_deref() {
        return CorpusResolution {
            root: Some(LoroRoot::Root(PathBuf::from(v))),
            source: Some(CorpusSource::EnvRoot),
            tried,
        };
    }

    let home = match p.home.as_deref() {
        Some(h) => h,
        None => {
            tried.push("no HOME, so no per-user candidate could be formed".into());
            return CorpusResolution { root: None, source: None, tried };
        }
    };
    let support = home.join("Library").join("Application Support").join("RichOS");

    let candidates: [(PathBuf, CorpusSource); 3] = [
        (support.join("corpus"), CorpusSource::AppSupportCorpus),
        (support.join("loro-root"), CorpusSource::AppSupportRoot),
        (home.join("RichOS").join("corpus"), CorpusSource::HomeCorpus),
    ];

    for (dir, source) in candidates {
        let wants_repo_shape = matches!(source, CorpusSource::AppSupportRoot);
        let ok = if wants_repo_shape { repo_root_looks_valid(&dir) } else { provisioned_corpus_looks_valid(&dir) };
        if ok {
            let root = if wants_repo_shape { LoroRoot::Root(dir) } else { LoroRoot::Corpus(dir) };
            return CorpusResolution { root: Some(root), source: Some(source), tried };
        }
        tried.push(format!(
            "{} — {}",
            dir.display(),
            if !dir.exists() {
                "not present".to_string()
            } else if wants_repo_shape {
                "present but not a loro root (wants wiki/ and loro/)".to_string()
            } else {
                "present but not a provisioned corpus (wants ceo/ and companies/)".to_string()
            }
        ));
    }

    CorpusResolution { root: None, source: None, tried }
}

/// Resolve `node` for a launch that has launchd's `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`)
/// and nothing else.
///
/// The same shape as `native.rs::resolve_claude_bin`, and here for the same measured
/// reason: on this machine `node` is `/opt/homebrew/bin/node`, which is on no GUI process's
/// `PATH`, so a resolved corpus with an unresolvable `node` would fail once per rotation
/// with `could not start the loro compiler: No such file or directory`.
///
/// 1. `$RICHOS_NODE_BIN` — explicit, exclusive.
/// 2. the first `node` on `PATH` — the operator's own choice, returned as an ABSOLUTE path
///    so the child process does not have to repeat the search under a different `PATH`.
/// 3. the two default Homebrew prefixes, arm64 then x86_64.
/// 4. the bare name, which fails loudly at the first compile and names itself.
pub fn resolve_node_bin(p: &CorpusPaths) -> String {
    if let Some(v) = p.env_node.as_deref() {
        return v.to_string();
    }
    if let Some(engine) = &p.engine_dir {
        // A selected engine is authoritative even when its runtime is absent.
        // Falling back here would hide an incomplete delivered installation.
        return engine.join("runtime/bin/node").display().to_string();
    }
    if let Some(path_var) = p.path_var.as_deref() {
        for dir in path_var.split(':').filter(|d| !d.is_empty()) {
            let candidate = Path::new(dir).join("node");
            if candidate.is_file() {
                return candidate.display().to_string();
            }
        }
    }
    for candidate in ["/opt/homebrew/bin/node", "/usr/local/bin/node"] {
        if Path::new(candidate).is_file() {
            return candidate.to_string();
        }
    }
    "node".to_string()
}

/// Which candidate supplied the compiler, printed at boot for the same reason
/// [`CorpusSource`] is: an operator must never have to guess which of three a running app is
/// executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolsSource {
    /// `$RICHOS_LORO_DIR` — explicit, exclusive.
    EnvVar,
    /// The component in the exact engine selected by the app.
    EngineComponent,
    /// `<root>/loro` — the in-repo dogfood shape, where `repo_root_looks_valid` REQUIRED it
    /// to be. This is the one the CEO's own install resolves through today.
    RootsOwnLoro,
    /// `~/Library/Application Support/RichOS/loro-tools` — where first-run provisioning
    /// installs it (`provision::compiler_install_dir`).
    AppSupportLoroTools,
}

impl ToolsSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolsSource::EnvVar => "RICHOS_LORO_DIR",
            ToolsSource::EngineComponent => "the selected engine's loro component",
            ToolsSource::RootsOwnLoro => "the corpus root's own loro/ directory",
            ToolsSource::AppSupportLoroTools => "the loro-tools install in Application Support",
        }
    }
}

/// WHERE IS THE PROGRAM THAT READS THE CORPUS? — the twin of [`resolve_corpus`], and it
/// exists for the second half of the same premise failure.
///
/// Until 2026-09-01 the answer was `$RICHOS_LORO_DIR`, or the root's own `loro/`, or an
/// error. That is correct for the CEO's dogfood machine, where the corpus IS a checkout that
/// carries the compiler, and impossible for a provisioned corpus: a corpus with `loro/`
/// inside it is REFUSED by loro's own open-source boundary — measured, exit 2, *"refusing a
/// corpus inside the RichOS product repo"* — so a provisioned corpus can never carry its own
/// tools, and something outside it has to.
///
/// | # | candidate | why it is here |
/// |---|---|---|
/// | 1 | `$RICHOS_LORO_DIR` | explicit and EXCLUSIVE: a bad value is an error, never a reason to search. |
/// | 2 | `<root>/loro` | the in-repo shape by definition. Kept ahead of 3 so the CEO's install resolves exactly as it did before this function existed. |
/// | 3 | `~/Library/Application Support/RichOS/loro-tools` | where provisioning installs it. Named `loro-tools` and not `loro` because an ancestor directory named `loro` makes loro refuse the corpus — measured, both placements. |
///
/// Every searched candidate is validated by [`LoroTools::locate`] — both entry points present
/// — so a directory that merely exists is rejected here rather than failing once per rotation
/// forever.
pub fn resolve_tools(
    p: &CorpusPaths,
    root: &LoroRoot,
) -> Result<(Option<(LoroTools, ToolsSource)>, Vec<String>), LoroError> {
    let mut tried = Vec::new();
    if let Some(v) = p.env_tools.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        // Explicit and exclusive. `?` on purpose: an operator who named a directory that is
        // not a loro checkout gets that fact, not a silent search past it.
        return Ok((Some((LoroTools::locate(v)?, ToolsSource::EnvVar)), tried));
    }
    if let Some(engine) = &p.engine_dir {
        let component = engine.join("loro");
        return match LoroTools::locate(&component) {
            Ok(tools) => Ok((Some((tools, ToolsSource::EngineComponent)), tried)),
            Err(error) => Ok((None, vec![format!("{} (selected engine): {error}", component.display())])),
        };
    }
    let mut candidates: Vec<(PathBuf, ToolsSource)> =
        vec![(root.path().join("loro"), ToolsSource::RootsOwnLoro)];
    if let Some(home) = p.home.as_deref() {
        candidates.insert(0, (crate::setup::engine_install_dir(home).join("loro"), ToolsSource::EngineComponent));
        candidates.push((crate::provision::compiler_install_dir(home), ToolsSource::AppSupportLoroTools));
    }
    for (dir, source) in candidates {
        match LoroTools::locate(&dir) {
            Ok(tools) => return Ok((Some((tools, source)), tried)),
            Err(e) => tried.push(format!("{} ({}) — {e}", dir.display(), source.as_str())),
        }
    }
    Ok((None, tried))
}

// ---------------------------------------------------------------------------
// ONE RESOLUTION, BOTH HALVES — the seam a third copy would have become
// ---------------------------------------------------------------------------

/// WHERE LORO IS, resolved ONCE, for every consumer.
///
/// # Why this type exists at all
///
/// On 2026-09-01 the same premise failure — *"read your configuration from an environment
/// variable a real launch does not have"* — was found and fixed three times in one day: the
/// engine directory (`engine.rs`), the corpus READ path ([`resolve_corpus`], landed at
/// `46d8f56`), and then the corpus WRITE path, one file over, which never got the twin.
/// The obvious fourth move was a `CliLoroWriter::locate` repeating the reader's candidate
/// walk. **That would have been the third near-identical resolver, and three near-identical
/// resolvers is how the fourth one gets forgotten** — which is the mechanism that produced
/// the defect this type closes.
///
/// So resolution happens here, once, and both halves of loro are BUILT FROM THE RESULT:
/// [`CliContextCompiler::from_install`] reads, `correction::CliLoroWriter::from_install`
/// writes. The agreement property — *the reader and the writer are looking at the same
/// corpus* — is therefore structural rather than merely tested for. It is still tested for,
/// because a future edit could reintroduce a second walk, and the test is what would catch
/// it.
///
/// Why that agreement matters more than either half on its own: a build where the read path
/// resolves one root and the write path another shows the CEO a proposal computed against
/// one record and writes his confirmation into a different corpus. That is worse than the
/// dead desk it replaced — a dead desk refuses, a disagreeing pair succeeds and is wrong.
#[derive(Debug, Clone)]
pub struct LoroInstall {
    root: LoroRoot,
    source: CorpusSource,
    tools: LoroTools,
    tools_source: ToolsSource,
}

impl LoroInstall {
    /// The corpus root, and which of `--corpus` / `--root` names it.
    pub fn root(&self) -> &LoroRoot {
        &self.root
    }

    /// Which candidate answered, for the boot line.
    pub fn source(&self) -> CorpusSource {
        self.source
    }

    /// The loro checkout, with `node` already resolved for a launch that has launchd's
    /// `PATH` and nothing else.
    pub fn tools(&self) -> &LoroTools {
        &self.tools
    }

    /// Which candidate supplied the compiler.
    pub fn tools_source(&self) -> ToolsSource {
        self.tools_source
    }

    /// RESOLVE, ONCE. `Ok((None, tried))` = no corpus, which is the ordinary state of an
    /// install with no corpus and is not an error; `tried` travels with it so the caller can
    /// say what was looked for instead of only that it failed.
    ///
    /// [`LoroError::CompilerNotInstalled`] = a corpus resolved and the program that reads and
    /// writes it did not. Also not a misconfiguration, and also its own sentence.
    pub fn locate(p: &CorpusPaths) -> Result<(Option<Self>, Vec<String>), LoroError> {
        let resolved = resolve_corpus(p);
        let Some(root) = resolved.root else { return Ok((None, resolved.tried)) };

        // TOOLS. Explicit first and exclusive; otherwise the `loro/` directory of the root
        // that was just resolved — which is where it is in the in-repo shape by definition
        // (`repo_root_looks_valid` required it), and where a provisioned corpus may or may
        // not have one. Never inferred from THIS checkout: richos ships no `loro/`.
        let (found, tools_tried) = resolve_tools(p, &root)?;
        let Some((mut tools, tools_source)) = found else {
            return Err(LoroError::CompilerNotInstalled {
                root: root.path().display().to_string(),
                tried: tools_tried.join("; "),
            });
        };
        tools.set_node(resolve_node_bin(p));

        Ok((
            Some(LoroInstall {
                root,
                source: resolved.source.expect("a root has a source"),
                tools,
                tools_source,
            }),
            resolved.tried,
        ))
    }
}

impl CliContextCompiler {
    /// Build from a launch's paths, or explain why not — the GUI-safe twin of
    /// [`Self::from_env`], which stays exactly as it was because its name is a promise
    /// about where it looks.
    ///
    /// `Ok(None)` = no corpus resolved, which is the ordinary state of an install with no
    /// corpus and is not an error. The [`CorpusResolution::tried`] list travels with it so
    /// the caller can say what was looked for.
    pub fn locate(
        p: &CorpusPaths,
        registry: &crate::entity::EntityRegistry,
    ) -> Result<(Option<(Self, CorpusSource)>, Vec<String>), LoroError> {
        let (install, tried) = LoroInstall::locate(p)?;
        let Some(install) = install else { return Ok((None, tried)) };
        let source = install.source();
        Ok((Some((CliContextCompiler::from_install(&install, registry)?, source)), tried))
    }

    /// The READ half of a resolved install. The lane map is this half's own configuration —
    /// it narrows what a slice may CONTAIN and has nothing to say about where a write goes —
    /// so it is read here and not in [`LoroInstall`].
    pub fn from_install(
        install: &LoroInstall,
        registry: &crate::entity::EntityRegistry,
    ) -> Result<Self, LoroError> {
        let lanes = LaneMap::from_env(registry)?;
        lanes.validate_against(registry)?;
        Ok(CliContextCompiler::new(install.tools.clone(), install.root.clone(), lanes))
    }
}

#[cfg(test)]
mod locate_tests {
    use super::*;

    /// `CliContextCompiler::locate` with NO companies registered.
    ///
    /// Every test in this module is about finding a corpus, not about lane narrowing, and an
    /// empty registry yields an empty lane map — which is exactly the "no narrowing" posture
    /// they were written under, when the map came from a compiled-in company list and these
    /// tests set no `RICHOS_LORO_LANES`.
    fn locate_here(
        p: &CorpusPaths,
    ) -> Result<(Option<(CliContextCompiler, CorpusSource)>, Vec<String>), LoroError> {
        CliContextCompiler::locate(p, &crate::entity::EntityRegistry::empty())
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-corpus-{name}-{}", crate::util::now_millis()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn provisioned(at: &Path) {
        std::fs::create_dir_all(at.join("ceo")).unwrap();
        std::fs::create_dir_all(at.join("companies")).unwrap();
    }

    fn repo_shaped(at: &Path) {
        std::fs::create_dir_all(at.join("wiki")).unwrap();
        std::fs::create_dir_all(at.join("loro").join("bin")).unwrap();
        std::fs::write(at.join("loro").join("bin").join("loro-context.mjs"), "//").unwrap();
        std::fs::write(at.join("loro").join("bin").join("loro-write.mjs"), "//").unwrap();
    }

    /// THE CASE THAT SHIPPED BROKEN: a Finder launch. No environment at all.
    #[test]
    fn the_gui_launch_with_nothing_set_resolves_nothing_and_says_what_it_tried() {
        let home = tmp("empty-home");
        let r = resolve_corpus(&CorpusPaths { home: Some(home), ..Default::default() });
        assert!(r.root.is_none());
        assert!(r.source.is_none());
        assert_eq!(r.tried.len(), 3, "three per-user candidates, each named: {:?}", r.tried);
        assert!(r.tried.iter().all(|t| t.contains("not present")), "{:?}", r.tried);
    }

    /// THE CASE THAT MAKES THE FIX REAL: the same empty environment, with the pointer the
    /// operator put in place.
    #[test]
    fn the_gui_launch_finds_the_app_support_loro_root_pointer() {
        let home = tmp("with-pointer");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(&support).unwrap();
        repo_shaped(&support.join("loro-root"));

        let r = resolve_corpus(&CorpusPaths { home: Some(home.clone()), ..Default::default() });
        assert_eq!(r.source, Some(CorpusSource::AppSupportRoot));
        assert_eq!(
            r.root,
            Some(LoroRoot::Root(support.join("loro-root"))),
            "the in-repo shape resolves to --root, never --corpus: loro REFUSES --corpus inside a checkout"
        );
    }

    #[test]
    fn a_provisioned_corpus_pointer_resolves_to_the_corpus_flag() {
        let home = tmp("provisioned");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(&support).unwrap();
        provisioned(&support.join("corpus"));

        let r = resolve_corpus(&CorpusPaths { home: Some(home), ..Default::default() });
        assert_eq!(r.source, Some(CorpusSource::AppSupportCorpus));
        assert!(matches!(r.root, Some(LoroRoot::Corpus(_))));
    }

    /// The shape check is the whole safety of a searched candidate. A directory with the
    /// right NAME and the wrong contents is rejected, and the rejection says which.
    #[test]
    fn a_directory_with_the_right_name_and_the_wrong_shape_is_refused() {
        let home = tmp("wrong-shape");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(support.join("corpus").join("notes")).unwrap();
        std::fs::create_dir_all(support.join("loro-root").join("notes")).unwrap();

        let r = resolve_corpus(&CorpusPaths { home: Some(home), ..Default::default() });
        assert!(r.root.is_none(), "resolved {:?} from a directory that is neither shape", r.root);
        assert!(r.tried[0].contains("wants ceo/ and companies/"), "{:?}", r.tried);
        assert!(r.tried[1].contains("wants wiki/ and loro/"), "{:?}", r.tried);
    }

    /// An explicit statement is EXCLUSIVE — it wins over a pointer that is right there and
    /// valid, and it is not second-guessed.
    #[test]
    fn an_explicit_root_wins_over_a_valid_pointer_and_is_never_validated() {
        let home = tmp("explicit");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(&support).unwrap();
        repo_shaped(&support.join("loro-root"));

        let r = resolve_corpus(&CorpusPaths {
            env_root: Some("/nowhere/at/all".into()),
            home: Some(home),
            ..Default::default()
        });
        assert_eq!(r.source, Some(CorpusSource::EnvRoot));
        assert_eq!(r.root, Some(LoroRoot::Root(PathBuf::from("/nowhere/at/all"))));
        assert!(r.tried.is_empty(), "an exclusive answer considers nothing else");
    }

    #[test]
    fn loro_corpus_outranks_loro_root_which_is_the_contracts_own_precedence() {
        let r = resolve_corpus(&CorpusPaths {
            env_corpus: Some("/c".into()),
            env_root: Some("/r".into()),
            ..Default::default()
        });
        assert_eq!(r.root, Some(LoroRoot::Corpus(PathBuf::from("/c"))));
    }

    /// `node` on launchd's PATH, which is the GUI condition: none of the four directories
    /// holds one, so the Homebrew prefixes are what save the compile.
    #[test]
    fn node_falls_through_launchd_path_to_the_homebrew_prefix_or_names_itself() {
        let resolved = resolve_node_bin(&CorpusPaths {
            path_var: Some("/usr/bin:/bin:/usr/sbin:/sbin".into()),
            ..Default::default()
        });
        let plausible = resolved == "node"
            || resolved == "/opt/homebrew/bin/node"
            || resolved == "/usr/local/bin/node";
        assert!(plausible, "resolved {resolved:?}");
    }

    #[test]
    fn an_explicit_node_is_taken_verbatim() {
        let resolved = resolve_node_bin(&CorpusPaths {
            env_node: Some("/opt/custom/node".into()),
            path_var: Some("/usr/bin:/bin".into()),
            ..Default::default()
        });
        assert_eq!(resolved, "/opt/custom/node");
    }

    #[test]
    fn a_node_on_path_is_returned_as_an_absolute_path() {
        let dir = tmp("nodebin");
        std::fs::write(dir.join("node"), "#!/bin/sh\n").unwrap();
        let resolved = resolve_node_bin(&CorpusPaths {
            path_var: Some(format!("/nonexistent:{}", dir.display())),
            ..Default::default()
        });
        assert_eq!(resolved, dir.join("node").display().to_string());
    }

    /// A resolved root whose tools are missing is an ERROR, not a silent `None`: the
    /// difference between "this install has no corpus" and "this install has a corpus it
    /// cannot read" is the difference between a fact and a defect.
    #[test]
    fn a_root_with_no_usable_tools_is_an_error_naming_the_root() {
        let home = tmp("no-tools");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(&support).unwrap();
        provisioned(&support.join("corpus"));

        let msg = match locate_here(&CorpusPaths { home: Some(home), ..Default::default() }) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("a corpus with no tools must not resolve silently"),
        };
        assert!(msg.contains("RICHOS_LORO_DIR"), "{msg}");
        assert!(msg.contains("corpus"), "{msg}");
    }

    #[test]
    fn a_resolved_root_carries_its_own_loro_directory_as_the_tools() {
        let home = tmp("root-tools");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(&support).unwrap();
        repo_shaped(&support.join("loro-root"));

        let (built, _tried) =
            locate_here(&CorpusPaths { home: Some(home), ..Default::default() }).unwrap();
        let (compiler, source) = built.expect("a valid pointer resolves");
        assert_eq!(source, CorpusSource::AppSupportRoot);
        assert_eq!(compiler.tools().dir(), support.join("loro-root").join("loro"));
    }

    fn compiler_at(dir: &Path) {
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("loro-context.mjs"), "//").unwrap();
        std::fs::write(dir.join("bin").join("loro-write.mjs"), "//").unwrap();
    }

    /// THE CASE PROVISIONING CREATES, and the one that could not resolve before it existed:
    /// a provisioned corpus cannot carry its own `loro/` — loro refuses a corpus with one
    /// inside it — so the compiler is found beside it in Application Support.
    #[test]
    fn a_provisioned_corpus_finds_its_compiler_in_the_loro_tools_install() {
        let home = tmp("tools-install");
        let support = home.join("Library").join("Application Support").join("RichOS");
        provisioned(&support.join("corpus"));
        compiler_at(&crate::provision::compiler_install_dir(&home));

        let (built, _tried) =
            locate_here(&CorpusPaths { home: Some(home.clone()), ..Default::default() }).unwrap();
        let (compiler, source) = built.expect("a provisioned corpus with an installed compiler resolves");
        assert_eq!(source, CorpusSource::AppSupportCorpus);
        assert_eq!(compiler.tools().dir(), crate::provision::compiler_install_dir(&home));
    }

    /// The CEO's own arrangement, unchanged by the candidate list growing: the root's own
    /// `loro/` is tried BEFORE the Application Support install, so an install that exists for
    /// some other reason can never quietly take over a dogfood checkout's compiler.
    #[test]
    fn the_roots_own_loro_outranks_the_application_support_install() {
        let home = tmp("both-tools");
        let support = home.join("Library").join("Application Support").join("RichOS");
        std::fs::create_dir_all(&support).unwrap();
        repo_shaped(&support.join("loro-root"));
        compiler_at(&crate::provision::compiler_install_dir(&home));

        let (built, _) =
            locate_here(&CorpusPaths { home: Some(home.clone()), ..Default::default() }).unwrap();
        let (compiler, _) = built.unwrap();
        assert_eq!(compiler.tools().dir(), support.join("loro-root").join("loro"));
    }

    /// A corpus with no compiler is its OWN error, distinct from "no corpus" — because the
    /// two sentences the CEO is owed are different, and the boot line writes one of them.
    #[test]
    fn a_corpus_with_no_compiler_is_a_named_state_and_not_a_shrug() {
        let home = tmp("corpus-no-compiler");
        let support = home.join("Library").join("Application Support").join("RichOS");
        provisioned(&support.join("corpus"));

        match locate_here(&CorpusPaths { home: Some(home.clone()), ..Default::default() }) {
            Err(LoroError::CompilerNotInstalled { root, tried }) => {
                assert_eq!(root, support.join("corpus").display().to_string());
                assert!(tried.contains("loro-tools"), "it names where it looked: {tried}");
            }
            Err(other) => panic!("{other}"),
            Ok(_) => panic!("a corpus with no compiler must not resolve silently"),
        }
    }

    /// An explicit tools directory is exclusive in BOTH directions: it wins over a valid
    /// install, and a bad value is an error rather than a reason to search on.
    #[test]
    fn an_explicit_tools_directory_is_exclusive_and_a_bad_one_is_an_error() {
        let home = tmp("explicit-tools");
        let support = home.join("Library").join("Application Support").join("RichOS");
        provisioned(&support.join("corpus"));
        compiler_at(&crate::provision::compiler_install_dir(&home));

        let named = tmp("named-tools");
        compiler_at(&named);
        let (built, _) = locate_here(&CorpusPaths {
            env_tools: Some(named.display().to_string()),
            home: Some(home.clone()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(built.unwrap().0.tools().dir(), named);

        let bad = locate_here(&CorpusPaths {
            env_tools: Some("/nowhere/loro".into()),
            home: Some(home),
            ..Default::default()
        });
        assert!(matches!(bad, Err(LoroError::ToolsNotFound(_))), "an explicit bad value must not fall through");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req<'a>(entity: &'a str, topic: &'a str) -> SliceRequest<'a> {
        SliceRequest { thread_id: "t1", entity_id: entity, topic, budget_chars: 1200 }
    }

    fn compiler(lanes: &str) -> CliContextCompiler {
        CliContextCompiler::new(
            LoroTools { dir: PathBuf::from("/nowhere/loro"), node: "node".into() },
            LoroRoot::Corpus(PathBuf::from("/nowhere/corpus")),
            LaneMap::parse(lanes).unwrap(),
        )
    }

    fn slice_json(items: &str, thin: bool, text: &str) -> String {
        format!(
            r#"{{"schemaVersion":1,"compiler":"loro-context-compiler/1.2.0","thin":{thin},
                "coverage":"direct","text":{text:?},"items":[{items}],
                "corpus":{{"recordCount":3,"fingerprint":"sha256:abc","layout":"corpus","rootSource":"--corpus"}},
                "budget":{{"chars":1200,"usedChars":10,"itemsIncluded":1,"withheldByScope":0}},
                "notes":[]}}"#
        )
    }

    #[test]
    fn a_lane_map_is_explicit_and_a_typo_is_refused_rather_than_dropped() {
        let m = LaneMap::parse("femcboost=fb, richos=richos").unwrap();
        assert_eq!(m.lane_for("femcboost"), Some("fb"));
        assert_eq!(m.lane_for("richos"), Some("richos"));
        // Name equality is NOT a mapping. "deeply" is unmapped even though a `deeply`
        // partition might well exist — inferring it is the thing this type refuses to do.
        assert_eq!(m.lane_for("deeply"), None);
        assert!(LaneMap::parse("femcboost").is_err());
        assert!(LaneMap::parse("femcboost=").is_err());
        assert!(LaneMap::parse("=fb").is_err());
        assert!(LaneMap::parse("a=1,a=2").is_err(), "a doubly-mapped entity is ambiguous, not last-wins");
        assert!(LaneMap::parse("").unwrap().is_empty());
    }

    /// The `items[]` entry the compiler emits for a record promoted from the CEO's calendar.
    ///
    /// COPIED VERBATIM from a real `loro-context compile` run — every value here was produced by
    /// the pipeline, not composed to make a test pass. The run is
    /// `tools/richos-service/test/workspace-recall-e2e.mjs`, recorded at
    /// `docs/verification/2026-09-17-workspace-recall-end-to-end.md`. The long segment inside
    /// `provenance.ref` is the adapter's `sourceInstanceId`, which is what keeps two Google
    /// accounts on one machine from colliding on an event id.
    const WORKSPACE_ITEM: &str = r#"{
      "ref": "rec:ceo/unfiled/ws-google-calendar-2026-09-15-northwind-partnership-terms-81f6b3fc",
      "kind": "event",
      "kindInferred": false,
      "title": "Northwind partnership terms",
      "scope": "ceo-private",
      "company": null,
      "score": 1.1432,
      "truncated": true,
      "provenance": {
        "source": "records",
        "path": "ceo/unfiled/ws-google-calendar-2026-09-15-northwind-partnership-terms-81f6b3fc.md",
        "anchor": null,
        "link": null,
        "method": "rich_inferred",
        "ref": "google:calendar:d60daeaa8472855e083ff3950b3a702ef1bd94d0b3c0c760817325eb24ac1bbd:evt_partner",
        "origin": "workspace:google:calendar"
      },
      "confidence": 0.9
    }"#;

    #[test]
    fn a_slice_item_carries_the_provenance_that_lets_an_answer_say_where_it_came_from() {
        // The whole point of parsing this: an answer built on the CEO's own calendar must be
        // able to name the calendar and resolve the evidence. Before 2026-09-17 the compiler
        // sent all of this and the struct dropped it, so the best the app could cite was a
        // `rec:` path that names a file and says nothing about where the material came from.
        let slice: Slice = serde_json::from_str(&slice_json(WORKSPACE_ITEM, false, "COMPANY MEMORY (loro)")).unwrap();
        let p = &slice.items[0].provenance;
        assert_eq!(p.origin.as_deref(), Some("workspace:google:calendar"));
        assert_eq!(p.source, "records", "the loro FAMILY that read it is a different fact from its origin");
        assert_eq!(p.method.as_deref(), Some("rich_inferred"), "Rich inferred this; the CEO did not assert it");
        assert!(p.source_ref.as_deref().is_some_and(|r| r.starts_with("google:calendar:") && r.ends_with(":evt_partner")));
        assert_eq!(p.path.as_deref(),
            Some("ceo/unfiled/ws-google-calendar-2026-09-15-northwind-partnership-terms-81f6b3fc.md"));
        assert_eq!(slice.items[0].scope, "ceo-private", "a 1:1 with an outside party is the private perimeter");
        assert!(p.is_workspace());
        assert_eq!(p.workspace_source(), Some(("google", "calendar")));
        assert_eq!(slice.workspace_items().count(), 1);
    }

    #[test]
    fn an_ordinary_record_is_not_mistaken_for_a_workspace_one() {
        // The positive control for the test above. A build that answered `is_workspace()`
        // with `true` unconditionally would pass that one and be worse than useless: it
        // would put "from your Google Calendar" under the CEO's own written decisions.
        let wiki = r#"{"ref":"wiki:pricing.md#discounts","kind":"decision","kindInferred":true,
            "title":"Pricing — Discounts","scope":"org-shared","company":null,
            "provenance":{"source":"wiki","path":"wiki/pricing.md","anchor":"discounts","link":null,
            "method":null,"ref":null,"origin":null}}"#;
        let slice: Slice = serde_json::from_str(&slice_json(wiki, false, "COMPANY MEMORY (loro)")).unwrap();
        assert!(!slice.items[0].provenance.is_workspace());
        assert_eq!(slice.items[0].provenance.workspace_source(), None);
        assert_eq!(slice.workspace_items().count(), 0);
        assert_eq!(slice.items[0].provenance.anchor.as_deref(), Some("discounts"));
    }

    #[test]
    fn a_slice_from_an_older_compiler_with_no_provenance_parses_rather_than_failing() {
        // §2's forward-compat rule runs both ways: an item that predates the `origin` field
        // must still parse, and must report "I do not know where this came from" rather than
        // taking the build down or guessing.
        let bare = r#"{"ref":"rec:ceo/records/x","kind":"decision","title":"X","scope":"org-shared"}"#;
        let slice: Slice = serde_json::from_str(&slice_json(bare, false, "COMPANY MEMORY (loro)")).unwrap();
        assert_eq!(slice.items[0].provenance, SliceItemProvenance::default());
        assert!(!slice.items[0].provenance.is_workspace());
        assert_eq!(slice.items[0].provenance.origin, None);
    }

    #[test]
    fn provenance_reaches_the_record_a_correction_would_be_filed_against() {
        // The rendered line, also from the recorded run.
        let text = "COMPANY MEMORY (loro) — bearing on: \"what did I have on Tuesday\"\n\
            • [event] Northwind partnership terms — Calendar entry for Tuesday, September 15, 2026 at \
            3:00 PM (America/Los_Angeles). Source: your Google Calendar — \
            https://calendar.google.com/event?eid=evt_partner. \
            (ref: rec:ceo/unfiled/ws-google-calendar-2026-09-15-northwind-partnership-terms-81f6b3fc)";
        let slice: Slice = serde_json::from_str(&slice_json(WORKSPACE_ITEM, false, text)).unwrap();
        let records = slice.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].provenance.origin.as_deref(), Some("workspace:google:calendar"));
        assert_eq!(records[0].provenance.method.as_deref(), Some("rich_inferred"));
        assert!(records[0].is_supersedable(), "a promoted record is a rec: ref, so it can be corrected");
        assert!(records[0].line.is_some(), "the rendered line must be found by its own (ref: …) suffix");
    }

    #[test]
    fn the_workspace_origin_test_reads_the_stated_label_and_not_the_shape_of_a_ref() {
        // A prefix match on the REF would claim anything a future source happened to key
        // with a vendor name. The label is what the writer stated, so that is what is read.
        let mut p = SliceItemProvenance {
            source_ref: Some("google:calendar:evt_x".into()),
            origin: Some("conversation".into()),
            ..Default::default()
        };
        assert!(!p.is_workspace(), "a vendor-shaped ref is not a claim about where the record came from");
        p.origin = Some("workspace".into());
        assert!(p.is_workspace(), "the bare label is the source with no vendor named");
        assert_eq!(p.workspace_source(), None);
        p.origin = Some("workspace:microsoft:mail".into());
        assert_eq!(p.workspace_source(), Some(("microsoft", "mail")), "vendor-agnostic by construction");
    }

    #[test]
    fn the_compilers_argv_names_the_read_entry_point_and_carries_no_write_verb() {
        // The module's structural claim, asserted rather than promised: this type cannot
        // reach the writer, so wiring the read seam creates no write path through it.
        let c = compiler("femcboost=fb");
        let argv = c.argv(&req("femcboost", "how does pricing work"));
        assert!(argv[0].ends_with("bin/loro-context.mjs"), "{argv:?}");
        assert!(!argv.iter().any(|a| a.contains("loro-write")), "{argv:?}");
        for verb in ["append", "supersede", "correct", "create-company"] {
            assert!(!argv.iter().any(|a| a == verb), "argv must carry no write verb, found {verb}: {argv:?}");
        }
        assert!(argv.contains(&"--topic-stdin".to_string()), "the topic never goes through shell quoting");
        assert_eq!(argv.iter().filter(|a| *a == "--audience").count(), 1);
        assert!(argv.contains(&"rich".to_string()));
        // The mapped entity narrows to its lane...
        assert_eq!(argv.iter().position(|a| a == "--company").map(|i| argv[i + 1].clone()), Some("fb".into()));
        // ...and an UNMAPPED one never widens to "every company" by omission-plus-default.
        // It sends no --company at all, and the lane re-assertion below is what catches
        // anything the corpus then hands back.
        let wide = c.argv(&req("prospects", "how does pricing work"));
        assert!(!wide.contains(&"--company".to_string()), "{wide:?}");
    }

    /// A slice with two items, one of them the LAST line — the one the budget can truncate.
    fn two_item_slice(last_line_intact: bool) -> String {
        let items = concat!(
            r#"{"ref":"rec:ceo/records/ship-date","kind":"decision","kindInferred":false,"#,
            r#""title":"Ship date","scope":"org-shared","company":"fb"},"#,
            r#"{"ref":"mem:company:renewal","kind":"commitment","kindInferred":true,"#,
            r#""title":"Halstead renewal","scope":"ceo-private","company":"fb"}"#
        );
        let tail = if last_line_intact {
            "• [commitment?] Halstead renewal — renews in February. (ref: mem:company:renewal)"
        } else {
            "• [commitment?] Halstead renewal — renews in Feb"
        };
        slice_json(
            items,
            false,
            &format!(
                "COMPANY MEMORY (loro) — bearing on: \"the quarter\"\n\
                 • [decision] Ship date — We ship on Thursday. (ref: rec:ceo/records/ship-date)\n{tail}"
            ),
        )
    }

    /// INVARIANT: the provenance sink retains the ITEMS of an accepted slice, paired with
    /// the lines they were rendered as. This is the whole of what makes a proposal able to
    /// name a record, and every field it keeps is one `CorrectionDesk::propose` or the §7
    /// gate actually needs.
    #[test]
    fn an_accepted_slice_leaves_provenance_a_correction_can_resolve_against() {
        let mut c = compiler("femcboost=fb");
        let sink: SharedSliceProvenance = Arc::new(Mutex::new(SliceProvenance::new()));
        c.set_provenance_sink(Arc::clone(&sink));
        assert!(matches!(
            c.interpret(&two_item_slice(true), &req("femcboost", "the quarter")),
            LoroTier::Slice(_)
        ));

        let held = sink.lock().unwrap();
        let recs = held.records_for("t1");
        assert_eq!(recs.len(), 2, "{recs:?}");
        assert_eq!(recs[0].record_ref, "rec:ceo/records/ship-date");
        assert_eq!(recs[0].scope, "org-shared", "the scope a correction must carry through");
        assert!(!recs[0].kind_inferred);
        assert_eq!(
            recs[0].matchable(),
            "Ship date — We ship on Thursday.",
            "the kind label and the ref must not be matchable text"
        );
        assert!(recs[0].evidence().contains("(ref: rec:ceo/records/ship-date)"), "{:?}", recs[0].line);
        // A GUESSED kind is carried as a guess, never flattened into a declaration.
        assert!(recs[1].kind_inferred, "kindInferred was dropped");
        assert!(recs[1].is_supersedable(), "a mem: ref is supersedable");
        let injected = held.for_thread("t1").expect("recorded");
        assert_eq!(injected.topic, "the quarter");
        assert_eq!(injected.fingerprint, "sha256:abc");
    }

    /// INVARIANT: a slice REFUSED for any reason leaves NO provenance. A record retained
    /// from a refused slice could be resolved against later, filing a proposal that cites
    /// memory the CEO was never shown — which is the corruption this whole seam exists to
    /// avoid. Checked on the lane refusal, because that is the one a foreign company's
    /// memory arrives through.
    #[test]
    fn a_refused_slice_leaves_no_provenance_at_all() {
        let mut c = compiler("femcboost=fb");
        let sink: SharedSliceProvenance = Arc::new(Mutex::new(SliceProvenance::new()));
        c.set_provenance_sink(Arc::clone(&sink));
        let foreign = slice_json(
            r#"{"ref":"rec:y","kind":"decision","title":"t","scope":"org-shared","company":"northwind"}"#,
            false,
            "COMPANY MEMORY (loro) — bearing on: \"x\"\n• [decision] t (ref: rec:y)",
        );
        assert!(matches!(c.interpret(&foreign, &req("femcboost", "x")), LoroTier::Unavailable(_)));
        assert_eq!(sink.lock().unwrap().threads(), 0, "a refused slice was retained");

        // ...and so does an unsupported schema, which never reaches the lane guard at all.
        let future = two_item_slice(true).replace("\"schemaVersion\":1", "\"schemaVersion\":2");
        assert!(matches!(c.interpret(&future, &req("femcboost", "x")), LoroTier::Unavailable(_)));
        assert_eq!(sink.lock().unwrap().threads(), 0);
    }

    /// INVARIANT: an item whose rendered line lost its `(ref: …)` suffix to the budget cut
    /// keeps NO line rather than borrowing a neighbor's. `items[]` says what is in the
    /// slice; the text says what Rich read; when they disagree the resolver falls back to
    /// the title, which is a weaker match and not a wrong one.
    #[test]
    fn a_truncated_last_line_loses_its_line_and_never_borrows_another() {
        let mut c = compiler("femcboost=fb");
        let sink: SharedSliceProvenance = Arc::new(Mutex::new(SliceProvenance::new()));
        c.set_provenance_sink(Arc::clone(&sink));
        assert!(matches!(
            c.interpret(&two_item_slice(false), &req("femcboost", "the quarter")),
            LoroTier::Slice(_)
        ));
        let held = sink.lock().unwrap();
        let recs = held.records_for("t1");
        assert_eq!(recs[1].line, None, "a truncated line must not resolve to one");
        assert_eq!(recs[1].matchable(), "Halstead renewal", "and falls back to the title");
        assert!(recs[0].line.is_some(), "the intact line is unaffected");
    }

    /// INVARIANT: the writer cannot address `wiki:` or `entity:` refs, so neither may ever
    /// be proposed against. Asserted on the type rather than at the call site, because the
    /// call site is where it would be forgotten.
    #[test]
    fn only_the_refs_the_writer_can_address_are_supersedable() {
        let r = |rf: &str| SliceRecord {
            record_ref: rf.into(),
            kind: "decision".into(),
            kind_inferred: false,
            title: "t".into(),
            scope: "ceo-private".into(),
            company: None,
            provenance: Default::default(),
            line: None,
        };
        assert!(r("rec:ceo/records/x").is_supersedable());
        assert!(r("mem:company:x").is_supersedable());
        assert!(!r("wiki:loro-structure.md#the-human-surface").is_supersedable());
        assert!(!r("entity:halstead-group").is_supersedable());
    }

    #[test]
    fn a_slice_with_content_is_injected_verbatim() {
        let c = compiler("femcboost=fb");
        let json = slice_json(r#"{"ref":"rec:x","kind":"decision","title":"t","scope":"org-shared","company":"fb"}"#, false, "COMPANY MEMORY (loro) — bearing on: \"pricing\"\n• [decision] per seat");
        match c.interpret(&json, &req("femcboost", "pricing")) {
            LoroTier::Slice(t) => {
                assert!(t.starts_with("COMPANY MEMORY (loro)"), "{t}");
                assert!(!t.contains("RELEVANT COMPANY MEMORY"), "the heading must not be doubled");
            }
            other => panic!("expected a slice, got {other:?}"),
        }
    }

    #[test]
    fn the_ceo_layer_is_always_readable_because_ecs_says_it_is_in_the_default_read_set() {
        let c = compiler("femcboost=fb");
        let json = slice_json(r#"{"ref":"rec:p","kind":"principle","title":"t","scope":"ceo-private","company":null}"#, false, "COMPANY MEMORY (loro) — bearing on: \"x\"\n• [principle] p");
        assert!(c.interpret(&json, &req("femcboost", "x")).is_slice());
        // ...and it is readable from an entity that is mapped to no lane at all, which is
        // what makes an unratified decision 1.6 survivable rather than blocking.
        assert!(c.interpret(&json, &req("prospects", "x")).is_slice());
    }

    // ---- THE CROSS-ENTITY NEGATIVE CONTROL -------------------------------
    //
    // Remove the `foreign_lane` check in `interpret` and this test fails: the slice is
    // well-formed, exit 0, non-thin, inside budget, and carries another company's memory.

    #[test]
    fn a_slice_carrying_another_companys_memory_is_refused_whole() {
        let c = compiler("femcboost=fb,richos=rx");
        let json = slice_json(
            r#"{"ref":"rec:companies/rx/records/margin","kind":"decision","title":"margins","scope":"org-shared","company":"rx"}"#,
            false,
            "COMPANY MEMORY (loro) — bearing on: \"pricing\"\n• [decision] rx margins are 40%",
        );
        match c.interpret(&json, &req("femcboost", "pricing")) {
            LoroTier::Unavailable(why) => {
                assert!(why.contains("\"rx\""), "{why}");
                assert!(why.contains("rec:companies/rx/records/margin"), "{why}");
                assert!(why.contains("femcboost"), "{why}");
            }
            other => panic!("a cross-entity slice must be REFUSED, got {other:?}"),
        }
    }

    #[test]
    fn an_unmapped_entity_refuses_every_company_item_rather_than_accepting_all_of_them() {
        // The failure mode this guards: "no lane configured" must not read as "no
        // restriction". An unmapped entity may read the CEO layer and nothing else.
        let c = compiler("");
        let json = slice_json(
            r#"{"ref":"rec:companies/anything/records/x","kind":"fact","title":"t","scope":"org-shared","company":"anything"}"#,
            false,
            "COMPANY MEMORY (loro) — bearing on: \"x\"\n• [fact] t",
        );
        match c.interpret(&json, &req("femcboost", "x")) {
            LoroTier::Unavailable(why) => assert!(why.contains("mapped to no lane"), "{why}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_corpus_yields_loros_own_honest_line_and_never_a_fabricated_slice() {
        let c = compiler("femcboost=fb");
        let thin = "COMPANY MEMORY (loro): nothing recorded bears on \"pricing\". Do not assume company facts — ask the CEO or check a live system.";
        let json = slice_json("", true, thin);
        match c.interpret(&json, &req("femcboost", "pricing")) {
            LoroTier::NothingRecorded(t) => {
                assert_eq!(t, thin);
                assert!(t.contains("Do not assume company facts"));
            }
            other => panic!("expected NothingRecorded, got {other:?}"),
        }
    }

    #[test]
    fn a_thin_slice_with_no_text_still_says_something_honest_rather_than_falling_silent() {
        let c = compiler("femcboost=fb");
        match c.interpret(&slice_json("", true, ""), &req("femcboost", "pricing")) {
            LoroTier::NothingRecorded(t) => assert!(t.contains("nothing recorded bears on \"pricing\"")),
            other => panic!("expected NothingRecorded, got {other:?}"),
        }
    }

    #[test]
    fn an_unsupported_schema_version_degrades_rather_than_mis_parsing() {
        let c = compiler("femcboost=fb");
        let json = slice_json("", false, "text").replace("\"schemaVersion\":1", "\"schemaVersion\":2");
        match c.interpret(&json, &req("femcboost", "x")) {
            LoroTier::Unavailable(why) => assert!(why.contains("schemaVersion 2"), "{why}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn a_slice_over_its_own_budget_is_refused_because_the_ceo_pays_for_this_string() {
        let c = compiler("femcboost=fb");
        let long = "x".repeat(1201);
        let json = slice_json(r#"{"ref":"r","kind":"fact","title":"t","scope":"org-shared","company":null}"#, false, &long);
        match c.interpret(&json, &req("femcboost", "x")) {
            LoroTier::Unavailable(why) => assert!(why.contains("1201 chars against a 1200-char cap"), "{why}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn unknown_fields_are_ignored_so_a_ranking_change_does_not_break_this_consumer() {
        // §2's forward-compatibility rule, exercised: 1.2.0 ADDED five fields inside
        // schemaVersion 1, and the next ranker may add more.
        let c = compiler("femcboost=fb");
        let json = slice_json(r#"{"ref":"r","kind":"fact","title":"t","scope":"org-shared","company":null,"somethingNew":42}"#, false, "COMPANY MEMORY (loro) — bearing on: \"x\"\n• [fact] t")
            .replace("\"notes\":[]", "\"notes\":[],\"laneDiagnostics\":{\"whatever\":true}");
        assert!(c.interpret(&json, &req("femcboost", "x")).is_slice());
    }

    #[test]
    fn unparseable_stdout_never_fails_the_turn() {
        let c = compiler("femcboost=fb");
        assert!(matches!(c.interpret("not json at all", &req("femcboost", "x")), LoroTier::Unavailable(_)));
    }

    #[test]
    fn an_empty_topic_is_refused_before_a_process_is_ever_started() {
        // Not a corpus question — a caller bug. Asking anyway exits 2 on every rotation.
        let c = compiler("femcboost=fb");
        match c.compile_slice(&req("femcboost", "   ")) {
            LoroTier::Unavailable(why) => assert!(why.contains("no topic"), "{why}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    #[test]
    fn tools_must_hold_both_entry_points_or_they_are_not_a_loro_checkout() {
        let dir = std::env::temp_dir().join(format!("loro-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        assert!(LoroTools::locate(&dir).is_err(), "an empty bin/ is not a loro checkout");
        std::fs::write(dir.join("bin").join("loro-context.mjs"), "//").unwrap();
        assert!(LoroTools::locate(&dir).is_err(), "the read half alone is not enough to be sure");
        std::fs::write(dir.join("bin").join("loro-write.mjs"), "//").unwrap();
        assert!(LoroTools::locate(&dir).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
