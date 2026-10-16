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
        assert_eq!(groups.as_array().unwrap().len(), if event == "PreToolUse" { 2 } else { 1 });
        assert_eq!(groups[0]["hooks"].as_array().unwrap().len(), 1);
    }
    let bash = &hooks["PreToolUse"][1];
    assert_eq!(bash["matcher"], "Bash");
    let registered = bash["hooks"].as_array().unwrap();
    for (hook, name) in registered.iter().zip([
        "guard-interactive-prompt.sh", "guard-no-shell-c.sh", "shell-evidence.sh"]) {
        assert!(hook["command"].as_str().unwrap().contains(engine().join("scripts/hooks").join(name).to_str().unwrap()));
    }
    assert_eq!(registered.len(), 3);
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
    assert_eq!(manifest["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
    assert_eq!(manifest["hooks"]["PreToolUse"][1], before["hooks"]["PreToolUse"][1]);
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
fn child_configuration_is_explicit_and_disables_auto_memory_only_for_that_child() {
    let f = Scratch::new(); let profile = prepare_fixture(&engine(), &f.0, f.runtime()).unwrap();
    let mut command = std::process::Command::new("/fictional/provider");
    command.env("RICHOS_PROJECTS_DIR", "/fictional/terminal-context").env("LORO_CORPUS", "/fictional/private-corpus").env("NODE_OPTIONS", "--require /fictional/private.js")
        .env("RICHOS_APP_READ_ROOTS", "[\"/fictional/other-company\"]");
    profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
    let environment: BTreeMap<_, _> = command.get_envs().map(|(k,v)| (k.to_string_lossy().to_string(),v.map(|v|v.to_string_lossy().to_string()))).collect();
    assert_eq!(environment["CLAUDE_CODE_DISABLE_AUTO_MEMORY"].as_deref(), Some("1"));
    assert_eq!(environment["RICHOS_ENTITY_ROOT"].as_deref(), profile.coordination.to_str());
    assert!(!environment["PATH"].as_ref().unwrap().contains("homebrew"));
    assert_eq!(environment["RICHOS_CLAUDE_BIN"].as_deref(), Some("/fictional/provider"));
    assert_eq!(environment["GIT_CONFIG_GLOBAL"].as_deref(), profile.plugin.join("gitconfig").to_str());
    assert_eq!(environment["GIT_AUTHOR_NAME"], None);
    assert_eq!(environment["LORO_CORPUS"], None);
    assert_eq!(environment["NODE_OPTIONS"], None);
    assert_eq!(environment["RICHOS_PROJECTS_DIR"].as_deref(), profile.state.join("platform-projects").to_str());
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
        command.env("RICHOS_CLAUDE_ACCOUNT", "9");
        profile.configure(&mut command, Path::new("/fictional/provider"), "fictional-session", &f.0.join("scope.json"));
        command.get_envs().map(|(k, v)| (k.to_string_lossy().to_string(), v.map(|v| v.to_string_lossy().to_string())))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(environment(&profile)["RICHOS_CLAUDE_ACCOUNT"], None, "an inherited account is removed");
    profile.claude_account = Some(Account { id: "1".into(), label: "Home".into(), folder: None });
    let one = environment(&profile);
    assert_eq!(one["RICHOS_CLAUDE_ACCOUNT"].as_deref(), Some("1"));
    assert!(!one.contains_key("CLAUDE_CONFIG_DIR"), "Account 1 keeps the app's own folder");
    let folder = f.0.join("claude-accounts/2");
    profile.claude_account = Some(Account { id: "2".into(), label: "Work".into(), folder: Some(folder.clone()) });
    let two = environment(&profile);
    assert_eq!(two["RICHOS_CLAUDE_ACCOUNT"].as_deref(), Some("2"));
    assert_eq!(two["CLAUDE_CONFIG_DIR"].as_deref(), folder.to_str());
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
    for (extra, expected) in [(vec![],"RichOS"), (vec!["-c","user.name=Fixture"],"Fixture")] {
        let mut git=std::process::Command::new("/usr/bin/git");
        for (key,value) in configured.get_envs() {
            if let Some(value)=value { git.env(key,value); } else { git.env_remove(key); }
        }
        let output=git.current_dir(&profile.coordination).args(extra).args(["config","--get","user.name"]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
    }
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
        let shipped = std::fs::read(engine().join(format!("agents/{stock}.md"))).unwrap();
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
    assert_eq!(std::fs::read(roster.join("dean.md")).unwrap(), std::fs::read(engine().join("agents/dean.md")).unwrap());

    // A fresh install has no team folder at all and gets the stock five.
    let fresh = Scratch::new();
    let profile = prepare_fixture(&engine(), &fresh.0, fresh.runtime()).unwrap();
    assert_eq!(registered(&profile), stock);
}

/// Native definitions retain the user's chosen tools and instructions byte for byte.
#[test]
fn saved_teammate_instructions_and_tool_lists_are_registered_unchanged() {
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
            assert_eq!(std::fs::read(registered).unwrap(), original);
        }
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
