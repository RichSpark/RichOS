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
