//! THE OPERATOR PROFILE — the arguments and the environment his lead starts with, and the
//! check it must pass before it takes any work (operator back-end spec r2 (b), (h), (i), (j),
//! (k), (n), with r3 §11 items 3-5; richos-hq `docs/plans/2026-09-24-operator-back-end-spec-r3.md`).
//!
//! **A separate type, never an [`crate::engine_profile::EngineProfile`]** (r3 (b)). Nothing in
//! here reads, calls or copies `EngineProfile::configure`: that function points a child at the
//! app's own engine, its own partition of state and its own Git identity, which is exactly
//! what his team must never run with.
//!
//! ## One engine: his live one, and none of the app's
//!
//! - `--setting-sources user,project,local`, so his `CLAUDE.md`, his settings and his plugin
//!   registration load natively, as they do in his terminal.
//! - No `--plugin-dir`, so `richos-app-engine` is never offered; [`init_check`] refuses a
//!   session that lists it anyway, and one that lists `richos-engine` other than exactly once.
//! - No `RICHOS_PROJECTS_DIR`, `RICHOS_SESSIONS_DIR` or any other app partition variable,
//!   because the environment is built from empty ([`OperatorProfile::environment`]).
//! - The supervisor is `provider-supervisor.py` from HIS declared engine (B9), with
//!   `--reap-descendants` (r3 §11 item 5). The product lease names the same flag since the
//!   product reap gap design, with its parameters on the supervisor's command line
//!   (`lease_commands.rs`); this profile keeps the environment names below.
//!
//! ## The environment, built from empty (r3 §11 item 3)
//!
//! `env_clear()`, then exactly: the names `enable.sh` stored from his terminal, `HOME` from the
//! declaration, `SSH_AUTH_SOCK` from `launchctl getenv`, `TMPDIR` from the per-user temporary
//! directory, `RICHOS_OPERATOR_LEAD`, r4 §2.3's `CLAUDE_CODE_ARTIFACT=1` (P17 passed), and, when
//! the host gives them, the supervisor's two file paths (`RICHOS_OPERATOR_REAP_LOG`,
//! `RICHOS_OPERATOR_REAP_STATE`), which `provider-supervisor.py` reads from its own environment
//! and the lead therefore inherits. The supervisor adds `RICHOS_SESSION_PID`
//! (`provider-supervisor.py:22`). Nothing of the app's own environment reaches his lead: not a
//! nightly's scratch `HOME`, not its `CLAUDE_CONFIG_DIR`, not a planted `ANTHROPIC_API_KEY`.
//! **`PATH` is the one stored at `enable.sh`**, so a tool installed later is invisible to his
//! team in the app until `enable.sh` runs again (r3 (i)).
use crate::operator_declaration::{Declaration, Refusal};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The engine plugin his terminal loads, by the name it announces in `system/init.plugins`.
pub const HIS_ENGINE_PLUGIN: &str = "richos-engine";
/// The report tool the operator lead must hold (c).
pub const REPORT_TOOL: &str = "mcp__richos_operator__report";
/// The environment variable that tells his engine's claim hook this session is the app's own
/// lead (r2 (e), the claim item 4).
pub const LEAD_CLAIM_ENV: &str = "RICHOS_OPERATOR_LEAD";
/// The supervisor flag that makes it reap the lead's whole tree (r3 (q) item 2).
pub const REAP_DESCENDANTS: &str = "--reap-descendants";
/// Where the supervisor appends what it killed (r3 (q) item 2: "the operator log names every
/// process killed"). `provider-supervisor.py` reads it from its own environment, so it is set
/// here, and the lead inherits it: an app-set path, never a credential.
pub const REAP_LOG_ENV: &str = "RICHOS_OPERATOR_REAP_LOG";
/// Where the supervisor writes its snapshot of the lead's descendants outside the lead's own
/// group, once a second: the host's idle test (Frank's G8, r3 (q) item 4).
pub const REAP_STATE_ENV: &str = "RICHOS_OPERATOR_REAP_STATE";
/// **r4 §2.3's one stated exception to "never a `CLAUDE_*` name".** The 2.1.282 binary
/// withholds `Artifact` from every `sdk-*` entrypoint unless this is truthy, and his terminal
/// has the tool. P17 measured it (2026-09-25, VM probes-e620cde236d2): with it, `Artifact` is in
/// `system/init.tools` and two read-only calls (`list` with `scope: types`, then `list`) answer
/// with no error; without it, the tool is absent. Set by the app, never stored: `enable.sh` and
/// [`crate::operator_declaration::is_denied_name`] still refuse every stored `CLAUDE*` name.
pub const ARTIFACT_ENV: (&str, &str) = ("CLAUDE_CODE_ARTIFACT", "1");
/// The only tool taken away from his lead: nobody is at a terminal to answer it (n).
///
/// App-owned questions now run the declared engine guard and write its ask witness
/// after a durable display acknowledgement. The operator runtime must supply the
/// original session's `ReportScope.question_context` and connect its inbound delivery
/// before opening the runtime gate. The existing fixture probes do not prove that path.
pub const DISALLOWED_TOOL: &str = "AskUserQuestion";

/// The `claude` versions the probe harness has measured this path against (r2 (b), note 7).
/// A lead on any other version still opens, and he is told once that the harness should be
/// run again. Updated from the harness's recorded `claude --version`, never by hand.
///
/// 2.1.282: the operator probes P1-P15, 2026-09-24/25, every run report's `claude_version`
/// (richos-hq `docs/verification/2026-09-24-operator-probes/summary.json`).
///
/// 2.1.304: the same probes P1-P18, 2026-10-20, every verdict as on 2.1.282, every probe's
/// `claude_version` (richos-hq `docs/verification/2026-10-20-operator-probes-2.1.304/summary.json`).
pub const MEASURED_CLAUDE_VERSIONS: &[&str] = &["2.1.282", "2.1.304"];

/// The two values derived fresh at every lead start (r3 (i)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEnvironment {
    pub ssh_auth_sock: String,
    pub tmpdir: String,
}

impl SessionEnvironment {
    /// Ask launchd for the ssh agent's socket and the OS for the per-user temporary folder.
    /// Neither depends on this app's own environment, which under a nightly is `env -i`.
    pub fn derive() -> Result<Self, Refusal> {
        Self::derive_with(Path::new("/bin/launchctl"))
    }

    /// [`derive`](Self::derive), with the `launchctl` to ask.
    pub fn derive_with(launchctl: &Path) -> Result<Self, Refusal> {
        let cannot = |name: &str| Refusal { what: format!("{name} could not be derived for your team's session") };
        // `launchctl getenv` asks launchd's user domain, which is where the ssh agent's socket
        // is published at login; it is the same value his terminal carries (r2 §2, measured).
        let output = std::process::Command::new(launchctl)
            .args(["getenv", "SSH_AUTH_SOCK"])
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map_err(|_| cannot("SSH_AUTH_SOCK"))?;
        let ssh_auth_sock = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !output.status.success() || !ssh_auth_sock.starts_with('/') {
            return Err(cannot("SSH_AUTH_SOCK"));
        }
        let tmpdir = user_temp_dir().ok_or_else(|| cannot("TMPDIR"))?;
        Ok(SessionEnvironment { ssh_auth_sock, tmpdir })
    }
}

/// How a lead is started: a new session under an id the app chose, or its last session
/// resumed (r3 (l)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeadStart {
    New(String),
    Resume(String),
}

/// Everything a lead's process is made of, from a declaration that passed the gate.
#[derive(Clone, Debug)]
pub struct OperatorProfile {
    declaration: Declaration,
    session: SessionEnvironment,
    lead_claim: String,
    python: PathBuf,
    claude: PathBuf,
    /// The supervisor's operator log and descendant snapshot, when the host has given them.
    supervisor_files: Option<(PathBuf, PathBuf)>,
    machine_settings: crate::machine_settings::Settings,
}

/// The per-user temporary folder, straight from the OS: `confstr(_CS_DARWIN_USER_TEMP_DIR)`,
/// which is what `getconf DARWIN_USER_TEMP_DIR` prints. It does not read `TMPDIR`, so a
/// nightly's `env -i` scratch value can never leak into his team's session.
#[cfg(target_os = "macos")]
fn user_temp_dir() -> Option<String> {
    let mut buffer = vec![0u8; 1024];
    // SAFETY: the buffer is writable for its whole length, and confstr writes at most that
    // many bytes including the terminating NUL.
    let needed = unsafe {
        libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buffer.as_mut_ptr().cast(), buffer.len())
    };
    if needed == 0 || needed > buffer.len() {
        return None;
    }
    buffer.truncate(needed - 1);
    let dir = String::from_utf8(buffer).ok()?;
    dir.starts_with('/').then_some(dir)
}

#[cfg(not(target_os = "macos"))]
fn user_temp_dir() -> Option<String> {
    None
}

fn is_executable_file(candidate: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(candidate).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        candidate.is_file()
    }
}

/// Every match for `name` in a `PATH` value, in order. Only absolute entries count: a relative
/// entry would mean "relative to wherever the app happens to be", which is not his PATH.
fn all_in_path(name: &str, path: &str) -> Vec<PathBuf> {
    path.split(':').filter(|dir| dir.starts_with('/')).map(|dir| Path::new(dir).join(name))
        .filter(|candidate| is_executable_file(candidate)).collect()
}

fn find_in_path(name: &str, path: &str) -> Option<PathBuf> {
    all_in_path(name, path).into_iter().next()
}

/// Apple's developer-tools shim. It runs the real interpreter through `xcrun`, and `xcrun`
/// adds `CPATH`, `LIBRARY_PATH`, `MANPATH` and `SDKROOT` to the environment of what it runs
/// (measured 2026-09-24 on this Mac, `env -i /usr/bin/python3`). The supervisor passes its
/// environment to his lead, and `SDKROOT` changes how every build his team runs behaves.
pub const XCRUN_PYTHON_SHIM: &str = "/usr/bin/python3";

/// Names any Python adds to its own environment on macOS, whatever it was started with:
/// `__CF_USER_TEXT_ENCODING` (CoreFoundation) always, and `LC_CTYPE` when `LANG` is unset
/// (PEP 538 locale coercion). Measured 2026-09-24 against Homebrew's 3.14 and Xcode's
/// `python3` under `env -i`. They are the interpreter's, not the app's, and P8 records what
/// actually reaches the lead's tool shell.
pub const INTERPRETER_ADDED_NAMES: [&str; 2] = ["__CF_USER_TEXT_ENCODING", "LC_CTYPE"];

/// The interpreter the supervisor runs under: the first `python3` on his stored PATH that is
/// not the `xcrun` shim; if the shim is the only one, the interpreter behind it
/// (`xcrun --find python3`), run directly so nothing is added.
fn resolve_python(path: &str) -> Option<PathBuf> {
    let all = all_in_path("python3", path);
    if let Some(direct) = all.iter().find(|p| p.as_path() != Path::new(XCRUN_PYTHON_SHIM)) {
        return Some(direct.clone());
    }
    if all.is_empty() {
        return None;
    }
    let found = std::process::Command::new("/usr/bin/xcrun").args(["--find", "python3"])
        .env_clear().stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .output().ok().filter(|o| o.status.success())?;
    let real = PathBuf::from(String::from_utf8_lossy(&found.stdout).trim());
    (real.is_absolute() && real != Path::new(XCRUN_PYTHON_SHIM) && is_executable_file(&real)).then_some(real)
}

impl OperatorProfile {
    pub fn new(declaration: Declaration, session: SessionEnvironment, lead_claim: &str) -> Result<Self, Refusal> {
        let refuse = |what: &str| Err(Refusal { what: what.to_string() });
        if !session.ssh_auth_sock.starts_with('/') {
            return refuse("SSH_AUTH_SOCK could not be derived for your team's session");
        }
        if !session.tmpdir.starts_with('/') {
            return refuse("TMPDIR could not be derived for your team's session");
        }
        if lead_claim.is_empty() || lead_claim.contains('\0') {
            return refuse("the app has no claim for this lead");
        }
        let path = declaration.environment.get("PATH").cloned().unwrap_or_default();
        let Some(python) = resolve_python(&path) else {
            return refuse("no python3 is on the PATH stored for your team");
        };
        // HIS claude: the one his terminal resolves, from his stored PATH, then the
        // installer's launcher in his home. Never the app's own resolution, which looks in the
        // app's HOME, a scratch folder under a nightly.
        let Some(claude) = find_in_path("claude", &path)
            .or_else(|| Some(declaration.home.join(".local/bin/claude")).filter(|p| p.is_file()))
        else {
            return refuse("no claude is on the PATH stored for your team");
        };
        let machine_settings = crate::machine_settings::Settings::load(&declaration.home, &declaration.environment)
            .map_err(|what| Refusal { what })?;
        Ok(OperatorProfile { declaration, session, lead_claim: lead_claim.to_string(), python, claude,
                             supervisor_files: None, machine_settings })
    }

    /// Give the supervisor its operator log and its descendant snapshot file (r3 (q) item 2,
    /// G8). Both are absolute paths in this install's own operator folder.
    pub fn with_supervisor_files(mut self, log: &Path, state: &Path) -> Self {
        self.supervisor_files = Some((log.to_path_buf(), state.to_path_buf()));
        self
    }

    pub fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// The whole environment the supervisor is started with. Exactly this set of names.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut environment = self.declaration.environment.clone();
        environment.extend(self.machine_settings.environment());
        environment.insert("HOME".into(), self.declaration.home.display().to_string());
        environment.insert("SSH_AUTH_SOCK".into(), self.session.ssh_auth_sock.clone());
        environment.insert("TMPDIR".into(), self.session.tmpdir.clone());
        environment.insert(LEAD_CLAIM_ENV.into(), self.lead_claim.clone());
        environment.insert(ARTIFACT_ENV.0.into(), ARTIFACT_ENV.1.into());
        if let Some((log, state)) = &self.supervisor_files {
            environment.insert(REAP_LOG_ENV.into(), log.display().to_string());
            environment.insert(REAP_STATE_ENV.into(), state.display().to_string());
        }
        environment
    }

    /// The `claude` arguments, in order. `mcp_config` is [`mcp_config`]'s value.
    ///
    /// Two of these flags take several values (`--disallowed-tools`, `--mcp-config`), so each
    /// is followed by another flag or ends the line: nothing after them can be swallowed.
    pub fn child_args(&self, start: &LeadStart, mcp_config: &Value) -> Vec<String> {
        let mut args: Vec<String> = [
            "--print",
            "--input-format=stream-json",
            "--output-format=stream-json",
            "--include-partial-messages",
            "--verbose",
            // (r): his engine's alarms arrive as hook frames.
            "--include-hook-events",
            // (c): the CLI echoes each message it takes, by uuid, so a turn is attributed to the
            // handle whose message started it (P13 read queue order this way).
            "--replay-user-messages",
            // (b): his settings, his CLAUDE.md, his plugin registration, as in his terminal.
            "--setting-sources",
            "user,project,local",
        ].iter().map(|s| s.to_string()).collect();
        // (k): no --no-session-persistence; (l): a resumed lead keeps its own session.
        match start {
            LeadStart::New(id) => args.extend(["--session-id".to_string(), id.clone()]),
            LeadStart::Resume(id) => args.extend(["--resume".to_string(), id.clone()]),
        }
        // (h): the declared mode, and the permission route to the desk kept as a safety net.
        args.extend([crate::native::PERMISSION_PROMPT_TOOL.to_string(), "stdio".to_string()]);
        if self.declaration.permission_mode == "bypassPermissions" {
            // Exactly what his terminal runs (r2 (h)).
            args.push("--dangerously-skip-permissions".into());
        } else {
            args.extend(["--permission-mode".to_string(), self.declaration.permission_mode.clone()]);
        }
        // (n): nobody is at a terminal to answer it; questions go through report(question).
        args.extend(["--disallowed-tools".to_string(), DISALLOWED_TOOL.to_string()]);
        // (a): the operator contract, whose digest the gate verified.
        args.extend([crate::native::APPEND_SYSTEM_PROMPT_FILE.to_string(),
                     self.declaration.contract_path.display().to_string()]);
        // (c): added to his own MCP servers, never instead of them (no --strict-mcp-config).
        args.extend(["--mcp-config".to_string(), mcp_config.to_string()]);
        args
    }

    /// The supervisor invocation: `python3 -B <his engine>/scripts/provider-supervisor.py
    /// --reap-descendants <claude> <args…>`, from his entity folder, built from empty.
    ///
    /// The caller owns the process group (`owned_process::OwnedChild::configure`) and the
    /// stdio; this sets what the lead IS, not how the host holds it.
    pub fn command(&self, start: &LeadStart, mcp_config: &Value) -> std::process::Command {
        let mut command = std::process::Command::new(&self.python);
        command
            .arg("-B")
            .arg(self.declaration.engine_root.join("scripts/provider-supervisor.py"))
            .arg(REAP_DESCENDANTS)
            .arg(&self.claude)
            .args(self.child_args(start, mcp_config))
            .current_dir(&self.declaration.entity_root)
            // r3 §11 item 3: from EMPTY. Never the app's environment with names removed.
            .env_clear()
            .envs(self.environment());
        command
    }
}

/// The `--mcp-config` value that gives the lead the `richos_operator` server: this app's own
/// executable, run as `--operator-mcp <scope>` (c).
pub fn mcp_config(executable: &Path, scope: &Path) -> Value {
    json!({"mcpServers": {REPORT_SERVER: {
        "type": "stdio", "command": executable, "args": ["--operator-mcp", scope]}}})
}

/// The server name the report tool is qualified under (`mcp__<server>__report`).
pub const REPORT_SERVER: &str = "richos_operator";

/// The engine's own `SessionStart` banner.
///
/// **The person-facing form is the short one**, and it is the one on the wire as a
/// `systemMessage`: `RichOS engine <v>: ENFORCEMENT ACTIVE for <entity> (<n>/<m> guards, engine
/// at <root>, root via <how>).` (`engine-status.sh`'s `emit_context <model-summary>
/// <operator-line>`; measured in the operator probes, P1 and P10, 2.1.282). The long form r2
/// and r3 quote, `RichOS engine <v> ACTIVE. Engine: <root>. Governing: …`, is the MODEL's
/// `additionalContext`; a transcript carries both, which is how a grep of one found the long
/// one. Both are read here, so the check does not depend on which channel carried it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineBanner {
    pub version: String,
    pub engine_root: PathBuf,
    pub governing: PathBuf,
    pub guard_count: u32,
    pub guard_expected: u32,
}

/// Read the banner out of one alarm's text, or `None` when this text is not the ACTIVE banner.
pub fn parse_banner(text: &str) -> Option<EngineBanner> {
    let rest = text.trim_start().strip_prefix("RichOS engine ")?;
    // The person's line: `<v>: ENFORCEMENT ACTIVE for <entity> (<n>/<m> guards, engine at
    // <root>, root via <how>).`
    if let Some((version, rest)) = rest.split_once(": ENFORCEMENT ACTIVE for ") {
        if version.contains(char::is_whitespace) {
            return None;
        }
        let (governing, rest) = rest.split_once(" (")?;
        let (counts, rest) = rest.split_once(" guards, engine at ")?;
        let (engine_root, _) = rest.split_once(", root via ")?;
        let (count, expected) = counts.split_once('/')?;
        return Some(EngineBanner {
            version: version.to_string(),
            engine_root: PathBuf::from(engine_root),
            governing: PathBuf::from(governing),
            guard_count: count.parse().ok()?,
            guard_expected: expected.parse().ok()?,
        });
    }
    // The model's summary: `<v> ACTIVE. Engine: <root>. Governing: <entity> (resolved via
    // <how>). <n>/<m> guards present …`
    let (version, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix("ACTIVE. Engine: ")?;
    let (engine_root, rest) = rest.split_once(". Governing: ")?;
    let (governing, rest) = rest.split_once(" (resolved via ")?;
    let counts = rest.split(" guards present").next()?.rsplit(' ').next()?;
    let (count, expected) = counts.split_once('/')?;
    Some(EngineBanner {
        version: version.to_string(),
        engine_root: PathBuf::from(engine_root),
        governing: PathBuf::from(governing),
        guard_count: count.parse().ok()?,
        guard_expected: expected.parse().ok()?,
    })
}

/// What the init check decided for one lead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitVerdict {
    /// The lead may take work. `announce` is the one sentence about an unmeasured `claude`.
    Open { announce: Option<String> },
    Refuse(Refusal),
}

/// The init check (r3 (b)): run once, on the lead's `system/init` frame and the banner read
/// from its hook frames, with the fence status command's answer. Pure, so every condition is
/// a table row in its tests.
pub fn init_check(declaration: &Declaration, init: &Value, banner: Option<&EngineBanner>,
                  fences: &Result<(), String>) -> InitVerdict {
    let refuse = |what: String| InitVerdict::Refuse(Refusal { what });
    let plugin_count = |name: &str| init.get("plugins").and_then(Value::as_array)
        .map_or(0, |plugins| plugins.iter().filter(|p| p.get("name").and_then(Value::as_str) == Some(name)).count());
    match plugin_count(HIS_ENGINE_PLUGIN) {
        1 => {}
        0 => return refuse(format!("his engine plugin {HIS_ENGINE_PLUGIN} did not load in the lead")),
        n => return refuse(format!("his engine plugin {HIS_ENGINE_PLUGIN} loaded {n} times in the lead")),
    }
    if plugin_count(crate::engine_profile::PLUGIN_NAME) > 0 {
        return refuse(format!("the app's own engine ({}) loaded beside his", crate::engine_profile::PLUGIN_NAME));
    }
    let has_report = init.get("tools").and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|t| t.as_str() == Some(REPORT_TOOL)));
    if !has_report {
        return refuse("the lead did not get the report tool it tells you things with".into());
    }
    let Some(banner) = banner else {
        return refuse("his engine's banner never said it was active in the lead".into());
    };
    let cache: PathBuf = [".claude", "plugins", "cache"].iter().collect();
    let in_cache = banner.engine_root.components().collect::<Vec<_>>().windows(3)
        .any(|w| w.iter().map(|c| c.as_os_str()).eq(cache.components().map(|c| c.as_os_str())));
    if in_cache || banner.engine_root != declaration.engine_root {
        return refuse(format!("the lead ran the engine at {}, not the one declared at {}",
                              banner.engine_root.display(), declaration.engine_root.display()));
    }
    if banner.guard_count != banner.guard_expected {
        return refuse(format!("his engine reported only {}/{} guards present",
                              banner.guard_count, banner.guard_expected));
    }
    if let Err(why) = fences {
        return refuse(format!("the land fences are not in place ({why})"));
    }
    // The field is `claude_code_version` on the wire (the 2.1.282 binary's init frame
    // builder); r2 and r3 call it `system/init.version`.
    let version = init.get("claude_code_version").or_else(|| init.get("version"))
        .and_then(Value::as_str).unwrap_or("unknown");
    let announce = (!MEASURED_CLAUDE_VERSIONS.contains(&version)).then(|| format!(
        "Your team is running on Claude Code {version}, a version Rich hasn't checked this setup against yet."));
    InitVerdict::Open { announce }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator_declaration::{ClaimPaths, REQUIRED_ENGINE_FILES};

    struct Fixture {
        root: PathBuf,
        declaration: Declaration,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[cfg(unix)]
    fn executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        write(path, body);
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A declaration as the gate would hand it over. `bin` holds a fake `claude` and the
    /// stored PATH puts it first, before the system's own tools.
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("operator-profile-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        let home = root.join("home");
        let entity = home.join("ab/femcboost");
        let engine = home.join("ab/richos/richos/engine");
        for file in REQUIRED_ENGINE_FILES {
            write(&engine.join(file), "fixture\n");
        }
        std::fs::create_dir_all(&entity).unwrap();
        let bin = root.join("bin");
        #[cfg(unix)]
        executable(&bin.join("claude"), "#!/bin/sh\nexec /usr/bin/env\n");
        let contract = home.join("ab/operator-contract.md");
        write(&contract, "The operator contract.\n");
        let mut environment = BTreeMap::new();
        environment.insert("PATH".to_string(), format!("{}:/usr/bin:/bin", bin.display()));
        environment.insert("LANG".to_string(), "en_US.UTF-8".to_string());
        environment.insert("USER".to_string(), "fixture".to_string());
        environment.insert("GIT_EDITOR".to_string(), "vi".to_string());
        let declaration = Declaration {
            entity_root: entity,
            engine_root: engine,
            home: home.clone(),
            claim: ClaimPaths { file: home.join(".claude/state/operator-lead.json"),
                                lock: home.join(".claude/state/operator-lead.lock") },
            permission_mode: "bypassPermissions".into(),
            environment,
            origins: vec!["desk-typed".into()],
            file_roots: vec![home.join("ab")],
            contract_path: contract,
            contract_sha256: "0".repeat(64),
        };
        Fixture { root, declaration }
    }

    fn session() -> SessionEnvironment {
        SessionEnvironment { ssh_auth_sock: "/private/tmp/com.apple.launchd.fixture/Listeners".into(),
                             tmpdir: "/var/folders/fx/fixture/T/".into() }
    }

    fn profile(f: &Fixture) -> OperatorProfile {
        OperatorProfile::new(f.declaration.clone(), session(), "claim-1").expect("a valid profile")
    }

    // ---- (i): the environment, from empty, exactly this set ------------------------------

    #[test]
    fn the_environment_is_exactly_the_allowlist_plus_the_four_the_app_supplies() {
        let f = fixture();
        let env = profile(&f).environment();
        let names: Vec<&str> = env.keys().map(String::as_str).collect();
        assert_eq!(names, ["CLAUDE_CODE_ARTIFACT", "GIT_EDITOR", "HOME", "LANG", "PATH", "RICHOS_MACHINE_SETTINGS_RESOLVED", "RICHOS_OPERATOR_LEAD", "SSH_AUTH_SOCK",
                           "TMPDIR", "USER"]);
        assert_eq!(env["CLAUDE_CODE_ARTIFACT"], "1", "r4 §2.3, P17");
        assert_eq!(env["HOME"], f.declaration.home.display().to_string());
        assert_eq!(env["SSH_AUTH_SOCK"], session().ssh_auth_sock);
        assert_eq!(env["TMPDIR"], session().tmpdir);
        assert_eq!(env[LEAD_CLAIM_ENV], "claim-1");
        assert_eq!(env["PATH"], f.declaration.environment["PATH"]);
    }

    /// The whole chain, run: the command this profile builds, under a supervisor that does
    /// what the real one does to the environment (adds `RICHOS_SESSION_PID`, then execs), with a
    /// fake `claude` that prints what it received. This test process carries dozens of
    /// variables of its own (`CARGO_*`, its own `HOME`, `PATH`), and none may arrive.
    #[cfg(unix)]
    #[test]
    fn the_lead_receives_exactly_that_set_and_nothing_of_the_app_s_own_environment() {
        let f = fixture();
        let out = f.root.join("lead-env.txt");
        write(&f.declaration.engine_root.join("scripts/provider-supervisor.py"), &format!(
            "import os, sys\n\
             assert sys.argv[1] == '--reap-descendants', sys.argv\n\
             os.environ['RICHOS_SESSION_PID'] = str(os.getpid())\n\
             fd = os.open('{}', os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)\n\
             os.dup2(fd, 1)\n\
             os.execvp(sys.argv[2], sys.argv[2:])\n", out.display()));
        let p = profile(&f);
        let status = p.command(&LeadStart::New("session-1".into()), &mcp_config(Path::new("/fixture/RichOS"), Path::new("/fixture/scope.json")))
            .stdin(std::process::Stdio::null()).status().unwrap();
        assert!(status.success(), "the fixture chain failed: {status}");
        let text = std::fs::read_to_string(&out).unwrap();
        let mut names: Vec<&str> = text.lines().filter_map(|l| l.split_once('=').map(|(n, _)| n))
            // What the shell adds for itself, and what the interpreter adds for itself.
            .filter(|n| !["PWD", "OLDPWD", "SHLVL", "_"].contains(n) && !INTERPRETER_ADDED_NAMES.contains(n))
            .collect();
        names.sort();
        assert_eq!(names, ["CLAUDE_CODE_ARTIFACT", "GIT_EDITOR", "HOME", "LANG", "PATH", "RICHOS_MACHINE_SETTINGS_RESOLVED", "RICHOS_OPERATOR_LEAD",
                           "RICHOS_SESSION_PID", "SSH_AUTH_SOCK", "TMPDIR", "USER"], "{text}");
        // The fixture's PATH offers only the xcrun shim; the profile must have gone around it.
        for shim_name in ["SDKROOT", "CPATH", "LIBRARY_PATH", "MANPATH"] {
            assert!(!text.lines().any(|l| l.starts_with(&format!("{shim_name}="))), "{shim_name} reached the lead: {text}");
        }
    }

    #[test]
    fn the_supervisor_files_are_the_only_other_names_and_only_when_given() {
        let f = fixture();
        let log = f.root.join("operator/operator.log");
        let state = f.root.join("operator/t/reap-state.json");
        let env = profile(&f).with_supervisor_files(&log, &state).environment();
        let names: Vec<&str> = env.keys().map(String::as_str).collect();
        assert_eq!(names, ["CLAUDE_CODE_ARTIFACT", "GIT_EDITOR", "HOME", "LANG", "PATH", "RICHOS_MACHINE_SETTINGS_RESOLVED", "RICHOS_OPERATOR_LEAD",
                           REAP_LOG_ENV, REAP_STATE_ENV, "SSH_AUTH_SOCK", "TMPDIR", "USER"]);
        assert_eq!(env[REAP_LOG_ENV], log.display().to_string());
        assert_eq!(env[REAP_STATE_ENV], state.display().to_string());
        assert!(!crate::operator_declaration::is_denied_name(REAP_LOG_ENV) && !crate::operator_declaration::is_denied_name(REAP_STATE_ENV));
        assert!(crate::operator_declaration::is_denied_name(ARTIFACT_ENV.0),
                "never STORED: the stored-environment rule still refuses it; only the app sets it");
    }

    #[test]
    fn the_supervisor_never_runs_under_the_xcrun_shim_when_another_python_is_there() {
        let f = fixture();
        let other = f.root.join("brew/bin");
        #[cfg(unix)]
        executable(&other.join("python3"), "#!/bin/sh\nexit 0\n");
        assert_eq!(resolve_python(&format!("/usr/bin:{}", other.display())), Some(other.join("python3")));
        assert_eq!(resolve_python(&f.root.join("none").display().to_string()), None);
    }

    #[test]
    fn a_value_that_cannot_be_derived_refuses_the_lead() {
        let f = fixture();
        for bad in [SessionEnvironment { ssh_auth_sock: String::new(), tmpdir: "/tmp/".into() },
                    SessionEnvironment { ssh_auth_sock: "relative".into(), tmpdir: "/tmp/".into() },
                    SessionEnvironment { ssh_auth_sock: "/s".into(), tmpdir: String::new() }] {
            assert!(OperatorProfile::new(f.declaration.clone(), bad, "claim").is_err());
        }
        assert!(OperatorProfile::new(f.declaration.clone(), session(), "").is_err(), "a lead with no claim id");
    }

    #[test]
    fn a_path_with_no_python3_or_no_claude_refuses_the_lead() {
        let f = fixture();
        let mut d = f.declaration.clone();
        d.environment.insert("PATH".into(), f.root.join("empty").display().to_string());
        let refusal = OperatorProfile::new(d, session(), "claim").unwrap_err();
        assert!(refusal.what.contains("python3") || refusal.what.contains("claude"), "{}", refusal.what);
    }

    #[cfg(unix)]
    #[test]
    fn the_session_values_come_from_launchd_and_the_os_never_from_this_process() {
        let f = fixture();
        let launchctl = f.root.join("launchctl");
        executable(&launchctl, "#!/bin/sh\n[ \"$1 $2\" = 'getenv SSH_AUTH_SOCK' ] || exit 9\necho /private/tmp/com.apple.launchd.x/Listeners\n");
        let derived = SessionEnvironment::derive_with(&launchctl).unwrap();
        assert_eq!(derived.ssh_auth_sock, "/private/tmp/com.apple.launchd.x/Listeners");
        assert!(derived.tmpdir.starts_with('/') && derived.tmpdir.ends_with('/'), "{}", derived.tmpdir);
        executable(&launchctl, "#!/bin/sh\nexit 0\n");
        let refusal = SessionEnvironment::derive_with(&launchctl).unwrap_err();
        assert!(refusal.what.contains("SSH_AUTH_SOCK"), "{}", refusal.what);
    }

    // ---- (b), (h), (j), (k), (n): the arguments ------------------------------------------

    fn args(start: LeadStart) -> Vec<String> {
        let f = fixture();
        profile(&f).child_args(&start, &mcp_config(Path::new("/fixture/RichOS"), Path::new("/fixture/scope.json")))
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
    }

    #[test]
    fn his_settings_load_natively_and_the_app_engine_is_never_offered() {
        let a = args(LeadStart::New("s-1".into()));
        assert_eq!(value_after(&a, "--setting-sources"), Some("user,project,local"));
        assert!(!a.iter().any(|x| x == "--plugin-dir"), "{a:?}");
        assert!(!a.iter().any(|x| x == "--strict-mcp-config"), "his own MCP servers stay: {a:?}");
        assert!(!a.iter().any(|x| x == "--no-session-persistence"), "(k): the lead persists: {a:?}");
    }

    #[test]
    fn the_wire_is_stream_json_with_hook_events_and_the_permission_route_kept() {
        let a = args(LeadStart::New("s-1".into()));
        for flag in ["--print", "--input-format=stream-json", "--output-format=stream-json", "--verbose",
                     "--include-hook-events", "--replay-user-messages", "--dangerously-skip-permissions"] {
            assert!(a.iter().any(|x| x == flag), "{flag} missing: {a:?}");
        }
        assert_eq!(value_after(&a, crate::native::PERMISSION_PROMPT_TOOL), Some("stdio"));
        assert_eq!(value_after(&a, "--disallowed-tools"), Some(DISALLOWED_TOOL));
        assert_eq!(value_after(&a, "--session-id"), Some("s-1"));
        assert!(!a.iter().any(|x| x == "--resume"));
    }

    #[test]
    fn a_resumed_lead_names_its_last_session_and_takes_no_new_id() {
        let a = args(LeadStart::Resume("s-0".into()));
        assert_eq!(value_after(&a, "--resume"), Some("s-0"));
        assert!(!a.iter().any(|x| x == "--session-id"));
    }

    #[test]
    fn a_declared_mode_other_than_bypass_is_passed_as_a_permission_mode() {
        let f = fixture();
        let mut d = f.declaration.clone();
        d.permission_mode = "acceptEdits".into();
        let p = OperatorProfile::new(d, session(), "claim").unwrap();
        let a = p.child_args(&LeadStart::New("s".into()), &json!({}));
        assert_eq!(value_after(&a, "--permission-mode"), Some("acceptEdits"));
        assert!(!a.iter().any(|x| x == "--dangerously-skip-permissions"));
    }

    #[test]
    fn the_contract_and_the_report_server_are_the_last_things_on_the_line() {
        let f = fixture();
        let config = mcp_config(Path::new("/fixture/RichOS"), Path::new("/fixture/scope.json"));
        let a = profile(&f).child_args(&LeadStart::New("s".into()), &config);
        assert_eq!(value_after(&a, crate::native::APPEND_SYSTEM_PROMPT_FILE),
                   Some(f.declaration.contract_path.to_str().unwrap()));
        // `--mcp-config` takes several values; nothing may follow it and be swallowed.
        assert_eq!(a[a.len() - 2], "--mcp-config");
        let parsed: Value = serde_json::from_str(&a[a.len() - 1]).unwrap();
        assert_eq!(parsed, config);
        assert_eq!(config["mcpServers"]["richos_operator"]["command"], "/fixture/RichOS");
        assert_eq!(config["mcpServers"]["richos_operator"]["args"], json!(["--operator-mcp", "/fixture/scope.json"]));
    }

    #[test]
    fn the_supervisor_is_his_engine_s_with_the_reap_flag_seated_in_his_entity() {
        let f = fixture();
        let p = profile(&f);
        let command = p.command(&LeadStart::New("s".into()), &json!({}));
        let argv: Vec<String> = command.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(command.get_program().to_string_lossy().ends_with("python3"), "{:?}", command.get_program());
        assert_eq!(argv[0], "-B", "no bytecode is written into his engine checkout");
        assert_eq!(Path::new(&argv[1]), f.declaration.engine_root.join("scripts/provider-supervisor.py"));
        assert_eq!(argv[2], REAP_DESCENDANTS);
        assert_eq!(Path::new(&argv[3]), f.root.join("bin/claude"), "claude from HIS stored PATH");
        assert_eq!(argv[4..], p.child_args(&LeadStart::New("s".into()), &json!({}))[..]);
        assert_eq!(command.get_current_dir(), Some(f.declaration.entity_root.as_path()));
    }

    #[test]
    fn the_product_supervisor_reaps_too_with_its_parameters_on_its_own_command_line() {
        // N1, split by the product reap gap design (1.5): clause 1 (no declaration → the
        // product path, `claude`'s arguments and environment unchanged) stands and is the
        // engine's R11; clause 2 ("the product supervisor runs without the flag") is retired.
        // The product now names the same flag, and its reap parameters are arguments, never
        // the environment names this profile sets for the operator.
        let args = crate::lease_commands::supervisor_args(Path::new("/e"), Path::new("/s.json"), Path::new("/r.log"));
        assert_eq!(args[1].to_str(), Some(REAP_DESCENDANTS));
        assert!(args.iter().all(|a| !a.to_string_lossy().contains(REAP_LOG_ENV) && !a.to_string_lossy().contains(REAP_STATE_ENV)));
        let native = include_str!("native.rs");
        assert!(native.contains("crate::lease_commands::supervisor_args("), "the product lease no longer builds the reaping invocation");
        assert!(!native.contains(REAP_STATE_ENV) && !native.contains(REAP_LOG_ENV), "the product put a reap parameter in the environment");
    }

    // ---- (b): the init check -------------------------------------------------------------

    fn banner(f: &Fixture) -> EngineBanner {
        EngineBanner { version: "1.2.0".into(), engine_root: f.declaration.engine_root.clone(),
                       governing: f.declaration.entity_root.clone(), guard_count: 26, guard_expected: 26 }
    }

    fn init(plugins: &[&str], tools: &[&str], version: &str) -> Value {
        json!({"type":"system","subtype":"init",
               "plugins": plugins.iter().map(|n| json!({"name": n, "path": "/x"})).collect::<Vec<_>>(),
               "tools": tools, "claude_code_version": version})
    }

    fn refused(verdict: InitVerdict, needle: &str) {
        match verdict {
            InitVerdict::Refuse(r) => assert!(r.what.contains(needle), "{:?} does not name {needle:?}", r.what),
            other => panic!("expected a refusal naming {needle:?}, got {other:?}"),
        }
    }

    #[test]
    fn a_lead_with_his_engine_once_the_report_tool_the_banner_and_the_fences_opens() {
        let f = fixture();
        let verdict = init_check(&f.declaration, &init(&["richos-engine"], &[REPORT_TOOL], "9.9.9"),
                                 Some(&banner(&f)), &Ok(()));
        let InitVerdict::Open { announce } = verdict else { panic!("{verdict:?}") };
        let announce = announce.expect("an unmeasured version is announced");
        assert!(announce.contains("9.9.9"), "{announce}");
    }

    #[test]
    fn every_init_condition_refuses_by_name() {
        let f = fixture();
        let good_banner = banner(&f);
        let ok = Ok(());
        refused(init_check(&f.declaration, &init(&[], &[REPORT_TOOL], "1"), Some(&good_banner), &ok), "richos-engine");
        refused(init_check(&f.declaration, &init(&["richos-engine", "richos-engine"], &[REPORT_TOOL], "1"),
                           Some(&good_banner), &ok), "richos-engine");
        refused(init_check(&f.declaration, &init(&["richos-engine", "richos-app-engine"], &[REPORT_TOOL], "1"),
                           Some(&good_banner), &ok), "richos-app-engine");
        refused(init_check(&f.declaration, &init(&["richos-engine"], &[], "1"), Some(&good_banner), &ok), "report");
        refused(init_check(&f.declaration, &init(&["richos-engine"], &[REPORT_TOOL], "1"), None, &ok), "banner");
        let mut other_root = banner(&f);
        other_root.engine_root = PathBuf::from("/Users/x/.claude/plugins/cache/richos-local/richos-engine/1.0.0");
        refused(init_check(&f.declaration, &init(&["richos-engine"], &[REPORT_TOOL], "1"), Some(&other_root), &ok), "engine");
        let mut short = banner(&f);
        short.guard_count = 25;
        refused(init_check(&f.declaration, &init(&["richos-engine"], &[REPORT_TOOL], "1"), Some(&short), &ok), "25/26");
        refused(init_check(&f.declaration, &init(&["richos-engine"], &[REPORT_TOOL], "1"), Some(&good_banner),
                           &Err("femcboost is not fenced".into())), "femcboost is not fenced");
    }

    #[test]
    fn a_measured_version_opens_silently() {
        let f = fixture();
        for version in MEASURED_CLAUDE_VERSIONS {
            let verdict = init_check(&f.declaration, &init(&["richos-engine"], &[REPORT_TOOL], version),
                                     Some(&banner(&f)), &Ok(()));
            assert_eq!(verdict, InitVerdict::Open { announce: None });
        }
    }

    #[test]
    fn the_banner_is_read_off_the_engine_s_own_words() {
        let text = "RichOS engine 1.2.0 ACTIVE. Engine: /Users/alex/ab/richos/richos/engine. Governing: /Users/alex/ab/femcboost (resolved via CLAUDE_PROJECT_DIR). 26/26 guards present (denominator derived from the engine's hooks/hooks.json registration; the status announcer itself is not counted among the guards). Enforcement is ON for this repository.";
        let b = parse_banner(text).expect("the ACTIVE banner parses");
        assert_eq!(b.version, "1.2.0");
        assert_eq!(b.engine_root, PathBuf::from("/Users/alex/ab/richos/richos/engine"));
        assert_eq!(b.governing, PathBuf::from("/Users/alex/ab/femcboost"));
        assert_eq!((b.guard_count, b.guard_expected), (26, 26));
        assert_eq!(parse_banner("RichOS engine 1.2.0 loaded but STOOD DOWN — this repository has NOT adopted it. Engine: /e."), None);
        assert_eq!(parse_banner("RichOS engine 1.2.0: STOOD DOWN — this repository has not adopted the engine, so NONE of its 77/77 guards will enforce anything in this session."), None);
        assert_eq!(parse_banner("RichOS engine 1.2.0: ROOT RESOLUTION FAILURE — ENFORCEMENT IS NOT ACTIVE. why"), None);
        assert_eq!(parse_banner("Stop says: hello"), None);
    }

    /// **The line that actually arrives as a `systemMessage`**, verbatim from the operator
    /// probes (P1, 2.1.282, guest fixture paths). It is the SHORT form: `engine-status.sh`'s
    /// `emit_context <model-summary> <operator-line>` sends the long form to the model only.
    #[test]
    fn the_person_facing_banner_measured_on_the_wire_is_read() {
        let measured = "RichOS engine 1.2.0: ENFORCEMENT ACTIVE for /Users/admin/testvm/probes-412d7918db8e/home/ab/femcboost (77/77 guards, engine at /Users/admin/testvm/probes-412d7918db8e/home/ab/richos/richos/engine, root via project-dir). Engine HEAD not-a-git-checkout (this is the path that RUNS; an installation record naming another path is not evidence until something is shown to read it).";
        let b = parse_banner(measured).expect("the operator line is the banner");
        assert_eq!(b.version, "1.2.0");
        assert_eq!(b.engine_root, PathBuf::from("/Users/admin/testvm/probes-412d7918db8e/home/ab/richos/richos/engine"));
        assert_eq!(b.governing, PathBuf::from("/Users/admin/testvm/probes-412d7918db8e/home/ab/femcboost"));
        assert_eq!((b.guard_count, b.guard_expected), (77, 77));
    }
    #[test]
    fn storage_comes_from_the_declared_home_and_preserves_env_clear() {
        let f = fixture();
        let source = crate::machine_settings::file(&f.declaration.home);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, "RICHOS_BULK_ROOT=/operator storage\n").unwrap();
        let p = profile(&f);
        assert_eq!(p.environment()["RICHOS_BULK_ROOT"], "/operator storage");
        std::fs::write(&source, "UNKNOWN=/edited\n").unwrap();
        assert_eq!(p.environment()["RICHOS_BULK_ROOT"], "/operator storage");
        assert!(OperatorProfile::new(f.declaration.clone(), session(), "claim").is_err());
    }

}
