//! ENTITY — the hard scope and privacy boundary (ECS architecture §3.2–3.4).
//!
//! An entity is a company, division, fund, book or venture. The UX brief §1 is explicit
//! that it is *"also a hard scope and privacy boundary"* rather than a folder, and §22's
//! "Must not be faked" list names **cross-entity context** directly. So this module's job
//! is not to carry a label around; it is to make the ECS scope key
//!
//! ```text
//! person_id + entity_id + thread_id + binding_revision + turn_id + audience
//! ```
//!
//! real rather than advisory. Slice 1 implements every component of that key except
//! `audience`: there is exactly one audience today (the CEO), the worker slice that
//! introduces a second one is a later leg, and a field that never differentiates anything
//! would be decoration, not enforcement. It is named here so nobody has to rediscover the
//! omission.
//!
//! Three rules from ECS §3.2–3.4 are load-bearing here and are enforced, not documented:
//!
//! 1. **Every thread has exactly one home entity, immutable after creation.** Moving a
//!    conversation to another entity creates a NEW thread; it never rebinds. That is why
//!    [`ThreadBinding`] has private fields, no setter and no `&mut` accessor, and why its
//!    constructor is crate-private — the only way to *obtain* one outside this crate is
//!    [`crate::ledger::Ledger::thread_binding`], which fills the entity in from the
//!    durable record. A caller cannot fabricate a binding that says something the ledger
//!    does not.
//! 2. **`entity_id` and `thread_id` are required before retrieval or mutation.** There is
//!    no unscoped read path and no unscoped write path (see `ledger.rs`).
//! 3. **An unknown or ambiguous repository root fails.** [`EntityRegistry::resolve_root`]
//!    never defaults to the last entity, the first entity or all entities.
//! 4. **The registry belongs to the person running the app, and to nobody else.** It is
//!    read from [`ENTITY_REGISTRY_FILENAME`] in this install's own configuration directory
//!    and it is EMPTY until that person puts something in it — see [`EntityRegistry::load`].
//!    Rule 4 is younger than the other three and it exists because the other three were
//!    enforced against a `const` table of one man's six companies, bound to absolute roots
//!    under his own home directory. On every machine that was not his, that table was
//!    simultaneously a privacy leak (his company list and his home path rendered in the
//!    company picker of every install) and a wall: no path a second person worked in was
//!    ever registered, so root resolution refused correctly and forever, and the only way
//!    through the picker was to file his work under one of another man's companies.
//!
//! **An empty registry is a legal state, not an error.** It resolves nothing, it refuses
//! every root, and it is what a first launch has. The app's job in that state is to ask —
//! `entity_choice`/`register_entity` in the shell — never to invent an answer.

use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

/// The single CEO layer this app serves (ECS §3.2 `person: id: ceo-default`). RichOS
/// v1 is deliberately one CEO on one machine, so this is a constant rather than a table —
/// but the scope key carries it explicitly so a second person is a data change, not a
/// re-architecture.
///
/// **Why the prose says "CEO layer" while the identifier stays `PERSON_DEFAULT`.** The CEO
/// ruled on 2026-09-01 (`richos-hq/wiki/ceo-decisions.md` §5) that the corpus layer holding
/// his own synthesis is named `ceo/`, and the prose term followed it everywhere. The ECS
/// scope KEY did not, and deliberately: `person_id` is a persisted field in an append-only
/// event store (`ledger.rs`, and femcboost's ECS adapters read it too), so renaming it is a
/// cross-repo data migration rather than a rename. It also still means the right thing —
/// here `person` contrasts with `entity`, naming the human principal rather than the layer.
pub const PERSON_DEFAULT: &str = "ceo-default";

/// Max length of an entity id. Bounded because an entity id reaches the filesystem (the
/// machinery journal shards per thread today, per entity tomorrow) and an unbounded,
/// unvalidated id is a path-traversal seam waiting to happen.
pub const ENTITY_ID_MAX_LEN: usize = 64;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EntityError {
    #[error("invalid entity id {0:?}: must be 1..={max} chars of [a-z0-9-], starting with [a-z0-9]", max = ENTITY_ID_MAX_LEN)]
    InvalidId(String),
    #[error("duplicate entity id in registry: {0}")]
    DuplicateId(String),
    #[error("entity is not registered: {0}")]
    UnknownId(String),
    #[error("connected repository must be an explicit registered root, or one another company already connected: {0}")]
    InvalidRepository(String),
    /// A company with no name renders as an empty button. Refused at the door rather than
    /// stored and discovered by the person looking at a blank row.
    #[error("entity {0} has an empty display name")]
    EmptyDisplayName(String),
    /// Resolution is LEXICAL and requires an absolute path (see [`EntityRegistry::resolve_root`]),
    /// so a relative root can never match anything. Storing one would be a dead entry that
    /// looks exactly like a working one — the failure this whole module is built against.
    #[error("entity {id}: root {root} is not absolute — a relative root can never match")]
    RootNotAbsolute { id: String, root: PathBuf },
    /// Two entities whose roots contain one another make every path under the inner one
    /// [`EntityResolveError::AmbiguousRoot`], which blocks the turn (ECS §10.2). That is the
    /// correct behavior at RESOLUTION time and a terrible thing to discover then, so
    /// [`EntityRegistry::register`] refuses to create the condition in the first place.
    #[error("entity {id}: root {root} overlaps {other}'s root {other_root}")]
    OverlappingRoot { id: String, root: PathBuf, other: String, other_root: PathBuf },
}

/// A validated entity identifier (`femcboost`, `deeply`, `prospects`, `richos`).
///
/// Newtype rather than `String` so an entity id can never be confused with a thread id,
/// a title or a display name at a call site — the compiler enforces the distinction that
/// the privacy boundary depends on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntityId(String);

impl EntityId {
    /// Validate and construct. The character class is deliberately narrow: lowercase
    /// ASCII, digits and `-`. No `/`, no `.`, no `..`, no whitespace, no uppercase —
    /// so an id is safe as a path component and two ids can never differ only by case.
    pub fn parse(s: &str) -> Result<Self, EntityError> {
        let ok = !s.is_empty()
            && s.len() <= ENTITY_ID_MAX_LEN
            && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && s.chars().next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
        if ok {
            Ok(EntityId(s.to_string()))
        } else {
            Err(EntityError::InvalidId(s.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EntityId {
    type Error = EntityError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        EntityId::parse(&s)
    }
}

impl From<EntityId> for String {
    fn from(e: EntityId) -> String {
        e.0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The person a scope belongs to (ECS §3.4: an aggregate or event must carry either an
/// explicit `person_id` or an `entity_id`; an entity object carries both).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonId(String);

impl PersonId {
    pub fn new(s: &str) -> Self {
        PersonId(s.to_string())
    }

    pub fn default_ceo() -> Self {
        PersonId(PERSON_DEFAULT.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for PersonId {
    fn default() -> Self {
        PersonId::default_ceo()
    }
}

impl std::fmt::Display for PersonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether this entity area is SELECTABLE in this install — registration state, and
/// nothing more.
///
/// **It is not the company's lifecycle status.** `loro-structure.md`'s `company.yaml`
/// carries a `status` (and a `role`, and a `startedAt`) describing the venture itself:
/// whether the CEO still runs it, in what capacity, and since when. The CEO has not
/// stated any of those, Rich collects them in conversation, and this enum must never be
/// mistaken for an answer to them — a registry that invented `role: founder` would be
/// asserting a fact about his life from a directory listing, which is the exact defect
/// [`EntityRegistry::ceos_companies`] exists to end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    Active,
    Archived,
}

/// `active`, so a hand-written registry row may leave `status` out entirely. Registering
/// something is the act of saying it is selectable; the other value has to be asked for.
impl Default for EntityStatus {
    fn default() -> Self {
        EntityStatus::Active
    }
}

/// One entity area and the repository roots that deterministically select it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub display_name: String,
    pub status: EntityStatus,
    /// Absolute repository roots owned by this entity (ECS §10.2). Empty is legal — an
    /// entity with no root simply cannot be selected by root resolution.
    pub roots: Vec<PathBuf>,
    /// Exact roots explicitly connected for app execution. A company folder alone is not consent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connected_repositories: Vec<PathBuf>,
}

impl Entity {
    /// Convenience constructor for literal roots. Validating — see [`Entity::try_new`].
    pub fn new(id: &str, display_name: &str, roots: &[&str]) -> Result<Self, EntityError> {
        Entity::try_new(id, display_name, roots.iter().map(PathBuf::from).collect())
    }

    /// Build a validated entity from owned roots — the path a config file row and a UI
    /// registration both take.
    ///
    /// It rejects three things a `String` field cannot: an id that is not a safe path
    /// component, an empty display name, and a relative root. The third is the one worth
    /// naming: `resolve_root` skips a non-absolute root silently (it must — resolution is
    /// lexical), so an unvalidated relative root produces an entity that is registered,
    /// listed, pickable and permanently unresolvable from its own folder.
    pub fn try_new(id: &str, display_name: &str, roots: Vec<PathBuf>) -> Result<Self, EntityError> {
        let id = EntityId::parse(id)?;
        let display_name = display_name.trim().to_string();
        if display_name.is_empty() {
            return Err(EntityError::EmptyDisplayName(id.to_string()));
        }
        for root in &roots {
            if !root.is_absolute() {
                return Err(EntityError::RootNotAbsolute { id: id.to_string(), root: root.clone() });
            }
        }
        Ok(Entity { id, display_name, status: EntityStatus::Active, roots, connected_repositories: Vec::new() })
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EntityResolveError {
    /// ECS §3.3: "An unknown or ambiguous root fails closed. It never defaults to the
    /// last company, the first company or all companies."
    #[error("unknown root {0}: no registered entity owns this path — refusing to guess")]
    UnknownRoot(PathBuf),
    /// ECS §10.2: "A path that maps to two entities blocks the turn."
    #[error("ambiguous root {path}: maps to {candidates:?} — refusing to guess")]
    AmbiguousRoot { path: PathBuf, candidates: Vec<EntityId> },
    /// Resolution is purely lexical (no filesystem access, so it is testable and cannot
    /// be perturbed by a symlink), which requires an absolute, normalized path.
    #[error("root {0} is not absolute — resolution is lexical and requires an absolute path")]
    NotAbsolute(PathBuf),
}

/// The entity registry — **the entity areas the person running this install registered**,
/// read from their own configuration file (see [`EntityRegistry::load`]).
///
/// # It was a `const` table of one man's companies, and that was the bug
///
/// Until 2026-09-04 this type carried `CEOS_COMPANIES`: six rows naming the CEO's real
/// companies, each bound to an absolute root under the author's own home directory, and
/// `impl Default` returned them, so that table WAS the shipping registry. The doc argued
/// the case for it — a registry is a privacy boundary, and a file that can be missing or
/// stale is a boundary that can move without anybody deciding to move it.
///
/// The argument was sound and the conclusion was wrong, because it was reasoning about one
/// machine. On every other machine the same table did two things at once:
///
///   * it **published a private list**. The company picker renders `display_name` and
///     `roots` for every registered entity, so a second person opening RichOS was shown six
///     companies that are not his and the absolute path of a home directory that is not his.
///   * it **locked the app**. No path a second person works in is a registered root, so
///     `resolve_root` refused (correctly — rule 3), the picker's only answers were another
///     man's companies, and the honest states available to him were "refuse every send" or
///     "file my work under FemcBoost".
///
/// The boundary is not weakened by moving it into a file. It is the SAME boundary, owned by
/// the person it protects: [`Self::load`] never invents a registry, an unreadable file
/// yields an EMPTY registry rather than a guessed one, and [`Self::resolve_root`] still
/// refuses everything it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EntityRegistry {
    entities: Vec<Entity>,
}

impl EntityRegistry {
    pub fn new(entities: Vec<Entity>) -> Result<Self, EntityError> {
        for (i, e) in entities.iter().enumerate() {
            if entities[..i].iter().any(|prev| prev.id == e.id) {
                return Err(EntityError::DuplicateId(e.id.to_string()));
            }
        }
        Ok(EntityRegistry { entities })
    }

    /// **Invented data, for tests and examples only. Nothing in the app reaches this.**
    ///
    /// Three companies that belong to nobody, at paths that exist on no machine. It is
    /// deliberately shaped like the hardest real registry the resolver has ever had to
    /// answer for, so the properties that used to be pinned against the CEO's own six are
    /// still pinned against something:
    ///
    ///   * `harbor` owns **two roots and is one entity** — a second root on the same entity
    ///     must not read as ambiguity;
    ///   * `harbor-private` is a STRING prefix match of `harbor`'s first root and resolves
    ///     to `harbor` only because it is registered as its own root, not because the
    ///     component matcher was fooled;
    ///   * every root is absolute and no root is shared, so nothing here is ambiguous.
    ///
    /// It is `pub` because the integration tests in `tests/` compile against the library
    /// rather than inside it and cannot see a `#[cfg(test)]` item. It is not `Default`, it
    /// is not reachable from any boot path, and a grep for it should only ever find tests,
    /// examples and this comment.
    pub const FIXTURE_COMPANIES: &'static [(&'static str, &'static str, &'static [&'static str])] = &[
        ("northwind", "Northwind Traders", &["/Users/example/Projects/northwind"]),
        (
            "harbor",
            "Harbor Analytics",
            &["/Users/example/Projects/harbor", "/Users/example/Projects/harbor-private"],
        ),
        ("lumen", "Lumen Labs", &["/Users/example/Projects/lumen"]),
    ];

    /// [`Self::FIXTURE_COMPANIES`], validated. **Test fixture — never a default.**
    pub fn fixture() -> Self {
        EntityRegistry::new(
            Self::FIXTURE_COMPANIES
                .iter()
                .map(|(id, name, roots)| Entity::new(id, name, roots).expect("a fixture entity id is valid"))
                .collect(),
        )
        .expect("fixture entity ids are distinct")
    }

    /// A registry with nothing in it. **A legal state**, and the state of every first
    /// launch: it resolves nothing, contains nothing, and refuses every root.
    pub fn empty() -> Self {
        EntityRegistry { entities: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Add one entity, refusing anything that would make the registry lie later.
    ///
    /// **This is the door the app registers through, so it checks more than [`Self::new`]
    /// does.** `new` accepts overlapping roots because overlap is a real state a
    /// hand-written file can be in and [`Self::resolve_root`] has to keep failing closed on
    /// it (the `AmbiguousRoot` path is tested, and must stay reachable). But a person adding
    /// a folder in the window must not be able to CREATE that state by accident and then
    /// discover it as "Rich refuses to file anything under this company" a week later. So
    /// registration refuses:
    ///
    ///   * a duplicate id;
    ///   * a root that contains, or is contained by, a root already registered to another
    ///     entity — in either direction, because either direction makes some path ambiguous.
    pub fn connect_repository(&mut self, id: &EntityId, root: PathBuf) -> Result<(), EntityError> {
        let mut entity = self.get(id).cloned().ok_or_else(|| EntityError::UnknownId(id.to_string()))?;
        if entity.connected_repositories.contains(&root) { return Ok(()); }
        // A repository already connected (and owned) by another company is SHARED, not
        // re-owned: `roots` stays exclusive so a path still resolves to one company, while
        // `connected_repositories` may overlap so both companies can work in it.
        let shared = self.entities.iter().any(|e| &e.id != id && owns_connected(e, &root));
        if !shared && !entity.roots.contains(&root) { entity.roots.push(root.clone()); }
        entity.connected_repositories.push(root);
        // Validate against the other entities before changing the live registry.
        let mut others = Self::new(self.entities.iter().filter(|e| &e.id != id).cloned().collect())?;
        others.register(entity.clone())?;
        *self.entities.iter_mut().find(|e| &e.id == id).unwrap() = entity;
        Ok(())
    }

    pub fn register(&mut self, entity: Entity) -> Result<(), EntityError> {
        if entity.connected_repositories.iter().any(|p| !entity.roots.contains(p) && !self.entities.iter().any(|e| owns_connected(e, p))) {
            return Err(EntityError::InvalidRepository(entity.id.to_string()));
        }
        if self.contains(&entity.id) {
            return Err(EntityError::DuplicateId(entity.id.to_string()));
        }
        for root in &entity.roots {
            if !root.is_absolute() {
                return Err(EntityError::RootNotAbsolute { id: entity.id.to_string(), root: root.clone() });
            }
            for other in &self.entities {
                for other_root in &other.roots {
                    if contains_path(root, other_root) || contains_path(other_root, root) {
                        return Err(EntityError::OverlappingRoot {
                            id: entity.id.to_string(),
                            root: root.clone(),
                            other: other.id.to_string(),
                            other_root: other_root.clone(),
                        });
                    }
                }
            }
        }
        self.entities.push(entity);
        Ok(())
    }

    /// Rebuild a registry from entity ids that ALREADY OWN durable records on this machine.
    ///
    /// **The no-orphan migration** (and the only place a registry is ever derived rather
    /// than stated). An install that ran while the registry was compiled in has a ledger
    /// full of threads bound to `femcboost`, `richos` and the rest; if the registry file is
    /// absent on the next launch, every one of those ids stops being registered and every
    /// scoped read and write against those threads fails closed. The threads are not lost —
    /// they are unreachable, which for the person reading the screen is the same thing.
    ///
    /// So this asserts exactly one fact, and it is a fact about the machine it is running
    /// on rather than a claim about anybody's business: **these ids already own records
    /// here.** It does NOT invent roots — an entity restored this way has none, cannot be
    /// selected by root resolution, and is honest about it until somebody adds one. The
    /// display name is derived from the id ([`display_name_from_id`]) because a label is the
    /// one thing an id can honestly supply, and the file is editable.
    pub fn from_existing_ids(ids: &[EntityId]) -> Self {
        let mut registry = EntityRegistry::empty();
        for id in ids {
            if registry.contains(id) {
                continue;
            }
            registry.entities.push(Entity {
                id: id.clone(),
                display_name: display_name_from_id(id.as_str()),
                status: EntityStatus::Active,
                roots: Vec::new(),
                connected_repositories: Vec::new(),
            });
        }
        registry
    }

    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    pub fn get(&self, id: &EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| &e.id == id)
    }

    pub fn contains(&self, id: &EntityId) -> bool {
        self.get(id).is_some()
    }

    /// Deterministically select the entity that owns `path` (ECS §3.3/§10.2).
    ///
    /// Matching is by PATH COMPONENT, never by string prefix — `/Projects/harb` must
    /// not match the root `/Projects/harbor`, and a naive `starts_with` on the string
    /// would say it does. Zero matches is [`EntityResolveError::UnknownRoot`]; matches
    /// spanning more than one entity is [`EntityResolveError::AmbiguousRoot`]. Neither
    /// falls back to anything.
    ///
    /// A worktree must be resolved by the CALLER through its Git common directory before
    /// arriving here (ECS §10.2: "Worktrees resolve through their Git common directory or
    /// registered parent repo, never by path-name guessing"). A worktree path that happens
    /// to sit under a registered root resolves correctly by containment; one that does not
    /// fails closed rather than guessing, which is the intended behavior.
    pub fn resolve_root(&self, path: &Path) -> Result<&Entity, EntityResolveError> {
        if !path.is_absolute() {
            return Err(EntityResolveError::NotAbsolute(path.to_path_buf()));
        }
        let target: Vec<Component> = path.components().collect();
        let mut hits: Vec<&Entity> = Vec::new();
        for entity in &self.entities {
            let owns = entity.roots.iter().any(|root| {
                let root_parts: Vec<Component> = root.components().collect();
                root.is_absolute()
                    && root_parts.len() <= target.len()
                    && root_parts.iter().zip(target.iter()).all(|(a, b)| a == b)
            });
            if owns {
                hits.push(entity);
            }
        }
        match hits.len() {
            0 => Err(EntityResolveError::UnknownRoot(path.to_path_buf())),
            1 => Ok(hits[0]),
            _ => Err(EntityResolveError::AmbiguousRoot {
                path: path.to_path_buf(),
                candidates: hits.iter().map(|e| e.id.clone()).collect(),
            }),
        }
    }
}

// =======================================================================================
// THE REGISTRY FILE — one person's companies, on that person's machine
// =======================================================================================
//
// Format, path and an example are documented for a human in `docs/entity-registry.md`, and
// the example there is [`EXAMPLE_ENTITY_REGISTRY_JSON`] below rather than a second copy, so
// the documentation cannot drift from what the parser accepts (a test parses it).

/// The registry's file name, inside this install's own configuration directory — beside
/// `config.json`, same directory, same durability posture.
///
/// On macOS that directory is `~/Library/Application Support/com.richos.app/`, so the file
/// is `~/Library/Application Support/com.richos.app/entities.json`. The app prints the full
/// resolved path at boot, because a documented path that the reader has to assemble from two
/// halves is a path somebody will assemble wrongly.
pub const ENTITY_REGISTRY_FILENAME: &str = "entities.json";

/// Writes version 2. Reads legacy version 1 without granting repository execution.
pub const ENTITY_REGISTRY_VERSION: u32 = 2;

/// The documented example, verbatim, as the parser sees it.
///
/// It is a `const` in the source rather than a fenced block in a Markdown file because a
/// documented example that no test runs is a promise nobody kept. `example_in_the_docs_parses`
/// parses this exact string.
pub const EXAMPLE_ENTITY_REGISTRY_JSON: &str = r#"{
  "version": 1,
  "entities": [
    {
      "id": "northwind",
      "display_name": "Northwind Traders",
      "roots": ["/Users/example/Projects/northwind"]
    },
    {
      "id": "harbor",
      "display_name": "Harbor Analytics",
      "status": "active",
      "roots": [
        "/Users/example/Projects/harbor",
        "/Users/example/Projects/harbor-private"
      ]
    }
  ]
}
"#;

/// The registry file inside a configuration directory.
pub fn entity_registry_path(config_dir: &Path) -> PathBuf {
    config_dir.join(ENTITY_REGISTRY_FILENAME)
}

/// This install's configuration directory, derived from a home directory.
///
/// **The shell is the authority, not this function.** `src-tauri` resolves the directory
/// with Tauri's `app_data_dir()` and prints the resolved registry path at every boot, which
/// is the value a person should trust. This mirrors that resolution for callers that have no
/// webview to ask — the headless examples, the documentation, and anything diagnosing an
/// install from a terminal — so those three do not each carry their own copy of a path.
///
/// The identifier is `com.richos.app` (`app/src-tauri/tauri.conf.json`), and RichOS v1 ships
/// on macOS only, so the macOS layout is the one that is written out. The other arms exist so
/// a headless caller on another platform gets something plausible rather than a macOS path
/// under a Linux home, and they are NOT a claim that RichOS runs there.
pub fn app_config_dir(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library").join("Application Support").join("com.richos.app")
    }
    #[cfg(target_os = "windows")]
    {
        home.join("AppData").join("Roaming").join("com.richos.app")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home.join(".config").join("com.richos.app")
    }
}

/// Where the registry in force came from. Three states, and the third is why this is an
/// enum rather than a `bool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrySource {
    /// No file at that path. The ordinary first-run state: the registry is empty, nothing
    /// is wrong, and the app asks.
    Absent,
    /// Read and parsed.
    File,
    /// The file is there and could not be trusted — unreadable, malformed, or semantically
    /// invalid. The registry is EMPTY.
    ///
    /// **Not folded into `Absent`, and the difference is the whole point.** "You have not
    /// told me your companies yet" and "you told me and I could not read it" call for
    /// opposite responses: the first is a question, the second is a repair, and an app that
    /// answered the second with the first would quietly invite somebody to re-enter a list
    /// that is already sitting on disk one typo away from working.
    Unreadable,
}

impl RegistrySource {
    /// The wire string, for a UI that renders the three states differently.
    pub fn as_str(&self) -> &'static str {
        match self {
            RegistrySource::Absent => "absent",
            RegistrySource::File => "file",
            RegistrySource::Unreadable => "unreadable",
        }
    }
}

/// The outcome of [`EntityRegistry::load`]: what was loaded, where from, and every line an
/// operator should see about it.
///
/// `notes` rather than `eprintln!` inside the loader, for the reason `BootEntity` in the
/// shell keeps its own: the ORDER and CONTENT of what gets said is then a pure function of
/// the inputs, and can be asserted by a test rather than read off a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryLoad {
    pub registry: EntityRegistry,
    pub source: RegistrySource,
    pub path: PathBuf,
    pub notes: Vec<String>,
}

impl RegistryLoad {
    /// True when a file exists and could not be trusted — the state that calls for a repair
    /// rather than a question.
    pub fn is_unreadable(&self) -> bool {
        self.source == RegistrySource::Unreadable
    }
}

/// The file, as serde sees it.
///
/// A DTO rather than `Vec<Entity>` directly, for two reasons that both cost something to
/// skip: `deny_unknown_fields` turns `"root"` (a plausible typo for `"roots"`) into a named
/// parse error instead of an entity that is registered, listed, pickable and silently
/// unresolvable; and validation errors can name the ROW rather than surfacing serde's
/// message about a `TryFrom<String>` nobody has heard of.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFile {
    version: u32,
    #[serde(default)]
    entities: Vec<RegistryFileRow>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryFileRow {
    id: String,
    display_name: String,
    /// Omittable — [`EntityStatus::default`] is `active`.
    #[serde(default)]
    status: EntityStatus,
    /// Omittable. An entity with no root is legal: it cannot be selected by launching from
    /// a folder, and it can still be chosen in the picker and own threads. That is exactly
    /// what [`EntityRegistry::from_existing_ids`] produces.
    #[serde(default)]
    roots: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    connected_repositories: Vec<PathBuf>,
}

/// True when `entity` both owns `path` as a root and has it connected: the one company a
/// shared repository belongs to for deciding which company a path is in.
fn owns_connected(entity: &Entity, path: &Path) -> bool {
    entity.roots.iter().any(|r| r == path) && entity.connected_repositories.iter().any(|r| r == path)
}

impl EntityRegistry {
    /// Read the registry from `path`. **Never panics, never errors, never guesses.**
    ///
    /// Every failure mode collapses to the same registry — the EMPTY one — and differs only
    /// in what it SAYS:
    ///
    /// | on disk | source | registry |
    /// |---|---|---|
    /// | nothing | `Absent` | empty |
    /// | unreadable / not JSON / unknown field / unknown version | `Unreadable` | empty |
    /// | a row with a bad id, an empty name, a relative root, or a duplicate id | `Unreadable` | empty |
    /// | a valid document | `File` | its entities, in file order |
    ///
    /// **A bad row fails the WHOLE file rather than being dropped.** Dropping it would move
    /// the privacy boundary without anybody deciding to move it: an entity would silently
    /// stop existing, its threads would stop resolving, and the app would look like it was
    /// working. One typo making the app say "I could not read your company list, here is the
    /// problem" is recoverable; one typo making a company disappear is not noticeable.
    pub fn load(path: &Path) -> RegistryLoad {
        let mut notes = Vec::new();
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                notes.push(format!(
                    "no company registry at {} — this install has registered no companies yet",
                    path.display()
                ));
                return RegistryLoad {
                    registry: EntityRegistry::empty(),
                    source: RegistrySource::Absent,
                    path: path.to_path_buf(),
                    notes,
                };
            }
            Err(e) => {
                notes.push(format!("company registry at {} could not be read: {e}", path.display()));
                return RegistryLoad {
                    registry: EntityRegistry::empty(),
                    source: RegistrySource::Unreadable,
                    path: path.to_path_buf(),
                    notes,
                };
            }
        };

        let unreadable = |notes: Vec<String>| RegistryLoad {
            registry: EntityRegistry::empty(),
            source: RegistrySource::Unreadable,
            path: path.to_path_buf(),
            notes,
        };

        let file: RegistryFile = match serde_json::from_str(&raw) {
            Ok(f) => f,
            Err(e) => {
                notes.push(format!(
                    "company registry at {} is not valid: {e}. Nothing was loaded from it — \
                     no company is registered until it parses.",
                    path.display()
                ));
                return unreadable(notes);
            }
        };
        if file.version != 1 && file.version != ENTITY_REGISTRY_VERSION {
            notes.push(format!(
                "company registry at {} is version {}, and this build reads version {} — \
                 refusing it rather than reading it as something it is not",
                path.display(),
                file.version,
                ENTITY_REGISTRY_VERSION
            ));
            return unreadable(notes);
        }

        let mut registry = EntityRegistry::empty();
        for (i, row) in file.entities.iter().enumerate() {
            let shared_elsewhere = |p: &PathBuf| file.entities.iter().enumerate()
                .any(|(j, o)| j != i && o.roots.contains(p) && o.connected_repositories.contains(p));
            if (file.version == 1 && !row.connected_repositories.is_empty()) || row.connected_repositories.iter().any(|p| !row.roots.contains(p) && !shared_elsewhere(p)) {
                notes.push("connected repositories require registry version 2 and an exact owned root".into());
                return unreadable(notes);
            }
            let entity = match Entity::try_new(&row.id, &row.display_name, row.roots.clone()) {
                Ok(mut e) => {
                    e.status = row.status;
                    e.connected_repositories = row.connected_repositories.clone();
                    e
                }
                Err(e) => {
                    notes.push(format!(
                        "company registry at {}: entry {} is not usable ({e}). Nothing was \
                         loaded from the file.",
                        path.display(),
                        i + 1
                    ));
                    return unreadable(notes);
                }
            };
            // `new`-level validation, not `register`-level: a file MAY describe overlapping
            // roots, and `resolve_root` still has to fail closed on them (ECS §10.2). What a
            // file may not do is name the same company twice.
            if registry.contains(&entity.id) {
                notes.push(format!(
                    "company registry at {}: {} is listed twice. Nothing was loaded from the file.",
                    path.display(),
                    entity.id
                ));
                return unreadable(notes);
            }
            registry.entities.push(entity);
        }

        notes.push(format!(
            "company registry: {} compan{} from {}",
            registry.len(),
            if registry.len() == 1 { "y" } else { "ies" },
            path.display()
        ));
        RegistryLoad { registry, source: RegistrySource::File, path: path.to_path_buf(), notes }
    }

    /// Write the registry to `path`, atomically.
    ///
    /// Temp file in the SAME directory, fsync, then rename — so a crash or a full disk
    /// leaves the previous registry intact rather than a truncated one. A half-written
    /// registry is a moved privacy boundary, which is the one outcome this file may not
    /// have.
    ///
    /// Mode `0600` on unix: it names the person's companies and the folders they live in.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Retain the exact legacy registry before the first schema-2 write.
        // Older app builds cannot read execution grants and must not silently drop them.
        match std::fs::read(path) {
            Ok(previous) => {
                if serde_json::from_slice::<serde_json::Value>(&previous).ok().is_some_and(|v| v["version"] == 1) {
                    let backup = path.with_extension(format!("v1-{}.backup.json", uuid::Uuid::new_v4()));
                    let mut options = std::fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
                    let mut file = options.open(backup)?;
                    file.write_all(&previous)?; file.sync_all()?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let file = RegistryFile {
            version: ENTITY_REGISTRY_VERSION,
            entities: self
                .entities
                .iter()
                .map(|e| RegistryFileRow {
                    id: e.id.to_string(),
                    display_name: e.display_name.clone(),
                    status: e.status,
                    roots: e.roots.clone(),
                    connected_repositories: e.connected_repositories.clone(),
                })
                .collect(),
        };
        let mut body = serde_json::to_string_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        body.push('\n');

        let tmp = path.with_extension(format!("{}.incoming", uuid::Uuid::new_v4()));
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
            let mut f = options.open(&tmp)?;
            f.write_all(body.as_bytes())?;
            f.sync_all()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)?;
        if let Some(parent) = path.parent() { std::fs::File::open(parent)?.sync_all()?; }
        Ok(())
    }
}

/// Lexical containment: does `outer` contain `inner` (or equal it), by PATH COMPONENT?
///
/// The same comparison [`EntityRegistry::resolve_root`] makes, extracted so registration and
/// resolution cannot disagree about what "inside" means. String prefixes are not used here
/// for the reason they are not used there: `/a/b/rich` is a string prefix of `/a/b/richos`
/// and is not inside it.
fn contains_path(outer: &Path, inner: &Path) -> bool {
    let outer_parts: Vec<Component> = outer.components().collect();
    let inner_parts: Vec<Component> = inner.components().collect();
    outer_parts.len() <= inner_parts.len() && outer_parts.iter().zip(inner_parts.iter()).all(|(a, b)| a == b)
}

/// A readable label from an entity id: `gpt-exporter` -> `Gpt Exporter`.
///
/// Used ONLY by [`EntityRegistry::from_existing_ids`], and it is deliberately dumb. It
/// cannot know that `femcboost` is written `FemcBoost` or that `richos` is `RichOS` — nobody
/// told this program that, and a table that claimed to know would be the compiled-in company
/// list coming back in a costume. It produces something a person can read and then correct,
/// in a file whose whole purpose is that they can correct it.
pub fn display_name_from_id(id: &str) -> String {
    id.split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The durable scope of one thread: person + entity + thread + the binding revision that
/// fences it (ECS §3.3/§3.4).
///
/// **Immutability is structural, not documentary.** Every field is private, there is no
/// setter, no `&mut` accessor and no public constructor. Outside this crate the only way
/// to obtain a `ThreadBinding` is [`crate::ledger::Ledger::thread_binding`], which reads
/// the entity out of the immutable durable record. That is what makes "a thread has
/// exactly one home entity, immutable after creation" a property of the type system
/// rather than a comment somebody can violate next quarter.
///
/// A binding carries a REVISION because it is a fencing token, not a UI hint (§3.4): a
/// command captured at revision N is refused once the active context has moved on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThreadBinding {
    person_id: PersonId,
    entity_id: EntityId,
    thread_id: String,
    binding_revision: u64,
}

impl ThreadBinding {
    /// Crate-private on purpose — see the type doc. `Ledger` is the only authority
    /// allowed to say which entity a thread belongs to.
    pub(crate) fn new(person_id: PersonId, entity_id: EntityId, thread_id: &str, binding_revision: u64) -> Self {
        ThreadBinding { person_id, entity_id, thread_id: thread_id.to_string(), binding_revision }
    }

    pub fn person_id(&self) -> &PersonId {
        &self.person_id
    }

    pub fn entity_id(&self) -> &EntityId {
        &self.entity_id
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub fn binding_revision(&self) -> u64 {
        self.binding_revision
    }

    /// The ECS scope key as a single string, for logs and violation records. `audience`
    /// is absent because slice 1 does not implement it (see the module doc); `turn_id` is
    /// supplied by the caller when one exists.
    pub fn scope_key(&self, turn_id: Option<&str>) -> String {
        format!(
            "{}+{}+{}+r{}+{}",
            self.person_id,
            self.entity_id,
            self.thread_id,
            self.binding_revision,
            turn_id.unwrap_or("-")
        )
    }
}

/// A thread's entity home. `Unbound` exists ONLY for threads written before entity
/// binding existed — the app can never create one (see `Ledger::create_thread`).
///
/// This is the deliberate answer to "existing threads have no `entity_id`": they are not
/// migrated by a heuristic, because §22 names cross-entity context as something that must
/// not be faked and a wrong binding is a privacy-boundary violation rather than a
/// cosmetic bug. They are quarantined in this state and every scoped read and write
/// against them fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ThreadEntity {
    Bound { person_id: PersonId, entity_id: EntityId, binding_revision: u64 },
    /// Pre-entity record. Listable (so an operator can see it and adopt it), never
    /// readable and never writable.
    Unbound,
}

impl ThreadEntity {
    pub fn entity_id(&self) -> Option<&EntityId> {
        match self {
            ThreadEntity::Bound { entity_id, .. } => Some(entity_id),
            ThreadEntity::Unbound => None,
        }
    }

    pub fn is_bound(&self) -> bool {
        matches!(self, ThreadEntity::Bound { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-entity-{name}-{}", crate::util::now_millis()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn one_repository_can_be_connected_to_two_companies_while_roots_stay_exclusive() {
        let dir = tmp_dir("shared-repo");
        let shared = PathBuf::from("/Users/example/Projects/shared");
        let (a, b) = (EntityId::parse("alpha").unwrap(), EntityId::parse("beta").unwrap());
        let mut reg = EntityRegistry::from_existing_ids(&[a.clone(), b.clone()]);
        reg.connect_repository(&a, shared.clone()).unwrap();
        reg.connect_repository(&b, shared.clone()).unwrap();
        assert_eq!(reg.get(&a).unwrap().connected_repositories, vec![shared.clone()]);
        assert_eq!(reg.get(&b).unwrap().connected_repositories, vec![shared.clone()]);
        // Roots stay exclusive: the path still resolves to the one company that owns it.
        assert!(reg.get(&b).unwrap().roots.is_empty());
        assert_eq!(reg.resolve_root(&shared.join("src")).unwrap().id, a);
        // The shared state survives a save and a load.
        let file = dir.join("entities.json");
        reg.save(&file).unwrap();
        let back = EntityRegistry::load(&file).registry;
        assert_eq!(back.get(&b).unwrap().connected_repositories, vec![shared]);
        assert_eq!(back.len(), 2);
    }

    #[test]
    fn a_folder_that_is_only_a_company_root_is_still_not_shareable() {
        let b = EntityId::parse("beta").unwrap();
        let mut reg = EntityRegistry::empty();
        reg.register(Entity::new("alpha", "Alpha", &["/Users/example/Projects/shared"]).unwrap()).unwrap();
        reg.register(Entity::new("beta", "Beta", &[]).unwrap()).unwrap();
        // Alpha has not connected it, so Beta connecting it still claims a root and is refused.
        assert!(matches!(reg.connect_repository(&b, PathBuf::from("/Users/example/Projects/shared")), Err(EntityError::OverlappingRoot { .. })));
    }

    #[test]
    fn entity_id_rejects_everything_that_could_escape_a_path_component() {
        assert!(EntityId::parse("northwind").is_ok());
        assert!(EntityId::parse("harbor-2").is_ok());
        assert!(EntityId::parse("9lives").is_ok());
        for bad in ["", "Northwind", "north wind", "../etc", "a/b", ".hidden", "-lead", "northwind\n"] {
            assert!(EntityId::parse(bad).is_err(), "{bad:?} should be rejected");
        }
        // Bounded: 64 ok, 65 rejected.
        assert!(EntityId::parse(&"a".repeat(ENTITY_ID_MAX_LEN)).is_ok());
        assert!(EntityId::parse(&"a".repeat(ENTITY_ID_MAX_LEN + 1)).is_err());
    }

    /// THE PRIVACY HALF OF THE 2026-09-04 FIX, as an assertion rather than a promise.
    ///
    /// `Default` returned six of the CEO's companies bound to roots under his own home
    /// was the shipping registry. A test that only checked "the app still boots" would have
    /// passed the whole time it was wrong, so this checks the thing that was wrong: the
    /// default is EMPTY, and it resolves nothing.
    #[test]
    fn the_shipping_default_registry_is_empty_and_belongs_to_nobody() {
        let reg = EntityRegistry::default();
        assert!(reg.is_empty(), "the default registry must ship with nobody's companies in it");
        assert_eq!(reg.entities().len(), 0);
        assert_eq!(reg.len(), 0);
        assert_eq!(EntityRegistry::empty(), reg);
        // An empty registry is LEGAL: it answers, it does not panic, and it never guesses.
        assert_eq!(
            reg.resolve_root(Path::new("/Users/jordan/code/northwind")).unwrap_err(),
            EntityResolveError::UnknownRoot(PathBuf::from("/Users/jordan/code/northwind"))
        );
        assert_eq!(reg.resolve_root(Path::new("/")).unwrap_err(), EntityResolveError::UnknownRoot(PathBuf::from("/")));
        assert!(!reg.contains(&EntityId::parse("northwind").unwrap()));
    }

    #[test]
    fn root_resolution_is_by_component_not_string_prefix() {
        let reg = EntityRegistry::fixture();
        // Exact root, and a path deep inside it.
        assert_eq!(reg.resolve_root(Path::new("/Users/example/Projects/harbor")).unwrap().id.as_str(), "harbor");
        assert_eq!(
            reg.resolve_root(Path::new("/Users/example/Projects/harbor/etl/src")).unwrap().id.as_str(),
            "harbor"
        );
        // THE STRING-PREFIX TRAP: "/Users/example/Projects/harb" is a string prefix of the
        // harbor root but is NOT inside it. A `starts_with` implementation would bind this
        // to harbor; component matching correctly refuses.
        assert_eq!(
            reg.resolve_root(Path::new("/Users/example/Projects/harb")).unwrap_err(),
            EntityResolveError::UnknownRoot(PathBuf::from("/Users/example/Projects/harb"))
        );
    }

    #[test]
    fn one_entity_may_own_two_roots_and_that_is_not_ambiguity() {
        // The property the CEO's `richos`/`richos-hq` pair used to pin, pinned against data
        // that belongs to nobody. `harbor-private` is also a STRING prefix match of
        // `harbor`'s first root, so this covers both halves at once.
        let reg = EntityRegistry::fixture();
        let e = reg.resolve_root(Path::new("/Users/example/Projects/harbor-private/notes")).unwrap();
        assert_eq!(e.id.as_str(), "harbor");
        assert_eq!(e.roots.len(), 2);
        assert_eq!(reg.entities().iter().filter(|x| x.id.as_str() == "harbor").count(), 1);
    }

    #[test]
    fn unknown_and_ambiguous_roots_fail_closed_and_never_default() {
        let reg = EntityRegistry::fixture();
        assert!(matches!(
            reg.resolve_root(Path::new("/tmp/somewhere-else")),
            Err(EntityResolveError::UnknownRoot(_))
        ));
        assert!(matches!(reg.resolve_root(Path::new("relative/path")), Err(EntityResolveError::NotAbsolute(_))));

        // ECS §10.2: "A path that maps to two entities blocks the turn." `new` accepts the
        // overlap (a hand-written file can be in this state and resolution has to keep
        // failing closed on it); `register` refuses to create it — see the test below.
        let overlapping = EntityRegistry::new(vec![
            Entity::new("alpha", "Alpha", &["/Users/example/Projects"]).unwrap(),
            Entity::new("beta", "Beta", &["/Users/example/Projects/shared"]).unwrap(),
        ])
        .unwrap();
        match overlapping.resolve_root(Path::new("/Users/example/Projects/shared/pkg")) {
            Err(EntityResolveError::AmbiguousRoot { candidates, .. }) => {
                assert_eq!(candidates.len(), 2);
            }
            other => panic!("expected AmbiguousRoot, got {other:?}"),
        }
    }

    #[test]
    fn registry_rejects_duplicate_ids() {
        let dup = EntityRegistry::new(vec![
            Entity::new("northwind", "One", &["/Users/example/Projects/a"]).unwrap(),
            Entity::new("northwind", "Two", &["/Users/example/Projects/b"]).unwrap(),
        ]);
        assert_eq!(dup.unwrap_err(), EntityError::DuplicateId("northwind".into()));
    }

    // ---- registration: the door the app adds a company through -------------------------

    #[test]
    fn registering_refuses_a_relative_root_instead_of_storing_a_dead_entry() {
        // A relative root can never match anything (resolution is lexical), so an entity
        // holding one is registered, listed, pickable and permanently unresolvable.
        assert!(matches!(
            Entity::try_new("northwind", "Northwind", vec![PathBuf::from("Projects/northwind")]),
            Err(EntityError::RootNotAbsolute { .. })
        ));
        let mut reg = EntityRegistry::empty();
        let mut sneaky = Entity::new("northwind", "Northwind", &[]).unwrap();
        sneaky.roots.push(PathBuf::from("relative/root"));
        assert!(matches!(reg.register(sneaky), Err(EntityError::RootNotAbsolute { .. })));
        assert!(reg.is_empty(), "a refused registration must leave the registry untouched");
    }

    #[test]
    fn registering_refuses_an_empty_display_name() {
        assert!(matches!(
            Entity::try_new("northwind", "   ", vec![]),
            Err(EntityError::EmptyDisplayName(_))
        ));
    }

    #[test]
    fn registering_refuses_an_overlap_in_either_direction() {
        // Resolution fails closed on an overlap, which is correct and is a terrible thing
        // to discover a week later. The door refuses to create the condition.
        let mut reg = EntityRegistry::empty();
        reg.register(Entity::new("harbor", "Harbor", &["/Users/example/Projects/harbor"]).unwrap()).unwrap();

        // New root INSIDE an existing one.
        assert!(matches!(
            reg.register(Entity::new("inner", "Inner", &["/Users/example/Projects/harbor/etl"]).unwrap()),
            Err(EntityError::OverlappingRoot { .. })
        ));
        // New root CONTAINING an existing one.
        assert!(matches!(
            reg.register(Entity::new("outer", "Outer", &["/Users/example/Projects"]).unwrap()),
            Err(EntityError::OverlappingRoot { .. })
        ));
        // A sibling is fine — this is not a blanket refusal to add anything.
        reg.register(Entity::new("lumen", "Lumen", &["/Users/example/Projects/lumen"]).unwrap()).unwrap();
        assert_eq!(reg.len(), 2);
        // And a duplicate id is still refused here too.
        assert!(matches!(
            reg.register(Entity::new("lumen", "Lumen Again", &["/Users/example/Projects/lumen2"]).unwrap()),
            Err(EntityError::DuplicateId(_))
        ));
    }

    // ---- the file --------------------------------------------------------------------

    #[test]
    fn an_absent_file_is_an_empty_registry_and_not_an_error() {
        let dir = tmp_dir("absent");
        let load = EntityRegistry::load(&entity_registry_path(&dir));
        assert_eq!(load.source, RegistrySource::Absent);
        assert!(load.registry.is_empty());
        assert!(!load.is_unreadable(), "nothing is wrong; the owner has just not said anything yet");
        assert_eq!(load.notes.len(), 1, "and it says so once, naming the path");
        assert!(load.notes[0].contains(&dir.display().to_string()));
    }

    #[test]
    fn a_saved_registry_round_trips_byte_for_byte_in_meaning() {
        let dir = tmp_dir("roundtrip");
        let path = entity_registry_path(&dir);
        let written = EntityRegistry::fixture();
        written.save(&path).unwrap();
        let load = EntityRegistry::load(&path);
        assert_eq!(load.source, RegistrySource::File);
        assert_eq!(load.registry, written, "what comes back is what went in, in file order");
        // 0600, because it names the owner's companies and the folders they live in.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // And no temp file is left behind by the atomic write.
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn the_example_in_the_docs_parses_and_is_what_the_docs_say_it_is() {
        // A documented example no test runs is a promise nobody kept.
        let dir = tmp_dir("example");
        let path = entity_registry_path(&dir);
        std::fs::write(&path, EXAMPLE_ENTITY_REGISTRY_JSON).unwrap();
        let load = EntityRegistry::load(&path);
        assert_eq!(load.source, RegistrySource::File, "notes: {:?}", load.notes);
        let ids: Vec<&str> = load.registry.entities().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["northwind", "harbor"]);
        // `status` is omitted on the first row and present on the second; both are active.
        assert!(load.registry.entities().iter().all(|e| e.status == EntityStatus::Active));
        // The second row's two roots survive as two roots on ONE entity.
        assert_eq!(load.registry.entities()[1].roots.len(), 2);
    }

    /// A BAD ROW FAILS THE WHOLE FILE. Dropping it would move the privacy boundary without
    /// anybody deciding to: a company would silently stop existing, its threads would stop
    /// resolving, and the app would look like it was working.
    #[test]
    fn every_way_a_file_can_be_wrong_yields_an_empty_registry_and_says_so() {
        let dir = tmp_dir("bad");
        let cases: Vec<(&str, &str)> = vec![
            ("not-json", "{ this is not json"),
            ("wrong-version", r#"{"version": 99, "entities": []}"#),
            (
                "unknown-field",
                r#"{"version":1,"entities":[{"id":"a","display_name":"A","root":["/x"]}]}"#,
            ),
            (
                "bad-id",
                r#"{"version":1,"entities":[{"id":"../etc","display_name":"A","roots":["/x"]}]}"#,
            ),
            (
                "empty-name",
                r#"{"version":1,"entities":[{"id":"a","display_name":"  ","roots":["/x"]}]}"#,
            ),
            (
                "relative-root",
                r#"{"version":1,"entities":[{"id":"a","display_name":"A","roots":["x/y"]}]}"#,
            ),
            (
                "duplicate-id",
                r#"{"version":1,"entities":[{"id":"a","display_name":"A","roots":["/x"]},
                    {"id":"a","display_name":"B","roots":["/y"]}]}"#,
            ),
        ];
        for (name, body) in cases {
            let path = dir.join(format!("{name}.json"));
            std::fs::write(&path, body).unwrap();
            let load = EntityRegistry::load(&path);
            assert_eq!(load.source, RegistrySource::Unreadable, "{name} should be refused");
            assert!(load.registry.is_empty(), "{name} must load NOTHING, not a partial list");
            assert!(load.is_unreadable());
            assert!(
                load.notes.iter().any(|n| n.contains(&path.display().to_string())),
                "{name} must name the file the owner has to fix: {:?}",
                load.notes
            );
        }
    }

    /// `Unreadable` and `Absent` are different questions with opposite answers, so they are
    /// different states rather than one `bool`.
    #[test]
    fn an_unreadable_file_is_not_the_same_state_as_no_file() {
        let dir = tmp_dir("states");
        let missing = EntityRegistry::load(&dir.join("missing.json"));
        let broken_path = dir.join("broken.json");
        std::fs::write(&broken_path, "{").unwrap();
        let broken = EntityRegistry::load(&broken_path);
        assert_eq!(missing.registry, broken.registry, "both end with nothing registered");
        assert_ne!(missing.source, broken.source, "and the app must be able to tell them apart");
        assert_eq!(missing.source.as_str(), "absent");
        assert_eq!(broken.source.as_str(), "unreadable");
    }

    // ---- the no-orphan migration ------------------------------------------------------

    #[test]
    fn ids_that_already_own_threads_are_restored_without_inventing_a_root() {
        let ids: Vec<EntityId> = ["femcboost", "richos", "gpt-exporter", "richos"]
            .iter()
            .map(|s| EntityId::parse(s).unwrap())
            .collect();
        let reg = EntityRegistry::from_existing_ids(&ids);
        // De-duplicated, in first-seen order.
        let got: Vec<&str> = reg.entities().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(got, vec!["femcboost", "richos", "gpt-exporter"]);
        // Every restored id is REGISTERED, which is what stops its threads being orphaned.
        for id in &ids {
            assert!(reg.contains(id), "{id} owns records here and must stay registered");
        }
        // NO ROOT IS INVENTED. The migration knows which ids own records; it does not know
        // where anything lives, and a guessed root is a wrong entity waiting to happen.
        assert!(reg.entities().iter().all(|e| e.roots.is_empty()));
        assert!(matches!(
            reg.resolve_root(Path::new("/anywhere/at/all")),
            Err(EntityResolveError::UnknownRoot(_))
        ));
        // The label is derived from the id and nothing pretends to know his capitalization.
        assert_eq!(reg.entities()[2].display_name, "Gpt Exporter");
    }

    #[test]
    fn display_names_are_derived_dumbly_and_never_from_a_table() {
        assert_eq!(display_name_from_id("gpt-exporter"), "Gpt Exporter");
        assert_eq!(display_name_from_id("richos"), "Richos");
        assert_eq!(display_name_from_id("a"), "A");
        assert_eq!(display_name_from_id("9lives"), "9lives");
    }

    #[test]
    fn thread_binding_exposes_the_scope_key_and_has_no_mutator() {
        let b = ThreadBinding::new(PersonId::default_ceo(), EntityId::parse("northwind").unwrap(), "thr_1", 7);
        assert_eq!(b.scope_key(Some("turn_9")), "ceo-default+northwind+thr_1+r7+turn_9");
        assert_eq!(b.scope_key(None), "ceo-default+northwind+thr_1+r7+-");
        assert_eq!(b.entity_id().as_str(), "northwind");
        assert_eq!(b.binding_revision(), 7);
    }
}
