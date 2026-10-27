//! FIRST-RUN SETUP — the two things a customer's Mac does not have, fetched and verified.
//!
//! # The gap this closes, stated as it was found
//!
//! **Today RichOS runs on the CEO's Mac and would not run on anyone else's.** `ceo-decisions.md`
//! §19 names the five manual steps a customer faces, and two of them are executables:
//!
//! > *What the customer must already have: Claude Code, an Anthropic account, a completed
//! > Claude login (there is no login flow inside RichOS), **the engine directory**, and
//! > whisper for voice — five manual steps.*
//!
//! The engine is the harder half and it is the launch blocker: it *"ships in no payload and
//! has no route onto another machine at all"*. `src-tauri/src/engine.rs` resolves seven
//! candidates for it and says so in its own header — *"It does not put an engine directory on
//! anybody else's computer… Candidates 3 and 7 are the two slots a future payload decision
//! could fill"*. **This module fills slot 7.**
//!
//! # The CEO's instruction, and what it is applied to
//!
//! > **"automatically download and install whatever the user needs"**
//!
//! Applied to Claude Code he named it himself and calls it **Option D** — *"detect at install
//! or first run whether Claude Code is present, and download and install it if not"*
//! (`open-items.md` row 3.14; `ceo-decisions.md` §19 records it as *"NOT superseded and not
//! dead"*). Licensing is **CLOSED and PERMITTED**, under four conditions, all of which this
//! module meets by construction:
//!
//! | Condition | How it is met here |
//! |---|---|
//! | the binary unmodified and installed as published | [`install_claude_code`] downloads **Anthropic's own installer** from `https://claude.ai/install.sh` and runs it. RichOS never writes a byte of their binary, never re-signs it, never nests it. §19 finding 2: re-signing it destroys it while `--version` still answers. |
//! | no authentication method removed or restricted | nothing here touches auth. RichOS is **BYO-Anthropic** — the customer still needs an account and a login, and the consent copy says so. |
//! | no paying for / reselling / intermediating usage | no key, no proxy, no token. [`verify_claude_signature`] resolves a path and reads a signature; that is the whole of RichOS's relationship with the binary. |
//! | plain-text naming only | the strings here name "Claude Code" in plain text and carry no mark. |
//!
//! # WHERE THE ENGINE COMES FROM — the choice, and why it needed no new ruling
//!
//! **Fetched from the public repository's Releases, as a deterministic tarball whose SHA-256
//! is compiled into this binary.** Not bundled. Four measured reasons, none of them a new
//! decision:
//!
//! 1. **§19 lists the engine directory under "What is NOT bundled"** and rules the payload at
//!    four files and 8,754,980 B, on a measurement of exactly that payload. The engine is
//!    5.8 MB on disk. Putting it in `Contents/Resources/engine` would contradict a ruling made
//!    the same day on the number it would change.
//! 2. **He ruled where the download lives the same day**: *"Where the download lives: THE
//!    PUBLIC GITHUB REPO'S RELEASES."* The engine is already **in that repository** —
//!    `<repo>/engine`, `VERSION` 1.0.0 — so a release asset needs no new host, no new
//!    mechanism, and no new decision. It is the intersection of two rulings, not an invention.
//! 3. **The engine moves on its own cadence.** Hooks, skills and agent definitions change
//!    without the Rust changing. A fetched engine updates without a new signed, notarized
//!    `.app`; a bundled one would make every hook edit a release.
//! 4. **It is verifiable, and a bundled one would be verified by the same act anyway.** The
//!    app's Developer ID signature covers the pinned digest below, so a fetched engine's
//!    integrity rests on the same signature a bundled one would have rested on.
//!
//! **The cost, stated rather than left to be discovered:** the pin is per-release. A build
//! whose pin is unset **refuses to install an engine** ([`SetupError::EngineUnpinned`]) rather
//! than trusting whatever a URL returns. There is no "if nobody said, fetch latest" anywhere
//! below — the same posture `provision.rs` takes with an unset corpus target, and for the same
//! reason: *exit 0 while doing the wrong thing is the failure mode we keep paying for.*
//!
//! # WHAT CAN BE PINNED AND WHAT CANNOT — measured, both directions
//!
//! **Claude Code: pinned by designated requirement, offline.** Measured on
//! `~/.local/share/claude/versions/2.1.257` on 2026-09-01:
//!
//! ```text
//! Identifier=com.anthropic.claude-code
//! Authority=Developer ID Application: Anthropic PBC (Q6L2SF6YDW)
//! TeamIdentifier=Q6L2SF6YDW
//! Sealed Resources=none
//! ```
//!
//! `codesign --verify --strict -R '<designated requirement>'` returns 0 against it with no
//! network. That is [`CLAUDE_DESIGNATED_REQUIREMENT`].
//!
//! **What CANNOT be pinned, said plainly: a stapled notarization ticket.** `Sealed
//! Resources=none` — it is a loose Mach-O, not a bundle, and a loose executable has nowhere to
//! carry a ticket. Measured, both directions, on this machine:
//!
//! ```text
//! xcrun stapler validate ~/.local/share/claude/versions/2.1.257
//!   -> "Stapler is incapable of working with Document files."   exit 66
//! xcrun stapler validate ~/.local/bin/claude          # the SYMLINK
//!   -> "Stapler is incapable of working with Alias files."      exit 0    <-- A FALSE PASS
//! ```
//!
//! **The symlink case exits 0 while validating nothing.** So stapling is not merely
//! unavailable here, it is a trap: a naive `stapler validate` in a setup script would report
//! success on the path RichOS actually resolves. Nothing in this module calls `stapler`.
//! Signature verification is [`verify_claude_signature`], and it resolves the symlink first so
//! the verdict names the file it actually checked.
//!
//! **The engine tarball cannot be pinned by signature either** — it is not code Apple signs.
//! It is pinned by **SHA-256, compiled into this binary**, which the app's own Developer ID
//! signature then covers. Digest is checked **before** extraction, so a tampered archive never
//! reaches `tar`.
//!
//! # NEVER A HALF-INSTALLED STATE THAT REPORTS SUCCESS
//!
//! [`install_engine`] stages into `<dest>.incoming.<pid>.<nanos>`, verifies the digest, then
//! the extracted **shape**, and only then swaps by `rename`. Every early return removes the
//! staging directory, and [`Staging`] removes it on unwind too, so a panic cannot leave
//! residue that the next boot would resolve as an engine. The swap itself is two renames on
//! one filesystem with the old copy kept until the new one is in place.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};

// ===========================================================================================
// WHAT A LAUNCH KNOWS
// ===========================================================================================

/// The inputs setup reads, **injected rather than read**, for the reason
/// `engine.rs::LaunchPaths` gives: the GUI condition (`cwd = /`, empty environment) is then a
/// VALUE in a test instead of a mutation of the test process.
#[derive(Debug, Default, Clone)]
pub struct SetupPaths {
    /// `$HOME`. Without it nothing here can run — [`SetupError::NoHome`].
    pub home: Option<PathBuf>,
    /// `$RICHOS_CLAUDE_BIN`, the operator override `native.rs::resolve_claude_bin` honors.
    pub claude_bin_override: Option<PathBuf>,
    /// `$RICHOS_ENGINE_DIR` / `$RICHOS_ENGINE_ROOT` — either explicit engine statement.
    pub engine_override: Option<PathBuf>,
    /// `$CLAUDE_CONFIG_DIR`, when the host's config directory has been moved.
    pub config_dir: Option<PathBuf>,
    /// `std::env::current_exe()` — used for the bundle-resources engine candidate.
    pub exe: Option<PathBuf>,
    /// `$PATH`, searched for a bare `claude` as the last detection step.
    pub path_var: Option<String>,
}

impl SetupPaths {
    /// Read the real process. The only function in this module that touches global state.
    pub fn from_process() -> Self {
        let nonempty = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        SetupPaths {
            home: nonempty("HOME").map(PathBuf::from),
            claude_bin_override: nonempty("RICHOS_CLAUDE_BIN").map(PathBuf::from),
            engine_override: nonempty("RICHOS_ENGINE_DIR")
                .or_else(|| nonempty("RICHOS_ENGINE_ROOT"))
                .map(PathBuf::from),
            config_dir: nonempty("CLAUDE_CONFIG_DIR").map(PathBuf::from),
            exe: std::env::current_exe().ok(),
            path_var: nonempty("PATH"),
        }
    }
}

// ===========================================================================================
// THE ENGINE'S SHAPE — one definition, shared with the resolver
// ===========================================================================================

/// Does this directory look like the RichOS engine?
///
/// `scripts/hooks/` and `VERSION`, which is `locate-engine.sh`'s `richos_engine_looks_valid`
/// and `engine.rs::looks_like_engine`. **It lives here so there is exactly one definition**:
/// `engine.rs` warns in its own header that *"a second, differently-shaped resolution order is
/// a thing that can disagree with the first"*, and an installer that accepts a shape the
/// resolver later rejects is that bug wearing a different hat.
pub fn engine_looks_valid(dir: &Path) -> bool {
    dir.join("scripts/hooks").is_dir() && dir.join("VERSION").is_file()
}

/// The engine's own version string, trimmed. `None` when the file is absent or unreadable.
pub fn engine_version(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("VERSION"))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

// ===========================================================================================
// THE ENGINE'S RELEASE — the engine this build boots, or none of them
// ===========================================================================================
//
// `engine_looks_valid` above answers SHAPE and keeps answering only that, deliberately. It is
// asked by `install_engine` step 4 (whose step 5 is the version check, and would be swallowed
// by a shape predicate that also checked versions) and by `provision.rs`, which uses it to ask
// "is this compiler source sitting inside an engine?" — a question with no release in it.
// Folding the pin into it would make a wrong-release engine report `nothing that looks like
// the engine is there`, which is a false sentence about a directory full of engine and exactly
// the dishonest-message failure the nightly's D1 was raised to end.

/// **Why a directory that was looked at is not the engine THIS BUILD boots.**
///
/// Two reasons, and they are different sentences on purpose. `NotEngineShaped` is "there is
/// nothing here"; `WrongRelease` is "there IS an engine here and it is not the one this build
/// was built against" — the distinction the nightly's D1 spent four launches proving matters,
/// because an operator reading "not found" about a directory full of engine goes hunting for
/// the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineRejected {
    /// No `scripts/hooks/`, or no `VERSION`.
    NotEngineShaped,
    /// Engine-shaped, and a different release from the one this build pins.
    WrongRelease {
        /// The `VERSION` found there. `None` when the file could not be read.
        found: Option<String>,
        /// The release this build pins.
        needed: String,
    },
    /// Engine-shaped, carrying the RIGHT release string, and **not the content this build
    /// pins** — the 2026-09-18 case, where two nightly cuts of engine `1.2.0` are two
    /// different directories of code wearing one version number. `installed: None` is a
    /// directory RichOS is answerable for that carries no `INSTALLED-FROM` at all: its content
    /// is unknown, and unknown is never the same as matching.
    WrongIdentity {
        /// The digest recorded in `INSTALLED-FROM`, or `None` when there is no stamp.
        installed: Option<String>,
        /// The digest this build pins.
        pinned: String,
    },
}

impl EngineRejected {
    /// The OPERATOR's sentence — it carries version numbers, so it never reaches the CEO's
    /// screen (the rule `SetupStatus::engine_pin_version` already states).
    pub fn reason(&self) -> String {
        match self {
            EngineRejected::NotEngineShaped => {
                "nothing that looks like the engine is there".to_string()
            }
            EngineRejected::WrongRelease { found, needed } => format!(
                "the engine there is {}, and this build boots engine {needed}",
                found.as_deref().unwrap_or("of no readable version"),
            ),
            // IT NAMES THE VERSION AGREEMENT FIRST, because that is the trap: an operator who
            // reads only "wrong engine" about a directory whose VERSION matches goes looking
            // for a version fault that is not there. The sentence has to say that the label is
            // right and the contents are not.
            EngineRejected::WrongIdentity { installed: Some(installed), pinned } => format!(
                "the engine there carries the right version and different contents — installed \
                 from {}, and this build pins {}",
                short_digest(installed),
                short_digest(pinned),
            ),
            EngineRejected::WrongIdentity { installed: None, pinned } => format!(
                "the engine there has no record of what installed it, so its contents are \
                 unknown, and this build pins {}",
                short_digest(pinned),
            ),
        }
    }
}

/// The engine release this build boots, or `None` when this build carries no pin.
///
/// One value, read from the pin compiled in at build time ([`engine_pin`]). **A build that
/// names no engine DEMANDS no engine**: the gate below is inert, every candidate is judged on
/// shape alone, and the boot line says so rather than leaving it to be inferred. That is the
/// honest state of every `cargo run` and every `cargo test` in this repository — `option_env!`
/// is a compile-time read and nothing in a plain `cargo` invocation sets the three variables.
/// Making it strict instead would be a rule invented here rather than one the build stated.
pub fn required_engine_version() -> Option<String> {
    engine_pin().map(|p| p.version)
}

/// **THE PIN GATE — shape AND release, one definition, `needed` injected.**
///
/// Separated from [`engine_matches_build`] for exactly the reason [`pin_from_parts`] is
/// separated from [`engine_pin`]: `option_env!` is a COMPILE-time read, so a test binary can
/// never carry a pin, and a gate that could only be exercised by rebuilding the crate with an
/// environment set is a gate nothing would ever test.
///
/// `needed: None` means this build pins nothing, and the release question is then not asked.
pub fn engine_accepted(dir: &Path, needed: Option<&str>) -> Result<(), EngineRejected> {
    if !engine_looks_valid(dir) {
        return Err(EngineRejected::NotEngineShaped);
    }
    let Some(needed) = needed else { return Ok(()) };
    let found = engine_version(dir);
    match found.as_deref() {
        Some(v) if v == needed => Ok(()),
        _ => Err(EngineRejected::WrongRelease { found, needed: needed.to_string() }),
    }
}

/// [`engine_accepted`] asked with the running build's own pin. The production entry point, and
/// what `engine.rs::looks_like_engine` delegates to, so the resolver, the detector and the
/// installer cannot disagree about which engine this copy of RichOS boots.
pub fn engine_matches_build(dir: &Path) -> Result<(), EngineRejected> {
    engine_accepted(dir, required_engine_version().as_deref())
}

// ===========================================================================================
// THE ENGINE'S IDENTITY — the CONTENT, not the label on it
// ===========================================================================================
//
// [`engine_accepted`] above compares `VERSION` strings, and until 2026-09-18 that was the whole
// of the gate. It is not enough, and the way it is not enough was MEASURED rather than reasoned
// about.
//
// EVERY NIGHTLY PUBLISHES ITS OWN `richos-engine-1.2.0.tar.gz`. The version inside it is
// `1.2.0` for all of them, because `1.2.0` is the engine's RELEASE and a nightly is not a new
// release of the engine — it is a new CUT of it. So two engines a day apart answer the
// identical question identically:
//
//   - installed under the walk's HOME, from `v1.2.0-nightly.20260917.2` (its own
//     `INSTALLED-FROM`): sha256 `b7a882ef4381ca294259c1a01a6af29acc3159dc0b871e8064bd7f410b71da07`,
//     119,463,136 B;
//   - pinned by candidate .8's binary (`strings` on `Contents/MacOS/richos-tauri`): sha256
//     `ea7f79043e7dc8f51b5f4207964ec5fa3e15ca11b44e5342a5fb4d38580db194`, from
//     `v1.2.0-nightly.20260918.2`. That binary contains the pinned digest once and the
//     installed digest ZERO times;
//   - `VERSION` in both: `1.2.0`, so `engine_accepted` returned `Ok(())` and the app booted
//     the older directory.
//
// The consequence, on Ray's walk of candidate .8: the first background job failed 6.28 s after
// registration with *"the selected engine cannot hold a separate seat for background work"*
// (`ecs.rs:328-333`). The installed `ecs/adapters/app.py` contains **no occurrence of the
// substring `seat`, in any case, anywhere beneath `ecs/`** — the seat work landed on main on
// 2026-09-17 and the app was running the cut from the night before. `supports_work_seats`
// (`ecs.rs:295`) is a POSITIVE probe and answered correctly; the fault was never in the probe.
// A user who updates from one 1.2.0 nightly to the next otherwise keeps yesterday's engine
// forever.
//
// BOTH HALVES OF THE COMPARISON WERE ALREADY ON THE MACHINE, and that is the part worth saying
// plainly. [`install_engine`] has written the installed digest into `INSTALLED-FROM` since it
// was first written, and [`engine_pin`] has carried the pinned digest compiled in. Nothing ever
// read the first one. What follows is a COMPARISON, not a new mechanism.
//
// WHY THE RULE IS THREE-VALUED AND NOT A BOOLEAN. A digest can only judge a directory whose
// provenance RichOS knows, and RichOS installs exactly one ([`engine_install_dir`]). A
// developer's checkout carries no stamp and is not stale — it is not an installation at all.
// Collapsing those two would make every `cargo run` in this repository unresolvable, which is a
// worse defect than the one being fixed, so the three cases are NAMED ([`EngineIdentity`])
// rather than merged.

/// The `INSTALLED-FROM` stamp, parsed.
///
/// Every field is optional because a stamp written by an older build, or truncated by a full
/// disk, is one we can still read PART of — and the part that decides (`sha256`) is then either
/// there or it is not. Parsed **by key** rather than by line position, so a field added later
/// cannot shift the meaning of the ones above it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledFrom {
    /// The `engine <version>` line.
    pub version: Option<String>,
    /// The `sha256 <digest>` line — **the identity**.
    pub sha256: Option<String>,
    /// The `bytes <n>` line.
    pub bytes: Option<u64>,
    /// The `from <url>` line.
    pub url: Option<String>,
}

/// Read and parse `<dir>/INSTALLED-FROM`. `None` when there is no stamp at all — the honest
/// answer for every directory RichOS did not install.
pub fn installed_from(dir: &Path) -> Option<InstalledFrom> {
    let text = std::fs::read_to_string(dir.join("INSTALLED-FROM")).ok()?;
    let mut out = InstalledFrom::default();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once(char::is_whitespace) else { continue };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key {
            "engine" => out.version = Some(value.to_string()),
            "sha256" => out.sha256 = Some(value.to_ascii_lowercase()),
            "bytes" => out.bytes = value.parse().ok(),
            "from" => out.url = Some(value.to_string()),
            _ => {}
        }
    }
    Some(out)
}

/// The first twelve hex characters of a digest — how an identity is SHOWN, never how it is
/// compared. Comparison is always over the full 64 ([`EngineIdentity::judge`]).
pub fn short_digest(sha256: &str) -> String {
    sha256.chars().take(12).collect()
}

/// **How much this build is entitled to demand of a directory's CONTENT.**
///
/// Two cases, and the line between them is **whether this build names an asset at all**:
///
/// - `Unasked` — this build pins no asset (`option_env!` is a compile-time read, so this is
///   every `cargo run` and every `cargo test` in this repository), or the caller is honoring an
///   operator's explicit `$RICHOS_ENGINE_DIR`. A statement outranks the pin here exactly as it
///   does for the release ([`engine_boot_refusal`]'s first silence). The dogfood checkout is
///   this case, which is why nothing about a developer's day changes.
/// - `Required` — this build NAMES an asset, so it boots the directory installed from that
///   asset and nothing else. A stamp is required; **no stamp means the contents are unknown, and
///   unknown is refreshed rather than trusted.**
///
/// # WHY A MISSING STAMP IS NOT FORGIVEN, EVEN OUTSIDE THE DIRECTORY RICHOS INSTALLS
///
/// The first draft of this gate forgave it — *"if RichOS stamped it, RichOS checks it"* — so a
/// directory with no `INSTALLED-FROM` passed on the grounds that RichOS had not installed it and
/// it therefore had no identity to be stale against. That is true, and it leaves a hole the same
/// size as the one being closed. MEASURED on candidate .8's walk: with the managed directory
/// moved aside, pid 66030 resolved its engine to `/Users/alex/.claude/richos-engine` — **the
/// developer's engine pointer, outside the scratch HOME entirely** — and booted it silently. A
/// shipped, signed, notarized build running the engine that happens to be checked out on the
/// machine that built it is a freshness hole wearing a plausible path.
///
/// So the pin is the discriminator, and it is exactly the right one: a build with a pin is a
/// build somebody cut and published, and it has been TOLD which engine it boots. A build without
/// one has been told nothing and demands nothing. The escape hatch for the developer who wants a
/// pinned build to run against a working tree is the one the rest of this module already honors
/// and documents — name it in `$RICHOS_ENGINE_DIR`, once, deliberately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineIdentity<'a> {
    /// No identity demanded.
    #[default]
    Unasked,
    /// A stamp is required, and must carry this digest.
    Required(&'a str),
}

impl<'a> EngineIdentity<'a> {
    /// The digest demanded, or `None` when none is.
    pub fn pinned(&self) -> Option<&'a str> {
        match self {
            EngineIdentity::Unasked => None,
            EngineIdentity::Required(s) => Some(s),
        }
    }

    /// Judge one directory. `Ok(())` accepts; the error carries the operator's sentence.
    ///
    /// **The comparison is over the full digest**, case-insensitively on both sides, and never
    /// over a truncation — a twelve-character compare would promote a display convenience into
    /// a security property.
    fn judge(&self, dir: &Path) -> Result<(), EngineRejected> {
        let Some(pinned) = self.pinned() else { return Ok(()) };
        match installed_from(dir).and_then(|s| s.sha256) {
            Some(found) if found.eq_ignore_ascii_case(pinned) => Ok(()),
            found => {
                Err(EngineRejected::WrongIdentity { installed: found, pinned: pinned.to_string() })
            }
        }
    }
}

/// **What this build demands of an engine directory** — the release, and the content.
///
/// One struct rather than two arguments because the two are asked TOGETHER at every call site,
/// and a call site that passed one and forgot the other would be the same class of hole this
/// type closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineDemand<'a> {
    /// The `VERSION` demanded, or `None` when this build pins no release.
    pub version: Option<&'a str>,
    /// The content demanded. See [`EngineIdentity`].
    pub identity: EngineIdentity<'a>,
}

impl<'a> EngineDemand<'a> {
    /// A release demand and nothing more — the behavior every caller had before 2026-09-18,
    /// available BY NAME so the places that legitimately ask only this say so out loud.
    pub fn release(version: Option<&'a str>) -> Self {
        EngineDemand { version, identity: EngineIdentity::Unasked }
    }

    /// **The whole demand a build with a pin makes**: that release, and that content.
    ///
    /// `None` — a build with no pin — demands nothing, which is [`EngineDemand::default`] and is
    /// the state of every `cargo` invocation in this repository.
    ///
    /// There is deliberately ONE constructor and not one per candidate kind. An earlier draft had
    /// two, a strict rule for the directory RichOS installs and a lenient one for everywhere
    /// else, and the lenient one was precisely what let pid 66030 boot the developer's
    /// `~/.claude/richos-engine` (see [`EngineIdentity`]). Two rules meant a hole at the seam
    /// between them; one rule has no seam.
    pub fn pinned(pin: Option<&'a EnginePin>) -> Self {
        match pin {
            None => EngineDemand::default(),
            Some(p) => EngineDemand {
                version: Some(&p.version),
                identity: EngineIdentity::Required(&p.sha256),
            },
        }
    }
}

/// [`engine_accepted`], asked about CONTENT as well as release.
///
/// Shape, then release, then identity, and the order is deliberate: the sentence an operator
/// reads should name the coarsest thing that is wrong. A directory that is not an engine at all
/// must not be reported as one carrying the wrong digest.
pub fn engine_accepted_demand(dir: &Path, demand: EngineDemand<'_>) -> Result<(), EngineRejected> {
    engine_accepted(dir, demand.version)?;
    demand.identity.judge(dir)
}

/// **The last gate before `claude` is started in a directory** — the half of this rule that
/// stops the wrong engine WRITING, as opposed to merely being resolved.
///
/// It exists because resolution failing is not the same as nothing running: `main.rs`'s
/// `resolve_engine` hands the lease factory THE LAST PLACE IT LOOKED when no candidate
/// answered, and on macOS that last place is `~/Library/Application Support/RichOS/engine` —
/// the one directory RichOS itself writes, and therefore exactly where a newer engine sits
/// after the app has been rolled back. Without this gate, a refused resolution still ends with
/// `claude` running in that engine and writing through it.
///
/// `Some(reason)` refuses; `None` allows. Two deliberate silences:
///
///   - **An explicit statement outranks the pin.** `$RICHOS_ENGINE_DIR` / `$RICHOS_ENGINE_ROOT`
///     are taken verbatim by the resolver (`locate-engine.sh` rule 1, `engine.rs` candidates 1
///     and 2), and a gate that then refused what the operator named would overrule a statement
///     the rest of the system honors. It is also the escape hatch that makes strictness safe on
///     a developer's machine: name the working tree and the pin steps aside, once, by name.
///   - **A directory that is not an engine at all is not this gate's question.**
///     `runtime::verify_engine` and `native.rs::preflight` already say that, in better words,
///     with the path in them. Answering it twice would put two different sentences about one
///     condition into one log.
pub fn engine_boot_refusal(dir: &Path, explicit: bool, needed: Option<&str>) -> Option<String> {
    engine_boot_refusal_demand(dir, explicit, EngineDemand::release(needed))
}

/// [`engine_boot_refusal`] asked about CONTENT as well as release — **the line that actually
/// stops the 2026-09-18 defect writing.**
///
/// Resolution refusing a stale directory is not the same as nothing running in it: `main.rs`'s
/// `resolve_engine` hands the lease factory THE LAST PLACE IT LOOKED when no candidate answered,
/// and on macOS that last place is `~/Library/Application Support/RichOS/engine` — precisely the
/// directory a stale nightly engine sits in. Without this, candidate .8 would still have started
/// `claude` in yesterday's cut; it would simply have said so first.
///
/// The two silences of [`engine_boot_refusal`] are unchanged and are unchanged for the same
/// reasons: an operator's explicit statement outranks the pin, and a directory that is not an
/// engine at all is a different function's sentence. A build with no pin demands nothing, so
/// `EngineDemand::default()` refuses nothing.
pub fn engine_boot_refusal_demand(
    dir: &Path,
    explicit: bool,
    demand: EngineDemand<'_>,
) -> Option<String> {
    if explicit {
        return None;
    }
    // A build that pins no release pins no content either — `engine_pin` is all-or-nothing
    // (`pin_from_parts` returns `None` unless all three parts are present and well formed), so
    // there is no state in which content is demanded and a version is not.
    demand.version?;
    match engine_accepted_demand(dir, demand) {
        Ok(()) | Err(EngineRejected::NotEngineShaped) => None,
        Err(rejected) => Some(format!("{}: {}", dir.display(), rejected.reason())),
    }
}

/// `~/Library/Application Support/RichOS` — the per-user directory `provision.rs` already
/// uses for the corpus pointer and `engine.rs` names as candidate 7.
pub fn app_support_richos(home: &Path) -> PathBuf {
    home.join("Library").join("Application Support").join("RichOS")
}

/// **Where a fetched engine is installed**: `~/Library/Application Support/RichOS/engine`.
///
/// This is `engine.rs`'s candidate 7 verbatim — the slot it describes as *"the known per-user
/// location an installer could populate on a customer's Mac"* and, until now, *"Nothing puts
/// one there today either."* Choosing any other path would have needed a change to the
/// resolver; choosing this one needs none, which is the point.
pub fn engine_install_dir(home: &Path) -> PathBuf {
    app_support_richos(home).join("engine")
}

// ===========================================================================================
// DETECTION
// ===========================================================================================

/// One of the things a customer's Mac may be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Component {
    /// Anthropic's `claude` binary. RichOS drives it directly (`native.rs`).
    ClaudeCode,
    /// The engine directory RichOS hands `claude` as its working directory.
    Engine,
    /// **The video tools, an essential since 2026-10-07** (media-tools plan, richos-hq
    /// `docs/plans/2026-10-07-media-tools.md` §2 and §6 item 3). The CEO: *"These tools need to
    /// be classed as necessary essentials."* Present means BOTH: the app's own yt-dlp
    /// ([`crate::media_tools::installed`]) and the pinned speech models (the [`SpeechModel`]
    /// answer): the one voice resolves by speed AND the transcription model, both installed and
    /// verified (the CEO, 2026-10-07, asked whether setup should download both: "Both, in this
    /// nightly, yes."). ffmpeg, ffprobe and whisper-cli need no check of their own: they
    /// are runtime files, and an engine whose runtime does not verify is not an engine
    /// ([`engine_is_usable`]). Installed AFTER the engine, because yt-dlp runs on the engine's
    /// Python and the model fetch is offered only once the engine's `whisper-cli` resolves.
    MediaTools,
}

/// **His name for the video tools step**, from round 19 (the CEO, 2026-10-08: *"I'm already seeing
/// it as "My voice and video tools" there. So, yes, keep that."*). The sheet capitalizes it.
pub const MEDIA_TOOLS_NAME: &str = "my voice and video tools";

/// **His line for that step, verbatim**: the CEO's setup sentence (2026-10-08) after its colon,
/// *"My voice and video tools: the tools I use to watch and download videos for you. Plus, it gives
/// you a free & private/local replacement for Wispr Flow. So, it saves you $140+/year👍 and allows
/// you to talk instead of typing anywhere on this computer."* It carries a slash and a dollar sign,
/// so it is the one consent line exempt from the sheet's no-path, no-dollar floor, and the
/// exemption is declared where that floor is checked (`ui/tests/setup.js` case 2), compared
/// against this constant so a changed word is not exempt.
///
/// **It no longer mentions the refreshes.** The line it replaces ended "which I keep up to date
/// myself", and that clause was the one ask for yt-dlp's silent refreshes (media-tools plan Q1,
/// option A). His line drops it, and his text is used exactly as he wrote it. The refreshes are
/// unchanged (`media_tools.rs` still keeps yt-dlp current by itself, as Q1 decided); the sheet
/// just no longer says so.
pub const MEDIA_TOOLS_WHY: &str = "the tools I use to watch and download videos for you. Plus, it gives you a free & private/local replacement for Wispr Flow. So, it saves you $140+/year👍 and allows you to talk instead of typing anywhere on this computer.";

impl Component {
    /// The name the CEO sees. Plain text, no mark — the license's fourth condition. The video
    /// tools take his name, [`MEDIA_TOOLS_NAME`], only once dictation is there to stand behind it
    /// ([`crate::dictation_ready`]).
    pub fn display_name(self) -> &'static str {
        self.display_name_for(crate::dictation_ready())
    }

    /// [`Self::display_name`] with the dictation gate stated, so both names are unit tests.
    pub fn display_name_for(self, dictation: bool) -> &'static str {
        match self {
            Component::ClaudeCode => "Claude Code",
            Component::Engine => "the RichOS engine",
            Component::MediaTools if dictation => MEDIA_TOOLS_NAME,
            Component::MediaTools => "my video tools",
        }
    }

    /// **What it is for, in his language.** This is the consent copy: he is told what is about to
    /// be installed and why, once. No path and no version number, except his own line for the
    /// video tools ([`MEDIA_TOOLS_WHY`]), shown once dictation is there ([`crate::dictation_ready`]):
    /// it promises talking instead of typing anywhere, so it must not reach a nightly before that
    /// is true.
    pub fn why(self) -> &'static str {
        self.why_for(crate::dictation_ready())
    }

    /// [`Self::why`] with the dictation gate stated.
    pub fn why_for(self, dictation: bool) -> &'static str {
        match self {
            Component::ClaudeCode => {
                "the program I think with. It comes from Anthropic and installs itself; \
                 I only ask it to."
            }
            Component::Engine => {
                "the part of me that knows how I work: my instructions and my team."
            }
            Component::MediaTools if dictation => MEDIA_TOOLS_WHY,
            // The media-tools plan's consent line, verbatim (§2), until dictation is there. It is
            // the one ask for yt-dlp's silent refreshes afterwards (plan Q1, option A); his line
            // above drops that clause.
            Component::MediaTools => {
                "the tools I use to watch, hear and download videos for you, which I keep up to \
                 date myself."
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Component::ClaudeCode => "claude-code",
            Component::Engine => "engine",
            Component::MediaTools => "media-tools",
        }
    }
}

/// **Does this machine have its pinned speech models?** `Ok` naming them, or `Err` with the reason
/// it has not.
///
/// A seam rather than a call, for two reasons. `richos-core` does not depend on `richos-voice`
/// (that crate depends on this one), and the answer the product uses is
/// `richos_voice::stt::readiness()` being `Ready` — the SAME resolution voice mode and the model
/// download use, so setup can never call present a model the microphone path would not find —
/// and the transcription model verified beside it (`setup_view.rs` passes it). And a test can then say "the model is there" or "it is not"
/// without 574 MB of weights on disk, as [`EngineUsable`] does for a 322 MB runtime.
pub type SpeechModel<'a> = &'a dyn Fn() -> Result<String, String>;

/// Find the video tools, or report every place that was looked: the yt-dlp RichOS installs, and
/// the speech models. Both are named either way, so a boot line that says they are missing says
/// WHICH half is missing.
pub fn find_media_tools(paths: &SetupPaths, speech: SpeechModel<'_>) -> ComponentStatus {
    let Some(home) = paths.home.as_deref() else {
        // No home, no tools folder: `missing` turns the empty list into the "nowhere" line.
        return ComponentStatus::missing(Component::MediaTools, Vec::new());
    };
    let tools = crate::media_tools::tools_dir(home);
    let yt_dlp = crate::media_tools::installed(&tools);
    let model = speech();
    let mut looked = vec![match &yt_dlp {
        Some(i) => format!("{} — yt-dlp nightly {} is installed and matches its recorded hash", tools.join("yt-dlp").display(), i.tag),
        None => format!("{} — no yt-dlp that RichOS installed and can verify", tools.join("yt-dlp").display()),
    }];
    looked.push(match &model {
        Ok(models) => format!("the speech models — {models}: installed and verified"),
        Err(why) => format!("the speech models — {why}"),
    });
    match (yt_dlp, model) {
        (Some(i), Ok(models)) => ComponentStatus::found(
            Component::MediaTools,
            tools,
            Some(format!("yt-dlp nightly {}, speech models {models}", i.tag)),
        ),
        _ => ComponentStatus::missing(Component::MediaTools, looked),
    }
}

/// Present, with the path that answered — or missing, with every place that was looked.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentStatus {
    pub component: Component,
    pub present: bool,
    /// Where it was found. `None` when missing.
    pub at: Option<String>,
    /// A version or a signature note, when one is cheap and true.
    pub detail: Option<String>,
    /// Every candidate, in order, when missing — so a failure names the places instead of
    /// saying "not found" and leaving the operator to guess (`engine.rs`, same rule).
    pub looked_in: Vec<String>,
}

impl ComponentStatus {
    fn found(component: Component, at: PathBuf, detail: Option<String>) -> Self {
        ComponentStatus {
            component,
            present: true,
            at: Some(at.display().to_string()),
            detail,
            looked_in: Vec::new(),
        }
    }
    /// **A MISSING COMPONENT ALWAYS NAMES SOMEWHERE.** The invariant is enforced here, in
    /// the one constructor, rather than asked of every caller — because the caller that
    /// forgot is exactly how the nightly's D1 reached a screen: `main.rs` prints
    /// `looked in: {}` with this list joined, and an empty list rendered as a boot line
    /// that trailed off after the colon. There is no honest state in which RichOS reports
    /// something missing and can name no place it looked, so if a launch really is that
    /// impoverished — no `$HOME`, no executable path — it says THAT instead of nothing.
    fn missing(component: Component, mut looked_in: Vec<String>) -> Self {
        if looked_in.is_empty() {
            looked_in.push(format!(
                "nowhere — this launch could not name a single place to check for {}",
                component.display_name()
            ));
        }
        ComponentStatus { component, present: false, at: None, detail: None, looked_in }
    }
}

/// Everything first run needs to decide whether to ask, and what to say.
#[derive(Debug, Clone, Serialize)]
pub struct SetupStatus {
    pub claude: ComponentStatus,
    pub engine: ComponentStatus,
    /// The video tools ([`Component::MediaTools`]).
    pub media_tools: ComponentStatus,
    /// `true` when this build carries an engine pin. `false` means the engine cannot be
    /// installed by this copy of RichOS and the surface must say so instead of offering a
    /// button that will fail — [`SetupError::EngineUnpinned`].
    pub engine_installable: bool,
    /// The engine release this build is pinned to, for the operator's line. Never shown to
    /// the CEO: no version numbers on his screen.
    pub engine_pin_version: Option<String>,
    /// Set by `run_setup` on the status it returns, so the window can say "done" rather than
    /// re-asking. Mirrors `MemoryStatus::provisioned_now`.
    pub installed_now: bool,
}

impl SetupStatus {
    /// What is missing, in the order it must be installed. **Claude Code first**: the engine
    /// is the working directory a `claude` process is given, so an engine with no binary to
    /// run in it is the less useful half-state of the two. **The video tools last**: yt-dlp
    /// runs on the engine's Python and the speech model is fetched only once the engine's
    /// `whisper-cli` resolves (plan §2).
    pub fn needs(&self) -> Vec<Component> {
        let mut out = Vec::new();
        if !self.claude.present {
            out.push(Component::ClaudeCode);
        }
        if !self.engine.present {
            out.push(Component::Engine);
        }
        if !self.media_tools.present {
            out.push(Component::MediaTools);
        }
        out
    }

    /// Nothing missing — the state in which first run says nothing at all.
    pub fn complete(&self) -> bool {
        self.needs().is_empty()
    }

    /// Missing something this build cannot fix. The surface must then explain rather than
    /// offer, the way `memory.rs`'s `no-compiler` state does.
    pub fn blocked(&self) -> bool {
        !self.engine.present && !self.engine_installable
    }
}

/// Find the `claude` binary, or report every place that was looked.
///
/// The order **is** `native.rs::resolve_claude_bin`'s, with one addition: that function's last
/// resort is the bare name `claude`, whose absence only surfaces at spawn time as
/// `BinaryMissing`. Setup cannot wait for a spawn, so the bare name is resolved here against
/// `$PATH` — the same answer, reached before a process instead of after one.
pub fn find_claude(paths: &SetupPaths) -> ComponentStatus {
    let mut looked = Vec::new();

    if let Some(explicit) = paths.claude_bin_override.as_deref() {
        looked.push(format!("{} ($RICHOS_CLAUDE_BIN)", explicit.display()));
        if explicit.is_file() {
            return ComponentStatus::found(
                Component::ClaudeCode,
                explicit.to_path_buf(),
                claude_detail(explicit),
            );
        }
        // EXCLUSIVE, exactly as `engine.rs` treats an explicit engine: an operator who named a
        // path is making a statement, and falling through to one nobody named would silently
        // overrule it. A wrong explicit value is reported, not routed around.
        return ComponentStatus::missing(Component::ClaudeCode, looked);
    }

    if let Some(home) = paths.home.as_deref() {
        let launcher = home.join(".local/bin/claude");
        looked.push(launcher.display().to_string());
        if launcher.exists() {
            return ComponentStatus::found(
                Component::ClaudeCode,
                launcher.clone(),
                claude_detail(&launcher),
            );
        }
    }

    for dir in paths.path_var.as_deref().unwrap_or("").split(':').filter(|d| !d.is_empty()) {
        let candidate = Path::new(dir).join("claude");
        looked.push(candidate.display().to_string());
        if candidate.is_file() {
            return ComponentStatus::found(
                Component::ClaudeCode,
                candidate.clone(),
                claude_detail(&candidate),
            );
        }
    }

    ComponentStatus::missing(Component::ClaudeCode, looked)
}

/// A cheap, true note about a `claude` that is present: the version directory its launcher
/// points at. **Never runs the binary** — `claude --version` is a process spawn on a boot
/// path, and the symlink target already carries the version Anthropic's installer encodes in
/// it (`~/.local/share/claude/versions/2.1.257`, measured).
fn claude_detail(bin: &Path) -> Option<String> {
    let target = std::fs::read_link(bin).ok()?;
    let name = target.file_name()?.to_string_lossy().to_string();
    Some(format!("installed at {name}"))
}

/// Is this engine directory not merely engine-SHAPED, but actually usable?
///
/// `Ok(())`, or the operator-facing reason it cannot be used. The production implementation
/// is [`engine_is_usable`]; tests inject their own so the decision can be exercised without
/// a 322 MB delivered runtime on disk.
pub type EngineUsable<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

/// The production usability test: the component contracts and the delivered runtime
/// inventory, which is exactly what a lease needs before it can start
/// (`EngineLeaseFactory::create` verifies the same thing before spawning `claude`).
///
/// **Cheap in the case that matters.** Its first act is to canonicalize
/// `<engine>/runtime`, so a directory with no delivered runtime — a plugin checkout, a
/// source tree — is rejected in microseconds and never pays for hashing. Only a candidate
/// that really carries a runtime is hashed, and there is at most one of those on a machine
/// (measured 2026-09-17: 1.70 s cold / 0.81 s warm over 322 MB in 6,530 files, release
/// build, on the engine `run_setup` installed during the first nightly walk).
pub fn engine_is_usable(dir: &Path) -> Result<(), String> {
    crate::runtime::verify_engine(dir).map(|_| ()).map_err(|e| e.to_string())
}

/// Find the engine directory, or report every place that was looked.
///
/// **Deliberately a subset of `engine.rs::resolve_engine_dir`, in the same order**, covering
/// the candidates a shipped install can reach: the explicit override, the bundle's own
/// resources, the engine installer's pointer, and the application-support directory this
/// module writes. The repo-ancestor walks (candidates 4 and 5) are the dogfood layout; setup
/// includes them through `extra` so a developer running from the repo is never asked to
/// install something already three directories away.
///
/// # USABILITY IS PART OF SELECTION, NOT A VETO APPLIED AFTERWARDS
///
/// Until 2026-09-17 this function stopped at the first candidate that merely LOOKED like an
/// engine (`scripts/hooks/` + `VERSION`), and its caller then ran the real verification and,
/// on failure, flipped `present` to `false`. Two things were wrong with that, and together
/// they are the nightly's D1:
///
///   1. **The rejected candidate reported NOWHERE looked.** [`ComponentStatus::found`]
///      clears `looked_in`, so flipping `present` afterwards produced a "NOT installed"
///      whose list of places tried was EMPTY — printed verbatim at `main.rs`'s boot line as
///      `looked in: ` with nothing after the colon. A detector that names no candidates
///      reads as one that cannot find anything, ever.
///   2. **The walk stopped at the unusable one.** On any machine that also has the engine
///      installed as a Claude Code plugin, `~/.claude/richos-engine` answers first, carries
///      no delivered runtime, and therefore hid the engine RichOS had itself installed at
///      `~/Library/Application Support/RichOS/engine`. Setup completed, and the next launch
///      asked for setup again — on four consecutive launches
///      (`docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md` §D1).
///
/// So a candidate that looks like an engine but is not usable is now RECORDED WITH ITS
/// REASON and the walk CONTINUES. The first candidate that is actually usable wins, which
/// is the question the caller was really asking all along.
///
/// **The explicit override stays exclusive**, per `locate-engine.sh` rule 1 and
/// `engine.rs`'s header: an operator who named a directory is making a statement, and
/// falling through to one nobody named would silently overrule it. What changes is that
/// being wrong is now REPORTED — with the reason — instead of reported as an empty list.
///
/// # THE RELEASE IS PART OF SELECTION TOO — spec point 22
///
/// A searched candidate must also be the release this build PINS. A rollback to an older app
/// otherwise finds the newer engine a later app installed, boots it, and lets it write into
/// the corpus — silently, because every other thing about it is right. So a wrong-release
/// engine is recorded with the release it carries and the release this build needs, and the
/// walk continues, ending — when nothing matches — in the same offer to install the pinned one
/// that a missing engine produces.
///
/// The explicit override is exempt, in both directions: it is judged on shape and usability as
/// it always was, and never on release. See [`engine_boot_refusal`] for why an operator's
/// statement outranks the pin everywhere.
pub fn find_engine(paths: &SetupPaths, extra: &[PathBuf], usable: EngineUsable<'_>) -> ComponentStatus {
    find_engine_pinned(paths, extra, usable, required_engine_version().as_deref())
}

/// [`find_engine`] with the pinned release supplied rather than compiled in — the same seam,
/// and for the same reason, as [`engine_accepted`] against [`engine_matches_build`].
pub fn find_engine_pinned(
    paths: &SetupPaths,
    extra: &[PathBuf],
    usable: EngineUsable<'_>,
    needed: Option<&str>,
) -> ComponentStatus {
    find_engine_demanded(paths, extra, usable, EngineDemand::release(needed))
}

/// [`find_engine_pinned`] asked about CONTENT as well as release — **the detection half of the
/// 2026-09-18 fix.**
///
/// The candidate order, the one-line-per-place reporting and the explicit override's exclusivity
/// are all unchanged. What changes is that a SEARCHED candidate must now also have been installed
/// from the asset this build pins ([`EngineDemand::pinned`]), so the stale-by-content directory
/// that ended Ray's walk of candidate .8 is reported here, at the boot line, with the two digests
/// in the sentence — instead of 6.28 s into the CEO's first background job.
///
/// The explicit override remains exempt from both halves, for the reason [`engine_boot_refusal`]
/// gives: an operator's statement is honored, a guess is not.
pub fn find_engine_demanded(
    paths: &SetupPaths,
    extra: &[PathBuf],
    usable: EngineUsable<'_>,
    demand: EngineDemand<'_>,
) -> ComponentStatus {
    let mut looked = Vec::new();

    // Test one candidate, appending exactly one line to `looked` whichever way it goes.
    // `Some(status)` means stop here; `None` means keep walking. `judged` is
    // `EngineDemand::default()` — demand nothing — for the explicit override alone.
    let consider = |looked: &mut Vec<String>,
                    candidate: &Path,
                    label: &str,
                    judged: EngineDemand<'_>| {
        if !engine_looks_valid(candidate) {
            looked.push(format!("{}{label} — nothing that looks like the engine is there", candidate.display()));
            return None;
        }
        if let Err(rejected) = engine_accepted_demand(candidate, judged) {
            // FOUND, AND REJECTED FOR ITS RELEASE OR ITS CONTENTS, AND SAID SO — with what is
            // there and what this build boots, because "the engine is not installed" about a
            // directory holding an engine is what sends an operator looking for the wrong
            // fault. `WrongIdentity` is the sentence that would have ended candidate .8's walk
            // at the boot log instead of 6.28 s into a background job.
            looked.push(format!("{}{label} — {}", candidate.display(), rejected.reason()));
            return None;
        }
        if let Err(why) = usable(candidate) {
            // FOUND, AND REJECTED, AND SAID SO. This is the line that was missing: the
            // engine is on disk at a path we can name, and the reason it cannot be used is
            // the reason the lease factory would give at the first send.
            looked.push(format!("{}{label} — the engine is there but cannot be used: {why}", candidate.display()));
            return None;
        }
        let v = engine_version(candidate).map(|v| format!("version {v}"));
        Some(ComponentStatus::found(Component::Engine, candidate.to_path_buf(), v))
    };

    if let Some(explicit) = paths.engine_override.as_deref() {
        // DEMAND NOTHING OF WHAT AN OPERATOR NAMED — neither release nor content.
        if let Some(found) =
            consider(&mut looked, explicit, " ($RICHOS_ENGINE_DIR)", EngineDemand::default())
        {
            return found;
        }
        return ComponentStatus::missing(Component::Engine, looked);
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(exe) = paths.exe.as_deref() {
        if let Some(contents) = exe.parent().and_then(|p| p.parent()) {
            candidates.push(contents.join("Resources/engine"));
        }
    }
    candidates.extend(extra.iter().cloned());
    let config_dir =
        paths.config_dir.clone().or_else(|| paths.home.as_ref().map(|h| h.join(".claude")));
    if let Some(cfg) = config_dir {
        candidates.push(cfg.join("richos-engine"));
    }
    // The one directory RichOS installs, and the only one any of these candidates is ever
    // STAMPED in practice — judged by the same rule as the rest, because a rule that applied
    // only here is what left the developer-pointer hole open (see [`EngineIdentity`]).
    if let Some(home) = paths.home.as_deref() {
        candidates.push(engine_install_dir(home));
    }

    // ONE LINE PER PLACE, not one per candidate SLOT. The boot engine arrives in `extra` and
    // is very often the same directory as the install pointer — on the machine the nightly
    // ran on they are the same path — so without this the operator's log reported the same
    // directory twice with the same rejection reason. Order is preserved; only repeats are
    // dropped.
    let mut seen: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if seen.contains(&candidate) {
            continue;
        }
        seen.push(candidate.clone());
        if let Some(found) = consider(&mut looked, &candidate, "", demand) {
            return found;
        }
    }

    ComponentStatus::missing(Component::Engine, looked)
}

/// The whole first-run question, answered from disk.
///
/// `usable` is how "is this engine real?" is decided — [`engine_is_usable`] in the product,
/// an injected stub in tests. It is a parameter rather than a hard-wired call so that the
/// decision and its reporting can be exercised without a delivered runtime, and so that
/// there is exactly one place the answer comes from (the caller no longer re-checks it
/// afterwards; see [`find_engine`]).
///
/// `speech` answers the speech-model half of [`Component::MediaTools`] ([`SpeechModel`]).
pub fn detect(
    paths: &SetupPaths,
    extra_engine_candidates: &[PathBuf],
    usable: EngineUsable<'_>,
    speech: SpeechModel<'_>,
) -> SetupStatus {
    detect_with_pin(paths, extra_engine_candidates, usable, engine_pin().as_ref(), speech)
}

/// [`detect`] with the pin supplied rather than compiled in.
///
/// **One pin answers both halves.** The release the walk demands of a candidate and the
/// release the first-run surface offers to install are the same value here, by construction —
/// a build that refused every engine on the machine and then offered a DIFFERENT one would be
/// a worse state than the one this rule exists to end.
pub fn detect_with_pin(
    paths: &SetupPaths,
    extra_engine_candidates: &[PathBuf],
    usable: EngineUsable<'_>,
    pin: Option<&EnginePin>,
    speech: SpeechModel<'_>,
) -> SetupStatus {
    SetupStatus {
        claude: find_claude(paths),
        // **ONE PIN ANSWERS BOTH HALVES, AND NOW BOTH HALVES OF THE PIN ARE ASKED.** Until
        // 2026-09-18 this passed `p.version` and dropped `p.sha256` on the floor — the pin has
        // carried the digest since it was written, and detection simply never looked at it.
        engine: find_engine_demanded(paths, extra_engine_candidates, usable, EngineDemand::pinned(pin)),
        media_tools: find_media_tools(paths, speech),
        engine_installable: pin.is_some(),
        engine_pin_version: pin.map(|p| p.version.clone()),
        installed_now: false,
    }
}

// ===========================================================================================
// THE PIN — an engine release, named at build time, verified at install time
// ===========================================================================================

/// The engine release this build will install, and the digest it must have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnginePin {
    /// The engine's `VERSION`, used to name the asset and to check what was extracted.
    pub version: String,
    /// The full download URL, in the public repository's Releases.
    pub url: String,
    /// The lowercase hex SHA-256 the downloaded bytes must have.
    pub sha256: String,
}

/// The pin compiled into this binary, or `None`.
///
/// `option_env!` — read at **compile** time, so the value is inside the executable the
/// Developer ID signature covers. A runtime environment variable would be a value an attacker
/// on the machine could set, which is the opposite of a pin.
///
/// **`None` is a refusal, never a fallback.** A build with no pin reports
/// `engine_installable: false` and [`install_engine`] returns [`SetupError::EngineUnpinned`].
/// There is no branch that fetches "latest" because nobody said.
pub fn engine_pin() -> Option<EnginePin> {
    let version = option_env!("RICHOS_ENGINE_VERSION")?.trim();
    let url = option_env!("RICHOS_ENGINE_URL")?.trim();
    let sha256 = option_env!("RICHOS_ENGINE_SHA256")?.trim();
    pin_from_parts(version, url, sha256)
}

/// The pin's validation, separated so it can be tested without a build-time environment.
///
/// Four rules, and each one is a way a bad release could otherwise reach a customer:
/// non-empty everywhere; a 64-character lowercase hex digest; and an **https** URL, because a
/// plain-http asset would make the digest the only guard against a network that can also see
/// which digest is being requested.
pub fn pin_from_parts(version: &str, url: &str, sha256: &str) -> Option<EnginePin> {
    if version.is_empty() || url.is_empty() {
        return None;
    }
    if !url.starts_with("https://") {
        return None;
    }
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(EnginePin {
        version: version.to_string(),
        url: url.to_string(),
        sha256: sha256.to_string(),
    })
}

/// Anthropic's own installer. **Not a mirror, not a copy** — the URL their documentation
/// gives, resolved 2026-09-01 to `https://downloads.claude.ai/claude-code-releases/bootstrap.sh`
/// (HTTP 302 -> 200, 9,704 B). Driving it is the act the license permits; redistributing what
/// it downloads is the act nobody grants, and RichOS never performs it.
pub const CLAUDE_INSTALLER_URL: &str = "https://claude.ai/install.sh";

/// The designated requirement `claude` must satisfy, verbatim from
/// `codesign -d -r-` on the installed binary (2026-09-01, version 2.1.257).
///
/// Identifier **and** team, anchored to Apple's root: an unrelated binary renamed `claude`
/// fails it, and so does one signed by anybody but Anthropic PBC.
pub const CLAUDE_DESIGNATED_REQUIREMENT: &str =
    "identifier \"com.anthropic.claude-code\" and anchor apple generic and certificate leaf[subject.OU] = \"Q6L2SF6YDW\"";

/// The requirement as `codesign -R` wants it — **with a leading `=`**.
///
/// MEASURED, and it is a trap worth naming: `codesign -R <text>` treats its argument as a
/// PATH to a requirement file. Handed the requirement itself it reports
///
/// ```text
/// <requirement>: No such file or directory
/// invalid requirement specification
/// ```
///
/// and exits non-zero — which looks exactly like "this binary failed verification". The first
/// version of this module made that mistake and the test suite caught it against the real
/// `claude` on this machine: a **false rejection**, the mirror image of the stapler symlink's
/// false pass. A leading `=` makes `codesign` read the text. With it, the real binary returns
/// exit 0 offline.
///
/// The constant above stays the exact string `codesign -d -r-` prints, so it can be diffed
/// against a future measurement without an argument-syntax character in the way.
pub fn codesign_requirement_arg() -> String {
    format!("={CLAUDE_DESIGNATED_REQUIREMENT}")
}

// ===========================================================================================
// FAILURES — every one named, none silent
// ===========================================================================================

/// **Every way setup can fail, and each one says what to do about it.**
///
/// There is no `Other(String)`. A failure this enum cannot name is a failure the customer
/// would be shown as a shrug, and the house lesson is that a shrug and a success look the same
/// from the outside.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("I couldn't work out where your home folder is, so I have nowhere to install to. This is a problem with how RichOS was launched, not with your Mac.")]
    NoHome,

    #[error("I couldn't reach the internet, so there's nothing to download yet. Connect and try again. Nothing has been changed on your Mac.")]
    NoNetwork { url: String, detail: String },

    #[error("The download didn't arrive ({url} answered {status}). Nothing has been changed on your Mac.")]
    DownloadFailed { url: String, status: String },

    #[error("The download stopped partway through: {got} bytes of an expected {expected}. Nothing has been installed; try again.")]
    DownloadIncomplete { url: String, expected: u64, got: u64 },

    #[error("What downloaded isn't what this copy of RichOS expects, so I stopped and installed nothing. (expected {expected}, got {got}, from {url})")]
    DigestMismatch { url: String, expected: String, got: String },

    /// **NAMES THE PARTY, in the product's own words for him.** `affordances.js` holds a
    /// closed set of parties a state the CEO cannot fix is allowed to point at, and
    /// "whoever set RichOS up" is the one this product uses everywhere else. A sentence that
    /// invented a new name for the same person would leave him with a fault and no owner he
    /// recognizes.
    #[error("This copy of RichOS wasn't built with an engine to install, so I can't fetch one. It needs whoever set RichOS up to publish one and pin it.")]
    EngineUnpinned,

    #[error("The download opened, but what was inside it isn't a RichOS engine ({detail}). Nothing has been installed.")]
    EngineShapeInvalid { detail: String },

    #[error("The download is the wrong engine: it says version {found} and this copy of RichOS expects {expected}. Nothing has been installed.")]
    EngineVersionMismatch { expected: String, found: String },

    #[error("Anthropic's installer for Claude Code stopped with an error (exit {code}). It said: {stderr}")]
    InstallerRefused { code: i32, stderr: String },

    #[error("Anthropic's installer finished, but I still can't find Claude Code. Looked in: {looked_in}")]
    ClaudeStillMissing { looked_in: String },

    #[error("Claude Code installed, but macOS won't confirm it came from Anthropic, so I'm not going to run it. ({detail})")]
    SignatureRejected { path: String, detail: String },

    #[error("I couldn't finish installing {what} ({detail}). What was there before is still there and unchanged.")]
    InstallFailed { what: String, detail: String },

    /// **The speech model did not arrive.** The sentence is `richos_voice::provision`'s own,
    /// already written for the CEO (it says what failed and whether asking again resumes), so it
    /// is carried as it stands rather than wrapped in a second one that could disagree with it.
    #[error("{sentence}")]
    SpeechModelFailed { sentence: String },
}

impl SetupError {
    /// A short machine tag for the operator's log and the UI's test hooks. The CEO reads the
    /// `Display` sentence; nothing shows him this.
    pub fn kind(&self) -> &'static str {
        match self {
            SetupError::NoHome => "no-home",
            SetupError::NoNetwork { .. } => "no-network",
            SetupError::DownloadFailed { .. } => "download-failed",
            SetupError::DownloadIncomplete { .. } => "download-incomplete",
            SetupError::DigestMismatch { .. } => "digest-mismatch",
            SetupError::EngineUnpinned => "engine-unpinned",
            SetupError::EngineShapeInvalid { .. } => "engine-shape-invalid",
            SetupError::EngineVersionMismatch { .. } => "engine-version-mismatch",
            SetupError::InstallerRefused { .. } => "installer-refused",
            SetupError::ClaudeStillMissing { .. } => "claude-still-missing",
            SetupError::SignatureRejected { .. } => "signature-rejected",
            SetupError::InstallFailed { .. } => "install-failed",
            SetupError::SpeechModelFailed { .. } => "speech-model-failed",
        }
    }

    /// **Is the Mac unchanged?** Every variant above answers `true` except the ones reached
    /// after something was already written — the two after Anthropic's installer has run, and
    /// a speech model that stopped partway, whose partial download is KEPT so that pressing
    /// "Set it up" again resumes it (`richos_voice::provision`'s resume rules). Each names
    /// what exists. Nothing in this module has a state where the answer is unknown.
    pub fn machine_unchanged(&self) -> bool {
        !matches!(
            self,
            SetupError::ClaudeStillMissing { .. }
                | SetupError::SignatureRejected { .. }
                | SetupError::SpeechModelFailed { .. }
        )
    }
}

// ===========================================================================================
// THE NETWORK SEAM
// ===========================================================================================

/// Fetching one URL to one file. **A trait, so every failure path below is exercised by a
/// test that needs no network**: a refused connection, a 404, a truncated body and a tampered
/// body are four fakes, not four network conditions somebody has to arrange.
pub trait Fetcher: Send + Sync {
    /// Fetch `url` into `dest`, returning the byte count written.
    fn fetch(&self, url: &str, dest: &Path) -> Result<u64, SetupError>;
}

/// `/usr/bin/curl`, by absolute path.
///
/// Two reasons it is curl and not a Rust HTTP client: it is what **Anthropic's own installer**
/// uses, so the transport a customer's Claude Code arrives over is the same either way; and
/// `richos-core` stays free of a TLS stack, which is the property that keeps
/// `cargo test -p richos-core` fast (`app/README.md`).
///
/// `--proto '=https'` refuses a redirect to plain http — the pin's https rule enforced at the
/// wire as well as at the string.
pub struct CurlFetcher;

impl Fetcher for CurlFetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<u64, SetupError> {
        let out = Command::new("/usr/bin/curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--tlsv1.2",
                "--connect-timeout",
                "20",
                "--max-time",
                "900",
                "--output",
            ])
            .arg(dest)
            .arg(url)
            .output()
            .map_err(|e| SetupError::NoNetwork {
                url: url.to_string(),
                detail: format!("curl could not be run: {e}"),
            })?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            // curl's own exit codes, mapped so the sentence the CEO reads is the true one.
            // 6 = host not resolved, 7 = connect failed, 35 = TLS handshake — all "no
            // network" from where he sits. 18/28 = transfer stopped short. Everything else
            // is the server answering something we did not want.
            let code = out.status.code().unwrap_or(-1);
            let partial = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            let _ = std::fs::remove_file(dest);
            return Err(match code {
                6 | 7 | 35 => SetupError::NoNetwork { url: url.to_string(), detail: stderr },
                18 | 28 => SetupError::DownloadIncomplete {
                    url: url.to_string(),
                    expected: 0,
                    got: partial,
                },
                _ => SetupError::DownloadFailed {
                    url: url.to_string(),
                    status: if stderr.is_empty() { format!("curl exit {code}") } else { stderr },
                },
            });
        }

        let bytes = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        if bytes == 0 {
            let _ = std::fs::remove_file(dest);
            return Err(SetupError::DownloadFailed {
                url: url.to_string(),
                status: "empty response".to_string(),
            });
        }
        Ok(bytes)
    }
}

// ===========================================================================================
// DIGEST
// ===========================================================================================

/// SHA-256 of a file, lowercase hex, streamed in 64 KiB blocks so a 200 MB artifact never
/// becomes 200 MB of resident memory.
///
/// **Pure Rust, deliberately.** `shasum -a 256 | cut -d' ' -f1` is what the shell installers
/// use, and it is a subprocess whose exit code and stdout have to be parsed correctly for the
/// answer to mean anything — which is the exact shape of "exit 0 while doing the wrong thing".
/// An integrity check is the last place to accept that shape.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

// ===========================================================================================
// STAGING — a directory that cannot outlive a failure
// ===========================================================================================

/// A scratch directory removed on **every** exit path, including an unwind.
///
/// The alternative — remove it at each early return — is the version where the tenth early
/// return is added a month later and forgets. This one cannot forget.
/// **It is removed on success too.** An earlier version of this file "released" the staging
/// directory once the engine had been renamed out of it, on the reasoning that the rename had
/// consumed it. It had not: the downloaded archive and the emptied `unpacked/` were still in
/// there, and a successful install left `engine.incoming.<pid>.<nanos>` sitting next to
/// `engine` in the CEO's Application Support folder forever. The residue test caught it. There
/// is no release path now — the directory always goes.
pub(crate) struct Staging {
    dir: PathBuf,
}

impl Staging {
    pub(crate) fn new(near: &Path, tag: &str) -> Result<Staging, SetupError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = near.with_file_name(format!(
            "{}.{tag}.{}.{nanos}",
            near.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "richos".into()),
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).map_err(|e| SetupError::InstallFailed {
            what: "a working folder".to_string(),
            detail: format!("{}: {e}", dir.display()),
        })?;
        Ok(Staging { dir })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ===========================================================================================
// INSTALL — CLAUDE CODE
// ===========================================================================================

/// Running a command. A trait for the same reason [`Fetcher`] is one: "the installer refused"
/// is a test, not a network condition.
pub trait Runner: Send + Sync {
    /// Run `script` with `bash`, with `HOME` set to `home`, returning (exit code, stderr).
    fn run_installer(&self, script: &Path, home: &Path) -> Result<(i32, String), SetupError>;
}

/// `/bin/bash <script>` with a deliberately narrow environment.
pub struct BashRunner;

impl Runner for BashRunner {
    fn run_installer(&self, script: &Path, home: &Path) -> Result<(i32, String), SetupError> {
        // THE ENVIRONMENT IS BUILT, NOT INHERITED. A GUI launch has launchd's environment,
        // which is not the customer's shell environment, and passing an arbitrary inherited
        // one to somebody else's installer is how a surprise happens. HOME and a base PATH
        // are what the installer documents that it needs.
        let out = Command::new("/bin/bash")
            .arg(script)
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .output()
            .map_err(|e| SetupError::InstallFailed {
                what: "Claude Code".to_string(),
                detail: format!("could not start bash: {e}"),
            })?;
        let code = out.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Ok((code, stderr))
    }
}

/// What macOS said about the binary's signature.
#[derive(Debug, Clone, Serialize)]
pub struct SignatureVerdict {
    /// `true` only when `codesign --verify --strict -R` returned 0.
    pub trusted: bool,
    /// The file that was actually checked — the symlink's **target**, when it is one.
    pub checked: String,
    /// `codesign`'s own words, kept verbatim for the operator's line.
    pub detail: String,
}

/// Verify a `claude` binary against [`CLAUDE_DESIGNATED_REQUIREMENT`].
///
/// **Offline.** A designated-requirement check reads the embedded signature and the certificate
/// chain in it; it does not contact Apple. That is why it is the pin used here and stapling is
/// not — see the module header, where the stapler measurement is recorded in both directions,
/// including the symlink that exits 0 having validated nothing.
pub fn verify_claude_signature(bin: &Path) -> SignatureVerdict {
    // Resolve the symlink FIRST, so the verdict names the file it checked. Anthropic's
    // installer retargets `~/.local/bin/claude` on every self-update, so the name in a log
    // that does not resolve it can be a version that is no longer there.
    let checked = std::fs::canonicalize(bin).unwrap_or_else(|_| bin.to_path_buf());
    let out = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R"])
        .arg(codesign_requirement_arg())
        .arg(&checked)
        .output();
    match out {
        Ok(o) => {
            let detail = String::from_utf8_lossy(&o.stderr).trim().to_string();
            SignatureVerdict {
                trusted: o.status.success(),
                checked: checked.display().to_string(),
                detail: if detail.is_empty() {
                    if o.status.success() {
                        "valid on disk; satisfies the Anthropic PBC designated requirement".into()
                    } else {
                        format!("codesign exit {}", o.status.code().unwrap_or(-1))
                    }
                } else {
                    detail
                },
            }
        }
        // codesign missing is not a reason to trust an unverified binary.
        Err(e) => SignatureVerdict {
            trusted: false,
            checked: checked.display().to_string(),
            detail: format!("codesign could not be run: {e}"),
        },
    }
}

/// What happened when Claude Code was installed.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeReport {
    pub installer_bytes: u64,
    pub installed_at: String,
    pub signature: SignatureVerdict,
}

/// **Drive Anthropic's installer.** Download it, run it, find what it installed, verify it.
///
/// Four properties, each of which is a license condition or a house rule:
///
/// 1. **The installer is theirs and is run unmodified.** It is written to disk first rather
///    than piped into `bash`, so a truncated download is a legible failure instead of half a
///    script executing — `curl | bash` cannot tell the two apart.
/// 2. **RichOS never writes their binary.** Everything under `~/.local` is written by their
///    process, not this one.
/// 3. **A finished installer is not a success.** The binary is located again from scratch
///    ([`find_claude`]) and its signature verified; either failing is an error, not a warning.
/// 4. **The staging directory holding their script is removed on every path**, including a
///    panic ([`Staging`]).
pub fn install_claude_code(
    fetcher: &dyn Fetcher,
    runner: &dyn Runner,
    paths: &SetupPaths,
) -> Result<ClaudeReport, SetupError> {
    let home = paths.home.clone().ok_or(SetupError::NoHome)?;
    let staging = Staging::new(&app_support_richos(&home).join("claude-installer"), "download")?;
    let script = staging.path().join("install.sh");

    let installer_bytes = fetcher.fetch(CLAUDE_INSTALLER_URL, &script)?;

    // A shell script that is not a shell script is a redirect page or a captive portal, and
    // running it would be the worst possible way to find that out.
    let head = std::fs::read(&script).unwrap_or_default();
    if !head.starts_with(b"#!") {
        return Err(SetupError::DownloadFailed {
            url: CLAUDE_INSTALLER_URL.to_string(),
            status: format!("what came back is not a script ({installer_bytes} bytes, no shebang)"),
        });
    }

    let (code, stderr) = runner.run_installer(&script, &home)?;
    if code != 0 {
        return Err(SetupError::InstallerRefused { code, stderr });
    }

    // FOUND AGAIN FROM SCRATCH. Not "the installer exited 0, therefore it is there."
    let found = find_claude(paths);
    let at = match (&found.present, &found.at) {
        (true, Some(at)) => PathBuf::from(at),
        _ => {
            return Err(SetupError::ClaudeStillMissing {
                looked_in: if found.looked_in.is_empty() {
                    "(nowhere — no HOME and no PATH)".into()
                } else {
                    found.looked_in.join("; ")
                },
            })
        }
    };

    let signature = verify_claude_signature(&at);
    if !signature.trusted {
        return Err(SetupError::SignatureRejected {
            path: signature.checked.clone(),
            detail: signature.detail.clone(),
        });
    }

    Ok(ClaudeReport { installer_bytes, installed_at: at.display().to_string(), signature })
}

// ===========================================================================================
// INSTALL — THE ENGINE
// ===========================================================================================

/// Unpacking a tarball. A trait so the malformed-archive and wrong-shape paths are tests.
pub trait Extractor: Send + Sync {
    /// Extract `archive` into `into`. Implementations must not follow the archive's own idea
    /// of where things go outside `into`.
    fn extract(&self, archive: &Path, into: &Path) -> Result<(), SetupError>;
}

/// `/usr/bin/tar`, by absolute path, gzip, no owner mapping.
///
/// **Runs only after the digest matches**, so the bytes `tar` sees are bytes this build named
/// at compile time. That ordering is the actual protection against a hostile archive; the
/// flags are the second layer.
pub struct TarExtractor;

impl Extractor for TarExtractor {
    fn extract(&self, archive: &Path, into: &Path) -> Result<(), SetupError> {
        let out = Command::new("/usr/bin/tar")
            .args(["-x", "-z", "--no-same-owner", "-f"])
            .arg(archive)
            .arg("-C")
            .arg(into)
            .output()
            .map_err(|e| SetupError::InstallFailed {
                what: "the RichOS engine".to_string(),
                detail: format!("tar could not be run: {e}"),
            })?;
        if !out.status.success() {
            return Err(SetupError::EngineShapeInvalid {
                detail: format!(
                    "tar refused it: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            });
        }
        Ok(())
    }
}

/// What happened when the engine was installed.
#[derive(Debug, Clone, Serialize)]
pub struct EngineReport {
    pub installed_at: String,
    pub version: String,
    pub sha256: String,
    pub bytes: u64,
    /// The `INSTALLED-FROM` stamp's contents, so the operator's log carries what the file does.
    pub stamp: String,
    /// Preserved predecessor, including any adopter-owned material in an old engine.
    pub previous_at: Option<String>,
}

/// These are adopter-owned surfaces in the legacy engine contract. Preserve them
/// at the same relative locations when replacing code. Unknown customized files
/// also survive in the complete predecessor backup for later ownership review.
const LEGACY_ADOPTION_PATHS: &[&str] = &[
    "CLAUDE.md", "orchestration.config", ".claude/agents", "ceo-wiki",
    "ceo-inbox", "ceo-briefings", "ceo-todos",
];

fn copy_adoption(source: &Path, destination: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if let Some(parent) = destination.parent() { std::fs::create_dir_all(parent)?; }
    if metadata.file_type().is_symlink() {
        #[cfg(unix)] { return std::os::unix::fs::symlink(std::fs::read_link(source)?, destination); }
        #[cfg(not(unix))] { return Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "legacy symbolic links require an explicit platform adapter")); }
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(destination)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_adoption(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        std::fs::copy(source, destination)?;
    } else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported legacy file type"));
    }
    std::fs::set_permissions(destination, metadata.permissions())?;
    Ok(())
}

/// **Fetch the pinned engine release and install it, atomically.**
///
/// The order is the whole design:
///
/// 1. download to staging;
/// 2. **digest**, against the value compiled into this binary — a mismatch stops here, before
///    `tar` ever sees the bytes;
/// 3. extract into staging;
/// 4. **shape** — `scripts/hooks/` and `VERSION`, the same predicate the resolver uses, so
///    nothing can be installed that the next boot would refuse;
/// 5. **version** — the extracted `VERSION` must be the pinned one, which catches a release
///    asset that was rebuilt under the same name;
/// 6. only now, swap: the existing engine (if any) is renamed aside, the new one renamed in,
///    and the old one removed **after** the new one is in place.
///
/// Step 6 keeps the old copy until the new one is live, so the failure mode of a failed
/// `rename` is "you still have the engine you had", never "you have neither".
pub fn install_engine(
    fetcher: &dyn Fetcher,
    extractor: &dyn Extractor,
    pin: &EnginePin,
    dest: &Path,
) -> Result<EngineReport, SetupError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SetupError::InstallFailed {
            what: "the RichOS engine".to_string(),
            detail: format!("{}: {e}", parent.display()),
        })?;
    }
    let staging = Staging::new(dest, "incoming")?;
    let archive = staging.path().join("engine.tar.gz");

    // THE GITHUB MOVE (CEO, 2026-10-27): a pin on RichOS's old GitHub address is fetched from its
    // new address first, and from the pinned one when the new one has nothing (before the move),
    // so a setup after the move does not depend on GitHub's redirect. The digest below is checked
    // either way, so the address cannot change what is installed.
    let (bytes, from) = match crate::github_home::at_home(&pin.url) {
        Some(home) => match fetcher.fetch(&home, &archive) {
            Ok(bytes) => (bytes, home),
            Err(_) => (fetcher.fetch(&pin.url, &archive)?, pin.url.clone()),
        },
        None => (fetcher.fetch(&pin.url, &archive)?, pin.url.clone()),
    };

    // 2. DIGEST FIRST.
    let got = sha256_file(&archive).map_err(|e| SetupError::InstallFailed {
        what: "the RichOS engine".to_string(),
        detail: format!("could not read what downloaded: {e}"),
    })?;
    if got != pin.sha256 {
        return Err(SetupError::DigestMismatch {
            url: pin.url.clone(),
            expected: pin.sha256.clone(),
            got,
        });
    }

    // 3. EXTRACT.
    let unpacked = staging.path().join("unpacked");
    std::fs::create_dir_all(&unpacked).map_err(|e| SetupError::InstallFailed {
        what: "the RichOS engine".to_string(),
        detail: format!("{}: {e}", unpacked.display()),
    })?;
    extractor.extract(&archive, &unpacked)?;

    // The asset carries exactly one top-level `engine/`. Anything else is a different artifact
    // wearing the right digest's name, which cannot happen — and is checked anyway, because
    // "cannot happen" is what every unchecked assumption said first.
    let root = unpacked.join("engine");
    if !root.is_dir() {
        let saw: Vec<String> = std::fs::read_dir(&unpacked)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        return Err(SetupError::EngineShapeInvalid {
            detail: format!(
                "expected a single `engine` folder inside, found: {}",
                if saw.is_empty() { "nothing".to_string() } else { saw.join(", ") }
            ),
        });
    }

    // 4. SHAPE — the resolver's own predicate.
    if !engine_looks_valid(&root) {
        let mut missing = Vec::new();
        if !root.join("scripts/hooks").is_dir() {
            missing.push("scripts/hooks");
        }
        if !root.join("VERSION").is_file() {
            missing.push("VERSION");
        }
        return Err(SetupError::EngineShapeInvalid {
            detail: format!("it is missing {}", missing.join(" and ")),
        });
    }

    // 5. VERSION.
    let found_version = engine_version(&root).unwrap_or_default();
    if found_version != pin.version {
        return Err(SetupError::EngineVersionMismatch {
            expected: pin.version.clone(),
            found: if found_version.is_empty() { "(blank)".into() } else { found_version },
        });
    }

    if pin.version == "1.2.0" {
        crate::runtime::verify_engine(&root)
            .map_err(|e| SetupError::EngineShapeInvalid { detail: e.to_string() })?;
    }

    // Carry known adopter data into the staged replacement before touching the old
    // installation. The asset digest describes delivered code, not these private overlays.
    let mut preserved = Vec::new();
    for relative in LEGACY_ADOPTION_PATHS {
        let source = dest.join(relative);
        match std::fs::symlink_metadata(&source) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(SetupError::InstallFailed { what: "legacy data preservation".into(), detail: format!("{relative}: {error}") }),
        }
        let target = root.join(relative);
        if let Ok(metadata) = std::fs::symlink_metadata(&target) {
            let removed = if metadata.is_dir() && !metadata.file_type().is_symlink() {
                std::fs::remove_dir_all(&target)
            } else { std::fs::remove_file(&target) };
            removed.map_err(|e| SetupError::InstallFailed { what: "legacy data preservation".into(), detail: e.to_string() })?;
        }
        copy_adoption(&source, &target).map_err(|e| SetupError::InstallFailed {
            what: "legacy data preservation".into(), detail: format!("{relative}: {e}") })?;
        preserved.push(*relative);
    }
    if !preserved.is_empty() {
        let receipt = serde_json::json!({"schema":1,"preserved_paths":preserved,"personal_migration":false});
        std::fs::write(root.join("ADOPTION-PRESERVED.json"), receipt.to_string()).map_err(|e| SetupError::InstallFailed {
            what: "legacy data preservation".into(), detail: e.to_string() })?;
    }

    // THE FRESHNESS STAMP, written before the swap so it is inside the directory that lands.
    // `provision.rs` writes the same shape for the loro compiler: identity baked INSIDE the
    // artifact, so a stale copy is detectable rather than silent.
    let stamp = format!(
        "engine {version}\nsha256 {sha}\nbytes {bytes}\nfrom {url}\n",
        version = pin.version,
        sha = pin.sha256,
        url = from
    );
    std::fs::write(root.join("INSTALLED-FROM"), &stamp).map_err(|e| SetupError::InstallFailed {
        what: "the RichOS engine".to_string(),
        detail: format!("could not write the version stamp: {e}"),
    })?;

    // 6. SWAP. The old copy is moved aside, not deleted, until the new one is in place.
    let previous = dest.with_file_name(format!(
        "{}.previous.{}",
        dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "engine".into()),
        uuid::Uuid::new_v4()
    ));
    let had_previous = dest.exists();
    if had_previous {
        std::fs::rename(dest, &previous).map_err(|e| SetupError::InstallFailed {
            what: "the RichOS engine".to_string(),
            detail: format!("could not move the existing one aside: {e}"),
        })?;
    }
    if let Err(e) = std::fs::rename(&root, dest) {
        // PUT IT BACK. This is the branch that decides whether a failure leaves him with the
        // engine he had or with nothing.
        if had_previous {
            let _ = std::fs::rename(&previous, dest);
        }
        return Err(SetupError::InstallFailed {
            what: "the RichOS engine".to_string(),
            detail: format!("could not put the new one in place: {e}"),
        });
    }
    // An older engine may contain adopter-owned doctrine, agents or wiki records.
    // Preserve the complete predecessor. Cleanup requires a separate inventory;
    // a successful code replacement is not permission to delete user knowledge.
    // `staging` is dropped here and its directory removed. The engine has already been renamed
    // OUT of it; what goes is the downloaded archive and the emptied `unpacked/`.
    drop(staging);

    Ok(EngineReport {
        installed_at: dest.display().to_string(),
        version: pin.version.clone(),
        sha256: pin.sha256.clone(),
        bytes,
        stamp,
        previous_at: had_previous.then(|| previous.display().to_string()),
    })
}
