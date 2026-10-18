//! Durable onboarding decisions survive restart without a hidden priming model call.
use richos_core::{company::{company_file,company_home},entity::EntityId,ledger::Ledger,onboarding::{record_path,OnboardingRecord,OnboardingState}};
use std::path::PathBuf;
mod support;
static SEQ: std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
fn company() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tag(name: &str) -> String {
    format!(
        "{name}-{}-{}-{}",
        std::process::id(),
        richos_core::util::now_millis(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn tmp_ledger(name: &str) -> (PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!("richos-declination-{}.jsonl", tag(name)));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("richos-declination-{}", tag(name)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_declination_with_nowhere_to_go_is_refused() {
    let (path, ledger) = tmp_ledger("nowhere");
    let mut spine = support::spine(ledger);
    // `set_onboarding_record` deliberately NOT called — the shipped default for every
    // headless example and every other test in this crate.
    let err = spine
        .record_onboarding_declination(1_788_634_800_000)
        .expect_err("a declination with nowhere to go must be refused, not silently dropped");
    let said = err.to_string();
    assert!(
        said.contains("nowhere to write") || said.contains("nobody told this spine where"),
        "and the refusal must say what did not happen: {said}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_spines_write_produces_a_record_holding_only_a_declination() {
    let config = tmp_dir("onefact-config");
    let (path, ledger) = tmp_ledger("onefact");
    let mut spine = support::spine(ledger);
    spine.set_onboarding_record(record_path(&config));
    spine.ensure_active_thread_in(&company()).unwrap();
    spine.record_onboarding_declination(1_788_634_800_000).unwrap();

    let body = std::fs::read_to_string(record_path(&config)).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["declined_by_entity"][company().as_str()], 1_788_634_800_000_u64);
    assert_eq!(parsed["declined_by_entity"].as_object().unwrap().len(), 1);
    assert_eq!(
        OnboardingRecord::load(&record_path(&config)).for_entity(&company()).declined_at_millis(),
        Some(1_788_634_800_000),
        "and it reads back as what was written"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_notice_uses_the_durable_declination_after_restart_and_company_material_wins() {
    let (path,ledger)=tmp_ledger("durable-notice");
    let central=tmp_dir("durable-central");let config=tmp_dir("durable-config");
    let mut spine=support::spine(ledger);
    let binding=spine.ensure_active_thread_in(&company()).unwrap();
    assert_eq!(spine.onboarding_state(&binding),OnboardingState::NoCentralFolder);
    spine.set_central_root(central.clone());spine.set_onboarding_record(record_path(&config));
    assert_eq!(spine.onboarding_state(&binding),OnboardingState::NotYet);
    spine.record_onboarding_declination(1_788_634_800_000).unwrap();drop(spine);
    let mut reopened=support::spine(Ledger::open(&path).unwrap());
    reopened.set_central_root(central.clone());reopened.set_onboarding_record(record_path(&config));
    assert_eq!(reopened.onboarding_state(&binding),OnboardingState::Declined{at_millis:1_788_634_800_000});
    std::fs::create_dir_all(company_home(&central,&company())).unwrap();
    std::fs::write(company_file(&central,&company()),"# Example company\nCompany facts").unwrap();
    assert_eq!(reopened.onboarding_state(&binding),OnboardingState::Described);
    drop(reopened);std::fs::remove_file(path).unwrap();std::fs::remove_dir_all(central).unwrap();std::fs::remove_dir_all(config).unwrap();
}
