//! Durable CEO-facing configuration — company identity + the assertiveness dial.
//!
//! Persisted to a small JSON file, deliberately SEPARATE from the ledger's append-only
//! event log (this is mutable point-in-time state, not an event stream) but stored
//! "alongside the ledger's storage" per the task brief — i.e. in the same app data dir,
//! same durability posture (survives restart), same file-based substrate.
//!
//! Four seams live here:
//!   - `company_name` — the UX direction doc §2.1: "Rail header = the company/CEO identity,
//!     not RichOS." Configurable, persisted, sensible fallback when unset (§4.4/P4
//!     provisioning is the eventual "create your company" flow; this is the plumbing
//!     it will write into, wired ahead of that UI existing).
//!   - `assertiveness` — UX doc §5.2: one plain 3-way dial ("How much should Rich
//!     interrupt you?" Quiet / Balanced / Only when it's urgent). Default = Quiet
//!     (per the CEO decision: the acceptable failure mode is "too quiet," never
//!     "annoying"). Survives restart.
//!   - `splash_enabled` — the opening screen's off switch. It lives HERE, beside the
//!     assertiveness dial, because it is the same kind of thing: a durable CEO
//!     preference about how the product behaves toward him, not view state.
//!   - `techy_default` + `techy_entities` + `techy_threads` — the techy-mode toggle
//!     (`richos-hq/docs/plans/richos-techy-mode-2026-08-26.md` §3.1), THREE TIERS since
//!     2026-09-18. Rendering only: routing and retention run ALWAYS (§3.2), which is the
//!     only reason "turn the technical view on for a conversation I already had" is
//!     possible at all.
//!   - `raw_retention` — how long the stored output of that technical view survives
//!     (§7.2). It is here for the same reason as everything above it: a mutable
//!     point-in-time preference about how the product behaves toward him. It was two
//!     `const`s in `journal.rs` until 2026-08-30; see the §7.2 note below.
//!
//! WHY THE SWITCH IS NOT OPTIONAL, stated where the field is rather than in a design
//! doc nobody reads at the call site: the splash's failure mode is SILENT. Nobody
//! writes in to say a splash screen was beneath them — they switch it off, and if
//! there is nothing to switch, they resent it quietly and we learn nothing
//! (`docs/design/richos-splash-micro-game-2026-08-30.md` §7, richos-hq). So the
//! switch is the instrument, and `splash_first_shown_at` / `splash_disabled_at` are
//! the two timestamps that make §7's primary metric — time-to-disable — actually
//! derivable rather than gestured at. They are MEASUREMENT, never display: §5 of the
//! same document bans every counter, streak and score from the CEO's screen, and
//! nothing reads these two fields back to him.
//!
//! ## Why the toggle is here and not a ledger event (§3.1)
//! It is mutable point-in-time PREFERENCE, which is exactly what this file's first
//! paragraph says the store exists for. A toggle flipped twenty times would otherwise
//! write twenty immutable facts into a log whose job is conversational truth.
//!
//! ## §7.1 WAS THE CEO'S QUESTION AND HE ANSWERED IT ON 2026-09-18
//! *"Global default, or per-thread only?"* was open (open-items 1.4) and the answer is
//! BOTH, plus a tier neither option had. In his words:
//!
//! > "By default (on first app start) the techy mode is off. But when the user toggles
//! > the techy mode on while being inside a conversation thread, then there should be
//! > something like a radio button choice between 3 choices. And those choices could be
//! > something like 1) "For all threads in all companies" 2) "For all threads in this
//! > company" 3) "For this thread only". And the first choice should be preselected
//! > after the user switches the techy mode on. That also means that later the user
//! > should be able to switch off the techy mode for a given company or a given thread."
//!
//! So the store holds THREE tiers and [`ConfigStore::techy_mode`] resolves them in one
//! order — thread pin, else company pin, else `techy_default` — while
//! [`ConfigStore::apply_techy_scope`] is the single writer that takes one of his three
//! radio buttons and leaves the store in a state that agrees with it. The shape did not
//! have to change to carry his answer, which is what the old wording here was betting on:
//! a tier is a map, `None` still means "follow the tier above me", and the clearing
//! setters ([`ConfigStore::clear_techy_thread`], [`ConfigStore::clear_techy_entity`]) are
//! what make his "switch it off for a given company or a given thread" reversible in both
//! directions instead of one.
//!
//! ## §7.2 IS ALSO THE CEO'S QUESTION AND IS ALSO NOT ANSWERED HERE
//! *"How long do raw payloads survive?"* is open (open-items 1.4), and this file does not
//! decide it. It removes the reason the question was not neutral: "14 days" was
//! `RAW_RETENTION_DAYS` in `journal.rs` and "forever" was a developer's edit, so the
//! status quo was the only answer that cost nothing. Now every answer costs a click, the
//! shipping default reproduces yesterday's behaviour exactly, and [`RetentionChoice`]
//! carries a `Custom` arm precisely so that a value nobody put on a menu is still a value
//! the store can hold and the surface can report honestly rather than round to the nearest
//! button.

use crate::entity::EntityId;
use crate::journal::{RawRetention, RAW_MAX_TOTAL_BYTES, RAW_RETENTION_DAYS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The CEO's proactive-attention dial (UX doc §5.2). Values match the radio inputs in
/// `app/ui/index.html` (`name="assertiveness"`) verbatim, so the wire string never needs
/// translating on the UI side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Assertiveness {
    Quiet,
    Balanced,
    UrgentOnly,
    /// **A value on disk that this build has no name for.** Spec point 21: a closed enum in
    /// a persisted document is a schema change waiting to happen that no shape-diff will
    /// ever report, and without this arm an unknown value fails the parse of the WHOLE
    /// `config.json` and costs every preference in it.
    ///
    /// It is never a value the app SETS — [`ConfigStore::set_assertiveness`] refuses it by
    /// name — and never a value the CEO sees: [`ConfigStore::assertiveness`] answers with
    /// the default for the read, and [`ConfigStore::persist`] writes the original string
    /// back out verbatim rather than this placeholder.
    #[serde(other)]
    Unknown,
}

impl Default for Assertiveness {
    /// Default = Quiet. Per the CEO decision: model an excellent human chief of staff;
    /// the failure mode is "too quiet," NEVER "annoying."
    fn default() -> Self {
        Assertiveness::Quiet
    }
}

impl Assertiveness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Assertiveness::Quiet => "quiet",
            Assertiveness::Balanced => "balanced",
            Assertiveness::UrgentOnly => "urgent-only",
            // Unreachable through the store, which resolves `Unknown` to the default before
            // any caller sees it. Answering with the DEFAULT's wire string rather than a
            // word like "unknown" means that if a path is ever added that reaches here, it
            // hands the UI a radio value the UI actually has — a wrong-but-valid answer
            // instead of a dead one. The durable file is never written from this.
            Assertiveness::Unknown => Assertiveness::Quiet.as_str(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "quiet" => Some(Assertiveness::Quiet),
            "balanced" => Some(Assertiveness::Balanced),
            "urgent-only" => Some(Assertiveness::UrgentOnly),
            _ => None,
        }
    }
}

/// Which lighting the app opens in.
///
/// **CEO ruling §63 (2026-09-19) supersedes §15's default for the normal screens:** *"the
/// app should detect the user's system preference for dark or light theme and system
/// should be the default for a freshly installed app. Only if the user switches the theme
/// to some other option, only then should the app remember that and from now on that would
/// become their default."*
///
/// So `Default` is [`Theme::System`], and it is the ONLY value this store ever holds
/// without the user having chosen it. `Dark` and `Light` in here are always his word —
/// which is the whole of §63's second sentence, expressed as a type: there is no code path
/// that writes a lighting on his behalf, so "remembered" and "he switched" are the same
/// event.
///
/// §15's "a newly installed app opens dark" is NOT gone; it moved. It still governs the
/// splash and the home screen, which are clamped dark unconditionally and which no switch
/// reaches — `RichTheme.forceDark` in `theme-boot.js`, a FORCE flag rather than a write,
/// precisely so the clamp never becomes a stored preference.
///
/// The resolution of `System` to an actual palette happens in the UI, where the OS
/// preference is observable and where it must also be observed CONTINUOUSLY: the
/// `prefers-color-scheme` media query's change event repaints while the preference is
/// `System`, so an OS that flips while the app is open takes the app with it.
///
/// Values match `data-th` in the settings menu and the strings `theme-boot.js` mirrors,
/// verbatim, so the wire string never needs translating on either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    Dark,
    Light,
    System,
    /// **A lighting on disk that this build has no name for** — the worked example in spec
    /// point 21, where a nightly adds `Theme::Sepia`, the CEO picks it, and a rollback to
    /// stable used to lose every preference in the file permanently.
    ///
    /// Same contract as [`Assertiveness::Unknown`]: never set, never shown, never persisted
    /// as itself.
    #[serde(other)]
    Unknown,
}

impl Default for Theme {
    /// §63: an unchosen lighting is `System`, never a lighting. See the enum's doc for why
    /// this is the one value the store can hold that is not the user's own word.
    fn default() -> Self {
        Theme::System
    }
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::System => "system",
            // See `Assertiveness::as_str`: the DEFAULT's wire string, so a hypothetical
            // future caller gets a value the UI has rather than a `data-th` it does not.
            // Written as `Theme::System` and not as a literal because the sentence above is
            // "the default", and §63 moved which variant that is; the store never lets this
            // be reached, and `persist` never writes it.
            Theme::Unknown => Theme::System.as_str(),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "system" => Some(Theme::System),
            _ => None,
        }
    }
}

/// **Which Claude model one of the two sessions runs on** — the CEO's §119 (2026-10-10):
/// *"Rich and Stu stay on Opus high effort in RichOS by default. But the user should have the
/// option to switch one or both to Fable (in general settings, not in quick settings)."*
///
/// One value per role, stored as `rich_model` and `stu_model` in `config.json`. The wire
/// strings are the `claude` CLI's own model aliases (`--model opus`, `--model fable`), so the
/// value goes to the child unchanged ([`ModelChoice::cli_alias`]). Effort is not a setting:
/// his sentence fixes it at high for both, and `native.rs` passes `--effort high` either way.
///
/// Before this, the app never named a model, so both sessions ran on whatever the CLI chose
/// for the account (Clark's 2026-10-10 Haiku audit in the private record).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ModelChoice {
    /// His default for both roles.
    #[default]
    Opus,
    /// The option he named.
    Fable,
    /// **A model on disk this build has no name for** — same contract as
    /// [`Assertiveness::Unknown`]: never set, never sent to the child, never persisted as
    /// itself. A session reads it as the default, Opus.
    #[serde(other)]
    Unknown,
}

impl ModelChoice {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelChoice::Opus => "opus",
            ModelChoice::Fable => "fable",
            // See `Assertiveness::as_str`: the default's wire string. The store resolves
            // `Unknown` before any caller sees it.
            ModelChoice::Unknown => ModelChoice::Opus.as_str(),
        }
    }

    /// The `--model` value the `claude` CLI is given. The aliases are the CLI's own
    /// (`claude --help`: "an alias for the latest model (e.g. 'fable', 'opus', or 'sonnet')").
    /// Measured on 2.1.295 with the lease's `--setting-sources ''` on 2026-10-10: `opus`
    /// answered as `claude-opus-5-5` and `fable` as `claude-fable-5-1`.
    pub fn cli_alias(&self) -> &'static str {
        self.as_str()
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "opus" => Some(ModelChoice::Opus),
            "fable" => Some(ModelChoice::Fable),
            _ => None,
        }
    }

    fn resolved(self) -> Self {
        match self {
            ModelChoice::Unknown => ModelChoice::default(),
            known => known,
        }
    }
}

/// The two stored choices together, as the spawn path reads them: Rich is the front desk
/// (the conversation lease), Stu is the back end (the work lease, §110).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LeaseModels {
    pub rich: ModelChoice,
    pub stu: ModelChoice,
}

/// The models a data directory's `config.json` names, read fresh at every lease spawn so a
/// change in settings reaches the next session that starts. An absent or unreadable file
/// yields the default (Opus for both) rather than a failure, the same rule
/// `doctrine::identity_from_config` follows: a preference is never a reason a session cannot
/// start.
pub fn lease_models_from_config(data_dir: &Path) -> LeaseModels {
    match ConfigStore::open(data_dir.join("config.json")) {
        Ok(c) => c.lease_models(),
        Err(_) => LeaseModels::default(),
    }
}

/// The steps the font-size control walks, as percentages of the 16px root.
///
/// A DISCRETE LADDER AND NOT A FREE NUMBER, deliberately. §15 makes font size a control —
/// "`Ctrl`/`Cmd` `+` / `-` / `0` increase, decrease and reset the overall font size, exactly
/// as on a web page" — and every reading of that is a stepper. A percentage the CEO can
/// type into is a way to arrive at 43% type on a surface whose whole claim is that it is
/// readable, and there is no keystroke in the ruling that produces an arbitrary number.
pub const FONT_SCALE_STEPS: [u16; 7] = [80, 90, 100, 110, 120, 135, 150];

/// 100% — the 16px root the type scale is authored against.
pub const FONT_SCALE_DEFAULT: u16 = 100;

/// Snap an arbitrary percentage to the nearest legal step.
///
/// The UI only ever sends a step, so this is not the normal path — it is what happens when
/// a hand-edited config file, or a future control with a different ladder, offers something
/// else. Rejecting it outright would silently reset the CEO's preference to 100%; rounding
/// keeps his INTENT (bigger, or smaller) while keeping the invariant that a stored scale is
/// always one this build can step away from.
pub fn snap_font_scale(pct: u16) -> u16 {
    let mut best = FONT_SCALE_DEFAULT;
    let mut best_gap = u16::MAX;
    for step in FONT_SCALE_STEPS {
        let gap = step.abs_diff(pct);
        if gap < best_gap {
            best_gap = gap;
            best = step;
        }
    }
    best
}

fn font_scale_default() -> u16 {
    FONT_SCALE_DEFAULT
}

/// The CEO's initials, for the circle at the foot of his own rail — "AB" from
/// "Alex Booster".
///
/// Returns `None` rather than a guess whenever there is nothing honest to derive. That is
/// the load-bearing half: the rail footer used to carry Rich's hand and the label "Rich",
/// and the CEO's correction is that the foot of HIS rail shows HIS identity. An invented
/// name would be worse than the thing it replaced, and `??` is a placeholder pretending to
/// be a value — the surface renders the honest unset state instead (`app/ui/main.js`).
///
/// TWO TOKENS AT MOST, first and last, because that is what the pattern is: "Alex Booster"
/// is AB, and "Alexander James Booster" is AB as well, not AJB. A single token gives a
/// single letter rather than two letters off one word.
pub fn initials_from(name: &str) -> Option<String> {
    let tokens: Vec<&str> = name.split_whitespace().filter(|t| !t.is_empty()).collect();
    let first = tokens.first()?.chars().next()?;
    let mut out: String = first.to_uppercase().collect();
    if tokens.len() > 1 {
        if let Some(last) = tokens.last().and_then(|t| t.chars().next()) {
            out.extend(last.to_uppercase());
        }
    }
    Some(out)
}

/// The sensible fallback shown when no company name has been configured yet — matches
/// the UI's own placeholder constant (`app/ui/main.js` `COMPANY_LABEL_FALLBACK`) so the
/// two sides can never drift into showing two different defaults.
pub const COMPANY_NAME_FALLBACK: &str = "My Company";

/// The splash's default, as a named function because `#[serde(default)]` on a `bool`
/// means `false` and that is the wrong answer here: a config file written before the
/// splash existed must read as ENABLED, not as "the CEO turned it off". An absent field
/// is an absent OPINION, and the product's opinion is on.
fn splash_default() -> bool {
    true
}

/// THE SHAPE NUMBER OF `config.json` — spec point 5 and point 21's second enforcement.
///
/// `grep -n schema_version config.rs` returned nothing before this change; the real file on
/// the CEO's Mac has fourteen keys and none of them was a version. Without one, a genuine
/// shape change to this file is undeclarable and therefore invisible to point 25's gate.
///
/// It goes up when, and only when, this document's shape changes — the same event that, from
/// point 26, carries a declared migration.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// **An ABSENT `schema_version` reads as 1, and that is the opposite of what `launch.rs`
/// does with its own absent version.** The difference is deliberate and it is about which
/// mistake each file can afford.
///
/// `launches.json` treats an absent version as "not ours", because announcing that a foreign
/// file is one of ours would fire the first-run reward at the person who has been here
/// longest. `config.json` cannot take that line: every config file that exists today was
/// written before this field did, so reading absence as unreadable would make this very
/// change wipe the preferences of every user it is meant to protect — the exact failure spec
/// point 21 describes, caused by the fix for it.
fn config_schema_version_default() -> u32 {
    CONFIG_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredConfig {
    /// See [`CONFIG_SCHEMA_VERSION`] and [`config_schema_version_default`].
    #[serde(default = "config_schema_version_default")]
    schema_version: u32,
    company_name: Option<String>,
    #[serde(default)]
    assertiveness: Assertiveness,
    /// The opening screen's off switch. Default on; an older config file with no such
    /// key reads as on (see `splash_default`).
    #[serde(default = "splash_default")]
    splash_enabled: bool,
    /// When the splash was first shown to this CEO, in epoch millis. Written ONCE, by
    /// the surface itself, and never again — it is the zero point time-to-disable is
    /// measured from.
    #[serde(default)]
    splash_first_shown_at: Option<u64>,
    /// When he last switched it OFF, in epoch millis. Cleared when he switches it back
    /// on, so it never reports a disable that was reversed.
    #[serde(default)]
    splash_disabled_at: Option<u64>,
    /// §3.1: the global default. `false`, so a fresh install's conversation surface is
    /// the calm one and Urban's v1 direction needs no amendment (§3.3).
    #[serde(default)]
    techy_default: bool,
    /// §3.1: per-thread overrides. A thread ABSENT from this map follows the tier above
    /// it; a thread present pins its own answer. `BTreeMap` so the file serializes in a
    /// stable order and a diff of `config.json` is readable.
    #[serde(default)]
    techy_threads: BTreeMap<String, bool>,
    /// §7.1's MIDDLE TIER, added 2026-09-18 on the CEO's answer to it — *"1) For all
    /// threads in all companies 2) For all threads in this company 3) For this thread
    /// only"*. An entity ABSENT from this map follows `techy_default`; an entity present
    /// pins every thread in it that has no pin of its own. Same `BTreeMap`, same reason.
    ///
    /// Keyed by [`crate::entity::EntityId`]'s string form, which is the same key
    /// `techy_threads` uses for threads: a plain owned `String`, so a config file written
    /// by a newer build that knows an entity this one does not is still readable, and the
    /// unknown key is simply never consulted.
    #[serde(default)]
    techy_entities: BTreeMap<String, bool>,
    /// §63: which lighting the app opens in. An ABSENT key is an absent opinion, and under
    /// §63 an absent opinion is the OPERATING SYSTEM's — `Theme::default()`, which is
    /// `System`. A config file written before theming existed therefore reads as `System`
    /// too, and that is correct rather than incidental: its owner never switched the theme,
    /// so there is nothing for the app to remember on his behalf.
    #[serde(default)]
    theme: Theme,
    /// §15: the font-size control's position, as a percentage of the 16px root. Absent
    /// reads as 100%, which is the size the type scale is authored at.
    #[serde(default = "font_scale_default")]
    font_scale: u16,
    /// The PERSON, not the company. `company_name` above is the entity in the rail header;
    /// this is the human at the foot of the rail. There was no such field until the CEO's
    /// correction to round 10.1, so an absent key is the overwhelmingly common case and
    /// means exactly what it says: nobody has told the app who this is. It is `Option` and
    /// not a defaulted string for that reason — there is no honest default for a person's
    /// name, and the surface renders an unset state rather than inventing one.
    #[serde(default)]
    user_name: Option<String>,
    /// §7.2: how long the raw output survives. **The absent key is what carries the
    /// shipping default**: `RawRetention::default()`, which is FOREVER since the CEO's
    /// 2026-10-06 feedback (item 6), so an install that never chose keeps everything and one
    /// that chose keeps its choice. A key that is PRESENT and unreadable is a different case and keeps
    /// everything instead (`RawRetention::from_json`).
    #[serde(default)]
    raw_retention: RawRetention,
    /// WHICH COMPANY THIS COPY OF RICH WORKS FOR — the CEO's own answer, remembered.
    ///
    /// The whole reason this field exists: a double-clicked `.app` has working directory
    /// `/`, which owns no entity, so `EntityRegistry::resolve_root` fails closed and every
    /// send is refused (measured 2026-09-01, `docs/verification/entity-choice-2026-09-01/`).
    /// The fix is NOT to guess an entity at compile time — ECS §3.3's refusal to guess is
    /// correct and stays — it is to let him choose once and to remember the answer here.
    ///
    /// **`String`, deliberately, and not `EntityId`.** `EntityId` deserializes through
    /// `try_from = "String"` and REJECTS a value that is not `[a-z0-9-]{1,64}`, which would
    /// fail the parse of the WHOLE file — and `ConfigStore::open` degrades an unparseable
    /// file to defaults, so one bad character in this one key would silently reset his
    /// theme, his dial and his retention window. A raw string cannot do that. It is
    /// validated on the way OUT instead ([`ConfigStore::entity`]), where a value that no
    /// longer parses or is no longer registered resolves to nothing rather than to a guess.
    ///
    /// Absent means NOBODY HAS CHOSEN YET, which is a real and different state from "he
    /// chose and it went missing" — it is the state that makes the app ask.
    #[serde(default)]
    entity: Option<String>,
    /// THE HOME SCREEN'S ENTITY ROW — the CEO's own LABEL for a company, keyed by entity id.
    ///
    /// His words, 2026-09-01: *"the user should also be able to customize in the settings the
    /// labels on those company buttons ... so, if the user wanted to anonymize their home
    /// screen (for sharing on social media), they could change the label buttons to something
    /// like '1', '2', '3'"*.
    ///
    /// **A MASK OVER A NAME, NEVER A RENAME.** Nothing keyed on identity moves: the entity id,
    /// the registry, the thread bindings and every ledger record are untouched by anything in
    /// this map. It is consulted at one place — where a button's text is decided — and nowhere
    /// else. That is what makes anonymizing the screen safe to do and safe to undo.
    ///
    /// An ABSENT id means the registry's own display name. That is the common case and it is
    /// never blank and never the raw id.
    ///
    /// `String` keys and not `EntityId`, for exactly the reason `entity` above is a `String`:
    /// `EntityId` rejects on deserialize, and a rejected key here would fail the parse of the
    /// whole file, which `ConfigStore::open` degrades to defaults — one bad character would
    /// silently reset his theme, his dial and his retention window.
    #[serde(default)]
    home_entity_labels: BTreeMap<String, String>,
    /// WHICH COMPANIES SHOW IN THAT ROW. His words: *"which to display on their home screen"*.
    ///
    /// Stored as the HIDDEN set rather than the visible one, and the direction is the whole
    /// point: a company he has never had an opinion about SHOWS, so a company added to the
    /// registry tomorrow appears on his home screen rather than being invisible until he finds
    /// a setting. Absence is an absent opinion, and the product's opinion is "show it".
    #[serde(default)]
    home_entity_hidden: BTreeMap<String, bool>,
    /// §119: the model Rich's session (the front desk) starts on. Absent reads as Opus, his
    /// default; see [`ModelChoice`].
    #[serde(default)]
    rich_model: ModelChoice,
    /// §119: the model Stu's session (the back end) starts on. Absent reads as Opus.
    #[serde(default)]
    stu_model: ModelChoice,
    /// THE OXFORD COMMA IN WHAT CLAUDE WRITES FOR HIM (CEO, 2026-10-19): "make the Oxford comma
    /// linter switchable in settings. Default setting: Oxford comma off." `true` keeps the comma
    /// before and/or/nor in Rich's and Stu's replies and in the questions Claude writes; `false`
    /// (the default, and what an absent key reads as, so an install from before this setting
    /// existed starts off) has the app delete it where the text is drawn. The app's own text
    /// never has it, whatever this says.
    #[serde(default)]
    oxford_comma: bool,
    /// How the Mac resources square is drawn. An absent value reads as Ink,
    /// including on installations from before the square existed.
    ///
    /// A plain `String`, not a closed enum, for the reason `entity` above is one: a view a later
    /// build adds is kept verbatim rather than flattened, and [`ConfigStore::resources_view`]
    /// answers Ink for anything this build cannot draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resources_view: Option<String>,
}

/// The views the resources square can be drawn in. The first is the default.
pub const RESOURCES_VIEWS: [&str; 2] = ["ink", "readout"];

impl Default for StoredConfig {
    fn default() -> Self {
        StoredConfig {
            schema_version: CONFIG_SCHEMA_VERSION,
            company_name: None,
            assertiveness: Assertiveness::default(),
            splash_enabled: splash_default(),
            splash_first_shown_at: None,
            splash_disabled_at: None,
            techy_default: false,
            techy_threads: BTreeMap::new(),
            techy_entities: BTreeMap::new(),
            raw_retention: RawRetention::default(),
            theme: Theme::default(),
            font_scale: FONT_SCALE_DEFAULT,
            user_name: None,
            entity: None,
            home_entity_labels: BTreeMap::new(),
            home_entity_hidden: BTreeMap::new(),
            rich_model: ModelChoice::default(),
            stu_model: ModelChoice::default(),
            oxford_comma: false,
            resources_view: None,
        }
    }
}

/// Where a thread's techy-mode answer came from — reported to the UI so the CEO's own
/// surface can say *"this thread follows your default"* rather than implying he chose it.
///
/// Not decoration: it is what makes clearing an override a visible, reversible act
/// instead of a state the surface cannot distinguish from "he pinned it to the same
/// value the default happens to have".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TechySource {
    /// The CEO pinned this thread.
    Thread,
    /// The CEO pinned the company this thread lives in, and this thread has no pin of
    /// its own (§7.1's middle tier, 2026-09-18).
    Entity,
    /// No override at either tier — the thread follows `techy_default`.
    Default,
}

impl TechySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TechySource::Thread => "thread",
            TechySource::Entity => "entity",
            TechySource::Default => "default",
        }
    }
}

/// WHICH TIER A SWITCH-ON (OR SWITCH-OFF) APPLIES TO — the CEO's three radio buttons,
/// as a type.
///
/// His answer to §7.1, 2026-09-18, verbatim: *"when the user toggles the techy mode on
/// while being inside a conversation thread, then there should be something like a radio
/// button choice between 3 choices. And those choices could be something like 1) "For all
/// threads in all companies" 2) "For all threads in this company" 3) "For this thread
/// only". And the first choice should be preselected."*
///
/// The variants are in HIS order, and [`TechyScope::PRESELECTED`] is the first of them,
/// named once here so no surface has to re-decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TechyScope {
    /// 1) For all threads in all companies.
    AllCompanies,
    /// 2) For all threads in this company.
    Company,
    /// 3) For this thread only.
    Thread,
}

impl TechyScope {
    /// The choice the sheet opens on, on switch-ON and switch-OFF alike. He specified it
    /// for switch-on; nothing in the store argues for a different one on switch-off, and
    /// one preselection that is the same in both directions is the only version of this
    /// a person can learn once. The explicit confirm button, not the preselection, is
    /// what stops an accidental app-wide flip.
    pub const PRESELECTED: TechyScope = TechyScope::AllCompanies;

    pub fn as_str(&self) -> &'static str {
        match self {
            TechyScope::AllCompanies => "all-companies",
            TechyScope::Company => "company",
            TechyScope::Thread => "thread",
        }
    }

    /// Parse the wire form a surface sends back. `None` for anything else — a scope this
    /// build does not know is refused rather than rounded to an adjacent tier, which
    /// would apply the CEO's switch somewhere he did not point it.
    pub fn parse(s: &str) -> Option<TechyScope> {
        match s {
            "all-companies" => Some(TechyScope::AllCompanies),
            "company" => Some(TechyScope::Company),
            "thread" => Some(TechyScope::Thread),
            _ => None,
        }
    }
}

/// One thread's resolved techy-mode state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechyMode {
    /// What the renderer should do for this thread, right now.
    pub enabled: bool,
    /// Where `enabled` came from.
    pub source: TechySource,
    /// The global default, carried alongside so a settings surface can show both without
    /// a second round trip.
    pub default: bool,
}

/// THE THREE ANSWERS ON THE MENU, plus the honest fourth.
///
/// §7.2's own options, in his words: *"14 days or 2 GB, whichever binds first"*, or
/// *"forever"* — *"~3-4 GB/year on his own disk and a perfectly reasonable choice, it is
/// his machine"*. Those are the two ends; `ThreeMonths` is the middle one, because a dial
/// whose only positions are a fortnight and eternity is not a dial.
///
/// **WHY THE MENU IS THREE NAMES AND NOT TWO NUMBER FIELDS.** The setting has two axes —
/// days and bytes — and asking a CEO to reason about "90 days OR 2 GB, whichever binds
/// first" is asking him to hold the implementation in his head to predict what he will
/// still be able to look at. A named choice hides the arithmetic and the SURFACE states
/// the consequence in a sentence. The axes are still both there, still independent, and
/// still separately settable by anyone who edits `config.json`.
///
/// **AND WHY `Custom` EXISTS.** A hand-edited file can hold a window no button on the menu
/// produces. Rounding that to the nearest named choice would misreport his setting on the
/// screen that is supposed to tell him what it is, and — worse — the first click on any
/// other control would write the rounded value back. So a window that is not one of the
/// three reports as itself, and the surface shows no selection rather than a wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RetentionChoice {
    /// 14 days or 2 GB, whichever binds first.
    TwoWeeks,
    /// 90 days, same 2 GB ceiling. The ceiling is deliberately NOT raised with the window:
    /// it is the only thing between §2.4 and a full disk, and the surface says plainly
    /// which of the two will bind.
    ThreeMonths,
    /// Nothing is ever evicted, on either axis. **The shipping default** (CEO 2026-10-06,
    /// feedback item 6): the one an install with no `raw_retention` key has.
    Forever,
    /// A window that is in the file and on no menu. Reported, never written by a click.
    Custom,
}

/// 90 days. Named here rather than inline so the value the menu produces and the value the
/// menu recognizes cannot drift apart — they are the same constant read twice.
pub const THREE_MONTHS_DAYS: u64 = 90;

impl RetentionChoice {
    pub fn as_str(&self) -> &'static str {
        match self {
            RetentionChoice::TwoWeeks => "two-weeks",
            RetentionChoice::ThreeMonths => "three-months",
            RetentionChoice::Forever => "forever",
            RetentionChoice::Custom => "custom",
        }
    }

    /// Parse a wire value. `"custom"` is deliberately NOT parseable: it is a description of
    /// what is in the file, never an instruction, and a settable `custom` would be a
    /// command with no argument.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "two-weeks" => Some(RetentionChoice::TwoWeeks),
            "three-months" => Some(RetentionChoice::ThreeMonths),
            "forever" => Some(RetentionChoice::Forever),
            _ => None,
        }
    }

    /// The window this choice means, or `None` for `Custom`.
    pub fn retention(&self) -> Option<RawRetention> {
        match self {
            RetentionChoice::TwoWeeks => Some(RawRetention::of(RAW_RETENTION_DAYS, RAW_MAX_TOTAL_BYTES)),
            RetentionChoice::ThreeMonths => Some(RawRetention::of(THREE_MONTHS_DAYS, RAW_MAX_TOTAL_BYTES)),
            RetentionChoice::Forever => Some(RawRetention::FOREVER),
            RetentionChoice::Custom => None,
        }
    }

    /// Which menu entry, if any, a stored window IS. Derived by comparing against what each
    /// entry produces, so the two directions can never disagree.
    pub fn of(retention: &RawRetention) -> Self {
        for c in [RetentionChoice::TwoWeeks, RetentionChoice::ThreeMonths, RetentionChoice::Forever] {
            if c.retention().as_ref() == Some(retention) {
                return c;
            }
        }
        RetentionChoice::Custom
    }
}

/// A small JSON-file-backed config store. Read once at open, written eagerly (whole-file
/// rewrite) on every set — the file is tiny, so this stays simple and crash-safe-enough
/// (a torn write here loses at most the CEO's last preference toggle, not a message).
pub struct ConfigStore {
    path: PathBuf,
    config: StoredConfig,
    /// The settings as last read or written. A change that is not saved puts `config` back
    /// to this ([`ConfigStore::persist`]), so memory never holds a preference the file does
    /// not, and an identical retry is refused again rather than found "already set" (hunt
    /// part 1 v3, finding 38).
    saved: StoredConfig,
    /// **The document exactly as it was read**, kept so [`ConfigStore::persist`] can write
    /// back a value this build cannot represent instead of flattening it to a placeholder.
    /// `None` when there was no file, or when the file could not be read at all — in which
    /// case nothing is written anyway.
    raw: Option<serde_json::Map<String, serde_json::Value>>,
    /// False when the file on disk exists and this build could not read it. Every write is
    /// gated on this; see [`ConfigStore::persist`].
    readable: bool,
    /// Why, in a sentence composed here. Never a parser message — see
    /// [`ConfigStore::unreadable_reason`].
    unreadable_reason: Option<String>,
}

impl ConfigStore {
    /// Open the config file at `path`.
    ///
    /// # What this does with a file it cannot read, and why it changed
    ///
    /// **It leaves it exactly as it is.** Spec point 19 — *"a reader that does not understand
    /// what it finds never rewrites, never truncates, never treats it as absent"* — and spec
    /// point 5, which names this function as the row of `ledger.rs`'s own survey
    /// (`ledger.rs:56`) that contradicted it.
    ///
    /// Until 2026-09-17 this took `unwrap_or_else` into defaults and the first subsequent
    /// `set_*` wrote the whole file back, so an unreadable `config.json` cost every
    /// preference in it PERMANENTLY — going forward to the build that could read it did not
    /// bring them back. The preferences are now served from memory at their defaults for that
    /// boot, the file is untouched, and the app still starts: config is a preference layer,
    /// never a reason "talk to Rich" cannot open.
    ///
    /// The one piece of the old path that was already right and is kept: `raw_retention`
    /// resolves to `FOREVER` rather than its default, because it is the only preference here
    /// whose default DELETES. A wrong theme costs a re-tick; a wrong retention window costs
    /// records, at the next boot, silently.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        // What a boot gets when the file is there and unreadable: defaults in memory, and
        // the one field whose default deletes kept at FOREVER.
        let held_open = || StoredConfig {
            raw_retention: RawRetention::FOREVER,
            ..StoredConfig::default()
        };

        let (config, raw, readable, reason) = match fs::read_to_string(&path) {
            // No file: a fresh install. Every default applies, `raw_retention` included —
            // which is the shipping window and nothing new.
            Err(e) if e.kind() == io::ErrorKind::NotFound => (StoredConfig::default(), None, true, None),
            // A file that is THERE and cannot be read (invalid UTF-8, permissions, I/O) is
            // not a fresh install: it keeps everything, like the parse-failure arm below.
            Err(_) => (
                held_open(),
                None,
                false,
                Some(
                    "the settings file on disk could not be read; it is being left exactly \
                     as it is rather than read as a fresh install, and nothing is written \
                     over it"
                        .to_string(),
                ),
            ),
            Ok(text) => match serde_json::from_str::<StoredConfig>(&text) {
                Ok(c) if c.schema_version <= CONFIG_SCHEMA_VERSION => {
                    // Keep the document itself. `persist` reads it to restore any value this
                    // build parsed into an `Unknown` placeholder.
                    let raw = serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .and_then(|v| v.as_object().cloned());
                    (c, raw, true, None)
                }
                Ok(c) => (
                    held_open(),
                    None,
                    false,
                    Some(format!(
                        "the settings file on disk is schema version {} and this build knows \
                         version {CONFIG_SCHEMA_VERSION}; it is being left exactly as it is \
                         rather than read as a fresh install, and nothing is written over it",
                        c.schema_version
                    )),
                ),
                Err(e) => (
                    held_open(),
                    None,
                    false,
                    Some(format!(
                        "the settings file on disk stops being readable at line {}, column {}; \
                         it is being left exactly as it is rather than read as a fresh \
                         install, and nothing is written over it",
                        e.line(),
                        e.column()
                    )),
                ),
            },
        };
        Ok(ConfigStore { path, saved: config.clone(), config, raw, readable, unreadable_reason: reason })
    }

    /// Whether the file on disk is one this build could read.
    ///
    /// `false` means every getter below is answering from a default rather than from his
    /// settings, and every setter is refusing to write. A surface that shows preferences has
    /// to be able to say that, which is the whole reason this is public — the same contract
    /// `launch.rs`'s `LaunchStore::readable` carries.
    pub fn readable(&self) -> bool {
        self.readable
    }

    /// The sentence to show when [`ConfigStore::readable`] is false, or `None`.
    ///
    /// **Composed here, never taken from serde.** serde's messages quote the offending value
    /// — `invalid type: string "…"` — and in THIS file that value is the company he works
    /// for, his own name, or a thread title. Only the parser's line and column are used: they
    /// locate the damage and reveal nothing. Same rule `skip.rs` holds for the ledger.
    pub fn unreadable_reason(&self) -> Option<&str> {
        self.unreadable_reason.as_deref()
    }

    /// The configured company name, or `None` if never set.
    pub fn company_name(&self) -> Option<&str> {
        self.config.company_name.as_deref()
    }

    /// The configured company name, falling back to `COMPANY_NAME_FALLBACK` when unset —
    /// what the Tauri command actually returns, so the UI never has to know the fallback.
    pub fn company_name_or_default(&self) -> String {
        self.config
            .company_name
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| COMPANY_NAME_FALLBACK.to_string())
    }

    pub fn set_company_name(&mut self, name: &str) -> io::Result<()> {
        let trimmed = name.trim();
        self.config.company_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
        self.persist()
    }

    /// The dial's position. A value on disk this build has no name for answers with the
    /// DEFAULT — spec point 21: *"an unknown value degrades to a known default for that
    /// field instead of failing the whole document"*. The original string stays on disk and
    /// goes back out at the next write; see [`ConfigStore::persist`].
    pub fn assertiveness(&self) -> Assertiveness {
        match self.config.assertiveness {
            Assertiveness::Unknown => Assertiveness::default(),
            known => known,
        }
    }

    /// Refuses [`Assertiveness::Unknown`] by name: it is a READER's placeholder for a value
    /// written by some other build, and nothing in this app is entitled to mint one. Without
    /// this refusal, `set_*(Unknown)` would make `persist` restore a string the caller never
    /// asked for, which is a silent write of something nobody chose.
    pub fn set_assertiveness(&mut self, level: Assertiveness) -> io::Result<()> {
        if level == Assertiveness::Unknown {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Assertiveness::Unknown is what a value written by another build reads as; it \
                 is not a setting this build can choose",
            ));
        }
        self.config.assertiveness = level;
        self.persist()
    }

    // -----------------------------------------------------------------------------------
    // The splash's off switch, and the two timestamps that make it measurable
    // -----------------------------------------------------------------------------------

    /// Whether the opening screen shows at launch. Default on.
    pub fn splash_enabled(&self) -> bool {
        self.config.splash_enabled
    }

    /// Switch the opening screen on or off. Switching OFF stamps `splash_disabled_at`;
    /// switching back ON clears it, because a disable that was reversed is not a disable
    /// and reporting it as one would overstate the only metric this field exists to feed.
    ///
    /// Setting it to the value it already holds is not a change and does not restamp —
    /// otherwise a UI that writes the setting on every launch would keep moving the
    /// timestamp forward and time-to-disable would measure nothing.
    pub fn set_splash_enabled(&mut self, enabled: bool, now_millis: u64) -> io::Result<()> {
        if self.config.splash_enabled == enabled {
            return Ok(());
        }
        self.config.splash_enabled = enabled;
        self.config.splash_disabled_at = if enabled { None } else { Some(now_millis) };
        self.persist()
    }

    /// The first time the splash was shown, epoch millis, or `None` if it never has been.
    pub fn splash_first_shown_at(&self) -> Option<u64> {
        self.config.splash_first_shown_at
    }

    /// When he switched it off, epoch millis, or `None` if it is on (or never was off).
    pub fn splash_disabled_at(&self) -> Option<u64> {
        self.config.splash_disabled_at
    }

    /// Record that the splash has now been shown at least once. Idempotent: the FIRST
    /// call wins and every later one is a no-op that does not even touch the disk, so
    /// the surface can call it on every launch without rewriting the file each time.
    ///
    /// Returns whether it wrote.
    pub fn note_splash_shown(&mut self, now_millis: u64) -> io::Result<bool> {
        if self.config.splash_first_shown_at.is_some() {
            return Ok(false);
        }
        self.config.splash_first_shown_at = Some(now_millis);
        self.persist()?;
        Ok(true)
    }

    // ---- appearance: the two lightings and the type knob (§15) ---------------------

    /// Which lighting he chose, or `Theme::System` if he has not chosen one — §63, and
    /// that is what a fresh install answers. A value on disk this build has no name for
    /// answers with the DEFAULT (`System`) — see [`ConfigStore::assertiveness`] for the
    /// rule, and [`ConfigStore::persist`] for what keeps his actual choice alive on disk
    /// meanwhile.
    pub fn theme(&self) -> Theme {
        match self.config.theme {
            Theme::Unknown => Theme::default(),
            known => known,
        }
    }

    /// Refuses [`Theme::Unknown`] by name — see [`ConfigStore::set_assertiveness`].
    pub fn set_theme(&mut self, theme: Theme) -> io::Result<()> {
        if theme == Theme::Unknown {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Theme::Unknown is what a value written by another build reads as; it is not a \
                 setting this build can choose",
            ));
        }
        if self.config.theme == theme {
            return Ok(());
        }
        self.config.theme = theme;
        self.persist()
    }

    // ---- §119: the model each session starts on ----------------------------------------

    /// The model Rich's session starts on. Opus unless he chose Fable; a value this build has
    /// no name for answers with Opus (see [`ConfigStore::assertiveness`] for the rule).
    pub fn rich_model(&self) -> ModelChoice {
        self.config.rich_model.resolved()
    }

    /// The model Stu's session starts on, by the same rule as [`ConfigStore::rich_model`].
    pub fn stu_model(&self) -> ModelChoice {
        self.config.stu_model.resolved()
    }

    /// Both, as the spawn path reads them.
    pub fn lease_models(&self) -> LeaseModels {
        LeaseModels { rich: self.rich_model(), stu: self.stu_model() }
    }

    /// The general-settings row's write for Rich. Refuses [`ModelChoice::Unknown`] by name —
    /// see [`ConfigStore::set_assertiveness`]. Takes effect at the next session Rich starts.
    pub fn set_rich_model(&mut self, model: ModelChoice) -> io::Result<()> {
        refuse_unknown_model(model)?;
        if self.config.rich_model == model {
            return Ok(());
        }
        self.config.rich_model = model;
        self.persist()
    }

    /// **Both rows in one write**, as Preferences saves them when it closes (round 23). Either
    /// name refused refuses both, and the file is written once, so a failure can never leave
    /// one row saved and the other not.
    pub fn set_lease_models(&mut self, models: LeaseModels) -> io::Result<()> {
        refuse_unknown_model(models.rich)?;
        refuse_unknown_model(models.stu)?;
        if self.config.rich_model == models.rich && self.config.stu_model == models.stu {
            return Ok(());
        }
        self.config.rich_model = models.rich;
        self.config.stu_model = models.stu;
        // One write; on failure `persist` puts both back to what the file holds.
        self.persist()
    }

    /// The general-settings row's write for Stu, by the same rule as
    /// [`ConfigStore::set_rich_model`].
    pub fn set_stu_model(&mut self, model: ModelChoice) -> io::Result<()> {
        refuse_unknown_model(model)?;
        if self.config.stu_model == model {
            return Ok(());
        }
        self.config.stu_model = model;
        self.persist()
    }

    /// The font control's position, as a percentage of the 16px root. Always one of
    /// `FONT_SCALE_STEPS` — a value read off a hand-edited file is snapped on the way out
    /// as well as on the way in, so a caller can never observe a scale it cannot step from.
    pub fn font_scale(&self) -> u16 {
        snap_font_scale(self.config.font_scale)
    }

    pub fn set_font_scale(&mut self, pct: u16) -> io::Result<()> {
        let snapped = snap_font_scale(pct);
        if self.config.font_scale == snapped {
            return Ok(());
        }
        self.config.font_scale = snapped;
        self.persist()
    }

    // ---- the Oxford comma in what Claude writes (CEO, 2026-10-19) -------------------

    /// Whether he keeps the Oxford comma in what Claude writes. `false` on a fresh install.
    pub fn oxford_comma(&self) -> bool {
        self.config.oxford_comma
    }

    pub fn set_oxford_comma(&mut self, keep: bool) -> io::Result<()> {
        if self.config.oxford_comma == keep {
            return Ok(());
        }
        self.config.oxford_comma = keep;
        self.persist()
    }

    // ---- the Mac resources square (round 47) ---------------------------------------

    /// `"ink"` or `"readout"`. Ink when nothing was chosen or the stored word is one this build
    /// does not draw.
    pub fn resources_view(&self) -> &'static str {
        let stored = self.config.resources_view.as_deref();
        RESOURCES_VIEWS.into_iter().find(|v| Some(*v) == stored).unwrap_or(RESOURCES_VIEWS[0])
    }

    /// Save his choice. A word that is not a view is refused rather than coerced.
    pub fn set_resources_view(&mut self, view: &str) -> io::Result<()> {
        if !RESOURCES_VIEWS.contains(&view) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("unknown resources view: {view}")));
        }
        if self.config.resources_view.as_deref() == Some(view) {
            return Ok(());
        }
        self.config.resources_view = Some(view.to_string());
        self.persist()
    }

    // ---- the person at the foot of the rail ----------------------------------------

    /// The CEO's own name, or `None` when nobody has set one.
    ///
    /// There is deliberately no `user_name_or_default` twin of `company_name_or_default`.
    /// A company with no name can honestly be called "My Company"; a PERSON with no name
    /// cannot be called anything at all without inventing them. The surface asks for the
    /// `Option` and renders an unset state.
    pub fn user_name(&self) -> Option<&str> {
        self.config.user_name.as_deref().filter(|s| !s.trim().is_empty())
    }

    pub fn set_user_name(&mut self, name: &str) -> io::Result<()> {
        let trimmed = name.trim();
        self.config.user_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
        self.persist()
    }

    /// His initials for the rail circle — `None` when the name is unset, never a guess.
    pub fn user_initials(&self) -> Option<String> {
        self.user_name().and_then(initials_from)
    }

    // ---- techy mode (design §3.1) ------------------------------------------------

    /// The global default. `false` on a fresh install.
    pub fn techy_default(&self) -> bool {
        self.config.techy_default
    }

    /// Set the global default. Threads with no override follow it immediately; threads
    /// the CEO pinned are untouched, which is what makes a pin mean something.
    pub fn set_techy_default(&mut self, enabled: bool) -> io::Result<()> {
        self.config.techy_default = enabled;
        self.persist()
    }

    /// This thread's override, or `None` when it follows the default.
    pub fn techy_thread(&self, thread_id: &str) -> Option<bool> {
        self.config.techy_threads.get(thread_id).copied()
    }

    /// Pin one thread's answer, overriding the default.
    pub fn set_techy_thread(&mut self, thread_id: &str, enabled: bool) -> io::Result<()> {
        self.config.techy_threads.insert(thread_id.to_string(), enabled);
        self.persist()
    }

    /// Hand this thread back to the global default. See the module doc's §7.1 note —
    /// without this a pin would be one-way and the CEO's "all of them" switch would stop
    /// reaching any thread he had ever touched.
    pub fn clear_techy_thread(&mut self, thread_id: &str) -> io::Result<()> {
        self.config.techy_threads.remove(thread_id);
        self.persist()
    }

    /// §2.5 rule 6 / §7.4's shape: forget a thread's preference when the thread goes.
    ///
    /// **This does NOT answer §7.4** (*"does deleting a thread delete its machinery?"*) —
    /// that is the CEO's call and no delete-thread command exists (`main.rs`). This is
    /// only the preference half, so that whoever adds a delete does not leave a pin
    /// pointing at a thread that no longer exists. The journal half is
    /// [`crate::journal::MachineryJournal::delete_thread`], deliberately separate, so
    /// either answer to §7.4 can be assembled from these two without changing either.
    pub fn forget_techy_thread(&mut self, thread_id: &str) -> io::Result<()> {
        self.clear_techy_thread(thread_id)
    }

    /// This company's override, or `None` when every thread in it follows the default.
    pub fn techy_entity(&self, entity_id: &str) -> Option<bool> {
        self.config.techy_entities.get(entity_id).copied()
    }

    /// Pin one company's answer, overriding the default for every thread in it that has
    /// no pin of its own.
    pub fn set_techy_entity(&mut self, entity_id: &str, enabled: bool) -> io::Result<()> {
        self.config.techy_entities.insert(entity_id.to_string(), enabled);
        self.persist()
    }

    /// Hand this company back to the global default. The exact counterpart of
    /// [`ConfigStore::clear_techy_thread`] and there for the same reason: without it the
    /// company tier would be one-way and his *"for all threads in all companies"* switch
    /// would stop reaching any company he had ever pinned.
    pub fn clear_techy_entity(&mut self, entity_id: &str) -> io::Result<()> {
        self.config.techy_entities.remove(entity_id);
        self.persist()
    }

    /// Forget a company's preference when the company goes — the entity-tier counterpart
    /// of [`ConfigStore::forget_techy_thread`], and, like it, NOT an answer to §7.4.
    pub fn forget_techy_entity(&mut self, entity_id: &str) -> io::Result<()> {
        self.clear_techy_entity(entity_id)
    }

    /// The resolved answer for one thread: **thread pin, else company pin, else the
    /// global default** — reported with its provenance so the surface never implies a
    /// choice the CEO did not make.
    ///
    /// `entity_id` is `None` only for a thread with no company binding — a thread written
    /// before entity scoping existed (`thread::ThreadSummary::entity_id`). Such a thread
    /// skips the middle tier rather than guessing a company for it.
    pub fn techy_mode(&self, thread_id: &str, entity_id: Option<&str>) -> TechyMode {
        let default = self.config.techy_default;
        if let Some(enabled) = self.techy_thread(thread_id) {
            return TechyMode { enabled, source: TechySource::Thread, default };
        }
        if let Some(enabled) = entity_id.and_then(|id| self.techy_entity(id)) {
            return TechyMode { enabled, source: TechySource::Entity, default };
        }
        TechyMode { enabled: default, source: TechySource::Default, default }
    }

    /// **APPLY THE CEO'S CHOICE AT THE TIER HE PICKED.** One call, one write, one
    /// resolved answer back — so a surface can never leave two tiers half-applied.
    ///
    /// THE PART THAT IS A DECISION AND NOT A TRANSCRIPTION. Setting a tier is not enough
    /// on its own: he makes this choice *while looking at one conversation*, and if that
    /// conversation carries a pin at a lower tier, the switch he just flipped would
    /// change nothing on the screen he flipped it on. So applying a scope also CLEARS the
    /// pins below it **on the path to this thread, and only on that path**:
    ///
    ///   - [`TechyScope::AllCompanies`] — set the global default; clear this company's
    ///     pin and this thread's pin.
    ///   - [`TechyScope::Company`] — set this company's pin; clear this thread's pin.
    ///     The global default is untouched.
    ///   - [`TechyScope::Thread`] — set this thread's pin. Nothing else is touched.
    ///
    /// Every OTHER company's and OTHER thread's pin survives untouched, which is the
    /// guarantee `set_techy_default`'s doc has made since §3.1 and the one that makes a
    /// pin worth making. The two rules are not in tension: one is about the path he is
    /// standing on, the other about the paths he is not.
    ///
    /// A [`TechyScope::Company`] with no `entity_id` is REFUSED rather than silently
    /// promoted to the global tier — the surface must not offer a company scope for a
    /// thread that is in no company, and if it ever does, this is where that bug stops.
    pub fn apply_techy_scope(
        &mut self,
        scope: TechyScope,
        thread_id: &str,
        entity_id: Option<&str>,
        enabled: bool,
    ) -> io::Result<TechyMode> {
        match scope {
            TechyScope::AllCompanies => {
                self.config.techy_default = enabled;
                if let Some(id) = entity_id {
                    self.config.techy_entities.remove(id);
                }
                self.config.techy_threads.remove(thread_id);
            }
            TechyScope::Company => {
                let Some(id) = entity_id else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "this conversation is in no company, so there is no company to set",
                    ));
                };
                self.config.techy_entities.insert(id.to_string(), enabled);
                self.config.techy_threads.remove(thread_id);
            }
            TechyScope::Thread => {
                self.config.techy_threads.insert(thread_id.to_string(), enabled);
            }
        }
        // ONE persist for the whole application, not one per tier touched: a crash
        // between two writes would otherwise leave a scope half-applied on disk.
        self.persist()?;
        Ok(self.techy_mode(thread_id, entity_id))
    }

    // ---- the raw-payload window (design §7.2) --------------------------------------

    /// How long the technical view's stored output survives. The shipping default on an
    /// install that has never set it.
    pub fn raw_retention(&self) -> RawRetention {
        self.config.raw_retention
    }

    /// Which menu entry the stored window is, or `Custom` for one that is on no menu.
    pub fn retention_choice(&self) -> RetentionChoice {
        RetentionChoice::of(&self.config.raw_retention)
    }

    /// Set the window. Takes the WINDOW, not the menu entry, so a hand-written
    /// `{"age_days": 45}` is as settable as a click and `Custom` stays describable.
    pub fn set_raw_retention(&mut self, retention: RawRetention) -> io::Result<()> {
        self.config.raw_retention = retention;
        self.persist()
    }

    /// Set the window from a menu entry. `Custom` has no window to write and is refused —
    /// it is a description of the file, never an instruction.
    pub fn set_retention_choice(&mut self, choice: RetentionChoice) -> io::Result<bool> {
        match choice.retention() {
            Some(r) => {
                self.set_raw_retention(r)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    // ---- which company this copy of Rich works for -----------------------------------

    /// The remembered choice, VALIDATED. `None` covers three genuinely different things
    /// and deliberately collapses them at this seam, because every one of them means the
    /// same thing to the caller — there is no answer here that may be trusted:
    ///
    ///   * nobody has chosen yet (the fresh-install case, and the case that makes the app
    ///     ask);
    ///   * the stored value is not a well-formed entity id (a hand-edited file);
    ///   * the stored value parses but names nothing the caller recognizes — which this
    ///     function cannot see, so the caller checks registry membership itself and
    ///     [`ConfigStore::entity_raw`] exists so it can SAY what it is discarding.
    ///
    /// It never falls back to the first entity, the last entity or any entity. ECS §3.3.
    pub fn entity(&self) -> Option<EntityId> {
        self.config.entity.as_deref().and_then(|s| EntityId::parse(s.trim()).ok())
    }

    /// Exactly what is on disk, unvalidated — so a caller discarding a stale or malformed
    /// value can name it in a log instead of dropping it silently.
    pub fn entity_raw(&self) -> Option<&str> {
        self.config.entity.as_deref()
    }

    /// Remember the CEO's answer. Takes an `EntityId` rather than a `&str` so the write
    /// side cannot store something the read side would refuse; registry membership is the
    /// caller's check, because this crate's config layer does not own the registry.
    // ---- the home screen's entity row (CEO, 2026-09-01) ----------------------------
    //
    // Two independent per-company preferences: what a button is CALLED, and whether it is
    // THERE. Independent because he asked for them independently, and because hiding a
    // company he has relabeled must not throw the label away.

    /// His own label for this company, or `None` when he has not given one.
    ///
    /// A stored value that is empty or whitespace reads as `None` — the same treatment
    /// `user_name` gives, and for the same reason: a blank button is not a label, and a
    /// surface that rendered one would look broken rather than anonymized.
    pub fn home_entity_label(&self, entity_id: &str) -> Option<&str> {
        self.config
            .home_entity_labels
            .get(entity_id)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    /// Set (or, with `None` or an empty string, CLEAR) his label for one company. Clearing
    /// is not a separate command on purpose: "set it back to nothing" is how he undoes an
    /// anonymized screen, and a one-way override would make that impossible from the UI.
    pub fn set_home_entity_label(&mut self, entity_id: &str, label: Option<&str>) -> io::Result<()> {
        let trimmed = label.map(str::trim).filter(|s| !s.is_empty());
        match trimmed {
            Some(v) => {
                self.config.home_entity_labels.insert(entity_id.to_string(), v.to_string());
            }
            None => {
                self.config.home_entity_labels.remove(entity_id);
            }
        }
        self.persist()
    }

    /// Whether this company shows in the home screen's row. **Absent means yes.**
    pub fn home_entity_visible(&self, entity_id: &str) -> bool {
        !self.config.home_entity_hidden.get(entity_id).copied().unwrap_or(false)
    }

    /// Show or hide one company in that row. Setting it back to visible REMOVES the key
    /// rather than storing `false`, so a config file only ever carries the opinions he
    /// actually holds and the default stays readable in a diff.
    pub fn set_home_entity_visible(&mut self, entity_id: &str, visible: bool) -> io::Result<()> {
        if visible {
            self.config.home_entity_hidden.remove(entity_id);
        } else {
            self.config.home_entity_hidden.insert(entity_id.to_string(), true);
        }
        self.persist()
    }

    pub fn set_entity(&mut self, entity: &EntityId) -> io::Result<()> {
        self.config.entity = Some(entity.as_str().to_string());
        self.persist()
    }

    /// Forget it, returning the store to the state that makes the app ask again.
    pub fn clear_entity(&mut self) -> io::Result<()> {
        self.config.entity = None;
        self.persist()
    }

    /// Write the whole file.
    ///
    /// # Two things this will not do, and they are the point of spec points 5, 19 and 21
    ///
    /// **1. It does not write at all over a file this build could not read.** `fs::write` here
    /// is where the CEO's preferences actually died: the read degraded to defaults and the
    /// first `set_*` made that permanent. Now an unreadable file is left alone for the whole
    /// boot. The caller is told it was NOT saved (an `Err`); the file is
    /// intact and [`ConfigStore::readable`] is how a surface asks.
    ///
    /// **2. It does not flatten a value it cannot represent.** A `theme` or `assertiveness`
    /// this build has no name for parses to `Unknown` (spec point 21's `#[serde(other)]`);
    /// serializing that placeholder would write the literal `"unknown"` over his `"sepia"`
    /// and destroy it exactly as surely as the old path did, one field at a time instead of
    /// all of them. So the ORIGINAL string is restored from the document that was read, and
    /// only the fields this build actually understands are rewritten from memory.
    ///
    /// The restore can only ever fire on a key that came off disk: `Unknown` has no other
    /// source. [`ConfigStore::set_theme`] and [`ConfigStore::set_assertiveness`] refuse it by
    /// name, so there is no path by which this build invents one.
    fn persist(&mut self) -> io::Result<()> {
        let result = self.write();
        match result {
            Ok(()) => self.saved = self.config.clone(),
            Err(_) => self.config = self.saved.clone(),
        }
        result
    }

    fn write(&self) -> io::Result<()> {
        if !self.readable {
            // Not written, and SAID: an `Ok` here told the caller "saved" about a change that
            // is gone at the next launch (hunt part 1, finding 38).
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the settings file could not be read, so it was left untouched and this change was not saved",
            ));
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut document = serde_json::to_value(&self.config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if let (Some(object), Some(raw)) = (document.as_object_mut(), self.raw.as_ref()) {
            for key in self.keys_this_build_cannot_represent() {
                if let Some(original) = raw.get(key) {
                    object.insert(key.to_string(), original.clone());
                }
            }
        }
        let serialized = serde_json::to_string_pretty(&document)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&self.path, serialized)
    }

    /// Which keys currently hold an `Unknown` placeholder, and therefore have to be written
    /// back from the original document rather than from memory.
    ///
    /// A list rather than a trait or a macro: there are two closed enums in this file, both
    /// named in spec point 21, and a third one added later should be added here with a test
    /// beside it. The general rule the spec states — *"a closed enum in a persisted document
    /// is a schema change waiting to happen that no shape-diff will ever report"* — is what
    /// this function is the local answer to.
    fn keys_this_build_cannot_represent(&self) -> Vec<&'static str> {
        let mut keys = Vec::new();
        if self.config.theme == Theme::Unknown {
            keys.push("theme");
        }
        if self.config.assertiveness == Assertiveness::Unknown {
            keys.push("assertiveness");
        }
        if self.config.rich_model == ModelChoice::Unknown {
            keys.push("rich_model");
        }
        if self.config.stu_model == ModelChoice::Unknown {
            keys.push("stu_model");
        }
        keys
    }
}

fn refuse_unknown_model(model: ModelChoice) -> io::Result<()> {
    if model == ModelChoice::Unknown {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ModelChoice::Unknown is what a value written by another build reads as; it is not \
             a setting this build can choose",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::RetentionLimit;

    #[test]
    fn each_session_s_model_defaults_to_opus_survives_restart_and_keeps_a_model_it_cannot_name() {
        // §119: absent reads as Opus for both.
        let path = tmp_path("models");
        let mut store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.lease_models(), LeaseModels { rich: ModelChoice::Opus, stu: ModelChoice::Opus });
        store.set_stu_model(ModelChoice::Fable).unwrap();
        assert!(store.set_rich_model(ModelChoice::Unknown).is_err(), "Unknown is never a choice");
        let reopened = ConfigStore::open(&path).unwrap();
        assert_eq!(reopened.lease_models(), LeaseModels { rich: ModelChoice::Opus, stu: ModelChoice::Fable });
        assert_eq!(lease_models_from_config(path.parent().unwrap().join("no-such-dir").as_path()),
            LeaseModels::default(), "no file is the default, not a failure");

        // A model a newer build wrote reads as Opus here and goes back out unchanged.
        fs::remove_file(&path).ok();
        fs::write(&path, r#"{"rich_model":"fable-6","stu_model":"fable"}"#).unwrap();
        let mut store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.rich_model(), ModelChoice::Opus);
        assert_eq!(store.stu_model(), ModelChoice::Fable);
        store.set_stu_model(ModelChoice::Opus).unwrap();
        let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["rich_model"], "fable-6");
        assert_eq!(written["stu_model"], "opus");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn both_model_rows_are_saved_in_one_write_or_neither_is() {
        // Round 23: Preferences saves both rows as it closes. A refused name refuses both, and
        // a write that fails leaves both as the file holds them, never one row moved alone.
        let path = tmp_path("models-pair");
        let mut store = ConfigStore::open(&path).unwrap();
        let both = LeaseModels { rich: ModelChoice::Fable, stu: ModelChoice::Fable };
        store.set_lease_models(both).unwrap();
        assert_eq!(ConfigStore::open(&path).unwrap().lease_models(), both);

        let half_unknown = LeaseModels { rich: ModelChoice::Opus, stu: ModelChoice::Unknown };
        assert!(store.set_lease_models(half_unknown).is_err(), "Unknown is never a choice");
        assert_eq!(store.lease_models(), both, "the refused pair moved a row in memory");
        assert_eq!(ConfigStore::open(&path).unwrap().lease_models(), both, "the refused pair moved a row on disk");

        // A write that cannot land: a folder now stands where the file goes.
        fs::remove_file(&path).ok();
        let dir = std::env::temp_dir().join(format!("richos-config-test-blocked-{}-{}", std::process::id(), crate::util::now_millis()));
        fs::create_dir_all(&dir).unwrap();
        let blocked = dir.join("config.json");
        let mut store = ConfigStore::open(&blocked).unwrap();
        fs::create_dir_all(&blocked).unwrap();
        let opus = LeaseModels { rich: ModelChoice::Opus, stu: ModelChoice::Opus };
        assert!(store.set_lease_models(both).is_err(), "a write onto a folder reported success");
        assert_eq!(store.lease_models(), opus, "a failed write left a row moved in memory");
        fs::remove_dir_all(&dir).ok();
    }

    fn tmp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("richos-config-test-{tag}-{}-{}.json", std::process::id(), crate::util::now_millis()))
    }

    // ---- which company this copy of Rich works for ---------------------------------

    #[test]
    fn a_chosen_company_survives_the_next_boot() {
        // THE PERSISTENCE HALF of the double-click fix: he answers once, and the answer is
        // still in force on a launch with the same empty environment. Written as
        // open, set, DROP, reopen — because a getter reading back its own in-memory field
        // would pass without anything ever reaching the disk.
        let path = tmp_path("entity-persist");
        let chosen = EntityId::parse("richos").unwrap();
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.entity(), None, "a fresh install has chosen nothing");
            store.set_entity(&chosen).unwrap();
        }
        let reopened = ConfigStore::open(&path).unwrap();
        assert_eq!(reopened.entity(), Some(chosen));
        assert_eq!(reopened.entity_raw(), Some("richos"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn clearing_the_choice_returns_the_store_to_the_state_that_asks() {
        let path = tmp_path("entity-clear");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_entity(&EntityId::parse("deeply").unwrap()).unwrap();
        store.clear_entity().unwrap();
        assert_eq!(store.entity(), None);
        assert_eq!(ConfigStore::open(&path).unwrap().entity(), None);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_hand_edited_company_value_costs_the_choice_and_nothing_else() {
        // WHY THE FIELD IS A `String` AND NOT AN `EntityId`. `EntityId` deserializes
        // through `try_from = "String"` and rejects `"Rich OS"`, so typing that into the
        // file by hand would fail the parse of the WHOLE document — and `open` degrades an
        // unparseable file to defaults, which would silently take his theme, his dial and
        // his retention window with it. This test is the guard on that: the bad value is
        // discarded, every adjacent preference survives, and `entity_raw` still reports
        // what was discarded so a log can name it.
        let path = tmp_path("entity-garbage");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_theme(Theme::Light).unwrap();
            store.set_assertiveness(Assertiveness::Balanced).unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        doc["entity"] = serde_json::Value::String("Rich OS".to_string());
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.entity(), None, "an unparseable id must never resolve to an entity");
        assert_eq!(store.entity_raw(), Some("Rich OS"), "and the caller must be able to name it");
        assert_eq!(store.theme(), Theme::Light, "the adjacent preference must survive");
        assert_eq!(store.assertiveness(), Assertiveness::Balanced);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_config_file_written_before_the_company_choice_existed_still_loads() {
        // The file on the CEO's disk today has no `entity` key at all. Absent is an absent
        // ANSWER, not a corrupt file, and it must read as "nobody has chosen".
        let path = tmp_path("entity-legacy");
        std::fs::write(&path, r#"{"company_name":null,"assertiveness":"quiet","theme":"dark"}"#).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.entity(), None);
        assert_eq!(store.entity_raw(), None);
        assert_eq!(store.theme(), Theme::Dark);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn default_assertiveness_is_quiet() {
        // Independently confirm the enum default AND the on-disk-absent default agree —
        // both are load-bearing ("the CEO's dial survives restart, default = quiet").
        assert_eq!(Assertiveness::default(), Assertiveness::Quiet);
        let path = tmp_path("default");
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.assertiveness(), Assertiveness::Quiet);
        assert_eq!(store.assertiveness().as_str(), "quiet");
    }

    #[test]
    fn company_name_unset_falls_back_honestly() {
        let path = tmp_path("company-fallback");
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.company_name(), None);
        assert_eq!(store.company_name_or_default(), COMPANY_NAME_FALLBACK);
    }

    #[test]
    fn company_name_and_assertiveness_survive_restart() {
        let path = tmp_path("restart");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_company_name("Acme Robotics").unwrap();
            store.set_assertiveness(Assertiveness::Balanced).unwrap();
        }
        let reopened = ConfigStore::open(&path).unwrap();
        assert_eq!(reopened.company_name(), Some("Acme Robotics"));
        assert_eq!(reopened.company_name_or_default(), "Acme Robotics");
        assert_eq!(reopened.assertiveness(), Assertiveness::Balanced);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blank_company_name_clears_to_fallback() {
        let path = tmp_path("blank");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_company_name("Acme").unwrap();
        store.set_company_name("   ").unwrap();
        assert_eq!(store.company_name(), None);
        assert_eq!(store.company_name_or_default(), COMPANY_NAME_FALLBACK);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn assertiveness_wire_strings_match_the_ui_radio_values() {
        // app/ui/index.html: value="quiet" | "balanced" | "urgent-only" — verbatim.
        assert_eq!(Assertiveness::parse("quiet"), Some(Assertiveness::Quiet));
        assert_eq!(Assertiveness::parse("balanced"), Some(Assertiveness::Balanced));
        assert_eq!(Assertiveness::parse("urgent-only"), Some(Assertiveness::UrgentOnly));
        assert_eq!(Assertiveness::parse("bogus"), None);
    }

    // ---- techy mode (§3.1) --------------------------------------------------------

    /// A company with NO pin of its own, which is what every test below that is not about
    /// the middle tier passes: the tier has to be transparent when nothing is set in it,
    /// or every one of §3.1's original guarantees would have changed meaning on the day it
    /// was added.
    const E: Option<&str> = Some("ent_acme");

    #[test]
    fn techy_mode_is_off_on_a_fresh_install_and_says_it_follows_the_default() {
        // §3.3: with techy mode off the conversation surface is byte-identical to today,
        // so the ONLY safe default is off — and a fresh install has no override anywhere,
        // which is a different fact from "he chose off for this thread".
        let path = tmp_path("techy-fresh");
        let store = ConfigStore::open(&path).unwrap();
        assert!(!store.techy_default());
        assert_eq!(store.techy_thread("thr_1"), None);
        let mode = store.techy_mode("thr_1", Some("ent_acme"));
        assert!(!mode.enabled);
        assert_eq!(mode.source, TechySource::Default);
        assert_eq!(mode.source.as_str(), "default");
        assert!(!mode.default);
    }

    #[test]
    fn the_global_switch_reaches_every_thread_that_has_not_been_pinned() {
        // §7.1's "all" half, and the CEO's own words: "some OR ALL of their
        // conversations". One switch, not N toggles.
        let path = tmp_path("techy-global");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_default(true).unwrap();
        for thread in ["thr_a", "thr_b", "thr_never_seen_before"] {
            let mode = store.techy_mode(thread, Some("ent_acme"));
            assert!(mode.enabled, "{thread} should follow the global default");
            assert_eq!(mode.source, TechySource::Default);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_pinned_thread_keeps_its_answer_when_the_global_default_moves_under_it() {
        // §7.1's "some" half. If flipping the global switch silently rewrote the CEO's
        // per-thread choices, "per-thread" would be decoration.
        let path = tmp_path("techy-pin");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_thread("thr_pinned_off", false).unwrap();
        store.set_techy_thread("thr_pinned_on", true).unwrap();
        store.set_techy_default(true).unwrap();
        assert!(!store.techy_mode("thr_pinned_off", E).enabled, "the pin survives the switch");
        assert!(store.techy_mode("thr_pinned_on", E).enabled);
        assert!(store.techy_mode("thr_unpinned", E).enabled, "and the unpinned one follows it");
        store.set_techy_default(false).unwrap();
        assert!(store.techy_mode("thr_pinned_on", E).enabled, "in both directions");
        assert!(!store.techy_mode("thr_unpinned", E).enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clearing_an_override_hands_the_thread_back_to_the_default() {
        // The reversibility that keeps §7.1 OPEN. Without it a pin is one-way and the
        // product has quietly chosen "per-thread only".
        let path = tmp_path("techy-clear");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_default(true).unwrap();
        store.set_techy_thread("thr_1", false).unwrap();
        assert_eq!(store.techy_mode("thr_1", E).source, TechySource::Thread);
        assert!(!store.techy_mode("thr_1", E).enabled);

        store.clear_techy_thread("thr_1").unwrap();
        assert_eq!(store.techy_thread("thr_1"), None);
        let mode = store.techy_mode("thr_1", E);
        assert_eq!(mode.source, TechySource::Default);
        assert!(mode.enabled, "back under the global switch, not stuck at its old value");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pinning_a_thread_to_the_same_value_as_the_default_is_still_a_pin() {
        // The distinction `TechySource` exists for: "he chose this" and "this is what the
        // default happens to be" are different facts, and only the second one moves when
        // the global switch moves.
        let path = tmp_path("techy-samevalue");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_thread("thr_1", false).unwrap();
        assert_eq!(store.techy_mode("thr_1", E).source, TechySource::Thread);
        store.set_techy_default(true).unwrap();
        assert!(!store.techy_mode("thr_1", E).enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn techy_settings_survive_restart() {
        let path = tmp_path("techy-restart");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_techy_default(true).unwrap();
            store.set_techy_thread("thr_off", false).unwrap();
        }
        let reopened = ConfigStore::open(&path).unwrap();
        assert!(reopened.techy_default());
        assert_eq!(reopened.techy_thread("thr_off"), Some(false));
        assert!(reopened.techy_mode("thr_other", E).enabled);
        let _ = std::fs::remove_file(&path);
    }

    // ---- §7.1's THIRD TIER, on the CEO's answer of 2026-09-18 -------------------------

    #[test]
    fn the_three_tiers_resolve_in_one_order_thread_then_company_then_default() {
        // His three radio buttons are three tiers, and a tier is only a tier if the one
        // below it wins. All four combinations of (company pinned, thread pinned) are
        // here, because the interesting one is the pair that DISAGREE.
        let path = tmp_path("techy-order");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_default(false).unwrap();
        store.set_techy_entity("ent_acme", true).unwrap();

        // company pinned on, no thread pin -> the company answers, and SAYS it did
        let mode = store.techy_mode("thr_plain", Some("ent_acme"));
        assert!(mode.enabled);
        assert_eq!(mode.source, TechySource::Entity);
        assert_eq!(mode.source.as_str(), "entity");
        assert!(!mode.default, "and the global default is carried alongside, unchanged");

        // company pinned on, thread pinned off -> the THREAD wins
        store.set_techy_thread("thr_quiet", false).unwrap();
        let mode = store.techy_mode("thr_quiet", Some("ent_acme"));
        assert!(!mode.enabled);
        assert_eq!(mode.source, TechySource::Thread);

        // a DIFFERENT company, unpinned -> falls past the middle tier to the default
        let mode = store.techy_mode("thr_plain", Some("ent_other"));
        assert!(!mode.enabled);
        assert_eq!(mode.source, TechySource::Default);

        // no company at all (a thread written before entity scoping) -> same fall-through,
        // never a guess at which company it might have meant
        let mode = store.techy_mode("thr_plain", None);
        assert!(!mode.enabled);
        assert_eq!(mode.source, TechySource::Default);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_pinned_company_keeps_its_answer_when_the_global_default_moves_over_it() {
        // The same guarantee `a_pinned_thread_keeps_its_answer_...` makes one tier down,
        // and the brief asks for it by name: the company tier gets what the thread tier
        // already had, in both directions.
        let path = tmp_path("techy-company-pin");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_entity("ent_off", false).unwrap();
        store.set_techy_entity("ent_on", true).unwrap();
        store.set_techy_default(true).unwrap();
        assert!(!store.techy_mode("thr", Some("ent_off")).enabled, "the pin survives the switch");
        assert!(store.techy_mode("thr", Some("ent_on")).enabled);
        assert!(store.techy_mode("thr", Some("ent_unpinned")).enabled, "and an unpinned company follows it");
        store.set_techy_default(false).unwrap();
        assert!(store.techy_mode("thr", Some("ent_on")).enabled, "in both directions");
        assert!(!store.techy_mode("thr", Some("ent_unpinned")).enabled);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn every_tier_reverses_and_clearing_a_company_hands_it_back_to_the_default() {
        // *"That also means that later the user should be able to switch off the techy
        // mode for a given company or a given thread."* — his sentence, as arithmetic.
        let path = tmp_path("techy-company-reverse");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_techy_default(true).unwrap();

        store.set_techy_entity("ent_acme", false).unwrap();
        assert!(!store.techy_mode("thr", E).enabled, "off for a given company, under an on default");
        store.set_techy_entity("ent_acme", true).unwrap();
        assert!(store.techy_mode("thr", E).enabled, "and back on again");

        store.clear_techy_entity("ent_acme").unwrap();
        assert_eq!(store.techy_entity("ent_acme"), None);
        let mode = store.techy_mode("thr", E);
        assert_eq!(mode.source, TechySource::Default);
        assert!(mode.enabled, "back under the global switch, not stuck at its old value");

        // and `forget_` is the deletion-time alias, exactly as it is for a thread
        store.set_techy_entity("ent_acme", false).unwrap();
        store.forget_techy_entity("ent_acme").unwrap();
        assert_eq!(store.techy_entity("ent_acme"), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn applying_a_scope_clears_the_tiers_below_it_on_this_thread_and_no_others() {
        // THE DECISION IN `apply_techy_scope`, tested from both sides at once. He makes
        // this choice while looking at ONE conversation, so the choice has to be true on
        // that conversation — and every pin he made elsewhere has to survive it, which is
        // the guarantee §3.1 has made since the first tier existed.
        let path = tmp_path("techy-scope-apply");
        let mut store = ConfigStore::open(&path).unwrap();

        // The bystanders: another company, and another thread inside the one being changed.
        store.set_techy_entity("ent_other", true).unwrap();
        store.set_techy_thread("thr_elsewhere", true).unwrap();
        // The path being changed, pinned OFF at both lower tiers so an "all companies"
        // switch-on that did not clear them would be invisible on the very screen he used.
        store.set_techy_entity("ent_acme", false).unwrap();
        store.set_techy_thread("thr_here", false).unwrap();

        let mode = store
            .apply_techy_scope(TechyScope::AllCompanies, "thr_here", E, true)
            .unwrap();
        assert!(mode.enabled, "the conversation he flipped it on is ON");
        assert_eq!(mode.source, TechySource::Default, "and it is the global switch holding it");
        assert!(store.techy_default());
        assert_eq!(store.techy_entity("ent_acme"), None, "this company's pin cleared");
        assert_eq!(store.techy_thread("thr_here"), None, "this thread's pin cleared");
        assert_eq!(store.techy_entity("ent_other"), Some(true), "another company's pin untouched");
        assert_eq!(store.techy_thread("thr_elsewhere"), Some(true), "another thread's pin untouched");

        // COMPANY scope: sets the middle tier, clears this thread, leaves the global alone.
        store.set_techy_thread("thr_here", true).unwrap();
        let mode = store
            .apply_techy_scope(TechyScope::Company, "thr_here", E, false)
            .unwrap();
        assert!(!mode.enabled);
        assert_eq!(mode.source, TechySource::Entity);
        assert_eq!(store.techy_entity("ent_acme"), Some(false));
        assert_eq!(store.techy_thread("thr_here"), None, "this thread's pin cleared");
        assert!(store.techy_default(), "the global default is NOT touched by a company scope");
        assert_eq!(store.techy_entity("ent_other"), Some(true));

        // THREAD scope: touches exactly one key.
        let mode = store
            .apply_techy_scope(TechyScope::Thread, "thr_here", E, true)
            .unwrap();
        assert!(mode.enabled);
        assert_eq!(mode.source, TechySource::Thread);
        assert_eq!(store.techy_entity("ent_acme"), Some(false), "the company tier is NOT touched");
        assert!(store.techy_default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_company_scope_on_a_thread_with_no_company_is_refused_not_promoted() {
        // The surface must never offer a company scope for a thread that is in no
        // company. If it ever does, the write stops here rather than landing one tier up
        // and changing every company he has.
        let path = tmp_path("techy-scope-nocompany");
        let mut store = ConfigStore::open(&path).unwrap();
        let err = store
            .apply_techy_scope(TechyScope::Company, "thr_unbound", None, true)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!store.techy_default(), "and nothing was written one tier up instead");
        assert_eq!(store.techy_thread("thr_unbound"), None);

        // The other two scopes DO work without a company — an unbound thread can still be
        // pinned on its own, and can still follow the global switch.
        store
            .apply_techy_scope(TechyScope::AllCompanies, "thr_unbound", None, true)
            .unwrap();
        assert!(store.techy_default());
        let mode = store
            .apply_techy_scope(TechyScope::Thread, "thr_unbound", None, false)
            .unwrap();
        assert!(!mode.enabled);
        assert_eq!(mode.source, TechySource::Thread);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_preselected_scope_is_his_first_choice() {
        // *"And the first choice should be preselected after the user switches the techy
        // mode on."* Named once, in the type, so a surface cannot drift from it quietly.
        assert_eq!(TechyScope::PRESELECTED, TechyScope::AllCompanies);
        assert_eq!(TechyScope::PRESELECTED.as_str(), "all-companies");
        for scope in [TechyScope::AllCompanies, TechyScope::Company, TechyScope::Thread] {
            assert_eq!(TechyScope::parse(scope.as_str()), Some(scope), "round-trips over the wire");
        }
        assert_eq!(TechyScope::parse("everything"), None, "and an unknown scope is refused");
        assert_eq!(TechyScope::parse("Thread"), None);
    }

    #[test]
    fn company_pins_survive_restart_and_a_file_with_no_company_tier_still_opens() {
        let path = tmp_path("techy-company-restart");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_techy_entity("ent_acme", true).unwrap();
            store.set_techy_thread("thr_off", false).unwrap();
        }
        let reopened = ConfigStore::open(&path).unwrap();
        assert_eq!(reopened.techy_entity("ent_acme"), Some(true));
        assert!(reopened.techy_mode("thr_any", E).enabled);
        assert!(!reopened.techy_mode("thr_off", E).enabled);

        // The file on the CEO's disk TODAY has `techy_default` and `techy_threads` and no
        // `techy_entities`. It has to keep opening, with every other preference intact —
        // the same guarantee the pre-techy-mode file gets below.
        let older = tmp_path("techy-company-legacy");
        std::fs::write(
            &older,
            r#"{"company_name":"Acme","techy_default":true,"techy_threads":{"thr_off":false}}"#,
        )
        .unwrap();
        let store = ConfigStore::open(&older).unwrap();
        assert_eq!(store.company_name(), Some("Acme"));
        assert!(store.techy_default());
        assert_eq!(store.techy_thread("thr_off"), Some(false));
        assert_eq!(store.techy_entity("anything"), None, "no company tier is an absent opinion");
        assert!(store.techy_mode("thr_new", E).enabled);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&older);
    }

    #[test]
    fn a_config_written_before_techy_mode_existed_still_opens() {
        // The file on the CEO's disk today has neither key. `#[serde(default)]` on both is
        // what makes this true, and a missing default attribute would turn a launch into a
        // parse failure that silently degrades every OTHER preference to its default too.
        let path = tmp_path("techy-legacy");
        std::fs::write(&path, r#"{"company_name":"Acme Robotics","assertiveness":"balanced"}"#).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.company_name(), Some("Acme Robotics"));
        assert_eq!(store.assertiveness(), Assertiveness::Balanced);
        assert!(!store.techy_default());
        assert_eq!(store.techy_thread("anything"), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_splash_is_on_by_default_and_an_absent_key_is_not_an_opinion() {
        // Two different "no value" cases have to give the same answer, and the second is
        // the one a `#[serde(default)]` bool gets wrong: a config file written before the
        // splash existed has no key at all, and reading that as `false` would silently
        // disable a surface for every CEO who already had a config.json.
        let path = tmp_path("splash-default");
        let fresh = ConfigStore::open(&path).unwrap();
        assert!(fresh.splash_enabled(), "a brand-new store shows the splash");
        assert_eq!(fresh.splash_first_shown_at(), None);
        assert_eq!(fresh.splash_disabled_at(), None);

        let older = tmp_path("splash-older-config");
        std::fs::write(&older, r#"{"company_name":"Acme","assertiveness":"balanced"}"#).unwrap();
        let store = ConfigStore::open(&older).unwrap();
        assert!(store.splash_enabled(), "a pre-splash config file reads as ON, not off");
        assert_eq!(store.company_name(), Some("Acme"));
        assert_eq!(store.assertiveness(), Assertiveness::Balanced);
        let _ = std::fs::remove_file(&older);
    }

    #[test]
    fn switching_the_splash_off_survives_restart_and_stamps_when() {
        let path = tmp_path("splash-off");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_splash_enabled(false, 1_700_000_000_000).unwrap();
        }
        let reopened = ConfigStore::open(&path).unwrap();
        assert!(!reopened.splash_enabled(), "off stays off across a relaunch");
        assert_eq!(reopened.splash_disabled_at(), Some(1_700_000_000_000));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_reversed_disable_is_not_reported_as_a_disable() {
        // Time-to-disable is the whole point of the timestamp. A CEO who switched it off,
        // looked, and switched it back on has NOT disabled the surface, and leaving the
        // stamp behind would count him as though he had.
        let path = tmp_path("splash-reversed");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_splash_enabled(false, 1_700_000_000_000).unwrap();
        assert_eq!(store.splash_disabled_at(), Some(1_700_000_000_000));
        store.set_splash_enabled(true, 1_700_000_060_000).unwrap();
        assert!(store.splash_enabled());
        assert_eq!(store.splash_disabled_at(), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn setting_the_switch_to_what_it_already_is_never_moves_the_clock() {
        // The UI syncs this preference on every launch. If an idempotent write restamped,
        // the disable timestamp would walk forward with every relaunch and measure the
        // last launch rather than the decision.
        let path = tmp_path("splash-idempotent");
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_splash_enabled(false, 1_700_000_000_000).unwrap();
        store.set_splash_enabled(false, 1_799_999_999_999).unwrap();
        assert_eq!(store.splash_disabled_at(), Some(1_700_000_000_000));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_first_showing_is_recorded_once_and_only_once() {
        let path = tmp_path("splash-first-shown");
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(store.note_splash_shown(1_700_000_000_000).unwrap(), "the first call writes");
        assert!(!store.note_splash_shown(1_700_000_500_000).unwrap(), "every later call is a no-op");
        assert_eq!(store.splash_first_shown_at(), Some(1_700_000_000_000));

        let reopened = ConfigStore::open(&path).unwrap();
        assert_eq!(reopened.splash_first_shown_at(), Some(1_700_000_000_000));
        let _ = std::fs::remove_file(&path);
    }

    // ---- the raw-payload window (§7.2) ---------------------------------------------

    #[test]
    fn an_install_that_has_never_set_the_window_gets_yesterdays_behaviour() {
        // CEO 2026-10-06, feedback item 6: "'Keep the stored output' default must be forever".
        // A fresh store AND a config file written before this setting existed both read as
        // Forever on both axes, and the menu shows Forever selected. (The test's name is kept
        // so the history of this claim stays findable; its answer is the CEO's now.)
        let path = tmp_path("retention-default");
        let fresh = ConfigStore::open(&path).unwrap();
        assert_eq!(fresh.raw_retention(), RawRetention::FOREVER);
        assert_eq!(fresh.retention_choice(), RetentionChoice::Forever);

        let older = tmp_path("retention-legacy");
        std::fs::write(&older, r#"{"company_name":"Acme","assertiveness":"balanced","techy_default":true}"#).unwrap();
        let store = ConfigStore::open(&older).unwrap();
        assert_eq!(store.raw_retention(), RawRetention::default(), "an absent key is the shipping default");
        assert_eq!(store.retention_choice(), RetentionChoice::Forever);
        assert!(store.techy_default(), "and the rest of the file still reads");
        let _ = std::fs::remove_file(&older);
    }

    #[test]
    fn every_answer_to_7_2_is_a_value_the_store_can_hold_and_forever_is_one_of_them() {
        // The item, in one test. "Forever" is a real stored value that survives a restart,
        // not a sentinel someone has to remember and not a code change.
        let path = tmp_path("retention-forever");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert!(store.set_retention_choice(RetentionChoice::Forever).unwrap());
        }
        let reopened = ConfigStore::open(&path).unwrap();
        assert_eq!(reopened.raw_retention(), RawRetention::FOREVER);
        assert!(reopened.raw_retention().is_forever(), "on BOTH axes — a ceiling that still evicts is not forever");
        assert_eq!(reopened.retention_choice(), RetentionChoice::Forever);
        // And it is a word on disk, not a number to decode.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains(r#""age_days": "forever""#), "readable by hand: {on_disk}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_three_menu_entries_round_trip_and_custom_is_never_written_by_one() {
        let path = tmp_path("retention-menu");
        let mut store = ConfigStore::open(&path).unwrap();
        for choice in [RetentionChoice::TwoWeeks, RetentionChoice::ThreeMonths, RetentionChoice::Forever] {
            assert!(store.set_retention_choice(choice).unwrap());
            assert_eq!(store.retention_choice(), choice, "what was set is what is reported");
            assert_eq!(RetentionChoice::parse(choice.as_str()), Some(choice), "and the wire string round-trips");
        }
        assert_eq!(RetentionChoice::ThreeMonths.retention(), Some(RawRetention::of(THREE_MONTHS_DAYS, RAW_MAX_TOTAL_BYTES)));
        assert_eq!(THREE_MONTHS_DAYS, 90);
        // `custom` is a description, never an instruction: unparseable from the wire, and a
        // no-op that writes nothing if one is somehow constructed.
        assert_eq!(RetentionChoice::parse("custom"), None);
        assert_eq!(RetentionChoice::parse("two weeks"), None);
        assert_eq!(RetentionChoice::Custom.retention(), None);
        let before = store.raw_retention();
        assert!(!store.set_retention_choice(RetentionChoice::Custom).unwrap(), "refused");
        assert_eq!(store.raw_retention(), before, "and nothing was written");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_hand_written_window_is_reported_as_itself_and_never_rounded_to_a_button() {
        // Rounding a hand-edited window to the nearest named choice would misreport it on
        // the one screen whose job is to say what it is — and the next click on any OTHER
        // control would write the rounded value back over his.
        let path = tmp_path("retention-custom");
        std::fs::write(&path, r#"{"company_name":null,"raw_retention":{"age_days":45,"total_bytes":"forever"}}"#).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(
            store.raw_retention(),
            RawRetention { age_days: RetentionLimit::Of(45), total_bytes: RetentionLimit::Forever }
        );
        assert_eq!(store.retention_choice(), RetentionChoice::Custom);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unreadable_window_keeps_data_and_leaves_every_other_preference_alone() {
        // A `raw_retention` key of the wrong shape must not fail the parse of config.json —
        // that would degrade EVERY preference to its default, this one included, back to a
        // window that deletes. It reads as forever, and the file around it still reads.
        let path = tmp_path("retention-garbage");
        std::fs::write(&path, r#"{"company_name":"Acme","assertiveness":"balanced","raw_retention":"a fortnight"}"#).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.raw_retention(), RawRetention::FOREVER, "keeps, never deletes");
        assert_eq!(store.company_name(), Some("Acme"), "and took nothing else down with it");
        assert_eq!(store.assertiveness(), Assertiveness::Balanced);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_corrupt_config_file_keeps_the_stored_output_rather_than_evicting_on_a_guess() {
        // The asymmetry, at the file level. Every other field degrading to its default
        // costs a preference the CEO can set again; `raw_retention` degrading to 14 days
        // costs records, at the next boot, with nothing on screen to say so. A file that
        // exists and will not parse is not a fresh install and is not treated as one.
        let path = tmp_path("retention-corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.raw_retention(), RawRetention::FOREVER);
        assert_eq!(store.retention_choice(), RetentionChoice::Forever);
        // ...while the fields whose worst case is a lost preference still take their
        // defaults, exactly as they did before.
        assert_eq!(store.assertiveness(), Assertiveness::Quiet);
        assert!(store.splash_enabled());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_config_file_degrades_to_defaults_not_a_boot_failure() {
        let path = tmp_path("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.assertiveness(), Assertiveness::Quiet);
        assert_eq!(store.company_name(), None);
        // And the splash degrades to ON, not off — a corrupt preferences file must not be
        // able to silently remove a surface.
        assert!(store.splash_enabled());
        let _ = std::fs::remove_file(&path);
    }

    // ---- §15: the two lightings, the type knob, and the person -----------------------

    #[test]
    fn a_fresh_install_follows_the_system_and_sits_at_100_percent() {
        // §63, as an assertion: *"system should be the default for a freshly installed
        // app."* This test asserted `Theme::Dark` until 2026-09-19, and the sentence it
        // quoted (§15, "a newly installed app opens dark") is still true of the splash and
        // the home screen — which are clamped in the UI and are not this store's business.
        // A `Default` of `Dark` is what would hand a light-OS user a dark app.
        let path = tmp_path("appearance-fresh");
        let _ = std::fs::remove_file(&path);
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.theme(), Theme::System);
        assert_eq!(store.font_scale(), 100);
        assert_eq!(store.user_name(), None);
        assert_eq!(store.user_initials(), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_config_written_before_theming_existed_reads_as_system() {
        // The `splash_enabled` lesson, applied, with §63's answer rather than §15's: an
        // ABSENT key is an absent OPINION, and an absent opinion about lighting belongs to
        // the operating system. A file from before this field existed must not read as "the
        // CEO chose light" NOR as "the CEO chose dark" — nobody chose, and §63 is what an
        // app does when nobody has chosen.
        let path = tmp_path("appearance-legacy");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, r#"{"company_name":"FemcBoost","assertiveness":"quiet"}"#).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.theme(), Theme::System);
        assert_eq!(store.font_scale(), FONT_SCALE_DEFAULT);
        assert_eq!(store.company_name(), Some("FemcBoost"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn only_an_explicit_switch_is_remembered_and_system_is_reachable_again() {
        // §63's SECOND sentence, which is the half a default alone does not deliver:
        // *"Only if the user switches the theme to some other option, only then should the
        // app remember that and from now on that would become their default."*
        //
        // Three launches in one test ON PURPOSE, because the claim is about what survives
        // between them. The middle one is the one that would catch a build treating
        // `System` as "no preference" and refusing to store it — picking System after Dark
        // would then silently leave him on Dark forever, which is the one-way door this
        // test exists to keep open.
        let path = tmp_path("appearance-63-explicit-only");
        let _ = std::fs::remove_file(&path);

        // Launch 1: nothing chosen. Nothing is written on his behalf.
        {
            let store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.theme(), Theme::System, "an unchosen lighting is the system's");
        }
        // Launch 2: he switches to Dark. That IS the remembering.
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_theme(Theme::Dark).unwrap();
        }
        {
            let store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.theme(), Theme::Dark, "his switch is his default from now on");
        }
        // Launch 3: he picks System again, and it takes.
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_theme(Theme::System).unwrap();
        }
        {
            let store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.theme(), Theme::System, "System is a choice he can return to");
        }
        // And it is on DISK as the wire string the UI mirrors, not merely in memory. The
        // needle is built from `Theme::as_str` rather than typed, so a rename on the wire
        // reaches this test as a failure instead of as a literal nobody updated.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        let needle = format!("\"theme\":\"{}\"", Theme::System.as_str());
        assert!(
            on_disk.replace(' ', "").contains(&needle),
            "the durable file must name the choice in the form `theme-boot.js` mirrors; it held: {on_disk}"
        );
        let _ = std::fs::remove_file(&path);
    }

    // The next two tests are deliberately SEPARATE, one preference each, and the reason is
    // worth writing down: `persist()` rewrites the whole file, so a single test that set the
    // theme AND the scale passed even when `set_theme` was mutated to skip persisting —
    // the scale's write had carried the in-memory theme to disk for it. A test that cannot
    // fail for the reason it names is not a test. One setter per test, so each one's write
    // is the only write.

    #[test]
    fn the_theme_survives_a_reopen_on_its_own() {
        let path = tmp_path("appearance-durable-theme");
        let _ = std::fs::remove_file(&path);
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_theme(Theme::Light).unwrap();
        }
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.theme(), Theme::Light);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_font_scale_survives_a_reopen_on_its_own() {
        let path = tmp_path("appearance-durable-scale");
        let _ = std::fs::remove_file(&path);
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_font_scale(135).unwrap();
        }
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.font_scale(), 135);
        assert_eq!(store.theme(), Theme::System, "and the untouched preference is untouched");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_resources_square_is_ink_by_default_and_readout_survives_a_reopen() {
        let path = tmp_path("resources-view-durable");
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.resources_view(), "ink", "a first run draws Ink");
            assert!(store.set_resources_view("dock").is_err(), "a word that is not a view is refused");
            store.set_resources_view("readout").unwrap();
        }
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.resources_view(), "readout", "his choice survives a reopen");
            store.set_resources_view("ink").unwrap();
        }
        assert_eq!(ConfigStore::open(&path).unwrap().resources_view(), "ink");

        // A returning user's file from before the square (no key) reads as Ink, and a view a
        // later build wrote is drawn as Ink here and kept verbatim when something else is saved.
        let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("resources_view");
        std::fs::write(&path, json.to_string()).unwrap();
        assert_eq!(ConfigStore::open(&path).unwrap().resources_view(), "ink", "no key reads as Ink");
        json["resources_view"] = serde_json::json!("sparkline");
        std::fs::write(&path, json.to_string()).unwrap();
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert_eq!(store.resources_view(), "ink");
            store.set_oxford_comma(true).unwrap();
        }
        let kept: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(kept["resources_view"], "sparkline", "a later build's view is not flattened");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn the_oxford_comma_is_off_by_default_and_his_choice_survives_a_reopen() {
        // His words, 2026-10-19: "Default setting: Oxford comma off." A config file is what an
        // update leaves in place, so a reopen is what an update and a restart both are.
        let path = tmp_path("oxford-comma-durable");
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert!(!store.oxford_comma(), "a fresh install starts with the Oxford comma off");
            store.set_oxford_comma(true).unwrap();
        }
        {
            let mut store = ConfigStore::open(&path).unwrap();
            assert!(store.oxford_comma(), "switched on, it stays on across a reopen");
            assert_eq!(store.theme(), Theme::System, "and the untouched preference is untouched");
            store.set_oxford_comma(false).unwrap();
        }
        assert!(!ConfigStore::open(&path).unwrap().oxford_comma(), "switched back off, it stays off");

        // A config file written before this setting existed (no key at all) reads as off.
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_oxford_comma(true).unwrap();
        }
        let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(json.as_object_mut().unwrap().remove("oxford_comma").is_some(), "the key is on disk");
        std::fs::write(&path, json.to_string()).unwrap();
        assert!(!ConfigStore::open(&path).unwrap().oxford_comma(), "an older file reads as off");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn every_theme_string_round_trips_through_the_wire_form() {
        // `as_str` is what crosses to `theme-boot.js` and `parse` is what comes back. A
        // rename on one side only would leave the CEO's stored choice unreadable and
        // silently reset him to dark on the next launch.
        for t in [Theme::Dark, Theme::Light, Theme::System] {
            assert_eq!(Theme::parse(t.as_str()), Some(t));
        }
        assert_eq!(Theme::parse("Dark"), None, "the wire form is lowercase and exact");
        assert_eq!(Theme::parse(""), None);
    }

    #[test]
    fn a_scale_off_the_ladder_is_snapped_not_reset() {
        // Rounding keeps his INTENT (bigger, or smaller); rejecting would silently put him
        // back at 100% — the one outcome a hand-edited file should not produce.
        assert_eq!(snap_font_scale(133), 135);
        assert_eq!(snap_font_scale(101), 100);
        assert_eq!(snap_font_scale(0), 80, "below the ladder clamps to its bottom");
        assert_eq!(snap_font_scale(9000), 150, "above it clamps to its top");
        for step in FONT_SCALE_STEPS {
            assert_eq!(snap_font_scale(step), step, "a legal step is never moved");
        }
    }

    #[test]
    fn a_stored_scale_is_always_one_the_control_can_step_from() {
        let path = tmp_path("appearance-snap");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, r#"{"company_name":null,"font_scale":133}"#).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert!(
            FONT_SCALE_STEPS.contains(&store.font_scale()),
            "a hand-edited 133% must not become a position the +/- control cannot leave"
        );
        assert_eq!(store.font_scale(), 135);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn initials_are_first_and_last_and_never_invented() {
        assert_eq!(initials_from("Alex Booster").as_deref(), Some("AB"));
        // Two tokens at most: the pattern is first-and-last, not every word.
        assert_eq!(initials_from("Alexander James Booster").as_deref(), Some("AB"));
        // One token gives ONE letter — two letters off one word would be a fabrication.
        assert_eq!(initials_from("Alex").as_deref(), Some("A"));
        assert_eq!(initials_from("  alex   booster  ").as_deref(), Some("AB"));
        assert_eq!(initials_from("ada lovelace").as_deref(), Some("AL"));
        // NOTHING to derive from is `None`, never a placeholder. The CEO's instruction was
        // explicit: do not invent a name and do not ship "??".
        assert_eq!(initials_from(""), None);
        assert_eq!(initials_from("   "), None);
        assert_eq!(initials_from("\t\n"), None);
    }

    #[test]
    fn initials_survive_a_name_that_is_not_ascii() {
        // `chars().next()` and not `bytes()[0]`: a byte index into a multi-byte name is a
        // panic, and the first person it would fire on is a real user with a real name.
        assert_eq!(initials_from("Ólafur Árnason").as_deref(), Some("ÓÁ"));
        assert_eq!(initials_from("张 伟").as_deref(), Some("张伟"));
    }

    #[test]
    fn the_person_is_unset_until_someone_sets_them() {
        // There is no `user_name_or_default`, and this test is where that absence is
        // asserted rather than merely true. A company with no name can honestly be called
        // "My Company"; a person with no name cannot be called anything without inventing
        // them, so the store reports `None` and the surface renders the unset state.
        let path = tmp_path("person");
        let _ = std::fs::remove_file(&path);
        let mut store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.user_name(), None);
        assert_eq!(store.user_initials(), None);
        assert_eq!(
            store.company_name_or_default(),
            COMPANY_NAME_FALLBACK,
            "the COMPANY still has an honest fallback; only the person does not"
        );

        store.set_user_name("Alex Booster").unwrap();
        assert_eq!(store.user_name(), Some("Alex Booster"));
        assert_eq!(store.user_initials().as_deref(), Some("AB"));

        // Clearing it returns to unset rather than storing an empty string that would
        // render as a circle with nothing in it and a label with nothing after it.
        store.set_user_name("   ").unwrap();
        assert_eq!(store.user_name(), None);
        assert_eq!(store.user_initials(), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_person_survives_a_reopen_and_is_separate_from_the_company() {
        let path = tmp_path("person-durable");
        let _ = std::fs::remove_file(&path);
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_company_name("FemcBoost").unwrap();
            store.set_user_name("Alex Booster").unwrap();
        }
        let store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.company_name(), Some("FemcBoost"));
        assert_eq!(store.user_name(), Some("Alex Booster"));
        assert_eq!(store.user_initials().as_deref(), Some("AB"));
        let _ = std::fs::remove_file(&path);
    }

    // ---- the home screen's entity row (CEO, 2026-09-01) ----------------------------

    #[test]
    fn an_unlabelled_company_reads_as_no_override_and_never_as_a_blank() {
        let path = tmp_path("home-entity-label");
        let _ = std::fs::remove_file(&path);
        let mut store = ConfigStore::open(&path).unwrap();

        // The overwhelmingly common state: he has never opened this setting.
        assert_eq!(store.home_entity_label("femcboost"), None);

        store.set_home_entity_label("femcboost", Some("1")).unwrap();
        assert_eq!(store.home_entity_label("femcboost"), Some("1"));

        // A one-character label is the CEO's own example ("they could change the label
        // buttons to something like '1', '2', '3'"), so it is a case with a test and not
        // an edge the surface discovers.
        store.set_home_entity_label("deeply", Some("2")).unwrap();
        assert_eq!(store.home_entity_label("deeply"), Some("2"));

        // Whitespace is not a label. Storing one would render a button with nothing in it,
        // which looks broken rather than anonymized.
        store.set_home_entity_label("femcboost", Some("   ")).unwrap();
        assert_eq!(store.home_entity_label("femcboost"), None);

        // ...and clearing is the way back, because that is how he un-anonymizes the screen.
        store.set_home_entity_label("deeply", None).unwrap();
        assert_eq!(store.home_entity_label("deeply"), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_company_nobody_has_an_opinion_about_shows_on_the_home_screen() {
        let path = tmp_path("home-entity-visible");
        let _ = std::fs::remove_file(&path);
        let mut store = ConfigStore::open(&path).unwrap();

        // THE DIRECTION IS THE POINT. Stored as the HIDDEN set, so a company added to the
        // registry tomorrow appears rather than being invisible until he finds a setting.
        assert!(store.home_entity_visible("webinar-booster"));
        assert!(store.home_entity_visible("a-company-that-does-not-exist-yet"));

        store.set_home_entity_visible("webinar-booster", false).unwrap();
        assert!(!store.home_entity_visible("webinar-booster"));

        store.set_home_entity_visible("webinar-booster", true).unwrap();
        assert!(store.home_entity_visible("webinar-booster"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_label_and_the_visibility_are_independent_and_both_survive_a_reopen() {
        let path = tmp_path("home-entity-durable");
        let _ = std::fs::remove_file(&path);
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_home_entity_label("richos", Some("3")).unwrap();
            store.set_home_entity_visible("richos", false).unwrap();
            store.set_home_entity_label("prospects", Some("4")).unwrap();
        }
        let store = ConfigStore::open(&path).unwrap();
        // Hiding a company he has relabelled must not throw the label away — he asked for
        // the two as separate controls and they are separate stores.
        assert_eq!(store.home_entity_label("richos"), Some("3"));
        assert!(!store.home_entity_visible("richos"));
        assert_eq!(store.home_entity_label("prospects"), Some("4"));
        assert!(store.home_entity_visible("prospects"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_label_is_a_mask_and_never_a_rename() {
        let path = tmp_path("home-entity-mask");
        let _ = std::fs::remove_file(&path);
        let mut store = ConfigStore::open(&path).unwrap();
        store.set_entity(&EntityId::parse("femcboost").unwrap()).unwrap();
        store.set_home_entity_label("femcboost", Some("1")).unwrap();

        // The identity the whole system keys on is untouched by the label. If this ever
        // fails, an anonymized home screen has re-homed his work.
        assert_eq!(store.entity_raw(), Some("femcboost"));
        assert_eq!(store.entity().unwrap().as_str(), "femcboost");
        assert_eq!(store.company_name(), None, "the company NAME is a different field again");
        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------------------
    // POINTS 5, 19 AND 21 — THE CLOSED-ENUM WIPE, AND THE FILE THAT IS LEFT ALONE
    //
    // Spec: `richos-hq docs/plans/nightly-channel-spec-2026-09-17.md`. Point 21 walks the
    // exact failure these hold shut, in five steps: a nightly adds `Theme::Sepia`; the CEO
    // picks it; he rolls back; `from_str::<StoredConfig>` fails on the unknown VARIANT
    // (serde's unknown-FIELD tolerance does not extend to variants); `open` degraded to
    // defaults; the first `set_*` wrote them over his file. Going forward again did not
    // bring them back.
    //
    // Every negative below carries a positive control, because all of them would pass on a
    // store that simply never wrote anything at all.
    // -----------------------------------------------------------------------------------

    /// A file with four preferences in it, one of which names a theme this build has never
    /// heard of. `raw_retention` is deliberately NOT the default, so the test can tell
    /// "preserved" from "happened to match".
    fn config_with_a_sepia_theme() -> &'static str {
        r#"{
  "company_name": "Booster Labs",
  "assertiveness": "balanced",
  "theme": "sepia",
  "font_scale": 120,
  "raw_retention": { "days": null, "max_total_bytes": null }
}"#
    }

    #[test]
    fn a_theme_written_by_a_newer_build_costs_that_field_and_nothing_else() {
        let path = tmp_path("sepia-reads");
        std::fs::write(&path, config_with_a_sepia_theme()).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let store = ConfigStore::open(&path).unwrap();
        assert!(store.readable(), "the document PARSES — one unknown value is not a broken file");
        assert_eq!(store.theme(), Theme::System, "the one field it cannot name degrades to the default");
        assert_eq!(
            store.company_name(),
            Some("Booster Labs"),
            "and every other preference is his, not a default"
        );
        assert_eq!(store.assertiveness(), Assertiveness::Balanced);
        assert_eq!(store.font_scale(), 120);
        assert_eq!(store.raw_retention(), RawRetention::FOREVER, "including the one that DELETES");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "and opening the file did not write a byte"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// **THE CASE THE WHOLE POINT EXISTS FOR.** He sets some OTHER preference while a value
    /// this build cannot represent is on disk. The write must not flatten it.
    #[test]
    fn setting_another_field_writes_his_unknown_theme_back_out_verbatim() {
        let path = tmp_path("sepia-survives-a-write");
        std::fs::write(&path, config_with_a_sepia_theme()).unwrap();

        let mut store = ConfigStore::open(&path).unwrap();
        store.set_company_name("Booster Labs Ltd").unwrap();

        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            on_disk.get("theme").and_then(|v| v.as_str()),
            Some("sepia"),
            "his theme is still his theme — not \"unknown\", not \"dark\""
        );
        assert_eq!(
            on_disk.get("company_name").and_then(|v| v.as_str()),
            Some("Booster Labs Ltd"),
            "and the field he actually set did change"
        );

        // POSITIVE CONTROL: the restore is not a pin. When he picks a theme this build DOES
        // know, that is what lands, and the unknown value is gone because he replaced it.
        store.set_theme(Theme::Light).unwrap();
        let after: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after.get("theme").and_then(|v| v.as_str()), Some("light"));
        let _ = std::fs::remove_file(&path);
    }

    /// The same contract on the second closed enum named in point 21. Two enums, one rule —
    /// and a test each, because `keys_this_build_cannot_represent` is a hand-written list and
    /// a hand-written list is exactly the thing that grows a hole.
    #[test]
    fn an_assertiveness_written_by_a_newer_build_survives_a_write_too() {
        let path = tmp_path("dial-survives");
        std::fs::write(&path, r#"{"company_name":"X","assertiveness":"insistent","theme":"light"}"#).unwrap();

        let mut store = ConfigStore::open(&path).unwrap();
        assert_eq!(store.assertiveness(), Assertiveness::Quiet, "degrades to the default for the read");
        assert_eq!(store.theme(), Theme::Light, "and the fields it knows are untouched");
        store.set_font_scale(120).unwrap();

        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk.get("assertiveness").and_then(|v| v.as_str()), Some("insistent"));
        assert_eq!(on_disk.get("font_scale").and_then(|v| v.as_u64()), Some(120));
        let _ = std::fs::remove_file(&path);
    }

    /// Both placeholders at once, and this is the rollback his sentence is about: go
    /// forward, come back, change something, go forward again, and find what you left.
    #[test]
    fn a_rollback_across_two_unknown_values_loses_neither_of_them() {
        let path = tmp_path("rollback-round-trip");
        std::fs::write(
            &path,
            r#"{"company_name":"Booster","assertiveness":"insistent","theme":"sepia","font_scale":100}"#,
        )
        .unwrap();

        // The older build boots, is used, and writes.
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_font_scale(120).unwrap();
            store.set_company_name("Booster Labs").unwrap();
            store.set_splash_enabled(false, 1).unwrap();
        }
        // The newer build comes back and finds its own values where it left them.
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk.get("theme").and_then(|v| v.as_str()), Some("sepia"));
        assert_eq!(on_disk.get("assertiveness").and_then(|v| v.as_str()), Some("insistent"));
        // And the older build's own work is there too.
        assert_eq!(on_disk.get("font_scale").and_then(|v| v.as_u64()), Some(120));
        assert_eq!(on_disk.get("company_name").and_then(|v| v.as_str()), Some("Booster Labs"));
        assert_eq!(on_disk.get("splash_enabled").and_then(|v| v.as_bool()), Some(false));
        let _ = std::fs::remove_file(&path);
    }

    /// `Unknown` is a READER's verdict about somebody else's bytes. Nothing in this build may
    /// mint one — otherwise `persist` would restore a string the caller never chose.
    #[test]
    fn the_placeholder_is_not_a_setting_this_build_can_choose() {
        let path = tmp_path("refuse-placeholder");
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(store.set_theme(Theme::Unknown).is_err());
        assert!(store.set_assertiveness(Assertiveness::Unknown).is_err());
        // POSITIVE CONTROL: the real values are accepted on the same store.
        store.set_theme(Theme::Light).unwrap();
        store.set_assertiveness(Assertiveness::Balanced).unwrap();
        assert_eq!(store.theme(), Theme::Light);
        let _ = std::fs::remove_file(&path);
    }

    /// **Point 19 on this file.** A `config.json` this build cannot parse at all is left
    /// EXACTLY as it is, for the whole boot, through every setter — and the app still opens.
    #[test]
    fn an_unparseable_config_is_left_on_disk_and_nothing_is_written_over_it() {
        let path = tmp_path("unparseable");
        let original = r#"{"company_name":"Booster","theme":"light",,,}"#;
        std::fs::write(&path, original).unwrap();

        let mut store = ConfigStore::open(&path).unwrap();
        assert!(!store.readable(), "the file exists and this build cannot read it");
        assert!(store.unreadable_reason().is_some(), "and it can say so");
        assert_eq!(store.theme(), Theme::System, "the boot is served from defaults");
        assert_eq!(store.company_name(), None);
        assert_eq!(
            store.raw_retention(),
            RawRetention::FOREVER,
            "the one default that DELETES is never reconstructed from a guess"
        );

        assert!(store.set_theme(Theme::Light).is_err(), "an unwritten change is not reported as saved");
        assert!(store.set_company_name("Anything").is_err());
        assert!(store.set_font_scale(120).is_err());
        assert!(store.set_assertiveness(Assertiveness::Balanced).is_err());
        assert!(store.set_raw_retention(RawRetention::FOREVER).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            original,
            "five writes later, the file is byte-for-byte what it was"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// **Hunt part 1 v3, finding 38.** A refused change leaves memory as it was, so an
    /// identical retry is refused too instead of finding "already set" and answering `Ok`
    /// about a preference that was never written. Theme, type size and splash skip an
    /// unchanged value, which is where the false success came from.
    #[test]
    fn a_refused_change_is_refused_again_on_an_identical_retry_and_memory_keeps_the_old_value() {
        let path = tmp_path("refused-retry");
        let original = "not JSON";
        std::fs::write(&path, original).unwrap();
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(store.set_theme(Theme::Light).is_err());
        assert_eq!(store.theme(), Theme::System, "a refused change still changed memory");
        assert!(store.set_theme(Theme::Light).is_err(), "the identical retry reported 'saved'");
        assert!(store.set_font_scale(120).is_err());
        assert!(store.set_font_scale(120).is_err(), "the identical type-size retry reported 'saved'");
        assert!(store.set_splash_enabled(false, 1).is_err());
        assert!(store.set_splash_enabled(false, 2).is_err(), "the identical splash retry reported 'saved'");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert_eq!(ConfigStore::open(&path).unwrap().theme(), Theme::System);
        drop(std::fs::remove_file(&path));
    }

    /// The positive control for the test above, and it is load-bearing: every one of those
    /// assertions would pass on a `ConfigStore` that had simply stopped writing altogether.
    #[test]
    fn a_readable_config_really_is_written_to() {
        let path = tmp_path("writes-do-land");
        std::fs::write(&path, r#"{"company_name":"Booster","theme":"light"}"#).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(store.readable());
        store.set_font_scale(120).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert_ne!(after, before, "a readable file IS written");
        assert_eq!(ConfigStore::open(&path).unwrap().font_scale(), 120);
        let _ = std::fs::remove_file(&path);
    }

    /// A `theme` whose JSON TYPE is wrong is a different failure from a theme whose VALUE is
    /// unknown: `#[serde(other)]` catches an unknown variant name, not a number where a
    /// string belongs. That falls to point 19 — unreadable, untouched — and this pins which
    /// of the two happens, so nobody later assumes `#[serde(other)]` covers more than it does.
    #[test]
    fn a_theme_of_the_wrong_json_type_is_point_19_not_point_21() {
        let path = tmp_path("theme-wrong-type");
        let original = r#"{"company_name":"Booster","theme":42}"#;
        std::fs::write(&path, original).unwrap();
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(!store.readable());
        assert!(store.set_font_scale(120).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let _ = std::fs::remove_file(&path);
    }

    /// **The regression that would make this whole change the disaster it prevents.** Every
    /// `config.json` in existence was written before `schema_version` was, so an absent
    /// version MUST read as version 1. If it read as unreadable, shipping this would reset
    /// the preferences of every user it is meant to protect.
    #[test]
    fn a_config_written_before_schema_versions_existed_reads_every_preference() {
        let path = tmp_path("no-version-key");
        std::fs::write(
            &path,
            r#"{"company_name":"Booster","assertiveness":"balanced","theme":"light","font_scale":120}"#,
        )
        .unwrap();
        let store = ConfigStore::open(&path).unwrap();
        assert!(store.readable(), "a file with no version key is one of OURS, written before the key");
        assert_eq!(store.company_name(), Some("Booster"));
        assert_eq!(store.assertiveness(), Assertiveness::Balanced);
        assert_eq!(store.theme(), Theme::Light);
        assert_eq!(store.font_scale(), 120);
        let _ = std::fs::remove_file(&path);
    }

    /// Bytes that are not UTF-8 make `read_to_string` fail with something other than
    /// NotFound. That is an unreadable file, not an absent one: retention must stay FOREVER
    /// and nothing may be written over it.
    #[test]
    fn a_config_with_invalid_utf8_is_unreadable_not_a_fresh_install() {
        let path = tmp_path("invalid-utf8");
        let original: Vec<u8> = vec![b'{', 0xff, 0xfe, b'}'];
        std::fs::write(&path, &original).unwrap();
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(!store.readable());
        assert_eq!(store.raw_retention(), RawRetention::FOREVER);
        assert!(store.set_font_scale(120).is_err(), "an unreadable store refuses the change, not 'saved'");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_config_from_a_newer_schema_is_left_exactly_as_it_is() {
        let path = tmp_path("newer-schema");
        let original = r#"{"schema_version":99,"company_name":"Booster","theme":"light"}"#;
        std::fs::write(&path, original).unwrap();
        let mut store = ConfigStore::open(&path).unwrap();
        assert!(!store.readable(), "a shape this build does not know is not read");
        assert!(store.unreadable_reason().unwrap().contains("99"), "and it names the version it found");
        assert_eq!(store.company_name(), None, "nothing is read out of a document it cannot understand");
        assert!(store.set_company_name("Anything").is_err(), "an unreadable store refuses the change, not 'saved'");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "and nothing is written over it");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn this_builds_own_schema_version_round_trips() {
        let path = tmp_path("version-round-trip");
        {
            let mut store = ConfigStore::open(&path).unwrap();
            store.set_company_name("Booster").unwrap();
        }
        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            on_disk.get("schema_version").and_then(|v| v.as_u64()),
            Some(u64::from(CONFIG_SCHEMA_VERSION)),
            "a file this build writes declares its shape, so point 25 has something to check"
        );
        assert!(ConfigStore::open(&path).unwrap().readable());
        let _ = std::fs::remove_file(&path);
    }

    /// serde's messages quote the offending value. In THIS file that value is the company he
    /// works for, his own name, or a thread title — so the sentence is composed here from the
    /// parser's line and column and nothing else. Same rule `skip.rs` holds for the ledger.
    #[test]
    fn the_unreadable_reason_never_quotes_one_word_of_his_settings() {
        const SECRET: &str = "ACQUISITION-PRICE-IS-FORTY-MILLION";
        let path = tmp_path("no-content-in-the-reason");
        std::fs::write(&path, format!(r#"{{"company_name":"{SECRET}","theme":}}"#)).unwrap();
        let store = ConfigStore::open(&path).unwrap();
        let reason = store.unreadable_reason().expect("unreadable");
        assert!(!reason.contains(SECRET), "his settings must not reach a report: {reason}");
        assert!(reason.contains("line") && reason.contains("column"), "it locates the damage: {reason}");
        let _ = std::fs::remove_file(&path);
    }
}
