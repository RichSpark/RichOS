#![cfg(unix)]
use richos_core::engine_profile::EngineProfile;
use richos_core::native::LeaseRole;
use richos_core::runtime::EngineRuntime;
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("richos profile fixture {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap(); Self(std::fs::canonicalize(root).unwrap())
    }
    fn runtime(&self) -> EngineRuntime {
        // Explicit source-test runtimes. This fixture does not certify delivery.
        EngineRuntime { root: self.0.join("runtime"), git: PathBuf::from("/usr/bin/git"),
            python: self.0.join("runtime/bin/python3"), node: self.0.join("runtime/bin/node"), versions:BTreeMap::new() }
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
fn engine() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine") }

fn prepare_fixture(engine: &Path, data: &Path, runtime: EngineRuntime) -> Result<EngineProfile, richos_core::runtime::RuntimeError> {
    EngineProfile::prepare_with_settings(engine, data, runtime, richos_core::machine_settings::Settings::default())
}

#[test]
fn app_profile_keeps_app_scope_on_every_tool_and_reuses_terminal_bash_hooks() {
    let f = Scratch::new(); let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(profile.plugin.join("hooks/hooks.json")).unwrap()).unwrap();
    let hooks = manifest["hooks"].as_object().unwrap();
    assert_eq!(hooks.len(), 9);
    for (event, groups) in hooks {
        assert_eq!(groups.as_array().unwrap().len(), if event == "PreToolUse" { 3 } else { 1 });
        assert_eq!(groups[0]["hooks"].as_array().unwrap().len(), 1);
    }
    let bash = &hooks["PreToolUse"][1];
    assert_eq!(bash["matcher"], "Bash");
    let registered = bash["hooks"].as_array().unwrap();
    for (hook, name) in registered.iter().zip([
        "guard-interactive-prompt.sh", "guard-no-shell-c.sh", "shell-evidence.sh", "guard-foreign-app-data.sh",
        "dispatch-pretooluse.sh"]) {
        assert!(hook["command"].as_str().unwrap().contains(engine().join("scripts/hooks").join(name).to_str().unwrap()));
    }
    assert_eq!(registered.len(), 5);
    // **NO `matcher`, SO `PreToolUse` COVERS EVERY TOOL.** Asserted rather than left to be
    // inferred: two escalations and one brief on 2026-09-18 were built on the belief that
    // this registration gated only the two continuity tools, and deferring the app's action
    // grant on that basis would have made the register itself impossible (CEO §55). If a
    // matcher is ever added here, the reach of `RICHOS_APP_SCOPE` changes and this must be
    // the thing that says so.
    for (event, groups) in hooks {
        assert!(groups[0].get("matcher").is_none(),
            "{event} was registered with a matcher; the app hook's reach is no longer every tool");
    }
    // Slices 2 and 3 of the proto-teammate shelf plan: the three duties are texts the dispatch
    // adapter puts in a named teammate's brief, never registered definitions.
    for duty in ["worker", "reviewer", "consult"] {
        assert!(!profile.plugin.join(format!("agents/{duty}.md")).exists(), "{duty} is registered");
        assert!(engine().join(format!("mega-lander/duties/{duty}.md")).is_file(), "the {duty} duty text is missing");
    }
    assert!(profile.coordination.join(".git").is_dir());
    assert!(!profile.coordination.join("ceo-wiki").exists());
    assert!(!profile.coordination.starts_with(profile.engine));
    let next = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_ne!(profile.plugin, next.plugin);
    assert_eq!(profile.coordination, next.coordination);
}

#[test]
fn desktop_quota_wrapper_preserves_the_canonical_hook_and_its_full_tool_reach() {
    let f = Scratch::new();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let before: serde_json::Value = serde_json::from_slice(&std::fs::read(profile.plugin.join("hooks/hooks.json")).unwrap()).unwrap();
    profile.install_quota_gate(Path::new("/Applications/RichOS Test.app/Contents/MacOS/richos")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(profile.plugin.join("hooks/hooks.json")).unwrap()).unwrap();
    let group = &manifest["hooks"]["PreToolUse"][0];
    assert!(group.get("matcher").is_none());
    let hook = &group["hooks"][0];
    assert!(hook["command"].as_str().unwrap().contains("--claude-quota-gate"));
    assert!(hook["command"].as_str().unwrap().contains("app-engine-hook.py"));
    assert_eq!(hook["timeout"], 21600);
    assert_eq!(manifest["hooks"]["PreToolUse"].as_array().unwrap().len(), 3);
    assert_eq!(manifest["hooks"]["PreToolUse"][1], before["hooks"]["PreToolUse"][1]);
    assert_eq!(manifest["hooks"]["PreToolUse"][2], before["hooks"]["PreToolUse"][2]);
    assert_eq!(manifest["hooks"].as_object().unwrap().len(), 9);
}

#[test]
fn an_unrelated_or_redirected_directory_is_never_adopted() {
    let f = Scratch::new(); std::fs::create_dir(f.0.join("coordination")).unwrap();
    std::fs::write(f.0.join("coordination/keep.txt"), "fictional user file").unwrap();
    assert!(prepare_fixture(&engine(), &f.0, f.runtime()).is_err());
    assert_eq!(std::fs::read_to_string(f.0.join("coordination/keep.txt")).unwrap(), "fictional user file");
    assert!(!f.0.join("coordination/.git").exists());
}

#[test]
fn child_configuration_is_explicit() {
    let f = Scratch::new(); let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut command = std::process::Command::new("/fictional/provider");
    command.env("RICHOS_PROJECTS_DIR", "/fictional/terminal-context").env("RICHOS_SESSIONS_DIR", "/fictional/terminal-sessions").env("LORO_CORPUS", "/fictional/private-corpus").env("NODE_OPTIONS", "--require /fictional/private.js")
        .env("RICHOS_APP_READ_ROOTS", "[\"/fictional/other-company\"]");
    profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(environment["RICHOS_ENTITY_ROOT"].as_deref(), profile.coordination.to_str());
    assert_eq!(environment["STALL_WATCH_STATE_DIR"].as_deref(), profile.state.join("stall-watch").to_str());
    assert!(!environment["PATH"].as_ref().unwrap().contains("homebrew"));
    assert_eq!(environment["RICHOS_CLAUDE_BIN"].as_deref(), Some("/fictional/provider"));
    assert_eq!(environment["GIT_CONFIG_SYSTEM"].as_deref(), profile.plugin.join("gitconfig").to_str());
    assert_eq!(environment.get("GIT_CONFIG_GLOBAL"), Some(&None), "the user's own ~/.gitconfig is read as Git always reads it");
    assert_eq!(environment["GIT_AUTHOR_NAME"], None);
    assert_eq!(environment["LORO_CORPUS"], None);
    assert_eq!(environment["NODE_OPTIONS"], None);
    assert_eq!(environment["RICHOS_PROJECTS_DIR"], None);
    assert_eq!(environment["RICHOS_SESSIONS_DIR"], None);
    assert_eq!(environment["RICHOS_SESSION_ID"].as_deref(), Some("fictional-session"));
    assert_eq!(environment["RICHOS_APP_READ_ROOTS"].as_deref(), Some("[]"));
    let args: Vec<_> = command.get_args().map(|a|a.to_string_lossy().to_string()).collect();
    assert!(args.contains(&"--plugin-dir".to_string()));
    assert!(args.contains(&"--settings".to_string()));
}

/// **Helpers inside a lease are told the `claude` path, never the supervisor's Python.** Every
/// product lease runs `provider-supervisor.py` with the app's Python in front of `claude`
/// (`native.rs`, `lease_commands::supervisor_args`), and the brief check (`pierce.py`) runs
/// `$RICHOS_CLAUDE_BIN --print …`. Nightly 54 named the Python there, so every dispatch's check
/// failed with Python's own "unknown option --print", and the back end's dismissal of that
/// broken check stopped as "Approve this" in front of the CEO (2026-10-13).
#[test]
fn a_supervised_lease_names_claude_and_not_python_as_the_provider() {
    let f = Scratch::new(); let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut command = std::process::Command::new(&profile.runtime.python);
    command.args(richos_core::lease_commands::supervisor_args(&profile.engine, &f.0.join("lease.json"), &f.0.join("reap.log")))
        .arg("/fictional/provider");
    profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(environment["RICHOS_CLAUDE_BIN"].as_deref(), Some("/fictional/provider"));
}

/// **Weekly-switch plan §1: the provider is told which Claude account it spends**, beside its
/// folder, so the quota gate can order the helpers of an account being left. With no account
/// bound nothing is set (the gate reads that as Account 1), and a value inherited from the
/// app's own environment never survives.
#[test]
fn the_provider_is_told_which_claude_account_it_spends() {
    use richos_core::claude_accounts::Account;
    let f = Scratch::new();
    let mut profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let environment = |profile: &EngineProfile| {
        let mut command = std::process::Command::new("/fictional/provider");
        command.env("RICHOS_CLAUDE_ACCOUNT", "9").env("RICHOS_CLAUDE_HOME", "/fictional/inherited")
            .env("RICHOS_CLAUDE_ACCOUNT_DIR", "/fictional/inherited-account");
        profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
        command.get_envs().map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect::<BTreeMap<_, _>>()
    };
    let none = environment(&profile);
    assert_eq!(none["RICHOS_CLAUDE_ACCOUNT"], None, "an inherited account is removed");
    profile.claude_account = Some(Account { id: "1".into(), label: "Home".into(), folder: None });
    let one = environment(&profile);
    assert_eq!(one["RICHOS_CLAUDE_ACCOUNT"].as_deref(), Some("1"));
    assert!(!one.contains_key("CLAUDE_CONFIG_DIR"), "Account 1 keeps the app's own folder");
    let folder = f.0.join("claude-accounts/2");
    profile.claude_account = Some(Account { id: "2".into(), label: "Work".into(), folder: Some(folder.clone()) });
    let two = environment(&profile);
    assert_eq!(two["RICHOS_CLAUDE_ACCOUNT"].as_deref(), Some("2"));
    assert_eq!(two["CLAUDE_CONFIG_DIR"].as_deref(), folder.to_str());
    // **One machine-wide Claude home whichever account is spent** (2026-10-28): Account 1's folder,
    // the app's own CLAUDE_CONFIG_DIR else ~/.claude, never the added account's and never inherited.
    let machine = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|dir| !dir.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".claude"));
    for (lease, env) in [("no account", &none), ("Account 1", &one), ("Account 2", &two)] {
        assert_eq!(env["RICHOS_CLAUDE_HOME"].as_deref(), machine.to_str(), "{lease}: the engine's home is Account 1's");
    }
    // The engine honors that home only while CLAUDE_CONFIG_DIR is the added account's folder the
    // app set (machine_settings.claude_home), so the pair names exactly that folder, and nothing
    // where the app set no folder; an inherited value never survives.
    assert_eq!(two["RICHOS_CLAUDE_ACCOUNT_DIR"].as_deref(), folder.to_str(), "Account 2: the folder it was given");
    for (lease, env) in [("no account", &none), ("Account 1", &one)] {
        assert_eq!(env["RICHOS_CLAUDE_ACCOUNT_DIR"], None, "{lease}: no added account's folder, none inherited");
    }
}

/// **Media-tools plan section 3: the video skill finds the speech model in
/// `RICHOS_SPEECH_MODEL`**, set to the path the shell resolved (`speech_model_for_video`). With no model
/// resolved nothing is set, and a value inherited from the app's own environment never survives.
#[test]
fn the_provider_is_told_where_the_speech_model_is() {
    let f = Scratch::new();
    let mut profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let environment = |profile: &EngineProfile| {
        let mut command = std::process::Command::new("/fictional/provider");
        command.env("RICHOS_SPEECH_MODEL", "/fictional/inherited.bin");
        profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
        command.get_envs().map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(profile.speech_model, None, "prepare resolves no model; the shell does");
    assert_eq!(environment(&profile)["RICHOS_SPEECH_MODEL"], None, "an inherited model path is removed");
    let model = f.0.join("models/ggml-large-v3-turbo-q5_0.bin");
    profile.speech_model = Some(model.clone());
    assert_eq!(environment(&profile)["RICHOS_SPEECH_MODEL"].as_deref(), model.to_str());
}

/// **The video skill transcribes with the large model when it is installed and verified** (the
/// CEO, 2026-10-07, asked whether setup should download both speech models: "Both, in this
/// nightly, yes."), and with the model voice resolves otherwise.
#[test]
fn video_transcribes_with_the_large_model_when_it_is_verified() {
    use richos_core::engine_profile::speech_model_for_video;
    let large = PathBuf::from("/fictional/models/ggml-large-v3-turbo-q5_0.bin");
    let voice = PathBuf::from("/fictional/models/ggml-small.en.bin");
    assert_eq!(speech_model_for_video(Ok(large.clone()), || Some(voice.clone())), Some(large),
               "a verified large model is the one the video skill uses");
    assert_eq!(speech_model_for_video(Err("not installed".into()), || Some(voice.clone())), Some(voice),
               "without it, the model voice resolves");
    assert_eq!(speech_model_for_video(Err("not installed".into()), || None), None,
               "with neither, nothing is set");
}

#[test]
fn interrupted_initialization_recovers_without_a_second_initial_commit() {
    let f = Scratch::new();
    let root = f.0.join("coordination"); std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("app-owned-coordination.json"), "{\"schema\":1,\"owner\":\"richos-app\"}\n").unwrap();
    assert!(std::process::Command::new("/usr/bin/git").args(["init", "-q", "-b", "main"]).current_dir(&root).status().unwrap().success());
    prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let count = std::process::Command::new("/usr/bin/git").args(["rev-list", "--count", "HEAD"]).current_dir(root).output().unwrap();
    assert_eq!(String::from_utf8(count.stdout).unwrap().trim(), "1");
}

#[test]
fn private_paths_and_generated_files_cannot_redirect_writes() {
    let f = Scratch::new();
    let elsewhere = f.0.join("elsewhere"); std::fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, f.0.join("engine-state")).unwrap();
    assert!(prepare_fixture(&engine(), &f.0, f.runtime()).is_err());
    assert_eq!(std::fs::read_dir(elsewhere).unwrap().count(), 0);
}

#[test]
fn workspace_partitions_are_stable_per_company_thread_and_distinct_across_threads() {
    use richos_core::{EntityId, EntityRegistry, Ledger, Spine};
    let f=Scratch::new(); let mut spine=Spine::new(Ledger::open(&f.0.join("ledger.jsonl")).unwrap());
    let entity=EntityId::parse("fictional").unwrap();
    spine.set_entity_registry(EntityRegistry::from_existing_ids(&[entity.clone()]));
    let first=spine.create_thread("First", &entity).unwrap();
    let second=spine.create_thread("Second", &entity).unwrap();
    let mut a=prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut b=prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_ne!(a.workspace_state(),b.workspace_state());
    a.scope_to(&spine.ledger().thread_binding(&first).unwrap()).unwrap();
    b.scope_to(&spine.ledger().thread_binding(&first).unwrap()).unwrap();
    assert_eq!(a.workspace_state(),b.workspace_state());
    b.scope_to(&spine.ledger().thread_binding(&second).unwrap()).unwrap();
    assert_ne!(a.workspace_state(),b.workspace_state());
    assert_ne!(a.target_state(),b.target_state());
    assert!(a.target_state().is_dir());
}

#[test]
fn the_engine_delivers_distinct_plain_definitions_without_composing_them() {
    let plugin = engine().join("app");
    richos_core::skills::verify_present(&plugin).unwrap();
    let rich = std::fs::read_to_string(plugin.join("agents/rich.md")).unwrap();
    let stu = std::fs::read_to_string(plugin.join("agents/stu.md")).unwrap();
    assert!(rich.contains("You are Rich"));
    assert!(!rich.contains("You are Stu"));
    assert!(stu.contains("You are Stu"));
    assert!(!stu.contains("You are Rich"));
    for body in [&rich, &stu] {
        assert!(body.contains("RichOS"));
        assert!(body.contains("top-level agents"));
    }
}

#[test]
fn generic_git_identity_is_a_default_and_explicit_assignment_identity_can_override_it() {
    let f=Scratch::new(); let profile=prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut configured=std::process::Command::new("/fictional/provider");
    profile.configure(&mut configured,Path::new("/fictional/provider"),"fixture-session",&f.0.join("scope.json"));
    // A user whose own Git settings name nobody: an empty fixture home, so the person running
    // this test is never read.
    let home=f.0.join("home without git settings"); std::fs::create_dir_all(&home).unwrap();
    for (extra, expected) in [(vec![],"RichOS"), (vec!["-c","user.name=Fixture"],"Fixture")] {
        let mut git=std::process::Command::new("/usr/bin/git");
        for (key,value) in configured.get_envs() {
            if let Some(value)=value { git.env(key,value); } else { git.env_remove(key); }
        }
        git.env("HOME",&home).env_remove("XDG_CONFIG_HOME");
        let output=git.current_dir(&profile.coordination).args(extra).args(["config","--get","user.name"]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
}

/// **The name the user gives the app reaches a running lease's next commit** (round 41 piece 2):
/// the lease's lowest layer includes the answer's file, so a lease prepared before the answer
/// commits as the user after it, and the user's own `~/.gitconfig` still wins over it.
#[test]
fn the_git_name_the_user_saves_reaches_a_running_lease_and_their_own_git_setup_still_wins() {
    let f = Scratch::new(); let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut configured = std::process::Command::new("/fictional/provider");
    profile.configure(&mut configured, Path::new("/fictional/provider"), "fixture-session", &f.0.join("scope.json"));
    let home = f.0.join("home without git settings"); std::fs::create_dir_all(&home).unwrap();
    let ask = |key: &str| {
        let mut git = std::process::Command::new("/usr/bin/git");
        for (k, v) in configured.get_envs() { if let Some(v) = v { git.env(k, v); } else { git.env_remove(k); } }
        let out = git.env("HOME", &home).env_remove("XDG_CONFIG_HOME").current_dir(&profile.coordination)
            .args(["config", "--get", key]).output().unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    assert_eq!(ask("user.name"), "RichOS");
    richos_core::git_name::save(&profile.state, "Fixture Whitfield", "fixture@northwind.invalid").unwrap();
    assert_eq!((ask("user.name"), ask("user.email")), ("Fixture Whitfield".into(), "fixture@northwind.invalid".into()));
    std::fs::write(home.join(".gitconfig"), "[user]\n\tname = Their Own\n\temail = own@example.invalid\n").unwrap();
    assert_eq!(ask("user.name"), "Their Own", "the user's own Git setup wins");
    richos_core::git_name::use_richos(&profile.state).unwrap();
    std::fs::remove_file(home.join(".gitconfig")).unwrap();
    assert_eq!(ask("user.name"), "RichOS", "Use RichOS keeps the stand-in");
}

/// The Git name wait is Git's own hook in the lease's Git layer, as the app's own executable, and
/// no Claude Code hook changes.
#[test]
fn the_git_name_wait_is_a_git_hook_in_the_leases_layer_and_changes_no_claude_hook() {
    let f = Scratch::new();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let hooks = || std::fs::read(profile.plugin.join("hooks/hooks.json")).unwrap();
    let (before, layer_before) = (hooks(), std::fs::read_to_string(profile.plugin.join("gitconfig")).unwrap());
    let exe = Path::new("/Applications/RichOS Test.app/Contents/MacOS/richos");
    profile.install_git_name_wait(exe).unwrap();
    assert_eq!(hooks(), before, "no Claude Code hook moved");
    let layer = std::fs::read_to_string(profile.plugin.join("gitconfig")).unwrap();
    assert_eq!(layer, layer_before + &richos_core::git_name::hook_section(exe));
    let event = std::process::Command::new("/usr/bin/git").args(["config", "--file"]).arg(profile.plugin.join("gitconfig"))
        .args(["--get", "hook.richos-git-name.event"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&event.stdout).trim(), "prepare-commit-msg");
}

#[test]
fn bypass_permissions_match_the_terminal_and_keep_company_and_thread_read_roots() {
    use richos_core::{EntityId, EntityRegistry, Ledger, Spine};
    let f=Scratch::new();
    let alpha=EntityId::parse("alpha").unwrap();let beta=EntityId::parse("beta").unwrap();
    let mut registry=EntityRegistry::from_existing_ids(&[alpha.clone(),beta.clone()]);
    let a=f.0.join("alpha repository");let b=f.0.join("beta repository");
    std::fs::create_dir(&a).unwrap();std::fs::create_dir(&b).unwrap();
    registry.connect_repository(&alpha,a.clone()).unwrap();registry.connect_repository(&beta,b.clone()).unwrap();
    registry.save(&f.0.join("entities.json")).unwrap();
    let mut spine=Spine::new(Ledger::open(&f.0.join("ledger.jsonl")).unwrap());spine.set_entity_registry(registry);
    let thread=spine.create_thread("Alpha",&alpha).unwrap();
    let mut profile=prepare_fixture(&engine(),&f.0,f.runtime()).unwrap();
    profile.scope_to(&spine.ledger().thread_binding(&thread).unwrap()).unwrap();
    let mut command=std::process::Command::new("/fictional/provider");profile.configure(&mut command,Path::new("/fictional/provider"),"session",&f.0.join("scope"));
    let args:Vec<_>=command.get_args().map(|a|a.to_string_lossy().to_string()).collect();
    assert!(args.windows(2).any(|pair|pair==["--permission-mode","bypassPermissions"]));
    let start=args.iter().position(|arg|arg=="--add-dir").unwrap()+1;
    let directories:Vec<_>=args[start..].iter().take_while(|arg|!arg.starts_with("--")).map(String::as_str).collect();
    // THIS conversation's attached files (CEO §86): without them in a read root, an attachment
    // from the phone or the Mac is a path Rich can see and a file he cannot open.
    let files=richos_core::attachments::conversation_folder(&f.0,thread.as_str());
    assert_eq!(profile.attachments_folder().as_deref(),Some(files.as_path()));
    assert_eq!(directories,vec![a.to_str().unwrap(),profile.target_state().to_str().unwrap(),files.to_str().unwrap()]);
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    let inspector_roots: Vec<String> = serde_json::from_str(environment["RICHOS_APP_READ_ROOTS"].as_deref().unwrap()).unwrap();
    assert_eq!(inspector_roots, directories);
    assert!(!inspector_roots.iter().any(|root|root==b.to_str().unwrap()));
    // It exists before the session starts, and it is private.
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&files).unwrap().permissions().mode()&0o777,0o700);
    // Never the root, and never another conversation's folder.
    let beta_thread=spine.create_thread("Beta",&beta).unwrap();
    let other=richos_core::attachments::conversation_folder(&f.0,beta_thread.as_str());
    assert!(!args.iter().any(|arg|arg==f.0.join("attachments").to_str().unwrap()||arg==other.to_str().unwrap()));
    let settings:serde_json::Value=serde_json::from_str(&args[args.iter().position(|arg|arg=="--settings").unwrap()+1]).unwrap();
    assert!(settings["permissions"]["blockReadsOutsideWorkingDirectories"].is_null());
    assert!(settings["sandbox"].is_null(), "the app must preserve the baseline sandbox policy");
    assert!(settings["autoMode"].is_null(), "bypass must not carry classifier configuration");
    assert_eq!(environment["RICHOS_AGENT_HOLD_DIR"].as_deref(), Some(profile.state.join("agent-hold").to_str().unwrap()));
    assert!(!settings.to_string().contains(b.to_str().unwrap()));
}

// =======================================================================================
// WHOSE GUARDS JUDGE A DISPATCH THE APP MAKES — Rich's ruling, 2026-09-18, on CEO §57
// =======================================================================================
//
// *"The app's prepare/spawn path is judged only by guards whose reason protects the app
// user's own work on the app user's own machine."* Three facts have to hold together for
// that to be true of the shipped app, and each of them was wrong in a different way before
// this slice:
//
//   1. the preflight the app writes is DERIVED from the declaration, not typed;
//   2. no operator-session guard is on it (`guard-brief-scope.sh` was);
//   3. `RICHOS_SPAWN_HOOK_SOURCES` points `spawn.py` at that preflight — the lease always
//      set it and the probe never did, which is the whole of the "refused by 1 of 9
//      guard(s)" finding of 2026-09-18.

#[test]
fn the_app_preflight_is_the_declared_user_work_list_and_nothing_else() {
    let f = Scratch::new();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(profile.plugin.join("spawn-preflight.json")).unwrap()).unwrap();
    let groups = manifest["hooks"]["PreToolUse"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "one Agent group");
    assert_eq!(groups[0]["matcher"], "Agent");
    let written: Vec<String> = groups[0]["hooks"].as_array().unwrap().iter()
        .map(|hook| hook["command"].as_str().unwrap()
             .trim_end_matches('\'').rsplit('/').next().unwrap().to_string())
        .collect();
    let declared = richos_core::engine_profile::user_work_guards(&engine()).unwrap();
    assert_eq!(written, declared,
        "the preflight must BE the declared user-work list, in order, with nothing added");
    assert!(!written.is_empty(), "an empty preflight is an unguarded dispatch");

    // The operator-session side, named rather than inferred from the absence of a name.
    // `guard-brief-scope.sh` is the one that was on this list until 2026-09-18.
    let text = std::fs::read_to_string(engine().join(
        richos_core::engine_profile::GUARD_AUDIENCE_DECLARATION)).unwrap();
    let mut operator = Vec::new();
    for record in text.split("\n\n") {
        let id = record.lines().find_map(|l| l.strip_prefix("id: ")).map(str::trim);
        let audience = record.lines().find_map(|l| l.strip_prefix("audience: ")).map(str::trim);
        if let (Some(id), Some("operator-session")) = (id, audience) { operator.push(id.to_string()); }
    }
    assert!(operator.contains(&"guard-brief-scope.sh".to_string()),
        "the guard this slice removed from the app must still be CLASSIFIED, not deleted");
    for guard in &operator {
        assert!(!written.contains(guard), "{guard} is operator-session and is on the app's preflight");
    }
}

#[test]
fn the_provider_child_is_pinned_to_that_preflight_and_to_nothing_else() {
    let f = Scratch::new();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut command = std::process::Command::new("/fictional/provider");
    // A machine whose own sessions register the engine's nine guards: the variable must be
    // REPLACED, never inherited, or the app is judged by whatever that machine installed.
    command.env("RICHOS_SPAWN_HOOK_SOURCES", "engine=/fictional/hooks.json");
    profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = command.get_envs()
        .map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
        .collect();
    assert_eq!(environment["RICHOS_SPAWN_HOOK_SOURCES"].as_deref(),
        Some(profile.spawn_hook_sources().as_str()),
        "the lease must pin spawn.py at the app's own preflight");
    assert_eq!(profile.spawn_hook_sources(),
        format!("app={}", profile.plugin.join("spawn-preflight.json").display()),
        "the accessor and the file the profile actually wrote must be the same path");
}

#[test]
fn a_declaration_that_cannot_be_read_is_an_error_and_never_an_empty_allowlist() {
    let f = Scratch::new();
    let broken = f.0.join("broken-engine");
    std::fs::create_dir_all(&broken).unwrap();
    let declaration = broken.join(richos_core::engine_profile::GUARD_AUDIENCE_DECLARATION);
    // Absent.
    assert!(richos_core::engine_profile::user_work_guards(&broken).is_err());
    // Present and classifying nothing as user-work: an unguarded dispatch, refused.
    std::fs::write(&declaration, "id: guard-a.sh\naudience: operator-session\nreason: fixture\n").unwrap();
    assert!(richos_core::engine_profile::user_work_guards(&broken).is_err());
    // User-work with no sentence for the user: refused, the same rule the Python reader keeps.
    std::fs::write(&declaration, "id: guard-a.sh\naudience: user-work\nreason: fixture\n").unwrap();
    assert!(richos_core::engine_profile::user_work_guards(&broken).is_err());
    // A path masquerading as a guard file name: refused.
    std::fs::write(&declaration,
        "id: ../../../bin/sh\naudience: user-work\nreason: fixture\nuser_message: m\n").unwrap();
    assert!(richos_core::engine_profile::user_work_guards(&broken).is_err());
    // The positive control, so the four refusals above are not all failing for one
    // unrelated reason.
    std::fs::write(&declaration,
        "id: guard-a.sh\naudience: user-work\nreason: fixture\nuser_message: Nothing was created.\n").unwrap();
    assert_eq!(richos_core::engine_profile::user_work_guards(&broken).unwrap(), vec!["guard-a.sh"]);
}

/// **Slice 1 of the proto-teammate shelf plan** (richos-hq
/// `docs/plans/2026-10-06-proto-teammate-shelf.md` §2, §9 step 2): every lease registers the
/// five always-active teammates (Dean, Clark, Reed, Frank and Pierce) and every file in
/// `<app data>/team/`, in the plugin's `agents` list and in the private roster the isolation
/// guard reads; a teammate no longer active leaves the roster; the shelf is never registered.
/// Since slice 2 the two generic duties are not registered either, and a user file named for
/// one is not.
#[test]
fn a_lease_registers_the_always_active_and_the_users_team_and_drops_what_is_no_longer_active() {
    let f = Scratch::new();
    let roster = f.0.join("coordination/.claude/agents");
    let team = f.0.join("team");
    std::fs::create_dir_all(&team).unwrap();
    std::fs::write(team.join("mark.md"), "---\nname: mark\nmodel: sonnet\n---\nFictional fitted engineer.\n").unwrap();
    std::fs::write(team.join("dean.md"), "---\nname: dean\nmodel: sonnet\n---\nThe user's own refit of Dean.\n").unwrap();
    std::fs::write(team.join("notes.txt"), "not a definition").unwrap();
    std::fs::write(team.join("worker.md"), "---\nname: worker\n---\nA user file may not replace the app mechanics.\n").unwrap();

    let registered = |profile: &EngineProfile| -> Vec<String> {
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(profile.plugin.join(".claude-plugin/plugin.json")).unwrap()).unwrap();
        let mut names: Vec<String> = manifest["agents"].as_array().unwrap().iter().map(|a| {
            let path = a.as_str().unwrap();
            assert!(profile.plugin.join(path).is_file(), "{path} is listed but was not written");
            path.trim_start_matches("./agents/").trim_end_matches(".md").to_string()
        }).collect();
        names.sort();
        names
    };
    let on_roster = || -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&roster).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().trim_end_matches(".md").to_string()).collect();
        names.sort();
        names
    };

    let first = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let expected = ["clark", "dean", "frank", "mark", "pierce", "reed"];
    assert_eq!(registered(&first), expected);
    assert_eq!(on_roster(), expected);
    for stock in ["clark", "reed", "frank", "pierce"] {
        let shipped = richos_core::engine_profile::with_resource_access(&std::fs::read_to_string(engine().join(format!("agents/{stock}.md"))).unwrap()).into_bytes();
        assert_eq!(std::fs::read(first.plugin.join(format!("agents/{stock}.md"))).unwrap(), shipped);
        assert_eq!(std::fs::read(roster.join(format!("{stock}.md"))).unwrap(), shipped);
    }
    for user in ["mark", "dean"] {
        let want = std::fs::read_to_string(team.join(format!("{user}.md"))).unwrap();
        assert_eq!(std::fs::read_to_string(first.plugin.join(format!("agents/{user}.md"))).unwrap(), want,
            "the user's {user} is registered as written");
        assert_eq!(std::fs::read_to_string(roster.join(format!("{user}.md"))).unwrap(), want);
    }
    // The shelf is never registered: none of its names reaches the plugin or the roster.
    for shelf in std::fs::read_dir(engine().join("team/shelf")).unwrap() {
        let name = shelf.unwrap().file_name().to_string_lossy().trim_end_matches(".md").to_string();
        if name == "mark" { continue; } // activated above, as the user's own
        assert!(!registered(&first).contains(&name) && !on_roster().contains(&name), "{name} is on the shelf");
    }

    // The same live plugin and guard roster accept a new hire without another profile.
    std::fs::write(team.join("fresh.md"), "---\nname: fresh\nmodel: sonnet\n---\nFictional newly hired teammate.\n").unwrap();
    let reloaded=first.refresh_team().unwrap();
    assert!(reloaded.contains(&"richos-app-engine:fresh".to_string()));
    assert!(registered(&first).contains(&"fresh".to_string()));
    assert_eq!(std::fs::read(first.plugin.join("agents/fresh.md")).unwrap(),std::fs::read(team.join("fresh.md")).unwrap());
    let loaded_mark=std::fs::read(first.plugin.join("agents/mark.md")).unwrap();
    let revised_mark="---\nname: mark\nmodel: opus\n---\nRevised definition for the next Stu session.\n";
    std::fs::write(team.join("mark.md"),revised_mark).unwrap();
    first.refresh_team().unwrap();
    assert_eq!(std::fs::read(first.plugin.join("agents/mark.md")).unwrap(),loaded_mark,"a saved revision must not change a loaded definition");
    let later=prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_eq!(std::fs::read_to_string(later.plugin.join("agents/mark.md")).unwrap(),revised_mark);
    std::fs::remove_file(team.join("fresh.md")).unwrap();
    first.refresh_team().unwrap();
    assert!(!first.plugin.join("agents/fresh.md").exists());
    assert!(!roster.join("fresh.md").exists());

    // Mark is retired and a stale file from an older lease sits on the roster: the next lease
    // drops both and falls back to the stock Dean once the user's refit is gone.
    std::fs::remove_file(team.join("mark.md")).unwrap();
    std::fs::remove_file(team.join("dean.md")).unwrap();
    std::fs::write(roster.join("retired.md"), "---\nname: retired\n---\n").unwrap();
    // The generic worker an install registered before slice 2 leaves the roster the same way.
    std::fs::write(roster.join("worker.md"), "---\nname: worker\n---\n").unwrap();
    let next = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let stock = ["clark", "dean", "frank", "pierce", "reed"];
    assert_eq!(registered(&next), stock);
    assert_eq!(on_roster(), stock);
    assert_eq!(std::fs::read_to_string(roster.join("dean.md")).unwrap(), richos_core::engine_profile::with_resource_access(&std::fs::read_to_string(engine().join("agents/dean.md")).unwrap()));

    // A fresh install has no team folder at all and gets the stock five.
    let fresh = Scratch::new();
    let profile = prepare_fixture(&engine(), &fresh.0, fresh.runtime()).unwrap();
    assert_eq!(registered(&profile), stock);
}

/// Saved definitions stay intact; generated tool lists also grant the shared host read.
#[test]
fn saved_teammate_instructions_stay_intact_and_generated_lists_gain_host_read() {
    let f = Scratch::new();
    let team = f.0.join("team");
    std::fs::create_dir_all(&team).unwrap();
    for (name, body) in [
        ("scout", "---\nname: scout\ntools: [Read, Bash]\n---\nUser instructions.\n"),
        ("writer", "---\nname: writer\ntools:\n  - Read\n---\nDifferent instructions.\n"),
    ] {
        std::fs::write(team.join(format!("{name}.md")), body).unwrap();
    }
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    for name in ["scout", "writer"] {
        let original = std::fs::read(team.join(format!("{name}.md"))).unwrap();
        for registered in [profile.plugin.join(format!("agents/{name}.md")),
                           profile.coordination.join(format!(".claude/agents/{name}.md"))] {
            let loaded = std::fs::read_to_string(registered).unwrap();
            let expected = if name=="scout" {
                "---\nname: scout\ntools: [Read, Bash, mcp__richos_status__system_resources]\n---\nUser instructions.\n"
            } else {
                "---\nname: writer\ntools:\n  - mcp__richos_status__system_resources\n  - Read\n---\nDifferent instructions.\n"
            };
            assert_eq!(loaded,expected);
            assert_eq!(std::fs::read(team.join(format!("{name}.md"))).unwrap(),original);
        }
    }
}

#[test]
fn resource_access_preserves_commented_and_multiline_tool_definitions() {
    use richos_core::engine_profile::with_resource_access;
    let tool="mcp__richos_status__system_resources";
    for (before,after) in [
        ("tools: Read, Bash # keep these",format!("tools: Read, Bash, {tool} # keep these")),
        ("tools: \"Read, Bash\" # label",format!("tools: \"Read, Bash, {tool}\" # label")),
        ("tools: [Read] # a comment with ]",format!("tools: [Read, {tool}] # a comment with ]")),
        ("tools: [\n  Read,\n  Bash, # last tool\n]",format!("tools: [Read,\n  Bash, {tool}]")),
        ("tools: # list\n# keep this comment\n  - Read",format!("tools: # list\n# keep this comment\n  - {tool}\n  - Read")),
        ("tools: Read # mcp__richos_status__system_resources is not yet allowed",format!("tools: Read, {tool} # mcp__richos_status__system_resources is not yet allowed")),
        ("tools: Read, Glob,\n  Grep, Bash",format!("tools: Read, Glob,\n  Grep, Bash, {tool}")),
        ("tools: Read, Glob,\n  Grep, Bash, # keep",format!("tools: Read, Glob,\n  Grep, Bash, {tool} # keep")),
        ("tools: Read, Bash,",format!("tools: Read, Bash, {tool}")),
    ] {
        let original=format!("---\nname: fixture\n{before}\nmodel: sonnet\n---\nInstructions stay exactly here.\n");
        let expected=format!("---\nname: fixture\n{after}\nmodel: sonnet\n---\nInstructions stay exactly here.\n");
        let rendered=with_resource_access(&original);
        assert_eq!(rendered,expected);
        assert_eq!(with_resource_access(&rendered),rendered,"the host read was duplicated");
    }
}

/// **Slice 5: the digest that renews a back end when its team changed** (plan §1, §6). It
/// moves with every change the next lease would register (a teammate saved, refitted or
/// retired) and with none it would not (the shelf, a temporary file, a duty name, a file that
/// is not a definition), so a renewal is never spent on a change no lease can see.
#[test]
fn the_team_digest_moves_with_what_a_lease_registers_and_nothing_else() {
    use richos_core::engine_profile::team_digest;
    let f = Scratch::new();
    let team = f.0.join("team");
    let digest = || team_digest(&engine(), &f.0).unwrap();
    let fresh = digest();
    std::fs::create_dir_all(&team).unwrap();
    assert_eq!(digest(), fresh, "an empty team folder registers nothing new");
    for (file, body) in [("notes.txt", "not a definition"), (".0a1b.incoming", "---\nname: mark\n---\n"),
                         ("worker.md", "---\nname: worker\n---\n"), ("Scout.md", "---\nname: Scout\n---\n")] {
        std::fs::write(team.join(file), body).unwrap();
        assert_eq!(digest(), fresh, "{file} is not registered, so it must not renew a back end");
    }
    std::fs::write(team.join("mark.md"), "---\nname: mark\nmodel: sonnet\n---\nFitted engineer.\n").unwrap();
    let saved = digest();
    assert_ne!(saved, fresh, "a saved teammate must move the digest");
    assert_eq!(digest(), saved, "the digest is stable while nothing changes");
    std::fs::write(team.join("mark.md"), "---\nname: mark\nmodel: opus\n---\nFitted engineer.\n").unwrap();
    assert_ne!(digest(), saved, "a refit must move the digest");
    std::fs::remove_file(team.join("mark.md")).unwrap();
    assert_eq!(digest(), fresh, "retiring the teammate gives the stock team's digest again");
}

/// CEO 2026-10-14 prioritizes immediate replies over effort. Each role's flags come
/// from the one function `spawn_with_tools` appends to the child's argument vector, given the
/// profile every product lease is spawned with, which reads `config.json` in the data folder.
#[test]
fn each_role_keeps_its_model_with_low_effort_for_replies_and_high_for_work() {
    use richos_core::config::{ConfigStore, ModelChoice};
    use richos_core::native::lease_model_args;
    let rich_opus = ["--model", "opus", "--effort", "low"];
    let rich_fable = ["--model", "fable", "--effort", "low"];
    let stu_opus = ["--model", "opus", "--effort", "high"];
    let stu_fable = ["--model", "fable", "--effort", "high"];
    let f = Scratch::new();

    // No setting written: both on Opus, with effort appropriate to their role.
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_eq!(lease_model_args(LeaseRole::Conversation, Some(&profile)), rich_opus, "Rich's default");
    assert_eq!(lease_model_args(LeaseRole::Work, Some(&profile)), stu_opus, "Stu's default");

    // Rich switched to Fable; Stu stays on Opus. Read at the next spawn's `prepare`.
    ConfigStore::open(f.0.join("config.json")).unwrap().set_rich_model(ModelChoice::Fable).unwrap();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_eq!(lease_model_args(LeaseRole::Conversation, Some(&profile)), rich_fable, "Rich set to Fable");
    assert_eq!(lease_model_args(LeaseRole::Work, Some(&profile)), stu_opus, "Stu was not switched");

    // Both on Fable.
    ConfigStore::open(f.0.join("config.json")).unwrap().set_stu_model(ModelChoice::Fable).unwrap();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_eq!(lease_model_args(LeaseRole::Conversation, Some(&profile)), rich_fable);
    assert_eq!(lease_model_args(LeaseRole::Work, Some(&profile)), stu_fable, "Stu set to Fable");

    // And back: Rich returned to Opus, Stu keeps Fable.
    ConfigStore::open(f.0.join("config.json")).unwrap().set_rich_model(ModelChoice::Opus).unwrap();
    let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_eq!(lease_model_args(LeaseRole::Conversation, Some(&profile)), rich_opus, "Rich switched back");
    assert_eq!(lease_model_args(LeaseRole::Work, Some(&profile)), stu_fable);
}

#[test]
fn machine_settings_survive_filtering_and_hold_a_job_snapshot() {
    let f = Scratch::new();
    let settings = richos_core::machine_settings::parse("RICHOS_BULK_ROOT=/external data\nCARGO_TARGET_DIR=/component cache").unwrap();
    let profile = EngineProfile::prepare_with_settings(&engine(), &f.0, f.runtime(), settings).unwrap();
    let mut command = std::process::Command::new("claude");
    command.env("RICHOS_UNSUPPORTED", "private").env("RICHOS_BULK_ROOT", "/old");
    profile.configure(&mut command, Path::new("/fictional/provider"), "session", &f.0);
    let values: BTreeMap<_,_> = command.get_envs().map(|(key,value)|(key.to_string_lossy().to_string(),value.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(values["RICHOS_BULK_ROOT"].as_deref(), Some("/external data"));
    assert_eq!(values["CARGO_TARGET_DIR"].as_deref(), Some("/component cache"));
    assert_eq!(values["RICHOS_MACHINE_SETTINGS_RESOLVED"].as_deref(), Some("1"));
    assert_eq!(values["RICHOS_UNSUPPORTED"], None);
}

/// **The home screen names the teammates the user added, and only them** (product hunt
/// 2026-10-15, finding 7: a made-up company of 18 specialists on the home screen of a user who
/// had just hired two). Name and role come from Dean's `# {Name} — {Role Title}` heading; the
/// shelf's trailing parenthesis is dropped; a definition with no heading is named by its file.
/// The always-active five (a refit of Dean included), the duty names and a file that is not a
/// definition are not his team. No folder is no team, never an error.
#[test]
fn the_home_screen_team_is_the_users_own_teammates_by_name_and_role() {
    use richos_core::engine_profile::{user_teammates, Teammate};
    let f = Scratch::new();
    assert_eq!(user_teammates(&f.0).unwrap(), Vec::<Teammate>::new());
    let team = f.0.join("team");
    std::fs::create_dir_all(&team).unwrap();
    std::fs::write(team.join("will.md"), "---\nname: will\n# a yaml comment\nmodel: sonnet\n---\n\n# Will — Marketing Director\n\nYou are Will.\n").unwrap();
    std::fs::write(team.join("beth.md"), "---\nname: beth\n---\n# Beth — Bookkeeper (Small-Business Books & Payroll)\n").unwrap();
    std::fs::write(team.join("nora.md"), "---\nname: nora\n---\nNo heading at all.\n").unwrap();
    std::fs::write(team.join("dean.md"), "---\nname: dean\n---\n# Dean — HR Director\n").unwrap();
    std::fs::write(team.join("worker.md"), "---\nname: worker\n---\n# Worker — Duty\n").unwrap();
    std::fs::write(team.join("notes.txt"), "# Notes — not a definition\n").unwrap();
    assert_eq!(user_teammates(&f.0).unwrap(), vec![
        Teammate { name: "Beth".into(), role: Some("Bookkeeper".into()) },
        Teammate { name: "Nora".into(), role: None },
        Teammate { name: "Will".into(), role: Some("Marketing Director".into()) },
    ]);
}

#[test]
fn a_repository_connected_to_two_companies_is_a_read_root_in_each_companys_conversation() {
    use richos_core::{EntityId, EntityRegistry, Ledger, Spine};
    let f=Scratch::new();
    let alpha=EntityId::parse("alpha").unwrap();let beta=EntityId::parse("beta").unwrap();
    let mut registry=EntityRegistry::from_existing_ids(&[alpha.clone(),beta.clone()]);
    let shared=f.0.join("shared repository");
    std::fs::create_dir(&shared).unwrap();
    registry.connect_repository(&alpha,shared.clone()).unwrap();
    registry.connect_repository(&beta,shared.clone()).unwrap();
    registry.save(&f.0.join("entities.json")).unwrap();
    let mut spine=Spine::new(Ledger::open(f.0.join("ledger.jsonl")).unwrap());spine.set_entity_registry(registry);
    for (name,id) in [("Alpha",&alpha),("Beta",&beta)] {
        let thread=spine.create_thread(name,id).unwrap();
        let mut profile=prepare_fixture(&engine(),&f.0,f.runtime()).unwrap();
        profile.scope_to(&spine.ledger().thread_binding(&thread).unwrap()).unwrap();
        let mut command=std::process::Command::new("/fictional/provider");profile.configure(&mut command,Path::new("/fictional/provider"),"session",&f.0.join("scope"));
        let args:Vec<_>=command.get_args().map(|a|a.to_string_lossy().to_string()).collect();
        let start=args.iter().position(|arg|arg=="--add-dir").unwrap()+1;
        assert_eq!(args[start],shared.to_str().unwrap(),"{name} conversation must reach the shared repository");
    }
}

/// **Rich and Stu can answer Codex** (gap X): each Codex channel folder the user's setting names
/// (`CODEX_CHANNELS` in their `orchestration.config`) that exists is one of the session's
/// folders, so `to-codex.md` there can be written; one that does not exist is left out, and the
/// read-only inspectors' roots are unchanged.
#[test]
fn the_users_codex_channel_folders_are_folders_of_the_session() {
    use richos_core::{EntityId, EntityRegistry, Ledger, Spine};
    let f=Scratch::new();
    let alpha=EntityId::parse("alpha").unwrap();
    let mut spine=Spine::new(Ledger::open(f.0.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(EntityRegistry::from_existing_ids(std::slice::from_ref(&alpha)));
    let thread=spine.create_thread("Alpha",&alpha).unwrap();
    let (central,channel,missing)=(f.0.join("central"),f.0.join("codex channel"),f.0.join("not there"));
    std::fs::create_dir_all(&central).unwrap();std::fs::create_dir_all(&channel).unwrap();
    std::fs::write(central.join("orchestration.config"),format!("CODEX_CHANNELS=\"{}:{}\"\n",channel.display(),missing.display())).unwrap();
    let mut profile=prepare_for_user(&f,&central);
    profile.scope_to(&spine.ledger().thread_binding(&thread).unwrap()).unwrap();
    let mut command=std::process::Command::new("/fictional/provider");profile.configure(&mut command,Path::new("/fictional/provider"),"session",&f.0.join("scope"));
    let args:Vec<_>=command.get_args().map(|a|a.to_string_lossy().to_string()).collect();
    assert_eq!(args.iter().filter(|arg|*arg=="--add-dir").count(),1,"one --add-dir keeps every folder");
    let start=args.iter().position(|arg|arg=="--add-dir").unwrap()+1;
    let directories:Vec<_>=args[start..].iter().take_while(|arg|!arg.starts_with("--")).map(String::as_str).collect();
    assert_eq!(directories.last().copied(),Some(channel.to_str().unwrap()),"{directories:?}");
    assert!(!directories.contains(&missing.to_str().unwrap()),"a folder that does not exist: {directories:?}");
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    let inspector_roots: Vec<String> = serde_json::from_str(environment["RICHOS_APP_READ_ROOTS"].as_deref().unwrap()).unwrap();
    assert!(!inspector_roots.contains(&channel.to_str().unwrap().to_string()),"{inspector_roots:?}");
    // Set empty, no channel folder.
    std::fs::write(central.join("orchestration.config"),"CODEX_CHANNELS=\"\"\n").unwrap();
    let mut command=std::process::Command::new("/fictional/provider");profile.configure(&mut command,Path::new("/fictional/provider"),"session",&f.0.join("scope"));
    assert!(!command.get_args().any(|arg|arg==channel.as_os_str()),"set empty, the channel stays a session folder");
}

// =======================================================================================
// THE USER'S OWN SETTINGS FILES — CEO ruling §138; build plan 2026-10-21, item 0
// =======================================================================================

/// The user's copy as a session loads it: with the memory scope the app adds when the copy
/// names none (`user_settings::MEMORY_SCOPE`).
fn as_loaded(copy: &str) -> String {
    copy.replacen("---\n", &format!("---\n{}\n", richos_core::user_settings::MEMORY_SCOPE), 1)
}

fn prepare_for_user(f: &Scratch, folder: &Path) -> EngineProfile {
    EngineProfile::prepare_for(&engine(), &f.0.join("data"), f.runtime(),
        richos_core::machine_settings::Settings::default(), Some(folder.to_path_buf()), None).unwrap()
}

/// **Both folders of a lease say who made them** (review rv-20261025T021219Z-91f589b0-af71 #1):
/// the app's sweep reads each folder's own owner record, so a sweep that took the base and left
/// the `-standing` copy for the next one has not taken the copy's ownership proof with it.
#[test]
fn both_folders_of_a_lease_carry_this_apps_owner_record() {
    use richos_core::app_cleanup::OWNER_FILE;
    let f = Scratch::new();
    let profile = prepare_for_user(&f, &f.0.join("central"));
    let standing = profile.standing_plugin(&engine().join("app"));
    for folder in [&profile.plugin, &standing] {
        let record: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join(OWNER_FILE))
            .unwrap_or_else(|e| panic!("{} carries no owner record: {e}", folder.display()))).unwrap();
        assert_eq!(record["pid"], std::process::id(), "{}", folder.display());
        assert_eq!(record["procStart"].as_str(), richos_core::app_cleanup::own_start().as_deref(), "{}", folder.display());
    }
}

/// **The CEO, 2026-10-21: a technical user tweaks Rich's and Stu's own files by hand.** On
/// first run the shipped `rich.md` and `stu.md` (and an empty `instructions.md`) are copied to
/// the user's folder; every later session starts from those copies, an edit is in the next
/// session, and the app never writes them again.
#[test]
fn rich_and_stu_start_from_the_users_own_copies_which_the_app_never_writes_again() {
    use richos_core::user_settings::{RICH, STU, INSTRUCTIONS};
    let f = Scratch::new();
    let folder = f.0.join("central");
    let shipped = engine().join("app");
    let first = prepare_for_user(&f, &folder);
    for name in [RICH, STU] {
        assert_eq!(std::fs::read(folder.join(name)).unwrap(), std::fs::read(shipped.join("agents").join(name)).unwrap(),
            "{name} is copied as shipped on first run");
    }
    assert_eq!(std::fs::read_to_string(folder.join(INSTRUCTIONS)).unwrap(), "");
    let standing = first.standing_plugin(&shipped);
    assert_ne!(standing, shipped, "a session with a user folder must not start from the shipped plugin");
    richos_core::skills::verify_present(&standing).unwrap();

    let edited = std::fs::read_to_string(folder.join(RICH)).unwrap() + "\nFixture rule: end every reply with the word teal.\n";
    std::fs::write(folder.join(RICH), &edited).unwrap();
    std::fs::write(folder.join(INSTRUCTIONS), "My own rules.\n").unwrap();
    let next = prepare_for_user(&f, &folder);
    let standing = next.standing_plugin(&shipped);
    // The user's instructions.md follows each definition (a_line_in_the_users_instructions...).
    let loaded = std::fs::read_to_string(standing.join("agents").join(RICH)).unwrap();
    assert!(loaded.starts_with(&as_loaded(&edited)) && loaded.ends_with("My own rules.\n"), "the next session follows the edit");
    assert_eq!(std::fs::read_to_string(folder.join(RICH)).unwrap(), edited, "the user's rich.md was written over");
    assert_eq!(std::fs::read_to_string(folder.join(INSTRUCTIONS)).unwrap(), "My own rules.\n");
    assert!(std::fs::read_to_string(standing.join("agents").join(STU)).unwrap().starts_with(&as_loaded(&std::fs::read_to_string(folder.join(STU)).unwrap())));
    assert_ne!(first.user.digest, next.user.digest, "the edit must renew a session at its next boundary");
    assert_eq!(std::fs::read_to_string(first.standing_plugin(&shipped).join("agents").join(RICH)).unwrap(),
        as_loaded(&std::fs::read_to_string(shipped.join("agents").join(RICH)).unwrap()), "a running session's plugin was changed under it");
    // A retiring lease takes its own folders with it (second review rv-20261021T092945Z, finding 2).
    let standing = first.standing_plugin(&shipped);
    first.remove_session_files();
    assert!(!first.plugin.exists() && !standing.exists(), "a retired lease left its folders behind");
    assert!(next.standing_plugin(&shipped).is_dir(), "another lease's folders were removed");

    // Without a user folder (tests, an install with no home) the shipped plugin is used.
    assert_eq!(prepare_fixture(&engine(), &f.0.join("data"), f.runtime()).unwrap().standing_plugin(&shipped), shipped);

    // A link at the name is the user's choice of file and is never written through.
    let linked = Scratch::new();
    let elsewhere = linked.0.join("my-rich.md");
    std::fs::write(&elsewhere, "Kept elsewhere.\n").unwrap();
    let folder = linked.0.join("central");
    std::fs::create_dir_all(&folder).unwrap();
    std::os::unix::fs::symlink(&elsewhere, folder.join(RICH)).unwrap();
    let profile = prepare_for_user(&linked, &folder);
    assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "Kept elsewhere.\n");
    assert_eq!(std::fs::read_to_string(profile.standing_plugin(&shipped).join("agents").join(RICH)).unwrap(), "Kept elsewhere.\n");
}

/// **The user's instructions.md reaches both agents** (second review
/// rv-20261021T082940Z-f7fbba2a-0c12): a line written there is in Rich's and Stu's standing
/// instructions from the next session, in full; an empty file adds nothing.
#[test]
fn a_line_in_the_users_instructions_reaches_both_rich_and_stu() {
    use richos_core::user_settings::{RICH, STU, INSTRUCTIONS};
    let f = Scratch::new();
    let folder = f.0.join("central");
    let shipped = engine().join("app");
    let first = prepare_for_user(&f, &folder);
    for name in [RICH, STU] {
        assert_eq!(std::fs::read_to_string(first.standing_plugin(&shipped).join("agents").join(name)).unwrap(),
            as_loaded(&std::fs::read_to_string(folder.join(name)).unwrap()), "an empty instructions.md must add nothing to {name}");
    }
    let rule = "Fixture rule: sign every message with the word lighthouse.";
    let long = format!("{rule}\n{}", "Another fixture line that makes this file long.\n".repeat(400));
    std::fs::write(folder.join(INSTRUCTIONS), &long).unwrap();
    let next = prepare_for_user(&f, &folder);
    for name in [RICH, STU] {
        let loaded = std::fs::read_to_string(next.standing_plugin(&shipped).join("agents").join(name)).unwrap();
        assert!(loaded.starts_with(&as_loaded(&std::fs::read_to_string(folder.join(name)).unwrap())), "{name}'s own file comes first, frontmatter intact");
        assert!(loaded.contains(rule), "the user's rule is not in {name}'s standing instructions");
        assert!(loaded.ends_with(&long), "{name} did not get the user's instructions in full");
    }
    assert_ne!(first.user.digest, next.user.digest, "the edit must renew a session at its next boundary");
}

/// **Rich knows where a standing order goes** (richos-hq
/// `docs/plans/2026-10-21-switching-to-the-app.md` row W, section 1: the user's instructions file
/// is edited "by hand, or Rich when he gives a new order"). On a first run the file is empty, so the
/// app adds nothing about it to Rich's definition (the test above) and the only place Rich can learn
/// where it is, is his own instructions. The path is built from the code's own names, so moving the
/// file or the folder without the sentence breaks this.
#[test]
fn rich_s_own_instructions_name_the_file_a_standing_order_is_saved_in() {
    use richos_core::user_settings::{RICH, INSTRUCTIONS};
    let file = format!("~/{}/{INSTRUCTIONS}", richos_core::company::DEFAULT_CENTRAL_DIRNAME);
    let f = Scratch::new();
    let folder = f.0.join("central");
    let first = prepare_for_user(&f, &folder);
    assert_eq!(std::fs::read_to_string(folder.join(INSTRUCTIONS)).unwrap(), "", "a first run's instructions file is empty");
    let loaded = std::fs::read_to_string(first.standing_plugin(&engine().join("app")).join("agents").join(RICH)).unwrap();
    let says = loaded.split("\n\n").find(|paragraph| paragraph.contains(&file))
        .unwrap_or_else(|| panic!("Rich's definition on a first run never names {file}"));
    assert!(says.to_lowercase().contains("from now on"), "the paragraph naming {file} does not say a standing order goes there: {says}");
}

/// With no file of the user's, the generated config carries the engine's own model list, order
/// and cost ceiling (build plan item 7: the engine's full list, the shipped ceiling Opus); with
/// one, each key appears once: the user's value where the user set it, the engine's where not,
/// and the app's own key the app's, whatever the user's file says.
#[test]
fn the_users_orchestration_config_reaches_the_coordination_folder_beside_the_apps_own_keys() {
    let f = Scratch::new();
    let folder = f.0.join("central");
    let profile = prepare_for_user(&f, &folder);
    let teams = profile.state.join("teams");
    let shipped = std::fs::read_to_string(engine().join("orchestration.config")).unwrap();
    let engine_line = |key: &str| shipped.lines().find(|l| l.starts_with(&format!("{key}="))).unwrap().to_string();
    assert_eq!(engine_line("ALLOWED_MODELS"), "ALLOWED_MODELS=\"fable opus sonnet haiku\"");
    assert_eq!(engine_line("MODEL_CEILING"), "MODEL_CEILING=\"opus\"");
    let today = format!("# Generated desktop coordination, not target-repository configuration.\n{}\n{}\n{}\nSESSION_TEAMS_DIR='{}'\n",
        engine_line("ALLOWED_MODELS"), engine_line("MODEL_TIERS"), engine_line("MODEL_CEILING"), teams.display());
    assert_eq!(std::fs::read_to_string(profile.coordination.join("orchestration.config")).unwrap(), today,
        "a user with no orchestration.config gets the engine's models, order and ceiling");

    std::fs::write(folder.join("orchestration.config"),
        "# fixture user values\nMODEL_CEILING=\"sonnet\"\nexport ALLOWED_MODELS=\"sonnet haiku\"\nSESSION_TEAMS_DIR=/fixture/elsewhere\n").unwrap();
    let profile = prepare_for_user(&f, &folder);
    let written = std::fs::read_to_string(profile.coordination.join("orchestration.config")).unwrap();
    let lines = |key: &str| written.lines().filter(|l| l.trim_start().trim_start_matches("export ").starts_with(&format!("{key}="))).map(str::to_string).collect::<Vec<_>>();
    // Written without `export`, so the engine's readers, which match `KEY="` at the start of a
    // line, read the user's value (second review rv-20261021T072931Z-6a246fab-ad3e, finding 2).
    assert_eq!(lines("ALLOWED_MODELS"), ["ALLOWED_MODELS=\"sonnet haiku\""], "{written}");
    assert_eq!(lines("MODEL_TIERS"), [engine_line("MODEL_TIERS")], "{written}");
    assert_eq!(lines("MODEL_CEILING"), ["MODEL_CEILING=\"sonnet\""], "{written}");
    assert_eq!(lines("SESSION_TEAMS_DIR"), [format!("SESSION_TEAMS_DIR='{}'", teams.display())], "{written}");
    assert!(!written.contains("/fixture/elsewhere"), "the app's own key was taken from the user's file");
    assert!(written.contains("# fixture user values"), "the user's file is copied as written");
}

/// The user's `settings.json` adds environment and hooks to every lease; the names the app sets
/// for itself stay the app's. A file that is not JSON adds nothing.
#[test]
fn the_users_settings_json_adds_environment_and_hooks_and_never_the_apps_own_names() {
    let f = Scratch::new();
    let folder = f.0.join("central");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("settings.json"), serde_json::json!({
        "env": {"FIXTURE_TOOL_HOME": "/fixture/tool", "RICHOS_ENTITY_ROOT": "/fixture/hijack",
                "CLAUDE_CONFIG_DIR": "/fixture/other-account", "GIT_DIR": "/fixture/repo", "NODE_OPTIONS": "--require /fixture/x.js"},
        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "/fixture/user-hook.sh"}]}]},
        "model": "fixture-model"
    }).to_string()).unwrap();
    let profile = prepare_for_user(&f, &folder);
    let mut command = std::process::Command::new("/fictional/provider");
    profile.configure(&mut command, Path::new("/fictional/provider"), "session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(environment["FIXTURE_TOOL_HOME"].as_deref(), Some("/fixture/tool"));
    assert_eq!(environment["RICHOS_ENTITY_ROOT"].as_deref(), profile.coordination.to_str());
    assert!(!environment.contains_key("CLAUDE_CONFIG_DIR"), "the account in use is the app's");
    assert_eq!(environment.get("GIT_DIR"), Some(&None));
    assert_eq!(environment.get("NODE_OPTIONS"), Some(&None));
    let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    let settings: serde_json::Value = serde_json::from_str(&args[args.iter().position(|a| a == "--settings").unwrap() + 1]).unwrap();
    assert_eq!(settings["hooks"]["Stop"][0]["hooks"][0]["command"], "/fixture/user-hook.sh");
    assert!(settings["model"].is_null(), "only env and hooks are read from the user's file");
    assert!(settings["claudeMdExcludes"].is_array());

    std::fs::write(folder.join("settings.json"), "{ not json").unwrap();
    let profile = prepare_for_user(&f, &folder);
    assert_eq!(profile.user.settings, Default::default());
}

/// The digest that renews a session moves when any of the user's files is created, changed or
/// removed, and with nothing else in the folder.
#[test]
fn the_user_settings_digest_moves_with_the_users_files_and_nothing_else() {
    use richos_core::user_settings::{digest, FILES};
    let f = Scratch::new();
    let folder = f.0.join("central");
    std::fs::create_dir_all(&folder).unwrap();
    let empty = digest(&folder);
    std::fs::write(folder.join("notes.md"), "not a settings file").unwrap();
    assert_eq!(digest(&folder), empty);
    let mut seen = vec![empty.clone()];
    for name in FILES {
        std::fs::write(folder.join(name), "{}").unwrap();
        let now = digest(&folder);
        assert!(!seen.contains(&now), "{name} did not move the digest");
        seen.push(now);
    }
    for name in FILES { std::fs::remove_file(folder.join(name)).unwrap(); }
    assert_eq!(digest(&folder), empty);
}

/// **A Terminal setup read again that found something different renews a lease** (round 41
/// piece 1, "Your team uses this from its next job"): the lease's digest carries its login
/// environment's fingerprint, so the same environment renews nothing and a changed one does.
#[test]
fn a_lease_is_renewed_when_the_terminal_setup_it_got_has_changed_and_only_then() {
    let f = Scratch::new();
    let folder = f.0.join("central");
    let env = |java: &str| -> std::sync::Arc<BTreeMap<String, String>> {
        std::sync::Arc::new([("PATH", "/usr/bin"), ("FIXTURE_JAVA_HOME", java)].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    };
    let prepare = |login| EngineProfile::prepare_for(&engine(), &f.0.join("data"), f.runtime(),
        richos_core::machine_settings::Settings::default(), Some(folder.clone()), Some(login)).unwrap();
    let (a, b, c) = (prepare(env("/jdk/17")), prepare(env("/jdk/17")), prepare(env("/jdk/21")));
    assert_eq!(a.user.digest, b.user.digest, "the same Terminal setup renews nothing");
    assert_ne!(a.user.digest, c.user.digest, "a changed one renews the lease at its next boundary");
    // The app's side of the comparison is computed the same way.
    assert_eq!(a.user.digest, Some(richos_core::user_settings::with_login(richos_core::user_settings::digest(&folder), &env("/jdk/17"))));
}

// =======================================================================================
// THE USER'S TERMINAL ENVIRONMENT — build plan 2026-10-21, item 4
// =======================================================================================

/// Every lease gets the login shell's environment, with the user's `PATH` after the runtime's
/// and before the system folders; the names the app sets for itself stay the app's.
#[test]
fn a_lease_carries_the_users_login_environment_under_the_apps_own_names() {
    let f = Scratch::new();
    let login: BTreeMap<String, String> = [("FIXTURE_JAVA_HOME", "/fixture/jdk"), ("PATH", "/fixture/bin:/usr/bin:/fixture/bin"),
        ("RICHOS_ENTITY_ROOT", "/fixture/hijack"), ("CLAUDE_CONFIG_DIR", "/fixture/other-account"), ("PYTHONPATH", "/fixture/py"),
        ("ANTHROPIC_API_KEY", "fixture-not-a-key")].into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let profile = EngineProfile::prepare_for(&engine(), &f.0, f.runtime(), richos_core::machine_settings::Settings::default(),
        None, Some(std::sync::Arc::new(login))).unwrap();
    let mut command = std::process::Command::new("/fictional/provider");
    profile.configure(&mut command, Path::new("/fictional/provider"), "session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(environment["FIXTURE_JAVA_HOME"].as_deref(), Some("/fixture/jdk"));
    let path = environment["PATH"].clone().unwrap();
    let want = format!("{}:/fixture/bin:/usr/bin:/bin:/usr/sbin:/sbin", f.0.join("runtime/bin").display());
    assert!(path == want || path.starts_with(&(want.clone() + ":")), "{path}");
    assert_eq!(environment["RICHOS_ENTITY_ROOT"].as_deref(), profile.coordination.to_str());
    assert!(!environment.contains_key("CLAUDE_CONFIG_DIR") && !environment.contains_key("ANTHROPIC_API_KEY"),
        "which account a lease spends is the app's");
    assert_eq!(environment.get("PYTHONPATH"), Some(&None), "the runtime's interpreters stay isolated");

    // With no login environment the lease is today's: the runtime, the system folders.
    let today = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    assert_eq!(today.lease_path(), today.runtime.path());
}

/// The login shell is read as a login, interactive shell, between marks, and a shell that does
/// not answer in time gives up rather than holding the lease.
#[test]
fn the_login_environment_is_the_login_shells_and_a_slow_shell_times_out() {
    use richos_core::login_environment::read;
    let f = Scratch::new();
    // A real zsh with a fixture profile: what the user's terminal would have.
    let zdotdir = f.0.join("zdot");
    std::fs::create_dir_all(&zdotdir).unwrap();
    std::fs::write(zdotdir.join(".zprofile"), "export FIXTURE_LOGIN_ONLY=from-zprofile\nexport PATH=/fixture/login/bin:$PATH\n").unwrap();
    std::fs::write(zdotdir.join(".zshrc"), "echo interactive-noise\nexport FIXTURE_INTERACTIVE=from-zshrc\n").unwrap();
    let found = read(Path::new("/bin/zsh"), &[("ZDOTDIR", zdotdir.to_str().unwrap())], std::time::Duration::from_secs(20)).unwrap();
    assert_eq!(found.get("FIXTURE_LOGIN_ONLY").map(String::as_str), Some("from-zprofile"));
    assert_eq!(found.get("FIXTURE_INTERACTIVE").map(String::as_str), Some("from-zshrc"));
    assert!(found["PATH"].starts_with("/fixture/login/bin:"), "{}", found["PATH"]);
    assert!(!found.contains_key("SHLVL") && !found.contains_key("_"));

    let slow = f.0.join("slow-shell");
    std::fs::write(&slow, "#!/bin/sh\nexec sleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).unwrap();
    let began = std::time::Instant::now();
    assert!(read(&slow, &[], std::time::Duration::from_secs(1)).is_err());
    assert!(began.elapsed() < std::time::Duration::from_secs(10), "the read waited {:?}", began.elapsed());
}

// =======================================================================================
// AUTO-MEMORY — build plan 2026-10-21, item 10
// =======================================================================================

/// Rich and Stu keep auto-memory as the user's terminal does, in one folder in the app's data,
/// and it is the same folder on every Claude account, so a switch keeps what they remembered.
#[test]
fn auto_memory_is_on_in_one_folder_whichever_claude_account_the_lease_runs_on() {
    use richos_core::claude_accounts::Account;
    use richos_core::user_settings::{MEMORY_SCOPE, RICH, STU};
    let f = Scratch::new();
    let folder = f.0.join("central");
    let mut profile = prepare_for_user(&f, &folder);
    let lease = |profile: &EngineProfile| {
        let mut command = std::process::Command::new("/fictional/provider");
        command.env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1");
        profile.configure(&mut command, Path::new("/fictional/provider"), "session", &f.0.join("scope.json"));
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().to_string()).collect();
        let settings: serde_json::Value = serde_json::from_str(&args[args.iter().position(|a| a == "--settings").unwrap() + 1]).unwrap();
        let disabled = command.get_envs().find(|(k, _)| *k == "CLAUDE_CODE_DISABLE_AUTO_MEMORY").map(|(_, v)| v.map(|v| v.to_owned()));
        (settings, disabled)
    };
    for account in [Account { id: "1".into(), label: "Home".into(), folder: None },
                    Account { id: "2".into(), label: "Work".into(), folder: Some(f.0.join("claude-accounts/2")) }] {
        profile.claude_account = Some(account);
        let (settings, disabled) = lease(&profile);
        assert_eq!(settings["autoMemoryEnabled"], true);
        assert_eq!(disabled, Some(None), "an inherited switch-off is removed");
    }
    // Rich and Stu run as plugin agents, whose memory is their definition's scope: `project`,
    // under the lease's working folder (the coordination folder), the same on every account.
    let shipped = engine().join("app");
    for name in [RICH, STU] {
        let loaded = std::fs::read_to_string(profile.standing_plugin(&shipped).join("agents").join(name)).unwrap();
        let front = loaded.split("\n---").next().unwrap();
        assert!(front.lines().any(|l| l == MEMORY_SCOPE), "{name} has no memory scope: {front}");
        assert!(!std::fs::read_to_string(folder.join(name)).unwrap().contains(MEMORY_SCOPE), "the user's own {name} was written");
    }
    // A user who names a scope keeps it.
    let mine = std::fs::read_to_string(folder.join(RICH)).unwrap().replacen("---\n", "---\nmemory: user\n", 1);
    std::fs::write(folder.join(RICH), &mine).unwrap();
    let next = prepare_for_user(&f, &folder);
    let loaded = std::fs::read_to_string(next.standing_plugin(&shipped).join("agents").join(RICH)).unwrap();
    assert!(loaded.starts_with(&mine), "the user's own memory scope was changed");
}

// =======================================================================================
// THE USER'S OWN GIT — build plan 2026-10-21, item 8
// =======================================================================================

/// A lease commits under the user's own name and email from ~/.gitconfig, and its Git asks the
/// helpers the user's settings name: the user's own Git's system file is included (where a Mac's
/// osxkeychain helper is set) and that Git's helper folder is on PATH, last.
#[test]
fn a_lease_commits_as_the_user_and_asks_the_users_credential_helpers() {
    let f = Scratch::new();
    let home = f.0.join("home");
    let bin = f.0.join("user git/bin");
    let helpers = f.0.join("user git/libexec/git-core");
    for dir in [&home, &bin, &helpers] { std::fs::create_dir_all(dir).unwrap(); }
    std::fs::write(home.join(".gitconfig"), "[user]\n\tname = Fixture Person\n\temail = fixture.person@example.invalid\n[credential]\n\thelper = fixture-user-helper\n").unwrap();
    let system = f.0.join("user git/etc/gitconfig");
    std::fs::create_dir_all(system.parent().unwrap()).unwrap();
    std::fs::write(&system, "[credential]\n\thelper = fixture-system-helper\n").unwrap();
    // A vendor file below the system one, as Apple's Git has (scope `unknown`).
    let vendor = f.0.join("user git/share/git-core/gitconfig");
    std::fs::create_dir_all(vendor.parent().unwrap()).unwrap();
    std::fs::write(&vendor, "[credential]\n\thelper = fixture-vendor-helper\n").unwrap();
    // The user's Git, as `git --exec-path` and `git config --list --show-origin --show-scope`
    // answer (its global line is the user's own file, read natively, never included).
    let git = bin.join("git");
    std::fs::write(&git, format!("#!/bin/sh\ncase \"$1\" in\n  --exec-path) echo '{}' ;;\n  config) printf 'unknown\\tfile:%s\\tcredential.helper=fixture-vendor-helper\\nsystem\\tfile:%s\\tcredential.helper=fixture-system-helper\\nglobal\\tfile:%s\\tuser.name=Fixture Person\\n' '{}' '{}' '{}' ;;\nesac\n",
        helpers.display(), vendor.display(), system.display(), home.join(".gitconfig").display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let login: BTreeMap<String, String> = [("PATH", format!("{}:/usr/bin:/bin", bin.display())), ("HOME", home.display().to_string())]
        .into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let profile = EngineProfile::prepare_for(&engine(), &f.0.join("data"), f.runtime(), richos_core::machine_settings::Settings::default(),
        None, Some(std::sync::Arc::new(login))).unwrap();
    assert_eq!(profile.user.git, richos_core::user_git::UserGit { system_configs: vec![vendor.clone(), system.clone()], exec_path: Some(helpers.clone()) });
    let mut configured = std::process::Command::new("/fictional/provider");
    profile.configure(&mut configured, Path::new("/fictional/provider"), "session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = configured.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(environment["HOME"].as_deref(), home.to_str());
    assert!(environment["PATH"].as_deref().unwrap().ends_with(&format!(":{}", helpers.display())), "{:?}", environment["PATH"]);
    let ask = |args: &[&str]| {
        let mut git = std::process::Command::new("/usr/bin/git");
        for (key, value) in configured.get_envs() {
            if let Some(value) = value { git.env(key, value); } else { git.env_remove(key); }
        }
        let output = git.env_remove("XDG_CONFIG_HOME").current_dir(&profile.coordination).args(args).output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    };
    assert_eq!(ask(&["config", "--get", "user.name"]), "Fixture Person");
    assert_eq!(ask(&["config", "--get", "user.email"]), "fixture.person@example.invalid");
    // Ends with: Apple's own `git`, which this test runs, reads one more layer of its own first.
    let helpers = ask(&["config", "--get-all", "credential.helper"]);
    assert!(helpers.ends_with("fixture-vendor-helper\nfixture-system-helper\nfixture-user-helper"),
        "the user's Git's vendor and system helpers, then the user's own, as the user's terminal asks them: {helpers:?}");
}

/// **Every lease loads the engine's shipped skills, except the terminal's two** (CEO §138: everything
/// he uses in the terminal is a feature of the regular app; richos-hq
/// `docs/plans/2026-10-21-switching-to-the-app.md` §4.6 and row S). Until this, a lease loaded only
/// the standing plugin's four, and 19 of his 29 teammates name skills that never reached them, eight
/// of them `playwright-cli`. Read off the engine folder, never a list typed here, so a skill added to
/// the engine reaches every lease without a change in this crate.
#[test]
fn every_lease_loads_the_engine_s_shipped_skills_except_the_terminal_only_two() {
    let f = Scratch::new(); let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut shipped: Vec<String> = std::fs::read_dir(engine().join("skills")).unwrap()
        .map(|entry| entry.unwrap().path()).filter(|dir| dir.join("SKILL.md").is_file())
        .map(|dir| dir.file_name().unwrap().to_string_lossy().into_owned()).collect();
    shipped.sort();
    assert!(shipped.iter().any(|name| name == "playwright-cli"), "the engine ships no playwright-cli: {shipped:?}");
    let mut loaded = Vec::new();
    for name in &shipped {
        let linked = profile.plugin.join("skills").join(name);
        if ["rich-lander", "using-git-worktrees"].contains(&name.as_str()) {
            // The terminal lead's landing and worktree procedures: the app lands and makes
            // workspaces itself (Mega Lander), so a worker told to follow them would go around it.
            assert!(!linked.exists(), "{name} is a terminal-only skill and reached a lease");
        } else if richos_core::skills::SKILLS.contains(&name.as_str()) {
            // The standing plugin carries the app's own words for these; the engine's copies name
            // the terminal's install path, so a second copy would be the wrong one.
            assert!(!linked.exists(), "{name} is in the lease twice: the standing plugin's and the engine's");
        } else {
            assert_eq!(std::fs::canonicalize(linked.join("SKILL.md")).unwrap(),
                       std::fs::canonicalize(engine().join("skills").join(name).join("SKILL.md")).unwrap(),
                       "{name} is not the engine's own skill in this lease");
            loaded.push(name.clone());
        }
    }
    assert!(loaded.contains(&"playwright-cli".to_string()), "playwright-cli did not reach the lease: {loaded:?}");
    // Nothing else is in the lease's skills folder.
    let mut present: Vec<String> = std::fs::read_dir(profile.plugin.join("skills")).unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    present.sort();
    assert_eq!(present, loaded);
    // The plugin the child is given is this one (`configure`'s `--plugin-dir`).
    let mut command = std::process::Command::new("/fictional/provider");
    profile.configure(&mut command, Path::new("/fictional/provider"), "session", &f.0.join("scope.json"));
    let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    assert!(args.windows(2).any(|pair| pair[0] == "--plugin-dir" && Path::new(&pair[1]) == profile.plugin));
    // A retired lease's folders go, and the engine's skills stay exactly where they were.
    let before = std::fs::read(engine().join("skills/playwright-cli/SKILL.md")).unwrap();
    profile.remove_session_files();
    assert!(!profile.plugin.exists());
    assert_eq!(std::fs::read(engine().join("skills/playwright-cli/SKILL.md")).unwrap(), before);
}

/// **With Apple's real Git** (second review rv-20261021T092945Z-1f1d7133-1f6d, finding 1): the
/// files a lease includes are every file Apple's Git itself reads its credential helper from
/// for a user with no Git settings of their own, so its `osxkeychain` default is the lease's.
/// Skipped, and says so, on a Mac without Apple's developer tools.
#[test]
fn with_apples_git_the_lease_includes_the_file_its_osxkeychain_default_comes_from() {
    let f = Scratch::new();
    let home = f.0.join("home with no git settings");
    std::fs::create_dir_all(&home).unwrap();
    let mut apple = std::process::Command::new("/usr/bin/git");
    apple.args(["config", "--show-origin", "--get-all", "credential.helper"]).env_clear()
        .env("HOME", &home).env("PATH", "/usr/bin:/bin").current_dir("/");
    let developer = ["/Library/Developer/CommandLineTools".into()].into_iter()
        .chain(std::fs::read_link("/var/db/xcode_select_link").ok()).any(|d: PathBuf| d.join("usr/bin/git").is_file());
    let Some(answer) = developer.then(|| apple.output().ok()).flatten().filter(|o| o.status.success()) else {
        eprintln!("skipped: no Apple developer tools on this Mac, so no Apple Git to ask");
        return;
    };
    let origins: Vec<PathBuf> = String::from_utf8(answer.stdout).unwrap().lines()
        .filter_map(|line| line.strip_prefix("file:")?.split_once('\t').map(|(file, _)| PathBuf::from(file))).collect();
    assert!(!origins.is_empty(), "Apple's Git names no credential helper of its own here");
    let found = richos_core::user_git::discover("/usr/bin:/bin", Some(&home), &f.0.join("runtime"));
    for origin in &origins {
        assert!(found.system_configs.contains(origin), "{origin:?} is where Apple's Git reads its helper, and the lease does not include it: {found:?}");
    }
    let layer = richos_core::user_git::system_layer(&found, None);
    assert!(origins.iter().all(|o| layer.contains(&format!("path = \"{}\"", o.display()))), "{layer}");
}

/// **Gap E: the engine's checks at the write run in every lease, and the terminal's claim never
/// does.** Rich, Stu and every worker and reviewer they start run this one hooks file, so a
/// credential, private material in a public repository, a listed person's name and a
/// non-American word are stopped at the write as the terminal stops them. The registered
/// commands are run here, not only read, on the payload shape Claude Code sends.
#[test]
fn every_lease_runs_the_engines_checks_at_the_write_and_never_the_terminal_claim() {
    // The checks at the write the terminal runs and every user needs, named here on their own
    // so this states what the lease must carry rather than echoing what it does carry.
    const APP_BASH_RULES: [&str; 7] = ["guard-publication-commits.sh", "guard-named-persons-commands.sh",
        "guard-public-record-repo.sh", "guard-unguarded-rm.sh", "guard-host-sleep.sh", "guard-headed-browser.sh",
        "guard-toolchain.sh"];
    const APP_WRITE_RULES: [&str; 3] = ["scan-secrets.sh", "guard-publication-writes.sh", "guard-named-persons-writes.sh"];
    let f = Scratch::new();
    let folder = f.0.join("central");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("orchestration.config"), "DIALECT_TARGET=\"en-US\"\n").unwrap();
    let profile = prepare_for_user(&f, &folder);
    let text = std::fs::read_to_string(profile.plugin.join("hooks/hooks.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    let pre = manifest["hooks"]["PreToolUse"].as_array().unwrap();

    // Nothing that claims or checks the terminal operator's seat is registered, anywhere.
    for claim in ["operator-claim.sh", "guard-operator-claim.sh"] {
        assert!(!text.contains(claim), "{claim} is registered in a user's lease: {text}");
    }
    // Every rule the app names is one the engine's manifest lists on that chain, so a renamed
    // or retired rule fails here rather than going quiet in the app.
    let shipped = std::fs::read_to_string(engine().join("scripts/hooks/dispatch-pretooluse.manifest")).unwrap();
    for (chain, rules) in [("Bash", &APP_BASH_RULES[..]), ("Write", &APP_WRITE_RULES[..])] {
        for rule in rules {
            assert!(shipped.lines().any(|line| line == format!("{chain}|{rule}")), "{chain}|{rule} is not in the manifest");
            assert!(!rule.contains("operator"), "{rule}");
        }
    }
    let bash = pre.iter().find(|g| g["matcher"] == "Bash").unwrap()["hooks"].as_array().unwrap();
    assert!(bash.iter().any(|h| h["command"].as_str().unwrap()
        .ends_with(&format!("dispatch-pretooluse.sh' Bash {}", APP_BASH_RULES.join(" ")))), "{bash:?}");
    let write = pre.iter().find(|g| g["matcher"] == "Write|Edit|MultiEdit|NotebookEdit")
        .expect("no Write|Edit|MultiEdit|NotebookEdit chain in the lease")["hooks"].as_array().unwrap();
    let commands: Vec<&str> = write.iter().map(|h| h["command"].as_str().unwrap()).collect();
    assert!(commands[0].ends_with(&format!("dispatch-pretooluse.sh' Write {}", APP_WRITE_RULES.join(" "))), "{commands:?}");
    assert!(commands[1].ends_with("dialect-guard.sh'"), "{commands:?}");

    // Run them as the lease's hooks run: the lease's own environment, and a repository the
    // worker writes in that has adopted nothing.
    let repo = f.0.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    assert!(std::process::Command::new("/usr/bin/git").args(["init", "-q", "-b", "main"]).current_dir(&repo).status().unwrap().success());
    let run = |command: &str, content: &str| {
        use std::io::Write;
        let payload = serde_json::json!({"session_id": "e", "cwd": repo, "hook_event_name": "PreToolUse",
            "tool_name": "Write", "tool_input": {"file_path": repo.join("notes.md"), "content": content}});
        let mut child = std::process::Command::new("/bin/sh").arg("-c").arg(command)
            .env("RICHOS_ENGINE_ROOT", &profile.engine).env("RICHOS_ENTITY_ROOT", &profile.coordination)
            .env("CLAUDE_PROJECT_DIR", &profile.coordination).env("TMPDIR", &f.0)
            .env("RICHOS_NAMED_PERSONS_FILE", f.0.join("no-list"))
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
            .spawn().unwrap();
        child.stdin.take().unwrap().write_all(payload.to_string().as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
    };
    // A key-shaped secret, assembled here so this file is not itself one.
    let key = format!("AKIA{}", "QWERTYUIOPASDFGH");
    let (code, _, err) = run(commands[0], &format!("aws_access_key_id = {key}\n"));
    assert_eq!(code, Some(2), "a credential was written: {err}");
    assert!(err.contains("scan-secrets.sh"), "{err}");
    let (code, out, err) = run(commands[0], "Plain notes.\n");
    assert_eq!(code, Some(0), "a plain note was refused: {out} {err}");
    // The user's DIALECT_TARGET reaches the write: the word is fixed in the write itself.
    let (code, out, err) = run(commands[1], &format!("The col{}r of the sky.\n", "ou"));
    assert_eq!(code, Some(0), "{err}");
    let fixed: serde_json::Value = serde_json::from_str(out.trim()).unwrap_or_else(|_| panic!("no fix: {out} {err}"));
    assert_eq!(fixed["hookSpecificOutput"]["updatedInput"]["content"], format!("The col{}r of the sky.\n", "o"));
}
