//! THE SPLASH PICKED AT QUIT — Sage's plan `richos-hq/docs/plans/2026-11-14-existing-splash-under-
//! half-a-second.md` ("the splash the rules picked is playing within 380 ms of the click"),
//! rules 1 and 2.
//!
//! The CEO, 2026-11-14: "THE JOB IS TO START SHOWING OUR EXISTING SPLASH SCREEN WHICH LASTS FOR AT
//! LEAST 3 SECONDS BEFORE IT SWITCHES TO THE HOME SCREEN" "IN WELL UNDER 500 MS AFTER BUTTON
//! CLICK." "We have LOTS of different splash screen and a lot more of them will be added. And
//! there a bunch of rules (or will be) that determine which splash screen needs to be shown when.
//! SO, ALL OF THAT MUST BE TAKEN INTO ACCOUNT." "when the app quits, it should already determine
//! which splash screen will be shown on the next startup."
//!
//! So the pick is made when the app QUITS and saved, with the opening of that splash (its first
//! second as pictures, `splash_opening.rs`), and the next start only reads it:
//!
//! * the rules are one function in `richos-core` ([`richos_core::launch::pick_splash`]), applied to
//!   the catalog as the library ships it (`ui/splash-library.js`, data, compiled in here and read by
//!   the page alike), so a new splash is a new entry and a new rule is a change to that function;
//! * the saved record ([`Saved`], `splash-next.json` in the data folder) says which splash, its
//!   ground and its seconds, and what its opening was made for ([`MadeFor`]): the splash, its facts,
//!   the window's size and scale, reduced motion and a fingerprint of the splash's own files;
//! * a start reads that record and the opening's manifest (both a few hundred bytes) before its
//!   window exists, and uses the opening only if it was made for this window and these files
//!   ([`LaunchPick::opening`]); otherwise it says why, and the window shows the splash's ground until
//!   the page draws the splash from its start (plan section 9).
//!
//! THE OPENINGS ARE IN THE CATALOG TOO (round 74, the CEO's order of 2026-11-14): every row of
//! `ui/openings-catalog.js` is an entry `opening/<id>` after the library's own, on the ground of
//! the library entry it is built on (the materials' are `ui/splash-materials.js`), so the user's
//! own pick is picked, saved and drawn at quit exactly as the rule's is. The page builds the same
//! entries in the same order (`ui/openings.js` `installLibrary`).
//!
//! Nothing here touches AppKit; the drawing and the playing are `splash_opening.rs`'s.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The splash's own files, fingerprinted by `build.rs` (`splash_fingerprint`).
pub const FINGERPRINT: &str = env!("RICHOS_SPLASH_FINGERPRINT");
/// The saved pick, in the data folder.
pub const RECORD_FILE: &str = "splash-next.json";
/// The saved opening: a fixed folder, fixed names, never read from the record.
pub const OPENING_DIR: &str = "splash-opening";
pub const MANIFEST_FILE: &str = "opening.json";
pub const FIRST_PICTURE: &str = "first.bgra";
const SCHEMA: u32 = 1;

/// `splash.js`'s `SPLASH_SECONDS` and `MAX_SPLASH_SECONDS` (held equal by a test below).
pub const DEFAULT_SECONDS: f64 = 3.0;
pub const MAX_SECONDS: f64 = 5.0;
/// The opening is the splash's first second (plan section 1).
pub const OPENING_MS: f64 = 1000.0;
/// A picture every thirtieth of a second (trial 3: 30 a second play on time at 1x and, as JPEG,
/// at Retina size).
pub const OPENING_FPS: f64 = 30.0;

/// The library the page draws from, compiled in. Its own test (`ui/tests/splash.js` check 1)
/// holds it to plain data after the assignment, which is what lets this read it as JSON.
const LIBRARY: &str = include_str!("../../ui/splash-library.js");
/// The material entries the openings' ladder is built on: data, in the library's own shape.
const MATERIALS: &str = include_str!("../../ui/splash-materials.js");

/// One catalog entry, as far as the shell needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub ground: String,
    pub seconds: f64,
}

/// The catalog: the library in its own order, every entry with an id and a ground, then the
/// openings in theirs. `splash.js` validates the rest of each entry; the splash suite holds this
/// list equal to the page's pool.
pub fn catalog() -> Vec<Entry> {
    catalog_from(LIBRARY, MATERIALS, crate::openings::catalog())
}

/// The entries of one data file: everything after `window.<name> =` is JSON, either the list
/// itself or an object holding it under `variations`.
fn entries_of(source: &str, name: &str) -> Vec<Entry> {
    let Some(at) = source.find(name) else { return Vec::new() };
    let rest = &source[at..];
    let Some(eq) = rest.find('=') else { return Vec::new() };
    let json = rest[eq + 1..].trim().trim_end_matches(';').trim();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(variations) = value.get("variations").unwrap_or(&value).as_array() else { return Vec::new() };
    variations
        .iter()
        .filter_map(|v| {
            let id = v.get("id")?.as_str()?.to_string();
            let tokens = v.get("tokens")?;
            let ground = tokens.get("ground")?.as_str()?.to_string();
            parse_hex(&ground)?;
            let seconds = match tokens.get("seconds") {
                Some(serde_json::Value::String(s)) => s.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0),
                _ => None,
            }
            .map(|n| n.min(MAX_SECONDS))
            .unwrap_or(DEFAULT_SECONDS);
            Some(Entry { id, ground, seconds })
        })
        .collect()
}

fn catalog_from(library: &str, materials: &str, openings: &richos_core::openings::Catalog) -> Vec<Entry> {
    let mut entries = entries_of(library, "window.RichSplashLibrary");
    let mut sources = entries.clone();
    sources.extend(entries_of(materials, "window.RichSplashMaterials"));
    for opening in &openings.openings {
        let Some(from) = openings.base_of(opening).and_then(|base| sources.iter().find(|s| s.id == base)) else { continue };
        entries.push(Entry {
            id: richos_core::openings::key(&opening.id),
            ground: from.ground.clone(),
            seconds: openings.seconds_of(opening).map(|n| n.min(MAX_SECONDS)).unwrap_or(from.seconds),
        });
    }
    entries
}

/// `#RRGGBB` as three bytes.
pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// THE PICK for a start, by the rules, from the catalog: `facts` is everything the rules read
/// (which start it is for, the user's own pick, what he has, the day).
pub fn pick_for(facts: &richos_core::launch::SplashFacts) -> Option<Entry> {
    let entries = catalog();
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let id = richos_core::launch::pick_splash(&ids, facts)?.to_string();
    entries.into_iter().find(|e| e.id == id)
}

/// The saved pick for the next start (`splash-next.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub schema: u32,
    pub id: String,
    /// Which start it was picked for (for a reader; the start does not check it).
    pub start: Option<u64>,
    /// The facts the splash shows (a number, a date, his company line). None today.
    #[serde(default)]
    pub facts: BTreeMap<String, String>,
    pub ground: String,
    pub seconds: f64,
    /// What the saved opening was made for; `None` when there is none to use.
    pub opening: Option<MadeFor>,
}

/// What an opening was drawn for. It is used only when every field matches the start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MadeFor {
    pub id: String,
    #[serde(default)]
    pub facts: BTreeMap<String, String>,
    /// The page's size in points, and the screen's scale.
    pub page: [f64; 2],
    pub scale: f64,
    pub reduce_motion: bool,
    pub fingerprint: String,
    /// Names this drawing; the manifest in the opening folder carries the same.
    pub nonce: String,
}

/// The opening folder's manifest (`splash-opening/opening.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub nonce: String,
    pub page: [f64; 2],
    pub scale: f64,
    /// The first picture's pixel size and row length (`first.bgra`, no decoding needed).
    pub first: [u32; 3],
    /// The part of the page the later pictures cover, in points from the page's top left.
    pub region: [f64; 4],
    /// The later pictures: their moment after the first frame, in ms, and their index `k`
    /// (`f-{k:03}.jpg`), in the order they play.
    pub frames: Vec<Frame>,
    /// The last picture's moment: how long the opening runs.
    pub length_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub k: u32,
    pub at_ms: f64,
}

pub fn frame_file(k: u32) -> String {
    format!("f-{k:03}.jpg")
}

/// The window a start is about to build, as far as the opening cares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub page: [f64; 2],
    pub scale: f64,
    pub reduce_motion: bool,
}

impl MadeFor {
    fn fits(&self, id: &str, target: &Target) -> Result<(), String> {
        if self.id != id {
            return Err(format!("it was made for {} and the pick is {id}", self.id));
        }
        if self.fingerprint != FINGERPRINT {
            return Err("the splash's own files changed since it was made (an update)".into());
        }
        let near = |a: f64, b: f64| (a - b).abs() < 0.5;
        if !near(self.page[0], target.page[0]) || !near(self.page[1], target.page[1]) {
            return Err(format!(
                "it was made for a {}x{} window and this one is {}x{}",
                self.page[0], self.page[1], target.page[0], target.page[1]
            ));
        }
        if (self.scale - target.scale).abs() > 0.01 {
            return Err(format!("it was made at scale {} and this display is {}", self.scale, target.scale));
        }
        if self.reduce_motion != target.reduce_motion {
            return Err("reduced motion was changed since it was made".into());
        }
        Ok(())
    }
}

/// What a start knows about its splash before its window exists.
#[derive(Debug, Clone)]
pub struct LaunchPick {
    pub id: String,
    pub ground: (u8, u8, u8),
    /// The saved pick (`true`) or the rules applied now because nothing was saved (`false`: the
    /// first start after an install, or after an update from a build that saved none; or the
    /// saved pick was for another day, see `another_day`).
    pub saved: bool,
    /// "One of mine, a different one each day": the pick saved at the last quit was that day's,
    /// and this start is on a later one, so the rules were applied now and nothing was drawn for it.
    pub another_day: bool,
    record: Option<MadeFor>,
    dir: PathBuf,
}

/// A saved opening a start may play: its folder and manifest.
#[derive(Debug, Clone)]
pub struct Opening {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

/// READ, at a start, before the window: the saved pick, or the rules' answer for this start when
/// nothing was saved. `None` only for an empty catalog. Reads one small file.
///
/// ONE CASE CHOOSES AT THE START: "one of mine, a different one each day". The quit could only
/// pick for the day it was on; a start on a later day asks the rules again (they read the day),
/// and when their answer is another opening that one is shown, from the page, with no pictures
/// drawn ahead. The same day, and every other way of starting, only reads the saved answer.
pub fn for_launch(data_dir: &Path, facts: &richos_core::launch::SplashFacts) -> Option<LaunchPick> {
    let dir = data_dir.join(OPENING_DIR);
    let daily = facts.pick.as_deref() == Some(richos_core::openings::DAILY);
    let today = if daily { pick_for(facts) } else { None };
    if let Some(saved) = read_saved(data_dir) {
        if let Some(ground) = parse_hex(&saved.ground) {
            match &today {
                Some(entry) if entry.id != saved.id => {}
                _ => return Some(LaunchPick { id: saved.id, ground, saved: true, another_day: false, record: saved.opening, dir }),
            }
        }
    }
    let another_day = today.is_some();
    let entry = match today {
        Some(entry) => entry,
        None => pick_for(facts)?,
    };
    let ground = parse_hex(&entry.ground)?;
    Some(LaunchPick { id: entry.id, ground, saved: false, another_day, record: None, dir })
}

impl LaunchPick {
    /// The saved opening, if it was made for this window and these splash files; else why not.
    /// Reads the manifest (a few hundred bytes) only when everything else matches.
    pub fn opening(&self, target: &Target) -> Result<Opening, String> {
        let Some(made) = &self.record else {
            return Err(if self.saved {
                "the last quit saved no opening (its drawing failed or ran out of time)".into()
            } else if self.another_day {
                "the opening saved at the last quit was another day's (one of his, a different one each day)".into()
            } else {
                "nothing was saved at the last quit (the first start after an install or an update)".into()
            });
        };
        made.fits(&self.id, target)?;
        let bytes = std::fs::read(self.dir.join(MANIFEST_FILE)).map_err(|e| format!("its manifest is unreadable ({e})"))?;
        let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| format!("its manifest does not parse ({e})"))?;
        if manifest.nonce != made.nonce {
            return Err("its pictures belong to another drawing".into());
        }
        Ok(Opening { dir: self.dir.clone(), manifest })
    }
}

pub fn read_saved(data_dir: &Path) -> Option<Saved> {
    let bytes = std::fs::read(data_dir.join(RECORD_FILE)).ok()?;
    let saved: Saved = serde_json::from_slice(&bytes).ok()?;
    (saved.schema == SCHEMA).then_some(saved)
}

/// Write the record whole, by a temporary file and a rename, so a start reads the old record or
/// the new one, never half of one.
pub fn write_saved(data_dir: &Path, saved: &Saved) -> std::io::Result<()> {
    let path = data_dir.join(RECORD_FILE);
    let temporary = data_dir.join(format!("{RECORD_FILE}.writing"));
    let bytes = serde_json::to_vec_pretty(saved).map_err(std::io::Error::other)?;
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, &path)
}

/// THE PICK AT QUIT, for the next start, by the rules. `shown` is what the picked splash shows
/// that its own files do not hold (`openings::shown_facts`: his company's line, the version).
pub fn next(facts: &richos_core::launch::SplashFacts, shown: impl FnOnce(&str) -> BTreeMap<String, String>) -> Option<Saved> {
    let entry = pick_for(facts)?;
    Some(Saved {
        schema: SCHEMA,
        start: facts.start,
        facts: shown(&entry.id),
        id: entry.id,
        ground: entry.ground,
        seconds: entry.seconds,
        opening: None,
    })
}

/// What the quit does: nothing (the saved pick and its opening already fit), or draw an opening
/// for `next` at `target`. Pure, so the decision is a unit test.
#[derive(Debug, Clone, PartialEq)]
pub enum AtQuit {
    Keep,
    Draw(Box<(Saved, MadeFor)>),
}

pub fn at_quit(saved: Option<&Saved>, manifest_nonce: Option<&str>, next: Saved, target: &Target, nonce: String) -> AtQuit {
    if let Some(saved) = saved {
        if let Some(made) = &saved.opening {
            if saved.id == next.id
                && saved.facts == next.facts
                && made.facts == next.facts
                && made.fits(&next.id, target).is_ok()
                && manifest_nonce == Some(made.nonce.as_str())
            {
                return AtQuit::Keep;
            }
        }
    }
    let made = MadeFor {
        id: next.id.clone(),
        facts: next.facts.clone(),
        page: target.page,
        scale: target.scale,
        reduce_motion: target.reduce_motion,
        fingerprint: FINGERPRINT.to_string(),
        nonce,
    };
    AtQuit::Draw(Box::new((next, made)))
}

/// The nonce in the saved manifest, if any.
pub fn manifest_nonce(data_dir: &Path) -> Option<String> {
    let bytes = std::fs::read(data_dir.join(OPENING_DIR).join(MANIFEST_FILE)).ok()?;
    serde_json::from_slice::<Manifest>(&bytes).ok().map(|m| m.nonce)
}

/// The moments of the opening's later pictures, in the order they are DRAWN: coarse first, then
/// finer (every 16th thirtieth, then every 8th, 4th, 2nd, then the rest), so a drawing that runs
/// out of time still covers the whole second, at fewer pictures a second.
pub fn drawing_order() -> Vec<u32> {
    let n = (OPENING_MS / 1000.0 * OPENING_FPS).round() as u32;
    let mut order = Vec::new();
    for step in [16u32, 8, 4, 2, 1] {
        for k in (step..=n).step_by(step as usize) {
            if !order.contains(&k) {
                order.push(k);
            }
        }
    }
    order
}

pub fn moment_ms(k: u32) -> f64 {
    f64::from(k) * 1000.0 / OPENING_FPS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target { page: [1400.0, 852.0], scale: 1.0, reduce_motion: false }
    }

    use richos_core::launch::SplashFacts;

    fn start(n: Option<u64>) -> SplashFacts {
        SplashFacts { start: n, ..SplashFacts::default() }
    }

    fn next_for(n: Option<u64>) -> Option<Saved> {
        next(&start(n), |_| BTreeMap::new())
    }

    #[test]
    fn the_catalog_is_the_library_in_its_own_order_then_the_openings_and_the_rule_picks_from_it() {
        let entries = catalog();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(&ids[..2], ["round-11/v1", "round-11/v2"], "the shipped library, first and in its own order");
        assert_eq!(ids.len(), 2 + 28, "then round 74's twenty-eight openings");
        assert_eq!((ids[2], ids[3], ids[8], ids[29]), ("opening/ruled-standard", "opening/midnight-suede", "opening/power-100k", "opening/material-leaf"));
        assert_eq!(pick_for(&start(Some(1))).unwrap().id, "round-11/v1");
        assert_eq!(pick_for(&start(Some(2))).unwrap().id, "round-11/v2");
        assert_eq!(pick_for(&start(Some(3))).unwrap().id, "round-11/v1");
        assert_eq!(pick_for(&start(None)).unwrap().id, "round-11/v1");
        for e in &entries {
            assert!(parse_hex(&e.ground).is_some(), "{}", e.id);
            // The library's two and the materials stay their own 3 seconds; a milestone or an
            // edition is up for 5.
            let five = ["opening/power-", "opening/jobs-", "opening/hours-", "opening/edition-"].iter().any(|p| e.id.starts_with(p));
            assert_eq!(e.seconds, if five { MAX_SECONDS } else { DEFAULT_SECONDS }, "{}", e.id);
        }
        // An opening stands on the ground of the entry it is built on.
        let ground = |id: &str| entries.iter().find(|e| e.id == id).unwrap().ground.clone();
        assert_eq!(ground("opening/ruled-standard"), ground("round-11/v1"));
        assert_eq!(ground("opening/jobs-100"), ground("round-11/v2"));
        assert_eq!(ground("opening/power-100"), ground("round-11/v1"));
    }

    #[test]
    fn his_own_opening_is_picked_saved_and_carries_what_it_shows() {
        let mine = SplashFacts { start: Some(5), pick: Some("power-100".into()), earned: vec!["ruled-standard".into(), "power-100".into()], day: Some(20_000) };
        assert_eq!(pick_for(&mine).unwrap().id, "opening/power-100");
        let saved = next(&mine, |id| BTreeMap::from([("company".to_string(), format!("Acme on {id}"))])).unwrap();
        assert_eq!((saved.id.as_str(), saved.seconds, saved.start), ("opening/power-100", 5.0, Some(5)));
        assert_eq!(saved.facts.get("company").map(String::as_str), Some("Acme on opening/power-100"));
        // A changed company line is a changed fact: the quit draws again.
        let AtQuit::Draw(drawn) = at_quit(None, None, saved.clone(), &target(), "n1".into()) else { panic!() };
        let (s, made) = *drawn;
        let kept = Saved { opening: Some(made), ..s };
        assert_eq!(at_quit(Some(&kept), Some("n1"), saved.clone(), &target(), "n2".into()), AtQuit::Keep);
        let renamed = Saved { facts: BTreeMap::from([("company".to_string(), "Acme Robotics".to_string())]), ..saved };
        assert!(matches!(at_quit(Some(&kept), Some("n1"), renamed, &target(), "n2".into()), AtQuit::Draw(..)));
    }

    #[test]
    fn a_new_entry_is_a_new_pick_with_no_change_here() {
        let library = LIBRARY.replacen(
            "\"variations\": [",
            "\"variations\": [{\"id\": \"later/v9\", \"tokens\": {\"ground\": \"#101010\", \"seconds\": \"9\"}},",
            1,
        );
        let entries = catalog_from(&library, MATERIALS, &richos_core::openings::Catalog::default());
        assert_eq!(entries[0], Entry { id: "later/v9".into(), ground: "#101010".into(), seconds: MAX_SECONDS });
        assert_eq!(entries.len(), 3);
        // And an opening added to the data is an entry too, on its base's ground.
        let more = crate::openings::CATALOG_SOURCE.replacen(
            "\"openings\": [",
            "\"openings\": [{ \"id\": \"later\", \"family\": \"house\", \"name\": \"Later\", \"base\": \"later/v9\", \"earn\": { \"start\": true } },",
            1,
        );
        let entries = catalog_from(&library, MATERIALS, &richos_core::openings::parse(&more).unwrap());
        assert_eq!(entries[3], Entry { id: "opening/later".into(), ground: "#101010".into(), seconds: MAX_SECONDS });
        assert_eq!(entries.len(), 3 + 29);
    }

    #[test]
    fn the_seconds_agree_with_the_page() {
        let page = include_str!("../../ui/splash.js");
        assert!(page.contains("var SPLASH_SECONDS = 3;") && page.contains("var MAX_SPLASH_SECONDS = 5;"));
        assert_eq!((DEFAULT_SECONDS, MAX_SECONDS), (3.0, 5.0));
    }

    #[test]
    fn a_quit_draws_only_when_something_changed() {
        let next1 = next_for(Some(3)).unwrap();
        // Nothing saved: draw.
        let AtQuit::Draw(drawn) = at_quit(None, None, next1.clone(), &target(), "n1".into()) else { panic!() };
        let (saved, made) = *drawn;
        let saved = Saved { opening: Some(made.clone()), ..saved };
        // The same pick, the same window, the same files, its pictures present: keep.
        assert_eq!(at_quit(Some(&saved), Some("n1"), next1.clone(), &target(), "n2".into()), AtQuit::Keep);
        // Its pictures belong to another drawing: draw.
        assert!(matches!(at_quit(Some(&saved), Some("other"), next1.clone(), &target(), "n2".into()), AtQuit::Draw(..)));
        // A different splash for the next start (start 1 to start 2 does this today): draw it.
        let AtQuit::Draw(drawn2) = at_quit(Some(&saved), Some("n1"), next_for(Some(2)).unwrap(), &target(), "n2".into()) else { panic!() };
        let (s2, m2) = *drawn2;
        assert_eq!((s2.id.as_str(), m2.id.as_str(), m2.nonce.as_str()), ("round-11/v2", "round-11/v2", "n2"));
        // A resized window, another scale, reduced motion switched: draw.
        let bigger = Target { page: [1200.0, 800.0], ..target() };
        assert!(matches!(at_quit(Some(&saved), Some("n1"), next1.clone(), &bigger, "n2".into()), AtQuit::Draw(..)));
        let retina = Target { scale: 2.0, ..target() };
        assert!(matches!(at_quit(Some(&saved), Some("n1"), next1.clone(), &retina, "n2".into()), AtQuit::Draw(..)));
        let still = Target { reduce_motion: true, ..target() };
        assert!(matches!(at_quit(Some(&saved), Some("n1"), next1, &still, "n2".into()), AtQuit::Draw(..)));
    }

    #[test]
    fn a_start_uses_the_opening_only_when_it_was_made_for_it_and_says_why_not() {
        let dir = std::env::temp_dir().join(format!("richos-splash-pick-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(OPENING_DIR)).unwrap();
        // Nothing saved: the rules for this start, no opening.
        let first = for_launch(&dir, &start(Some(1))).unwrap();
        assert_eq!((first.id.as_str(), first.saved, first.ground), ("round-11/v1", false, (0x0C, 0x13, 0x22)));
        assert!(first.opening(&target()).unwrap_err().contains("first start after an install"));
        // Saved with an opening made for this window.
        let AtQuit::Draw(drawn) = at_quit(None, None, next_for(Some(2)).unwrap(), &target(), "n7".into()) else { panic!() };
        let (saved, made) = *drawn;
        write_saved(&dir, &Saved { opening: Some(made), ..saved }).unwrap();
        let manifest = Manifest { nonce: "n7".into(), page: [1400.0, 852.0], scale: 1.0, first: [1400, 852, 5600],
                                  region: [100.0, 100.0, 800.0, 700.0], frames: vec![Frame { k: 1, at_ms: 33.3 }], length_ms: 33.3 };
        std::fs::write(dir.join(OPENING_DIR).join(MANIFEST_FILE), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let pick = for_launch(&dir, &start(Some(9))).unwrap();
        assert_eq!((pick.id.as_str(), pick.saved), ("round-11/v2", true), "the saved pick wins; nothing is chosen at the start");
        assert_eq!(pick.opening(&target()).unwrap().manifest, manifest);
        let resized = Target { page: [1100.0, 800.0], ..target() };
        assert!(pick.opening(&resized).unwrap_err().contains("1400x852 window"));
        // Pictures from another drawing are never played.
        std::fs::write(dir.join(OPENING_DIR).join(MANIFEST_FILE), serde_json::to_vec(&Manifest { nonce: "x".into(), ..manifest }).unwrap()).unwrap();
        assert!(pick.opening(&target()).unwrap_err().contains("another drawing"));
        // A record from changed splash files (an update that changed them).
        let mut changed = read_saved(&dir).unwrap();
        changed.opening.as_mut().unwrap().fingerprint = "0".into();
        write_saved(&dir, &changed).unwrap();
        assert!(for_launch(&dir, &start(None)).unwrap().opening(&target()).unwrap_err().contains("files changed"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_different_one_each_day_is_asked_again_on_a_later_day_and_only_then() {
        let dir = std::env::temp_dir().join(format!("richos-splash-pick-daily-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(OPENING_DIR)).unwrap();
        let on = |day| SplashFacts { start: Some(9), pick: Some("daily".into()), earned: vec!["ruled-standard".into(), "midnight-suede".into(), "power-100".into()], day: Some(day) };
        // The quit, on day 30,000: that day's opening, saved with its pictures.
        let quit = next(&on(30_000), |_| BTreeMap::new()).unwrap();
        assert_eq!(quit.id, "opening/ruled-standard");
        let AtQuit::Draw(drawn) = at_quit(None, None, quit, &target(), "n1".into()) else { panic!() };
        let (saved, made) = *drawn;
        write_saved(&dir, &Saved { opening: Some(made), ..saved }).unwrap();
        // A start the same day reads the saved answer.
        let same = for_launch(&dir, &on(30_000)).unwrap();
        assert_eq!((same.id.as_str(), same.saved, same.another_day), ("opening/ruled-standard", true, false));
        // A start the next day shows that day's, and says why no pictures play.
        let later = for_launch(&dir, &on(30_001)).unwrap();
        assert_eq!((later.id.as_str(), later.saved, later.another_day), ("opening/midnight-suede", false, true));
        assert_eq!(later.ground, parse_hex(&catalog()[1].ground).unwrap());
        assert!(later.opening(&target()).unwrap_err().contains("another day"));
        // Three days on it has come round to the saved one again: the saved answer, pictures and all.
        assert!(for_launch(&dir, &on(30_003)).unwrap().saved);
        // A user who picked one opening is never asked again, whatever the day.
        let fixed = SplashFacts { pick: Some("power-100".into()), ..on(30_500) };
        assert_eq!(for_launch(&dir, &fixed).unwrap().id, "opening/ruled-standard", "the saved pick wins; nothing is chosen at the start");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_drawing_order_covers_the_second_coarse_first() {
        let order = drawing_order();
        assert_eq!(order.len(), 30);
        assert_eq!(&order[..3], &[16, 8, 24]);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, (1..=30).collect::<Vec<_>>());
        assert!((moment_ms(30) - OPENING_MS).abs() < 1e-9);
    }
}
