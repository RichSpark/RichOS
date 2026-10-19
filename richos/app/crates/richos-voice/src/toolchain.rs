//! Which binary and which weights are about to hear the CEO — checked before the mic opens.
//!
//! # Why this is here and not only in the service
//!
//! The whisper settings decision table's §7 is titled "The second consumer nobody had audited",
//! and this crate is that consumer: `stt.rs` shells out to the same `whisper-cli` the call
//! pipeline does, and until 2026-09-10 it ran at whisper.cpp's own `-mc -1` — the identical
//! unexamined default that filled 7.8% of a 92-minute channel with one fabricated sentence.
//! That got fixed. This closes the layer underneath it, which the table's §4 named and left open:
//! **the settings are now deliberate, and the binary they are handed to can change with no trace.**
//!
//! A settings table is a set of claims about ONE build. `-fa` is the worked example — flash
//! attention is whisper.cpp's default, `decode_args` pins it explicitly because it is worth 1.57
//! WER points, and that protection is worth exactly as much as knowing which binary received it.
//!
//! # ONE registry and ONE lock, shared with the Node service
//!
//! The model pins are `engine/voice/models/model-pins.json`, compiled in with `include_str!`
//! rather than re-typed here. Two registries of truth is how the second unpinned consumer came to
//! exist in the first place, and a table this crate copied would be a table free to drift.
//!
//! The trust-on-first-use lock is the SAME file the service writes,
//! `~/.config/richos/whisper-toolchain.lock.json` (`RICHOS_TOOLCHAIN_LOCK` overrides). That is not
//! merely tidy: the two consumers share the model-hash cache, so once the call pipeline has hashed
//! `ggml-small.en.bin` the voice path gets its verification for free, and a binary this machine
//! has never seen is noticed whichever surface meets it first.
//!
//! # Refuse versus warn, and why they differ
//!
//! Identical to the service's, because a rule that changes by surface is a rule nobody can state:
//!
//! - **Weights that are not the pinned weights: REFUSE.** The pin table has authority — upstream
//!   plus a witness on disk — and the fetch path already refuses on the same hash.
//! - **A binary or backend that is not the one this machine locked: WARN, naming both identities.**
//!   The lock records what WAS, not what is right, and a `brew upgrade` must not silently take
//!   the CEO's voice mode away.
//!
//! `RICHOS_WHISPER_STRICT_TOOLCHAIN=1` escalates every warning to a refusal.
//!
//! # Cost, measured rather than assumed
//!
//! A conversational utterance decodes in 0.47–0.74 s (`stt.rs`), so nothing on this path may cost
//! a second. Measured on this M4 with `shasum -a 256`, three runs each: the 654,720-byte
//! `whisper-cli` hashes in 0.01 s and the 487,614,201-byte `ggml-small.en.bin` in 0.93–0.95 s.
//! So the binary is hashed every time and the model is hashed only when the
//! (path, size, mtime, inode) tuple says it moved — and this all happens ONCE, in
//! `Recognizer::resolve()` at voice-mode start, never per utterance. A missing model is already a
//! calm message at the toggle rather than a failure mid-sentence; so is a substituted one.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The pin table, compiled in from the ONE place it lives. See the module docs.
///
/// `pub(crate)` so `provision.rs` reads the SAME embedded string rather than adding a second
/// `include_str!` of the same file. Two embeddings would compile and would be precisely the
/// defect `model-pins.json`'s own header warns about: "two registries of truth is how the second
/// unpinned consumer happened in the first place."
pub(crate) const MODEL_PINS_JSON: &str = include_str!("../../../../engine/voice/models/model-pins.json");

/// How loud a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Recorded in provenance. Nothing is wrong.
    Note,
    /// Said out loud and recorded; the run still happens.
    Warn,
    /// The run does not happen.
    Refuse,
}

/// Every way the toolchain can differ from what was expected. Mirrors the service's
/// `TOOLCHAIN_FINDING` so one vocabulary describes both surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// No lock existed; what was found is now the lock.
    FirstLock { sha256: String, version: Option<String>, path: String },
    /// The `whisper-cli` bytes are not the bytes this machine locked.
    BinChanged {
        was: String,
        now: String,
        was_version: Option<String>,
        now_version: Option<String>,
        was_path: Option<String>,
        now_path: String,
    },
    /// A ggml backend loaded at run time is not the one locked.
    BackendChanged { backend: String, was: Option<String>, now: Option<String> },
    /// The binary would not say its version. Measured: 1.8.3 exits 0 with empty stdout.
    VersionUnknown { path: String, sha256: String, stderr_first_line: Option<String> },
    /// Not the build the decision table's measurements were taken on.
    VersionOffReference { was: String, now: String },
    /// The weights are not the pinned weights.
    ModelHashMismatch { model_id: String, path: String, was: String, now: String },
    /// No source pin exists for this model — an override, or an unlisted model.
    ModelUnpinned { model_id: String, now: String },
}

impl Finding {
    /// The base severity, before strict mode has its say.
    pub fn base_severity(&self) -> Severity {
        match self {
            Finding::FirstLock { .. } => Severity::Note,
            Finding::BinChanged { .. }
            | Finding::BackendChanged { .. }
            | Finding::VersionUnknown { .. }
            | Finding::VersionOffReference { .. }
            | Finding::ModelUnpinned { .. } => Severity::Warn,
            Finding::ModelHashMismatch { .. } => Severity::Refuse,
        }
    }

    /// The severity after strict mode. A `Note` is never escalated: a first install is not a
    /// failure in any mode, and escalating it would make a fresh machine unusable in the exact
    /// configuration someone reproducing measurements would choose.
    pub fn severity(&self, strict: bool) -> Severity {
        match (strict, self.base_severity()) {
            (true, Severity::Warn) => Severity::Refuse,
            (_, s) => s,
        }
    }

    /// What changed, from what, to what, and what a person does about it. Names BOTH identities:
    /// "the binary changed" sends a reader to a support conversation; naming the two hashes sends
    /// them to `brew`.
    pub fn message(&self, strict: bool) -> String {
        let s = |h: &str| h.chars().take(12).collect::<String>();
        match self {
            Finding::FirstLock { sha256, version, path } => format!(
                "First run on this machine: no whisper toolchain was locked yet, so there was nothing to \
                 compare against. RichOS recorded what it found: whisper-cli {} (sha256 {}…) at {} and \
                 every later run is checked against it.",
                version.as_deref().map(|v| format!("version {v}")).unwrap_or_else(|| "of unknown version".into()),
                s(sha256),
                path
            ),
            Finding::BinChanged { was, now, was_version, now_version, was_path, now_path } => format!(
                "THE WHISPER BINARY CHANGED since this machine last listened. Was {} sha256 {}…{}; now {} \
                 sha256 {}… at {}. Decode defaults are a property of a build: whisper.cpp defaults flash \
                 attention ON, and losing it is worth 1.57 WER points on this project's own corpus. {}",
                was_version.as_deref().map(|v| format!("version {v}")).unwrap_or_else(|| "an unversioned build".into()),
                s(was),
                was_path.as_deref().map(|p| format!(" at {p}")).unwrap_or_default(),
                now_version.as_deref().map(|v| format!("version {v}")).unwrap_or_else(|| "an unversioned build".into()),
                s(now),
                now_path,
                if strict {
                    "RICHOS_WHISPER_STRICT_TOOLCHAIN is set, so voice mode will not start on it."
                } else {
                    "Voice mode is running on the NEW binary and every utterance is attributed to it. \
                     If this was a deliberate upgrade, nothing needs doing."
                }
            ),
            Finding::BackendChanged { backend, was, now } => format!(
                "The ggml {} backend changed: sha256 {}… -> {}…. The compute backends load at run time from \
                 their own formula, independently of whisper-cli's version, so this can change with the \
                 binary untouched.",
                backend,
                s(was.as_deref().unwrap_or("nothing")),
                s(now.as_deref().unwrap_or("nothing"))
            ),
            Finding::VersionUnknown { path, sha256, stderr_first_line } => format!(
                "whisper-cli at {} would not say its version: `--version` produced no version line on stdout{}. \
                 Measured on this project 2026-09-10: whisper-cpp 1.8.3 rejects `--version` and still EXITS 0, \
                 so an empty answer is a real build that will not identify itself, not a probe failure. Its \
                 sha256 {}… is the identity used instead.",
                path,
                stderr_first_line.as_deref().map(|l| format!(" (it said: \"{l}\")")).unwrap_or_default(),
                s(sha256)
            ),
            Finding::VersionOffReference { was, now } => format!(
                "whisper-cli is version {now}, and every decode setting RichOS ships was measured against \
                 {was}. Re-run the measurements before trusting the settings table's numbers on this build."
            ),
            Finding::ModelHashMismatch { model_id, path, was, now } => format!(
                "REFUSING TO LISTEN: the model file for \"{}\" is not the model RichOS pinned. Expected sha256 \
                 {}…, found {}… at {}. These are different weights under the right name — a different model \
                 would hear different words and nothing downstream would know. Re-fetch the model \
                 (`richos-service fetch-model {}`) and start voice mode again.",
                model_id,
                s(was),
                s(now),
                path,
                model_id
            ),
            Finding::ModelUnpinned { model_id, now } => format!(
                "The model \"{}\" is not in RichOS's pin table, so there is no source hash to check it against; \
                 its sha256 {}… has been recorded instead. That is expected for a deliberate override.",
                model_id,
                s(now)
            ),
        }
    }
}

/// What a check concluded.
#[derive(Debug, Clone)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub strict: bool,
    /// One line naming the binary and the weights, for the turn's provenance.
    pub provenance: String,
    pub bin_sha256: String,
    pub bin_version: Option<String>,
    pub model_sha256: Option<String>,
    pub model_pinned: bool,
    /// Was the model's hash read from the shared lock, or computed just now? Worth carrying rather
    /// than inferring: it is how anyone checks that the lock the two consumers share is actually
    /// being shared, and a cache that silently never hits looks exactly like one that always does.
    pub model_hash_cached: bool,
    pub first_run: bool,
}

impl Report {
    pub fn verdict(&self) -> Severity {
        self.findings.iter().map(|f| f.severity(self.strict)).max().unwrap_or(Severity::Note)
    }
    /// Every finding at or above `Warn`, as sentences — what the caller says out loud.
    pub fn warnings(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity(self.strict) >= Severity::Warn)
            .map(|f| f.message(self.strict))
            .collect()
    }
    /// The sentences behind a refusal, or empty when nothing refused.
    pub fn refusals(&self) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity(self.strict) == Severity::Refuse)
            .map(|f| f.message(self.strict))
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Pure: parsing what the binary said about itself.
// ---------------------------------------------------------------------------------------------

/// The version out of `whisper-cli --version` STDOUT, and only stdout.
///
/// 1.8.3 writes its whole usage banner to stderr and exits 0. Anything that read stderr — or
/// trusted the exit code — would call that a successful probe of a build with no version, so the
/// exit status is recorded and never consulted.
pub fn parse_version(stdout: &str) -> Option<String> {
    const NEEDLE: &str = "whisper.cpp version:";
    for line in stdout.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(idx) = lower.find(NEEDLE) {
            let rest = line[idx + NEEDLE.len()..].trim();
            let token = rest.split_whitespace().next().unwrap_or("");
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

/// The ggml backends the binary itself says it loaded, in order.
///
/// Read off its own `load_backend:` startup lines rather than guessed from a Cellar path, so a
/// `GGML_BACKEND_PATH` override is visible too.
pub fn parse_backends(stderr: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in stderr.lines() {
        let line = line.trim_end();
        let Some(rest) = line.strip_prefix("load_backend: loaded ") else { continue };
        let Some((name, tail)) = rest.split_once(" backend from ") else { continue };
        if !name.is_empty() && !tail.trim().is_empty() {
            out.push((name.to_string(), tail.trim().to_string()));
        }
    }
    out
}

/// Is strict mode on? Off by default, and the default is a decision: on by default would mean a
/// Homebrew upgrade silently takes voice mode away from someone who has no idea what a bottle is.
pub fn strict_mode() -> bool {
    matches!(
        std::env::var("RICHOS_WHISPER_STRICT_TOOLCHAIN").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// The pinned sha256 for a model id, from the compiled-in pin table.
pub fn pinned_sha256(model_id: &str) -> Option<String> {
    let v: Value = serde_json::from_str(MODEL_PINS_JSON).ok()?;
    let models = v.get("models")?.as_array()?;
    for m in models {
        if m.get("id")?.as_str()? == model_id {
            return Some(m.get("sha256")?.as_str()?.to_ascii_lowercase());
        }
    }
    None
}

/// The whisper.cpp version every decode setting was measured against.
pub fn reference_version() -> Option<String> {
    let v: Value = serde_json::from_str(MODEL_PINS_JSON).ok()?;
    Some(v.get("toolchain")?.get("whisperCppVersion")?.as_str()?.to_string())
}

/// One line naming the binary and the weights.
pub fn provenance(
    version: Option<&str>,
    bin_sha: &str,
    backends: &[String],
    model_id: &str,
    model_sha: Option<&str>,
) -> String {
    let head = match version {
        Some(v) => format!("whisper.cpp {v}"),
        None => "whisper.cpp (version not reported)".to_string(),
    };
    let b = if backends.is_empty() { String::new() } else { format!(" [{}]", backends.join("/")) };
    let m = match model_sha {
        Some(s) => format!(" model:{}@{}", model_id, s.chars().take(12).collect::<String>()),
        None => String::new(),
    };
    format!("{head} bin:{}{b}{m}", bin_sha.chars().take(12).collect::<String>())
}

// ---------------------------------------------------------------------------------------------
// I/O.
// ---------------------------------------------------------------------------------------------

/// sha256 of a file, streamed so a 1.6 GB model never lands in memory. `None` if unreadable.
pub fn hash_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// The stat tuple deciding whether a cached hash still describes this file.
///
/// FOUR FIELDS, AND THE SAME FOUR THE NODE SERVICE USES — size, whole-millisecond mtime, inode,
/// device. This is a cross-consumer contract, not a local choice: the two surfaces share one lock,
/// so a field one of them writes as `0` or at a different precision is a cache the other silently
/// misses. Node's `statSync` gives sub-millisecond mtime (`1787746219119.914`) where
/// `as_millis()` gives `1787746219119`; whole milliseconds is the coarsest unit both produce
/// exactly, so both truncate to it. `lib/toolchain.js#fileIdentity` says the same thing from the
/// other side, and the service's suite asserts the two still agree.
///
/// The limit is stated rather than buried: a rewrite in place that restored every field would be
/// believed until something re-hashes it. `richos-service toolchain --recheck` is that something.
fn file_identity(path: &Path) -> Option<(u64, i64, u64, u64)> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    #[cfg(unix)]
    let (ino, dev) = {
        use std::os::unix::fs::MetadataExt;
        (md.ino(), md.dev())
    };
    #[cfg(not(unix))]
    let (ino, dev) = (0u64, 0u64);
    Some((md.len(), mtime, ino, dev))
}

/// Where this machine's lock lives — the SAME file the Node service writes.
pub fn lock_path() -> PathBuf {
    if let Ok(p) = std::env::var("RICHOS_TOOLCHAIN_LOCK") {
        return PathBuf::from(expand_home(&p));
    }
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home).join(".config/richos/whisper-toolchain.lock.json")
}

fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

/// Ask the binary who it is. One invocation, both streams, exit status recorded and not trusted.
pub fn probe(bin: &Path) -> (Option<String>, Option<String>, Vec<(String, String)>) {
    let Ok(out) = Command::new(bin).arg("--version").output() else {
        return (None, None, Vec::new());
    };
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let version = parse_version(&stdout);
    let first_err = stderr.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
    (version, first_err, parse_backends(&stderr))
}

/// Establish and check the identity of everything about to hear the CEO.
///
/// Called ONCE, from `Recognizer::resolve()` at voice-mode start — never per utterance.
pub fn check(bin: &Path, model: &Path, model_id: &str) -> Report {
    let strict = strict_mode();
    let lock_file = lock_path();
    let previous: Option<Value> =
        std::fs::read_to_string(&lock_file).ok().and_then(|s| serde_json::from_str(&s).ok());

    let bin_sha = hash_file(bin).unwrap_or_default();
    let real_bin = std::fs::canonicalize(bin).unwrap_or_else(|_| bin.to_path_buf());
    let (version, stderr_first, backend_paths) = probe(bin);
    let backends: Vec<(String, String, Option<String>)> = backend_paths
        .into_iter()
        .map(|(name, p)| {
            let h = hash_file(Path::new(&p));
            (name, p, h)
        })
        .collect();

    let mut findings: Vec<Finding> = Vec::new();

    match previous.as_ref().and_then(|v| v.get("bin")) {
        None => findings.push(Finding::FirstLock {
            sha256: bin_sha.clone(),
            version: version.clone(),
            path: bin.display().to_string(),
        }),
        Some(locked_bin) => {
            let was = locked_bin.get("sha256").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            if !was.is_empty() && was != bin_sha {
                findings.push(Finding::BinChanged {
                    was,
                    now: bin_sha.clone(),
                    was_version: locked_bin.get("version").and_then(|v| v.as_str()).map(str::to_string),
                    now_version: version.clone(),
                    was_path: locked_bin
                        .get("realPath")
                        .or_else(|| locked_bin.get("path"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    now_path: real_bin.display().to_string(),
                });
            }
            // Backends are compared only when the probe actually saw some. "Nothing observed" is
            // not "changed"; the binary hash above already carries that case.
            let locked_backends = previous
                .as_ref()
                .and_then(|v| v.get("backends"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if !backends.is_empty() && !locked_backends.is_empty() {
                for (name, _p, now) in &backends {
                    let was = locked_backends
                        .iter()
                        .find(|b| b.get("name").and_then(|v| v.as_str()) == Some(name.as_str()))
                        .and_then(|b| b.get("sha256"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    if was.as_deref() != now.as_deref() {
                        findings.push(Finding::BackendChanged {
                            backend: name.clone(),
                            was,
                            now: now.clone(),
                        });
                    }
                }
            }
        }
    }

    match &version {
        None => findings.push(Finding::VersionUnknown {
            path: bin.display().to_string(),
            sha256: bin_sha.clone(),
            stderr_first_line: stderr_first.clone(),
        }),
        Some(v) => {
            if let Some(reference) = reference_version() {
                if *v != reference {
                    findings.push(Finding::VersionOffReference { was: reference, now: v.clone() });
                }
            }
        }
    }

    // ---- the weights ---------------------------------------------------------------------------
    let identity = file_identity(model);
    let cached = previous
        .as_ref()
        .and_then(|v| v.get("models"))
        .and_then(|m| m.get(model.to_string_lossy().as_ref()))
        .cloned();
    let cache_hit = match (&cached, identity) {
        (Some(c), Some((bytes, mtime, ino, dev))) => {
            c.get("bytes").and_then(|v| v.as_u64()) == Some(bytes)
                && c.get("mtimeMs").and_then(|v| v.as_f64()).map(|f| f as i64) == Some(mtime)
                && c.get("ino").and_then(|v| v.as_u64()) == Some(ino)
                && c.get("dev").and_then(|v| v.as_u64()) == Some(dev)
        }
        _ => false,
    };
    let model_sha = if cache_hit {
        cached.as_ref().and_then(|c| c.get("sha256")).and_then(|v| v.as_str()).map(str::to_string)
    } else {
        hash_file(model)
    };

    let pin = pinned_sha256(model_id);
    match (&pin, &model_sha) {
        (Some(want), Some(got)) if want != &got.to_ascii_lowercase() => {
            findings.push(Finding::ModelHashMismatch {
                model_id: model_id.to_string(),
                path: model.display().to_string(),
                was: want.clone(),
                now: got.clone(),
            });
        }
        (None, Some(got)) => {
            findings.push(Finding::ModelUnpinned { model_id: model_id.to_string(), now: got.clone() });
        }
        _ => {}
    }

    let backend_names: Vec<String> = backends.iter().map(|(n, _, _)| n.clone()).collect();
    let report = Report {
        provenance: provenance(
            version.as_deref(),
            &bin_sha,
            &backend_names,
            model_id,
            model_sha.as_deref(),
        ),
        strict,
        bin_sha256: bin_sha.clone(),
        bin_version: version.clone(),
        model_sha256: model_sha.clone(),
        model_pinned: pin.is_some(),
        model_hash_cached: cache_hit,
        first_run: previous.is_none(),
        findings,
    };

    // A refused run never updates the lock: caching the identity of bytes just rejected would let
    // the next attempt through on a cache hit, which is a guard that disarms itself.
    if report.verdict() != Severity::Refuse {
        write_lock(WriteLock {
            lock_file: &lock_file,
            previous,
            bin_sha: &bin_sha,
            real_bin: &real_bin,
            bin,
            version: &version,
            backends: &backends,
            model,
            model_id,
            model_sha: &model_sha,
            identity,
        });
    }
    report
}

struct WriteLock<'a> {
    lock_file: &'a Path,
    previous: Option<Value>,
    bin_sha: &'a str,
    real_bin: &'a Path,
    bin: &'a Path,
    version: &'a Option<String>,
    backends: &'a [(String, String, Option<String>)],
    model: &'a Path,
    model_id: &'a str,
    model_sha: &'a Option<String>,
    identity: Option<(u64, i64, u64, u64)>,
}

fn write_lock(w: WriteLock<'_>) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut models = w
        .previous
        .as_ref()
        .and_then(|v| v.get("models"))
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));
    if let (Some(sha), Some((bytes, mtime, ino, dev)), Some(obj)) =
        (w.model_sha, w.identity, models.as_object_mut())
    {
        obj.insert(
            w.model.to_string_lossy().to_string(),
            serde_json::json!({
                "id": w.model_id, "bytes": bytes, "mtimeMs": mtime, "ino": ino, "dev": dev, "sha256": sha
            }),
        );
    }
    let lock = serde_json::json!({
        "_comment": [
            "RichOS whisper toolchain lock — TRUST ON FIRST USE, on THIS machine.",
            "Shared by tools/richos-service and app/crates/richos-voice, on purpose: one lock means",
            "the two consumers share the model-hash cache and neither can miss a change the other saw.",
        ],
        "schema": 1,
        "lockedOn": w.previous.as_ref().and_then(|v| v.get("lockedOn")).cloned()
            .unwrap_or_else(|| Value::String(now.to_string())),
        "updatedOn": now.to_string(),
        "host": { "platform": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "bin": {
            "path": w.bin.display().to_string(),
            "realPath": w.real_bin.display().to_string(),
            "sha256": w.bin_sha,
            "version": w.version,
        },
        "backends": w.backends.iter()
            .map(|(n, p, h)| serde_json::json!({ "name": n, "path": p, "sha256": h }))
            .collect::<Vec<_>>(),
        "models": models,
    });
    if let Some(parent) = w.lock_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = w.lock_file.with_extension(format!("tmp-{}", std::process::id()));
    let body = format!("{}\n", serde_json::to_string_pretty(&lock).unwrap_or_default());
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, w.lock_file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_read_from_stdout_only() {
        assert_eq!(parse_version("whisper.cpp version: 1.9.1\n").as_deref(), Some("1.9.1"));
        assert_eq!(parse_version("").as_deref(), None);
    }

    #[test]
    fn a_build_that_refuses_version_yields_none_rather_than_a_version_from_its_banner() {
        // The measured trap: whisper-cpp 1.8.3 exits 0, prints its usage banner to STDERR, and
        // leaves stdout empty. Reading stderr, or trusting the exit code, would invent an answer.
        let stderr = "error: unknown argument: --version\nusage: whisper-cli [options] file0 file1 ...\n";
        assert_eq!(parse_version(stderr), None);
    }

    #[test]
    fn backends_are_read_off_the_binarys_own_startup_lines() {
        let stderr = "ggml_metal_device_init: has tensor            = false\n\
                      load_backend: loaded BLAS backend from /opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-blas.so\n\
                      load_backend: loaded MTL backend from /opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-metal.so\n";
        let b = parse_backends(stderr);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].0, "BLAS");
        assert_eq!(b[1].1, "/opt/homebrew/Cellar/ggml/0.17.0/libexec/libggml-metal.so");
        assert!(parse_backends("").is_empty());
    }

    #[test]
    fn the_pin_table_is_the_services_own_file_not_a_copy() {
        // If this ever fails, someone has forked the registry — the failure this change exists to
        // prevent. The values are read out of engine/voice/models/model-pins.json at compile
        // time, so a drifted copy could not produce them.
        assert_eq!(
            pinned_sha256("small.en").as_deref(),
            Some("c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d")
        );
        assert_eq!(
            pinned_sha256("large-v3-turbo").as_deref(),
            Some("1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69")
        );
        // The call-transcription default since 2026-09-10 (CEO decision page §10). It was already
        // in the shared table as the More accurate dictation model; what changed is that this
        // crate now shares a lock with a service that decodes CALLS with these weights too.
        assert_eq!(
            pinned_sha256("large-v3-turbo-q5_0").as_deref(),
            Some("394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2")
        );
        assert_eq!(pinned_sha256("no-such-model"), None);
        assert_eq!(reference_version().as_deref(), Some("1.9.1"));
    }

    #[test]
    fn wrong_weights_refuse_and_a_changed_binary_only_warns() {
        let mismatch = Finding::ModelHashMismatch {
            model_id: "small.en".into(),
            path: "/m/ggml-small.en.bin".into(),
            was: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d".into(),
            now: "46f780af19a3e6d84eaa5ab4798e42e99c712a47d9625b9db64348f76ff4fbbf".into(),
        };
        assert_eq!(mismatch.base_severity(), Severity::Refuse);
        let msg = mismatch.message(false);
        assert!(msg.contains("REFUSING TO LISTEN"));
        assert!(msg.contains("c6138d6d58ec"), "names the hash it expected");
        assert!(msg.contains("46f780af19a3"), "names the hash it found");

        let changed = Finding::BinChanged {
            was: "7dc20e3106d70746d61c419646d9bf87f726a5df7da562e26e8529067119f7b8".into(),
            now: "595da05cb412fd923ad66ce680b46d968f39be2c7156ae952d49ed0ace93d006".into(),
            was_version: Some("1.9.1".into()),
            now_version: None,
            was_path: Some("/opt/homebrew/Cellar/whisper-cpp/1.9.1/bin/whisper-cli".into()),
            now_path: "/opt/homebrew/Cellar/whisper-cpp/1.8.3/libexec/bin/whisper-cli".into(),
        };
        assert_eq!(changed.base_severity(), Severity::Warn);
        let m = changed.message(false);
        assert!(m.contains("1.9.1") && m.contains("7dc20e3106d7") && m.contains("595da05cb412"));
    }

    #[test]
    fn strict_mode_escalates_warnings_but_never_a_first_install() {
        let first = Finding::FirstLock { sha256: "ab".repeat(32), version: None, path: "/x".into() };
        assert_eq!(first.severity(true), Severity::Note, "a fresh machine is not a failure in any mode");
        let unpinned = Finding::ModelUnpinned { model_id: "x".into(), now: "cd".repeat(32) };
        assert_eq!(unpinned.severity(false), Severity::Warn);
        assert_eq!(unpinned.severity(true), Severity::Refuse);
    }

    #[test]
    fn the_strict_binary_message_does_not_claim_a_session_that_never_started() {
        let changed = Finding::BinChanged {
            was: "a".repeat(64),
            now: "b".repeat(64),
            was_version: Some("1.9.1".into()),
            now_version: Some("1.8.3".into()),
            was_path: None,
            now_path: "/x".into(),
        };
        assert!(changed.message(true).contains("will not start"));
        assert!(!changed.message(true).contains("Voice mode is running"));
    }

    #[test]
    fn the_verdict_is_the_worst_finding_present() {
        let mk = |findings: Vec<Finding>, strict: bool| Report {
            findings,
            strict,
            provenance: String::new(),
            bin_sha256: String::new(),
            bin_version: None,
            model_sha256: None,
            model_pinned: false,
            model_hash_cached: false,
            first_run: false,
        };
        assert_eq!(mk(vec![], false).verdict(), Severity::Note);
        let first = Finding::FirstLock { sha256: "a".repeat(64), version: None, path: "/x".into() };
        assert_eq!(mk(vec![first.clone()], false).verdict(), Severity::Note);
        let unpinned = Finding::ModelUnpinned { model_id: "x".into(), now: "b".repeat(64) };
        assert_eq!(mk(vec![first.clone(), unpinned.clone()], false).verdict(), Severity::Warn);
        assert_eq!(mk(vec![first, unpinned], true).verdict(), Severity::Refuse);
    }

    #[test]
    fn provenance_answers_which_binary_and_which_weights_from_the_string_alone() {
        let s = provenance(
            Some("1.9.1"),
            "7dc20e3106d70746d61c419646d9bf87f726a5df7da562e26e8529067119f7b8",
            &["BLAS".into(), "MTL".into(), "CPU".into()],
            "small.en",
            Some("c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d"),
        );
        assert_eq!(s, "whisper.cpp 1.9.1 bin:7dc20e3106d7 [BLAS/MTL/CPU] model:small.en@c6138d6d58ec");
        let unversioned = provenance(None, &"ab".repeat(32), &[], "x", None);
        assert!(unversioned.contains("version not reported"), "an unversioned build says so");
    }

    #[test]
    fn hash_file_agrees_with_the_pin_table_on_a_real_model_when_one_is_present() {
        // The strongest available check on the hashing itself: where a pinned model happens to be
        // on this machine, its hash must equal the pin. Skipped, never faked, when it is absent —
        // a test that quietly passes on a missing file proves nothing.
        //
        // ORDER MATTERS, because the loop returns after the first candidate that exists: the
        // shipping CALL-transcription default goes first, so on a machine carrying several pinned
        // models the one this product actually decodes with is the one that gets hashed. That
        // default moved to large-v3-turbo-q5_0 on 2026-09-10 (CEO decision page §10); before then
        // this list did not contain it at all. Cost is one hash either way — 1.05 s on the
        // 574,041,195-byte file against 0.93 s on small.en, measured on this M4.
        let home = std::env::var("HOME").unwrap_or_default();
        for (id, p) in [
            ("large-v3-turbo-q5_0", format!("{home}/Models/Whisper/ggml-large-v3-turbo-q5_0.bin")),
            ("small.en", format!("{home}/Models/Whisper/ggml-small.en.bin")),
            ("small.en", format!("{home}/.config/open-wispr/models/ggml-small.en.bin")),
            ("large-v3-turbo", format!("{home}/Models/Whisper/ggml-large-v3-turbo.bin")),
        ] {
            let path = Path::new(&p);
            if !path.exists() {
                continue;
            }
            assert_eq!(hash_file(path).as_deref(), pinned_sha256(id).as_deref(), "{p}");
            return;
        }
        eprintln!("no pinned model on this machine — hash agreement not exercised");
    }
}
