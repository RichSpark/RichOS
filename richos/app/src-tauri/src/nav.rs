//! NAVIGATION STATE — the shell's own durable *view* preferences (UX §3.1, §25).
//!
//! Codex-UX slice 4 needs four things persisted that are not conversation evidence:
//! pin, rename, archive and the left-navigation width. This module is where they live,
//! and the choice of where deserves a sentence, because it is load-bearing rather than
//! incidental.
//!
//! **These are NOT ledger events, deliberately.** §25 Navigation says *"Search, pin,
//! rename and archive work WITHOUT changing context authority."* The conversation ledger
//! is evidence — it records what the CEO said, what Rich said, and which entity each
//! thread is bound to. A thread's title as written into `Event::ThreadCreated` is part of
//! that record. Overwriting it because the CEO wanted a tidier label in a list would edit
//! evidence to serve a rail, and the ledger has no rename/pin/archive event to append
//! instead (adding one is a `richos-core` change and belongs to whoever owns that crate).
//!
//! So a rename here is a DISPLAY OVERRIDE: the durable ledger title is untouched and
//! still returned by `list_threads`; the rail prefers the override when one exists. Pin
//! and archive are pure view state and never touch scope — an archived thread is still
//! bound to exactly the entity it was always bound to, still readable, just not in the
//! normal list (§3.2: *"Removed from normal list … recoverable through archive view"*).
//!
//! Durability posture matches the ledger and config: one JSON file in the app data dir,
//! written whole, `fsync`ed, and replaced by atomic rename so a crash mid-write leaves
//! either the old file or the new one and never a truncated one. A corrupt or missing
//! file degrades to defaults rather than failing the launch — losing a sidebar width is
//! not worth refusing to start.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Left-navigation width bounds, verbatim from UX §2.1's desktop-shell table
/// (default 300px, minimum 224px, maximum 420px). Clamped in RUST, not only in CSS, so a
/// value that would make the rail unusable can never reach the durable file — the UI is
/// free to be wrong; the store is not.
pub const SIDEBAR_WIDTH_DEFAULT: f64 = 300.0;
pub const SIDEBAR_WIDTH_MIN: f64 = 224.0;
pub const SIDEBAR_WIDTH_MAX: f64 = 420.0;

/// Worker-inspector width bounds (UX §7.2: *"Its divider is draggable and its width is
/// persisted per window"*; §25: *"Worker-pane width can be changed directly and survives
/// relaunch"*).
///
/// The maximum is not a taste call. §20 requires the conversation to keep at least 620px
/// between 820px and 1179px, and §7.2 requires both panes to stay readable — so the
/// inspector is capped where a 1180px window still leaves the reading column its floor:
/// 1180 - 224 (the rail at its own minimum) - 620 = 336. Clamped in RUST, not only in CSS,
/// for the same reason the rail is: the UI is free to be wrong; the store is not.
pub const INSPECTOR_WIDTH_DEFAULT: f64 = 336.0;
pub const INSPECTOR_WIDTH_MIN: f64 = 280.0;
pub const INSPECTOR_WIDTH_MAX: f64 = 520.0;

/// The Output panel's split width (output side-panel PRD §9.5; slice S9). Default 400px and a
/// floor of 320px, as the panel's own `PANEL_DEFAULT` / `PANEL_MIN` (`ui/output-panel.js`).
///
/// **There is no ceiling here, and that is the design, not an omission.** The panel's maximum
/// is *the stop*: the window's width less the sidebar's less the conversation at its narrowest
/// (`STAGE_MIN`, 360px), a number that moves with the window and with the sidebar. A ceiling
/// fixed in this file would be wrong on every other window, so Rust clamps the floor and a
/// value that is not a number, and the page clamps the ceiling when it paints (§9.5: "Rust
/// clamps the floor and the page clamps the ceiling at paint"). *Open completely* is never
/// written here: `full` is not persisted, and reopening is at the split width (§9.4).
pub const OUTPUT_WIDTH_DEFAULT: f64 = 400.0;
pub const OUTPUT_WIDTH_MIN: f64 = 320.0;

/// A rename override is bounded for the same reason an entity id is: it is CEO-supplied
/// text that ends up in a durable file and in the accessible name of a control.
pub const TITLE_MAX_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NavState {
    /// Persisted left-navigation width in CSS pixels (§25: *"Left-navigation width can be
    /// changed directly and survives relaunch"*).
    pub sidebar_width: f64,
    /// Whether the sidebar (the rail) is away. The CEO's choice from the toggle at the top left
    /// of the conversation or ⌘⇧S, at every window width of 820px and wider (output side-panel
    /// PRD §8); below 820px the rail is a drawer whose open/closed state is not written here.
    pub sidebar_collapsed: bool,
    /// Persisted worker-inspector width in CSS pixels (§7.2, §25). Its own field rather
    /// than a shared one: §2.1 calls these two dividers separately adjustable, and one
    /// number for both would make dragging the rail move the inspector.
    pub inspector_width: f64,
    /// The Output panel's split width in CSS pixels (output side-panel PRD §9.5). Its own
    /// field, beside the inspector's, for the reason that one has its own: one number for two
    /// dividers would make dragging one move the other.
    pub output_width: f64,
    /// Entity ids whose disclosure is closed. Absent = open, so a newly registered entity
    /// appears expanded rather than silently hidden.
    pub collapsed_entities: Vec<String>,
    /// Thread ids pinned to the top-level `Pinned` group (§3.1).
    pub pinned_threads: Vec<String>,
    /// Thread ids removed from the normal list (§3.2) — recoverable, never deleted.
    pub archived_threads: Vec<String>,
    /// thread_id -> display title override. See the module doc: the ledger title is
    /// evidence and is never rewritten.
    pub renamed_threads: BTreeMap<String, String>,
    /// thread_id -> the title Haiku wrote from the conversation's first message (CEO,
    /// 2026-10-19). Shown in place of the provisional title, and never over a title the user
    /// set himself: [`NavState::display_title`] puts his rename first.
    pub auto_titles: BTreeMap<String, String>,
}

impl NavState {
    /// The title a conversation is shown under, everywhere it is listed: his own rename, then
    /// Haiku's title, then the title it was created with.
    pub fn display_title(&self, thread_id: &str, created_title: &str) -> String {
        self.renamed_threads
            .get(thread_id)
            .or_else(|| self.auto_titles.get(thread_id))
            .cloned()
            .unwrap_or_else(|| created_title.to_string())
    }
}

impl Default for NavState {
    fn default() -> Self {
        NavState {
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
            sidebar_collapsed: false,
            inspector_width: INSPECTOR_WIDTH_DEFAULT,
            output_width: OUTPUT_WIDTH_DEFAULT,
            collapsed_entities: Vec::new(),
            pinned_threads: Vec::new(),
            archived_threads: Vec::new(),
            renamed_threads: BTreeMap::new(),
            auto_titles: BTreeMap::new(),
        }
    }
}

pub struct NavStore {
    path: PathBuf,
    state: NavState,
    /// False when a file exists at `path` and this build could not read it. Every write is
    /// gated on it — see [`NavStore::persist`].
    readable: bool,
}

impl NavStore {
    /// Load, or start from defaults. NEVER fails on a corrupt or unreadable file — a
    /// mangled preferences file must not stop the CEO's app from launching.
    ///
    /// # What it does with a file it cannot read, and why that changed
    ///
    /// **It leaves it exactly as it is.** Spec point 19
    /// (`richos-hq docs/plans/nightly-channel-spec-2026-09-17.md`): *"a reader that does not
    /// understand what it finds never rewrites, never truncates, never treats it as
    /// absent"*, generalized to every store in `ledger.rs`'s survey — of which this is the
    /// last row that still rewrote.
    ///
    /// Until 2026-09-17 an unreadable file degraded to defaults and the first `set_*` wrote
    /// those defaults over it. The survey called the cost "sidebar and inspector widths…
    /// pure view state", and that undersells the file: `pinned_threads`, `archived_threads`
    /// and `renamed_threads` are the CEO's own decisions about his own conversations, and a
    /// title he typed is not view state. The app still opens either way; what changed is
    /// that the file it could not read survives the boot.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .map(|raw| serde_json::from_str::<NavState>(&raw).ok());
        let (state, readable) = match parsed {
            // No file at all: a fresh install, and defaults are the honest answer.
            None => (NavState::default(), true),
            Some(Some(mut s)) => {
                s.sidebar_width = clamp_width(s.sidebar_width);
                s.inspector_width = clamp_inspector_width(s.inspector_width);
                s.output_width = clamp_output_width(s.output_width);
                (s, true)
            }
            // A file that EXISTS and will not parse. Defaults for this boot, in memory only.
            Some(None) => (NavState::default(), false),
        };
        NavStore { path, state, readable }
    }

    /// Whether the file on disk is one this build could read. `false` means the rail is
    /// rendering defaults and nothing it does will be remembered — which a surface has to be
    /// able to say, the same contract `LaunchStore::readable` and `ConfigStore::readable`
    /// carry.
    pub fn readable(&self) -> bool {
        self.readable
    }

    pub fn state(&self) -> &NavState {
        &self.state
    }

    /// Write whole, fsync, then atomically rename over the live file.
    ///
    /// **Never over a file this build could not read** (spec point 19). The atomic rename
    /// below is exactly what made the old behavior permanent: it replaced the unreadable
    /// file with defaults in one step, so his pins, his archives and his own thread titles
    /// were gone the moment he dragged a divider. The caller is told nothing went wrong
    /// because nothing did — the file is intact, and [`NavStore::readable`] is how a surface
    /// asks.
    fn persist(&self) -> std::io::Result<()> {
        if !self.readable {
            // Not written, and SAID: an `Ok` told the caller "saved" about a pin, archive or
            // rename that is gone at the next launch (hunt part 1, finding 38).
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the navigation file could not be read, so it was left untouched and this change was not saved",
            ));
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let encoded = serde_json::to_vec_pretty(&self.state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&encoded)?;
            f.flush()?;
            f.sync_data()?;
        }
        std::fs::rename(&tmp, &self.path)
    }

    /// Returns the width actually stored, which is the CLAMPED one — the caller renders
    /// what the store accepted rather than what it asked for, so the UI and the durable
    /// file can never disagree about the rail's width.
    pub fn set_sidebar_width(&mut self, width: f64) -> std::io::Result<f64> {
        self.state.sidebar_width = clamp_width(width);
        self.persist()?;
        Ok(self.state.sidebar_width)
    }

    /// Returns the width actually stored — the CLAMPED one, same contract as the rail's.
    pub fn set_inspector_width(&mut self, width: f64) -> std::io::Result<f64> {
        self.state.inspector_width = clamp_inspector_width(width);
        self.persist()?;
        Ok(self.state.inspector_width)
    }

    /// Returns the width actually stored — floored at 320px, never capped (see
    /// [`OUTPUT_WIDTH_MIN`]): the page paints `min(this, the stop)`.
    pub fn set_output_width(&mut self, width: f64) -> std::io::Result<f64> {
        self.state.output_width = clamp_output_width(width);
        self.persist()?;
        Ok(self.state.output_width)
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool) -> std::io::Result<()> {
        self.state.sidebar_collapsed = collapsed;
        self.persist()
    }

    pub fn set_entity_collapsed(&mut self, entity_id: &str, collapsed: bool) -> std::io::Result<()> {
        set_membership(&mut self.state.collapsed_entities, entity_id, collapsed);
        self.persist()
    }

    pub fn set_thread_pinned(&mut self, thread_id: &str, pinned: bool) -> std::io::Result<()> {
        let before = self.state.pinned_threads.clone();
        set_membership(&mut self.state.pinned_threads, thread_id, pinned);
        // A refused save leaves the list as saved, not as asked: the rail reads this state.
        self.persist().inspect_err(|_| self.state.pinned_threads = before)
    }

    pub fn set_thread_archived(&mut self, thread_id: &str, archived: bool) -> std::io::Result<()> {
        let before = self.state.archived_threads.clone();
        set_membership(&mut self.state.archived_threads, thread_id, archived);
        self.persist().inspect_err(|_| self.state.archived_threads = before)
    }

    /// An empty/blank title CLEARS the override rather than storing a blank one, so the
    /// rail falls back to the ledger's real title instead of rendering a nameless row.
    pub fn rename_thread(&mut self, thread_id: &str, title: &str) -> std::io::Result<()> {
        let before = self.state.renamed_threads.clone();
        let trimmed: String = title.trim().chars().take(TITLE_MAX_LEN).collect();
        if trimmed.is_empty() {
            self.state.renamed_threads.remove(thread_id);
        } else {
            self.state.renamed_threads.insert(thread_id.to_string(), trimmed);
        }
        self.persist().inspect_err(|_| self.state.renamed_threads = before)
    }

    /// Haiku's title for a new conversation. Returns whether it was kept: never for a
    /// conversation the user has named himself, and only once per conversation, so a later
    /// answer for the same first message changes nothing.
    pub fn set_auto_title(&mut self, thread_id: &str, title: &str) -> std::io::Result<bool> {
        let trimmed: String = title.trim().chars().take(TITLE_MAX_LEN).collect();
        if trimmed.is_empty()
            || self.state.renamed_threads.contains_key(thread_id)
            || self.state.auto_titles.contains_key(thread_id)
        {
            return Ok(false);
        }
        self.state.auto_titles.insert(thread_id.to_string(), trimmed);
        self.persist().inspect_err(|_| {
            self.state.auto_titles.remove(thread_id);
        })?;
        Ok(true)
    }
}

fn clamp_width(w: f64) -> f64 {
    if !w.is_finite() {
        return SIDEBAR_WIDTH_DEFAULT;
    }
    w.clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX)
}

fn clamp_inspector_width(w: f64) -> f64 {
    if !w.is_finite() {
        return INSPECTOR_WIDTH_DEFAULT;
    }
    w.clamp(INSPECTOR_WIDTH_MIN, INSPECTOR_WIDTH_MAX)
}

/// The floor only: the ceiling is the live stop, which only the page can measure (§9.5).
/// `INFINITY` is not finite, so it is the default rather than a panel wider than any window.
fn clamp_output_width(w: f64) -> f64 {
    if !w.is_finite() {
        return OUTPUT_WIDTH_DEFAULT;
    }
    w.max(OUTPUT_WIDTH_MIN)
}

/// Idempotent set membership on a `Vec<String>` used as a small ordered set. Idempotent
/// because a double-click on `Pin` must not produce two entries that then need two
/// unpins to undo.
fn set_membership(set: &mut Vec<String>, id: &str, member: bool) {
    let present = set.iter().any(|x| x == id);
    match (member, present) {
        (true, false) => set.push(id.to_string()),
        (false, true) => set.retain(|x| x != id),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("richos-nav-test-{}-{}.json", name, std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn width_is_clamped_to_the_ux_shell_bounds_in_rust_not_only_in_css() {
        let path = tmp_path("width");
        let mut store = NavStore::open(&path);
        assert_eq!(store.state().sidebar_width, SIDEBAR_WIDTH_DEFAULT);
        assert_eq!(store.set_sidebar_width(10.0).unwrap(), SIDEBAR_WIDTH_MIN);
        assert_eq!(store.set_sidebar_width(9000.0).unwrap(), SIDEBAR_WIDTH_MAX);
        assert_eq!(store.set_sidebar_width(f64::NAN).unwrap(), SIDEBAR_WIDTH_DEFAULT);
        assert_eq!(store.set_sidebar_width(312.0).unwrap(), 312.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn width_survives_relaunch() {
        let path = tmp_path("relaunch");
        {
            let mut store = NavStore::open(&path);
            store.set_sidebar_width(388.0).unwrap();
        }
        let reopened = NavStore::open(&path);
        assert_eq!(reopened.state().sidebar_width, 388.0, "§25: width must survive relaunch");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_inspector_width_is_its_own_number_clamped_and_durable() {
        // §25: "Worker-pane width can be changed directly and survives relaunch." Its own
        // field, so dragging one divider can never move the other (§2.1).
        let path = tmp_path("inspector");
        {
            let mut store = NavStore::open(&path);
            assert_eq!(store.state().inspector_width, INSPECTOR_WIDTH_DEFAULT);
            assert_eq!(store.set_inspector_width(10.0).unwrap(), INSPECTOR_WIDTH_MIN);
            assert_eq!(store.set_inspector_width(9000.0).unwrap(), INSPECTOR_WIDTH_MAX);
            assert_eq!(store.set_inspector_width(f64::NAN).unwrap(), INSPECTOR_WIDTH_DEFAULT);
            assert_eq!(store.set_inspector_width(412.0).unwrap(), 412.0);
            // The rail is untouched by every one of those.
            assert_eq!(store.state().sidebar_width, SIDEBAR_WIDTH_DEFAULT);
            store.set_sidebar_width(388.0).unwrap();
            assert_eq!(store.state().inspector_width, 412.0, "and the reverse holds too");
        }
        let reopened = NavStore::open(&path);
        assert_eq!(reopened.state().inspector_width, 412.0, "§25: survives relaunch");
        assert_eq!(reopened.state().sidebar_width, 388.0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_output_width_is_floored_never_capped_and_durable() {
        // Output side-panel PRD §9.5: "default 400, clamped 320..∞ in Rust; the upper bound is the
        // live stop, so Rust clamps the floor and the page clamps the ceiling at paint".
        let path = tmp_path("output");
        {
            let mut store = NavStore::open(&path);
            assert_eq!(store.state().output_width, OUTPUT_WIDTH_DEFAULT);
            assert_eq!(store.set_output_width(10.0).unwrap(), OUTPUT_WIDTH_MIN, "the floor holds");
            // The stop at 1440px with the sidebar away is 1080px, and a 2560px display's is
            // 2200px: no number in this file could be the ceiling, so none is.
            assert_eq!(store.set_output_width(2200.0).unwrap(), 2200.0, "no ceiling in the store");
            assert_eq!(store.set_output_width(f64::NAN).unwrap(), OUTPUT_WIDTH_DEFAULT);
            assert_eq!(store.set_output_width(f64::INFINITY).unwrap(), OUTPUT_WIDTH_DEFAULT);
            assert_eq!(store.set_output_width(756.0).unwrap(), 756.0);
            // Its own number: the rail's and the inspector's are untouched, and the reverse.
            assert_eq!(store.state().sidebar_width, SIDEBAR_WIDTH_DEFAULT);
            assert_eq!(store.state().inspector_width, INSPECTOR_WIDTH_DEFAULT);
            store.set_inspector_width(412.0).unwrap();
            assert_eq!(store.state().output_width, 756.0, "the inspector's divider does not move it");
        }
        let reopened = NavStore::open(&path);
        assert_eq!(reopened.state().output_width, 756.0, "the split width survives a relaunch");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_stored_output_width_under_the_floor_is_floored_on_load() {
        // A hand-edited or older file: the store renders what it would have accepted.
        let path = tmp_path("output-floor");
        std::fs::write(&path, br#"{"output_width":12.5}"#).unwrap();
        assert_eq!(NavStore::open(&path).state().output_width, OUTPUT_WIDTH_MIN);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_preferences_file_written_before_the_inspector_existed_still_loads() {
        // The forward-compat case that matters in practice: an owner who has been running
        // RichOS has a nav.json with no `inspector_width` key. `#[serde(default)]` on the
        // struct must fill it rather than dropping the whole file to defaults and losing
        // their rail width, their pins and their renames.
        let path = tmp_path("legacy");
        std::fs::write(
            &path,
            r#"{"sidebar_width":355.0,"sidebar_collapsed":false,"collapsed_entities":[],"pinned_threads":["thr_a"],"archived_threads":[],"renamed_threads":{}}"#,
        )
        .unwrap();
        let store = NavStore::open(&path);
        assert_eq!(store.state().sidebar_width, 355.0, "the old file's own values survive");
        assert_eq!(store.state().pinned_threads, vec!["thr_a".to_string()]);
        assert_eq!(store.state().inspector_width, INSPECTOR_WIDTH_DEFAULT, "the new field defaults");
        assert_eq!(store.state().output_width, OUTPUT_WIDTH_DEFAULT, "and so does the Output panel's");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pin_and_archive_are_idempotent_sets() {
        let path = tmp_path("sets");
        let mut store = NavStore::open(&path);
        store.set_thread_pinned("thr_a", true).unwrap();
        store.set_thread_pinned("thr_a", true).unwrap();
        assert_eq!(store.state().pinned_threads, vec!["thr_a".to_string()]);
        store.set_thread_pinned("thr_a", false).unwrap();
        assert!(store.state().pinned_threads.is_empty());
        store.set_thread_archived("thr_b", true).unwrap();
        assert_eq!(store.state().archived_threads, vec!["thr_b".to_string()]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blank_rename_clears_the_override_rather_than_storing_a_nameless_row() {
        let path = tmp_path("rename");
        let mut store = NavStore::open(&path);
        store.rename_thread("thr_a", "  Acme counter  ").unwrap();
        assert_eq!(store.state().renamed_threads.get("thr_a").unwrap(), "Acme counter");
        store.rename_thread("thr_a", "   ").unwrap();
        assert!(store.state().renamed_threads.get("thr_a").is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn haikus_title_is_kept_once_and_never_over_a_name_he_gave() {
        let path = tmp_path("auto-title");
        let mut store = NavStore::open(&path);
        assert_eq!(store.state().display_title("thr_a", "Can you price the Acme…"), "Can you price the Acme…");
        assert!(store.set_auto_title("thr_a", "  Acme renewal pricing ").unwrap());
        assert_eq!(store.state().display_title("thr_a", "Can you price the Acme…"), "Acme renewal pricing");
        assert!(!store.set_auto_title("thr_a", "Another answer").unwrap(), "one title per conversation");
        assert!(!store.set_auto_title("thr_b", "   ").unwrap(), "an empty answer is not a title");
        store.rename_thread("thr_c", "Board pack").unwrap();
        assert!(!store.set_auto_title("thr_c", "Quarterly board materials").unwrap());
        assert_eq!(store.state().display_title("thr_c", "New thread"), "Board pack");
        store.rename_thread("thr_a", "Acme").unwrap();
        assert_eq!(store.state().display_title("thr_a", "Can you price the Acme…"), "Acme", "his rename wins");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_corrupt_file_degrades_to_defaults_instead_of_refusing_to_launch() {
        let path = tmp_path("corrupt");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let store = NavStore::open(&path);
        assert_eq!(store.state(), &NavState::default());
        let _ = std::fs::remove_file(&path);
    }

    /// **POINT 19 ON THIS FILE, which the survey's "pure view state" line undersells.** The
    /// test above pins that the app still opens. This one pins the other half: the file it
    /// could not read is still there afterwards, unchanged, through every setter.
    ///
    /// What is at stake is not widths. `pinned_threads`, `archived_threads` and
    /// `renamed_threads` are his own decisions about his own conversations, and the atomic
    /// rename in `persist` is what used to make losing them permanent in a single step.
    #[test]
    fn an_unreadable_file_survives_the_whole_boot_byte_for_byte() {
        let path = tmp_path("unreadable-survives");
        let original: &[u8] = br#"{"sidebar_width":300,"renamed_threads":{"thr_a":"Acme counter"},,,}"#;
        std::fs::write(&path, original).unwrap();

        let mut store = NavStore::open(&path);
        assert!(!store.readable(), "the file exists and this build cannot read it");
        assert_eq!(store.state(), &NavState::default(), "the boot is served from defaults");

        assert!(store.set_sidebar_width(420.0).is_err(), "an unwritten change is not reported as saved");
        assert!(store.set_inspector_width(380.0).is_err());
        assert!(store.set_output_width(560.0).is_err());
        assert!(store.set_sidebar_collapsed(true).is_err());
        assert!(store.set_entity_collapsed("ent_a", true).is_err());
        assert!(store.set_thread_pinned("thr_a", true).is_err());
        assert!(store.set_thread_archived("thr_b", true).is_err());
        assert!(store.rename_thread("thr_a", "Something Else").is_err());
        assert!(store.state().pinned_threads.is_empty(), "a refused pin is not shown as pinned");
        assert!(store.state().archived_threads.is_empty(), "a refused archive is not shown as archived");
        assert!(store.state().renamed_threads.is_empty(), "a refused rename is not shown as renamed");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            original,
            "eight writes later, his file is byte-for-byte what it was"
        );
        // The temp sibling `persist` would have written must not be left behind either.
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_file(&path);
    }

    /// The positive control the test above is worthless without: every one of its assertions
    /// passes on a `NavStore` that has simply stopped writing anything at all.
    #[test]
    fn a_readable_file_really_is_written_to() {
        let path = tmp_path("readable-writes");
        std::fs::write(&path, br#"{"sidebar_width":300}"#).unwrap();
        let mut store = NavStore::open(&path);
        assert!(store.readable());
        store.rename_thread("thr_a", "Acme counter").unwrap();
        let reopened = NavStore::open(&path);
        assert!(reopened.readable());
        assert_eq!(reopened.state().renamed_threads.get("thr_a").unwrap(), "Acme counter");
        let _ = std::fs::remove_file(&path);
    }
}
