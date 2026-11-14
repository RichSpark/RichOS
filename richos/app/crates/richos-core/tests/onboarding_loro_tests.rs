//! What onboarding saves reaches the user's loro by itself (CEO 2026-11-14), against the real
//! loro writer and compiler this checkout delivers (`engine/loro`), on a fictional corpus.

use richos_core::correction::CliLoroWriter;
use richos_core::entity::{Entity, EntityId, EntityRegistry};
use richos_core::home_field;
use richos_core::loro::{CorpusPaths, LoroInstall};
use richos_core::onboarding_loro;
use richos_core::onboarding_tools::{self, OnboardingToolScope};
use serde_json::json;
use std::path::{Path, PathBuf};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn engine_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine")
}

fn install(root: &Path) -> LoroInstall {
    let path_var = std::env::var("PATH").ok();
    let paths = CorpusPaths {
        home: Some(root.join("home")),
        env_corpus: Some(root.join("corpus").display().to_string()),
        engine_dir: Some(engine_source()),
        env_node: Some(richos_core::loro::resolve_node_bin(&CorpusPaths { path_var: path_var.clone(), ..Default::default() })),
        path_var,
        ..Default::default()
    };
    LoroInstall::locate(&paths).unwrap().0.unwrap()
}

/// The user's loro memory count: loro's own census of the corpus, current records only.
fn current_records(install: &LoroInstall, registry: &EntityRegistry) -> Vec<String> {
    let census = home_field::probe_census(install.tools(), install.root()).unwrap();
    let topics = home_field::field_topics(&census, registry);
    let mut refs: Vec<String> = home_field::collect_items(install.tools(), install.root(), &topics)
        .unwrap()
        .into_iter()
        .map(|i| i.r#ref)
        .collect();
    refs.sort();
    refs
}

fn census_total(install: &LoroInstall) -> u64 {
    home_field::probe_census(install.tools(), install.root()).unwrap().record_count()
}

#[test]
fn what_onboarding_saves_is_added_to_the_users_loro_and_a_change_supersedes_it() {
    let fixture = Fixture(std::env::temp_dir().join(format!("richos onboarding loro {}", uuid::Uuid::new_v4())));
    std::fs::create_dir_all(fixture.0.join("corpus/ceo/records")).unwrap();
    let central = fixture.0.join("myrichos");
    let data = fixture.0.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let install = install(&fixture.0);
    let writer = CliLoroWriter::from_install(&install);
    let corpus = install.root().path().display().to_string();
    let kept = data.join(onboarding_loro::KEPT_FILENAME);

    // A company added after the memory folder was made: it has no loro partition yet.
    let mut registry = EntityRegistry::default();
    registry.register(Entity::try_new("harborline", "Harborline Ferries", Vec::new()).unwrap()).unwrap();
    let entity = EntityId::parse("harborline").unwrap();

    // Before onboarding: nothing to copy, the loro is empty, and it stays the default.
    assert!(current_records(&install, &registry).is_empty());
    let none = onboarding_loro::sync(&onboarding_loro::sources(&central, &registry), &writer, &corpus, &kept);
    assert_eq!(none, onboarding_loro::SyncReport::default());
    assert_eq!(census_total(&install), 0);

    // Onboarding, through the tools Rich calls in his visible turn.
    let scope_path = data.join("onboarding-scope.json");
    let mut scope = OnboardingToolScope::new(&entity, &central, &data.join("onboarding.json"));
    scope.actions_allowed = true;
    onboarding_tools::write_scope(&scope_path, &scope).unwrap();
    onboarding_tools::call(&scope_path, "save_person", json!({"details": "Dana Reyes, founder. Call her Dana."})).unwrap();
    onboarding_tools::call(&scope_path, "save_company_notes", json!({
        "notes": "Harborline Ferries runs three fictional island routes. Dana wants weekly bookings off her plate.",
        "progress": "partial"})).unwrap();
    // And the team's research, saved beside the company notes.
    std::fs::write(
        richos_core::company::company_home(&central, &entity).join("onboarding-research.md"),
        "Harborline Ferries was founded in 2019 and employs about forty crew (fictional).\n",
    )
    .unwrap();

    let first = onboarding_loro::sync(&onboarding_loro::sources(&central, &registry), &writer, &corpus, &kept);
    assert!(first.problems.is_empty(), "{:?}", first.problems);
    assert_eq!(first.written.len(), 3, "{:?}", first.written);
    let after = current_records(&install, &registry);
    assert_eq!(after.len(), 3, "the person, the company notes and the research: {after:?}");
    assert!(after.iter().any(|r| r.starts_with("rec:ceo/records/person-")));
    assert!(after.iter().any(|r| r.starts_with("rec:companies/harborline/records/harborline-company-")));
    assert!(after.iter().any(|r| r.starts_with("rec:companies/harborline/records/harborline-onboarding-research-")));

    // Nothing changed: nothing written.
    let quiet = onboarding_loro::sync(&onboarding_loro::sources(&central, &registry), &writer, &corpus, &kept);
    assert_eq!(quiet, onboarding_loro::SyncReport::default());

    // The interview goes on: the new notes supersede the old, one current record per file.
    onboarding_tools::call(&scope_path, "save_company_notes", json!({
        "notes": "Harborline Ferries runs three fictional island routes. Dana wants weekly bookings and crew rotas off her plate.",
        "progress": "complete"})).unwrap();
    let changed = onboarding_loro::sync(&onboarding_loro::sources(&central, &registry), &writer, &corpus, &kept);
    assert!(changed.problems.is_empty(), "{:?}", changed.problems);
    assert_eq!(changed.written.len(), 1);
    let now = current_records(&install, &registry);
    assert_eq!(now.len(), 3, "{now:?}");
    assert_eq!(census_total(&install), 4, "the old notes are kept behind the new, never deleted");
    let notes = now.iter().find(|r| r.contains("harborline-company-")).unwrap();
    assert!(!after.contains(notes), "the current company notes are the new version");

    // The kept file lost: the same words are recognized, not written twice.
    std::fs::remove_file(&kept).unwrap();
    let relearned = onboarding_loro::sync(&onboarding_loro::sources(&central, &registry), &writer, &corpus, &kept);
    assert!(relearned.problems.is_empty(), "{:?}", relearned.problems);
    assert_eq!(census_total(&install), 4);
}
