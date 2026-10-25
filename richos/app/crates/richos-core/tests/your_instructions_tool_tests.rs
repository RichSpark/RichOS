//! **Rich saves the user's instructions** (round 36 screen 1; build plan item 1b): the
//! interview's last step, `mcp__richos_onboarding__save_your_instructions`, over the same
//! onboarding scope and visible-turn grant as the company notes, and the card it leaves for the
//! conversation. All fixtures are isolated; no account, model or personal HOME is needed.
use richos_core::{
    entity::EntityId,
    onboarding_tools::{self, OnboardingToolScope},
    question_tools::{self, Scope},
    questions::AskScope,
    your_instructions::{self, ITEM_KIND, TOOL},
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    central: PathBuf,
    scope: PathBuf,
    questions: PathBuf,
    state: PathBuf,
}

impl Fixture {
    fn new(grant: bool, question_entity: &str) -> Self {
        let root = std::env::temp_dir().join(format!("richos-your-instructions-tool-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let f = Self {
            central: root.join("central"),
            scope: root.join("scope.json"),
            questions: root.join("assignments.questions.json"),
            state: root.join("engine-state"),
            root,
        };
        let entity = EntityId::parse("alpha").unwrap();
        onboarding_tools::write_scope(&f.scope, &OnboardingToolScope::new(&entity, &f.central, &f.root.join("onboarding.json"))).unwrap();
        onboarding_tools::set_actions_allowed(&f.scope, grant).unwrap();
        question_tools::write_scope(&f.questions, &Scope {
            context: AskScope { root: f.state.clone(), entity_id: question_entity.into(), thread_id: "onboarding".into(),
                turn_id: "turn-7".into(), asker: "front_desk".into(), obligation_id: None, session_id: "s".into(),
                engine: None, entity_root: None, app_run: None },
            actions_allowed: grant, answer_method: "typed".into(), surface: "mac".into(),
        }).unwrap();
        f
    }
    fn save(&self, rules: Value) -> Result<Value, String> {
        richos_core::onboarding_tools::call_in(&self.scope, Some(&self.questions), TOOL, json!({"rules": rules}))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.root) {
            eprintln!("test scratch {} was not removed: {e}", self.root.display());
        }
    }
}

#[test]
fn the_server_lists_the_tool_beside_the_company_notes_tools() {
    let names: Vec<String> = onboarding_tools::tools()["tools"].as_array().unwrap().iter()
        .map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["save_company_notes", "decline_onboarding", "save_your_instructions"]);
    assert_eq!(your_instructions::QUALIFIED_TOOL, format!("mcp__{}__{}", onboarding_tools::SERVER_NAME, TOOL));
}

#[test]
fn a_save_in_a_visible_turn_adds_the_rules_to_the_users_file_and_leaves_a_card_for_that_turn() {
    let f = Fixture::new(true, "alpha");
    fs::create_dir_all(&f.central).unwrap();
    fs::write(f.central.join("instructions.md"), "Write in plain English.\n").unwrap();
    let answer = f.save(json!(["Ask me before spending more than $500 on anything.", "- Mornings are for deep work."])).unwrap();
    assert_eq!(answer["status"], "saved");
    assert_eq!(answer["appliesFrom"], "the user's next new conversation");
    assert_eq!(
        fs::read_to_string(f.central.join("instructions.md")).unwrap(),
        "Write in plain English.\n- Ask me before spending more than $500 on anything.\n- Mornings are for deep work.\n",
        "the rules go after everything already in the file"
    );
    let cards = your_instructions::items(&f.state, "alpha", "onboarding");
    assert_eq!(cards.len(), 1, "one card for the save");
    assert_eq!(cards[0]["kind"], ITEM_KIND);
    assert_eq!(cards[0]["turnId"], "turn-7");
    assert_eq!(cards[0]["rules"], json!(["Ask me before spending more than $500 on anything.", "Mornings are for deep work."]));
}

#[test]
fn outside_a_visible_turn_nothing_is_saved() {
    let f = Fixture::new(false, "alpha");
    assert!(f.save(json!(["Ask me first."])).is_err());
    assert!(!f.central.join("instructions.md").exists(), "a closed grant wrote the file");
    assert!(your_instructions::all(&f.state).is_empty(), "a closed grant left a card");
}

#[test]
fn arguments_beyond_the_rules_are_refused_and_write_nothing() {
    let f = Fixture::new(true, "alpha");
    let refused = onboarding_tools::call_in(&f.scope, Some(&f.questions), TOOL, json!({"rules": ["Ask me first."], "path": "/tmp/x"}));
    assert!(refused.is_err());
    assert!(!f.central.join("instructions.md").exists());
}

#[test]
fn a_question_scope_of_another_company_gets_no_card_but_the_rules_are_saved() {
    let f = Fixture::new(true, "beta");
    f.save(json!(["Ask me first."])).unwrap();
    assert_eq!(fs::read_to_string(f.central.join("instructions.md")).unwrap(), "- Ask me first.\n");
    assert!(your_instructions::all(&f.state).is_empty(), "a card was drawn in another company's conversation");
}

#[test]
fn over_the_stdio_server_given_the_question_scope() {
    let f = Fixture::new(true, "alpha");
    let input = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":TOOL,"arguments":{"rules":["Copy me on every invoice."]}}}),
    ].iter().map(|v| v.to_string() + "\n").collect::<String>();
    let mut out = Vec::new();
    onboarding_tools::serve_in(&f.scope, Some(&f.questions), std::io::Cursor::new(input), &mut out).unwrap();
    let replies: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies[1]["result"]["isError"], false, "{}", replies[1]);
    assert_eq!(your_instructions::items(&f.state, "alpha", "onboarding").len(), 1);
}
