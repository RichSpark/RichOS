//! FIRST-RUN SETUP — every failure path exercised, none described.
//!
//! The three seams in `setup.rs` ([`Fetcher`], [`Runner`], [`Extractor`]) exist for exactly
//! this: "no network", "a 404", "a truncated body", "a tampered body", "an installer that
//! refuses" and "an archive with the wrong thing in it" are **values** here, so none of them
//! needs a network condition somebody has to arrange, and none can be skipped because it was
//! inconvenient to reproduce.
//!
//! It is an INTEGRATION test rather than a `#[cfg(test)]` module on purpose: everything below
//! goes through the crate's public surface, which proves the surface is sufficient for the
//! Tauri layer that consumes it, and keeps a private helper from being tested in place of the
//! thing that ships.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use richos_core::setup::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "richos-setup-{name}-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A directory that passes [`engine_looks_valid`], shaped the way the real one is.
fn make_engine(at: &Path, version: &str) {
    std::fs::create_dir_all(at.join("scripts/hooks")).unwrap();
    std::fs::write(at.join("VERSION"), format!("{version}\n")).unwrap();
}

/// **THE USABILITY SEAM, held open.** `engine_is_usable` in the product is
/// `runtime::verify_engine` — component contracts plus a hashed 322 MB runtime inventory,
/// which no unit test can or should construct. Detection's job is to pick a candidate and
/// report honestly about the ones it passed over; WHICH predicate says "usable" is the
/// caller's, so these tests supply their own and exercise the choosing.
fn every_engine_is_usable(_: &Path) -> Result<(), String> {
    Ok(())
}

/// **THE SPEECH-MODEL SEAM**, held open the same way: the product asks
/// `richos_voice::stt::readiness()`, which needs 574 MB of pinned weights on disk.
fn a_speech_model() -> Result<String, String> {
    Ok("small.en for voice and large-v3-turbo-q5_0 for transcription".to_string())
}

fn no_speech_model() -> Result<String, String> {
    Err("for transcription, whisper model not found: ggml-large-v3-turbo-q5_0.bin".to_string())
}

/// One published yt-dlp nightly, for installing the real way ([`richos_core::media_tools::refresh`]).
struct OneNightly(&'static [u8]);

const NIGHTLY_TAG: &str = "2026.10.07.000000";

impl Fetcher for OneNightly {
    fn fetch(&self, url: &str, dest: &Path) -> Result<u64, SetupError> {
        let body = if url.ends_with("/SHA2-256SUMS") {
            use sha2::{Digest, Sha256};
            let hex: String = Sha256::digest(self.0).iter().map(|b| format!("{b:02x}")).collect();
            format!("{hex}  yt-dlp\n").into_bytes()
        } else {
            self.0.to_vec()
        };
        std::fs::write(dest, &body).unwrap();
        Ok(body.len() as u64)
    }
}

impl richos_core::media_tools::LatestRelease for OneNightly {
    fn latest_tag(&self) -> Result<String, SetupError> {
        Ok(NIGHTLY_TAG.to_string())
    }
}

/// yt-dlp installed into `home`'s tools folder by the same code the app runs.
fn install_yt_dlp(home: &Path) {
    let tools = richos_core::media_tools::tools_dir(home);
    richos_core::media_tools::refresh(&OneNightly(b"a yt-dlp zipapp"), &OneNightly(b"a yt-dlp zipapp"), &tools)
        .unwrap();
}

/// A predicate that rejects the named paths, with the reason the real one would give for a
/// directory carrying no delivered runtime. This is the shape of the nightly's D1: a
/// plugin-installed engine at `~/.claude/richos-engine` is engine-SHAPED and unusable.
fn usable_except(rejected: Vec<PathBuf>) -> impl Fn(&Path) -> Result<(), String> {
    move |dir: &Path| {
        if rejected.iter().any(|r| r == dir) {
            Err("the selected engine has no delivered runtimes".to_string())
        } else {
            Ok(())
        }
    }
}

/// Every file beneath `dir`, path and CONTENTS, sorted — so "his data is untouched" can be an
/// equality over bytes rather than a claim about a directory still existing.
fn walk_files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() => {
                    let bytes = std::fs::read(&path).unwrap_or_default();
                    out.push((path, bytes));
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

fn siblings_of(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

const A_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const ASSET_URL: &str =
    "https://github.com/WebDevBooster/richos/releases/download/engine-v1.0.0/richos-engine-1.0.0.tar.gz";

fn a_pin(sha: &str) -> EnginePin {
    pin_from_parts("1.0.0", ASSET_URL, sha).expect("the fixture pin must itself be well formed")
}

// ===========================================================================================
// FAKES
// ===========================================================================================

/// Writes fixed bytes. The URL it was asked for is recorded, so a test can assert that
/// Anthropic's own URL is what gets fetched and nothing else.
struct FakeFetcher {
    body: Vec<u8>,
    asked: Mutex<Vec<String>>,
}
impl FakeFetcher {
    fn new(body: &[u8]) -> Self {
        FakeFetcher { body: body.to_vec(), asked: Mutex::new(Vec::new()) }
    }
}
impl Fetcher for FakeFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<u64, SetupError> {
        self.asked.lock().unwrap().push(url.to_string());
        std::fs::write(dest, &self.body).unwrap();
        Ok(self.body.len() as u64)
    }
}

/// Fails, with whatever the test wants to happen.
struct FailingFetcher(fn(&str) -> SetupError);
impl Fetcher for FailingFetcher {
    fn fetch(&self, url: &str, _dest: &Path) -> Result<u64, SetupError> {
        Err((self.0)(url))
    }
}

/// Copies a prepared directory tree in, instead of running `tar`.
struct FakeExtractor {
    /// What to place under `into`: (relative path, is_dir, contents).
    plan: Vec<(String, bool, String)>,
}
impl Extractor for FakeExtractor {
    fn extract(&self, _archive: &Path, into: &Path) -> Result<(), SetupError> {
        for (rel, is_dir, body) in &self.plan {
            let p = into.join(rel);
            if *is_dir {
                std::fs::create_dir_all(&p).unwrap();
            } else {
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(&p, body).unwrap();
            }
        }
        Ok(())
    }
}

fn good_engine_plan(version: &str) -> FakeExtractor {
    FakeExtractor {
        plan: vec![
            ("engine/scripts/hooks".into(), true, String::new()),
            ("engine/VERSION".into(), false, format!("{version}\n")),
            ("engine/README.md".into(), false, "the engine\n".into()),
        ],
    }
}

struct FakeRunner {
    code: i32,
    stderr: String,
    /// Run when the installer "runs", so a test can make it produce a binary — or not.
    effect: Option<Box<dyn Fn() + Send + Sync>>,
}
impl Runner for FakeRunner {
    fn run_installer(&self, _script: &Path, _home: &Path) -> Result<(i32, String), SetupError> {
        if let Some(f) = &self.effect {
            f();
        }
        Ok((self.code, self.stderr.clone()))
    }
}

/// The bytes a test will serve, plus their true digest — so no test ever hard-codes a digest
/// it did not compute from the same bytes it is about to hand over.
fn body_and_digest(root: &Path, body: &[u8]) -> String {
    let p = root.join("body-under-test");
    std::fs::write(&p, body).unwrap();
    sha256_file(&p).unwrap()
}

// ===========================================================================================
// THE GAP: detection
// ===========================================================================================

/// **THE LAUNCH BLOCKER, AS A VALUE.** A customer's Mac: a real HOME, nothing installed, a GUI
/// launch with an `.app` in `/Applications` and no environment. All three components missing,
/// each naming every place it looked, and `needs()` putting Claude Code first and the video
/// tools last (media-tools plan §2: they need the engine's Python and `whisper-cli`).
#[test]
fn a_customers_mac_is_missing_both_and_says_where_it_looked() {
    let root = scratch("customer-mac");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let exe = root.join("Applications/RichOS.app/Contents/MacOS/richos-tauri");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();

    let paths = SetupPaths {
        home: Some(home.clone()),
        exe: Some(exe),
        path_var: Some("/usr/bin:/bin".into()),
        ..Default::default()
    };
    let status = detect(&paths, &[], &every_engine_is_usable, &no_speech_model);

    assert!(!status.claude.present, "{:?}", status.claude);
    assert!(!status.engine.present, "{:?}", status.engine);
    assert!(!status.media_tools.present, "{:?}", status.media_tools);
    assert_eq!(status.needs(), vec![Component::ClaudeCode, Component::Engine, Component::MediaTools]);
    assert!(!status.complete());
    let media_places = status.media_tools.looked_in.join(" | ");
    assert!(media_places.contains("Application Support/RichOS/tools/yt-dlp"), "{media_places}");
    assert!(media_places.contains("the speech models — for transcription, whisper model not found"), "{media_places}");

    // NAMED, not "not found".
    let claude_places = status.claude.looked_in.join(" ");
    assert!(claude_places.contains(".local/bin/claude"), "{claude_places}");
    assert!(claude_places.contains("/usr/bin/claude"), "{claude_places}");
    let engine_places = status.engine.looked_in.join(" ");
    assert!(engine_places.contains("Contents/Resources/engine"), "{engine_places}");
    assert!(engine_places.contains(".claude/richos-engine"), "{engine_places}");
    assert!(
        engine_places.contains("Application Support/RichOS/engine"),
        "candidate 7 must be searched, or an install nobody can find is possible: {engine_places}"
    );
}

/// **THE CEO'S OWN MACHINE**: `~/.claude/richos-engine` is a valid engine and `claude` is where
/// Anthropic's installer puts it. Nothing is missing, so first run says nothing.
#[test]
fn a_machine_that_already_has_both_is_asked_nothing() {
    let root = scratch("has-both");
    let home = root.join("home");
    make_engine(&home.join(".claude/richos-engine"), "1.0.0");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::fs::write(home.join(".local/bin/claude"), b"#!/bin/sh\n").unwrap();
    install_yt_dlp(&home);

    let paths = SetupPaths { home: Some(home), ..Default::default() };
    let status = detect(&paths, &[], &every_engine_is_usable, &a_speech_model);
    assert!(status.complete(), "{status:?}");
    assert!(status.needs().is_empty());
    assert!(!status.blocked());
    assert_eq!(
        status.media_tools.detail.as_deref(),
        Some(format!("yt-dlp nightly {NIGHTLY_TAG}, speech models small.en for voice and large-v3-turbo-q5_0 for transcription").as_str())
    );
}

/// **AN INSTALL THAT FINISHED SETUP BEFORE 2026-10-07 IS ASKED ONCE MORE, FOR THE VIDEO TOOLS
/// ONLY** (media-tools plan §2: "Installs that already finished setup ... see the sheet once
/// on their next launch, listing only the video tools"). Claude Code and the engine are both
/// there, so they are not asked for again.
#[test]
fn a_machine_set_up_before_the_video_tools_is_asked_for_them_alone() {
    let root = scratch("set-up-before");
    let home = root.join("home");
    make_engine(&home.join(".claude/richos-engine"), "1.0.0");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::fs::write(home.join(".local/bin/claude"), b"#!/bin/sh\n").unwrap();
    let paths = SetupPaths { home: Some(home.clone()), ..Default::default() };

    let before = detect(&paths, &[], &every_engine_is_usable, &no_speech_model);
    assert_eq!(before.needs(), vec![Component::MediaTools], "{before:?}");
    assert!(!before.complete());
    assert!(!before.blocked(), "the video tools are always installable; nothing is pinned at build time");

    // yt-dlp alone is not enough: the speech models are the other half.
    install_yt_dlp(&home);
    let half = detect(&paths, &[], &every_engine_is_usable, &no_speech_model);
    assert_eq!(half.needs(), vec![Component::MediaTools], "{half:?}");
    let places = half.media_tools.looked_in.join(" | ");
    assert!(places.contains(&format!("yt-dlp nightly {NIGHTLY_TAG} is installed")), "{places}");
    assert!(places.contains("the speech models — for transcription, whisper model not found"), "{places}");

    // ...and the model alone is not enough either: a yt-dlp that does not match its record.
    let tools = richos_core::media_tools::tools_dir(&home);
    let version = std::fs::read_dir(&tools).unwrap().flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("yt-dlp-")).unwrap().path();
    std::fs::write(&version, b"tampered").unwrap();
    let tampered = detect(&paths, &[], &every_engine_is_usable, &a_speech_model);
    assert_eq!(tampered.needs(), vec![Component::MediaTools], "{tampered:?}");

    // Both halves: complete.
    install_yt_dlp(&home);
    let after = detect(&paths, &[], &every_engine_is_usable, &a_speech_model);
    assert!(after.complete(), "{after:?}");
}

/// **NO HOME, NO TOOLS FOLDER, AND IT SAYS SO** rather than reporting an empty list of places.
#[test]
fn the_video_tools_with_no_home_name_nowhere_rather_than_nothing() {
    let status = find_media_tools(&SetupPaths::default(), &a_speech_model);
    assert!(!status.present);
    assert_eq!(status.looked_in.len(), 1, "{:?}", status.looked_in);
    assert!(status.looked_in[0].contains(Component::MediaTools.display_name()), "{:?}", status.looked_in);
}

/// **UNTIL DICTATION IS THERE, THE CONSENT LINE IS THE MEDIA-TOOLS PLAN'S, AND MEETS THE SHEET'S
/// FLOOR**: no path, no digit, no terminal (the same rule `setup_view.rs` holds the other two
/// components to). It was what shipped until slice 5 set `DICTATION_READY` true (2026-10-08);
/// the constant decides which line `display_name()` and `why()` give, as the last assertion says.
#[test]
fn the_video_tools_consent_line_is_the_plans() {
    assert_eq!(Component::MediaTools.display_name_for(false), "my video tools");
    assert_eq!(
        Component::MediaTools.why_for(false),
        "the tools I use to watch, hear and download videos for you, which I keep up to date myself."
    );
    assert_eq!(Component::MediaTools.as_str(), "media-tools");
    assert_eq!(
        Component::MediaTools.display_name(),
        Component::MediaTools.display_name_for(richos_core::dictation_ready())
    );
}

/// **ONCE DICTATION IS THERE, THE STEP IS HIS, WORD FOR WORD** (round 19, the CEO, 2026-10-08):
/// *"My voice and video tools: the tools I use to watch and download videos for you. Plus, it
/// gives you a free & private/local replacement for Wispr Flow. So, it saves you $140+/year👍 and
/// allows you to talk instead of typing anywhere on this computer."* The sheet renders the name,
/// capitalized, then the line, so the two together are his sentence.
#[test]
fn the_voice_and_video_tools_step_is_his_sentence_word_for_word() {
    let his = "My voice and video tools: the tools I use to watch and download videos for you. Plus, it gives you a free & private/local replacement for Wispr Flow. So, it saves you $140+/year👍 and allows you to talk instead of typing anywhere on this computer.";
    assert_eq!(Component::MediaTools.display_name_for(true), "my voice and video tools");
    assert_eq!(Component::MediaTools.why_for(true), richos_core::setup::MEDIA_TOOLS_WHY);
    let name = Component::MediaTools.display_name_for(true);
    let rendered = format!("{}{}: {}", name[..1].to_uppercase(), &name[1..], Component::MediaTools.why_for(true));
    assert_eq!(rendered, his);
    // The other two steps keep their copy either way.
    for c in [Component::ClaudeCode, Component::Engine] {
        assert_eq!(c.display_name_for(true), c.display_name_for(false));
        assert_eq!(c.why_for(true), c.why_for(false));
    }
}

/// **A FETCHED ENGINE IS FOUND WHERE IT WAS PUT.** The install location and the resolver's
/// candidate 7 are the same path by construction — this is the test that keeps them so.
#[test]
fn an_engine_installed_into_application_support_is_the_one_detection_finds() {
    let root = scratch("candidate-7");
    let home = root.join("home");
    make_engine(&engine_install_dir(&home), "1.0.0");

    let paths = SetupPaths { home: Some(home.clone()), ..Default::default() };
    let found = find_engine(&paths, &[], &every_engine_is_usable);
    assert!(found.present, "{found:?}");
    assert_eq!(found.at.as_deref(), Some(engine_install_dir(&home).display().to_string().as_str()));
    assert_eq!(found.detail.as_deref(), Some("version 1.0.0"));
}

/// A directory called `engine` that is not one is REJECTED, so an installer cannot leave
/// something the resolver would then refuse. The same predicate governs both sides.
#[test]
fn a_directory_that_is_not_an_engine_is_not_detected_as_one() {
    let root = scratch("decoy");
    let home = root.join("home");
    std::fs::create_dir_all(engine_install_dir(&home).join("scripts")).unwrap(); // no hooks/, no VERSION
    let paths = SetupPaths { home: Some(home), ..Default::default() };
    assert!(!find_engine(&paths, &[], &every_engine_is_usable).present);
}

/// An explicit override is EXCLUSIVE in both directions, matching `engine.rs`: a wrong one is
/// reported, never routed around to something nobody named.
#[test]
fn an_explicit_override_never_falls_through_to_something_nobody_named() {
    let root = scratch("exclusive");
    let home = root.join("home");
    make_engine(&home.join(".claude/richos-engine"), "1.0.0");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::fs::write(home.join(".local/bin/claude"), b"#!/bin/sh\n").unwrap();

    let paths = SetupPaths {
        home: Some(home),
        engine_override: Some(root.join("nope")),
        claude_bin_override: Some(root.join("also-nope")),
        ..Default::default()
    };
    let status = detect(&paths, &[], &every_engine_is_usable, &a_speech_model);
    assert!(!status.engine.present, "a bad override fell through: {:?}", status.engine);
    assert!(!status.claude.present, "a bad override fell through: {:?}", status.claude);
    assert!(status.engine.looked_in[0].contains("RICHOS_ENGINE_DIR"), "{:?}", status.engine.looked_in);
    assert!(status.claude.looked_in[0].contains("RICHOS_CLAUDE_BIN"), "{:?}", status.claude.looked_in);
}

/// The dogfood layout is not asked to install anything: an engine reachable from the repo is
/// passed in as an extra candidate and answers before the two per-user locations.
#[test]
fn a_repo_engine_passed_in_as_a_candidate_answers_before_the_install_location() {
    let root = scratch("dogfood");
    let home = root.join("home");
    let repo_engine = root.join("richos/engine");
    make_engine(&repo_engine, "1.0.0");
    make_engine(&engine_install_dir(&home), "9.9.9");

    let paths = SetupPaths { home: Some(home), ..Default::default() };
    let found = find_engine(&paths, &[repo_engine.clone()], &every_engine_is_usable);
    assert_eq!(found.at.as_deref(), Some(repo_engine.display().to_string().as_str()), "{found:?}");
}

// ===========================================================================================
// D1 — SETUP COMPLETES, AND THE NEXT LAUNCH ASKS AGAIN
//
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D1, reproduced
// on four consecutive launches of the published nightly. The three tests below are the three
// halves of it (the defect had three): the wrong candidate won, the right one was never
// reached, and the failure named nowhere.
// ===========================================================================================

/// **THE NIGHTLY'S D1, AS A VALUE.**
///
/// The audit's machine, exactly: an engine installed as a Claude Code plugin at
/// `~/.claude/richos-engine` — engine-SHAPED, carrying no delivered runtime, and therefore
/// unusable — sitting in front of the engine RichOS itself installed at
/// `~/Library/Application Support/RichOS/engine`.
///
/// Before 2026-09-17 the plugin pointer answered first, the caller then vetoed it, and the
/// app reported the engine "NOT installed" while 38 entries of working engine sat on disk
/// where setup had written them. Setup done once was not done.
///
/// The usable one wins, and the one that was passed over is NAMED WITH ITS REASON rather
/// than silently skipped — an operator reading the boot log has to be able to tell "there is
/// no engine" from "there are two and one of them is broken".
#[test]
fn an_unusable_plugin_engine_no_longer_hides_the_one_setup_installed() {
    let root = scratch("d1-plugin-shadows-install");
    let home = root.join("home");
    let plugin = home.join(".claude/richos-engine");
    let installed = engine_install_dir(&home);
    make_engine(&plugin, "1.2.0");
    make_engine(&installed, "1.2.0");

    let paths = SetupPaths { home: Some(home), ..Default::default() };
    let found = find_engine(&paths, &[], &usable_except(vec![plugin.clone()]));

    assert!(found.present, "the engine setup installed was not found: {found:?}");
    assert_eq!(
        found.at.as_deref(),
        Some(installed.display().to_string().as_str()),
        "detection picked something other than what setup wrote: {found:?}"
    );
    assert_eq!(found.detail.as_deref(), Some("version 1.2.0"), "{found:?}");
}

/// The same machine with nothing usable on it at all: the plugin engine is still the only
/// engine-shaped directory, and the report must say it is THERE AND BROKEN — not absent.
///
/// This is the line the nightly printed as `looked in: ` with nothing after the colon.
#[test]
fn an_engine_that_is_present_but_unusable_is_reported_as_present_and_unusable() {
    let root = scratch("d1-present-but-broken");
    let home = root.join("home");
    let plugin = home.join(".claude/richos-engine");
    make_engine(&plugin, "1.2.0");

    let paths = SetupPaths { home: Some(home.clone()), ..Default::default() };
    let found = find_engine(&paths, &[], &usable_except(vec![plugin.clone()]));

    assert!(!found.present, "an unusable engine was reported as usable: {found:?}");
    let places = found.looked_in.join(" | ");
    assert!(
        places.contains(&plugin.display().to_string()),
        "the engine on disk was not named at all: {places}"
    );
    assert!(
        places.contains("cannot be used"),
        "the reason it was rejected is missing: {places}"
    );
    assert!(
        places.contains("no delivered runtimes"),
        "the underlying reason was swallowed: {places}"
    );
    // The place setup WOULD write to is named too, so "install it" is an offer about a path
    // the operator can see rather than an unexplained button.
    assert!(
        places.contains(&engine_install_dir(&home).display().to_string()),
        "the install destination was never mentioned: {places}"
    );
}

/// **A MISSING COMPONENT NEVER NAMES NOWHERE.** The invariant `main.rs`'s boot line depends
/// on, asserted for both components and for the most impoverished launch there is — no
/// `$HOME`, no executable, no `$PATH`, no candidates of any kind. The old code produced an
/// empty `Vec` here and the app printed a sentence that stopped after "looked in:".
#[test]
fn a_component_reported_missing_always_names_at_least_one_place() {
    let paths = SetupPaths::default();
    let status = detect(&paths, &[], &every_engine_is_usable, &a_speech_model);

    for component in [&status.claude, &status.engine] {
        assert!(!component.present, "{component:?}");
        assert!(
            !component.looked_in.is_empty(),
            "a missing component named nowhere — this is the nightly's D1 boot line: {component:?}"
        );
        assert!(
            !component.looked_in.join("").trim().is_empty(),
            "a missing component named only blanks: {component:?}"
        );
    }
}

/// An explicit override that points at an engine which cannot be used stays EXCLUSIVE — it
/// does not fall through to one nobody named (`locate-engine.sh` rule 1) — but the refusal
/// now carries the reason instead of an empty list.
#[test]
fn an_explicit_override_that_cannot_be_used_says_why_and_still_does_not_fall_through() {
    let root = scratch("d1-explicit-unusable");
    let home = root.join("home");
    let named = root.join("named-engine");
    make_engine(&named, "1.2.0");
    make_engine(&engine_install_dir(&home), "1.2.0");

    let paths = SetupPaths {
        home: Some(home.clone()),
        engine_override: Some(named.clone()),
        ..Default::default()
    };
    let found = find_engine(&paths, &[], &usable_except(vec![named.clone()]));

    assert!(!found.present, "a bad override fell through: {found:?}");
    assert_eq!(found.looked_in.len(), 1, "the override was not exclusive: {found:?}");
    let line = &found.looked_in[0];
    assert!(line.contains("RICHOS_ENGINE_DIR"), "{line}");
    assert!(line.contains("cannot be used"), "{line}");
    assert!(line.contains("no delivered runtimes"), "{line}");
}

// ===========================================================================================
// THE RELEASE GATE — the app boots the engine its build pins, or refuses and says which
//
// Spec point 22 (`richos-hq docs/plans/nightly-channel-spec-2026-09-17.md`), from the CEO's
// question on 2026-09-17: *"shouldn't all the data be separate from the app so that a rollback
// or a manual re-install of the stable app version automatically always brings everything back
// to the working state no matter what?"* It cannot, if the rolled-back app boots the newer
// engine the later app installed — and the engine is what writes into the corpus.
//
// Every refusal below is paired with the POSITIVE CONTROL that makes it mean something: the
// same fixture, the pin it actually carries, accepted.
// ===========================================================================================

/// The rollback, exactly: an engine installed at the place RichOS writes, carrying the release
/// a NEWER app pinned, while this build pins the older one.
#[test]
fn an_engine_of_another_release_is_refused_and_both_versions_are_named() {
    let root = scratch("release-rollback");
    let home = root.join("home");
    let installed = engine_install_dir(&home);
    make_engine(&installed, "1.3.0");

    let paths = SetupPaths { home: Some(home), ..Default::default() };
    let found = find_engine_pinned(&paths, &[], &every_engine_is_usable, Some("1.2.0"));

    assert!(!found.present, "an engine from another release was booted: {found:?}");
    let places = found.looked_in.join(" | ");
    assert!(places.contains(&installed.display().to_string()), "the engine on disk was not named: {places}");
    assert!(places.contains("1.3.0"), "the release that is THERE is missing: {places}");
    assert!(places.contains("1.2.0"), "the release this build needs is missing: {places}");
    // The line about THIS directory says what is wrong with it. "Nothing that looks like the
    // engine is there" is a true sentence about the empty candidates and a false one about a
    // directory full of engine — the D1 sentence, which must not come back wearing a release.
    let line = found
        .looked_in
        .iter()
        .find(|l| l.starts_with(&installed.display().to_string()))
        .unwrap_or_else(|| panic!("the installed engine got no line of its own: {places}"));
    assert!(!line.contains("nothing that looks like the engine is there"), "{line}");

    // THE POSITIVE CONTROL. Same fixture, same walk, the release it carries: accepted.
    let matching = find_engine_pinned(&paths, &[], &every_engine_is_usable, Some("1.3.0"));
    assert!(matching.present, "the pinned release was refused: {matching:?}");
    assert_eq!(matching.at.as_deref(), Some(installed.display().to_string().as_str()));
    assert_eq!(matching.detail.as_deref(), Some("version 1.3.0"));
}

/// **An operator's statement outranks the pin.** `$RICHOS_ENGINE_DIR` is exclusive and verbatim
/// everywhere else (`locate-engine.sh` rule 1), and it is the escape hatch that makes the gate
/// safe on a developer's machine — so it is not release-checked here either.
#[test]
fn an_explicit_override_is_taken_whatever_release_it_carries() {
    let root = scratch("release-explicit");
    let home = root.join("home");
    let named = root.join("named-engine");
    make_engine(&named, "0.0.0-someone-elses");
    make_engine(&engine_install_dir(&home), "1.2.0");

    let paths = SetupPaths {
        home: Some(home),
        engine_override: Some(named.clone()),
        ..Default::default()
    };
    let found = find_engine_pinned(&paths, &[], &every_engine_is_usable, Some("1.2.0"));

    assert!(found.present, "a named engine was refused for its release: {found:?}");
    assert_eq!(found.at.as_deref(), Some(named.display().to_string().as_str()));
    // EXCLUSIVE: the walk never reached the pinned engine sitting in the install directory.
    // (`ComponentStatus::found` clears `looked_in`, so the assertion is on `at`, and the
    // install directory is the thing that would have answered had it fallen through.)
    assert_ne!(
        found.at.as_deref(),
        Some(engine_install_dir(root.join("home").as_path()).display().to_string().as_str()),
        "the override fell through to an engine nobody named: {found:?}"
    );
}

/// **THE DEVELOPER'S MACHINE, decided rather than left to be discovered.** Every `cargo run` and
/// every `cargo test` in this repository is an UNPINNED build: `engine_pin` reads `option_env!`,
/// a compile-time read, and nothing in a plain cargo invocation sets the three variables. A
/// build that names no engine demands none — so the working tree's engine is accepted whatever
/// its `VERSION` says, and the status still reports that this build cannot install one.
#[test]
fn a_build_with_no_pin_accepts_any_release_and_says_it_can_install_none() {
    let root = scratch("release-unpinned");
    let home = root.join("home");
    let working_tree = root.join("richos/engine");
    make_engine(&working_tree, "1.4.0-ahead-of-every-release");

    assert_eq!(required_engine_version(), None, "the test build must carry no pin");
    assert_eq!(engine_accepted(&working_tree, None), Ok(()));

    let paths = SetupPaths { home: Some(home), ..Default::default() };
    let status = detect_with_pin(&paths, &[working_tree.clone()], &every_engine_is_usable, None, &a_speech_model);
    assert!(status.engine.present, "an unpinned build refused an engine: {status:?}");
    assert_eq!(status.engine.at.as_deref(), Some(working_tree.display().to_string().as_str()));
    assert!(!status.engine_installable, "a build with no pin cannot install one");
    assert_eq!(status.engine_pin_version, None);

    // THE POSITIVE CONTROL for the same fixture: give the SAME build a pin and the same
    // working tree is refused by name. This is what a developer sees after sourcing
    // `engine-pin.env` from `make-engine-asset.sh` while the checkout has moved on.
    let pinned = pin_from_parts("1.2.0", "https://example.invalid/engine.tar.gz", A_DIGEST).unwrap();
    let pinned_status =
        detect_with_pin(&paths, &[working_tree.clone()], &every_engine_is_usable, Some(&pinned), &a_speech_model);
    assert!(!pinned_status.engine.present, "{pinned_status:?}");
    assert_eq!(pinned_status.engine_pin_version.as_deref(), Some("1.2.0"));
    let places = pinned_status.engine.looked_in.join(" | ");
    assert!(places.contains("1.4.0-ahead-of-every-release"), "{places}");
    assert!(places.contains("1.2.0"), "{places}");
}

/// The four answers of the gate itself, including the one that is deliberately silent.
#[test]
fn the_release_gate_answers_shape_and_release_separately() {
    let root = scratch("release-gate");
    let right = root.join("right");
    let wrong = root.join("wrong");
    let blank = root.join("blank");
    let empty = root.join("not-an-engine");
    make_engine(&right, "1.2.0");
    make_engine(&wrong, "1.1.0");
    std::fs::create_dir_all(blank.join("scripts/hooks")).unwrap();
    std::fs::write(blank.join("VERSION"), "   \n").unwrap();
    std::fs::create_dir_all(&empty).unwrap();

    assert_eq!(engine_accepted(&right, Some("1.2.0")), Ok(()));
    assert_eq!(
        engine_accepted(&wrong, Some("1.2.0")),
        Err(EngineRejected::WrongRelease { found: Some("1.1.0".into()), needed: "1.2.0".into() })
    );
    // A `VERSION` that is there and says nothing is not a match, and does not pretend to name
    // a version it could not read.
    assert_eq!(
        engine_accepted(&blank, Some("1.2.0")),
        Err(EngineRejected::WrongRelease { found: None, needed: "1.2.0".into() })
    );
    assert_eq!(engine_accepted(&empty, Some("1.2.0")), Err(EngineRejected::NotEngineShaped));
    assert_eq!(engine_accepted(&empty, None), Err(EngineRejected::NotEngineShaped));

    let wrong_reason = EngineRejected::WrongRelease {
        found: Some("1.1.0".into()),
        needed: "1.2.0".into(),
    }
    .reason();
    assert!(wrong_reason.contains("1.1.0") && wrong_reason.contains("1.2.0"), "{wrong_reason}");
    let blank_reason =
        EngineRejected::WrongRelease { found: None, needed: "1.2.0".into() }.reason();
    assert!(blank_reason.contains("no readable version"), "{blank_reason}");
}

/// **THE LAST GATE — what stops the wrong engine WRITING.** A refused resolution does not end
/// the launch: `main.rs::resolve_engine` hands the lease factory the last place it looked, and
/// on macOS that is the application-support directory a newer app installed into. So the lease
/// asks once more, at the door.
#[test]
fn the_lease_refuses_a_directory_this_build_does_not_boot_unless_an_operator_named_it() {
    let root = scratch("release-lease");
    let newer = root.join("newer");
    let right = root.join("right");
    let empty = root.join("nothing-here");
    make_engine(&newer, "1.3.0");
    make_engine(&right, "1.2.0");
    std::fs::create_dir_all(&empty).unwrap();

    let refusal = engine_boot_refusal(&newer, false, Some("1.2.0"))
        .expect("a lease started in an engine from another release");
    assert!(refusal.contains(&newer.display().to_string()), "{refusal}");
    assert!(refusal.contains("1.3.0") && refusal.contains("1.2.0"), "{refusal}");

    // POSITIVE CONTROLS, three of them — the states that must NOT be refused:
    assert_eq!(engine_boot_refusal(&right, false, Some("1.2.0")), None, "the pinned engine");
    assert_eq!(engine_boot_refusal(&newer, true, Some("1.2.0")), None, "an operator named it");
    assert_eq!(engine_boot_refusal(&newer, false, None), None, "this build pins nothing");
    // Not an engine at all is `verify_engine`'s sentence and preflight's, not this gate's.
    assert_eq!(engine_boot_refusal(&empty, false, Some("1.2.0")), None, "not this gate's question");
}

// ===========================================================================================
// THE IDENTITY GATE — same version string, different engine (2026-09-18)
// ===========================================================================================
//
// The two digests below are the REAL ones from candidate .8's walk, not invented values, so a
// reader can tie every assertion here back to the failure it came from:
//
//   - `STALE_DIGEST` is the `sha256` line of the `INSTALLED-FROM` in the engine directory the
//     app actually used, written when `v1.2.0-nightly.20260917.2` was installed;
//   - `PINNED_DIGEST` is the digest compiled into candidate .8's own binary, recovered with
//     `strings Contents/MacOS/richos-tauri` and appearing there exactly once — while
//     `STALE_DIGEST` appears in it zero times.
//
// Both directories carry `VERSION` = `1.2.0`, which is why the release gate above passed them.

/// `v1.2.0-nightly.20260917.2` — what was installed.
const STALE_DIGEST: &str = "b7a882ef4381ca294259c1a01a6af29acc3159dc0b871e8064bd7f410b71da07";
/// `v1.2.0-nightly.20260918.2` — what candidate .8 pinned.
const PINNED_DIGEST: &str = "ea7f79043e7dc8f51b5f4207964ec5fa3e15ca11b44e5342a5fb4d38580db194";

/// Write the stamp `install_engine` writes, so the reader under test is fed the writer's own
/// format rather than a test's idea of it.
fn stamp_engine(at: &Path, version: &str, sha256: &str, url: &str) {
    std::fs::write(
        at.join("INSTALLED-FROM"),
        format!("engine {version}\nsha256 {sha256}\nbytes 119463136\nfrom {url}\n"),
    )
    .unwrap();
}

fn a_nightly_pin(sha256: &str) -> EnginePin {
    pin_from_parts("1.2.0", "https://example.invalid/richos-engine-1.2.0.tar.gz", sha256).unwrap()
}

/// **THE DEFECT, AS A UNIT TEST.** Two engine directories, both `VERSION` = `1.2.0`, one
/// installed from yesterday's asset. The release gate cannot tell them apart; the identity gate
/// must.
#[test]
fn an_engine_with_the_right_version_and_yesterdays_contents_is_refused() {
    let root = scratch("identity-stale");
    let stale = root.join("stale");
    make_engine(&stale, "1.2.0");
    stamp_engine(&stale, "1.2.0", STALE_DIGEST, "https://example.invalid/20260917.2.tar.gz");

    let pin = a_nightly_pin(PINNED_DIGEST);

    // The version-only gate passes it — this is the state that shipped, asserted rather than
    // described, so a future change that "fixes" it by tightening the version gate is caught.
    assert_eq!(engine_accepted(&stale, Some("1.2.0")), Ok(()));

    // The identity gate refuses it.
    assert_eq!(
        engine_accepted_demand(&stale, EngineDemand::pinned(Some(&pin))),
        Err(EngineRejected::WrongIdentity {
            installed: Some(STALE_DIGEST.to_string()),
            pinned: PINNED_DIGEST.to_string(),
        })
    );

    // AND THE SENTENCE SAYS THE LABEL IS RIGHT AND THE CONTENTS ARE NOT. An operator who reads
    // only "wrong engine" about a directory whose VERSION matches hunts for a version fault
    // that is not there — which is the whole reason this rejection has its own variant.
    let reason = EngineRejected::WrongIdentity {
        installed: Some(STALE_DIGEST.to_string()),
        pinned: PINNED_DIGEST.to_string(),
    }
    .reason();
    assert!(reason.contains("right version"), "{reason}");
    assert!(reason.contains("b7a882ef4381"), "{reason}");
    assert!(reason.contains("ea7f79043e7d"), "{reason}");
    // The display is twelve characters; the COMPARISON is never truncated.
    assert!(!reason.contains(STALE_DIGEST), "the full digest is not operator copy: {reason}");
}

/// Matching contents are KEPT — the positive control, without which the test above would pass
/// for a gate that refused everything.
#[test]
fn an_engine_installed_from_the_pinned_asset_is_kept() {
    let root = scratch("identity-match");
    let current = root.join("current");
    make_engine(&current, "1.2.0");
    stamp_engine(&current, "1.2.0", PINNED_DIGEST, "https://example.invalid/20260918.2.tar.gz");
    let pin = a_nightly_pin(PINNED_DIGEST);

    assert_eq!(engine_accepted_demand(&current, EngineDemand::pinned(Some(&pin))), Ok(()));

    // A digest recorded in upper case is the same digest. Nothing writes one today; a gate that
    // depended on that is a gate with a silent failure waiting in it.
    let shouty = root.join("shouty");
    make_engine(&shouty, "1.2.0");
    stamp_engine(&shouty, "1.2.0", &PINNED_DIGEST.to_ascii_uppercase(), "https://example.invalid/x");
    assert_eq!(engine_accepted_demand(&shouty, EngineDemand::pinned(Some(&pin))), Ok(()));
}

/// **NO MANIFEST MEANS UNKNOWN, AND THE PIN IS WHAT DECIDES WHETHER UNKNOWN IS ACCEPTABLE.**
///
/// A build that names an asset boots the directory installed from that asset. A build that names
/// none demands nothing — which is every `cargo run` and every `cargo test` here, and is why the
/// dogfood checkout is untouched by all of this.
#[test]
fn a_missing_manifest_is_unknown_and_a_pinned_build_refuses_unknown() {
    let root = scratch("identity-unstamped");
    let unstamped = root.join("unstamped");
    make_engine(&unstamped, "1.2.0");
    assert!(installed_from(&unstamped).is_none(), "the fixture must carry no stamp");
    let pin = a_nightly_pin(PINNED_DIGEST);

    // A PINNED build: unknown contents, refused. This is the clause that closes the hole pid
    // 66030 fell through — `~/.claude/richos-engine` carries no stamp either.
    assert_eq!(
        engine_accepted_demand(&unstamped, EngineDemand::pinned(Some(&pin))),
        Err(EngineRejected::WrongIdentity { installed: None, pinned: PINNED_DIGEST.to_string() })
    );
    let reason =
        EngineRejected::WrongIdentity { installed: None, pinned: PINNED_DIGEST.to_string() }
            .reason();
    assert!(reason.contains("no record of what installed it"), "{reason}");

    // AN UNPINNED build: nothing demanded, and the same directory resolves exactly as it always
    // has. Without this control the rule above would be indistinguishable from one that breaks
    // every developer run in the repository.
    assert_eq!(engine_accepted_demand(&unstamped, EngineDemand::pinned(None)), Ok(()));
    assert_eq!(engine_accepted_demand(&unstamped, EngineDemand::default()), Ok(()));
    assert_eq!(required_engine_version(), None, "this test binary must carry no pin");
}

/// The stamp is parsed BY KEY, and a partial one still yields the field that decides.
#[test]
fn the_stamp_is_read_by_key_and_a_truncated_one_still_answers() {
    let root = scratch("identity-parse");
    let full = root.join("full");
    make_engine(&full, "1.2.0");
    stamp_engine(&full, "1.2.0", PINNED_DIGEST, "https://example.invalid/asset.tar.gz");
    let parsed = installed_from(&full).expect("a stamp that is there");
    assert_eq!(parsed.version.as_deref(), Some("1.2.0"));
    assert_eq!(parsed.sha256.as_deref(), Some(PINNED_DIGEST));
    assert_eq!(parsed.bytes, Some(119_463_136));
    assert_eq!(parsed.url.as_deref(), Some("https://example.invalid/asset.tar.gz"));

    // Fields in another order, an unknown field added, and the `bytes` line lost to a full disk.
    // The digest is still found, because nothing here depends on line position.
    let odd = root.join("odd");
    make_engine(&odd, "1.2.0");
    std::fs::write(
        odd.join("INSTALLED-FROM"),
        format!("from https://example.invalid/a\nsomething-new yes\nsha256 {PINNED_DIGEST}\nengine 1.2.0\n"),
    )
    .unwrap();
    let odd_parsed = installed_from(&odd).unwrap();
    assert_eq!(odd_parsed.sha256.as_deref(), Some(PINNED_DIGEST));
    assert_eq!(odd_parsed.bytes, None);
    assert_eq!(engine_accepted_demand(&odd, EngineDemand::pinned(Some(&a_nightly_pin(PINNED_DIGEST)))), Ok(()));

    // A stamp with no readable digest is a stamp that answers nothing, and must be treated as
    // unknown rather than as a match.
    let gutted = root.join("gutted");
    make_engine(&gutted, "1.2.0");
    std::fs::write(gutted.join("INSTALLED-FROM"), "engine 1.2.0\nsha256 \n").unwrap();
    assert_eq!(installed_from(&gutted).unwrap().sha256, None);
    assert_eq!(
        engine_accepted_demand(&gutted, EngineDemand::pinned(Some(&a_nightly_pin(PINNED_DIGEST)))),
        Err(EngineRejected::WrongIdentity { installed: None, pinned: PINNED_DIGEST.to_string() })
    );
}

/// **THE LAST GATE, ASKED ABOUT CONTENTS.** Resolution refusing a stale directory is not the
/// same as nothing running in it — `main.rs::resolve_engine` hands the lease factory the last
/// place it looked. This is the line that would have stopped candidate .8 booting yesterday's
/// engine, as opposed to merely mentioning it.
#[test]
fn the_lease_refuses_an_engine_whose_contents_are_not_the_pinned_ones() {
    let root = scratch("identity-lease");
    let stale = root.join("stale");
    let current = root.join("current");
    make_engine(&stale, "1.2.0");
    make_engine(&current, "1.2.0");
    stamp_engine(&stale, "1.2.0", STALE_DIGEST, "https://example.invalid/20260917.2.tar.gz");
    stamp_engine(&current, "1.2.0", PINNED_DIGEST, "https://example.invalid/20260918.2.tar.gz");
    let pin = a_nightly_pin(PINNED_DIGEST);

    let refusal = engine_boot_refusal_demand(&stale, false, EngineDemand::pinned(Some(&pin)))
        .expect("a lease started in yesterday's engine");
    assert!(refusal.contains(&stale.display().to_string()), "{refusal}");
    assert!(refusal.contains("b7a882ef4381") && refusal.contains("ea7f79043e7d"), "{refusal}");

    // POSITIVE CONTROLS — the states that must NOT be refused, each for its own stated reason:
    assert_eq!(
        engine_boot_refusal_demand(&current, false, EngineDemand::pinned(Some(&pin))),
        None,
        "the engine this build pins"
    );
    assert_eq!(
        engine_boot_refusal_demand(&stale, true, EngineDemand::pinned(Some(&pin))),
        None,
        "an operator named it, and a statement outranks the pin"
    );
    assert_eq!(
        engine_boot_refusal_demand(&stale, false, EngineDemand::pinned(None)),
        None,
        "this build pins nothing, so it demands nothing"
    );
    // And the old signature keeps its old, narrower meaning: version only.
    assert_eq!(
        engine_boot_refusal(&stale, false, Some("1.2.0")),
        None,
        "the release-only gate is unchanged, which is why the demand-aware one exists"
    );
}

/// **DETECTION picks the pinned engine over a stale one and names the reason it passed the stale
/// one over** — the walk, not just the predicate.
#[test]
fn detection_walks_past_a_stale_managed_engine_and_says_why() {
    let root = scratch("identity-detect");
    let home = root.join("home");
    let managed = engine_install_dir(&home);
    make_engine(&managed, "1.2.0");
    stamp_engine(&managed, "1.2.0", STALE_DIGEST, "https://example.invalid/20260917.2.tar.gz");
    let pin = a_nightly_pin(PINNED_DIGEST);

    let paths = SetupPaths { home: Some(home.clone()), ..Default::default() };
    let status = detect_with_pin(&paths, &[], &every_engine_is_usable, Some(&pin), &a_speech_model);
    assert!(!status.engine.present, "a stale-by-content engine was accepted: {status:?}");
    let places = status.engine.looked_in.join(" | ");
    assert!(places.contains(&managed.display().to_string()), "{places}");
    assert!(places.contains("right version"), "{places}");
    assert!(places.contains("b7a882ef4381"), "{places}");
    // The offer to fix it is the same one a MISSING engine produces — which is how the refresh
    // reaches the CEO without a second surface being invented for it.
    assert!(status.engine_installable, "nothing offered to replace it: {status:?}");
    assert_eq!(status.engine_pin_version.as_deref(), Some("1.2.0"));

    // POSITIVE CONTROL: the same walk, the same fixture, the pinned contents — found.
    stamp_engine(&managed, "1.2.0", PINNED_DIGEST, "https://example.invalid/20260918.2.tar.gz");
    let good = detect_with_pin(&paths, &[], &every_engine_is_usable, Some(&pin), &a_speech_model);
    assert!(good.engine.present, "{good:?}");
    assert_eq!(good.engine.at.as_deref(), Some(managed.display().to_string().as_str()));
}

/// **A SHIPPED BUILD DOES NOT SILENTLY BOOT THE DEVELOPER'S ENGINE** — Ray's second finding on
/// candidate .8, as a test.
///
/// With the managed directory absent, pid 66030 resolved `/Users/alex/.claude/richos-engine`: the
/// engine pointer on the machine that BUILT the candidate, outside the walk's HOME entirely, with
/// a matching `VERSION` and no record of where its contents came from. Signed, notarized, and
/// running a working tree.
///
/// The pointer is candidate 6 and it is probed BEFORE the directory RichOS installs, so leniency
/// here is not a corner case — it is the answer a shipped build reaches first on any machine that
/// has the engine checked out.
#[test]
fn a_pinned_build_refuses_the_unstamped_developer_pointer_and_names_it() {
    let root = scratch("identity-devpointer");
    let home = root.join("home");
    let config = root.join("developer-dot-claude");
    let pointer = config.join("richos-engine");
    make_engine(&pointer, "1.2.0");
    assert!(installed_from(&pointer).is_none(), "the developer's pointer carries no stamp");
    let pin = a_nightly_pin(PINNED_DIGEST);

    let paths = SetupPaths {
        home: Some(home),
        config_dir: Some(config),
        ..Default::default()
    };
    let status = detect_with_pin(&paths, &[], &every_engine_is_usable, Some(&pin), &a_speech_model);
    assert!(!status.engine.present, "a shipped build took the developer's engine: {status:?}");
    let places = status.engine.looked_in.join(" | ");
    assert!(places.contains(&pointer.display().to_string()), "{places}");
    assert!(places.contains("no record of what installed it"), "{places}");
    // It is refused, and it is refused LOUDLY: the place and the reason are both in the report
    // the boot line prints, rather than a silent fall-through to the next candidate.
    assert!(places.contains("ea7f79043e7d"), "{places}");

    // POSITIVE CONTROL — THE DOGFOOD GUARANTEE. The same pointer, the same walk, a build with no
    // pin: found. Every `cargo run` in this repository is this case, and if this assertion ever
    // fails the fix above has broken development rather than secured it.
    let unpinned = detect_with_pin(&paths, &[], &every_engine_is_usable, None, &a_speech_model);
    assert!(unpinned.engine.present, "an unpinned build refused a checkout: {unpinned:?}");
    assert_eq!(unpinned.engine.at.as_deref(), Some(pointer.display().to_string().as_str()));

    // AND THE ESCAPE HATCH STILL WORKS. A developer who wants a pinned build to run against a
    // working tree names it, once, deliberately — the same statement the release gate honors.
    let named = SetupPaths {
        engine_override: Some(pointer.clone()),
        ..Default::default()
    };
    let honored = detect_with_pin(&named, &[], &every_engine_is_usable, Some(&pin), &a_speech_model);
    assert!(honored.engine.present, "$RICHOS_ENGINE_DIR was overruled: {honored:?}");
}

/// **THE GATE, RUN OVER A REAL INSTALLED ENGINE ON THIS MACHINE.**
///
/// Everything above builds its own fixtures, which proves the RULE and cannot prove that
/// `installed_from` parses the file `install_engine` actually wrote onto a real Mac. Point this at
/// an engine directory and it reports the verdict against a digest you supply:
///
/// ```bash
/// RICHOS_ENGINE_DIR_FIXTURE="$HOME/Library/Application Support/RichOS/engine" \
/// RICHOS_ENGINE_PIN_FIXTURE=ea7f79043e7dc8f51b5f4207964ec5fa3e15ca11b44e5342a5fb4d38580db194 \
///   cargo test -p richos-core --test setup a_real_installed_engine -- --nocapture
/// ```
///
/// It is env-var-gated and skips with a printed reason when unset, the same shape
/// `richos-voice` uses for the CEO's private echo-path recording: evidence that depends on
/// something not in the repository must be reproducible AND must not fail a clean checkout.
///
/// **It only ever READS.** No install, no swap, no write of any kind — so it is safe to aim at a
/// real installation, which is the only reason aiming it there is worth doing.
#[test]
fn a_real_installed_engine_is_judged_against_a_supplied_pin() {
    let (Some(dir), Some(sha)) = (
        std::env::var_os("RICHOS_ENGINE_DIR_FIXTURE").map(PathBuf::from),
        std::env::var("RICHOS_ENGINE_PIN_FIXTURE").ok(),
    ) else {
        println!(
            "SKIPPED: set RICHOS_ENGINE_DIR_FIXTURE and RICHOS_ENGINE_PIN_FIXTURE to judge a real \
             engine directory on this machine. Nothing is assumed in their absence."
        );
        return;
    };

    println!("directory : {}", dir.display());
    println!("shape     : {}", if engine_looks_valid(&dir) { "engine" } else { "NOT an engine" });
    println!("VERSION   : {:?}", engine_version(&dir));
    match installed_from(&dir) {
        None => println!("INSTALLED-FROM: absent — contents unknown"),
        Some(s) => {
            println!("INSTALLED-FROM: {:?}", s.sha256);
            println!("  bytes  : {:?}", s.bytes);
            println!("  from   : {:?}", s.url);
        }
    }

    let version = engine_version(&dir).unwrap_or_default();
    let pin = pin_from_parts(&version, "https://example.invalid/asset.tar.gz", &sha)
        .expect("RICHOS_ENGINE_PIN_FIXTURE must be 64 lowercase hex characters");

    // THE OLD RULE — version equality alone. Printed, not asserted, because what it answers here
    // depends on the machine this runs on; it is the "before" half of the comparison.
    println!(
        "version-only gate : {:?}",
        engine_accepted(&dir, Some(version.as_str())).map_err(|e| e.reason())
    );
    // THE RULE AS IT SHIPS NOW.
    println!(
        "identity gate     : {:?}",
        engine_accepted_demand(&dir, EngineDemand::pinned(Some(&pin))).map_err(|e| e.reason())
    );

    // The one thing that IS asserted, because it is true of any real installation whatever digest
    // it carries: a directory whose recorded digest equals the pin is accepted, and one whose
    // recorded digest differs is refused. No claim is made here about which of those this machine
    // happens to be in.
    let recorded = installed_from(&dir).and_then(|s| s.sha256);
    let verdict = engine_accepted_demand(&dir, EngineDemand::pinned(Some(&pin)));
    match recorded {
        Some(r) if r.eq_ignore_ascii_case(&sha) => {
            assert_eq!(verdict, Ok(()), "a matching digest was refused")
        }
        _ if engine_looks_valid(&dir) => {
            assert!(verdict.is_err(), "a non-matching or absent digest was accepted")
        }
        _ => println!("(not engine-shaped, so the identity half is not this gate's question)"),
    }
}

/// **THE CEO'S OWN DATA IS NOT THE ENGINE, AND NOTHING HERE TOUCHES IT.** `install_engine`
/// replaces `Application Support/RichOS/engine`; his conversation, ledger and corpus live in
/// `Application Support/com.richos.app`, a sibling. This is the positive control the brief asks
/// for: a full refresh runs, and every byte of his directory is still there afterwards.
#[test]
fn a_refresh_replaces_the_engine_and_leaves_the_ceos_data_untouched() {
    let root = scratch("identity-refresh");
    let home = root.join("home");
    let managed = engine_install_dir(&home);

    // His data, beside the engine, with contents worth checking byte for byte.
    let his = app_support_richos(&home).with_file_name("com.richos.app");
    std::fs::create_dir_all(his.join("engine-state")).unwrap();
    std::fs::write(his.join("ledger.jsonl"), "his turns, and nothing may rewrite them\n").unwrap();
    std::fs::write(his.join("engine-state/assignments.jsonl"), "his background work\n").unwrap();
    let before: Vec<(PathBuf, Vec<u8>)> = walk_files(&his);
    assert_eq!(before.len(), 2, "the fixture must have two files: {before:?}");

    // Yesterday's engine, installed and stamped as yesterday's.
    //
    // The RELEASE here is `1.0.0` rather than `1.2.0` for one mechanical reason worth naming:
    // `install_engine` runs the full `runtime::verify_engine` inventory when the pinned version
    // is exactly `1.2.0`, and that wants a real 322 MB delivered runtime no unit test can build.
    // The identity rule under test has nothing to do with which release string is used.
    make_engine(&managed, "1.0.0");
    std::fs::write(managed.join("marker"), "from the 17th").unwrap();
    stamp_engine(&managed, "1.0.0", STALE_DIGEST, "https://example.invalid/20260917.2.tar.gz");
    let stale_pin = pin_from_parts("1.0.0", ASSET_URL, STALE_DIGEST).unwrap();
    assert_eq!(engine_accepted(&managed, Some("1.0.0")), Ok(()), "the version gate passes it");

    // Today's asset, fetched and installed the way the product installs it.
    let body = b"today's asset, and its digest is what identifies it";
    let digest = body_and_digest(&root, body);
    assert_ne!(digest, STALE_DIGEST, "the fixture must actually be a different asset");
    let pin = a_pin(&digest);
    let report =
        install_engine(&FakeFetcher::new(body), &good_engine_plan("1.0.0"), &pin, &managed)
            .expect("the refresh");

    // The engine is today's, and it SAYS it is today's — the stamp the next boot reads. This is
    // the round trip that matters: the digest the installer WRITES is the digest the gate READS,
    // so the writer and the reader cannot drift into disagreeing.
    assert_eq!(installed_from(&managed).and_then(|s| s.sha256).as_deref(), Some(pin.sha256.as_str()));
    assert_eq!(engine_accepted_demand(&managed, EngineDemand::pinned(Some(&pin))), Ok(()));
    assert!(!managed.join("marker").is_file(), "yesterday's engine survived the refresh");
    assert!(report.previous_at.is_some(), "the predecessor was not preserved");
    // And the refreshed directory is no longer the one the STALE pin describes — the refusal that
    // sent it here would not fire twice.
    assert_eq!(
        engine_accepted_demand(&managed, EngineDemand::pinned(Some(&stale_pin))),
        Err(EngineRejected::WrongIdentity {
            installed: Some(digest.clone()),
            pinned: STALE_DIGEST.to_string(),
        })
    );

    // AND HIS DATA IS BYTE-FOR-BYTE WHAT IT WAS.
    assert_eq!(walk_files(&his), before, "the refresh touched the CEO's own data");
}

// ===========================================================================================
// THE PIN
// ===========================================================================================

/// **UNPINNED IS A REFUSAL, NEVER A FALLBACK.** Five bad pins, five `None`s, and the plain-http
/// one is the reason the rule exists.
#[test]
fn a_pin_that_is_not_exactly_right_is_no_pin_at_all() {
    assert!(pin_from_parts("", "https://x/y", A_DIGEST).is_none(), "blank version");
    assert!(pin_from_parts("1.0.0", "", A_DIGEST).is_none(), "blank url");
    assert!(
        pin_from_parts("1.0.0", "http://github.com/x/y.tar.gz", A_DIGEST).is_none(),
        "PLAIN HTTP MUST BE REFUSED"
    );
    assert!(pin_from_parts("1.0.0", "https://x/y", "deadbeef").is_none(), "short digest");
    assert!(
        pin_from_parts("1.0.0", "https://x/y", &A_DIGEST.replace('0', "G")).is_none(),
        "non-hex digest"
    );
    assert!(
        pin_from_parts("1.0.0", "https://x/y", "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789")
            .is_none(),
        "uppercase hex is not the form the comparison uses, so it is not a pin"
    );
    assert!(pin_from_parts("1.0.0", "https://x/y.tar.gz", A_DIGEST).is_some());
}

/// An unpinned build says so in its status AND refuses at the door — two places, because a
/// surface that offers a button the backend will refuse is the failure this prevents.
#[test]
fn an_unpinned_build_reports_it_and_is_blocked_rather_than_guessing() {
    let root = scratch("unpinned");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let status = detect(&SetupPaths { home: Some(home), ..Default::default() }, &[], &every_engine_is_usable, &a_speech_model);
    // The test build carries no `RICHOS_ENGINE_*` at compile time.
    assert!(!status.engine_installable, "the test build must carry no pin");
    assert!(status.blocked(), "an engine that is missing and unpinnable is blocked");
    assert_eq!(status.engine_pin_version, None);
}

// ===========================================================================================
// THE ENGINE INSTALL — the happy path
// ===========================================================================================

/// **THE DELIVERABLE, IN ONE TEST**: nothing on disk, a pinned asset, and afterwards an engine
/// at the path the resolver searches, carrying its own freshness stamp.
#[test]
fn a_pinned_engine_lands_where_the_resolver_looks_and_stamps_itself() {
    let root = scratch("engine-happy");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    let body = b"not really a tarball, but the digest is what is checked";
    let digest = body_and_digest(&root, body);

    let report =
        install_engine(&FakeFetcher::new(body), &good_engine_plan("1.0.0"), &a_pin(&digest), &dest)
            .unwrap();

    assert!(engine_looks_valid(&dest), "the installed directory must satisfy the resolver");
    assert_eq!(engine_version(&dest).as_deref(), Some("1.0.0"));
    assert_eq!(report.version, "1.0.0");
    assert_eq!(report.sha256, digest);
    assert_eq!(report.bytes, body.len() as u64);

    // THE FRESHNESS STAMP, inside the artifact.
    let stamp = std::fs::read_to_string(dest.join("INSTALLED-FROM")).unwrap();
    assert!(stamp.contains(&digest), "{stamp}");
    assert!(stamp.contains("engine 1.0.0"), "{stamp}");
    // The pin names the old GitHub address; where the new one answers, it is fetched from there.
    assert!(stamp.contains("from https://github.com/RichSpark/RichOS/releases/"), "{stamp}");

    // And detection now finds it, which is the whole point.
    let paths = SetupPaths { home: Some(home), ..Default::default() };
    assert!(find_engine(&paths, &[], &every_engine_is_usable).present);
}

/// THE GITHUB MOVE (CEO, 2026-10-27): a build pinned to the old address installs its engine
/// before the move (the new address has nothing yet) and after it (fetched from the new address,
/// never through GitHub's redirect). The digest is checked whichever address the bytes came from.
#[test]
fn a_pinned_engine_is_fetched_from_the_new_address_first_and_the_old_one_when_it_has_nothing() {
    struct BeforeTheMove {
        body: Vec<u8>,
        asked: Mutex<Vec<String>>,
    }
    impl Fetcher for BeforeTheMove {
        fn fetch(&self, url: &str, dest: &Path) -> Result<u64, SetupError> {
            self.asked.lock().unwrap().push(url.to_string());
            if url.starts_with("https://github.com/RichSpark/RichOS/") {
                return Err(SetupError::DownloadFailed { url: url.to_string(), status: "The requested URL returned error: 404".into() });
            }
            std::fs::write(dest, &self.body).unwrap();
            Ok(self.body.len() as u64)
        }
    }
    let root = scratch("engine-before-the-move");
    let dest = engine_install_dir(&root.join("home"));
    let body = b"the same bytes at either address";
    let digest = body_and_digest(&root, body);
    let before = BeforeTheMove { body: body.to_vec(), asked: Mutex::new(Vec::new()) };
    install_engine(&before, &good_engine_plan("1.0.0"), &a_pin(&digest), &dest).unwrap();
    let home = ASSET_URL.replace("WebDevBooster/richos", "RichSpark/RichOS");
    assert_eq!(before.asked.lock().unwrap().as_slice(), [home.as_str(), ASSET_URL], "the new address first, then the pinned one");
    assert!(std::fs::read_to_string(dest.join("INSTALLED-FROM")).unwrap().contains(&format!("from {ASSET_URL}")));

    let root = scratch("engine-after-the-move");
    let dest = engine_install_dir(&root.join("home"));
    let digest = body_and_digest(&root, body);
    let after = FakeFetcher::new(body);
    install_engine(&after, &good_engine_plan("1.0.0"), &a_pin(&digest), &dest).unwrap();
    assert_eq!(after.asked.lock().unwrap().as_slice(), [home.as_str()], "the old address is not asked once the new one answers");
    assert!(std::fs::read_to_string(dest.join("INSTALLED-FROM")).unwrap().contains(&format!("from {home}")));
}

/// **SETUP DONE ONCE IS DONE — the relaunch, end to end.**
///
/// The nightly's D1 in the form the CEO met it: run setup to completion, quit, open RichOS
/// again, and be asked to set it up again — on four consecutive launches, with 38 entries of
/// working engine on disk the whole time.
///
/// This drives the REAL installer (`install_engine`, through its fetch/extract seams) onto a
/// scratch HOME that also carries the thing that broke it: an engine-shaped
/// `~/.claude/richos-engine` with no delivered runtime, which is what any machine with the
/// engine installed as a Claude Code plugin has. Then it asks the question the next launch
/// asks — `detect`, exactly as `setup_view::detect` calls it — and requires the answer to be
/// "nothing missing".
///
/// The two assertions that matter are `needs()` being empty and `complete()` being true:
/// those are the only inputs to whether the window puts the sheet up
/// (`main.rs`'s boot block and the `setup_status` command both branch on nothing else).
#[test]
fn setup_completes_and_the_next_launch_does_not_ask_again() {
    let root = scratch("d1-relaunch-after-setup");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    let body = b"the pinned asset";
    let digest = body_and_digest(&root, body);

    // The machine the audit ran on: an engine-shaped plugin pointer that cannot be used.
    let plugin = home.join(".claude/richos-engine");
    make_engine(&plugin, "1.2.0");
    // ...and Claude Code and the video tools present, so the engine is the only thing that
    // could be missing.
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    std::fs::write(home.join(".local/bin/claude"), b"#!/bin/sh\n").unwrap();
    install_yt_dlp(&home);

    // ---- THE FIRST LAUNCH: setup is genuinely needed, and says so naming both places.
    let paths = SetupPaths { home: Some(home.clone()), ..Default::default() };
    let usable = usable_except(vec![plugin.clone()]);
    let before = detect(&paths, &[], &usable, &a_speech_model);
    assert_eq!(before.needs(), vec![Component::Engine], "{before:?}");
    let places = before.engine.looked_in.join(" | ");
    assert!(places.contains(&plugin.display().to_string()), "{places}");
    assert!(places.contains(&dest.display().to_string()), "{places}");

    // ---- SETUP RUNS, for real.
    install_engine(&FakeFetcher::new(body), &good_engine_plan("1.0.0"), &a_pin(&digest), &dest)
        .unwrap();

    // ---- THE NEXT LAUNCH: the same question, asked the same way, on a fresh read of disk.
    let relaunch = detect(&SetupPaths { home: Some(home), ..Default::default() }, &[], &usable, &a_speech_model);
    assert!(
        relaunch.engine.present,
        "the engine setup had just written was not found on the next launch: {:?}",
        relaunch.engine
    );
    assert_eq!(relaunch.engine.at.as_deref(), Some(dest.display().to_string().as_str()));
    assert_eq!(relaunch.needs(), Vec::<Component>::new(), "the sheet would come back up: {relaunch:?}");
    assert!(relaunch.complete(), "{relaunch:?}");
}

/// NO RESIDUE. After a success the only thing beside the engine is the engine — no
/// `.incoming`, no `.previous`. His Application Support directory is not a scratch space.
#[test]
fn a_successful_install_leaves_no_staging_directory_behind() {
    let root = scratch("no-residue");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    let body = b"bytes";
    let digest = body_and_digest(&root, body);

    install_engine(&FakeFetcher::new(body), &good_engine_plan("1.0.0"), &a_pin(&digest), &dest)
        .unwrap();

    assert_eq!(siblings_of(dest.parent().unwrap()), vec!["engine".to_string()]);
}

// ===========================================================================================
// THE ENGINE INSTALL — every failure path
// ===========================================================================================

/// **A TAMPERED ARCHIVE IS REFUSED BEFORE `tar` SEES IT**, and nothing is installed. The
/// extractor here would happily produce a perfect engine; it never gets the chance.
#[test]
fn a_digest_mismatch_stops_before_extraction_and_installs_nothing() {
    let root = scratch("digest-mismatch");
    let home = root.join("home");
    let dest = engine_install_dir(&home);

    struct MustNotRun;
    impl Extractor for MustNotRun {
        fn extract(&self, _a: &Path, _i: &Path) -> Result<(), SetupError> {
            panic!("EXTRACTION RAN ON BYTES THAT FAILED THE DIGEST CHECK");
        }
    }

    let err =
        install_engine(&FakeFetcher::new(b"tampered"), &MustNotRun, &a_pin(A_DIGEST), &dest)
            .unwrap_err();

    assert_eq!(err.kind(), "digest-mismatch");
    assert!(err.machine_unchanged());
    assert!(!dest.exists(), "something was installed after a digest mismatch");
    assert!(err.to_string().contains("installed nothing"), "{err}");
}

/// An archive that opens to something that is not an engine is refused, by the SAME predicate
/// the resolver uses — so nothing can be installed that the next boot rejects.
#[test]
fn an_archive_without_the_engines_shape_is_refused_and_names_what_is_missing() {
    let root = scratch("wrong-shape");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    let body = b"x";
    let digest = body_and_digest(&root, body);

    // An `engine/` with a VERSION but no `scripts/hooks`.
    let half = FakeExtractor { plan: vec![("engine/VERSION".into(), false, "1.0.0\n".into())] };
    let err = install_engine(&FakeFetcher::new(body), &half, &a_pin(&digest), &dest).unwrap_err();
    assert_eq!(err.kind(), "engine-shape-invalid");
    assert!(err.to_string().contains("scripts/hooks"), "{err}");
    assert!(!dest.exists());

    // And an archive with no `engine/` at all names what it did find.
    let elsewhere = FakeExtractor { plan: vec![("something-else/x".into(), false, "y".into())] };
    let err =
        install_engine(&FakeFetcher::new(body), &elsewhere, &a_pin(&digest), &dest).unwrap_err();
    assert_eq!(err.kind(), "engine-shape-invalid");
    assert!(err.to_string().contains("something-else"), "{err}");
    assert!(!dest.exists());
}

/// A release asset rebuilt under the same name, carrying a different engine, is caught by the
/// version check even though its digest matched the pin.
#[test]
fn an_engine_whose_version_is_not_the_pinned_one_is_refused() {
    let root = scratch("version-mismatch");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    let body = b"x";
    let digest = body_and_digest(&root, body);

    let err =
        install_engine(&FakeFetcher::new(body), &good_engine_plan("2.0.0"), &a_pin(&digest), &dest)
            .unwrap_err();
    assert_eq!(err.kind(), "engine-version-mismatch");
    assert!(err.to_string().contains("2.0.0") && err.to_string().contains("1.0.0"), "{err}");
    assert!(!dest.exists());
}

/// **NO NETWORK.** Nothing is installed, the sentence says so, and no staging survives.
#[test]
fn no_network_installs_nothing_and_leaves_no_residue() {
    let root = scratch("no-network");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();

    let err = install_engine(
        &FailingFetcher(|url| SetupError::NoNetwork {
            url: url.to_string(),
            detail: "could not resolve host".into(),
        }),
        &good_engine_plan("1.0.0"),
        &a_pin(A_DIGEST),
        &dest,
    )
    .unwrap_err();

    assert_eq!(err.kind(), "no-network");
    assert!(err.machine_unchanged());
    assert!(err.to_string().contains("Nothing has been changed on your Mac"), "{err}");
    assert!(siblings_of(dest.parent().unwrap()).is_empty(), "residue after a failed download");
}

/// A refused download (a 404 on the release asset) is its own sentence.
#[test]
fn a_release_asset_that_is_not_there_is_named_as_one() {
    let root = scratch("asset-404");
    let dest = engine_install_dir(&root.join("home"));
    let err = install_engine(
        &FailingFetcher(|url| SetupError::DownloadFailed {
            url: url.to_string(),
            status: "The requested URL returned error: 404".into(),
        }),
        &good_engine_plan("1.0.0"),
        &a_pin(A_DIGEST),
        &dest,
    )
    .unwrap_err();
    assert_eq!(err.kind(), "download-failed");
    assert!(err.to_string().contains("404"), "{err}");
    assert!(!dest.exists());
}

/// A partial download is its own sentence, not "something went wrong".
#[test]
fn a_partial_download_is_named_as_one() {
    let root = scratch("partial");
    let dest = engine_install_dir(&root.join("home"));
    let err = install_engine(
        &FailingFetcher(|url| SetupError::DownloadIncomplete {
            url: url.to_string(),
            expected: 5_800_000,
            got: 1_204_224,
        }),
        &good_engine_plan("1.0.0"),
        &a_pin(A_DIGEST),
        &dest,
    )
    .unwrap_err();
    assert_eq!(err.kind(), "download-incomplete");
    assert!(err.to_string().contains("1204224"), "{err}");
    assert!(!dest.exists());
}

/// **THE ONE THAT DECIDES WHETHER A FAILURE COSTS HIM WHAT HE HAD.** An install over an
/// EXISTING engine that fails must leave the existing one exactly as it was.
#[test]
fn a_failed_reinstall_leaves_the_engine_he_already_had() {
    let root = scratch("keep-existing");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    make_engine(&dest, "1.0.0");
    std::fs::write(dest.join("MINE"), b"the engine he already had\n").unwrap();

    let err = install_engine(
        &FakeFetcher::new(b"tampered"),
        &good_engine_plan("1.0.0"),
        &a_pin(A_DIGEST),
        &dest,
    )
    .unwrap_err();
    assert_eq!(err.kind(), "digest-mismatch");

    assert!(engine_looks_valid(&dest), "the existing engine did not survive a failed install");
    assert_eq!(std::fs::read_to_string(dest.join("MINE")).unwrap(), "the engine he already had\n");
    assert_eq!(siblings_of(dest.parent().unwrap()), vec!["engine".to_string()]);
}

/// A reinstall activates new code while retaining older adopter-owned material.
#[test]
fn a_successful_reinstall_preserves_the_previous_engine_and_adopter_records() {
    let root = scratch("replace");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    make_engine(&dest, "0.9.0");
    std::fs::write(dest.join("OLD"), b"stale\n").unwrap();
    std::fs::create_dir_all(dest.join("ceo-wiki/wiki")).unwrap();
    std::fs::write(dest.join("ceo-wiki/wiki/private.md"), b"fictional private adoption record\n").unwrap();

    let body = b"x";
    let digest = body_and_digest(&root, body);
    let report = install_engine(&FakeFetcher::new(body), &good_engine_plan("1.0.0"), &a_pin(&digest), &dest)
        .unwrap();

    assert_eq!(engine_version(&dest).as_deref(), Some("1.0.0"));
    assert!(!dest.join("OLD").exists(), "the old engine's files survived the swap");
    assert_eq!(std::fs::read(dest.join("ceo-wiki/wiki/private.md")).unwrap(), b"fictional private adoption record\n");
    let previous = std::path::PathBuf::from(report.previous_at.unwrap());
    assert_eq!(std::fs::read(previous.join("ceo-wiki/wiki/private.md")).unwrap(), b"fictional private adoption record\n");
    assert_eq!(std::fs::read(previous.join("OLD")).unwrap(), b"stale\n");
    assert_eq!(siblings_of(dest.parent().unwrap()).len(), 2);
}

/// **A PANIC LEAVES NO RESIDUE.** `Staging`'s `Drop` removes its directory on unwind, so a
/// crash midway through an install cannot leave a half-tree the next boot would resolve.
#[test]
fn a_panic_during_an_install_removes_the_staging_directory() {
    let root = scratch("panic-residue");
    let home = root.join("home");
    let dest = engine_install_dir(&home);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();

    struct Panicking;
    impl Extractor for Panicking {
        fn extract(&self, _a: &Path, _i: &Path) -> Result<(), SetupError> {
            panic!("something went wrong halfway through");
        }
    }
    let body = b"x";
    let digest = body_and_digest(&root, body);

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        install_engine(&FakeFetcher::new(body), &Panicking, &a_pin(&digest), &dest)
    }));
    assert!(outcome.is_err(), "the fixture must actually panic");
    assert!(siblings_of(dest.parent().unwrap()).is_empty(), "residue after a panic");
}

// ===========================================================================================
// CLAUDE CODE
// ===========================================================================================

/// **RICHOS FETCHES ANTHROPIC'S INSTALLER AND NOTHING ELSE.** The URL is asserted, because the
/// license condition that matters most is "installed as published", and a mirror would break
/// it silently.
#[test]
fn the_only_url_fetched_for_claude_code_is_anthropics_own_installer() {
    let root = scratch("claude-url");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let fetcher = FakeFetcher::new(b"#!/bin/bash\necho hi\n");
    let runner = FakeRunner { code: 1, stderr: "no".into(), effect: None };

    let paths = SetupPaths { home: Some(home), ..Default::default() };
    let _ = install_claude_code(&fetcher, &runner, &paths);

    let asked = fetcher.asked.lock().unwrap().clone();
    assert_eq!(asked, vec![CLAUDE_INSTALLER_URL.to_string()]);
    assert_eq!(CLAUDE_INSTALLER_URL, "https://claude.ai/install.sh");
}

/// **A REFUSED INSTALL is the installer's own words, relayed.** Not paraphrased: what
/// Anthropic's script says is the instruction the customer needs.
#[test]
fn an_installer_that_refuses_is_relayed_verbatim_and_changes_nothing() {
    let root = scratch("installer-refused");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let runner = FakeRunner {
        code: 1,
        stderr: "Error: do not run this installer with sudo.".into(),
        effect: None,
    };
    let err = install_claude_code(
        &FakeFetcher::new(b"#!/bin/bash\n"),
        &runner,
        &SetupPaths { home: Some(home.clone()), ..Default::default() },
    )
    .unwrap_err();

    assert_eq!(err.kind(), "installer-refused");
    assert!(err.to_string().contains("do not run this installer with sudo"), "{err}");
    assert!(err.machine_unchanged());
    assert!(siblings_of(&app_support_richos(&home)).is_empty(), "the installer's staging survived");
}

/// **AN INSTALLER THAT EXITS 0 AND INSTALLED NOTHING IS A FAILURE.** The house lesson as a
/// test: the binary is located again from scratch, and a missing one is an error however
/// cheerful the exit code was.
#[test]
fn an_installer_that_exits_zero_without_installing_anything_is_still_a_failure() {
    let root = scratch("exit-zero-lie");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let runner = FakeRunner { code: 0, stderr: String::new(), effect: None };
    let err = install_claude_code(
        &FakeFetcher::new(b"#!/bin/bash\n"),
        &runner,
        &SetupPaths {
            home: Some(home),
            path_var: Some("/nonexistent".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.kind(), "claude-still-missing");
    assert!(err.to_string().contains(".local/bin/claude"), "{err}");
    assert!(!err.machine_unchanged(), "their installer ran; the Mac may have changed");
}

/// **A BODY THAT IS NOT A SCRIPT IS NEVER RUN.** A captive portal's HTML page reaching `bash`
/// is the worst possible way to find out the network was lying.
#[test]
fn a_download_that_is_not_a_script_is_never_executed() {
    let root = scratch("not-a-script");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();

    struct MustNotRun;
    impl Runner for MustNotRun {
        fn run_installer(&self, _s: &Path, _h: &Path) -> Result<(i32, String), SetupError> {
            panic!("BASH RAN ON A BODY WITH NO SHEBANG");
        }
    }

    let err = install_claude_code(
        &FakeFetcher::new(b"<!DOCTYPE html><html>Sign in to the WiFi</html>"),
        &MustNotRun,
        &SetupPaths { home: Some(home), ..Default::default() },
    )
    .unwrap_err();
    assert_eq!(err.kind(), "download-failed");
    assert!(err.to_string().contains("not a script"), "{err}");
}

/// **AN UNSIGNED, WRONGLY-SIGNED OR UNVERIFIABLE BINARY IS NOT RUN.** The installer here
/// "succeeds" and produces a plain file; `codesign` cannot make it Anthropic's, so setup
/// refuses. This is the negative half of the signature pin, and it needs no network.
#[test]
fn a_binary_that_is_not_anthropics_is_refused_after_the_installer_succeeds() {
    let root = scratch("bad-signature");
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".local/bin")).unwrap();
    let home_for_effect = home.clone();

    let runner = FakeRunner {
        code: 0,
        stderr: String::new(),
        effect: Some(Box::new(move || {
            std::fs::write(
                home_for_effect.join(".local/bin/claude"),
                b"#!/bin/sh\necho not claude\n",
            )
            .unwrap();
        })),
    };
    let err = install_claude_code(
        &FakeFetcher::new(b"#!/bin/bash\n"),
        &runner,
        &SetupPaths { home: Some(home), ..Default::default() },
    )
    .unwrap_err();

    assert_eq!(err.kind(), "signature-rejected");
    assert!(err.to_string().contains("won't confirm it came from Anthropic"), "{err}");
    assert!(!err.machine_unchanged());
}

/// **THE POSITIVE HALF, ON THE REAL BINARY.** If this machine has Anthropic's `claude`, the
/// same function that guards an install verifies it and says `trusted`. A negative-only
/// signature test is one that passes because everything fails.
///
/// # It used to be a negative-only signature test on most machines, and it said so itself
///
/// Until 2026-09-05 this body opened with two early `return`s — no `$HOME`, and no
/// `~/.local/bin/claude` — each preceded by an `eprintln!("SKIPPED: …")`. **A test that
/// returns is reported `ok`, and cargo captures the stderr of a passing test and throws it
/// away**, so the intent was legible in this file and nowhere in any log. It carried no
/// `cfg`, `app-spine-ci.yml` runs `cargo test --locked -p richos-core` on `ubuntu-latest`,
/// and no Linux runner has a `claude` — so this line has read `ok` on every CI run since
/// that job landed, over a signature check that could not even have been attempted there.
///
/// # Two conditions, and only one of them is a machine's business
///
/// [`verify_claude_signature`] runs `/usr/bin/codesign`. That is macOS's, so **off macOS
/// this test can never run, whatever is installed** — a compile-time fact about the target,
/// which is what `#[cfg_attr(…, ignore)]` is for. libtest then prints `ignored, <reason>`
/// and counts it in its own column: visibly not-run, in the log, on the runner that until
/// today printed `ok`.
///
/// **On macOS the absence of `claude` is a finding, not a reason to skip**, and this test is
/// a hard failure rather than a quiet pass. `richos-voice`'s `tts.rs` reached the same
/// conclusion about `/usr/bin/say` on 2026-09-05, and the reason is the same one: the value
/// of a green suite is that somebody checked, so a run that could not check must not be
/// green. The message names what is missing and what to do about it — installing Claude Code
/// is what `install_claude_code` above exists to do, and `$RICHOS_CLAUDE_BIN` points this at
/// a binary anywhere.
///
/// # Why the condition is NOT a build-script `cfg`, unlike `richos-voice`
///
/// `crates/richos-voice/build.rs` turns `RICHOS_VOICE_LIVE_AUDIO` into `cfg(live_audio)`, and
/// the obvious move here was to do the same for "does this machine have a `claude`". It was
/// measured instead of assumed: emitting one extra `cargo::rustc-cfg` from
/// `crates/richos-core/build.rs` changes the release library's bytes —
/// `librichos_core.rlib` went `c3f2d079…` → `e8b810c7…` and back to `c3f2d079…` when the line
/// was removed. That would make a crate that ships inside the signed `.app` compile
/// differently depending on whether the BUILD machine happened to have Claude Code installed,
/// which is a real cost against `docs/verification/reproducible-rust-builds-2026-09-04/` and
/// against this crate's own `build.rs` header (*"Nothing else belongs in this file"*). A
/// `target_os` cfg costs nothing: it is part of the build's identity already.
#[test]
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "NOT CHECKABLE ON THIS TARGET: the Anthropic signature pin is read by \
              /usr/bin/codesign, which is macOS's. This is a not-run, not a pass — run \
              it on a Mac. --include-ignored reaches a panic here, never a green line."
)]
fn the_real_claude_binary_on_this_machine_satisfies_the_requirement() {
    // `#[ignore]` suppresses the RUN, not the BODY: `--include-ignored` on Linux would reach
    // this line. Refuse in words rather than assert something meaningless about `codesign`.
    assert!(
        cfg!(target_os = "macos") && Path::new("/usr/bin/codesign").is_file(),
        "the signature pin is a macOS designated-requirement check and /usr/bin/codesign \
         is not runnable here, so nothing was verified. You reached this body with \
         `--include-ignored` or `--ignored`; on a Mac it runs by default."
    );

    // THE PRODUCT'S OWN LOCATOR, not a second definition of where `claude` lives: the
    // explicit `$RICHOS_CLAUDE_BIN`, then `~/.local/bin/claude`, then `$PATH` — so a binary
    // this app WOULD drive is a binary this test checks, and an install somewhere unusual is
    // not silently treated as no install at all.
    let found = find_claude(&SetupPaths::from_process());
    let at = match (found.present, &found.at) {
        (true, Some(at)) => PathBuf::from(at),
        _ => panic!(
            "no `claude` on this machine, so THE SIGNATURE PIN WAS NOT CHECKED BY THIS \
             RUN — and a check that did not happen must not report `ok`. Looked in: {}. \
             Install Claude Code (`install_claude_code`, tested above, is what RichOS \
             does for a customer) or set $RICHOS_CLAUDE_BIN to an existing binary.",
            found.looked_in.join("; ")
        ),
    };

    let verdict = verify_claude_signature(&at);
    assert!(verdict.trusted, "the installed claude failed its own designated requirement: {verdict:?}");

    // The SYMLINK is resolved, so the verdict names the file that was checked. `stapler`
    // exits 0 on the unresolved symlink having validated nothing; this must not do that.
    // Stated as the exact identity rather than a substring, so it holds for an install that
    // is not laid out the way Anthropic's installer lays one out.
    let canonical = std::fs::canonicalize(&at)
        .unwrap_or_else(|e| panic!("{} could not be resolved: {e}", at.display()));
    assert_eq!(
        verdict.checked,
        canonical.display().to_string(),
        "the verdict does not name the file that was checked: {verdict:?}"
    );

    // And for the standard install — the launcher Anthropic's installer writes and retargets
    // on every self-update — the resolved path is the versioned one, which is the concrete
    // form of the same property and the one that caught the unresolved-symlink bug.
    if at.ends_with(".local/bin/claude") {
        assert!(
            verdict.checked.contains("versions/"),
            "the symlink was not resolved before checking: {verdict:?}"
        );
    }
}

/// The designated requirement is the measured one, character for character. If somebody
/// loosens it this fails — a requirement without the team identifier would accept any
/// Developer ID binary that happened to use the same bundle identifier.
#[test]
fn the_designated_requirement_names_both_the_identifier_and_the_team() {
    assert!(CLAUDE_DESIGNATED_REQUIREMENT.contains("identifier \"com.anthropic.claude-code\""));
    assert!(CLAUDE_DESIGNATED_REQUIREMENT.contains("anchor apple generic"));
    assert!(CLAUDE_DESIGNATED_REQUIREMENT.contains("subject.OU] = \"Q6L2SF6YDW\""));
}

/// **THE ARGUMENT SYNTAX IS PART OF THE CHECK.** Without the leading `=`, `codesign -R` reads
/// its argument as a FILENAME, fails to open it, and exits non-zero — a false rejection that
/// is indistinguishable from a bad signature. This module made that mistake once; the test
/// above caught it against the real binary, and this one keeps the fix from being tidied away.
#[test]
fn the_requirement_is_passed_to_codesign_as_text_and_not_as_a_filename() {
    let arg = codesign_requirement_arg();
    assert!(arg.starts_with('='), "codesign would read this as a path: {arg}");
    assert_eq!(&arg[1..], CLAUDE_DESIGNATED_REQUIREMENT);
}

/// No HOME is a named error, not a panic and not a guess at `/Users/somebody`.
#[test]
fn no_home_is_a_named_refusal() {
    let err = install_claude_code(
        &FakeFetcher::new(b"#!/bin/bash\n"),
        &FakeRunner { code: 0, stderr: String::new(), effect: None },
        &SetupPaths::default(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), "no-home");
}

// ===========================================================================================
// DIGEST
// ===========================================================================================

/// The digest is SHA-256, and it matches the vectors every implementation agrees on.
#[test]
fn the_digest_is_sha256_and_matches_the_known_vectors() {
    let root = scratch("digest");
    let empty = root.join("empty");
    std::fs::write(&empty, b"").unwrap();
    assert_eq!(
        sha256_file(&empty).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    let abc = root.join("abc");
    std::fs::write(&abc, b"abc").unwrap();
    assert_eq!(
        sha256_file(&abc).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// **THE DIGEST STREAMS.** 200,000 bytes is past the 64 KiB buffer, and the expected value was
/// computed independently rather than by the code under test. A single-read implementation
/// that silently hashed only the first block would pass the vectors above and fail here.
#[test]
fn the_digest_is_correct_across_the_buffer_boundary() {
    let root = scratch("streaming");
    let big = root.join("big");
    let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(&big, &bytes).unwrap();
    assert_eq!(
        sha256_file(&big).unwrap(),
        "e24bc62381f1224fbbb74688663f8f9743b9680b193edd666835e97b06e730eb"
    );
}

/// And it agrees with the tool everybody else uses. Not because `shasum` is trusted more —
/// the point of a pure-Rust digest is that it is not a subprocess — but because a private
/// definition of SHA-256 that nobody else shares would make the published digest useless.
///
/// **A MISSING TOOL USED TO RETURN, AND A TEST THAT RETURNS IS REPORTED `ok`.** Same defect as
/// the signature test above, in the one place where the condition is not an opt-in at all:
/// this test's whole content is a COMPARISON, so a run in which no comparison happened has
/// nothing to report but a not-run. It now panics, in the words `richos-voice`'s `tts.rs`
/// already uses about a missing `/usr/bin/say` (32cefd2, today).
///
/// It is deliberately NOT `#[ignore]`d and NOT `cfg`-gated on a target: it needs no hardware,
/// no opt-in and no network, and it runs on `app-spine-ci.yml`'s `ubuntu-latest` runner today.
/// Ignoring it there would remove real coverage to fix a report that is not wrong.
///
/// **Two names for the same tool, because `/usr/bin/shasum` is a macOS certainty and a Linux
/// probability.** It is on macOS's sealed system volume; on Linux it comes with the full Perl
/// package and coreutils' `sha256sum` is the one that is always there. This crate's own
/// hooks already try exactly this pair in exactly this order — `engine/docs/ci-portability-
/// notes.md:240` — so the panic below fires only if a machine has NEITHER, which is a finding
/// about the machine rather than a test worth skipping. Both print the digest first, so the
/// same `split_whitespace().next()` reads both.
#[test]
fn the_digest_agrees_with_shasum() {
    let root = scratch("shasum-agreement");
    let f = root.join("bytes");
    std::fs::write(&f, b"RichOS engine asset\n").unwrap();

    let candidates: [(&str, &[&str]); 2] =
        [("/usr/bin/shasum", &["-a", "256"]), ("/usr/bin/sha256sum", &[])];
    let mut theirs: Option<(&str, String)> = None;
    for (bin, args) in candidates {
        let Ok(out) = std::process::Command::new(bin).args(args).arg(&f).output() else { continue };
        if !out.status.success() {
            continue;
        }
        let digest = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        theirs = Some((bin, digest));
        break;
    }
    let Some((tool, theirs)) = theirs else {
        panic!(
            "the whole content of this test is agreement with the tool everybody else uses, \
             and neither /usr/bin/shasum nor /usr/bin/sha256sum could be run here — refusing \
             to report green over a comparison that did not happen"
        )
    };

    assert_eq!(sha256_file(&f).unwrap(), theirs, "sha256_file disagrees with {tool}");
}

// ===========================================================================================
// THE CONSENT COPY
// ===========================================================================================

/// **NO TERMINAL, NO PATH, NO VERSION NUMBER.** The strings the CEO reads are checked for what
/// they must not contain, because the constraint is about what he is spared.
#[test]
fn the_consent_copy_carries_no_path_no_version_and_no_terminal() {
    for c in [Component::ClaudeCode, Component::Engine] {
        let line = format!("{} — {}", c.display_name(), c.why());
        assert!(!line.contains('/'), "a path reached his screen: {line}");
        assert!(!line.contains("Terminal"), "{line}");
        assert!(!line.contains('$'), "{line}");
        assert!(
            !line.chars().any(|ch| ch.is_ascii_digit()),
            "a version number reached his screen: {line}"
        );
    }
    // Plain text, and the mark is nobody's but Anthropic's to use.
    assert_eq!(Component::ClaudeCode.display_name(), "Claude Code");
}

/// Every error carries a sentence a non-technical reader can act on, none is a bare code, and
/// `machine_unchanged` is never unknown.
#[test]
fn every_failure_says_something_a_person_can_act_on() {
    let cases: Vec<SetupError> = vec![
        SetupError::NoHome,
        SetupError::NoNetwork { url: "u".into(), detail: "d".into() },
        SetupError::DownloadFailed { url: "u".into(), status: "404".into() },
        SetupError::DownloadIncomplete { url: "u".into(), expected: 10, got: 3 },
        SetupError::DigestMismatch { url: "u".into(), expected: "a".into(), got: "b".into() },
        SetupError::EngineUnpinned,
        SetupError::EngineShapeInvalid { detail: "d".into() },
        SetupError::EngineVersionMismatch { expected: "1".into(), found: "2".into() },
        SetupError::InstallerRefused { code: 1, stderr: "s".into() },
        SetupError::ClaudeStillMissing { looked_in: "p".into() },
        SetupError::SignatureRejected { path: "p".into(), detail: "d".into() },
        SetupError::InstallFailed { what: "w".into(), detail: "d".into() },
    ];
    let mut kinds = std::collections::HashSet::new();
    for e in &cases {
        let s = e.to_string();
        assert!(s.len() > 40, "too terse to act on: {s}");
        // A SENTENCE, not a code. The check is on the FIRST sentence rather than the last
        // character, because three variants deliberately end by relaying somebody else's
        // words verbatim — Anthropic's installer, `codesign`, a list of paths — and
        // paraphrasing those to make a period land would lose the instruction the customer
        // needs.
        let first = s.split('.').next().unwrap_or_default();
        assert!(s.contains('.'), "no sentence at all: {s}");
        assert!(first.len() >= 25, "opens with a fragment rather than a sentence: {s}");
        assert!(kinds.insert(e.kind()), "two errors share a kind tag: {}", e.kind());
        // `bool` cannot be "unknown", which is the property being asserted.
        let _: bool = e.machine_unchanged();
    }
    assert_eq!(kinds.len(), 12, "a variant was added without a kind tag");
}
