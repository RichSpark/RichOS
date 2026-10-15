//! ONBOARDING — the trigger for the bootstrap interview, and the refusal that keeps it honest.
//!
//! # The finding this closes, and the one it corrects
//!
//! `docs/plans/richos-onboarding-2026-09-06.md`: *"The trigger for the bootstrap interview
//! exists, and it is written in a file that is never created."* `engine/CLAUDE.md.template:25`
//! tells the orchestrator to run the interview before anything else; that template becomes a
//! real `CLAUDE.md` only through `provision-claude-md.sh`, which nothing invokes. The first
//! outside user was asked nothing because nobody was sitting next to him to type one command.
//!
//! Re-derived on this branch before anything was built on it, `claude` 2.1.263, 2026-09-06:
//!
//! ```text
//! grep -rn "bootstrap-interview" app/ | wc -l      -> 0
//! grep -rn "provision-claude-md\.sh" engine/ --include="*.sh" --include="*.py"
//!                                                  -> its own file and its own test; no caller
//! ```
//!
//! **And the correction.** The onboarding document's fix is M1/M2 — bridge the app's config
//! into `engine/identity.config` and call the provisioner — so that G1 can fill a `CLAUDE.md`.
//! Measured, that would repair a chain whose far end is a file the product never loads: under
//! the production chat-lease vector the child reads no `CLAUDE.md` at all (`company.rs` module
//! doc, cells B0a/B0b/B0c). So the provisioner is **not** wired here, deliberately, and this
//! module uses the channel that was measured to be open instead — the priming turn.
//!
//! # THE TRIGGER IS THE PRIMING TURN, AND THAT IS NOT A CONVENIENCE
//!
//! A skill that nothing invokes is `provision-claude-md.sh` with a different file extension. So
//! the trigger cannot be "the interview is available if Rich thinks to reach for it". It has to
//! be issued.
//!
//! The priming turn is the one place it can be issued from honestly:
//!
//! - it re-fires on every thread change and thread → entity is one-to-one, so it is already
//!   scoped to the company the question is about;
//! - it is derived from a **fact on disk** — whether this company has a `company.md` with
//!   substance in it — rather than from a flag saying a dialog was once shown;
//! - and it **retires itself**. The moment the file has substance, [`priming_block`] stops
//!   emitting the offer and starts emitting the material. Nothing has to remember to turn it off.
//!
//! The doctrine file cannot carry it: it is fixed at spawn, one lease serves many companies, and
//! `doctrine.rs`'s §4.1 boundary rule refuses anything that is not true for every turn of every
//! conversation. "This company has not been interviewed" is true of one entity at a time.
//!
//! # POSITIVE SIGNALS ONLY — no "seen it" flag, ever
//!
//! Onboarding document §3 states the rule and the reason: *"A launch counter, a 'seen it' flag,
//! or a first-run boolean would all record that the app SHOWED something, not that the CEO
//! ANSWERED anything — and the failure mode we are fixing is precisely a product that reported
//! success over work that never happened."*
//!
//! So there are exactly two facts here and both are answers:
//!
//! | fact | where it lives | why it is an answer |
//! |---|---|---|
//! | this company has been described | `<central>/companies/<id>/company.md` has substance | he said those things |
//! | he was asked and said not now | [`OnboardingRecord`] | "not now" is an answer, and the only one the file cannot hold |
//!
//! [`OnboardingRecord`] holds company-scoped explicit declines. There is no counter,
//! timestamp of an offer or "shown" boolean. The version and company id describe whose
//! answer was persisted, not whether a screen was shown.
//!
//! # THE DECLINATION EXISTS BECAUSE OF A FAILURE THIS PRODUCT ALREADY HAD
//!
//! Onboarding document M5: without a durable "not now", the offer either nags forever or
//! vanishes forever, and both are wrong. The memory question learned it the hard way —
//! `main.js:3538-3548` records a dialog that reopened at every launch and became *"a permanent
//! interruption"* on a first run.
//!
//! A declined company still gets a block, and that is deliberate: it tells Rich he was already
//! asked, so Rich neither re-offers nor wonders why he knows nothing. The way back is the CEO
//! asking, which needs no mechanism.
//!
//! # The interview and team handoff
//!
//! Rich uses the interview as an adaptive conversation and preserves the actual answers.
//! A team proposal uses current evidence. Approval is interpreted in context, then handed
//! over under the ordinary latency rule with every requested name, role and constraint.
//! The display title is never the execution contract. Only an observed saved definition
//! establishes that a teammate was added. This module supplies state, not conversation scripts.

use crate::company::CompanyLayer;
use crate::entity::EntityId;
use std::path::{Path, PathBuf};

/// The declination record's filename, in the install's own configuration directory beside
/// `config.json`, `entities.json` and `inner-doctrine.md`.
pub const RECORD_FILENAME: &str = "onboarding.json";

/// The skill the offer names, as it is announced on the wire — `<plugin>:<skill>`.
///
/// A constant rather than a literal in the block below, because `skills.rs` asserts on the same
/// string when it checks the `system/init` frame, and two spellings of one name is a schedule
/// for them to disagree.
pub const INTERVIEW_SKILL: &str = "rich-skills:bootstrap-interview";

/// The record path for a configuration directory.
pub fn record_path(config_dir: &Path) -> PathBuf {
    config_dir.join(RECORD_FILENAME)
}

/// A strictly parsed record of explicit declines. Legacy records are not applied to every
/// company: the shell migrates them once to the restored company before showing onboarding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OnboardingRecord {
    declined_at_millis: Option<u64>,
    declined_by_entity: std::collections::BTreeMap<String, u64>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecord { declined_at_millis: u64 }

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct EntityRecords {
    version: u32,
    declined_by_entity: std::collections::BTreeMap<String, u64>,
}

impl OnboardingRecord {
    /// Missing, malformed, unknown-version and unreadable records never invent a decline.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else { return Self::default(); };
        if let Ok(stored) = serde_json::from_str::<EntityRecords>(&text) {
            if stored.version == 1 && stored.declined_by_entity.keys().all(|id| EntityId::parse(id).is_ok()) {
                return Self { declined_at_millis: None, declined_by_entity: stored.declined_by_entity };
            }
        } else if let Ok(legacy) = serde_json::from_str::<LegacyRecord>(&text) {
            return Self { declined_at_millis: Some(legacy.declined_at_millis), ..Self::default() };
        }
        Self::default()
    }

    pub fn load_for_install() -> Self {
        crate::doctrine::install_config_dir().ok()
            .map(|dir| Self::load(&record_path(&dir))).unwrap_or_default()
    }

    /// Select only this company's explicit answer. An unbound legacy record is never global.
    pub fn for_entity(&self, entity: &EntityId) -> Self {
        Self { declined_at_millis: self.declined_by_entity.get(entity.as_str()).copied(),
               declined_by_entity: self.declined_by_entity.clone() }
    }

    pub fn declined_at_millis(&self) -> Option<u64> { self.declined_at_millis }
    pub fn is_declined(&self) -> bool { self.declined_at_millis.is_some() }

    /// Import the old install-wide answer into the restored active company exactly once.
    /// The old file contains no company identity; this explicit migration preserves that
    /// user's answer without applying it to unrelated companies added later.
    pub fn migrate_legacy(path: &Path, entity: &EntityId) -> Result<bool, crate::doctrine::DoctrineError> {
        let mut record = Self::load(path);
        let Some(at) = record.declined_at_millis.take() else { return Ok(false); };
        record.declined_by_entity.insert(entity.to_string(), at);
        record.write(path)?;
        Ok(true)
    }

    pub fn record_declination_for_entity(path: &Path, entity: &EntityId, now_millis: u64)
        -> Result<(), crate::doctrine::DoctrineError> {
        let mut record = Self::load(path);
        record.declined_by_entity.insert(entity.to_string(), now_millis);
        record.write(path)
    }

    pub fn clear_declination_for_entity(path: &Path, entity: &EntityId) -> Result<(), crate::doctrine::DoctrineError> {
        let mut record = Self::load(path);
        if record.declined_by_entity.remove(entity.as_str()).is_some() { record.write(path)?; }
        Ok(())
    }

    fn write(&self, path: &Path) -> Result<(), crate::doctrine::DoctrineError> {
        let body = serde_json::to_string_pretty(&EntityRecords {
            version: 1, declined_by_entity: self.declined_by_entity.clone(),
        }).expect("string-keyed decline records serialize");
        crate::doctrine::write_verified(path, &format!("{body}\n"))
    }

    /// Legacy import helper retained for older callers and migration fixtures. Product UI
    /// and tools use record_declination_for_entity, which never writes the global format.
    pub fn record_declination(path: &Path, now_millis: u64) -> Result<(), crate::doctrine::DoctrineError> {
        crate::doctrine::write_verified(path, &format!("{{\n  \"declined_at_millis\": {now_millis}\n}}\n"))
    }
}

/// Where this install stands, derived and never stored.
///
/// It is a function of two facts on disk, so it cannot drift from them — there is no third
/// place holding a cached opinion about which of these is true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingState {
    /// No central folder has been configured, so the question cannot even be asked yet. This is
    /// NOT "he has not been interviewed" — nothing has looked.
    NoCentralFolder,
    /// Nothing on file about this company, and no declination. The interview is due.
    NotYet,
    /// He was asked and said not now.
    Declined { at_millis: u64 },
    /// This company has been described. Whether that happened in an interview or because he
    /// wrote the file himself is not a distinction the product needs, and inventing one would
    /// mean trusting a marker over the file.
    Described,
    /// Real answers were saved and the remaining interview stages are explicitly open.
    Partial,
    /// The file is there and cannot be used — over budget, unreadable, or not a file. Kept
    /// apart from every other state because it is the only one that needs somebody to act.
    Unusable { why: String },
}

/// Where this install stands for one company.
pub fn state(layer: Option<&CompanyLayer>, record: &OnboardingRecord) -> OnboardingState {
    let Some(layer) = layer else {
        return OnboardingState::NoCentralFolder;
    };
    match layer {
        CompanyLayer::Present { .. } if crate::company::interview_is_partial(layer) => {
            match record.declined_at_millis() {
                Some(at_millis) => OnboardingState::Declined { at_millis },
                None => OnboardingState::Partial,
            }
        }
        CompanyLayer::Present { .. } => OnboardingState::Described,
        CompanyLayer::TooLarge { .. } | CompanyLayer::Unreadable { .. } => {
            OnboardingState::Unusable { why: layer.describe() }
        }
        CompanyLayer::NoHome { .. } | CompanyLayer::Absent { .. } | CompanyLayer::Empty { .. } => {
            match record.declined_at_millis() {
                Some(at_millis) => OnboardingState::Declined { at_millis },
                None => OnboardingState::NotYet,
            }
        }
    }
}

/// Current onboarding observations. Conversation behavior belongs in Rich's standing
/// role and the interview skill; these facts prescribe no reply or next action.
pub const OFFER_BLOCK: &str = "ONBOARDING — Company notes are not available in this install. \
The interview is optional. Stopping partway is fine; 'not sure yet' is a valid answer. \
Unknown answers remain open. Interview notes do not populate the home screen, which shows a \
worked example.\n\n";

/// The recorded decision survives company switches and restarts.
pub const DECLINED_BLOCK: &str = "ONBOARDING — He said not now to the interview or its \
resumption. That explicit decline is recorded. Declining does not change saved notes.\n\n";

/// Saved answers exist, with interview progress still partial.
pub const RESUME_BLOCK: &str = "ONBOARDING — This company's interview is partly complete. \
The company notes contain the answers already saved.\n\n";

/// A file exists but no usable contents are supplied to the agent.
pub const UNUSABLE_BLOCK: &str = "ONBOARDING — Company notes exist but could not be read. \
Their contents are unavailable in this context.\n\n";

/// The onboarding contribution to a priming turn: the material, the offer, or the honest note.
///
/// `None` in exactly two cases — nothing has looked ([`OnboardingState::NoCentralFolder`]), and
/// the company is described, in which case the block is the material itself. Every other state
/// says something, because the whole subject of this module is a product that said nothing.
pub fn priming_block(
    entity: &EntityId,
    layer: Option<&CompanyLayer>,
    record: &OnboardingRecord,
) -> Option<String> {
    let state = state(layer, record);
    if state == OnboardingState::NoCentralFolder { return None; }
    let layer = layer?;
    let path = match layer {
        CompanyLayer::NoHome { dir } => dir.join(crate::company::COMPANY_FILENAME),
        _ => layer.path().to_path_buf(),
    };
    let destination = serde_json::to_string(&path.to_string_lossy()).expect("path serializes");
    let mut block = format!(
        "COMPANY NOTES DESTINATION: entity={entity}; file={destination}. \
         Bound save tool: mcp__richos_onboarding__save_company_notes. \
         Bound decline tool: mcp__richos_onboarding__decline_onboarding. \
         Saves are atomic and verified by reading the notes back.\n\n"
    );
    if let Some(material) = layer.render_for_priming(entity) { block.push_str(&material); }
    block.push_str(match state {
        OnboardingState::NotYet => OFFER_BLOCK,
        OnboardingState::Declined { .. } => DECLINED_BLOCK,
        OnboardingState::Partial => RESUME_BLOCK,
        OnboardingState::Unusable { .. } => UNUSABLE_BLOCK,
        OnboardingState::Described | OnboardingState::NoCentralFolder => "",
    });
    Some(block)
}

/// The company's name, exactly as he typed it, for the priming turn.
///
/// The CEO, 2026-10-13: a name with a word in capitals was written back re-capitalized,
/// because nothing told Rich the name and the id is all lower case.
pub fn company_name_block(entity: &EntityId, name: &str) -> String {
    let quoted = serde_json::to_string(name).expect("a string serializes");
    format!(
        "\n\nCOMPANY NAME: this company is called {quoted}. Whenever you write or say its name, write \
         it exactly so, letter for letter, with exactly this capitalization. Never re-capitalize \
         it and never rebuild it from the id {entity}.\n\n"
    )
}

/// One line for the boot log, naming what this launch will actually do about onboarding.
///
/// It exists because the failure being fixed was silent. `--plugin-dir`'s silence on a missing
/// directory is the defect `skills.rs` was written around; a company that will never be asked
/// about is the same shape one layer up, and a boot that says nothing about it is a boot that
/// cannot be checked.
pub fn describe(entity: &EntityId, layer: Option<&CompanyLayer>, record: &OnboardingRecord) -> String {
    match state(layer, record) {
        OnboardingState::NoCentralFolder => {
            format!("onboarding {entity}: no central folder configured — nothing has looked, and he will not be asked")
        }
        OnboardingState::NotYet => {
            format!("onboarding {entity}: nothing on file and no declination — Rich will offer the interview")
        }
        OnboardingState::Declined { at_millis } => {
            format!("onboarding {entity}: offered and declined at {at_millis} — Rich will not offer again")
        }
        OnboardingState::Partial => format!("onboarding {entity}: partial — saved answers are in the priming turn; offer to resume"),
        OnboardingState::Described => {
            format!("onboarding {entity}: described — the company layer is in the priming turn")
        }
        OnboardingState::Unusable { why } => {
            format!("onboarding {entity}: UNUSABLE — {why}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::{company_file, company_home, COMPANY_BUDGET_BYTES};

    fn entity() -> EntityId {
        EntityId::parse("harborline").unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "richos-onboarding-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn layer_for(central: &Path) -> CompanyLayer {
        CompanyLayer::read(central, &entity())
    }

    fn write_company(central: &Path, body: &str) {
        std::fs::create_dir_all(company_home(central, &entity())).unwrap();
        std::fs::write(company_file(central, &entity()), body).unwrap();
    }

    // -----------------------------------------------------------------------
    // the state machine
    // -----------------------------------------------------------------------

    #[test]
    fn nothing_configured_is_not_the_same_as_nobody_interviewed() {
        assert_eq!(state(None, &OnboardingRecord::default()), OnboardingState::NoCentralFolder);
        assert_eq!(priming_block(&entity(), None, &OnboardingRecord::default()), None);
    }

    #[test]
    fn a_company_with_nothing_on_file_is_due_an_interview() {
        let central = tmp("notyet");
        assert_eq!(state(Some(&layer_for(&central)), &OnboardingRecord::default()), OnboardingState::NotYet);
        let block = priming_block(&entity(), Some(&layer_for(&central)), &OnboardingRecord::default()).unwrap();
        assert!(block.contains("Company notes are not available in this install"));
    }

    #[test]
    fn an_empty_file_is_still_nothing_on_file() {
        let central = tmp("blank");
        write_company(&central, "\n \n");
        assert_eq!(state(Some(&layer_for(&central)), &OnboardingRecord::default()), OnboardingState::NotYet);
    }

    #[test]
    fn a_described_company_gets_its_material_and_never_the_offer() {
        let central = tmp("described");
        write_company(&central, "We sell rope to harbors.\n");
        assert_eq!(state(Some(&layer_for(&central)), &OnboardingRecord::default()), OnboardingState::Described);
        let block = priming_block(&entity(), Some(&layer_for(&central)), &OnboardingRecord::default()).unwrap();
        assert!(block.contains("We sell rope to harbors."));
        assert!(!block.contains("ONBOARDING —"), "an interviewed company is never re-offered: {block}");
    }

    /// The self-retirement property, stated as a test because it is the thing that means nobody
    /// has to remember to turn the offer off.
    #[test]
    fn the_offer_retires_itself_the_moment_the_file_has_substance() {
        let central = tmp("retire");
        let record = OnboardingRecord::default();
        assert!(priming_block(&entity(), Some(&layer_for(&central)), &record).unwrap().contains("ONBOARDING —"));
        write_company(&central, "We sell rope to harbors.\n");
        let after = priming_block(&entity(), Some(&layer_for(&central)), &record).unwrap();
        assert!(!after.contains("ONBOARDING —"), "{after}");
    }

    // -----------------------------------------------------------------------
    // the declination — M5
    // -----------------------------------------------------------------------

    #[test]
    fn a_declination_replaces_the_offer_and_is_never_a_silence() {
        let central = tmp("declined");
        let path = central.join(RECORD_FILENAME);
        OnboardingRecord::record_declination(&path, 1_757_000_000_000).unwrap();
        let record = OnboardingRecord::load(&path);
        assert_eq!(
            state(Some(&layer_for(&central)), &record),
            OnboardingState::Declined { at_millis: 1_757_000_000_000 }
        );
        let block = priming_block(&entity(), Some(&layer_for(&central)), &record).unwrap();
        assert!(block.contains("said not now"), "{block}");
        assert!(block.contains("explicit decline is recorded"), "{block}");
        assert!(!block.contains("offer, once"), "a declined company is not re-offered: {block}");
    }

    #[test]
    fn an_absent_or_corrupt_record_leans_toward_asking_rather_than_toward_silence() {
        let dir = tmp("corrupt");
        assert!(!OnboardingRecord::load(&dir.join("nothing-here.json")).is_declined());
        let bad = dir.join("bad.json");
        std::fs::write(&bad, "{ this is not json at all").unwrap();
        assert!(!OnboardingRecord::load(&bad).is_declined(), "a corrupt record must not silence the offer");
    }

    /// THE TEST WHOSE ONLY JOB IS TO REFUSE A SECOND FIELD. Onboarding §3: a record of what the
    /// app SHOWED is the defect; a record of what he ANSWERED is not. A "last offered at" or an
    /// "offer count" would pass every other test here.
    #[test]
    fn the_record_holds_only_a_declination() {
        let dir = tmp("onefield");
        let path = dir.join(RECORD_FILENAME);
        OnboardingRecord::record_declination(&path, 42).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.matches('"').count(), 2, "exactly one key: {written}");
        assert!(written.contains("declined_at_millis"), "{written}");
        for forbidden in ["shown", "seen", "offered", "count", "launches", "first_run"] {
            assert!(!written.contains(forbidden), "a flag store, not an answer store: {written}");
        }
    }

    // -----------------------------------------------------------------------
    // the CEO's constraint, pinned in the text that is actually sent
    // -----------------------------------------------------------------------

    /// Runtime state is information, not a scripted conversation or work request.
    #[test]
    fn the_offer_reports_missing_notes_without_dictating_a_reply() {
        assert!(OFFER_BLOCK.contains("Company notes are not available in this install"));
        for instruction in ["Say this sentence", "word for word", "Write it down", "hire them"] {
            assert!(!OFFER_BLOCK.contains(instruction));
        }
    }

    /// The offer must never instruct a spawn. Cells B2/B3 measured that an attempt fails loudly
    /// today, but that is the model's behavior and this is the structure: a run that never asks
    /// for Dean cannot get a wrong answer about Dean.
    #[test]
    fn the_offer_instructs_no_spawn_of_anyone() {
        let lowered = OFFER_BLOCK.to_lowercase();
        for word in ["dean", "subagent", "spawn", "task tool", "teammate"] {
            assert!(!lowered.contains(word), "the offer must not reach for staffing: {word}");
        }
    }

    /// Onboarding §4: deferral has to be made visibly safe, unprompted, because alone nobody
    /// reminds him.
    #[test]
    fn the_offer_makes_stopping_and_not_knowing_visibly_safe() {
        assert!(OFFER_BLOCK.contains("Stopping partway is fine"));
        assert!(OFFER_BLOCK.contains("not sure yet"));
        assert!(OFFER_BLOCK.contains("Unknown answers remain open"));
    }

    /// Onboarding §6: the demo stays, and the interview must not be sold on the promise that
    /// answering questions makes his own data appear.
    #[test]
    fn the_offer_refuses_to_promise_his_data_on_the_home_screen() {
        assert!(OFFER_BLOCK.contains("Interview notes do not populate the home screen"));
        assert!(OFFER_BLOCK.contains("worked example"));
    }

    // -----------------------------------------------------------------------
    // the unusable state
    // -----------------------------------------------------------------------

    #[test]
    fn an_over_budget_file_is_unusable_and_says_so_without_quoting_itself() {
        let central = tmp("unusable");
        write_company(&central, &format!("SECRET-MARKER\n{}", "z".repeat(COMPANY_BUDGET_BYTES)));
        let record = OnboardingRecord::default();
        assert!(matches!(state(Some(&layer_for(&central)), &record), OnboardingState::Unusable { .. }));
        let block = priming_block(&entity(), Some(&layer_for(&central)), &record).unwrap();
        assert!(!block.contains("SECRET-MARKER"), "an unusable file's contents never go in: {block}");
        assert!(block.contains("could not be read"), "{block}");
    }

    /// An unusable file must NOT be treated as an un-interviewed company: offering the interview
    /// again over a file that exists would invite him to answer questions he has already
    /// answered, and could overwrite them.
    #[test]
    fn an_unusable_file_is_never_mistaken_for_an_uninterviewed_company() {
        let central = tmp("notoffer");
        write_company(&central, &"z".repeat(COMPANY_BUDGET_BYTES + 1));
        let block = priming_block(&entity(), Some(&layer_for(&central)), &OnboardingRecord::default()).unwrap();
        assert!(!block.contains("twenty minutes"), "{block}");
    }

    // -----------------------------------------------------------------------
    // THE SHIPPED INTERVIEW — the same contract, checked in the text that is actually sent
    // -----------------------------------------------------------------------
    //
    // The offer in the priming turn and the skill it names are two halves of one promise, and
    // the half that carries the questions is the half that could quietly promise more. These
    // read the compiled-in skill body rather than the file on disk, so they check what SHIPS.

    /// The shipped body, with every run of whitespace collapsed to one space.
    ///
    /// Normalized because the skill is prose that is hard-wrapped, so a sentence these tests
    /// care about spans a line break and a literal `contains` would fail on a re-wrap that
    /// changed nothing. A test that breaks when the paragraph is reflowed teaches people to
    /// stop reflowing paragraphs, and then to stop trusting the test.
    fn interview_skill() -> String {
        let body = crate::skills::SKILLS
            .iter()
            .find(|(n, _)| *n == "bootstrap-interview")
            .map(|(_, body)| *body)
            .expect("the interview the offer names must be a shipped skill");
        body.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The shipped skill preserves the full approved request and the agent's judgment.
    #[test]
    fn the_interview_hands_over_the_complete_approved_request_without_a_title_protocol() {
        let s = interview_skill();
        assert!(s.contains("No particular word is required"));
        assert!(s.contains("Rich decides what to handle and what to pass to Stu"));
        assert!(s.contains("A handoff carries the entire approved request"));
        assert!(s.contains("Do not split a request to fit that title"));
        assert!(s.contains("Acknowledge in your own words"));
        assert!(s.contains("Name someone as hired only after the hiring has reported them saved."));
        assert!(!s.contains("Hire these AI teammates onto my team:"));
        assert!(!s.contains("Say the words the register hands you"));
    }

    /// Team evidence has one source, rather than a second name/role catalog in the skill.
    #[test]
    fn the_interview_uses_the_current_team_evidence_instead_of_a_stock_roster() {
        let s = interview_skill();
        assert!(s.contains("`richos_work.team` lists and shows the shelf and saved definitions"));
        assert!(s.contains("Distinguish existing teammates from proposed additions"));
        assert!(!s.contains("At the beginning, I already had these teammates"));
    }

    /// The engine's interview ends by spawning Dean. This one must not: the front desk spawns
    /// nobody, and his go becomes an assignment. Dean is named now (he is one of the five on
    /// every team, in the CEO's own sentence), so the check is on the spawning words.
    #[test]
    fn the_interview_leaves_specialist_dispatch_to_stu() {
        let lowered = interview_skill().to_lowercase();
        for word in ["richos_work.prepare", "subagent_type", "isolation"] {
            assert!(!lowered.contains(word), "the interview must not reach for staffing: {word}");
        }
    }

    /// The destination is the one file the app actually reads back. Writing a `CLAUDE.md` here
    /// would produce exactly the failure `company.rs`'s cells B0a/B0b/B0c measured: a file the
    /// child never loads, filled in a conversation that told him it would be used.
    #[test]
    fn the_interview_names_one_destination_and_forbids_the_ones_that_are_never_read() {
        let s = interview_skill();
        assert!(s.contains("companies/<company>/company.md"));
        assert!(s.contains("You do not edit a `CLAUDE.md`"));
        assert!(s.contains("you do not run any setup or verification script"));
    }

    /// The budget has to be stated where the writer is, not only enforced where the reader is.
    /// A skill that cheerfully writes twelve thousand characters produces a file the app then
    /// refuses, and the CEO would have answered questions for nothing.
    #[test]
    fn the_interview_tells_the_writer_about_the_budget_the_reader_enforces() {
        let s = interview_skill();
        assert!(s.contains("under eight thousand UTF-8 bytes"));
        assert!(
            COMPANY_BUDGET_BYTES == 8192,
            "the skill says eight thousand UTF-8 bytes because the reader's budget is 8192 bytes; \
             change one and this test makes you change the other"
        );
    }

    /// Deferral, again, in the half that does the asking.
    #[test]
    fn the_interview_records_what_was_not_answered_rather_than_leaving_it_blank() {
        let s = interview_skill();
        assert!(s.contains("gets a line saying so"));
        assert!(s.contains("Deferral is honest and silence is not."));
        assert!(s.contains("What is still open."));
    }

    // -----------------------------------------------------------------------
    // the boot line
    // -----------------------------------------------------------------------

    #[test]
    fn every_state_says_something_at_boot() {
        let central = tmp("boot");
        let blank = OnboardingRecord::default();
        assert!(describe(&entity(), None, &blank).contains("nothing has looked"));
        assert!(describe(&entity(), Some(&layer_for(&central)), &blank).contains("will offer the interview"));
        write_company(&central, "We sell rope.\n");
        assert!(describe(&entity(), Some(&layer_for(&central)), &blank).contains("described"));
    }
}
