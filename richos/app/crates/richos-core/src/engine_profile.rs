//! Explicit desktop delivery. Installed code, private coordination and target
//! repositories are separate roots. No terminal settings or roster is adopted.
use crate::runtime::{EngineRuntime, RuntimeError};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The name the engine plugin is rendered under, and therefore the exact string the child
/// announces it by in `system/init.plugins`.
///
/// **One spelling, because two would drift silently and in the direction that matters.** It
/// is written into the manifest here and asserted against the wire in `native.rs`, which is
/// the same two-places-one-fact problem `skills.rs::PLUGIN_NAME` already solved for the
/// skills plugin — and `skills.rs`'s own test says why: a name that disagrees with itself
/// produces a readiness check that can only ever answer "absent", against a plugin that is
/// sitting right there. That is not hypothetical here: a readiness check answering a false
/// absence is exactly what refused every background job on 2026-09-18.
pub const PLUGIN_NAME: &str = "richos-app-engine";

/// The engine file that says whose work each spawn guard's reason protects.
pub const GUARD_AUDIENCE_DECLARATION: &str = "spawn-guard-audience.declaration";

/// **The engine's Bash-chain rules every lease runs** (gap E), out of
/// `scripts/hooks/dispatch-pretooluse.manifest`, through the engine's own dispatcher so a call
/// that names none of a rule's words does not start it. Each reads its contract from the
/// repository being written to (`.publication-boundary`) or from the user's own files, so it
/// does nothing for a user who has declared nothing. The rest of the manifest polices the
/// terminal operator's own sessions (his claim, his worktrees and lands, his record) and is
/// left out; the claim hooks above all, which must never run in a user's app.
const APP_BASH_RULES: [&str; 7] = [
    "guard-publication-commits.sh",   // private material in a commit to a public repository
    "guard-named-persons-commands.sh", // a listed person's name in a commit message, branch or PR
    "guard-public-record-repo.sh",    // a plan or research record added to a public repository
    "guard-unguarded-rm.sh",          // Claude Code's own rm prompt, which bypass mode still asks
    "guard-host-sleep.sh",            // caffeinate or a sleep-setting change on the user's Mac
    "guard-headed-browser.sh",        // a visible browser window opened by automation
    "guard-toolchain.sh",             // an Apple developer-tools stub with no toolchain (install dialog)
];

/// **The engine's Write-chain rules every lease runs** (gap E), the same way. `dialect-guard.sh`
/// is beside them, registered on its own as in the terminal, and is on only where the user's
/// `orchestration.config` or the repository's own sets `DIALECT_TARGET`.
const APP_WRITE_RULES: [&str; 3] = [
    "scan-secrets.sh",                // a credential in a written file
    "guard-publication-writes.sh",    // private material written into a public repository
    "guard-named-persons-writes.sh",  // a listed person's name in a written file or its path
];

/// **The file Claude Code runs ahead of every Bash command of a lease** (its `CLAUDE_ENV_FILE`,
/// read by Claude Code 2.1.314 for Bash commands only, never for hooks): it sets the agent's
/// own `PATH` ([`EngineProfile::commands_path`]). Written into the lease's plugin folder.
pub const COMMANDS_ENV: &str = "commands-env.sh";

/// The engine's dispatcher command for one chain, running only `rules` of it.
fn app_rules(engine: &Path, chain: &str, rules: &[&str]) -> String {
    format!("/bin/bash {} {chain} {}", quote(&engine.join("scripts/hooks/dispatch-pretooluse.sh")), rules.join(" "))
}

/// **The guards that judge a dispatch the APP makes, read from the engine's own
/// declaration rather than typed here.**
///
/// The CEO's §57 is that RichOS is a free app for non-technical people and that nothing the
/// user runs depends on our operator setup; §55 is that the work happens after "On it!", and
/// that work is a background dispatch. Rich ruled on 2026-09-18 that such a dispatch is
/// judged only by guards whose reason protects the user's own work on the user's own Mac.
///
/// **This used to be two script paths typed into [`EngineProfile::prepare`] below**, with an
/// identical pair typed into `scripts/app-engine-hook.py`, and one of the two —
/// `guard-brief-scope.sh` — is an operator-session guard: it judges a dispatch against a
/// design-round specification recorded in the DEVELOPMENT project, and its way out is a line
/// only an operator can write. Nothing said why those two, and nothing tied the two copies
/// together. So the classification is data now, and this reads it.
///
/// Returns the `user-work` rows in declaration order. A declaration that is missing,
/// unreadable or self-contradictory is an ERROR and never an empty list: the app's guard
/// surface is supposed to be a declared list, and an unreadable list is not a list. An empty
/// result is refused for the same reason `spawn.py` refuses one — an unguarded spawn is not a
/// verified one, whoever it is for.
pub fn user_work_guards(engine: &Path) -> Result<Vec<String>, RuntimeError> {
    let path = engine.join(GUARD_AUDIENCE_DECLARATION);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| RuntimeError(format!("{GUARD_AUDIENCE_DECLARATION} could not be read: {e}")))?;
    let mut guards = Vec::new();
    // Blank-line-separated `key: value` records, `#` comments, one line per value — the same
    // shape `owned-systems.declaration` uses and `scripts/lib/spawn-guard-audience.py` parses.
    // Two readers of one file, by necessity (nothing may shell out on this path), so the
    // engine's own test drives the Python reader against the same file and
    // `tests/engine_profile.rs` asserts this one agrees with the shipped rows.
    for record in text.split("\n\n") {
        let (mut id, mut audience, mut message) = (None, None, None);
        for line in record.lines() {
            let line = line.trim_end();
            if line.starts_with('#') || line.trim().is_empty() { continue; }
            let Some((key, value)) = line.split_once(':') else {
                return Err(RuntimeError(format!(
                    "{GUARD_AUDIENCE_DECLARATION} carries a line that is not `key: value`: {line}")));
            };
            match key.trim() {
                "id" => id = Some(value.trim().to_string()),
                "audience" => audience = Some(value.trim().to_string()),
                "user_message" => message = Some(value.trim().to_string()),
                _ => {}
            }
        }
        match (id, audience) {
            (Some(id), Some(audience)) if audience == "user-work" => {
                // The same rule the Python reader enforces, enforced here too rather than
                // assumed: a guard the app runs carries the app's own words for its refusal.
                if message.is_none_or(|m| m.is_empty()) {
                    return Err(RuntimeError(format!(
                        "{GUARD_AUDIENCE_DECLARATION}: {id} is user-work and has no user_message")));
                }
                if id.contains('/') || id.contains("..") || !id.ends_with(".sh") {
                    return Err(RuntimeError(format!(
                        "{GUARD_AUDIENCE_DECLARATION}: {id} is not a guard file name")));
                }
                guards.push(id);
            }
            (Some(_), Some(_)) | (None, None) => {}
            _ => return Err(RuntimeError(format!(
                "{GUARD_AUDIENCE_DECLARATION} has a record with an id or an audience but not both"))),
        }
    }
    if guards.is_empty() {
        return Err(RuntimeError(format!(
            "{GUARD_AUDIENCE_DECLARATION} classifies no guard as user-work, so this dispatch \
             would be judged by nothing")));
    }
    Ok(guards)
}

#[derive(Clone, Debug)]
pub struct EngineProfile {
    pub engine: PathBuf,
    pub coordination: PathBuf,
    pub plugin: PathBuf,
    pub state: PathBuf,
    pub runtime: EngineRuntime,
    pub work_scope: Option<(String, String)>,
    /// **Fill-first: the Claude account this lease runs under.** Set by the shell from the
    /// account in use (`quota::Service::lease_account`). An added account's folder reaches
    /// the provider as `CLAUDE_CONFIG_DIR` in [`Self::configure`]; Account 1 has no folder and
    /// inherits the app's environment exactly as before. `None` everywhere else.
    pub claude_account: Option<crate::claude_accounts::Account>,
    /// **Where this lease's token use goes** (`quota::Service::token_sink`), counted under
    /// `claude_account`. Set by the shell beside it; `None` everywhere else.
    pub token_sink: Option<std::sync::Arc<crate::quota::tokens::Sink>>,
    /// **The speech model the video skill transcribes with** (media-tools plan section 3). Set
    /// by the shell through [`speech_model_for_video`]: the pinned large-v3-turbo-q5_0 that setup
    /// installs for transcription when it is installed and verified (the CEO, 2026-10-07: "Both,
    /// in this nightly, yes."), otherwise the model voice mode resolves. It reaches the provider
    /// as `RICHOS_SPEECH_MODEL` in [`Self::configure`]. `None` when neither is ready, and then
    /// nothing is set: the skill tells the CEO it watched the pictures only. Resolved by the shell
    /// because this crate does not depend on `richos-voice` (no native audio here).
    pub speech_model: Option<PathBuf>,
    /// **The model each session starts on** (the CEO's §119, 2026-10-10): read by
    /// [`Self::prepare`] from `config.json` in the data directory, so every lease spawn sees the
    /// setting as it is at that moment. `native.rs` gives the conversation lease (Rich) `rich`
    /// and the work lease (Stu) `stu`.
    pub lease_models: crate::config::LeaseModels,
    pub machine_settings: crate::machine_settings::Settings,
    /// **What this lease takes from the user's own settings files** (§138, build plan item 0):
    /// read by [`Self::prepare`] from the user's central folder, the same way for every user.
    /// Empty when prepared without a user folder ([`Self::prepare_with_settings`]).
    pub user: crate::user_settings::UserSetup,
}

/// **Account 1's Claude folder, the machine-wide home** a lease is given as `RICHOS_CLAUDE_HOME`:
/// the app's own `CLAUDE_CONFIG_DIR`, else `~/.claude`, exactly as a terminal resolves it.
pub(crate) fn machine_claude_home() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty()).map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").filter(|home| !home.is_empty()).map(|home| PathBuf::from(home).join(".claude"))
    })
}

/// **Which speech model the video skill transcribes with**, the value of
/// [`EngineProfile::speech_model`]. `transcription` is the pinned transcription model's own check
/// (`richos_voice::stt::model_verified(TRANSCRIPTION_MODEL_ID)` in the shell); `voice` is the
/// model voice mode resolves, asked only when the transcription model is not usable.
pub fn speech_model_for_video(transcription: Result<PathBuf, String>, voice: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    transcription.ok().or_else(voice)
}

fn quote(path: &Path) -> String { format!("'{}'", path.to_string_lossy().replace('\'', "'\\''")) }
fn write(path: &Path, body: &str) -> Result<(), RuntimeError> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|e| RuntimeError(e.to_string()))?; }
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path).and_then(|mut f| { f.write_all(body.as_bytes())?; f.sync_all() })
        .map_err(|e| RuntimeError(e.to_string()))
}
fn git(runtime: &EngineRuntime, cwd: &Path, args: &[&str]) -> Result<(), RuntimeError> {
    let output = Command::new(&runtime.git).args(["-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false",
        "-c", "user.name=RichOS", "-c", "user.email=richos@localhost"])
        .args(args).current_dir(cwd).env("PATH", runtime.path())
        .env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0").env_remove("GIT_CONFIG_PARAMETERS").env_remove("GIT_TEMPLATE_DIR")
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE")
        .output().map_err(|e| RuntimeError(e.to_string()))?;
    if !output.status.success() { return Err(RuntimeError("private coordination repository could not be initialized".into())); }
    Ok(())
}

/// **The named teammates every install has from its first lease** (proto-teammate shelf plan,
/// richos-hq `docs/plans/2026-10-06-proto-teammate-shelf.md` §2): stock definitions shipped in
/// `engine/agents/`, replaced with the engine on update. Pierce, who reads the back end's briefs
/// (CEO §111), is always active like the four the plan names.
pub const ALWAYS_ACTIVE: [&str; 5] = ["dean", "clark", "reed", "frank", "pierce"];

/// **The three duties, never a teammate** (plan §3, slices 2 and 3): `worker` implements,
/// `reviewer` reviews and `consult` answers on a job that changes no repository. Each is a duty
/// text (`engine/mega-lander/duties/<duty>.md`) the dispatch adapter puts at the top of a named
/// teammate's brief, not a definition, so none is registered and no duty name can be a
/// teammate's.
pub const DUTIES: [&str; 3] = ["worker", "reviewer", "consult"];

/// **The user's own teammates: `<app data>/team/<name>.md`**, activated from the shelf or
/// hired by Dean. Outside the engine, so an engine update never touches them. The shelf
/// itself (`engine/team/shelf/`) is never registered.
pub const USER_TEAM: &str = "team";

/// **The engine's shipped skills no lease loads**: the terminal lead's landing and worktree
/// procedures. The app lands and makes workspaces itself (Mega Lander), so a worker told to
/// follow them would go around it (richos-hq `docs/plans/2026-10-21-switching-to-the-app.md`
/// §4.6).
pub const TERMINAL_ONLY_SKILLS: [&str; 2] = ["rich-lander", "using-git-worktrees"];

/// **The engine's shipped skills this lease's engine plugin carries** (CEO §138: everything he
/// uses in the terminal is a feature of the regular app; the plan above, row S): every folder in
/// `<engine>/skills/` holding a `SKILL.md`, read off the folder so a skill added to the engine
/// reaches every lease, except [`TERMINAL_ONLY_SKILLS`] and the skills the standing plugin
/// already carries in the app's own words (`skills::SKILLS`), whose engine copies name the
/// terminal's install path. Sorted.
pub fn shipped_skills(engine: &Path) -> Result<Vec<String>, RuntimeError> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(engine.join("skills")).map_err(|e| RuntimeError(format!("the engine's skills could not be read: {e}")))? {
        let path = entry.map_err(|e| RuntimeError(e.to_string()))?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
        if !path.join("SKILL.md").is_file() || TERMINAL_ONLY_SKILLS.contains(&name.as_str())
            || crate::skills::SKILLS.contains(&name.as_str())
            || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') { continue; }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// Link each of [`shipped_skills`] into `plugin/skills/`. A link, not a copy: a skill's scripts
/// and references stay where the engine keeps them, and retiring the lease removes only the
/// link (`remove_dir_all` does not follow one). Measured on `claude` 2.1.306: a `--plugin-dir`
/// whose `skills/<name>` is a link to a folder lists `<plugin>:<name>` in `system/init.skills`.
fn link_skills(engine: &Path, plugin: &Path) -> Result<(), RuntimeError> {
    let root = plugin.join("skills");
    std::fs::create_dir_all(&root).map_err(|e| RuntimeError(e.to_string()))?;
    for name in shipped_skills(engine)? {
        #[cfg(unix)]
        std::os::unix::fs::symlink(engine.join("skills").join(&name), root.join(&name))
            .map_err(|e| RuntimeError(format!("the {name} skill could not be linked into the lease: {e}")))?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(engine.join("skills").join(&name), root.join(&name))
            .map_err(|e| RuntimeError(format!("the {name} skill could not be linked into the lease: {e}")))?;
    }
    Ok(())
}


/// Every definition this lease registers, by name: the stock ones, then the user's. A user's
/// file of the same name as a stock one is the user's refit of it and wins (plan §10 point 3:
/// the user's copy is never overwritten). A file whose name could not be an agent type
/// (`[a-z0-9-]`, not starting with `-`), a symbolic link, or one of the duty names is
/// not registered: those are the engine's app mechanics, never a person.
pub fn active_team(engine: &Path, data: &Path) -> Result<std::collections::BTreeMap<String, String>, RuntimeError> {
    let mut team = std::collections::BTreeMap::new();
    for name in ALWAYS_ACTIVE {
        let body = std::fs::read_to_string(engine.join(format!("agents/{name}.md"))).map_err(|e| RuntimeError(e.to_string()))?;
        team.insert(name.to_string(), body);
    }
    for (name, body) in user_definitions(data)? {
        team.insert(name, body);
    }
    Ok(team)
}

/// The user's own definition files, by name, as saved: the files [`active_team`] registers
/// from [`USER_TEAM`], under the same rules.
fn user_definitions(data: &Path) -> Result<Vec<(String, String)>, RuntimeError> {
    let folder = data.join(USER_TEAM);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(RuntimeError(format!("the team folder could not be read: {e}"))),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| RuntimeError(e.to_string()))?;
        let path = entry.path();
        if !entry.file_type().map_err(|e| RuntimeError(e.to_string()))?.is_file()
            || path.extension().is_none_or(|ext| ext != "md") { continue; }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        if name.is_empty() || name.starts_with('-') || DUTIES.contains(&name)
            || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') { continue; }
        let body = std::fs::read_to_string(&path).map_err(|e| RuntimeError(format!("team/{name}.md could not be read: {e}")))?;
        found.push((name.to_string(), body));
    }
    found.sort();
    Ok(found)
}

/// **One teammate the user added, as the home screen names him** (product hunt 2026-10-15,
/// finding 7: the home screen showed a made-up company to a user who had just hired two).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Teammate {
    pub name: String,
    /// The role title, or `None` when the definition does not state one.
    pub role: Option<String>,
}

/// **The teammates the user added**: the [`USER_TEAM`] definitions a lease registers, minus a
/// refit of one of the [`ALWAYS_ACTIVE`] five, which every install has and nobody hired. Name
/// and role come from the definition's own heading, `# {Name} — {Role Title}` (Dean's
/// template; the shelf's carry a parenthesis after the title, which is dropped). A definition
/// with no such heading is named by its file name. Ordered by file name.
pub fn user_teammates(data: &Path) -> Result<Vec<Teammate>, RuntimeError> {
    Ok(user_definitions(data)?
        .into_iter()
        .filter(|(name, _)| !ALWAYS_ACTIVE.contains(&name.as_str()))
        .map(|(name, body)| teammate_from(&name, &body))
        .collect())
}

fn teammate_from(file_name: &str, body: &str) -> Teammate {
    // After the frontmatter, whose YAML may carry a `# comment` of its own.
    let mut lines = body.lines().peekable();
    if lines.peek().is_some_and(|l| l.trim_end() == "---") {
        lines.next();
        for l in lines.by_ref() { if l.trim_end() == "---" { break; } }
    }
    let heading = lines.find_map(|l| l.strip_prefix("# ")).map(str::trim).filter(|h| !h.is_empty());
    let fallback = || {
        let mut chars = file_name.chars();
        chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
    };
    let Some(heading) = heading else { return Teammate { name: fallback(), role: None } };
    let Some((name, role)) = heading.split_once(" — ") else { return Teammate { name: heading.to_string(), role: None } };
    let role = role.trim();
    let role = match role.find(" (") {
        Some(at) if role.ends_with(')') => role[..at].trim(),
        _ => role,
    };
    let name = name.trim();
    Teammate {
        name: if name.is_empty() { fallback() } else { name.to_string() },
        role: (!role.is_empty()).then(|| role.to_string()),
    }
}

/// **The sentence every RichOS teammate's definition carries about video and audio** (CEO
/// 2026-10-10: every teammate, in femcboost and in the RichOS app, uses the watch-video skill
/// for video and the audio-transcription skill for audio; 2026-10-12: in the first Axel's
/// words, inside the definition's own working rules, never above who the teammate is). The
/// stock definitions, the shelf and the three duty texts carry it; `tests/engine_profile.rs`
/// holds this copy to theirs.
/// **The digest of the team a lease registers** (proto-teammate shelf plan §1 and §6, slice 5):
/// every name and definition [`active_team`] returns, so it moves exactly when the next lease
/// would register something different, and never for a file that would not be registered (the
/// shelf, a duty name, a temporary file, a link). The design is the retired operator snapshot's, digest
/// the definitions a session loaded and refresh them when the digest moves; its file set is
/// the operator's, so the code is not shared. Each part is length-prefixed, so no two teams
/// share a digest by where one body ends and the next name starts.
pub fn team_digest(engine: &Path, data: &Path) -> Result<String, RuntimeError> {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for (name, body) in active_team(engine, data)? {
        for part in [name.as_bytes(), body.as_bytes()] {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Find a YAML delimiter outside quoted tool names and comments.
fn unquoted_delimiter(line: &str, wanted: char) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (at, ch) in line.char_indices() {
        if escaped { escaped=false; continue; }
        if quote==Some('"') && ch=='\\' { escaped=true; continue; }
        if let Some(mark)=quote {
            if ch==mark { quote=None; }
        } else if ch=='\'' || ch=='"' { quote=Some(ch); }
        else if ch==wanted { return Some(at); }
    }
    None
}
fn tool_yaml_code(line: &str) -> &str {
    &line[..unquoted_delimiter(line,'#').unwrap_or(line.len())]
}

/// Generated lease definitions expose the same host read even with a restricted tool list.
/// Saved definitions are untouched. Other frontmatter and the instruction body stay intact.
pub fn with_resource_access(body: &str) -> String {
    let tool=crate::system_resources::QUALIFIED_TOOL;
    let mut lines: Vec<String>=body.split_inclusive('\n').map(str::to_owned).collect();
    if lines.first().is_none_or(|line|line.trim_end()!="---") { return body.into(); }
    let end=lines.iter().skip(1).position(|line|line.trim_end()=="---").map(|n|n+1).unwrap_or(1);
    for i in 1..end {
        let Some(value)=tool_yaml_code(&lines[i]).strip_prefix("tools:").map(str::trim).map(str::to_owned) else { continue };
        let field_end=(i+1..end).find(|j| {
            let code=tool_yaml_code(&lines[*j]);
            code.starts_with(|c:char|c.is_ascii_alphabetic()) && code.contains(':')
        }).unwrap_or(end);
        let already=lines[i..field_end].iter().any(|line|tool_yaml_code(line)
            .split(|ch:char|ch.is_whitespace() || ",[]\"'".contains(ch)).any(|part|part==tool));
        if already { return body.into(); }
        if value.starts_with('[') {
            let mut parts=Vec::new();
            for j in i..field_end {
                let code=tool_yaml_code(&lines[j]);
                let part=if j==i { code.strip_prefix("tools:").unwrap().trim_start().strip_prefix('[').unwrap() } else { code };
                if let Some(close)=unquoted_delimiter(part,']') {
                    parts.push(part[..close].trim_end_matches(['\r','\n']).to_owned());
                    let contents=parts.join("\n");
                    let contents=contents.trim().trim_end_matches(',').trim_end();
                    let comma=if contents.is_empty() { "" } else { ", " };
                    let comment=unquoted_delimiter(&lines[j],'#').map(|at|format!(" {}",lines[j][at..].trim_end())).unwrap_or_default();
                    lines.splice(i..=j,[format!("tools: [{contents}{comma}{tool}]{comment}\n")]);
                    return lines.concat();
                }
                parts.push(part.trim_end_matches(['\r','\n']).to_owned());
            }
            // Leave an invalid/incomplete source for the provider's normal validation.
            return body.into();
        }
        if value.is_empty() {
            if let Some(first)=(i+1..field_end).find(|j|!tool_yaml_code(&lines[*j]).trim().is_empty()) {
                if tool_yaml_code(&lines[first]).trim_start().starts_with('-') {
                    let indent=lines[first].chars().take_while(|c|c.is_whitespace() && *c!='\n').collect::<String>();
                    lines.insert(first,format!("{indent}- {tool}\n"));
                }
            }
            return lines.concat();
        }
        let comment=unquoted_delimiter(&lines[i],'#').map(|at|format!(" {}",lines[i][at..].trim_end())).unwrap_or_default();
        if value.starts_with('"') || value.starts_with('\'') {
            let quote=value.chars().next().unwrap();
            if let Some(close)=value.rfind(quote).filter(|n|*n>0) {
                let mut value=value;
                let comma=if close==1 { "" } else { ", " };
                value.insert_str(close,&format!("{comma}{tool}"));
                lines[i]=format!("tools: {value}{comment}\n");
            }
        } else if value!="null" && value!="~" {
            // YAML folds a hand-wrapped plain scalar. Append at its end, not
            // between the first line's comma and the continuation's next tool.
            let last=(i..field_end).rev().find(|j|!tool_yaml_code(&lines[*j]).trim().is_empty()).unwrap_or(i);
            let code=tool_yaml_code(&lines[last]).trim_end();
            let separator=if code.ends_with(',') { " " } else { ", " };
            let comment=unquoted_delimiter(&lines[last],'#').map(|at|format!(" {}",lines[last][at..].trim_end())).unwrap_or_default();
            lines[last]=format!("{code}{separator}{tool}{comment}\n");
        }
        return lines.concat();
    }
    // No list (or a null list) means the provider inherits every tool, including this read.
    body.into()
}

fn render_team(engine:&Path, data:&Path, coordination:&Path, plugin:&Path, preserve_loaded:bool)->Result<Vec<String>,RuntimeError> {
    let team=active_team(engine,data)?;
    for root in [plugin.join("agents"),coordination.join(".claude/agents")] {
        for (name,body) in &team {
            let path=root.join(format!("{name}.md"));
            if !preserve_loaded || !path.exists() {write(&path,&with_resource_access(body))?;}
        }
        for entry in std::fs::read_dir(&root).map_err(|e|RuntimeError(e.to_string()))? {
            let path=entry.map_err(|e|RuntimeError(e.to_string()))?.path();
            if path.extension().is_some_and(|ext|ext=="md")
                && path.file_stem().and_then(|n|n.to_str()).is_none_or(|n|!team.contains_key(n)) {
                std::fs::remove_file(path).map_err(|e|RuntimeError(e.to_string()))?;
            }
        }
    }
    let agents:Vec<_>=team.keys().map(|name|format!("./agents/{name}.md")).collect();
    write(&plugin.join(".claude-plugin/plugin.json"),&json!({"name":PLUGIN_NAME,"version":"1.2.0","agents":agents}).to_string())?;
    Ok(team.keys().map(|name|format!("{PLUGIN_NAME}:{name}")).collect())
}

impl EngineProfile {
    pub fn refresh_team(&self)->Result<Vec<String>,RuntimeError> {
        let data=self.state.parent().ok_or_else(||RuntimeError("Missing app data directory".into()))?;
        render_team(&self.engine,data,&self.coordination,&self.plugin,true)
    }

    /// The app's lease profile: this install's machine settings, the user's own settings
    /// files in the central folder (`company::install_central_root`) and the user's login
    /// shell environment (`login_environment::current`).
    pub fn prepare(engine: &Path, data: &Path, runtime: EngineRuntime) -> Result<Self, RuntimeError> {
        let mut profile = Self::prepare_for(engine, data, runtime, crate::machine_settings::Settings::current().map_err(RuntimeError)?,
            crate::company::install_central_root(), Some(crate::login_environment::current()))?;
        // The co-author line the user switched off in the terminal stays off here (Hank's parity
        // audit 2026-10-30, item 3): the terminal reads it from its Claude folder's settings file.
        if let Some(home) = machine_claude_home() {
            profile.user.settings = std::mem::take(&mut profile.user.settings).with_terminal_attribution(&home.join("settings.json"));
        }
        Ok(profile)
    }

    /// A profile that reads no user settings file, for a caller that names its own machine
    /// settings (tests).
    pub fn prepare_with_settings(engine: &Path, data: &Path, runtime: EngineRuntime, machine_settings: crate::machine_settings::Settings) -> Result<Self, RuntimeError> {
        Self::prepare_for(engine, data, runtime, machine_settings, None, None)
    }

    /// `user_folder` is the user's central folder: on first run the shipped `rich.md` and
    /// `stu.md` and an empty `instructions.md` are copied there when the user has none, and
    /// the user's files there are read into this lease (`user_settings.rs`). `None` reads none
    /// and starts Rich and Stu from the shipped plugin. `login` is the user's login shell
    /// environment, which every lease and hook gets (build plan item 4); `None` adds none.
    pub fn prepare_for(engine: &Path, data: &Path, runtime: EngineRuntime, machine_settings: crate::machine_settings::Settings,
                       user_folder: Option<PathBuf>, login: Option<std::sync::Arc<std::collections::BTreeMap<String, String>>>)
                       -> Result<Self, RuntimeError> {
        if !engine.is_absolute() || !data.is_absolute() { return Err(RuntimeError("desktop roots must be absolute".into())); }
        for name in ["scripts/app-engine-hook.py", "scripts/provider-supervisor.py", "mega-lander/app.py", "mega-lander/DESKTOP.md"] {
            if !engine.join(name).is_file() { return Err(RuntimeError(format!("missing desktop engine entry point: {name}"))); }
        }
        for name in ALWAYS_ACTIVE {
            if !engine.join(format!("agents/{name}.md")).is_file() { return Err(RuntimeError(format!("missing desktop engine entry point: agents/{name}.md"))); }
        }
        for duty in DUTIES {
            if !engine.join(format!("mega-lander/duties/{duty}.md")).is_file() { return Err(RuntimeError(format!("missing desktop engine entry point: mega-lander/duties/{duty}.md"))); }
        }
        std::fs::create_dir_all(data).map_err(|e| RuntimeError(e.to_string()))?;
        let data = std::fs::canonicalize(data).map_err(|e| RuntimeError(e.to_string()))?;
        let engine = std::fs::canonicalize(engine).map_err(|e| RuntimeError(e.to_string()))?;
        if data.starts_with(&engine) { return Err(RuntimeError("private app state cannot live inside engine code".into())); }
        let coordination = data.join("coordination");
        if coordination.is_symlink() || coordination.join(".git").is_symlink()
            || (coordination.join(".git").exists() && !coordination.join(".git").is_dir()) {
            return Err(RuntimeError("private coordination cannot redirect to another repository".into()));
        }
        let marker = coordination.join("app-owned-coordination.json");
        if coordination.exists() {
            if std::fs::read_to_string(&marker).ok().as_deref() != Some("{\"schema\":1,\"owner\":\"richos-app\"}\n") {
                return Err(RuntimeError("the coordination directory is not owned by this app; choose separate app storage".into()));
            }
        } else {
            std::fs::create_dir(&coordination).map_err(|e| RuntimeError(e.to_string()))?;
            write(&marker, "{\"schema\":1,\"owner\":\"richos-app\"}\n")?;
        }
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&coordination, std::fs::Permissions::from_mode(0o700)).map_err(|e| RuntimeError(e.to_string()))?;
        }
        if !coordination.join(".git").is_dir() {
            git(&runtime, &coordination, &["init", "--template=", "-q", "--initial-branch=main"])?;
        }
        // An interrupted first setup may have initialized Git without its first commit.
        if git(&runtime, &coordination, &["rev-parse", "--verify", "HEAD"]).is_err() {
            git(&runtime, &coordination, &["commit", "--allow-empty", "-q", "-m", "Initialize private app coordination"])?;
        }
        for child in ["engine-state", "engine-profiles", "coordination/.claude", "coordination/.claude/agents", USER_TEAM] {
            if data.join(child).is_symlink() {
                return Err(RuntimeError(format!("private app path cannot be redirected: {child}")));
            }
        }
        let state = data.join("engine-state");
        std::fs::create_dir_all(&state).map_err(|e| RuntimeError(e.to_string()))?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).map_err(|e| RuntimeError(e.to_string()))?;
        }
        let plugin = data.join("engine-profiles").join(uuid::Uuid::new_v4().to_string());
        render_team(&engine, &data, &coordination, &plugin,false)?;
        // Who made it, so the app's sweep can tell a crash's leftover from a live lease's
        // folder (CEO §54 addendum 2, `app_cleanup::OWNER_FILE`). Without it the folder is
        // still taken, later, once nothing names it and it is past the reaper's age floor.
        if let Err(e) = crate::app_cleanup::write_owner(&plugin) {
            eprintln!("[richos] {} carries no owner record: {e}", plugin.display());
        }
        link_skills(&engine, &plugin)?;
        // Rich's and Stu's own files are the user's from the first run on: copied once, then
        // read into this session's plugin beside this lease's engine plugin.
        let mut user = crate::user_settings::UserSetup::prepare(user_folder, &engine.join("app"),
            &plugin.with_file_name(format!("{}-standing", plugin.file_name().unwrap().to_string_lossy())))?;
        // The copy carries its own owner record too: a sweep out of time can take the base and
        // leave the copy for the next one, which must still be able to tell whose it is
        // (review rv-20261025T021219Z-91f589b0-af71 #1).
        if let Some(standing) = &user.standing {
            if let Err(e) = crate::app_cleanup::write_owner(standing) {
                eprintln!("[richos] {} carries no owner record: {e}", standing.display());
            }
        }
        user.login = login;
        // A read again that found a different environment renews the lease at its next boundary,
        // as a changed settings file does, so the team's next job has it (round 41 piece 1).
        // With no login environment the files' digest stays as it is: taking it first would
        // drop it, and an edited file would then never renew the lease.
        if let Some(login) = user.login.as_ref() {
            user.digest = user.digest.take().map(|digest| crate::user_settings::with_login(digest, login));
        }
        // The Git the user's terminal runs, found on the login PATH (build plan item 8).
        if let Some(path) = user.login.as_ref().and_then(|login| login.get("PATH")) {
            let home = user.login.as_ref().and_then(|login| login.get("HOME")).map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
            user.git = crate::user_git::discover(path, home.as_deref(), &runtime.root);
        }
        let shipped_config = std::fs::read_to_string(engine.join("orchestration.config"))
            .map_err(|e| RuntimeError(format!("the engine's orchestration.config could not be read: {e}")))?;
        write(&coordination.join("orchestration.config"),
              &crate::user_settings::orchestration_config(user.folder.as_deref(), &shipped_config, &quote(&state.join("teams")))?)?;
        write(&coordination.join(".gitignore"), ".claude/\norchestration.config\napp-owned-coordination.json\n")?;
        write(&plugin.join("gitconfig"), &crate::user_git::system_layer(&user.git, Some(&crate::git_name::identity_file(&state))))?;
        let command = format!("{} {}", quote(&runtime.python), quote(&engine.join("scripts/app-engine-hook.py")));
        let mut hooks = serde_json::Map::new();
        // **NO `matcher` KEY, ON ANY OF THE NINE EVENTS — SO THE `PreToolUse` REGISTRATION
        // COVERS EVERY TOOL, NOT A CHOSEN FEW.**
        //
        // Stated here because two escalations and one brief were built on the opposite
        // belief. `scripts/app-engine-hook.py`'s `PreToolUse` branch reads `RICHOS_APP_SCOPE`
        // and raises *"This app turn is stopped or is supplying context. New actions are
        // unavailable."* whenever that file's `actions_allowed` is not true — for EVERY tool
        // the lease calls, because of this loop. It was read as gating only the two
        // continuity tools, and deferring the grant on that basis would have made the
        // register itself impossible, which is the opposite of the CEO's §55.
        //
        // This is the app's OWN turn lifecycle and it is right that it is broad: `prompt`
        // shuts it at the start of a turn, the reader opens it at his first words, and an app
        // turn that is stopped or is only supplying context genuinely may take no action of
        // any kind. Nothing in it reads a development session — it is not the guard-audience
        // defect wearing another costume (see
        // docs/verification/guard-audience-2026-09-18.md, "Is RICHOS_APP_SCOPE the same
        // root?"). What was wrong was that its reach was written down nowhere, so every
        // reader had to infer it and one inferred it narrow.
        for event in ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "SubagentStart", "SubagentStop", "Stop"] {
            // Agent dispatch includes Pierce's bounded five-minute inspection.
            let timeout = if event == "PreToolUse" { 330 } else { 25 };
            hooks.insert(event.into(), json!([{"hooks":[{"type":"command","command":command,"timeout":timeout}]}]));
        }
        hooks.get_mut("PreToolUse").unwrap().as_array_mut().unwrap().push(json!({
            "matcher":"Bash", "hooks": [
                {"type":"command", "command":format!("/bin/bash {}", quote(&engine.join("scripts/hooks/guard-interactive-prompt.sh"))), "timeout":25},
                {"type":"command", "command":format!("/bin/bash {}", quote(&engine.join("scripts/hooks/guard-no-shell-c.sh"))), "timeout":25},
                {"type":"command", "command":format!("/bin/bash {}", quote(&engine.join("scripts/hooks/shell-evidence.sh"))), "timeout":86400},
                {"type":"command", "command":format!("/bin/bash {}", quote(&engine.join("scripts/hooks/guard-foreign-app-data.sh"))), "timeout":25},
                {"type":"command", "command":app_rules(&engine, "Bash", &APP_BASH_RULES), "timeout":60}
            ]}));
        // The engine's checks at the write (gap E), as the terminal registers them: the
        // dispatcher's Write chain, then `dialect-guard.sh`, which fixes a word in the write
        // itself and so is registered on its own (the manifest's header says why).
        hooks.get_mut("PreToolUse").unwrap().as_array_mut().unwrap().push(json!({
            "matcher":"Write|Edit|MultiEdit|NotebookEdit", "hooks": [
                {"type":"command", "command":app_rules(&engine, "Write", &APP_WRITE_RULES), "timeout":40},
                {"type":"command", "command":format!("/bin/bash {}", quote(&engine.join("scripts/hooks/dialect-guard.sh"))), "timeout":20}
            ]}));
        write(&plugin.join("hooks/hooks.json"), &json!({"hooks":hooks}).to_string())?;
        // Spawn preflight runs the canonical guards directly. The real provider
        // additionally enters the desktop scope/evidence gate at dispatch time.
        // WHICH guards is [`user_work_guards`]'s answer, read from the engine's
        // declaration — never a list typed here, which is what it was until
        // 2026-09-18 and how an operator-session guard came to be judging a
        // user's assignment.
        let preflight: Vec<_> = user_work_guards(&engine)?.into_iter()
            .map(|guard| json!({"type":"command",
                "command":format!("/bin/bash {}", quote(&engine.join("scripts/hooks").join(guard)))}))
            .collect();
        write(&plugin.join("spawn-preflight.json"),
              &json!({"hooks":{"PreToolUse":[{"matcher":"Agent","hooks":preflight}]}}).to_string())?;
        let profile = Self { engine, coordination, plugin, state, runtime, work_scope: None,
                  claude_account: None, token_sink: None, speech_model: None,
                  lease_models: crate::config::lease_models_from_config(&data), machine_settings, user };
        write(&profile.plugin.join(COMMANDS_ENV), &profile.commands_env())?;
        Ok(profile)
    }
    /// Install the desktop executable's quota wrapper. Source-test clients which
    /// do not implement that CLI entrypoint keep their canonical hooks unchanged.
    pub fn install_quota_gate(&self, executable: &Path) -> Result<(), RuntimeError> {
        let path = self.plugin.join("hooks/hooks.json");
        let mut manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)
            .map_err(|e| RuntimeError(e.to_string()))?).map_err(|e| RuntimeError(e.to_string()))?;
        let command = format!("{} --claude-quota-gate {} {}", quote(executable),
            quote(&self.runtime.python), quote(&self.engine.join("scripts/app-engine-hook.py")));
        manifest["hooks"]["PreToolUse"][0] = json!({"hooks":[{"type":"command", "command":command, "timeout":21600}]});
        write(&path, &manifest.to_string())
    }
    /// **Make the team's first save in a connected folder wait for the user's Git name**
    /// (round 41 piece 2): Git's own hook, `<executable> --git-name-wait`, added to this lease's
    /// lowest Git layer (`git_name::hook_section`), so Git runs it at every commit the lease's Git
    /// makes, however the command was written. It holds only a commit the stand-in would sign
    /// (`git_name::wait_cli`).
    pub fn install_git_name_wait(&self, executable: &Path) -> Result<(), RuntimeError> {
        let path = self.plugin.join("gitconfig");
        let mut layer = std::fs::read_to_string(&path).map_err(|e| RuntimeError(e.to_string()))?;
        layer.push_str(&crate::git_name::hook_section(executable));
        write(&path, &layer)
    }
    /// **The files the CEO attached in THIS conversation**, from his phone or his Mac:
    /// `<app data>/attachments/<conversation>`, the folder the attachment desk writes to
    /// (`crate::attachments`). Given to the session as a read root beside the company's
    /// repositories, because the session blocks reads outside the directories it is given and
    /// an attached file Rich cannot open is a path, not a file. One conversation's folder, never
    /// the root: another conversation's files, under another company, stay out of reach.
    pub fn attachments_folder(&self) -> Option<PathBuf> {
        let (_, thread) = self.work_scope.as_ref()?;
        Some(crate::attachments::conversation_folder(self.state.parent()?, thread))
    }

    pub fn scope_to(&mut self, binding: &crate::entity::ThreadBinding) -> Result<(), RuntimeError> {
        self.work_scope = Some((binding.entity_id().to_string(), binding.thread_id().to_string()));
        // The attachments folder exists before the session starts, because the session is
        // given it as a read root and a file attached later in the conversation lands in it.
        // Private like the desk's own folders, and never a redirect.
        if let Some(files) = self.attachments_folder() {
            if files.is_symlink() || files.parent().is_some_and(Path::is_symlink) {
                return Err(RuntimeError("a conversation's attachments cannot redirect to another directory".into()));
            }
            let mut folder = std::fs::DirBuilder::new();
            folder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                folder.mode(0o700);
            }
            folder.create(&files).map_err(|e| RuntimeError(e.to_string()))?;
        }
        let target = self.target_state();
        if target.is_symlink() || target.parent().is_some_and(Path::is_symlink) {
            return Err(RuntimeError("thread workspaces cannot redirect to another directory".into()));
        }
        std::fs::create_dir_all(&target).map_err(|e| RuntimeError(e.to_string()))?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).map_err(|e| RuntimeError(e.to_string()))?;
        }
        Ok(())
    }
    pub fn target_state(&self) -> PathBuf {
        self.state.join("target-worktrees").join(self.workspace_state().file_name().unwrap())
    }
    pub fn workspace_state(&self) -> PathBuf {
        use sha2::Digest;
        let partition = match &self.work_scope {
            Some((entity, thread)) => format!("{:x}", sha2::Sha256::digest(serde_json::to_vec(&(entity, thread)).unwrap())),
            None => format!("unbound-{}", self.plugin.file_name().unwrap().to_string_lossy()),
        };
        self.state.join("workspaces").join(partition)
    }
    /// **The one place the app pins `spawn.py`'s guard surface**, as
    /// `RICHOS_SPAWN_HOOK_SOURCES` expects it: `<label>=<path>`.
    ///
    /// [`configure`](Self::configure) exports this for the provider child, and the
    /// `richos_work` server — `mega-lander/app.py`, a child of that child — inherits it, so
    /// `spawn.py`'s `settings_sources` reads this file and nothing else. That is what makes
    /// the app's guard surface the declared user-work list rather than whatever guards the
    /// machine the app happens to be running on has installed for its own sessions.
    ///
    /// **IT IS AN ACCESSOR BECAUSE A SECOND CALLER GOT IT WRONG BY OMISSION.** The
    /// `first_reply_timing_e2e` probe (deleted 2026-10-18) drove `prepare` directly, built its
    /// own environment, and never set this — so on 2026-09-18 it measured `spawn.py` collecting all NINE of
    /// the ENGINE's PreToolUse[Agent] guards and being refused by `guard-owned-state.sh` over
    /// the development session's paused CI. That refusal was real, reproducible, and about a
    /// path the shipped app never takes: the same binary, with this variable set the way
    /// `configure` sets it, answers `prepared`. A value spelled out in two places drifts; a
    /// value spelled out in one place and FORGOTTEN in another is worse, because the second
    /// caller looks right.
    pub fn spawn_hook_sources(&self) -> String {
        format!("app={}", self.plugin.join("spawn-preflight.json").display())
    }

    /// **A lease's `PATH`**: the runtime first, then the user's own `PATH` (`settings.json`'s
    /// when it sets one, otherwise the login shell's), then the system folders and the tools
    /// folder (`runtime::search_path_with`).
    pub fn lease_path(&self) -> String {
        let mut path = self.runtime.path_with(self.user_path());
        self.with_git_helpers(&mut path);
        path
    }

    /// The user's own `PATH`: `settings.json`'s when it sets one, otherwise the login shell's.
    fn user_path(&self) -> Option<&str> {
        self.user.settings.env.get("PATH").or_else(|| self.user.login.as_ref()?.get("PATH")).map(String::as_str)
    }

    /// The user's Git's helpers (`git-credential-osxkeychain` and the like), last, so the
    /// runtime's Git finds the helper the user's settings name and nothing is shadowed.
    fn with_git_helpers(&self, path: &mut String) {
        if let Some(helpers) = self.user.git.exec_path.as_ref().map(|p| p.display().to_string())
            .filter(|p| !p.contains(':') && !path.split(':').any(|entry| entry == p)) {
            path.push(':');
            path.push_str(&helpers);
        }
    }

    /// **The `PATH` an agent's own commands run with** (Hank's parity audit 2026-10-30, item 2):
    /// the user's own folders first, so `python3`, `node` and `git` are the user's where the user
    /// has them, and never one of Apple's install stand-ins (`runtime::commands_path`). The
    /// lease's own `PATH` ([`Self::lease_path`]), which the hooks and the app's tool servers run
    /// with, keeps the runtime first. With no user `PATH` the two are the same.
    pub fn commands_path(&self) -> String {
        let Some(user) = self.user_path() else { return self.lease_path() };
        let mut path = self.runtime.commands_path(user, &crate::user_git::StandIns::installed());
        self.with_git_helpers(&mut path);
        path
    }

    /// **What [`COMMANDS_ENV`] says ahead of every Bash command of an agent**: the agent's own
    /// `PATH` ([`Self::commands_path`]), and the user's interpreter settings that the lease leaves
    /// out for the app's own scripts (`runtime::isolate_interpreter_environment`): `PYTHONPATH`,
    /// `PYTHONUSERBASE` and the rest as the user's terminal has them (`settings.json`'s, else the
    /// login shell's), and Python's `pip --user` packages back on, so the user's `python3` imports
    /// what it imports in the terminal (second review rv-20261030T220254Z, P2). A name the user
    /// does not set is unset, as in the terminal.
    pub fn commands_env(&self) -> String {
        let user = |key: &str| self.user.settings.env.get(key).or_else(|| self.user.login.as_ref()?.get(key));
        let mut text = format!("export PATH={}\n", quote(Path::new(&self.commands_path())));
        for key in crate::runtime::INTERPRETER_KEYS.into_iter().chain([crate::runtime::NO_USER_SITE]) {
            match user(key) {
                Some(value) => text.push_str(&format!("export {key}={}\n", quote(Path::new(value)))),
                None => text.push_str(&format!("unset {key}\n")),
            }
        }
        text
    }

    /// **The folders this one lease was given, removed when it retires**: its engine plugin and
    /// its copy of the plugin Rich and Stu start from (`user_settings::standing_plugin`).
    pub fn remove_session_files(&self) {
        for folder in std::iter::once(&self.plugin).chain(self.user.standing.as_ref()) {
            if let Err(e) = std::fs::remove_dir_all(folder) {
                if e.kind() != std::io::ErrorKind::NotFound { eprintln!("[richos] {} was not removed: {e}", folder.display()); }
            }
        }
    }

    /// **The plugin Rich and Stu start from**: this lease's copy carrying the user's own
    /// `rich.md` and `stu.md` (`user_settings::standing_plugin`), or `shipped` when the
    /// profile reads no user folder.
    pub fn standing_plugin(&self, shipped: &Path) -> PathBuf {
        self.user.standing.clone().unwrap_or_else(|| shipped.to_path_buf())
    }

    /// **The `--settings` a lease starts with**: the app's own keys, the user's own hooks from
    /// `settings.json` beside the engine plugin's, which Claude Code runs together, and the
    /// user's commit attribution (`user_settings::ATTRIBUTION`).
    pub fn lease_settings(&self) -> serde_json::Value {
        // Auto-memory on (build plan item 10); where Rich and Stu keep it is their definitions'
        // `memory:` scope (`user_settings::MEMORY_SCOPE`), in the coordination folder.
        let mut settings = json!({"autoMemoryEnabled":true,
            "claudeMdExcludes":["**/CLAUDE.md", "**/CLAUDE.local.md", "**/.claude/rules/**"]});
        if let Some(hooks) = &self.user.settings.hooks { settings["hooks"] = hooks.clone(); }
        for (key, value) in &self.user.settings.attribution { settings[key] = value.clone(); }
        settings
    }

    /// `provider` is the `claude` binary itself, named by the caller. It is NOT read off
    /// `command`: a product lease's program is the supervisor's Python
    /// (`lease_commands::supervisor_args`), and reading it there told every helper that Python
    /// was Claude Code — so the brief check ran `python3 --print` and failed on every dispatch
    /// (nightly 54, 2026-10-13).
    pub fn configure(&self, command: &mut Command, provider: &Path, session: &str, scope: &Path) {
        // The user's terminal environment (the login shell's, build plan item 4), then the
        // user's own additions (`settings.json`), under every name the app sets for itself
        // below: those are applied after this and win.
        let user_env = self.user.login.iter().flat_map(|login| login.iter()).chain(self.user.settings.env.iter());
        command.envs(user_env.filter(|(name, _)| !crate::user_settings::app_owned(name)));
        crate::runtime::isolate_interpreter_environment(command);
        // App bindings replace terminal component roots and test overrides.
        // Native transcript/session readers use the provider's selected Claude account.
        let inherited: Vec<_> = std::env::vars_os().map(|(key,_)|key)
            .chain(command.get_envs().map(|(key,_)|key.to_owned())).collect();
        for key in inherited {
            let name = key.to_string_lossy();
            if ["RICHOS_", "LORO_", "ECS_", "GIT_"].iter().any(|prefix|name.starts_with(prefix)) {
                command.env_remove(key);
            }
        }
        for key in crate::machine_settings::KEYS { command.env_remove(key); }
        command.envs(self.machine_settings.environment());
        command.env_remove("RICHOS_APP_ENTITY").env_remove("RICHOS_APP_THREAD");
        // **One Claude home for the whole machine** (richos-hq follow-ups, "the usage hold reads
        // the wrong Claude account", 2026-10-28; Rich's decision 23:58Z): the engine's own stores
        // (the land lease, the fence settings, second-review records, the workspace registry's
        // fallback) live in Account 1's folder whichever account this lease spends. An added
        // account's `CLAUDE_CONFIG_DIR` moves only what Claude Code itself keeps there. The
        // engine reads `RICHOS_CLAUDE_HOME` only while `CLAUDE_CONFIG_DIR` is still the folder
        // named by `RICHOS_CLAUDE_ACCOUNT_DIR` (`machine_settings.claude_home`), so a test sandbox
        // that sets its own `CLAUDE_CONFIG_DIR` inside this lease keeps its own stores. Inherited
        // values of both were removed with every other `RICHOS_` name above.
        if let Some(home) = machine_claude_home() {
            command.env("RICHOS_CLAUDE_HOME", home);
        }
        if let Some(folder) = self.claude_account.as_ref().and_then(|a| a.folder.as_ref()) {
            command.env("CLAUDE_CONFIG_DIR", folder).env("RICHOS_CLAUDE_ACCOUNT_DIR", folder);
        }
        // **Which Claude account this provider and every helper inside it spend** (weekly-switch
        // plan §1): hooks run in the provider's environment, so the quota gate reads it on every
        // helper's tool call and orders the helpers of an account being left. Missing means
        // Account 1 (`quota::gate::account_of_this_process`).
        if let Some(account) = &self.claude_account {
            command.env(crate::quota::gate::ACCOUNT_ENV, &account.id);
        }
        if let Some((entity, thread)) = &self.work_scope {
            command.env("RICHOS_APP_ENTITY", entity).env("RICHOS_APP_THREAD", thread);
        }
        // The video skill's transcription step reads this (`skills/watch-video/SKILL.md`), and
        // so does `skills/audio-transcription/SKILL.md`. An
        // inherited value was removed with every other `RICHOS_` name above.
        if let Some(model) = &self.speech_model {
            command.env("RICHOS_SPEECH_MODEL", model);
        }
        let registry = crate::entity::EntityRegistry::load(&self.state.parent().unwrap().join("entities.json")).registry;
        let repos: Vec<_> = registry.entities().iter()
            .filter(|entity| self.work_scope.as_ref().is_some_and(|(id,_)| entity.id.as_str()==id))
            .flat_map(|entity| entity.connected_repositories.iter().cloned()).collect();
        // This native option is variadic. One occurrence preserves every root;
        // repeated occurrences can replace the earlier list in the CLI parser.
        let attachments = self.attachments_folder();
        let mut read_roots = repos.clone();
        if self.work_scope.is_some() {
            read_roots.push(self.target_state());
            read_roots.extend(attachments.iter().cloned());
            // The user's Codex channel folders that exist (`codex_watch.rs`, gap X): Rich and Stu
            // read Codex's `to-rich.md` and answer in its `to-codex.md`. Folders of the session,
            // not of the read-only inspectors below.
            let home = std::env::var_os("HOME").map(PathBuf::from).filter(|home| !home.as_os_str().is_empty());
            let channels: Vec<PathBuf> = crate::codex_watch::channel_folders(self.user.folder.as_deref(), home.as_deref())
                .into_iter().filter(|folder| folder.is_dir() && !read_roots.contains(folder)).collect();
            command.arg("--add-dir").args(&read_roots).args(&channels);
        }
        // Read-only inspectors inherit exactly this company's/conversation's
        // host-selected folders. A path in a model's brief is not a grant.
        command.env("RICHOS_APP_READ_ROOTS", serde_json::to_string(&read_roots).unwrap());
        command.arg("--plugin-dir").arg(&self.plugin)
            .args(["--permission-mode", "bypassPermissions"])
            .arg("--settings").arg(self.lease_settings().to_string())
            .env_remove("CLAUDE_CODE_DISABLE_AUTO_MEMORY")
            .env("PATH", self.lease_path()).env_remove("CLAUDECODE")
            .env("CLAUDE_ENV_FILE", self.plugin.join(COMMANDS_ENV))
            // The app's own tools run the app's Python whatever an agent's PATH finds first
            // (the yt-dlp launcher, `media_tools::launcher_text`).
            .env(crate::media_tools::RUNTIME_PYTHON, &self.runtime.python)
            // Hooks inherit the runtime-only PATH, which excludes Claude's installer path.
            .env("RICHOS_CLAUDE_BIN", provider)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            // Commits under the user's own name and pushes with the user's own credential
            // helper (build plan item 8): Git reads the user's ~/.gitconfig and each
            // repository's settings as it always does, above the app's lowest layer, which
            // holds the identity used only when the user has none and includes the user's
            // Git's system file (`user_git::system_layer`).
            .env_remove("GIT_CONFIG_NOSYSTEM").env_remove("GIT_CONFIG_GLOBAL")
            .env("GIT_CONFIG_SYSTEM", self.plugin.join("gitconfig"))
            .env("GIT_CONFIG_COUNT", "0").env_remove("GIT_CONFIG_PARAMETERS").env_remove("GIT_TEMPLATE_DIR")
            .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_AUTHOR_DATE").env_remove("GIT_COMMITTER_DATE")
            .env_remove("GIT_AUTHOR_NAME").env_remove("GIT_AUTHOR_EMAIL")
            .env_remove("GIT_COMMITTER_NAME").env_remove("GIT_COMMITTER_EMAIL")
            .env("RICHOS_APP_REGISTRY", self.coordination.parent().unwrap().join("entities.json"))
            .env("RICHOS_RESOURCE_READER", std::env::current_exe().unwrap_or_default())
            .env("RICHOS_APP_SCOPE", scope).env("RICHOS_APP_STATE", &self.state)
            .env("RICHOS_AGENT_HOLD_DIR", self.state.join("agent-hold"))
            .env("STALL_WATCH_STATE_DIR", self.state.join("stall-watch"))
            .env("RICHOS_ENGINE_ROOT", &self.engine).env("RICHOS_ENGINE_DIR", &self.engine)
            .env("RICHOS_ENTITY_ROOT", &self.coordination).env("CLAUDE_PROJECT_DIR", &self.coordination)
            .env("RICHOS_WORKSPACES_DIR", self.workspace_state())
            .env("RICHOS_SA_ENTITY_ROOT", &self.coordination).env("RICHOS_SA_TEAMS_DIR", self.state.join("teams"))
            .env("RICHOS_SESSION_ID", session).env_remove("RICHOS_SESSION_PID")
            .env("RICHOS_SPAWN_HOOK_SOURCES", self.spawn_hook_sources());
    }
}
