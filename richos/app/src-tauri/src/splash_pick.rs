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

/// One catalog entry, as far as the shell needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub ground: String,
    pub seconds: f64,
}

/// The catalog, in the library's own order: every entry with an id and a ground. `splash.js`
/// validates the rest of each entry; the splash suite holds this list equal to the page's pool.
pub fn catalog() -> Vec<Entry> {
    catalog_from(LIBRARY)
}

fn catalog_from(source: &str) -> Vec<Entry> {
    let Some(at) = source.find("window.RichSplashLibrary") else { return Vec::new() };
    let rest = &source[at..];
    let Some(eq) = rest.find('=') else { return Vec::new() };
    let json = rest[eq + 1..].trim().trim_end_matches(';').trim();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(variations) = value.get("variations").and_then(|v| v.as_array()) else { return Vec::new() };
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

/// `#RRGGBB` as three bytes.
pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// THE PICK for a start, by the rules, from the catalog: `start` is which start it is for.
pub fn pick_for(start: Option<u64>) -> Option<Entry> {
    let entries = catalog();
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let id = richos_core::launch::pick_splash(&ids, &richos_core::launch::SplashFacts { start })?.to_string();
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
    /// first start after an install, or after an update from a build that saved none).
    pub saved: bool,
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
pub fn for_launch(data_dir: &Path, start: Option<u64>) -> Option<LaunchPick> {
    let dir = data_dir.join(OPENING_DIR);
    if let Some(saved) = read_saved(data_dir) {
        if let Some(ground) = parse_hex(&saved.ground) {
            return Some(LaunchPick { id: saved.id, ground, saved: true, record: saved.opening, dir });
        }
    }
    let entry = pick_for(start)?;
    let ground = parse_hex(&entry.ground)?;
    Some(LaunchPick { id: entry.id, ground, saved: false, record: None, dir })
}

impl LaunchPick {
    /// The saved opening, if it was made for this window and these splash files; else why not.
    /// Reads the manifest (a few hundred bytes) only when everything else matches.
    pub fn opening(&self, target: &Target) -> Result<Opening, String> {
        let Some(made) = &self.record else {
            return Err(if self.saved {
                "the last quit saved no opening (its drawing failed or ran out of time)".into()
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

/// THE PICK AT QUIT, for the next start, by the rules.
pub fn next(start: Option<u64>) -> Option<Saved> {
    let entry = pick_for(start)?;
    Some(Saved {
        schema: SCHEMA,
        id: entry.id,
        start,
        facts: BTreeMap::new(),
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

    #[test]
    fn the_catalog_is_the_library_in_its_own_order_and_the_rule_picks_from_it() {
        let ids: Vec<String> = catalog().into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["round-11/v1", "round-11/v2"], "the shipped library");
        assert_eq!(pick_for(Some(1)).unwrap().id, "round-11/v1");
        assert_eq!(pick_for(Some(2)).unwrap().id, "round-11/v2");
        assert_eq!(pick_for(Some(3)).unwrap().id, "round-11/v1");
        assert_eq!(pick_for(None).unwrap().id, "round-11/v1");
        for e in catalog() {
            assert_eq!(e.seconds, DEFAULT_SECONDS, "{} asks for no seconds of its own", e.id);
            assert!(parse_hex(&e.ground).is_some());
        }
    }

    #[test]
    fn a_new_entry_is_a_new_pick_with_no_change_here() {
        let library = LIBRARY.replacen(
            "\"variations\": [",
            "\"variations\": [{\"id\": \"later/v9\", \"tokens\": {\"ground\": \"#101010\", \"seconds\": \"9\"}},",
            1,
        );
        let entries = catalog_from(&library);
        assert_eq!(entries[0], Entry { id: "later/v9".into(), ground: "#101010".into(), seconds: MAX_SECONDS });
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn the_seconds_agree_with_the_page() {
        let page = include_str!("../../ui/splash.js");
        assert!(page.contains("var SPLASH_SECONDS = 3;") && page.contains("var MAX_SPLASH_SECONDS = 5;"));
        assert_eq!((DEFAULT_SECONDS, MAX_SECONDS), (3.0, 5.0));
    }

    #[test]
    fn a_quit_draws_only_when_something_changed() {
        let next1 = next(Some(3)).unwrap();
        // Nothing saved: draw.
        let AtQuit::Draw(drawn) = at_quit(None, None, next1.clone(), &target(), "n1".into()) else { panic!() };
        let (saved, made) = *drawn;
        let saved = Saved { opening: Some(made.clone()), ..saved };
        // The same pick, the same window, the same files, its pictures present: keep.
        assert_eq!(at_quit(Some(&saved), Some("n1"), next1.clone(), &target(), "n2".into()), AtQuit::Keep);
        // Its pictures belong to another drawing: draw.
        assert!(matches!(at_quit(Some(&saved), Some("other"), next1.clone(), &target(), "n2".into()), AtQuit::Draw(..)));
        // A different splash for the next start (start 1 to start 2 does this today): draw it.
        let AtQuit::Draw(drawn2) = at_quit(Some(&saved), Some("n1"), next(Some(2)).unwrap(), &target(), "n2".into()) else { panic!() };
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
        let first = for_launch(&dir, Some(1)).unwrap();
        assert_eq!((first.id.as_str(), first.saved, first.ground), ("round-11/v1", false, (0x0C, 0x13, 0x22)));
        assert!(first.opening(&target()).unwrap_err().contains("first start after an install"));
        // Saved with an opening made for this window.
        let AtQuit::Draw(drawn) = at_quit(None, None, next(Some(2)).unwrap(), &target(), "n7".into()) else { panic!() };
        let (saved, made) = *drawn;
        write_saved(&dir, &Saved { opening: Some(made), ..saved }).unwrap();
        let manifest = Manifest { nonce: "n7".into(), page: [1400.0, 852.0], scale: 1.0, first: [1400, 852, 5600],
                                  region: [100.0, 100.0, 800.0, 700.0], frames: vec![Frame { k: 1, at_ms: 33.3 }], length_ms: 33.3 };
        std::fs::write(dir.join(OPENING_DIR).join(MANIFEST_FILE), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let pick = for_launch(&dir, Some(9)).unwrap();
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
        assert!(for_launch(&dir, None).unwrap().opening(&target()).unwrap_err().contains("files changed"));
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
