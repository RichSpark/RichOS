#![cfg(unix)]
//! A changed definition in a governed repository reaches the app's team (CEO 2026-11-02:
//! "where and why only there?"; `team_sync.rs`). Through `EngineProfile::prepare`, as a lease
//! start runs it.
use richos_core::engine_profile::EngineProfile;
use richos_core::runtime::EngineRuntime;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("richos team sync {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(std::fs::canonicalize(root).unwrap())
    }
    fn runtime(&self) -> EngineRuntime {
        EngineRuntime { root: self.0.join("runtime"), git: PathBuf::from("/usr/bin/git"),
            python: self.0.join("runtime/bin/python3"), node: self.0.join("runtime/bin/node"), versions: BTreeMap::new() }
    }
}
impl Drop for Scratch { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).ok(); } }
fn engine() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine") }

fn definition(name: &str, rule: &str) -> String {
    format!("---\nname: {name}\ndescription: {name} fixture\nmodel: sonnet\n---\n{rule}\nescalate: ~/.claude/richos-engine/scripts/escalate.sh raise\n")
}
fn app_form(name: &str, rule: &str) -> String {
    format!("---\nname: {name}\ndescription: {name} fixture\nmodel: sonnet\n---\n{rule}\nescalate: \"$RICHOS_ENGINE_ROOT/scripts/escalate.sh\" raise\n")
}
fn prepare(data: &Path, f: &Scratch) -> EngineProfile {
    EngineProfile::prepare_with_settings(&engine(), data, f.runtime(), richos_core::machine_settings::Settings::default()).unwrap()
}

#[test]
fn a_changed_definition_in_a_governed_repository_reaches_the_app_team() {
    let f = Scratch::new();
    let repo = f.0.join("governed");
    let agents = repo.join(".claude/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(repo.join("orchestration.config"), "X=1\n").unwrap();
    // A repository that is not governed (no orchestration.config) is never read.
    let plain = f.0.join("plain");
    std::fs::create_dir_all(plain.join(".claude/agents")).unwrap();
    std::fs::write(plain.join(".claude/agents/stranger.md"), definition("stranger", "x")).unwrap();
    let data = f.0.join("data");
    std::fs::create_dir_all(data.join("team")).unwrap();
    std::fs::write(data.join("entities.json"), serde_json::json!({"version": 2, "entities": [
        {"id": "gov", "display_name": "Gov", "status": "active", "roots": [repo]},
        {"id": "plain", "display_name": "Plain", "status": "active", "roots": [plain]}]}).to_string()).unwrap();
    // The hand copy of 2026-10-28 (stale), and one the user made only in the app.
    std::fs::write(data.join("team/ray.md"), app_form("ray", "WAIT for proceed")).unwrap();
    std::fs::write(data.join("team/mine.md"), definition("mine", "mine")).unwrap();
    std::fs::write(agents.join("ray.md"), definition("ray", "GO at once")).unwrap();
    std::fs::write(agents.join("kai.md"), definition("kai", "GO at once")).unwrap();
    std::fs::write(agents.join("dean.md"), definition("dean", "the repository's own Dean")).unwrap();
    std::fs::write(agents.join("broken.md"), "no front matter\n").unwrap();

    // The two engine-path forms the definitions use, as the plan's §3 rewrote them by hand.
    assert_eq!(richos_core::team_sync::for_the_app("python3 \"$HOME/.claude/richos-engine/scripts/lib/machine_settings.py\" x ~/.claude/richos-engine/scripts/escalate.sh raise"),
        "python3 \"$RICHOS_ENGINE_ROOT/scripts/lib/machine_settings.py\" x \"$RICHOS_ENGINE_ROOT/scripts/escalate.sh\" raise");
    assert_eq!(richos_core::team_sync::KEEP_STOCK, ["dean", "pierce"]);

    let profile = prepare(&data, &f);
    let team = |n: &str| std::fs::read_to_string(data.join(format!("team/{n}.md"))).unwrap();
    assert_eq!(team("ray"), app_form("ray", "GO at once"), "the stale copy takes the repository's text, rewritten for the app");
    assert_eq!(team("kai"), app_form("kai", "GO at once"), "a new definition is added");
    assert_eq!(team("mine"), definition("mine", "mine"), "a teammate made only in the app is untouched");
    assert!(!data.join("team/dean.md").exists() && !data.join("team/stranger.md").exists() && !data.join("team/broken.md").exists());
    assert!(std::fs::read_to_string(profile.plugin.join("agents/ray.md")).unwrap().contains("GO at once"), "the lease registers the new text");
    let kept: Vec<_> = std::fs::read_dir(data.join("team-replaced")).unwrap().flatten().collect();
    assert_eq!(kept.len(), 1, "the unrecorded stale copy is kept aside");
    assert!(std::fs::read_to_string(kept[0].path()).unwrap().contains("WAIT for proceed"));

    // A later change in the repository reaches a copy the sync wrote.
    std::fs::write(agents.join("kai.md"), definition("kai", "GO, second version")).unwrap();
    prepare(&data, &f);
    assert_eq!(team("kai"), app_form("kai", "GO, second version"));

    // A copy edited in the app is the user's and is kept.
    std::fs::write(data.join("team/kai.md"), app_form("kai", "edited in the app")).unwrap();
    std::fs::write(agents.join("kai.md"), definition("kai", "GO, third version")).unwrap();
    prepare(&data, &f);
    assert_eq!(team("kai"), app_form("kai", "edited in the app"));
    assert_eq!(team("ray"), app_form("ray", "GO at once"));
}

/// Review rv-20261102T050131Z-8c5e6fce-f873: Ray is defined in two governed repositories (normal
/// on his install); a lease for company B takes B's own definition, whatever the company order.
#[test]
fn a_lease_for_a_company_takes_that_companys_own_definition_first() {
    use richos_core::{Entity, EntityRegistry, Ledger, Spine};
    let f = Scratch::new();
    let repos: Vec<PathBuf> = ["a", "b"].iter().map(|n| {
        let r = f.0.join(format!("company-{n}"));
        std::fs::create_dir_all(r.join(".claude/agents")).unwrap();
        std::fs::write(r.join("orchestration.config"), "X=1\n").unwrap();
        std::fs::write(r.join(".claude/agents/ray.md"), definition("ray", "OLD_RULE")).unwrap();
        r
    }).collect();
    let data = f.0.join("data");
    std::fs::create_dir_all(data.as_path()).unwrap();
    let entities: Vec<Entity> = ["a", "b"].iter().zip(&repos)
        .map(|(n, r)| Entity::try_new(&format!("company-{n}"), n, vec![r.clone()]).unwrap()).collect();
    let registry = EntityRegistry::new(entities).unwrap();
    registry.save(&data.join("entities.json")).unwrap();
    let mut spine = Spine::new(Ledger::open(f.0.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(registry);
    let b = richos_core::EntityId::parse("company-b").unwrap();
    let thread = spine.create_thread("B work", &b).unwrap();
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let a_id = richos_core::EntityId::parse("company-a").unwrap();
    let a_thread = spine.create_thread("A work", &a_id).unwrap();
    let a_binding = spine.ledger().thread_binding(&a_thread).unwrap();

    prepare(&data, &f).scope_to(&binding).unwrap();
    std::fs::write(repos[1].join(".claude/agents/ray.md"), definition("ray", "NEW_RULE_IN_B")).unwrap();
    let mut lease = prepare(&data, &f);
    lease.scope_to(&binding).unwrap();
    let team = std::fs::read_to_string(data.join("team/ray.md")).unwrap();
    assert!(team.contains("NEW_RULE_IN_B"), "the second repository's change reaches the app: {team}");
    assert!(std::fs::read_to_string(lease.plugin.join("agents/ray.md")).unwrap().contains("NEW_RULE_IN_B"), "and this lease's plugin");

    // A lease for company A takes A's own, which still says the old rule.
    let mut lease_a = prepare(&data, &f);
    lease_a.scope_to(&a_binding).unwrap();
    assert!(std::fs::read_to_string(lease_a.plugin.join("agents/ray.md")).unwrap().contains("OLD_RULE"));
}

/// Review rv-20261102T053802Z-c66f511d: the coordination folder is shared by every lease, so
/// starting a lease for company A after one for company B must not change what B's next dispatch
/// reads. B, then A, then B dispatches Ray: the dispatch reads B's own roster and runs B's model.
#[test]
fn a_later_lease_for_another_company_does_not_change_the_first_leases_dispatch() {
    use richos_core::{Entity, EntityRegistry, Ledger, Spine};
    let f = Scratch::new();
    let with_model = |name: &str, model: &str, company: &str| definition(name, &format!("rule of company {company}"))
        .replace("model: sonnet", &format!("model: {model}"));
    let repos: Vec<PathBuf> = [("a", "sonnet"), ("b", "opus")].iter().map(|(n, model)| {
        let r = f.0.join(format!("company-{n}"));
        std::fs::create_dir_all(r.join(".claude/agents")).unwrap();
        std::fs::write(r.join("orchestration.config"), "X=1\n").unwrap();
        std::fs::write(r.join(".claude/agents/ray.md"), with_model("ray", model, n)).unwrap();
        r
    }).collect();
    let data = f.0.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let entities: Vec<Entity> = ["a", "b"].iter().zip(&repos)
        .map(|(n, r)| Entity::try_new(&format!("company-{n}"), n, vec![r.clone()]).unwrap()).collect();
    let registry = EntityRegistry::new(entities).unwrap();
    registry.save(&data.join("entities.json")).unwrap();
    let mut spine = Spine::new(Ledger::open(f.0.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(registry);
    let mut binding = |id: &str| {
        let thread = spine.create_thread(id, &richos_core::EntityId::parse(id).unwrap()).unwrap();
        spine.ledger().thread_binding(&thread).unwrap()
    };
    let (b, a) = (binding("company-b"), binding("company-a"));

    let mut lease_b = prepare(&data, &f);
    lease_b.scope_to(&b).unwrap();
    let mut lease_a = prepare(&data, &f);
    lease_a.scope_to(&a).unwrap();

    // B's next dispatch, with the environment B's session and its hooks run in.
    let mut command = std::process::Command::new("true");
    lease_b.configure(&mut command, Path::new("/fictional/provider"), "b-session", &f.0.join("scope.json"));
    let env: BTreeMap<String, std::ffi::OsString> = command.get_envs()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v?.to_os_string()))).collect();
    let script = "import importlib.util,sys\n\
        s=importlib.util.spec_from_file_location('app',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)\n\
        print(m.job_model('ray',None))";
    let mut dispatch = std::process::Command::new("python3");
    dispatch.args(["-c", script]).arg(engine().join("mega-lander/app.py"));
    for key in ["RICHOS_ENTITY_ROOT", "RICHOS_AGENT_ROSTER"] {
        if let Some(value) = env.get(key) { dispatch.env(key, value); }
    }
    let out = dispatch.output().unwrap();
    assert!(out.status.success(), "dispatch failed: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "opus", "B's Ray dispatches on B's model, not A's");
    assert!(std::fs::read_to_string(lease_a.plugin.join("agents/ray.md")).unwrap().contains("model: sonnet"));

    // Every other reader of the roster, run with the environment B's next dispatch has
    // (review rv-20261102T062434Z-a3c7a28e: Pierce still read the shared folder).
    let run = |program: &str, args: &[&str]| -> String {
        let mut c = std::process::Command::new(program);
        c.args(args);
        for key in ["RICHOS_ENTITY_ROOT", "RICHOS_AGENT_ROSTER", "CLAUDE_PROJECT_DIR"] {
            if let Some(value) = env.get(key) { c.env(key, value); }
        }
        c.env("RICHOS_APP_STATE", f.0.join("app-state"));
        let out = c.output().unwrap();
        assert!(out.status.success(), "{program} {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let path = |relative: &str| engine().join(relative).to_string_lossy().to_string();
    let entity = env["RICHOS_ENTITY_ROOT"].to_string_lossy().to_string();

    // guard-pierce: the instructions the assignment is inspected against.
    let pierce = run("python3", &["-c", "import sys\nsys.path.insert(0,sys.argv[1])\nimport pierce\n\
        print(pierce.instruction_text({'tool_input':{'subagent_type':'richos-app-engine:ray','prompt':'x'},'cwd':sys.argv[2]}))",
        &path("scripts/lib"), &entity]);
    assert!(pierce.contains("rule of company b") && !pierce.contains("rule of company a"), "Pierce reads B's Ray: {pierce}");

    // The shell guards' resolver and the folder they name in a hint or fall back to (drift guard).
    let roster = env["RICHOS_AGENT_ROSTER"].to_string_lossy().to_string();
    let script = format!(". '{}'\nresolve_agent_def '{entity}' '{}' richos-app-engine:ray\nagent_roster_dir '{entity}'",
        path("scripts/lib/resolve-roots.sh"), engine().to_string_lossy());
    let shell = run("bash", &["-c", &script]);
    assert_eq!(shell.lines().collect::<Vec<_>>(), [format!("{roster}/ray.md"), roster.clone()], "shell readers take B's roster");
    assert!(std::fs::read_to_string(format!("{roster}/ray.md")).unwrap().contains("rule of company b"));

    // The Stop hooks' list of the roles this installation has: a lease's roster folder holding a
    // teammate the shared folder does not have, read through the lease's variable.
    let own = f.0.join("own-roster");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(own.join("onlyroster.md"), definition("onlyroster", "x")).unwrap();
    for (file, name) in [("ass-kicker/ass-kicker.py", "ass_kicker"), ("scripts/hooks/guard-unresolved-claims.py", "unresolved_claims")] {
        let mut c = std::process::Command::new("python3");
        c.args(["-c", "import importlib.util,sys\n\
            s=importlib.util.spec_from_file_location(sys.argv[2],sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)\n\
            print(' '.join(sorted(m.roster_roles(sys.argv[3],None,None))))", &path(file), name, &entity]);
        c.env("RICHOS_AGENT_ROSTER", &own);
        let out = c.output().unwrap();
        assert!(out.status.success(), "{file}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "onlyroster", "{file} lists the lease's own roster");
    }
}
