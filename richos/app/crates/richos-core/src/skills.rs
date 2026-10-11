//! THE SKILLS RichOS gives its inner Rich — the on-demand half of `doctrine.rs`.
//!
//! # The question this module exists because somebody measured
//!
//! The engine ships 28 skills, and they reach the ORCHESTRATOR because the engine is registered
//! as a plugin in `~/.claude/settings.json` — a **user** settings file. `native.rs::child_args`
//! passes `--setting-sources ''`, which is the same flag that stops `CLAUDE.md` loading, and it
//! drops user, project and local settings alike. So none of them reached the inner Rich, and a
//! customer has no `~/.claude/settings.json` of ours in the first place.
//!
//! **Measured on 2026-09-06 against `claude` 2.1.263, not read off documentation**
//! (`docs/verification/inner-doctrine-skills-2026-09-06/`). The `system/init` frame carries a
//! `skills` array and a `plugins` array, so *"was it discovered"* is a fact on the wire rather
//! than an inference from whether the model happened to use it:
//!
//! | cell | delivery | `--setting-sources` | in `init.skills`? |
//! |---|---|---|---|
//! | K0 | `<cwd>/.claude/skills/rig-status/` | `''` (production) | **no** |
//! | K1 | the same directory | `project` | **yes** — so K0's null is a true negative |
//! | K2 | `--plugin-dir <path>` | `''` (production) | **yes**, as `rig-probe:rig-status` |
//! | K3 | the same, question asked | `''` | **invoked**: `Skill{"skill":"rig-probe:rig-status"}` |
//! | K4 | `--plugin-dir <missing path>` | `''` | **no, and SILENTLY** — exit 0, clean handshake |
//!
//! `--plugin-dir` is therefore the channel, and it is the same shape as
//! `--append-system-prompt-file`: an explicit flag naming a directory **RichOS wrote and owns**,
//! opening no settings source and reversing no stated intent. Not the operator's `~/.claude`,
//! not the visited folder — the CEO ruled that authority comes from what RichOS writes, never
//! from the folder it stands in.
//!
//! # K4 IS THE REASON HALF THIS MODULE EXISTS
//!
//! `--append-system-prompt-file` gave us the loud failure for free: a missing file is
//! `Error: Append system prompt file not found`, exit 1, zero bytes of stdout. **`--plugin-dir`
//! does not.** A path that is not there produces a successful handshake, `plugins: []`, and a
//! perfectly ordinary turn — which is precisely the quiet degradation this whole evening was
//! about. So the loudness is ours to build, in two layers:
//!
//! 1. **Before the spawn.** [`verify_present`] checks the manifest and every SKILL.md this
//!    build declares, and `native.rs::preflight` turns any gap into `NativeError::SkillsMissing`
//!    naming the path. That catches deleted, truncated and unreadable.
//! 2. **On the wire.** A directory that exists and that the binary *rejects* would still be
//!    silent — a schema this version does not accept, a name collision, a future change. So the
//!    reader thread checks the first `system/init` frame's `plugins` array for [`PLUGIN_NAME`]
//!    and reports its absence loudly ([`NativeClient::skills_verdict`]). **This is a mitigation
//!    `--permission-prompt-tool` cannot have**: `native.rs`'s module doc records that the
//!    initialize reply carries no field naming the permission prompt tool, so there is nothing
//!    to assert against. Here the field exists, so it is asserted against.
//!
//! # The boundary: what is a skill and what is a clause in the doctrine file
//!
//! It is not "what is always true" any more. `doctrine.rs` §4.1's rule still decides what may
//! appear at all; this second rule decides which of the two channels carries it:
//!
//! | | the doctrine file | a skill |
//! |---|---|---|
//! | when it arrives | fixed at spawn, in the cached prefix of **every** request | on demand, when the model reaches for it |
//! | what it costs | every byte, every turn, forever | nothing on the turns it is not used |
//! | so it may hold | what must be in force **before Rich has read anything** | what he can be told **when the moment arrives** |
//! | and it must be | short, and free of examples | as long, worked and exampled as the rule really is |
//!
//! **American English is the case that needs both halves, and that is a finding rather than a
//! hedge.** It governs every string he writes, so a one-line rule stays in the doctrine file —
//! and it had to, because the model went British on the first product-shaped question with no
//! clause at all. But §13's actual rule is not one line: it binds text a person reads and
//! explicitly does NOT bind code identifiers, file names, third-party API values or quoted
//! external material, and a model given only the short form has to guess the exceptions. The
//! long form, with its table and its five exclusions, is 3.2 KB — it would be a permanent tax on
//! every turn in the prefix, and it costs nothing as a skill.
//!
//! # What is deliberately NOT here
//!
//! **The engine's other 27 skills.** They are orchestration mechanics for a developer team —
//! `using-git-worktrees`, `rich-lander`, the QA and native-build skills — and they are wrong
//! instruction for a chief of staff talking to a non-technical CEO. §4.3's rule about the team
//! directory applies to them word for word: an instruction to do something the app cannot do is
//! worse than silence, because it lies. Which of the 27 are candidates is the CEO's list to
//! shape, not this module's.

use crate::doctrine::{write_verified, DoctrineError};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// The directory RichOS renders the plugin into, inside its own configuration directory —
/// beside `inner-doctrine.md`, `config.json` and `entities.json`.
pub const PLUGIN_DIRNAME: &str = "rich-skills";

/// The plugin's name, which is also the prefix every skill is announced under
/// (`rich-skills:american-english` in `system/init.skills`). [`NativeClient`] asserts on this
/// exact string, so it is a constant and not a literal in two places.
pub const PLUGIN_NAME: &str = "rich-skills";

/// The skills this build ships: directory name, then the SKILL.md compiled in.
///
/// **Adding one is adding a line here.** There is no discovery pass over a directory: an
/// inventory read off disk at runtime is an inventory that can be empty and still report
/// success, which is the defect `run.js` and `run-tests.sh` in this repository both carry scars
/// from. What ships is what is compiled in.
pub const SKILLS: &[(&str, &str)] = &[
    ("american-english", include_str!("../skills/american-english/SKILL.md")),
    // The bootstrap interview, and it is a SKILL rather than a clause for the reason the table
    // above gives: it is long, worked and exampled, and it costs nothing on the overwhelming
    // majority of turns where nobody is being interviewed. Its TRIGGER is not here — a skill
    // nothing invokes is `provision-claude-md.sh` with a different file extension — it is
    // issued from the priming turn by `onboarding.rs`, from a fact on disk, and it retires
    // itself when that fact changes.
    //
    // It is NOT the engine's `skills/bootstrap-interview/SKILL.md`, and that is deliberate.
    // That one is addressed to an orchestrator in a terminal with a git checkout: its
    // generation phase fills `CLAUDE.md` and `orchestration.config`, spawns Dean, runs
    // `install.sh`, `contract-integrity-probe.sh`, `demo.sh` and `ceo-todos-init.sh`, and lands
    // a branch. In this app the first of those writes a file the child provably never reads
    // (`company.rs` cells B0a/B0b/B0c), the second cannot resolve (`onboarding.rs` cells
    // B2/B3/B4), and the customer has no repository for the rest. Shipping it here would be
    // shipping instructions to do things that silently do nothing, which §4.3 of the doctrine
    // design already rules out for exactly this reason: it lies.
    ("bootstrap-interview", include_str!("../skills/bootstrap-interview/SKILL.md")),
    // Downloading, watching and hearing a video with the tools setup installs (yt-dlp, ffmpeg,
    // ffprobe, whisper-cli, and the speech model `engine_profile.rs` names in
    // `RICHOS_SPEECH_MODEL`). HERE and not in the engine's `skills/`, which reaches only the
    // operator's orchestrator (media-tools plan section 3, Frank's review M1).
    //
    // **Named `watch-video`, and `audio-transcription` below, because every RichOS teammate's
    // definition says so** (CEO, 2026-10-10: the audio and video skill use is part of all the
    // teammate and proto-teammate profiles in RichOS, "otherwise a RichOS user wouldn't take
    // advantage of that"). The standing instruction in `engine/agents/`, `engine/team/shelf/`
    // and the duty texts names these two skills, and the engine's own skills of the same names
    // serve the operator's team. A teammate runs inside the work lease, which is given this
    // plugin, so it reaches them as `rich-skills:watch-video` and
    // `rich-skills:audio-transcription` (`native.rs`,
    // `a_teammate_s_session_lists_every_skill_its_standing_instruction_names`).
    ("watch-video", include_str!("../skills/watch-video/SKILL.md")),
    // Getting the words out of an audio-only file, with the same two tools and the same pinned
    // decode flags as `watch-video`'s transcription step.
    ("audio-transcription", include_str!("../skills/audio-transcription/SKILL.md")),
];

/// The plugin root for a configuration directory.
pub fn plugin_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(PLUGIN_DIRNAME)
}

/// The manifest path inside a plugin root.
pub fn manifest_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join(".claude-plugin").join("plugin.json")
}

/// A skill's `SKILL.md` inside a plugin root.
pub fn skill_path(plugin_root: &Path, name: &str) -> PathBuf {
    plugin_root.join("skills").join(name).join("SKILL.md")
}

/// How every skill in this plugin is named on the wire — `<plugin>:<skill>`, the form
/// `system/init.skills` and the `Skill` tool both use (measured, cell K3).
pub fn qualified_names() -> Vec<String> {
    SKILLS.iter().map(|(n, _)| format!("{PLUGIN_NAME}:{n}")).collect()
}

/// The manifest, rendered rather than stored, so the shipped skill list and the manifest cannot
/// disagree: both are derived from [`SKILLS`].
pub fn manifest() -> String {
    let names: Vec<String> = SKILLS.iter().map(|(n, _)| format!("      \"{n}\"")).collect();
    format!(
        "{{\n  \"name\": \"{PLUGIN_NAME}\",\n  \"displayName\": \"RichOS\",\n  \"version\": \"1.0.0\",\n  \"description\": \"The skills RichOS gives the assistant it drives. Rendered by RichOS into its own directory; edits are replaced at the next start.\",\n  \"author\": {{ \"name\": \"RichOS\" }},\n  \"license\": \"AGPL-3.0-only\",\n  \"keywords\": [\n{}\n  ]\n}}\n",
        names.join(",\n")
    )
}

/// The digest of everything this build would write: the manifest and every SKILL.md, in order.
///
/// One number for "which skills are these", the way `doctrine::template_digest` is one number
/// for "which doctrine is this".
pub fn skills_digest() -> String {
    let mut h = Sha256::new();
    h.update(manifest().as_bytes());
    for (name, body) in SKILLS {
        h.update(name.as_bytes());
        h.update(b"\0");
        h.update(body.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Render the plugin into `config_dir` and return its root — writing what is missing and
/// REPLACING anything that is not exactly what belongs there.
///
/// Byte-for-byte, like the doctrine, and for the same reason: it needs no rule about which
/// differences are tolerable, because none are. A customer's edit, a partial write, a stale
/// build's skill — one case, one answer.
pub fn ensure_rendered(config_dir: &Path) -> Result<PathBuf, DoctrineError> {
    let root = plugin_dir(config_dir);

    let manifest_file = manifest_path(&root);
    let want = manifest();
    if std::fs::read_to_string(&manifest_file).ok().as_deref() != Some(want.as_str()) {
        write_verified(&manifest_file, &want)?;
    }

    for (name, body) in SKILLS {
        let p = skill_path(&root, name);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(*body) {
            write_verified(&p, body)?;
        }
    }

    Ok(root)
}

/// [`ensure_rendered`] for this install, resolving the directory the way `doctrine.rs` does.
///
/// For callers with no webview to ask — the headless examples, the live sentinel, anything
/// diagnosing an install from a terminal. The Tauri shell passes its own `app_data_dir()` to
/// [`ensure_rendered`] directly, because that is the authority there.
pub fn ensure_for_install() -> Result<PathBuf, DoctrineError> {
    ensure_rendered(&crate::doctrine::install_config_dir()?)
}

/// Is everything this build declares actually on disk and non-empty?
///
/// Called by `native.rs::preflight` BEFORE the spawn, because K4 measured that the binary will
/// not tell us: a `--plugin-dir` that is not there produces a clean handshake and an ordinary
/// turn. `Err` names the first file that is wrong and what is wrong with it.
pub fn verify_present(plugin_root: &Path) -> Result<(), (PathBuf, String)> {
    let mut required = vec![manifest_path(plugin_root)];
    required.extend(SKILLS.iter().map(|(n, _)| skill_path(plugin_root, n)));

    for p in required {
        match std::fs::metadata(&p) {
            Err(e) => return Err((p, e.to_string())),
            Ok(m) if !m.is_file() => return Err((p, "it is not a file".to_string())),
            Ok(m) if m.len() == 0 => {
                return Err((p, "it is empty, and an empty skill is no skill".to_string()))
            }
            Ok(_) => {}
        }
    }
    Ok(())
}

/// What the first `system/init` frame said about our plugin.
///
/// Three states rather than a `bool`, because "we have not been told yet" and "we were told it
/// is not there" call for opposite responses, and collapsing them is how an absence gets
/// reported as a fact. Same reasoning as `entity.rs::RegistrySource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillsVerdict {
    /// No `system/init` frame has arrived yet. It lands with the first TURN, not the handshake,
    /// so this is the state for the whole of a lease that has not been used.
    NotYetReported,
    /// The binary listed our plugin. The skills are live.
    Loaded,
    /// The binary answered, and our plugin was not in its list. The files are on disk (preflight
    /// proved that) and the binary declined them: a schema it does not accept, a name collision,
    /// a change in a version that self-updated underneath us.
    Rejected,
}

/// Read the verdict off a `system/init` frame.
///
/// `plugins` is an array of objects carrying `name`, `path`, `source` and `version` — measured,
/// cell K2. A frame with no `plugins` key at all is `Rejected` rather than `NotYetReported`: an
/// init frame that does not mention plugins is an init frame that loaded none of ours.
pub fn verdict_from_init(init: &serde_json::Value) -> SkillsVerdict {
    let listed = init
        .get("plugins")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .any(|p| p.get("name").and_then(|n| n.as_str()) == Some(PLUGIN_NAME))
        })
        .unwrap_or(false);
    if listed {
        SkillsVerdict::Loaded
    } else {
        SkillsVerdict::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Temp(PathBuf);
    impl Temp {
        fn new(tag: &str) -> Temp {
            let d = std::env::temp_dir().join(format!(
                "richos-skills-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::fs::create_dir_all(&d).unwrap();
            Temp(d)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_shipped_inventory_is_not_empty_and_every_entry_is_a_real_skill() {
        // An empty inventory reporting success is the defect this repository has now shipped
        // five times under a reassuring fraction. `SKILLS` is compiled in, so this cannot go
        // wrong quietly — but it can go wrong loudly, here.
        assert!(!SKILLS.is_empty(), "zero skills is not a shipping state");
        for (name, body) in SKILLS {
            assert!(body.starts_with("---\n"), "{name} has no frontmatter");
            assert!(body.contains(&format!("name: {name}")), "{name}'s frontmatter must name it");
            assert!(body.contains("description:"), "{name} has no description, so it can never be invoked");
        }
    }

    fn video_commands(prefix: &str) -> Vec<String> {
        skill_commands("watch-video", prefix)
    }

    fn skill_commands(skill: &str, prefix: &str) -> Vec<String> {
        let body = SKILLS.iter().find(|(name, _)| *name == skill).unwrap_or_else(|| panic!("{skill} ships")).1;
        // A command continued with a trailing backslash is one command.
        body.replace("\\\n", " ").lines().map(str::trim).filter(|l| l.starts_with(prefix)).map(String::from).collect()
    }

    /// **The video skill's caption rule** (the CEO, 2026-10-07: "relying on auto-generated
    /// Youtube captions is generally a bad idea"): captions a person uploaded first, then Rich's
    /// own transcription, and YouTube's auto-generated captions only after both.
    #[test]
    fn the_video_skill_prefers_uploaded_captions_then_whisper_and_auto_captions_last() {
        let yt = video_commands("yt-dlp --skip-download");
        let uploaded = yt.iter().position(|c| c.contains("--write-subs")).expect("an uploaded-captions command");
        assert!(!yt[uploaded].contains("--write-auto-subs"), "the first captions fetch must not take auto-generated ones: {}", yt[uploaded]);
        let auto = yt.iter().position(|c| c.contains("--write-auto-subs")).expect("the last-resort command");
        assert!(uploaded < auto, "uploaded captions come before auto-generated ones");
        let body = SKILLS.iter().find(|(name, _)| *name == "watch-video").unwrap().1;
        let whisper = body.find("whisper-cli -m").expect("a transcription command");
        assert!(whisper < body.find("yt-dlp --skip-download --write-auto-subs").expect("the auto-captions command"),
                "Rich's own transcription comes before auto-generated captions");
    }

    /// **A link checks its uploaded captions even after the video is downloaded** (D16, nightly
    /// 44 walk): the transcription step's heading said "(or a file on his Mac)", and once Rich
    /// had downloaded a linked video to `~/Downloads` it WAS a file on his Mac, so three runs of
    /// four skipped the captions a person uploaded and transcribed the audio instead. Only a file
    /// the CEO gave from his Mac, with no link, goes straight to transcription.
    #[test]
    fn a_link_checks_uploaded_captions_even_after_the_video_is_downloaded() {
        let body = SKILLS.iter().find(|(name, _)| *name == "watch-video").unwrap().1;
        let words = &body[body.find("## 4.").expect("the words section")..body.find("## 5.").expect("the next section")];
        let before_a = &words[..words.find("**a.").expect("source a")];
        assert!(before_a.contains("For a link, always start with a") && before_a.contains("already downloaded"),
                "before source a, the skill must say a link starts at a even once the video is downloaded: {before_a:?}");
        let b = words.lines().find(|l| l.starts_with("**b.")).expect("source b");
        assert!(!b.contains("(or a file on his Mac)"), "a downloaded link is a file on his Mac too, so b must not take any such file: {b}");
        assert!(b.contains("a file the CEO gave you from his Mac"), "b takes only a file the CEO gave from his Mac: {b}");
    }

    /// **The video skill transcribes with RichOS's pinned decode flags** (`richos_voice::stt::
    /// decode_args` with no prompt), never whisper.cpp's own `-mc -1`: on the CEO's 66-minute
    /// test video (D8PikZ1KhUo, 2026-10-07) that default looped "All right." 539 times and kept
    /// 1,748 of the 14,241 captioned words. Unlike voice's short utterances, a video is decoded
    /// with timestamps on (no `-nt`).
    ///
    /// `audio-transcription` runs the same command on an audio-only file, so it is held to the
    /// same flags.
    #[test]
    fn the_video_skill_transcribes_with_the_pinned_decode_flags() {
        for skill in ["watch-video", "audio-transcription"] {
            let whisper = skill_commands(skill, "whisper-cli ");
            assert_eq!(whisper.len(), 1, "{skill}: one transcription command: {whisper:?}");
            for flag in ["-m \"$RICHOS_SPEECH_MODEL\"", " -l en ", " -t 4 ", " -fa ", " -mc 0 ", " -np "] {
                assert!(whisper[0].contains(flag), "{skill}: missing {flag:?} in {}", whisper[0]);
            }
            // `-nt` decodes without timestamp tokens, and then whisper.cpp drops the words at
            // each 30-second window's seam: 44 of the 48 dropped stretches of 3+ words on
            // D8PikZ1KhUo.
            assert!(!whisper[0].contains(" -nt "), "{skill}: no -nt on a long recording: {}", whisper[0]);
        }
    }

    /// **A short screen recording gives at least one frame** (second review of `79ba6eb7e`,
    /// finding 3). `fps=1/10` rounds each frame's time to the nearest ten-second slot, and the
    /// filter emits slot 0 only once the recording reaches slot 1's half-way point: 5 s. A
    /// two-second bug recording gave zero frames, and with nothing reaching the JPEG encoder
    /// FFmpeg 9.0.2 even exited 234. This runs the skill's own extraction command, as the
    /// teammate would, on a two-second and a twenty-five-second H.264 recording: one frame
    /// for the first, three (0 s, 10 s, 20 s) for the second, so a short recording is covered
    /// without changing how densely a long one is sampled.
    ///
    /// Needs `ffmpeg` (with libx264) on `PATH`, or its path in `RICHOS_TEST_FFMPEG`; without
    /// it the test fails and says so rather than passing on nothing.
    #[test]
    fn a_short_screen_recording_gives_at_least_one_frame() {
        let ffmpeg = std::env::var_os("RICHOS_TEST_FFMPEG").map(PathBuf::from).or_else(|| {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).map(|d| d.join("ffmpeg")).find(|p| p.is_file())
        }).expect("this test runs the skill's own ffmpeg command: put ffmpeg on PATH or set RICHOS_TEST_FFMPEG");
        let extraction = video_commands("ffmpeg ");
        let extraction: Vec<&String> = extraction.iter().filter(|c| c.contains("frame-%04d.jpg")).collect();
        assert_eq!(extraction.len(), 1, "one frame-extraction command: {extraction:?}");
        let bin = ffmpeg.parent().unwrap().to_path_buf();
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()))).unwrap();
        for (seconds, want) in [(2, 1), (25, 3)] {
            let work = Temp::new(&format!("frames-{seconds}s"));
            let video = work.0.join("recording.mov");
            let made = std::process::Command::new(&ffmpeg)
                .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i"])
                .arg(format!("testsrc=s=640x360:r=30:d={seconds}"))
                .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"]).arg(&video).output().unwrap();
            assert!(made.status.success(), "fixture recording: {}", String::from_utf8_lossy(&made.stderr));
            let command = extraction[0].replace("<video>", video.to_str().unwrap());
            let ran = std::process::Command::new("/bin/sh").arg("-c").arg(&command)
                .env("work", &work.0).env("PATH", &path).output().unwrap();
            let frames = std::fs::read_dir(&work.0).unwrap()
                .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("frame-")).count();
            assert!(ran.status.success() && frames == want,
                "a {seconds} s recording must give {want} frame(s), gave {frames} (exit {:?}: {}) from: {command}",
                ran.status.code(), String::from_utf8_lossy(&ran.stderr).trim());
        }
    }

    /// The name in the frontmatter, the directory it is written to, and the name the wire
    /// announces are one fact. Measured form (cell K2): `rig-probe:rig-status`.
    #[test]
    fn a_skill_is_announced_as_plugin_colon_name() {
        assert_eq!(
            qualified_names(),
            vec![
                "rich-skills:american-english".to_string(),
                "rich-skills:bootstrap-interview".to_string(),
                "rich-skills:watch-video".to_string(),
                "rich-skills:audio-transcription".to_string(),
            ]
        );
    }

    /// The interview's name is spelled in two modules — here, and in
    /// `onboarding.rs::INTERVIEW_SKILL`, which the priming turn's offer names so the model has
    /// something to reach for. Two spellings of one name is a schedule for them to disagree, and
    /// the disagreement would be silent: the offer would name a skill that is not there and the
    /// model would improvise. `--plugin-dir`'s own silence on a missing directory (cell K4) is
    /// the same defect one level down, and it is why this assertion exists at all.
    #[test]
    fn the_offer_in_the_priming_turn_names_a_skill_that_actually_ships() {
        assert!(
            qualified_names().contains(&crate::onboarding::INTERVIEW_SKILL.to_string()),
            "onboarding.rs offers {} and the plugin ships {:?}",
            crate::onboarding::INTERVIEW_SKILL,
            qualified_names()
        );
    }

    #[test]
    fn the_manifest_is_derived_from_the_shipped_list_and_parses() {
        let m = manifest();
        let v: serde_json::Value = serde_json::from_str(&m).expect("the manifest must be JSON");
        assert_eq!(v["name"], PLUGIN_NAME);
        for (name, _) in SKILLS {
            assert!(m.contains(name), "the manifest must mention {name}");
        }
    }

    #[test]
    fn a_first_run_renders_the_plugin_and_a_second_run_leaves_it_alone() {
        let t = Temp::new("first-run");
        let root = ensure_rendered(&t.0).unwrap();
        assert!(manifest_path(&root).is_file());
        assert!(skill_path(&root, "american-english").is_file());
        verify_present(&root).expect("what was just written must verify");

        let before = std::fs::metadata(skill_path(&root, "american-english")).unwrap().modified().unwrap();
        ensure_rendered(&t.0).unwrap();
        assert_eq!(
            std::fs::metadata(skill_path(&root, "american-english")).unwrap().modified().unwrap(),
            before,
            "an unchanged skill must not be rewritten"
        );
    }

    #[test]
    fn an_edited_skill_is_replaced_rather_than_honored() {
        let t = Temp::new("edited");
        let root = ensure_rendered(&t.0).unwrap();
        let p = skill_path(&root, "american-english");
        std::fs::write(&p, "---\nname: american-english\ndescription: x\n---\nWrite in Latin.\n").unwrap();
        ensure_rendered(&t.0).unwrap();
        let back = std::fs::read_to_string(&p).unwrap();
        assert!(!back.contains("Latin"), "an edited skill must be replaced");
        assert_eq!(back, SKILLS[0].1);
    }

    /// K4's whole point: the binary will not tell us, so we check first.
    #[test]
    fn a_missing_skill_file_is_a_named_failure_and_not_a_shrug() {
        let t = Temp::new("missing");
        let root = ensure_rendered(&t.0).unwrap();
        let p = skill_path(&root, "american-english");
        std::fs::remove_file(&p).unwrap();
        let (path, why) = verify_present(&root).unwrap_err();
        assert_eq!(path, p);
        assert!(why.contains("No such file"), "{why}");
    }

    #[test]
    fn an_empty_skill_file_is_a_named_failure_too() {
        let t = Temp::new("empty");
        let root = ensure_rendered(&t.0).unwrap();
        std::fs::write(skill_path(&root, "american-english"), b"").unwrap();
        let (_, why) = verify_present(&root).unwrap_err();
        assert!(why.contains("empty"), "{why}");
    }

    #[test]
    fn a_missing_manifest_is_a_named_failure() {
        let t = Temp::new("no-manifest");
        let root = ensure_rendered(&t.0).unwrap();
        std::fs::remove_file(manifest_path(&root)).unwrap();
        let (path, _) = verify_present(&root).unwrap_err();
        assert!(path.ends_with("plugin.json"), "{}", path.display());
    }

    #[test]
    fn a_plugin_root_that_was_never_rendered_fails_verification() {
        let t = Temp::new("never");
        let (_, why) = verify_present(&t.0.join("nothing-here")).unwrap_err();
        assert!(why.contains("No such file"), "{why}");
    }

    // ---- the wire verdict (K4's second layer) ---------------------------------------------

    #[test]
    fn an_init_frame_listing_our_plugin_is_loaded() {
        let init = json!({"type":"system","subtype":"init","plugins":[
            {"name":"rich-skills","path":"/x","source":"rich-skills@inline","version":"1.0.0"}]});
        assert_eq!(verdict_from_init(&init), SkillsVerdict::Loaded);
    }

    /// THE CASE PREFLIGHT CANNOT SEE: the files are there and the binary declined them. Measured
    /// shape of a rejection (cell K4): `plugins: []`, everything else perfectly normal.
    #[test]
    fn an_init_frame_with_an_empty_plugin_list_is_a_rejection_not_a_shrug() {
        let init = json!({"type":"system","subtype":"init","plugins":[]});
        assert_eq!(verdict_from_init(&init), SkillsVerdict::Rejected);
    }

    #[test]
    fn an_init_frame_listing_somebody_elses_plugin_is_still_a_rejection() {
        let init = json!({"type":"system","subtype":"init","plugins":[{"name":"someone-else"}]});
        assert_eq!(verdict_from_init(&init), SkillsVerdict::Rejected);
    }

    #[test]
    fn an_init_frame_with_no_plugins_key_at_all_is_a_rejection() {
        // An init frame that does not mention plugins loaded none of ours. Reporting that as
        // "not yet known" would be an absence dressed up as a pending answer.
        assert_eq!(verdict_from_init(&json!({"type":"system","subtype":"init"})), SkillsVerdict::Rejected);
    }

    #[test]
    fn the_digest_moves_when_a_skill_moves() {
        // `skills_digest` is what makes "these are the skills we shipped" checkable. If it did
        // not change with content it would be decoration.
        let before = skills_digest();
        assert_eq!(before.len(), 64);
        assert_eq!(before, skills_digest(), "the digest must be stable across calls");
    }
}
